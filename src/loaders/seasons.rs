// TODO: Implemented in Task 2
use crate::models::DbSeason;

pub async fn upsert_seasons(_pool: &sqlx::PgPool, _records: &[DbSeason]) -> Result<usize, sqlx::Error> {
    Ok(0)
}
