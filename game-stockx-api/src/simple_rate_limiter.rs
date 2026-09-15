use actix_web::{
    Error,
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
};
use futures_util::future::{LocalBoxFuture, Ready};
use std::{
    net::IpAddr,
    rc::Rc,
    task::{Context, Poll},
};

#[derive(Clone)]
pub struct GovernorRateLimiter {
    requests_per_second: u32,
    whitelist_paths: Vec<String>,
    trust_proxy: bool,
}
impl GovernorRateLimiter {
    pub fn per_ip_with_whitelist(requests_per_second: u32, whitelist_paths: Vec<&str>) -> Self {
        Self {
            requests_per_second,
            whitelist_paths: whitelist_paths.into_iter().map(str::to_owned).collect(),
            trust_proxy: std::env::var("TRUST_PROXY_HEADERS").is_ok_and(|v| v == "true"),
        }
    }
}
fn client_ip(req: &ServiceRequest, trust_proxy: bool) -> String {
    // Enable only when the backend is private and its sole ingress overwrites X-Real-IP.
    if trust_proxy {
        if let Some(ip) = req
            .headers()
            .get("X-Real-IP")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse::<IpAddr>().ok())
        {
            return ip.to_string();
        }
    }
    req.peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|| "unknown".into())
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
            config: self.clone(),
        })
    }
}
pub struct GovernorMiddleware<S> {
    service: Rc<S>,
    config: GovernorRateLimiter,
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
        let ip = client_ip(&req, self.config.trust_proxy);
        let path = req.path();
        let exempt = self.config.whitelist_paths.iter().any(|p| {
            if p.ends_with('/') {
                path.starts_with(p)
            } else {
                path == p
            }
        });
        let allow = exempt
            || crate::security_limits::allow(
                format!("http:{ip}"),
                self.config.requests_per_second,
                self.config.requests_per_second as f64,
            );
        let auth_allowed = !matches!(path, "/api/login" | "/api/register")
            || crate::security_limits::allow(format!("auth-ip:{ip}"), 20, 1.0 / 3.0);
        let ws_allowed = !path.starts_with("/ws/")
            || crate::security_limits::allow(format!("ws-connect:{ip}"), 12, 1.0);
        if !allow || !auth_allowed || !ws_allowed {
            return Box::pin(async {
                Err(actix_web::error::ErrorTooManyRequests(
                    "Too many requests; retry shortly",
                ))
            });
        }
        let service = Rc::clone(&self.service);
        Box::pin(async move { service.call(req).await })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;
    #[actix_web::test]
    async fn proxy_headers_are_ignored_by_default_and_forwarded_for_is_never_trusted() {
        let req = TestRequest::default()
            .peer_addr("[::1]:1234".parse().unwrap())
            .insert_header(("X-Forwarded-For", "1.2.3.4"))
            .insert_header(("X-Real-IP", "198.51.100.5"))
            .to_srv_request();
        assert_eq!(client_ip(&req, false), "::1");
        assert_eq!(client_ip(&req, true), "198.51.100.5");
        let invalid = TestRequest::default()
            .peer_addr("127.0.0.1:2".parse().unwrap())
            .insert_header(("X-Real-IP", "random-key"))
            .to_srv_request();
        assert_eq!(client_ip(&invalid, true), "127.0.0.1");
    }
}
