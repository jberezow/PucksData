// TODO: Implemented in Task 2
use crate::models::DbTeam;

pub async fn upsert_teams(_pool: &sqlx::PgPool, _records: &[DbTeam]) -> Result<usize, sqlx::Error> {
    Ok(0)
}
