#[macro_use]
extern crate actix_web;
#[macro_use]
extern crate prometheus;

use dotenv::dotenv;
use std::{env, io};

use actix_cors::Cors;
use actix_web::{App, HttpServer, http, middleware, web};
use diesel::PgConnection;
use diesel::r2d2::ConnectionManager;
use r2d2::{Pool, PooledConnection};

use actix::prelude::*;

mod admin;
mod auth;
mod chat;
mod collection;
mod collectors;
mod companies;
mod constants;
mod franchises;
mod kudos;
mod metrics;
mod metrics_middleware;
mod pagination;
mod platforms;
mod product_details;
mod product_list;
mod profile;
mod redis;
mod register;
mod serial_requests;
mod simple_rate_limiter;

use crate::metrics::metrics_endpoint;
use crate::metrics_middleware::MetricsMiddleware;
use crate::redis::create_redis_pool;
use crate::simple_rate_limiter::GovernorRateLimiter;

pub type DBPool = Pool<ConnectionManager<PgConnection>>;
pub type DBPooledConnection = PooledConnection<ConnectionManager<PgConnection>>;

#[actix_web::main]
async fn main() -> io::Result<()> {
    dotenv().ok();
    env_logger::init_from_env(
        env_logger::Env::default().default_filter_or("actix_web=debug,actix_server=info"),
    );

    // Загрузка данных для подключения к базе данных
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    let pool = Pool::builder()
        .build(manager)
        .expect("Failed to create pool");

    auth::initialize(pool.clone()).map_err(|error| io::Error::other(error.to_string()))?;

    // Инициализация Redis
    let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://redis:6379".to_string());

    let redis_pool = create_redis_pool(&redis_url)
        .await
        .expect("Failed to create Redis pool");

    // Создание серверного экземпляра ChatServer
    let chat_server = chat::ChatServer::new(pool.clone()).start();
    let chat_server_data = web::Data::new(chat_server);

    // Настройка rate limiting - исправленные параметры
    let rate_limiter = GovernorRateLimiter::per_ip_with_whitelist(
        20, // 20 запросов в секунду
        vec![
            "/ws/",
            "/metrics",
            "/health",
            "/favicon.ico",
            "/static/",
            "/api/docs",
        ],
    );

    let bind_address = env::var("BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0:9090".to_string());

    // Запуск HTTP-сервера
    HttpServer::new(move || {
        App::new()
            // Rate limiting middleware - применяется ко всем запросам
            .wrap(rate_limiter.clone())
            .wrap(MetricsMiddleware)
            .service(metrics_endpoint)
            .route("/health", web::get().to(health))
            .app_data(web::Data::new(redis_pool.clone()))
            .app_data(web::Data::new(pool.clone()))
            .app_data(chat_server_data.clone())
            .wrap(middleware::Logger::default())
            .wrap(
                Cors::default()
                    .allow_any_origin()
                    .allowed_methods(vec!["GET", "POST", "DELETE", "OPTIONS"])
                    .allowed_headers(vec![
                        http::header::AUTHORIZATION,
                        http::header::CONTENT_TYPE,
                    ])
                    .max_age(3600),
            )
            .service(
                web::scope("/api")
                    .service(product_list::list)
                    .service(franchises::get_franchise)
                    .service(companies::get_company)
                    .service(product_details::get)
                    .service(register::register)
                    .service(auth::login)
                    .service(admin::me)
                    .service(kudos::score)
                    .service(kudos::challenge)
                    .service(admin::users)
                    .service(admin::promote)
                    .service(admin::delete_user)
                    .service(serial_requests::submit)
                    .service(serial_requests::list)
                    .service(serial_requests::photo)
                    .service(serial_requests::accept)
                    .service(serial_requests::reject)
                    .service(serial_requests::delete_archived)
                    .service(profile::change_password)
                    .service(profile::save_avatar)
                    .service(profile::get_avatar)
                    .service(profile::admin_badges)
                    .service(collection::add_release)
                    .service(collection::set_release_price)
                    .service(collection::remove_release)
                    .service(collection::add_wish)
                    .service(collection::remove_wish)
                    .service(collection::get_collection)
                    .service(collection::get_collection_by_login)
                    .service(collection::get_wishlist)
                    .service(collection::get_wts)
                    .service(collection::add_wts)
                    .service(collection::remove_wts)
                    .service(collection::get_collection_stats)
                    .service(collectors::get_collectors)
                    .service(collectors::get_collector_wts)
                    .service(platforms::get_platforms)
                    .service(chat::get_my_messages)
                    .service(chat::get_my_dialogs),
            )
            // Регистрация маршрута WebSocket для чата
            .service(web::resource("/ws/").to(chat::chat_ws))
            .service(web::resource("/ws/{login}").to(chat::chat_ws))
    })
    .bind(&bind_address)?
    .workers(8)
    .run()
    .await
}
// Liveness probe: confirms that the HTTP server is accepting requests.
async fn health() -> actix_web::HttpResponse {
    actix_web::HttpResponse::Ok().body("ok")
}

#[cfg(test)]
mod health_tests {
    use super::*;

    #[actix_web::test]
    async fn health_is_available_without_authentication() {
        let app =
            actix_web::test::init_service(App::new().route("/health", web::get().to(health))).await;
        let request = actix_web::test::TestRequest::get()
            .uri("/health")
            .to_request();
        let response = actix_web::test::call_service(&app, request).await;
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(actix_web::test::read_body(response).await, "ok");
    }
}
