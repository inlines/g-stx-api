use bb8::Pool;
use bb8_redis::RedisConnectionManager;
use bb8_redis::redis::{RedisError, AsyncCommands};
use serde::{Serialize, de::DeserializeOwned};
use std::fmt;
use async_trait::async_trait;

#[derive(Debug)]
pub enum CacheError {
    Connection(bb8::RunError<RedisError>),
    Redis(RedisError),
    Serialization(serde_json::Error),
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connection(e) => write!(f, "Redis connection error: {}", e),
            Self::Redis(e) => write!(f, "Redis operation error: {}", e),
            Self::Serialization(e) => write!(f, "Serialization error: {}", e),
        }
    }
}

impl std::error::Error for CacheError {}

impl From<bb8::RunError<RedisError>> for CacheError {
    fn from(err: bb8::RunError<RedisError>) -> Self {
        Self::Connection(err)
    }
}

impl From<RedisError> for CacheError {
    fn from(err: RedisError) -> Self {
        Self::Redis(err)
    }
}

impl From<serde_json::Error> for CacheError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err)
    }
}

pub type RedisPool = Pool<RedisConnectionManager>;

pub async fn create_redis_pool(redis_url: &str) -> Result<RedisPool, CacheError> {
    let manager = RedisConnectionManager::new(redis_url)?;
    let pool = Pool::builder().build(manager).await?;
    Ok(pool)
}

#[async_trait]
pub trait RedisCacheExt: Send + Sync {
    async fn get_json<T>(&self, key: &str) -> Result<Option<T>, CacheError>
    where
        T: DeserializeOwned + Send + 'static;
    
    async fn set_json<T>(&self, key: &str, value: &T, ttl: usize) -> Result<(), CacheError>
    where
        T: Serialize + Send + Sync + 'static;
}

#[async_trait]
impl RedisCacheExt for RedisPool {
    async fn get_json<T>(&self, key: &str) -> Result<Option<T>, CacheError>
    where
        T: DeserializeOwned + Send + 'static,
    {
        let mut conn = self.get().await?;
        let data: Option<String> = conn.get(key).await?;
        data.map(|json| serde_json::from_str(&json).map_err(Into::into))
            .transpose()
    }

    async fn set_json<T>(&self, key: &str, value: &T, ttl: usize) -> Result<(), CacheError>
    where
        T: Serialize + Send + Sync + 'static,
    {
        let mut conn = self.get().await?;
        let json = serde_json::to_string(value)?;
        conn.set_ex(key, json, ttl as u64).await.map_err(Into::into)
    }
}