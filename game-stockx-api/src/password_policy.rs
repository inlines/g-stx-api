pub fn valid_password(value: &str) -> bool {
    (8..=128).contains(&value.chars().count())
}

/// Bound memory/CPU spent on Argon2 across all HTTP workers, including registration.
pub fn hash_slot() -> Option<tokio::sync::OwnedSemaphorePermit> {
    use std::sync::{Arc, OnceLock};
    static SLOTS: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    SLOTS
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(4)))
        .clone()
        .try_acquire_owned()
        .ok()
}
pub fn valid_login(value: &str) -> bool {
    (1..=64).contains(&value.len()) && value.bytes().all(|c| c.is_ascii_alphanumeric())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[actix_web::test]
    async fn registration_boundaries() {
        for s in ["", "bad login", "логин", &"a".repeat(65)] {
            assert!(!valid_login(s));
        }
        for s in ["A", "segasanshiro", &"a".repeat(64)] {
            assert!(valid_login(s));
        }
        for s in ["", "1234567", &"a".repeat(129)] {
            assert!(!valid_password(s));
        }
        assert!(valid_password("пароль123"));
        assert!(valid_password(&"a".repeat(128)));
    }
}
