use crate::DBPool;
use actix_web::{HttpResponse, post, web};
use diesel::prelude::*;
use serde::Deserialize;

use crate::metrics::SUCCESSFUL_REGISTRATIONS;
use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHasher};

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
    if !crate::password_policy::valid_login(&data.user_login) {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Логин: от 1 до 64 латинских букв и цифр"
        }));
    }

    if !crate::password_policy::valid_password(&data.password) {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({"error":"Пароль должен содержать от 8 до 128 символов"}));
    }
    let Some(hash_slot) = crate::password_policy::hash_slot() else {
        return HttpResponse::ServiceUnavailable()
            .insert_header(("Retry-After", "1"))
            .finish();
    };
    let result = web::block(move || -> Result<usize, diesel::result::Error> {
        let _hash_slot = hash_slot;
        let password_hash = hash_password(&data.password);
        let mut conn = pool
            .get()
            .map_err(|_| diesel::result::Error::RollbackTransaction)?;
        diesel::sql_query("INSERT INTO users(user_login,password_hash) VALUES($1,$2)")
            .bind::<diesel::sql_types::Text, _>(&data.user_login)
            .bind::<diesel::sql_types::Text, _>(password_hash)
            .execute(&mut conn)
    })
    .await;
    match result {
        Ok(Ok(_)) => {
            SUCCESSFUL_REGISTRATIONS.inc();
            HttpResponse::Created().finish()
        }
        Ok(Err(diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::UniqueViolation,
            _,
        ))) => HttpResponse::Conflict().body("User already exists"),
        _ => HttpResponse::ServiceUnavailable().body("Registration temporarily unavailable"),
    }
}
