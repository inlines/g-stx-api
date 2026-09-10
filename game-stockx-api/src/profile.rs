use crate::{DBPool, auth::authenticated_claims};
use actix_web::{HttpRequest, HttpResponse, web};
use argon2::{
    Argon2, PasswordHasher, PasswordVerifier,
    password_hash::{PasswordHash, SaltString, rand_core::OsRng},
};
use diesel::{
    prelude::*,
    sql_types::{Binary, Nullable, Text},
};
use serde::Deserialize;
use std::io::Cursor;

const AVATAR_LIMIT: usize = 32768;

#[derive(Deserialize)]
pub struct PasswordChange {
    old_password: String,
    new_password: String,
    confirm_password: String,
}
#[derive(QueryableByName)]
struct PasswordRow {
    #[diesel(sql_type = Text)]
    password_hash: String,
}
#[derive(QueryableByName)]
struct AvatarRow {
    #[diesel(sql_type = Nullable<Binary>)]
    avatar: Option<Vec<u8>>,
}

fn error(status: actix_web::http::StatusCode, message: &str) -> HttpResponse {
    HttpResponse::build(status).json(serde_json::json!({"error": message}))
}

#[post("/profile/password")]
pub async fn change_password(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    data: web::Json<PasswordChange>,
) -> HttpResponse {
    use actix_web::http::StatusCode as S;
    let Some(claims) = authenticated_claims(&req) else {
        return HttpResponse::Unauthorized().finish();
    };
    if data.new_password != data.confirm_password {
        return error(S::BAD_REQUEST, "Новые пароли не совпадают");
    }
    if !(8..=128).contains(&data.new_password.chars().count()) || data.old_password.len() > 4096 {
        return error(
            S::BAD_REQUEST,
            "Новый пароль должен содержать от 8 до 128 символов",
        );
    }
    let result = web::block(move || -> Result<bool, String> {
        let conn = &mut pool.get().map_err(|e| e.to_string())?;
        let user = diesel::sql_query("SELECT password_hash FROM users WHERE user_login = $1")
            .bind::<Text, _>(&claims.sub)
            .get_result::<PasswordRow>(conn)
            .optional()
            .map_err(|e| e.to_string())?;
        let Some(user) = user else {
            return Ok(false);
        };
        let hash = PasswordHash::new(&user.password_hash).map_err(|e| e.to_string())?;
        if Argon2::default()
            .verify_password(data.old_password.as_bytes(), &hash)
            .is_err()
        {
            return Ok(false);
        }
        let salt = SaltString::generate(&mut OsRng);
        let new_hash = Argon2::default()
            .hash_password(data.new_password.as_bytes(), &salt)
            .map_err(|e| e.to_string())?
            .to_string();
        // Do not overwrite a password changed by a concurrent request.
        let count = diesel::sql_query(
            "UPDATE users SET password_hash = $1 WHERE user_login = $2 AND password_hash = $3",
        )
        .bind::<Text, _>(new_hash)
        .bind::<Text, _>(claims.sub)
        .bind::<Text, _>(user.password_hash)
        .execute(conn)
        .map_err(|e| e.to_string())?;
        Ok(count == 1)
    })
    .await;
    match result {
        Ok(Ok(true)) => HttpResponse::NoContent().finish(),
        Ok(Ok(false)) => error(S::BAD_REQUEST, "Старый пароль неверен или уже изменён"),
        _ => error(S::INTERNAL_SERVER_ERROR, "Не удалось изменить пароль"),
    }
}

fn normalize_avatar(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() > AVATAR_LIMIT {
        return Err("Аватар больше 32 КБ".into());
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_limits(png::Limits { bytes: 131072 });
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|_| "Некорректный PNG")?;
    if reader.info().width != 64
        || reader.info().height != 64
        || reader.info().animation_control.is_some()
    {
        return Err("Нужен статичный PNG размером 64×64".into());
    }
    let mut pixels = vec![0; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut pixels)
        .map_err(|_| "Некорректный PNG")?;
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, 64, 64);
        encoder.set_color(frame.color_type);
        encoder.set_depth(frame.bit_depth);
        encoder.set_compression(png::Compression::Best);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer
            .write_image_data(&pixels[..frame.buffer_size()])
            .map_err(|e| e.to_string())?;
    }
    if output.len() > AVATAR_LIMIT {
        return Err("Аватар больше 32 КБ".into());
    }
    Ok(output)
}

#[post("/profile/avatar")]
pub async fn save_avatar(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    body: web::Bytes,
) -> HttpResponse {
    use actix_web::http::StatusCode as S;
    let Some(claims) = authenticated_claims(&req) else {
        return HttpResponse::Unauthorized().finish();
    };
    if body.len() > AVATAR_LIMIT {
        return error(S::PAYLOAD_TOO_LARGE, "Аватар больше 32 КБ");
    }
    let avatar = match normalize_avatar(&body) {
        Ok(v) => v,
        Err(e) => return error(S::BAD_REQUEST, &e),
    };
    match web::block(move || {
        let conn = &mut pool.get().map_err(|e| e.to_string())?;
        diesel::sql_query("UPDATE users SET avatar = $1 WHERE user_login = $2")
            .bind::<Binary, _>(avatar)
            .bind::<Text, _>(claims.sub)
            .execute(conn)
            .map_err(|e| e.to_string())
    })
    .await
    {
        Ok(Ok(1)) => HttpResponse::NoContent().finish(),
        Ok(Ok(0)) => HttpResponse::NotFound().finish(),
        _ => error(S::INTERNAL_SERVER_ERROR, "Не удалось сохранить аватар"),
    }
}

// Public display metadata only: no IDs, private account details or admin actions.
#[get("/users/admin-badges")]
pub async fn admin_badges(pool: web::Data<DBPool>) -> HttpResponse {
    #[derive(QueryableByName)]
    struct Badge {
        #[diesel(sql_type = Text)]
        user_login: String,
    }
    match web::block(move || {
        let conn = &mut pool.get().map_err(|e| e.to_string())?;
        diesel::sql_query("SELECT user_login FROM users WHERE is_admin = TRUE ORDER BY user_login")
            .load::<Badge>(conn)
            .map(|rows| {
                rows.into_iter()
                    .map(|row| row.user_login)
                    .collect::<Vec<_>>()
            })
            .map_err(|e| e.to_string())
    })
    .await
    {
        Ok(Ok(logins)) => HttpResponse::Ok()
            .insert_header(("Cache-Control", "no-store"))
            .json(logins),
        _ => HttpResponse::InternalServerError().finish(),
    }
}

// Avatars are public profile images. No tokens in image URLs.
#[get("/avatars/{login}")]
pub async fn get_avatar(pool: web::Data<DBPool>, login: web::Path<String>) -> HttpResponse {
    match web::block(move || {
        let conn = &mut pool.get().map_err(|e| e.to_string())?;
        diesel::sql_query("SELECT avatar FROM users WHERE user_login = $1")
            .bind::<Text, _>(login.into_inner())
            .get_result::<AvatarRow>(conn)
            .optional()
            .map_err(|e| e.to_string())
    })
    .await
    {
        Ok(Ok(Some(AvatarRow {
            avatar: Some(bytes),
        }))) => HttpResponse::Ok()
            .content_type("image/png")
            .insert_header(("Cache-Control", "no-cache"))
            .insert_header(("X-Content-Type-Options", "nosniff"))
            .body(bytes),
        Ok(Ok(_)) => HttpResponse::NotFound().finish(),
        _ => HttpResponse::InternalServerError().finish(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut data = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut data, width, height);
            encoder.set_color(png::ColorType::Rgb);
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_image_data(&vec![123; (width * height * 3) as usize])
                .unwrap();
        }
        data
    }
    #[actix_web::test]
    async fn validates_and_reencodes_only_small_square_pngs() {
        assert!(normalize_avatar(b"not a PNG").is_err());
        assert!(normalize_avatar(&vec![0; AVATAR_LIMIT + 1]).is_err());
        assert!(normalize_avatar(&png(64, 32)).is_err());
        assert!(normalize_avatar(&png(128, 128)).is_err());
        let result = normalize_avatar(&png(64, 64)).unwrap();
        assert!(result.len() <= AVATAR_LIMIT);
        let reader = png::Decoder::new(Cursor::new(result)).read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height), (64, 64));
    }
}
