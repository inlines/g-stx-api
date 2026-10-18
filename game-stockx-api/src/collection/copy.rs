use crate::{DBPool, admin};
use actix_web::{HttpRequest, HttpResponse, post, web};
use diesel::{
    prelude::*,
    sql_types::{Bool, Integer, Nullable, Text},
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CopyDetails {
    release_id: i32,
    selected_serial: Option<String>,
    cib: Option<bool>,
    #[serde(default, deserialize_with = "edition_update")]
    selected_edition: Option<Option<String>>,
}

fn edition_update<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(deserializer).map(Some)
}

// Membership is checked against this exact release, not another region/game/platform.
const UPDATE_COPY: &str = r#"
WITH candidate AS (
 SELECT o.release_id, o.user_login,
   (SELECT s FROM unnest(r.serial) s
    WHERE regexp_replace(upper(s),'[^A-Z0-9]','','g') = regexp_replace(upper($3::text),'[^A-Z0-9]','','g')
    ORDER BY (s=$3::text) DESC, s LIMIT 1) AS serial
 FROM users_have_releases o JOIN releases r ON r.id=o.release_id
 WHERE o.user_login=$2 AND o.release_id=$1 FOR UPDATE OF o
), updated AS (
 UPDATE users_have_releases o SET selected_serial=c.serial, cib=$4, selected_edition=CASE WHEN $6 THEN $5 ELSE o.selected_edition END
 FROM candidate c WHERE o.release_id=c.release_id AND o.user_login=c.user_login
 AND ($3::text IS NULL OR c.serial IS NOT NULL) RETURNING o.release_id, o.user_login
), sale AS (
 UPDATE users_have_wts s SET cib=COALESCE($4,false) FROM updated u
 WHERE s.release_id=u.release_id AND s.user_login=u.user_login
)
SELECT CASE WHEN NOT EXISTS(SELECT 1 FROM candidate) THEN 404
 WHEN NOT EXISTS(SELECT 1 FROM updated) THEN 400 ELSE 200 END AS status
"#;
#[derive(QueryableByName)]
struct ResultRow {
    #[diesel(sql_type=Integer)]
    status: i32,
}

#[post("/collection-copy")]
pub(crate) async fn set_copy(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    data: web::Json<CopyDetails>,
) -> HttpResponse {
    let Some(claims) = crate::auth::authenticated_claims_async(&req).await else {
        return HttpResponse::Unauthorized().finish();
    };
    // Locally curated releases use negative IDs; existence and ownership are checked in SQL.
    if data
        .selected_serial
        .as_ref()
        .is_some_and(|s| s.trim().is_empty() || s.len() > 128)
    {
        return HttpResponse::BadRequest().body("Invalid serial");
    }
    if data
        .selected_edition
        .as_ref()
        .and_then(|e| e.as_deref())
        .is_some_and(|e| !matches!(e, "standard" | "platinum" | "essentials"))
    {
        return HttpResponse::BadRequest().body("Invalid edition");
    }
    let data = data.into_inner();
    let update_edition = data.selected_edition.is_some();
    match admin::db(pool, move |conn| {
        Ok(diesel::sql_query(UPDATE_COPY)
            .bind::<Integer, _>(data.release_id)
            .bind::<Text, _>(claims.sub)
            .bind::<Nullable<Text>, _>(data.selected_serial)
            .bind::<Nullable<Bool>, _>(data.cib)
            .bind::<Nullable<Text>, _>(data.selected_edition.flatten())
            .bind::<Bool, _>(update_edition)
            .get_result::<ResultRow>(conn)?)
    })
    .await
    {
        Ok(row) if row.status == 200 => HttpResponse::Ok().finish(),
        Ok(row) if row.status == 404 => HttpResponse::NotFound().body("Owned release not found"),
        Ok(_) => HttpResponse::BadRequest().body("Serial does not belong to this release"),
        Err(e) => {
            log::error!("Copy details update: {e}");
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CopyDetails;
    #[actix_web::test]
    async fn omitted_edition_preserves_legacy_clients_and_null_clears() {
        let missing: CopyDetails = serde_json::from_str(r#"{"release_id":1}"#).unwrap();
        let clear: CopyDetails =
            serde_json::from_str(r#"{"release_id":1,"selected_edition":null}"#).unwrap();
        let selected: CopyDetails =
            serde_json::from_str(r#"{"release_id":1,"selected_edition":"platinum"}"#).unwrap();
        assert_eq!(missing.selected_edition, None);
        assert_eq!(clear.selected_edition, Some(None));
        assert_eq!(selected.selected_edition, Some(Some("platinum".into())));
    }
}
