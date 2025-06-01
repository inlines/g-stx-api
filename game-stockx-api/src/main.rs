#[macro_use]
extern crate actix_web;
#[macro_use]
extern crate diesel;

use std::{env, io};

use actix_web::{middleware, App, HttpServer};
use diesel::r2d2::ConnectionManager;
use diesel::PgConnection;
use r2d2::{Pool, PooledConnection};

mod constants;
mod product;
mod response;
mod sales;

#[actix_rt::main]
async fn main() -> io::Result<()> {


    unsafe {
        env::set_var("RUST_LOG", "actix_web=debug,actix_server=info");
    }
    env_logger::init();

    // set up database connection pool
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL");
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    let pool = r2d2::Pool::builder()
        .build(manager)
        .expect("Failed to create pool");

    HttpServer::new(|| {
        App::new()
            // enable logger - always register actix-web Logger middleware last
            .wrap(middleware::Logger::default())
            // register HTTP requests handlers
            .service(product::list)
            .service(product::get)
            .service(sales::list)
            .service(sales::add_sale)

    })
    .bind("0.0.0.0:9090")?
    .run()
    .await
}