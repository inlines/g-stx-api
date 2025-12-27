use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::Error;
use futures_util::future::{LocalBoxFuture, Ready};

pub struct SimpleRateLimiter {
    burst_size: u32,
}

impl SimpleRateLimiter {
    pub fn new(burst_size: u32) -> Self {
        Self { burst_size }
    }
}

impl<S, B> Transform<S, ServiceRequest> for SimpleRateLimiter
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;
    type Transform = RateLimiterMiddleware<S>;

    fn new_transform(&self, service: S) -> Self::Future {
        futures_util::future::ok(RateLimiterMiddleware {
            service,
            burst_size: self.burst_size,
            requests: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

pub struct RateLimiterMiddleware<S> {
    service: S,
    burst_size: u32,
    requests: Arc<Mutex<HashMap<String, (Instant, u32)>>>,
}

impl<S, B> Service<ServiceRequest> for RateLimiterMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    actix_web::dev::forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // Пропускаем WebSocket и метрики
        let path = req.path();
        if path.starts_with("/ws/") || path.starts_with("/metrics") {
            let fut = self.service.call(req);
            return Box::pin(async move {
                fut.await
            });
        }
        
        // Получаем IP адрес
        let ip = req.connection_info().peer_addr()
            .unwrap_or("unknown")
            .to_string();
        
        // Проверяем rate limit
        let mut requests = self.requests.lock().unwrap();
        let now = Instant::now();
        
        // Очистка старых записей (старше 1 секунды)
        requests.retain(|_, (time, _)| now.duration_since(*time) < Duration::from_secs(1));
        
        let entry = requests.entry(ip.to_string()).or_insert((now, 0));
        
        if entry.1 >= self.burst_size {
            drop(requests); // Освобождаем мьютекс перед асинхронной операцией
            return Box::pin(async move {
                Err(actix_web::error::ErrorTooManyRequests("Rate limit exceeded. Please try again in a moment."))
            });
        }
        
        entry.1 += 1;
        drop(requests); // Освобождаем мьютекс
        
        let fut = self.service.call(req);
        Box::pin(async move {
            fut.await
        })
    }
}