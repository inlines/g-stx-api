//! Default-deny HTTP API boundary. Public catalogue pagination is checked by its handler.
use actix_web::{
    Error, HttpResponse,
    body::{EitherBody, MessageBody},
    dev::{ServiceRequest, ServiceResponse},
    http::Method,
    middleware::Next,
};

fn public(method: &Method, path: &str) -> bool {
    (method == Method::POST && matches!(path, "/api/login" | "/api/register"))
        || (method == Method::GET
            && matches!(path, "/api/products" | "/api/platforms" | "/api/genres"))
}

pub async fn enforce<B: MessageBody>(
    req: ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<EitherBody<B>>, Error> {
    // Login must report bad credentials itself, even if a browser sent an old token.
    let entry =
        req.method() == Method::POST && matches!(req.path(), "/api/login" | "/api/register");
    let needs_session =
        !public(req.method(), req.path()) || req.headers().contains_key("authorization");
    if !entry
        && needs_session
        && crate::auth::authenticated_claims_async(req.request())
            .await
            .is_none()
    {
        return Ok(req
            .into_response(HttpResponse::Unauthorized().finish())
            .map_into_right_body());
    }
    Ok(next.call(req).await?.map_into_left_body())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, middleware::from_fn, test, web};
    #[actix_web::test]
    async fn anonymous_boundary_is_default_deny() {
        let app = test::init_service(
            App::new().service(
                web::scope("/api")
                    .wrap(from_fn(enforce))
                    .default_service(web::to(|| async { HttpResponse::Ok().finish() })),
            ),
        )
        .await;
        for path in [
            "/api/products/1",
            "/api/release-calendar",
            "/api/companies/1",
            "/api/franchises/1",
            "/api/avatars/alice",
            "/api/users/admin-badges",
            "/api/kudos",
            "/api/collectors",
            "/api/collection",
            "/api/new-unprotected-route",
        ] {
            let response =
                test::call_service(&app, test::TestRequest::get().uri(path).to_request()).await;
            assert_eq!(response.status(), 401, "{path}");
        }
        for path in ["/api/products", "/api/platforms", "/api/genres"] {
            assert_eq!(
                test::call_service(&app, test::TestRequest::get().uri(path).to_request())
                    .await
                    .status(),
                200
            );
            assert_eq!(
                test::call_service(
                    &app,
                    test::TestRequest::get()
                        .uri(path)
                        .insert_header(("authorization", "Bearer invalid"))
                        .to_request()
                )
                .await
                .status(),
                401
            );
        }
        for path in ["/api/login", "/api/register"] {
            assert_eq!(
                test::call_service(&app, test::TestRequest::post().uri(path).to_request())
                    .await
                    .status(),
                200
            );
        }
        assert_eq!(
            test::call_service(
                &app,
                test::TestRequest::post().uri("/api/products").to_request()
            )
            .await
            .status(),
            401
        );
    }
}
