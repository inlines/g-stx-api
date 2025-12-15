use crate::constants::{CONNECTION_POOL_ERROR};
use actix_web::{HttpRequest, HttpResponse, post, web};
use crate::DBPool;
use jsonwebtoken::{encode, decode, Header, EncodingKey, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use diesel::prelude::*;
use argon2::{Argon2, PasswordVerifier};
use argon2::password_hash::PasswordHash;
use crate::metrics::{FAILED_LOGIN_ATTEMPTS, SUCCESSFUL_LOGINS, LOGIN_ATTEMPTS_BY_IP};

#[derive(Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

#[derive(QueryableByName)]
struct User {
    #[sql_type = "diesel::sql_types::Text"]
    user_login: String,
    
    #[sql_type = "diesel::sql_types::Text"]
    password_hash: String,
}

fn create_jwt(email: &str) -> String {
    let expiration = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::hours(24))
        .unwrap()
        .timestamp() as usize;

    let claims = Claims {
        sub: email.to_owned(),
        exp: expiration,
    };

    encode(&Header::default(), &claims, &EncodingKey::from_secret(b"my-secret")).unwrap()
}

pub fn verify_jwt(token: &str) -> Option<Claims> {
    decode::<Claims>(token, &DecodingKey::from_secret(b"my-secret"), &Validation::default())
        .map(|data| data.claims)
        .ok()
}

fn verify_password(password: &str, hash: &str) -> bool {
    let parsed_hash = PasswordHash::new(hash).unwrap();
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

#[derive(Deserialize)]
struct LoginRequest {
    user_login: String,
    password: String,
}

#[post("/login")]
async fn login(pool: web::Data<DBPool>, credentials: web::Json<LoginRequest>, req: HttpRequest) -> HttpResponse {
    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let client_ip = req
        .connection_info()
        .peer_addr()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let query = r#"
        SELECT user_login, password_hash
        FROM users
        WHERE user_login = $1
        LIMIT 1
    "#;

    let result = diesel::sql_query(query)
        .bind::<diesel::sql_types::Text, _>(&credentials.user_login)
        .get_result::<User>(conn);
    

    match result {
        Ok(user) => {
            if verify_password(&credentials.password, &user.password_hash) {
                SUCCESSFUL_LOGINS.inc();
                LOGIN_ATTEMPTS_BY_IP
                    .with_label_values(&[&client_ip, "success"])
                    .inc();
                let token = create_jwt(&user.user_login);
                return HttpResponse::Ok()
                  .json(serde_json::json!({ "token": token }));
            } else {
                FAILED_LOGIN_ATTEMPTS
                    .with_label_values(&["invalid_password", &credentials.user_login])
                    .inc();
                LOGIN_ATTEMPTS_BY_IP
                    .with_label_values(&[&client_ip, "failure"])
                    .inc();
                return HttpResponse::Unauthorized()
                  .body("Invalid credentials");
            }
        }
        Err(_) => {
            FAILED_LOGIN_ATTEMPTS
                .with_label_values(&["user_not_found", &credentials.user_login])
                .inc();
            LOGIN_ATTEMPTS_BY_IP
                .with_label_values(&[&client_ip, "failure"])
                .inc();
            HttpResponse::Unauthorized().body("Invalid credentials")
        },
    }
}
