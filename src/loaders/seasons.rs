use crate::models::DbSeason;

pub async fn upsert_seasons(pool: &sqlx::PgPool, records: &[DbSeason]) -> Result<usize, sqlx::Error> {
    for record in records {
        sqlx::query!(
            r#"
            INSERT INTO seasons (season_year, start_date, end_date, regular_season_end_date)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (season_year) DO UPDATE SET
                start_date              = EXCLUDED.start_date,
                end_date                = EXCLUDED.end_date,
                regular_season_end_date = EXCLUDED.regular_season_end_date
            "#,
            record.season_year,
            record.start_date,
            record.end_date,
            record.regular_season_end_date,
        )
        .execute(pool)
        .await?;
    }
    Ok(records.len())
}
