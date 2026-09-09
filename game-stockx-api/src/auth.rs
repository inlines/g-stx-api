use crate::DBPool;
use crate::constants::CONNECTION_POOL_ERROR;
use crate::metrics::{FAILED_LOGIN_ATTEMPTS, LOGIN_ATTEMPTS, SUCCESSFUL_LOGINS};
use actix_web::{HttpRequest, HttpResponse, post, web};
use argon2::password_hash::PasswordHash;
use argon2::{Argon2, PasswordVerifier};
use diesel::prelude::*;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

#[derive(QueryableByName)]
struct User {
    #[diesel(sql_type = diesel::sql_types::Text)]
    user_login: String,

    #[diesel(sql_type = diesel::sql_types::Text)]
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

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(b"my-secret"),
    )
    .unwrap()
}

pub fn verify_jwt(token: &str) -> Option<Claims> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(b"my-secret"),
        &Validation::default(),
    )
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
async fn login(
    pool: web::Data<DBPool>,
    credentials: web::Json<LoginRequest>,
    req: HttpRequest,
) -> HttpResponse {
    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let client_ip = req
        .connection_info()
        .peer_addr()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let username = &credentials.user_login;

    LOGIN_ATTEMPTS
        .with_label_values(&["attempt", username, &client_ip])
        .inc();

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
                LOGIN_ATTEMPTS
                    .with_label_values(&["success", username, &client_ip])
                    .inc();
                let token = create_jwt(&user.user_login);
                HttpResponse::Ok().json(serde_json::json!({ "token": token }))
            } else {
                // НЕВЕРНЫЙ ПАРОЛЬ
                FAILED_LOGIN_ATTEMPTS
                    .with_label_values(&["invalid_password", username, &client_ip])
                    .inc();

                LOGIN_ATTEMPTS
                    .with_label_values(&["failure", username, &client_ip])
                    .inc();
                HttpResponse::Unauthorized().body("Invalid credentials")
            }
        }
        Err(_) => {
            FAILED_LOGIN_ATTEMPTS
                .with_label_values(&["user_not_found", username, &client_ip])
                .inc();
            LOGIN_ATTEMPTS
                .with_label_values(&["failure", username, &client_ip])
                .inc();
            HttpResponse::Unauthorized().body("Invalid credentials")
        }
    }
}

/// Preserve the API's case-sensitive Bearer scheme and token contents.
pub(crate) fn bearer_token(req: &HttpRequest) -> Option<&str> {
    req.headers()
        .get(actix_web::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
}

pub(crate) fn authenticated_claims(req: &HttpRequest) -> Option<Claims> {
    bearer_token(req).and_then(verify_jwt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{http::header, test::TestRequest};

    #[actix_web::test]
    async fn bearer_scheme_remains_case_sensitive_and_does_not_trim_tokens() {
        for value in ["bearer token", "Basic token", "Bearer", " Bearer token"] {
            let request = TestRequest::default()
                .insert_header((header::AUTHORIZATION, value))
                .to_http_request();
            assert!(bearer_token(&request).is_none());
        }
        let request = TestRequest::default()
            .insert_header((header::AUTHORIZATION, "Bearer  token "))
            .to_http_request();
        assert_eq!(bearer_token(&request), Some(" token "));
    }

    #[actix_web::test]
    async fn missing_malformed_and_empty_tokens_remain_unauthenticated() {
        assert!(authenticated_claims(&TestRequest::default().to_http_request()).is_none());
        for value in ["Bearer ", "Bearer invalid"] {
            let request = TestRequest::default()
                .insert_header((header::AUTHORIZATION, value))
                .to_http_request();
            assert!(authenticated_claims(&request).is_none());
        }
    }

    #[actix_web::test]
    async fn valid_token_preserves_the_login() {
        let token = create_jwt("collector");
        let request = TestRequest::default()
            .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
            .to_http_request();
        assert_eq!(authenticated_claims(&request).unwrap().sub, "collector");
    }

    #[actix_web::test]
    async fn expired_tokens_are_rejected() {
        let claims = Claims {
            sub: "collector".to_owned(),
            exp: 1,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(b"my-secret"),
        )
        .unwrap();
        assert!(verify_jwt(&token).is_none());
    }
}
