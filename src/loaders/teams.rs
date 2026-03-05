use crate::models::DbTeam;

pub async fn upsert_teams(pool: &sqlx::PgPool, records: &[DbTeam]) -> Result<usize, sqlx::Error> {
    for record in records {
        sqlx::query!(
            r#"
            INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (team_id) DO UPDATE SET
                full_name   = EXCLUDED.full_name,
                common_name = EXCLUDED.common_name,
                place_name  = EXCLUDED.place_name,
                abbrev      = EXCLUDED.abbrev
            "#,
            record.team_id,
            record.full_name,
            record.common_name,
            record.place_name,
            record.abbrev,
        )
        .execute(pool)
        .await?;
    }
    Ok(records.len())
}
