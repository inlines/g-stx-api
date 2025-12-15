use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    Error, http::StatusCode,
};
use futures_util::future::LocalBoxFuture;
use std::future::{ready, Ready};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::Instant;
use crate::metrics::{HTTP_REQUESTS_TOTAL, HTTP_REQUESTS_DURATION};

pub struct MetricsMiddleware;

impl<S, B> Transform<S, ServiceRequest> for MetricsMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = MetricsMiddlewareService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(MetricsMiddlewareService {
            service: Rc::new(service),
        }))
    }
}

pub struct MetricsMiddlewareService<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for MetricsMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let start = Instant::now();
        
        // Получаем информацию о запросе
        let method = req.method().to_string();
        let endpoint = req.path().to_string();
        
        let service = self.service.clone();
        Box::pin(async move {
            let res = service.call(req).await;
            
            let duration = start.elapsed().as_secs_f64();
            let status = match &res {
                Ok(resp) => resp.response().status().as_u16().to_string(),
                Err(_) => "500".to_string(), // или другой код ошибки
            };
            
            // Инкрементируем счетчик с метками
            HTTP_REQUESTS_TOTAL
                .with_label_values(&[&method, &endpoint, &status])
                .inc();
            
            // Записываем длительность с метками
            HTTP_REQUESTS_DURATION
                .with_label_values(&[&method, &endpoint])
                .observe(duration);
            
            res
        })
    }
}