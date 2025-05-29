#[macro_use]
extern crate actix_web;

use std::{env, io};

use actix_web::{middleware, App, HttpServer};

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