use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpResponse,
};
use futures_util::future::{LocalBoxFuture, Ready};
use governor::{
    clock::{Clock, DefaultClock, QuantaClock},
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use std::{
    future::Future,
    num::NonZeroU32,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll},
};
use std::collections::HashMap;
use std::sync::Mutex;

// Для ключевого rate limiting (по IP)
use governor::state::keyed::DashMapStateStore;
use governor::RateLimiter as KeyedRateLimiter;

pub struct GovernorRateLimiter {
    // Глобальный rate limiter (на всё приложение)
    global_limiter: Option<Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>>>,
    // Rate limiter по ключам (IP)
    keyed_limiter: Option<Arc<KeyedRateLimiter<String, DashMapStateStore<String>, DefaultClock, NoOpMiddleware>>>,
    // Whitelist для путей
    whitelist_paths: Vec<String>,
}

impl GovernorRateLimiter {
    pub fn new(
        global_quota: Option<NonZeroU32>,  // Общий лимит в секунду
        per_ip_quota: Option<NonZeroU32>,   // Лимит на IP в секунду
        whitelist_paths: Vec<&str>,
    ) -> Self {
        let global_limiter = global_quota.map(|quota| {
            Arc::new(RateLimiter::direct(Quota::per_second(quota)))
        });

        let keyed_limiter = per_ip_quota.map(|quota| {
            Arc::new(KeyedRateLimiter::dashmap(Quota::per_second(quota)))
        });

        Self {
            global_limiter,
            keyed_limiter,
            whitelist_paths: whitelist_paths.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn setup() -> Self {
        // Читаем конфигурацию из переменных окружения
        use std::env;
        use std::num::NonZeroU32;
        
        let global_limit = env::var("RATE_LIMIT_GLOBAL")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .and_then(NonZeroU32::new);
        
        let per_ip_limit = env::var("RATE_LIMIT_PER_IP")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .and_then(NonZeroU32::new);
        
        let whitelist_str = env::var("RATE_LIMIT_WHITELIST")
            .unwrap_or_else(|_| "/ws/,/metrics,/health,/favicon.ico".to_string());
        
        let whitelist_paths: Vec<&str> = whitelist_str
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        
        Self::new(global_limit, per_ip_limit, whitelist_paths)
    }
    // Билдер для удобства
    pub fn per_ip(requests_per_second: u32) -> Self {
        Self::new(
            None,
            NonZeroU32::new(requests_per_second),
            vec!["/ws/", "/metrics", "/health"],
        )
    }

    pub fn global(requests_per_second: u32) -> Self {
        Self::new(
            NonZeroU32::new(requests_per_second),
            None,
            vec!["/ws/", "/metrics", "/health"],
        )
    }

    pub fn both(global: u32, per_ip: u32) -> Self {
        Self::new(
            NonZeroU32::new(global),
            NonZeroU32::new(per_ip),
            vec!["/ws/", "/metrics", "/health"],
        )
    }
}


impl Clone for GovernorRateLimiter {
    fn clone(&self) -> Self {
        Self {
            global_limiter: self.global_limiter.clone(),
            keyed_limiter: self.keyed_limiter.clone(),
            whitelist_paths: self.whitelist_paths.clone(),
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for GovernorRateLimiter
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = GovernorMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        futures_util::future::ok(GovernorMiddleware {
            service: Rc::new(service),
            global_limiter: self.global_limiter.clone(),
            keyed_limiter: self.keyed_limiter.clone(),
            whitelist_paths: self.whitelist_paths.clone(),
        })
    }
}

pub struct GovernorMiddleware<S> {
    service: Rc<S>,
    global_limiter: Option<Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>>>,
    keyed_limiter: Option<Arc<KeyedRateLimiter<String, DashMapStateStore<String>, DefaultClock, NoOpMiddleware>>>,
    whitelist_paths: Vec<String>,
}

impl<S, B> Service<ServiceRequest> for GovernorMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let path = req.path();
        
        // Проверяем whitelist
        let should_limit = !self.whitelist_paths.iter().any(|whitelist_path| {
            path.starts_with(whitelist_path)
        });

        if !should_limit {
            let service = Rc::clone(&self.service);
            return Box::pin(async move {
                service.call(req).await
            });
        }

        let global_limiter = self.global_limiter.clone();
        let keyed_limiter = self.keyed_limiter.clone();
        
        // Получаем IP (с обработкой прокси)
        let ip = extract_client_ip(&req);
        
        let service = Rc::clone(&self.service);
        
        Box::pin(async move {
            // 1. Проверка глобального лимита
            if let Some(limiter) = global_limiter {
                if let Err(not_until) = limiter.check() {
                    let wait_time = not_until.wait_time_from(DefaultClock::default().now());
                    
                    // Можно добалять заголовок с временем ожидания
                    return Err(actix_web::error::ErrorTooManyRequests(format!(
                        "Global rate limit exceeded. Try again in {:?}",
                        wait_time
                    )));
                }
            }
            
            // 2. Проверка лимита по IP
            if let Some(limiter) = keyed_limiter {
                if let Err(not_until) = limiter.check_key(&ip) {
                    let wait_time = not_until.wait_time_from(DefaultClock::default().now());
                    
                    // Логируем превышение лимита для конкретного IP
                    log::warn!("Rate limit exceeded for IP: {}. Wait time: {:?}", ip, wait_time);
                    
                    // Возвращаем стандартную ошибку (без деталей о времени для безопасности)
                    return Err(actix_web::error::ErrorTooManyRequests(
                        "Rate limit exceeded. Please try again later."
                    ));
                }
            }
            
            // Если все проверки пройдены, пропускаем запрос
            service.call(req).await
        })
    }
}

// Функция для извлечения реального IP клиента (с учетом прокси)
fn extract_client_ip(req: &ServiceRequest) -> String {
    // Попробуем получить IP из заголовка X-Forwarded-For (если за прокси)
    if let Some(forwarded_for) = req.headers().get("X-Forwarded-For") {
        if let Ok(ip_str) = forwarded_for.to_str() {
            // Берем первый IP из списка (оригинальный клиент)
            if let Some(first_ip) = ip_str.split(',').next() {
                return first_ip.trim().to_string();
            }
        }
    }
    
    // Или из X-Real-IP
    if let Some(real_ip) = req.headers().get("X-Real-IP") {
        if let Ok(ip_str) = real_ip.to_str() {
            return ip_str.to_string();
        }
    }
    
    // Или из информации о соединении
    req.connection_info()
        .peer_addr()
        .map(|s| s.split(':').next().unwrap_or(s).to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

// Альтернативная реализация с использованием actix-web-middleware-rate-limiter
// Это готовая интеграция, если не хочешь писать свою

#[cfg(feature = "actix-rate-limiter")]
pub fn create_governor_middleware() -> actix_governor::GovernorMiddleware {
    use actix_governor::{Governor, GovernorConfigBuilder};
    
    let config = GovernorConfigBuilder::default()
        .per_second(10)  // 10 запросов в секунду
        .burst_size(15)  // Разрешаем кратковременные всплески до 15
        .finish()
        .unwrap();
    
    Governor::new(&config)
}

// Пример использования
pub fn setup_rate_limiter() -> GovernorRateLimiter {
    // 1000 запросов в секунду глобально, 10 запросов в секунду на IP
    GovernorRateLimiter::both(1000, 10)
        // Можно добавить кастомные пути в whitelist
        // .with_whitelist(vec!["/api/docs", "/static/"])
}