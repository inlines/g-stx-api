use actix_web::{post, web, HttpResponse};
use serde::Deserialize;
use crate::constants::{CONNECTION_POOL_ERROR};
use crate::DBPool;
use diesel::prelude::*;

use argon2::{Argon2, PasswordHasher};
use argon2::password_hash::{SaltString, rand_core::OsRng};
use crate::metrics::{SUCCESSFUL_REGISTRATIONS};

fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}


#[derive(Deserialize)]
pub struct RegisterRequest {
    user_login: String,
    password: String,
}

#[post("/register")]
pub async fn register(pool: web::Data<DBPool>, data: web::Json<RegisterRequest>) -> HttpResponse {

    if !data.user_login.chars().all(|c| c.is_ascii_alphanumeric()) {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({
                "error": "Логин должен содержать только латинские буквы и цифры"
            }));
    }

    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);
    let password_hash = hash_password(&data.password);

    let query = r#"
        INSERT INTO users (user_login, password_hash) VALUES ($1, $2)
    "#;

    let result = diesel::sql_query(query)
        .bind::<diesel::sql_types::Text, _>(&data.user_login)
        .bind::<diesel::sql_types::Text, _>(&password_hash)
        .execute(conn);

    match result {
        Ok(_) =>{SUCCESSFUL_REGISTRATIONS.inc(); HttpResponse::Created()
            .finish()},
        Err(_) => HttpResponse::Conflict()
            .body("User already exists"),
    }
}

