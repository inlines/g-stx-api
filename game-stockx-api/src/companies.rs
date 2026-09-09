use crate::DBPool;
use actix_web::{HttpResponse, web};
use diesel::prelude::*;
use diesel::sql_types::{Array, Integer, Text};
use serde::Serialize;

#[derive(QueryableByName, Serialize)]
struct Company {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Text)]
    name: String,
    #[diesel(sql_type = Array<Integer>)]
    developer_platform_ids: Vec<i32>,
    #[diesel(sql_type = Array<Integer>)]
    publisher_platform_ids: Vec<i32>,
}

#[get("/companies/{id}")]
pub async fn get_company(pool: web::Data<DBPool>, id: web::Path<i32>) -> HttpResponse {
    let conn = &mut match pool.get() {
        Ok(conn) => conn,
        Err(error) => {
            log::error!("Company connection error: {}", error);
            return HttpResponse::InternalServerError().finish();
        }
    };
    let sql = r#"
        SELECT c.id, COALESCE(c.name, 'Компания #' || c.id) AS name,
            COALESCE(array_agg(DISTINCT pp.platform_id ORDER BY pp.platform_id)
                FILTER (WHERE platform.active = true AND ic.developer = true), ARRAY[]::integer[]) AS developer_platform_ids,
            COALESCE(array_agg(DISTINCT pp.platform_id ORDER BY pp.platform_id)
                FILTER (WHERE platform.active = true AND ic.publisher = true), ARRAY[]::integer[]) AS publisher_platform_ids
        FROM companies c
        LEFT JOIN involved_companies ic ON ic.company = c.id
        LEFT JOIN products p ON p.id = ic.game
            AND (p.game_type NOT IN (1, 2, 4, 13, 6, 5) OR p.game_type IS NULL)
        LEFT JOIN product_platforms pp ON pp.product_id = p.id
        LEFT JOIN platforms platform ON platform.id = pp.platform_id
        WHERE c.id = $1 GROUP BY c.id, c.name
    "#;
    match diesel::sql_query(sql)
        .bind::<Integer, _>(id.into_inner())
        .get_result::<Company>(conn)
        .optional()
    {
        Ok(Some(company)) => HttpResponse::Ok().json(company),
        Ok(None) => HttpResponse::NotFound().body("Company not found"),
        Err(error) => {
            log::error!("Company query error: {}", error);
            HttpResponse::InternalServerError().finish()
        }
    }
}
