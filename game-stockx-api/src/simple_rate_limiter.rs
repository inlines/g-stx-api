use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    Error,
};
use futures_util::future::{LocalBoxFuture, Ready};
use governor::{
    clock::{Clock, DefaultClock},
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    state::keyed::DashMapStateStore,
    Quota, RateLimiter,
};
use std::{
    future::Future,
    num::NonZeroU32,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

pub struct GovernorRateLimiter {
    global_limiter: Option<Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>>>,
    keyed_limiter: Option<Arc<RateLimiter<String, DashMapStateStore<String>, DefaultClock, NoOpMiddleware>>>,
    whitelist_paths: Vec<String>,
}

impl GovernorRateLimiter {
    pub fn new(
        global_quota: Option<NonZeroU32>,
        per_ip_quota: Option<NonZeroU32>,
        whitelist_paths: Vec<&str>,
    ) -> Self {
        let global_limiter = global_quota.map(|quota| {
            // Строгий лимит без burst
            let quota = Quota::per_second(quota);
            Arc::new(RateLimiter::direct(quota))
        });

        let keyed_limiter = per_ip_quota.map(|quota| {
            // Строгий лимит без burst
            let quota = Quota::per_second(quota);
            Arc::new(RateLimiter::dashmap(quota))
        });

        Self {
            global_limiter,
            keyed_limiter,
            whitelist_paths: whitelist_paths.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn per_ip_with_whitelist(
        requests_per_second: u32,
        whitelist_paths: Vec<&str>,
    ) -> Self {
        Self::new(
            None,
            NonZeroU32::new(requests_per_second),
            whitelist_paths,
        )
    }
    
    // Строгий лимит (без burst)
    pub fn per_ip_strict(requests_per_second: u32) -> Self {
        Self::new(
            None,
            NonZeroU32::new(requests_per_second),
            vec!["/ws/", "/metrics", "/health", "/favicon.ico"],
        )
    }
    
    // С burst
    pub fn per_ip_with_burst(requests_per_second: u32, burst_size: u32) -> Self {
        let per_ip_quota = NonZeroU32::new(requests_per_second);
        let burst = NonZeroU32::new(burst_size).unwrap_or_else(|| per_ip_quota.unwrap());
        
        let keyed_limiter = per_ip_quota.map(|quota| {
            let quota = Quota::per_second(quota).allow_burst(burst);
            Arc::new(RateLimiter::dashmap(quota))
        });

        Self {
            global_limiter: None,
            keyed_limiter,
            whitelist_paths: vec!["/ws/", "/metrics", "/health", "/favicon.ico"]
                .iter().map(|s| s.to_string()).collect(),
        }
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
    keyed_limiter: Option<Arc<RateLimiter<String, DashMapStateStore<String>, DefaultClock, NoOpMiddleware>>>,
    whitelist_paths: Vec<String>,
}

fn extract_client_ip(req: &ServiceRequest) -> String {
    if let Some(forwarded_for) = req.headers().get("X-Forwarded-For") {
        if let Ok(ip_str) = forwarded_for.to_str() {
            if let Some(first_ip) = ip_str.split(',').next() {
                return first_ip.trim().to_string();
            }
        }
    }
    
    if let Some(real_ip) = req.headers().get("X-Real-IP") {
        if let Ok(ip_str) = real_ip.to_str() {
            return ip_str.to_string();
        }
    }
    
    req.connection_info()
        .peer_addr()
        .map(|s| s.split(':').next().unwrap_or(s).to_string())
        .unwrap_or_else(|| "unknown".to_string())
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
        
        // ДОБАВЬТЕ ОТЛАДОЧНЫЙ ВЫВОД
        log::debug!("Rate limit check for path: {}", path);
        
        // Проверяем, не в whitelist ли путь
        let should_limit = !self.whitelist_paths.iter().any(|whitelist_path| {
            path.starts_with(whitelist_path)
        });

        if !should_limit {
            log::debug!("Path {} is whitelisted, skipping rate limit", path);
            let service = Rc::clone(&self.service);
            return Box::pin(async move {
                service.call(req).await
            });
        }

        // Клонируем все необходимые данные для async блока
        let global_limiter = self.global_limiter.clone();
        let keyed_limiter = self.keyed_limiter.clone();
        let ip = extract_client_ip(&req);
        let service = Rc::clone(&self.service);
        
        Box::pin(async move {
            log::debug!("Checking rate limit for IP: {}", ip);
            
            // 1. Проверка глобального лимита - НЕМЕДЛЕННЫЙ возврат 429 при превышении
            if let Some(limiter) = global_limiter {
                if let Err(not_until) = limiter.check() {
                    // Сразу возвращаем 429 без ожидания
                    let wait_time = not_until.wait_time_from(DefaultClock::default().now());
                    let wait_seconds = wait_time.as_secs().max(1); // Минимум 1 секунда
                    
                    log::warn!("Global rate limit exceeded. Required wait: {:?}", wait_time);
                    
                    return Err(actix_web::error::ErrorTooManyRequests(format!(
                        "Global rate limit exceeded. Please try again in {} seconds.",
                        wait_seconds
                    )));
                }
            }
            
            // 2. Проверка лимита по IP - НЕМЕДЛЕННЫЙ возврат 429 при превышении
            if let Some(limiter) = keyed_limiter {
                if let Err(not_until) = limiter.check_key(&ip) {
                    // Сразу возвращаем 429 без ожидания
                    let wait_time = not_until.wait_time_from(DefaultClock::default().now());
                    let wait_seconds = wait_time.as_secs().max(1); // Минимум 1 секунда
                    
                    log::warn!("Rate limit exceeded for IP: {}. Required wait: {:?}", ip, wait_time);
                    
                    return Err(actix_web::error::ErrorTooManyRequests(format!(
                        "Rate limit exceeded. Please try again in {} seconds.",
                        wait_seconds
                    )));
                }
            }
            
            log::debug!("Rate limit check passed for IP: {}", ip);
            
            // Все проверки пройдены - пропускаем запрос дальше
            service.call(req).await
        })
    }
}