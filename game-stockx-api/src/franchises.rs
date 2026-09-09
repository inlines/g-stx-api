use crate::DBPool;
use actix_web::{HttpResponse, web};
use diesel::prelude::*;
use diesel::sql_types::{Array, Integer, Text};
use serde::Serialize;

#[derive(QueryableByName, Serialize)]
struct Franchise {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Text)]
    name: String,
    #[diesel(sql_type = Array<Integer>)]
    platform_ids: Vec<i32>,
}

#[get("/franchises/{id}")]
pub async fn get_franchise(pool: web::Data<DBPool>, id: web::Path<i32>) -> HttpResponse {
    let conn = &mut match pool.get() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Franchise connection error: {}", error);
            return HttpResponse::InternalServerError().finish();
        }
    };
    let sql = r#"
        SELECT f.id, f.name, ARRAY(
            SELECT DISTINCT pp.platform_id
            FROM game_franschises gf
            JOIN products p ON p.id = gf.product_id
            JOIN product_platforms pp ON pp.product_id = p.id
            JOIN platforms platform ON platform.id = pp.platform_id
            WHERE gf.franschise_id = f.id AND platform.active = true
              AND (p.game_type NOT IN (1, 2, 4, 13, 6, 5) OR p.game_type IS NULL)
            ORDER BY pp.platform_id
        ) AS platform_ids
        FROM franschises f WHERE f.id = $1
    "#;
    match diesel::sql_query(sql)
        .bind::<Integer, _>(id.into_inner())
        .get_result::<Franchise>(conn)
        .optional()
    {
        Ok(Some(franchise)) => HttpResponse::Ok().json(franchise),
        Ok(None) => HttpResponse::NotFound().body("Franchise not found"),
        Err(error) => {
            log::error!("Franchise query error: {}", error);
            HttpResponse::InternalServerError().finish()
        }
    }
}
