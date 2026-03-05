// TODO: Implemented in Plan 03-02
use crate::models::DbPlayer;

pub async fn upsert_players(_pool: &sqlx::PgPool, _records: &[DbPlayer]) -> Result<usize, sqlx::Error> {
    Ok(0)
}
