//! Administrator access is read from the database for every operation, never from
//! a client-supplied role. Mutations serialize authorization and deletion together.
use crate::{
    DBPool,
    auth::{Claims, authenticated_claims},
    chat::{ChatCommand, ChatServer},
};
use actix::Addr;
use actix_web::{HttpRequest, HttpResponse, ResponseError, http::StatusCode, web};
use diesel::{
    prelude::*,
    sql_types::{BigInt, Bool, Integer, Nullable, Text, Timestamp},
};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub(crate) enum AdminError {
    Invalid(&'static str),
    Conflict(&'static str),
    TooLarge,
    Missing(&'static str),
    Unauthorized,
    Forbidden,
    NotFound,
    SelfDelete,
    Database(diesel::result::Error),
    Internal,
}
impl From<diesel::result::Error> for AdminError {
    fn from(error: diesel::result::Error) -> Self {
        Self::Database(error)
    }
}
impl std::fmt::Display for AdminError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid(message) | Self::Conflict(message) | Self::Missing(message) => message,
            Self::TooLarge => "Фото должно быть не больше 768 КБ",
            Self::Unauthorized => "Войдите в аккаунт заново",
            Self::Forbidden => "Доступ разрешён только администраторам",
            Self::NotFound => "Пользователь не найден",
            Self::SelfDelete => "Нельзя удалить собственный аккаунт из админки",
            Self::Database(_) | Self::Internal => {
                "Не удалось выполнить операцию. Попробуйте ещё раз"
            }
        })
    }
}
impl ResponseError for AdminError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Invalid(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound | Self::Missing(_) => StatusCode::NOT_FOUND,
            Self::SelfDelete => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
    fn error_response(&self) -> HttpResponse {
        if let Self::Database(error) = self {
            log::error!("Admin database operation failed: {error}");
        }
        HttpResponse::build(self.status_code()).json(serde_json::json!({"error": self.to_string()}))
    }
}

#[derive(QueryableByName, Serialize)]
pub struct UserInfo {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Text)]
    user_login: String,
    #[diesel(sql_type = Bool)]
    is_admin: bool,
    #[diesel(sql_type = Nullable<Timestamp>)]
    created_at: Option<chrono::NaiveDateTime>,
}
fn current(conn: &mut PgConnection, claims: &Claims) -> Result<UserInfo, AdminError> {
    diesel::sql_query(
        "SELECT id, user_login, is_admin, created_at FROM users WHERE id=$1 AND user_login=$2",
    )
    .bind::<Integer, _>(claims.uid)
    .bind::<Text, _>(&claims.sub)
    .get_result(conn)
    .optional()?
    .ok_or(AdminError::Unauthorized)
}
pub(crate) fn require_admin(conn: &mut PgConnection, claims: &Claims) -> Result<(), AdminError> {
    if current(conn, claims)?.is_admin {
        Ok(())
    } else {
        Err(AdminError::Forbidden)
    }
}
pub(crate) async fn db<T, F>(pool: web::Data<DBPool>, operation: F) -> Result<T, AdminError>
where
    T: Send + 'static,
    F: FnOnce(&mut PgConnection) -> Result<T, AdminError> + Send + 'static,
{
    web::block(move || {
        let mut conn = pool.get().map_err(|_| AdminError::Internal)?;
        operation(&mut conn)
    })
    .await
    .map_err(|_| AdminError::Internal)?
}
pub(crate) fn claims(req: &HttpRequest) -> Result<Claims, AdminError> {
    authenticated_claims(req).ok_or(AdminError::Unauthorized)
}

#[get("/profile/me")]
async fn me(pool: web::Data<DBPool>, req: HttpRequest) -> Result<HttpResponse, AdminError> {
    let claims = claims(&req)?;
    Ok(HttpResponse::Ok().json(db(pool, move |conn| current(conn, &claims)).await?))
}

#[derive(Deserialize)]
struct UserQuery {
    query: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}
#[derive(QueryableByName)]
struct Count {
    #[diesel(sql_type = BigInt)]
    total: i64,
}
#[get("/admin/users")]
async fn users(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    query: web::Query<UserQuery>,
) -> Result<HttpResponse, AdminError> {
    let claims = claims(&req)?;
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);
    let search = query.query.as_deref().unwrap_or("").trim().to_owned();
    let (items, total_count) = db(pool, move |conn| {
        // Repeatable read keeps the count and page consistent during concurrent registrations.
        conn.build_transaction().repeatable_read().read_only().run(|conn| {
            require_admin(conn, &claims)?;
            let total = diesel::sql_query("SELECT COUNT(*) AS total FROM users WHERE strpos(lower(user_login), lower($1)) > 0")
                .bind::<Text,_>(&search).get_result::<Count>(conn)?.total;
            let items = diesel::sql_query("SELECT id, user_login, is_admin, created_at FROM users WHERE strpos(lower(user_login), lower($1)) > 0 ORDER BY lower(user_login), id LIMIT $2 OFFSET $3")
                .bind::<Text,_>(&search).bind::<BigInt,_>(limit).bind::<BigInt,_>(offset).load::<UserInfo>(conn)?;
            Ok((items, total))
        })
    }).await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({"items":items,"total_count":total_count})))
}

pub(crate) fn lock_admin_actions(conn: &mut PgConnection) -> Result<(), AdminError> {
    // All promotions/deletions share this transaction lock. An administrator
    // being deleted cannot race a previously authorized mutation afterwards.
    diesel::sql_query("SELECT pg_advisory_xact_lock(724891003)").execute(conn)?;
    Ok(())
}
#[post("/admin/users/{id}/promote")]
async fn promote(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    id: web::Path<i32>,
) -> Result<HttpResponse, AdminError> {
    let claims = claims(&req)?;
    let user = db(pool, move |conn| conn.transaction(|conn| {
        lock_admin_actions(conn)?;
        require_admin(conn, &claims)?;
        diesel::sql_query("UPDATE users SET is_admin=TRUE WHERE id=$1 RETURNING id,user_login,is_admin,created_at")
            .bind::<Integer,_>(*id).get_result::<UserInfo>(conn).optional()?.ok_or(AdminError::NotFound)
    })).await?;
    Ok(HttpResponse::Ok().json(user))
}
#[delete("/admin/users/{id}")]
async fn delete_user(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    id: web::Path<i32>,
    chat: web::Data<Addr<ChatServer>>,
) -> Result<HttpResponse, AdminError> {
    let claims = claims(&req)?;
    let removed = db(pool, move |conn| {
        conn.transaction(|conn| {
            lock_admin_actions(conn)?;
            require_admin(conn, &claims)?;
            if *id == claims.uid {
                return Err(AdminError::SelfDelete);
            }
            // Foreign keys remove collection, wishes, WTS and both sides of messages.
            // The avatar lives in this same user row. Any failure rolls everything back.
            diesel::sql_query(
                "DELETE FROM users WHERE id=$1 RETURNING id,user_login,is_admin,created_at",
            )
            .bind::<Integer, _>(*id)
            .get_result::<UserInfo>(conn)
            .optional()?
            .ok_or(AdminError::NotFound)
        })
    })
    .await?;
    chat.do_send(ChatCommand::RemoveUser {
        login: removed.user_login,
    });
    Ok(HttpResponse::NoContent().finish())
}
