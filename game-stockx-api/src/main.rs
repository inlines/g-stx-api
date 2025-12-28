#[macro_use]
extern crate actix_web;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate prometheus;

use std::{env, io, num::NonZeroU32};
use dotenv::dotenv;

use actix_cors::Cors;
use actix_web::{middleware, App, HttpServer, web, http};
use diesel::r2d2::ConnectionManager;
use diesel::PgConnection;
use r2d2::{Pool, PooledConnection};

use actix::prelude::*;

mod constants;
mod product_list;
mod product_details;
mod response;
mod pagination;
mod register;
mod auth;
mod collection;
mod collectors;
mod platforms;
mod chat;
mod redis;
mod metrics;
mod metrics_middleware;
mod simple_rate_limiter;

use crate::simple_rate_limiter::GovernorRateLimiter;
use crate::metrics::metrics_endpoint;
use crate::metrics_middleware::MetricsMiddleware;
use crate::redis::create_redis_pool;

pub type DBPool = Pool<ConnectionManager<PgConnection>>;
pub type DBPooledConnection = PooledConnection<ConnectionManager<PgConnection>>;

#[actix_web::main]
async fn main() -> io::Result<()> {
    dotenv().ok();
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("actix_web=debug,actix_server=info"));

    // Загрузка данных для подключения к базе данных
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    let pool = Pool::builder()
        .build(manager)
        .expect("Failed to create pool");

    // Инициализация Redis
    let redis_url = env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://redis:6379".to_string());
    
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

    // Запуск HTTP-сервера
    HttpServer::new(move || {
        App::new()
            // Rate limiting middleware - применяется ко всем запросам
            .wrap(rate_limiter.clone())
            .wrap(MetricsMiddleware)
            .service(metrics_endpoint)
            .app_data(web::Data::new(redis_pool.clone()))
            .app_data(web::Data::new(pool.clone()))
            .app_data(chat_server_data.clone())
            .wrap(middleware::Logger::default())
            .wrap(
                Cors::default()
                    .allow_any_origin()
                    .allowed_methods(vec!["GET", "POST", "OPTIONS"])
                    .allowed_headers(vec![http::header::AUTHORIZATION, http::header::CONTENT_TYPE])
                    .max_age(3600)
            )
            .service(
                web::scope("/api")
                    .service(product_list::list)
                    .service(product_details::get)
                    .service(register::register)
                    .service(auth::login)
                    .service(collection::add_release)
                    .service(collection::set_release_price)
                    .service(collection::remove_release)
                    .service(collection::add_wish)
                    .service(collection::remove_wish)
                    .service(collection::get_collection)
                    .service(collection::get_collection_by_login)
                    .service(collection::get_wishlist)
                    .service(collection::get_collection_stats)
                    .service(collection::add_bid)
                    .service(collection::remove_bid)
                    .service(collectors::get_collectors)
                    .service(platforms::get_platforms)
                    .service(chat::get_my_messages)
                    .service(chat::get_my_dialogs)
            )
            // Регистрация маршрута WebSocket для чата
            .service(web::resource("/ws/{login}").to(chat::chat_ws))
    })
    .bind("0.0.0.0:9090")?
    .workers(8)
    .run()
    .await
}