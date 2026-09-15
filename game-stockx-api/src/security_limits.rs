//! Bounded token buckets shared by workers and WebSocket connections.
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

const CAPACITY: usize = 20_000;
struct Bucket {
    tokens: f64,
    updated: Instant,
}
pub struct Limiter {
    buckets: HashMap<String, Bucket>,
    cleaned: Instant,
}
impl Limiter {
    pub fn new() -> Self {
        Self {
            buckets: HashMap::new(),
            cleaned: Instant::now(),
        }
    }
    fn check(&mut self, key: String, burst: u32, rate: f64, now: Instant) -> bool {
        if now.duration_since(self.cleaned) >= Duration::from_secs(60) {
            self.buckets
                .retain(|_, b| now.duration_since(b.updated) < Duration::from_secs(600));
            self.cleaned = now;
        }
        // Do not evict active keys: rotating identifiers must not reset quotas.
        if !self.buckets.contains_key(&key) && self.buckets.len() >= CAPACITY {
            return false;
        }
        let b = self.buckets.entry(key).or_insert(Bucket {
            tokens: burst as f64,
            updated: now,
        });
        b.tokens =
            (b.tokens + now.duration_since(b.updated).as_secs_f64() * rate).min(burst as f64);
        b.updated = now;
        if b.tokens < 1.0 {
            return false;
        }
        b.tokens -= 1.0;
        true
    }
}
pub fn allow(key: String, burst: u32, rate: f64) -> bool {
    static LIMITER: OnceLock<Mutex<Limiter>> = OnceLock::new();
    LIMITER
        .get_or_init(|| Mutex::new(Limiter::new()))
        .lock()
        .is_ok_and(|mut limiter| limiter.check(key, burst, rate, Instant::now()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[actix_web::test]
    async fn burst_refills_without_resetting_on_retry() {
        let mut l = Limiter::new();
        let now = Instant::now();
        for _ in 0..3 {
            assert!(l.check("user".into(), 3, 1.0, now));
        }
        assert!(!l.check("user".into(), 3, 1.0, now));
        assert!(!l.check("user".into(), 3, 1.0, now + Duration::from_millis(500)));
        assert!(l.check("user".into(), 3, 1.0, now + Duration::from_secs(1)));
        assert!(l.check("other".into(), 3, 1.0, now));
    }
    #[actix_web::test]
    async fn capacity_is_bounded_and_idle_keys_expire() {
        let mut l = Limiter::new();
        let now = Instant::now();
        for i in 0..CAPACITY {
            assert!(l.check(i.to_string(), 1, 1.0, now));
        }
        assert!(!l.check("extra".into(), 1, 1.0, now));
        assert!(l.check("extra".into(), 1, 1.0, now + Duration::from_secs(601)));
        assert_eq!(l.buckets.len(), 1);
    }
}
