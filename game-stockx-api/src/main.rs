#[macro_use]
extern crate actix_web;
#[macro_use]
extern crate diesel;

use std::{env, io};
use dotenv::dotenv;
use actix_web::{middleware, App, HttpServer, web};
use diesel::r2d2::ConnectionManager;
use diesel::PgConnection;
use r2d2::{Pool, PooledConnection};

mod constants;
mod product;
mod response;
mod sales;
mod pagination;

pub type DBPool = Pool<ConnectionManager<PgConnection>>;
pub type DBPooledConnection = PooledConnection<ConnectionManager<PgConnection>>;

#[actix_web::main] // Убедись, что используешь `#[actix_web::main]` для асинхронного основного потока
async fn main() -> io::Result<()> {
    // Загружаем переменные окружения из файла .env
    dotenv().ok();

    // Устанавливаем уровень логирования, можно также читать из переменной окружения
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("actix_web=debug,actix_server=info"));

    // Настройка пула для подключения к базе данных
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    let pool = Pool::builder()
        .build(manager)
        .expect("Failed to create pool");

    // Запуск HTTP сервера
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .wrap(middleware::Logger::default()) // Логирование
            .service(product::list) // Роуты для работы с продуктами
            .service(product::get)
            .service(sales::list) // Роуты для работы с продажами
            .service(sales::add_sale)
    })
    .bind("0.0.0.0:9090")? // Привязываем сервер к адресу
    .run()
    .await
}