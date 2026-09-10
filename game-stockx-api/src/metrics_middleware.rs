use crate::metrics::{HTTP_REQUESTS_DURATION, HTTP_REQUESTS_TOTAL};
use actix_web::{
    Error,
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
};
use std::future::{Ready, ready};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::Instant;

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

        let method = match req.method().as_str() {
            "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" | "CONNECT"
            | "TRACE" => req.method().to_string(),
            _ => "OTHER".to_owned(),
        };
        // ResourceMap resolves templates without cloning the request: Actix routing
        // needs exclusive access to it while capturing route parameters.
        let endpoint = req
            .match_pattern()
            .unwrap_or_else(|| "unmatched".to_owned());

        let service = self.service.clone();
        Box::pin(async move {
            let res = service.call(req).await;

            let duration = start.elapsed().as_secs_f64();
            let status = match &res {
                Ok(resp) => resp.response().status().as_u16().to_string(),
                Err(error) => error.as_response_error().status_code().as_u16().to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, HttpResponse, error, test, web};

    #[actix_web::test]
    async fn groups_ids_and_unknown_paths_and_preserves_error_status() {
        let app = test::init_service(
            App::new().wrap(MetricsMiddleware).service(
                web::scope("/metrics-test-api")
                    .route(
                        "/products/{id}",
                        web::get().to(|| async { HttpResponse::Ok().finish() }),
                    )
                    .route(
                        "/denied",
                        web::get().to(|| async {
                            Err::<HttpResponse, _>(error::ErrorForbidden("denied"))
                        }),
                    ),
            ),
        )
        .await;
        let counter = HTTP_REQUESTS_TOTAL.with_label_values(&[
            "GET",
            "/metrics-test-api/products/{id}",
            "200",
        ]);
        let before = counter.get();
        for id in ["123", "456"] {
            let response = test::call_service(
                &app,
                test::TestRequest::get()
                    .uri(&format!("/metrics-test-api/products/{id}"))
                    .to_request(),
            )
            .await;
            assert_eq!(response.status(), 200);
        }
        assert_eq!(counter.get() - before, 2.0);
        let unknown = HTTP_REQUESTS_TOTAL.with_label_values(&["GET", "unmatched", "404"]);
        let before = unknown.get();
        for path in ["/missing-one", "/missing-two"] {
            assert_eq!(
                test::call_service(&app, test::TestRequest::get().uri(path).to_request())
                    .await
                    .status(),
                404
            );
        }
        assert_eq!(unknown.get() - before, 2.0);
        let forbidden =
            HTTP_REQUESTS_TOTAL.with_label_values(&["GET", "/metrics-test-api/denied", "403"]);
        let before = forbidden.get();
        assert_eq!(
            test::call_service(
                &app,
                test::TestRequest::get()
                    .uri("/metrics-test-api/denied")
                    .to_request()
            )
            .await
            .status(),
            403
        );
        assert_eq!(forbidden.get() - before, 1.0);
    }

    #[actix_web::test]
    async fn service_errors_are_not_all_counted_as_500() {
        let inner = actix_web::dev::fn_service(|_req: ServiceRequest| async {
            Err::<ServiceResponse, _>(actix_web::error::ErrorTooManyRequests("limited"))
        });
        let service = MetricsMiddleware.new_transform(inner).await.unwrap();
        let counter = HTTP_REQUESTS_TOTAL.with_label_values(&["POST", "unmatched", "429"]);
        let before = counter.get();
        assert!(
            service
                .call(test::TestRequest::post().to_srv_request())
                .await
                .is_err()
        );
        assert_eq!(counter.get() - before, 1.0);
    }
}
