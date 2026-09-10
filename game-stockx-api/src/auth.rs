use crate::DBPool;
use crate::constants::CONNECTION_POOL_ERROR;
use crate::metrics::{FAILED_LOGIN_ATTEMPTS, SUCCESSFUL_LOGINS};
use actix_web::{HttpRequest, HttpResponse, post, web};
use argon2::password_hash::PasswordHash;
use argon2::{Argon2, PasswordVerifier};
use diesel::prelude::*;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

struct AuthContext {
    secret: Vec<u8>,
    pool: DBPool,
}
static AUTH: OnceLock<AuthContext> = OnceLock::new();

/// Initialize once before accepting requests. Never fall back to a public key.
pub fn initialize(pool: DBPool) -> Result<(), Box<dyn std::error::Error>> {
    #[derive(QueryableByName)]
    struct Key {
        #[diesel(sql_type = diesel::sql_types::Binary)]
        secret: Vec<u8>,
    }
    let mut secret = [0u8; 32];
    OsRng.try_fill_bytes(&mut secret)?;
    let mut conn = pool.get()?;
    diesel::sql_query(
        "INSERT INTO auth_signing_keys(id, secret) VALUES(1, $1) ON CONFLICT(id) DO NOTHING",
    )
    .bind::<diesel::sql_types::Binary, _>(secret.as_slice())
    .execute(&mut conn)?;
    let key = diesel::sql_query("SELECT secret FROM auth_signing_keys WHERE id = 1")
        .get_result::<Key>(&mut conn)?;
    AUTH.set(AuthContext {
        secret: key.secret,
        pool,
    })
    .map_err(|_| "Authentication already initialized")?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub uid: i32,
    pub exp: usize,
}

#[derive(QueryableByName)]
struct User {
    #[diesel(sql_type = diesel::sql_types::Integer)]
    id: i32,
    #[diesel(sql_type = diesel::sql_types::Text)]
    user_login: String,

    #[diesel(sql_type = diesel::sql_types::Text)]
    password_hash: String,
}

fn create_jwt(email: &str, uid: i32, secret: &[u8]) -> String {
    let expiration = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::hours(24))
        .unwrap()
        .timestamp() as usize;

    let claims = Claims {
        sub: email.to_owned(),
        uid,
        exp: expiration,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .unwrap()
}

fn decode_jwt(token: &str, secret: &[u8]) -> Option<Claims> {
    let mut validation = Validation::default();
    validation.leeway = 0;
    decode::<Claims>(token, &DecodingKey::from_secret(secret), &validation)
        .map(|data| data.claims)
        .ok()
}

pub fn account_exists(uid: i32, account_login: &str) -> bool {
    let Some(auth) = AUTH.get() else {
        return false;
    };
    let Ok(mut conn) = auth.pool.get() else {
        return false;
    };
    #[derive(QueryableByName)]
    struct Exists {
        #[diesel(sql_type = diesel::sql_types::Bool)]
        present: bool,
    }
    diesel::sql_query(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id = $1 AND user_login = $2) AS present",
    )
    .bind::<diesel::sql_types::Integer, _>(uid)
    .bind::<diesel::sql_types::Text, _>(account_login)
    .get_result::<Exists>(&mut conn)
    .is_ok_and(|row| row.present)
}

pub fn verify_jwt(token: &str) -> Option<Claims> {
    let claims = decode_jwt(token, &AUTH.get()?.secret)?;
    // The immutable account ID prevents a deleted user's JWT from being reused
    // after someone registers the same login again.
    account_exists(claims.uid, &claims.sub).then_some(claims)
}

fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(hash) else {
        return false;
    };
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
async fn login(pool: web::Data<DBPool>, credentials: web::Json<LoginRequest>) -> HttpResponse {
    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let query = r#"
        SELECT id, user_login, password_hash
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
                let token = create_jwt(
                    &user.user_login,
                    user.id,
                    &AUTH.get().expect("Authentication initialized").secret,
                );
                HttpResponse::Ok().json(serde_json::json!({ "token": token }))
            } else {
                // НЕВЕРНЫЙ ПАРОЛЬ
                FAILED_LOGIN_ATTEMPTS
                    .with_label_values(&["invalid_password"])
                    .inc();
                HttpResponse::Unauthorized().body("Invalid credentials")
            }
        }
        Err(error) => {
            let reason = if matches!(error, diesel::result::Error::NotFound) {
                "user_not_found"
            } else {
                "database_error"
            };
            FAILED_LOGIN_ATTEMPTS.with_label_values(&[reason]).inc();
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
        let token = create_jwt("collector", 42, b"test-key");
        let request = TestRequest::default()
            .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
            .to_http_request();
        assert_eq!(
            decode_jwt(bearer_token(&request).unwrap(), b"test-key")
                .unwrap()
                .sub,
            "collector"
        );
    }

    #[actix_web::test]
    async fn expired_tokens_are_rejected() {
        let claims = Claims {
            sub: "collector".to_owned(),
            uid: 42,
            exp: 1,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(b"test-key"),
        )
        .unwrap();
        assert!(decode_jwt(&token, b"test-key").is_none());
    }
}
