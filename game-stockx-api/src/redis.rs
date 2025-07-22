use bb8::Pool;
use bb8_redis::RedisConnectionManager;
use redis::AsyncCommands;

pub type RedisPool = Pool<RedisConnectionManager>;

pub async fn create_redis_pool(redis_url: &str) -> Result<RedisPool, Box<dyn std::error::Error>> {
    let manager = RedisConnectionManager::new(redis_url)?;
    let pool = Pool::builder().build(manager).await?;
    Ok(pool)
}