use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::env;

pub type DbPool = Pool<Postgres>;

pub async fn create_pool() -> Result<DbPool, sqlx::Error> {
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
}

pub async fn insert_raw_data(
    pool: &DbPool,
    endpoint_name: &str,
    params: &serde_json::Value,
    data: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO raw_data (endpoint, parameters, data)
        VALUES ($1, $2, $3)
        "#
    )
    .bind(endpoint_name)
    .bind(params)
    .bind(data)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn raw_data_exists(
    pool: &DbPool,
    endpoint_name: &str,
    params: &serde_json::Value,
) -> Result<bool, sqlx::Error> {
    let count: (i64,) = sqlx::query_as(
        r#"
        SELECT count(*) FROM raw_data
        WHERE endpoint = $1 AND parameters = $2
        "#,
    )
    .bind(endpoint_name)
    .bind(params)
    .fetch_one(pool)
    .await?;

    Ok(count.0 > 0)
} 