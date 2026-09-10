use crate::{
    DBPool,
    admin::{AdminError, db},
};
use actix_web::{HttpResponse, web};
use diesel::{
    prelude::*,
    sql_types::{BigInt, Text},
};
use serde::{Deserialize, Serialize};

#[derive(QueryableByName, Serialize)]
struct Score {
    #[diesel(sql_type = Text)]
    user_login: String,
    #[diesel(sql_type = BigInt)]
    kudos: i64,
}
#[derive(Deserialize)]
struct ScoreQuery {
    login: String,
}
#[get("/kudos")]
async fn score(
    pool: web::Data<DBPool>,
    query: web::Query<ScoreQuery>,
) -> Result<HttpResponse, AdminError> {
    let login = query.into_inner().login;
    let score = db(pool, move |conn| diesel::sql_query("SELECT u.user_login,COALESCE(SUM(a.points),0)::bigint AS kudos FROM users u LEFT JOIN kudos_awards a ON a.user_id=u.id WHERE u.user_login=$1 GROUP BY u.id")
        .bind::<Text,_>(login).get_result::<Score>(conn).optional()?.ok_or(AdminError::Missing("Пользователь не найден"))).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(score))
}
#[get("/kudos/challenge")]
async fn challenge(pool: web::Data<DBPool>) -> Result<HttpResponse, AdminError> {
    let items = db(pool, |conn| Ok(diesel::sql_query("SELECT u.user_login,SUM(a.points)::bigint AS kudos FROM users u JOIN kudos_awards a ON a.user_id=u.id GROUP BY u.id HAVING SUM(a.points)>0 ORDER BY kudos DESC,lower(u.user_login),u.user_login,u.id LIMIT 100")
        .load::<Score>(conn)?)).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(items))
}
