// TODO: Implemented in Plan 03-03
use crate::models::DbGame;

pub async fn upsert_games(_pool: &sqlx::PgPool, _records: &[DbGame]) -> Result<usize, sqlx::Error> {
    Ok(0)
}
