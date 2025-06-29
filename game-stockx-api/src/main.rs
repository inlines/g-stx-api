#[macro_use]
extern crate actix_web;
use std::{env, io};
use dotenv::dotenv;
use actix_cors::Cors;
use actix_web::{middleware, App, HttpServer, web, http};
use diesel::r2d2::ConnectionManager;
use diesel::PgConnection;
use r2d2::{Pool, PooledConnection};

mod constants;
mod product;
mod response;
mod pagination;
mod register;
mod auth;
mod collection;
mod platforms;

pub type DBPool = Pool<ConnectionManager<PgConnection>>;
pub type DBPooledConnection = PooledConnection<ConnectionManager<PgConnection>>;

#[actix_web::main]
async fn main() -> io::Result<()> {
    dotenv().ok();
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("actix_web=debug,actix_server=info"));

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    let pool = Pool::builder()
        .build(manager)
        .expect("Failed to create pool");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
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
                    .service(product::list)
                    .service(product::get)
                    .service(register::register)
                    .service(auth::login)
                    .service(collection::add_release)
                    .service(collection::remove_release)
                    .service(collection::add_wish)
                    .service(collection::remove_wish)
                    .service(collection::get_collection)
                    .service(collection::get_wishlist)
                    .service(collection::get_collection_stats)
                    .service(collection::add_bid)
                    .service(collection::remove_bid)
                    .service(platforms::get_platforms)
                )
    })
    .bind("0.0.0.0:9090")?
    .run()
    .await
}