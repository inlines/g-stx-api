use async_trait::async_trait;
use bb8::{ManageConnection, Pool};
use bb8_redis::redis::{self, AsyncCommands, RedisError, aio::MultiplexedConnection};
use prometheus::{CounterVec, HistogramVec};
use serde::{Serialize, de::DeserializeOwned};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_millis(250);

// MultiplexedConnection preserves response ordering when a timed-out future is dropped.
pub struct RedisManager(redis::Client);
#[async_trait]
impl ManageConnection for RedisManager {
    type Connection = MultiplexedConnection;
    type Error = RedisError;
    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        self.0.get_multiplexed_async_connection().await
    }
    async fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        redis::cmd("PING")
            .query_async::<_, String>(conn)
            .await
            .map(|_| ())
    }
    fn has_broken(&self, _: &mut Self::Connection) -> bool {
        false
    }
}
pub type RedisPool = Pool<RedisManager>;
pub async fn create_redis_pool(url: &str) -> Result<RedisPool, RedisError> {
    let manager = RedisManager(redis::Client::open(url)?);
    Ok(Pool::builder()
        .connection_timeout(TIMEOUT)
        .build_unchecked(manager))
}

#[derive(Clone, Copy)]
pub enum Cache {
    Catalog,
    Basic,
    Companies,
    Franchises,
    Platforms,
}
impl Cache {
    fn label(self) -> &'static str {
        match self {
            Self::Catalog => "catalog",
            Self::Basic => "product_basic",
            Self::Companies => "product_companies",
            Self::Franchises => "product_franchises",
            Self::Platforms => "platforms",
        }
    }
}
lazy_static::lazy_static! {
    static ref READS: CounterVec = register_counter_vec!("app_cache_reads_total", "Cache reads, including failures before GET", &["cache", "result"]).unwrap();
    static ref WRITES: CounterVec = register_counter_vec!("app_cache_writes_total", "Cache writes by result", &["cache", "result"]).unwrap();
    static ref ERRORS: CounterVec = register_counter_vec!("app_cache_errors_total", "Cache failures by bounded operation and reason", &["cache", "operation", "reason"]).unwrap();
    static ref DURATION: HistogramVec = register_histogram_vec!("app_cache_operation_duration_seconds", "Cache latency including pool acquisition", &["cache", "operation"], vec![0.001,0.005,0.01,0.025,0.05,0.1,0.25,0.5]).unwrap();
}
pub fn initialize_metrics() {
    for cache in [
        Cache::Catalog,
        Cache::Basic,
        Cache::Companies,
        Cache::Franchises,
        Cache::Platforms,
    ] {
        for result in ["hit", "miss", "error"] {
            let _ = READS.with_label_values(&[cache.label(), result]);
        }
        for result in ["success", "error"] {
            let _ = WRITES.with_label_values(&[cache.label(), result]);
        }
        for operation in ["read", "write"] {
            let _ = DURATION.with_label_values(&[cache.label(), operation]);
            for reason in ["connection", "command", "serialization", "timeout"] {
                let _ = ERRORS.with_label_values(&[cache.label(), operation, reason]);
            }
        }
    }
}
fn error(cache: Cache, operation: &str, reason: &str) {
    ERRORS
        .with_label_values(&[cache.label(), operation, reason])
        .inc();
    // No keys, search text, credentials or user identifiers in metrics / log messages.
    log::warn!("Cache {} {} failed: {}", cache.label(), operation, reason);
}
pub async fn read<T: DeserializeOwned>(pool: &RedisPool, cache: Cache, key: &str) -> Option<T> {
    let start = Instant::now();
    let result = tokio::time::timeout(TIMEOUT, async {
        let mut conn = pool.get().await.map_err(|_| "connection")?;
        let raw: Option<String> = conn.get(key).await.map_err(|_| "command")?;
        raw.map(|text| serde_json::from_str(&text).map_err(|_| "serialization"))
            .transpose()
    })
    .await
    .unwrap_or(Err("timeout"));
    DURATION
        .with_label_values(&[cache.label(), "read"])
        .observe(start.elapsed().as_secs_f64());
    match result {
        Ok(Some(value)) => {
            READS.with_label_values(&[cache.label(), "hit"]).inc();
            Some(value)
        }
        Ok(None) => {
            READS.with_label_values(&[cache.label(), "miss"]).inc();
            None
        }
        Err(reason) => {
            READS.with_label_values(&[cache.label(), "error"]).inc();
            error(cache, "read", reason);
            None
        }
    }
}
pub async fn write<T: Serialize>(pool: &RedisPool, cache: Cache, key: &str, value: &T, ttl: usize) {
    let start = Instant::now();
    let result = tokio::time::timeout(TIMEOUT, async {
        let json = serde_json::to_string(value).map_err(|_| "serialization")?;
        let mut conn = pool.get().await.map_err(|_| "connection")?;
        conn.set_ex::<_, _, ()>(key, json, ttl as u64)
            .await
            .map_err(|_| "command")
    })
    .await
    .unwrap_or(Err("timeout"));
    DURATION
        .with_label_values(&[cache.label(), "write"])
        .observe(start.elapsed().as_secs_f64());
    WRITES
        .with_label_values(&[
            cache.label(),
            if result.is_ok() { "success" } else { "error" },
        ])
        .inc();
    if let Err(reason) = result {
        error(cache, "write", reason);
    }
}

#[derive(diesel::QueryableByName, Clone, Copy)]
pub(crate) struct Versions {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub catalog: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub names: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub product: i64,
}
/// Read before fetching data: in-flight old readers can only populate old keys.
pub(crate) async fn versions(
    pool: actix_web::web::Data<crate::DBPool>,
    product_id: i32,
) -> Result<Versions, crate::admin::AdminError> {
    use diesel::RunQueryDsl;
    crate::admin::db(pool, move |conn| {
        Ok(diesel::sql_query("SELECT c.revision AS catalog, n.revision AS names, COALESCE((SELECT cache_revision FROM products WHERE id=$1),0) AS product FROM catalog_cache_revision c CROSS JOIN catalog_name_revision n WHERE c.id=1 AND n.id=1")
            .bind::<diesel::sql_types::Integer,_>(product_id).get_result(conn)?)
    }).await
}
