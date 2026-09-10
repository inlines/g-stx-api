use crate::{
    DBPool,
    admin::{AdminError, claims, db, lock_admin_actions, require_admin},
};
use actix_web::{HttpRequest, HttpResponse, web};
use diesel::{
    prelude::*,
    sql_types::{Array, BigInt, Binary, Bool, Integer, Nullable, Text, Timestamptz},
};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

const PHOTO_LIMIT: usize = 768 * 1024;
const PLATFORMS: [i32; 5] = [8, 9, 48, 167, 38]; // PS2, PS3, PS4, PS5, PSP (IGDB).

fn normalize_serial(value: &str) -> Result<String, AdminError> {
    let value = value.trim().to_ascii_uppercase();
    if !(3..=64).contains(&value.len())
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b" -_./".contains(&c))
        || !value.bytes().any(|c| c.is_ascii_alphanumeric())
    {
        return Err(AdminError::Invalid(
            "Укажите серийник: 3–64 символа, латинские буквы, цифры, пробелы или - _ . /",
        ));
    }
    Ok(value)
}
fn normalize_photo(bytes: &[u8]) -> Result<Vec<u8>, AdminError> {
    if bytes.len() > PHOTO_LIMIT {
        return Err(AdminError::TooLarge);
    }
    let invalid = || AdminError::Invalid("Нужно читаемое фото JPEG до 2000×2000 пикселей");
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Jpeg);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2000);
    limits.max_image_height = Some(2000);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|_| invalid())?;
    if image.width() < 32 || image.height() < 32 {
        return Err(invalid());
    }
    // Keep the exact JPEG the user checked in the preview: no second lossy compression.
    Ok(bytes.to_vec())
}

#[derive(QueryableByName)]
struct Id {
    #[diesel(sql_type = Integer)]
    id: i32,
}
#[derive(QueryableByName)]
struct Count {
    #[diesel(sql_type = BigInt)]
    total: i64,
}
#[derive(QueryableByName)]
struct Release {
    #[diesel(sql_type = Integer)]
    platform: i32,
    #[diesel(sql_type = Nullable<Array<Nullable<Text>>>)]
    serial: Option<Vec<Option<String>>>,
}
fn release(conn: &mut PgConnection, id: i32) -> Result<Release, AdminError> {
    let row = diesel::sql_query("SELECT platform, serial FROM releases WHERE id=$1 FOR UPDATE")
        .bind::<Integer, _>(id)
        .get_result::<Release>(conn)
        .optional()?
        .ok_or(AdminError::Missing("Релиз не найден"))?;
    if !PLATFORMS.contains(&row.platform) {
        return Err(AdminError::Invalid(
            "Заявки доступны только для PS2, PS3, PS4, PS5 и PSP",
        ));
    }
    Ok(row)
}
#[derive(Deserialize)]
struct Submission {
    serial: String,
}
// Raw JPEG avoids multipart temporary files and base64 overhead. Limit applies to this route only.
#[post("/releases/{id}/serial-requests")]
async fn submit(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    id: web::Path<i32>,
    query: web::Query<Submission>,
    mut payload: web::Payload,
) -> Result<HttpResponse, AdminError> {
    use futures_util::StreamExt;
    let user = claims(&req)?;
    let serial = normalize_serial(&query.serial)?;
    let mut bytes = Vec::new();
    while let Some(chunk) = payload.next().await {
        let chunk = chunk.map_err(|_| AdminError::Invalid("Не удалось загрузить фото"))?;
        if bytes.len() + chunk.len() > PHOTO_LIMIT {
            return Err(AdminError::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    let id = db(pool, move |conn| {
        let image = normalize_photo(&bytes)?;
        conn.transaction(|conn| {
            // Serialize submissions per user, including the pending-count limit.
            diesel::sql_query("SELECT id FROM users WHERE id=$1 AND user_login=$2 FOR UPDATE")
                .bind::<Integer,_>(user.uid).bind::<Text,_>(&user.sub).get_result::<Id>(conn).optional()?.ok_or(AdminError::Unauthorized)?;
            let existing = release(conn, *id)?;
            if existing.serial.into_iter().flatten().flatten().any(|s| s.trim().eq_ignore_ascii_case(&serial)) {
                return Err(AdminError::Conflict("Этот серийник уже указан у релиза"));
            }
            let pending = diesel::sql_query("SELECT count(*) AS total FROM release_serial_requests WHERE submitter_id=$1 AND status='pending'")
                .bind::<Integer,_>(user.uid).get_result::<Count>(conn)?.total;
            if pending >= 20 { return Err(AdminError::Conflict("У вас уже 20 активных заявок. Дождитесь их рассмотрения")); }
            let added = diesel::sql_query("INSERT INTO release_serial_requests(release_id,submitter_id,serial,photo) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING RETURNING id")
                .bind::<Integer,_>(*id).bind::<Integer,_>(user.uid).bind::<Text,_>(&serial).bind::<Binary,_>(image)
                .get_result::<Id>(conn).optional()?.ok_or(AdminError::Conflict("Заявка с этим серийником уже ожидает рассмотрения"))?;
            Ok(added.id)
        })
    }).await?;
    Ok(HttpResponse::Created().json(serde_json::json!({"id":id,"status":"pending"})))
}

#[derive(Serialize, QueryableByName)]
struct RequestInfo {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Integer)]
    release_id: i32,
    #[diesel(sql_type = Integer)]
    product_id: i32,
    #[diesel(sql_type = Text)]
    product_name: String,
    #[diesel(sql_type = Array<Text>)]
    existing_serials: Vec<String>,
    #[diesel(sql_type = Integer)]
    platform_id: i32,
    #[diesel(sql_type = Text)]
    platform_name: String,
    #[diesel(sql_type = Integer)]
    region_id: i32,
    #[diesel(sql_type = Text)]
    region_name: String,
    #[diesel(sql_type = Nullable<Integer>)]
    release_date: Option<i32>,
    #[diesel(sql_type = Bool)]
    digital_only: bool,
    #[diesel(sql_type = Text)]
    submitter: String,
    #[diesel(sql_type = Text)]
    serial: String,
    #[diesel(sql_type = Text)]
    submitted_serial: String,
    #[diesel(sql_type = Text)]
    status: String,
    #[diesel(sql_type = Timestamptz)]
    created_at: chrono::DateTime<chrono::Utc>,
    #[diesel(sql_type = Nullable<Timestamptz>)]
    reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
    #[diesel(sql_type = Nullable<Text>)]
    reviewer: Option<String>,
}
#[derive(Deserialize)]
struct ListQuery {
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}
#[get("/admin/serial-requests")]
async fn list(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    query: web::Query<ListQuery>,
) -> Result<HttpResponse, AdminError> {
    let user = claims(&req)?;
    let status = query.status.clone().unwrap_or_else(|| "pending".into());
    if !["pending", "accepted"].contains(&status.as_str()) {
        return Err(AdminError::Invalid("Неизвестный статус заявки"));
    }
    let limit = query.limit.unwrap_or(10).clamp(1, 50);
    let offset = query.offset.unwrap_or(0).max(0);
    let (items, total) = db(pool, move |conn| conn.build_transaction().repeatable_read().read_only().run(|conn| {
        require_admin(conn, &user)?;
        let total = diesel::sql_query("SELECT count(*) AS total FROM release_serial_requests WHERE status=$1")
            .bind::<Text,_>(&status).get_result::<Count>(conn)?.total;
        let items = diesel::sql_query("SELECT q.id,q.release_id,r.product_id,p.name AS product_name,array_remove(COALESCE(r.serial,ARRAY[]::text[]),NULL) AS existing_serials,r.platform AS platform_id,pl.name AS platform_name,r.release_region AS region_id,reg.name AS region_name,r.release_date,r.digital_only,u.user_login AS submitter,COALESCE(q.accepted_serial,q.serial) AS serial,q.serial AS submitted_serial,q.status,q.created_at,q.reviewed_at,reviewer.user_login AS reviewer FROM release_serial_requests q JOIN releases r ON r.id=q.release_id JOIN products p ON p.id=r.product_id JOIN platforms pl ON pl.id=r.platform JOIN regions reg ON reg.id=r.release_region JOIN users u ON u.id=q.submitter_id LEFT JOIN users reviewer ON reviewer.id=q.reviewed_by WHERE q.status=$1 ORDER BY COALESCE(q.reviewed_at,q.created_at) DESC,q.id DESC LIMIT $2 OFFSET $3")
            .bind::<Text,_>(&status).bind::<BigInt,_>(limit).bind::<BigInt,_>(offset).load::<RequestInfo>(conn)?;
        Ok((items,total))
    })).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(serde_json::json!({"items":items,"total_count":total})))
}
#[get("/admin/serial-requests/{id}/photo")]
async fn photo(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    id: web::Path<i32>,
) -> Result<HttpResponse, AdminError> {
    let user = claims(&req)?;
    #[derive(QueryableByName)]
    struct Photo {
        #[diesel(sql_type = Binary)]
        photo: Vec<u8>,
    }
    let image = db(pool, move |conn| {
        require_admin(conn, &user)?;
        diesel::sql_query("SELECT photo FROM release_serial_requests WHERE id=$1")
            .bind::<Integer, _>(*id)
            .get_result::<Photo>(conn)
            .optional()?
            .ok_or(AdminError::Missing("Заявка не найдена"))
    })
    .await?;
    Ok(HttpResponse::Ok()
        .content_type("image/jpeg")
        .insert_header(("Cache-Control", "no-store"))
        .insert_header(("X-Content-Type-Options", "nosniff"))
        .body(image.photo))
}
#[derive(QueryableByName)]
struct Pending {
    #[diesel(sql_type = Integer)]
    release_id: i32,
    #[diesel(sql_type = Text)]
    serial: String,
    #[diesel(sql_type = Nullable<Text>)]
    accepted_serial: Option<String>,
    #[diesel(sql_type = Text)]
    status: String,
}
fn pending(conn: &mut PgConnection, id: i32) -> Result<Pending, AdminError> {
    diesel::sql_query(
        "SELECT release_id, serial, accepted_serial, status FROM release_serial_requests WHERE id=$1 FOR UPDATE",
    )
    .bind::<Integer, _>(id)
    .get_result(conn)
    .optional()?
    .ok_or(AdminError::Missing("Заявка не найдена"))
}
#[derive(Deserialize)]
struct Acceptance {
    serial: Option<String>,
}

#[post("/admin/serial-requests/{id}/accept")]
async fn accept(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    id: web::Path<i32>,
    body: web::Bytes,
) -> Result<HttpResponse, AdminError> {
    let user = claims(&req)?;
    let requested = if body.is_empty() {
        None
    } else {
        serde_json::from_slice::<Acceptance>(&body)
            .map_err(|_| AdminError::Invalid("Некорректные данные серийника"))?
            .serial
    }
    .map(|value| normalize_serial(&value))
    .transpose()?;
    db(pool, move |conn| conn.transaction(|conn| {
        lock_admin_actions(conn)?;
        require_admin(conn, &user)?;
        let item = pending(conn, *id)?;
        if item.status == "accepted" {
            let accepted = item.accepted_serial.as_deref().unwrap_or(&item.serial);
            if requested.as_deref().is_some_and(|value| value != accepted) {
                return Err(AdminError::Conflict("Заявка уже принята с другим серийником. Обновите список"));
            }
            return Ok(());
        }
        let serial = match requested { Some(value) => value, None => normalize_serial(&item.serial)? };
        release(conn, item.release_id)?;
        diesel::sql_query("UPDATE releases SET serial = CASE WHEN EXISTS(SELECT 1 FROM unnest(serial) s WHERE upper(btrim(s))=$1) THEN serial ELSE array_append(COALESCE(serial,ARRAY[]::text[]),$1) END WHERE id=$2")
            .bind::<Text,_>(&serial).bind::<Integer,_>(item.release_id).execute(conn)?;
        diesel::sql_query("UPDATE release_serial_requests SET status='accepted',reviewed_at=now(),reviewed_by=$1,accepted_serial=$3 WHERE id=$2")
            .bind::<Integer,_>(user.uid).bind::<Integer,_>(*id).bind::<Text,_>(&serial).execute(conn)?;
        diesel::sql_query("INSERT INTO kudos_awards(request_id,user_id) SELECT id,submitter_id FROM release_serial_requests WHERE id=$1 ON CONFLICT(request_id) DO NOTHING")
            .bind::<Integer,_>(*id).execute(conn)?;
        Ok(())
    })).await?;
    // Releases are read directly from PostgreSQL, never from the product cache.
    Ok(HttpResponse::NoContent().finish())
}
async fn delete_request(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    id: i32,
    expected: &'static str,
) -> Result<HttpResponse, AdminError> {
    let user = claims(&req)?;
    db(pool, move |conn| {
        conn.transaction(|conn| {
            lock_admin_actions(conn)?;
            require_admin(conn, &user)?;
            let item = pending(conn, id)?;
            if item.status != expected {
                return Err(AdminError::Conflict(
                    "Статус заявки изменился. Обновите список",
                ));
            }
            // Delete only the evidence record. Approved catalogue serials remain untouched.
            diesel::sql_query("DELETE FROM release_serial_requests WHERE id=$1")
                .bind::<Integer, _>(id)
                .execute(conn)?;
            Ok(())
        })
    })
    .await?;
    Ok(HttpResponse::NoContent().finish())
}
#[delete("/admin/serial-requests/{id}")]
async fn reject(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    id: web::Path<i32>,
) -> Result<HttpResponse, AdminError> {
    delete_request(pool, req, *id, "pending").await
}
#[delete("/admin/serial-requests/{id}/archive")]
async fn delete_archived(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    id: web::Path<i32>,
) -> Result<HttpResponse, AdminError> {
    delete_request(pool, req, *id, "accepted").await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[actix_web::test]
    async fn supports_ps2_but_not_ps1() {
        assert!(PLATFORMS.contains(&8));
        assert!(!PLATFORMS.contains(&7));
        assert_eq!(PLATFORMS, [8, 9, 48, 167, 38]);
    }
    #[actix_web::test]
    async fn serial_normalization_preserves_separators() {
        assert_eq!(normalize_serial("  slus-12345  ").unwrap(), "SLUS-12345");
        for invalid in ["", "--", "   ", "<script>", "СЕРИЙНИК", "a\nb"] {
            assert!(normalize_serial(invalid).is_err());
        }
        assert!(normalize_serial(&"A".repeat(65)).is_err());
    }
    #[actix_web::test]
    async fn rejects_non_images_and_large_uploads() {
        assert!(normalize_photo(b"not an image").is_err());
        assert!(matches!(
            normalize_photo(&vec![0; PHOTO_LIMIT + 1]),
            Err(AdminError::TooLarge)
        ));
    }
}
