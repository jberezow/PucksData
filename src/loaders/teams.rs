//! Upserts team records to the `teams` table.
use crate::models::DbTeam;

/// Refresh known source identities without removing historical mappings.
pub async fn upsert_team_identities(
    pool: &sqlx::PgPool,
    records: &[crate::fetchers::teams::TeamIdentity],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    for row in records {
        let Some(franchise_id) = row.franchise_id else {
            continue;
        };
        sqlx::query(
            "INSERT INTO nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (nhl_team_id) DO UPDATE SET
                 franchise_id = EXCLUDED.franchise_id, abbrev = EXCLUDED.abbrev,
                 full_name = EXCLUDED.full_name, observed_at = NOW()",
        )
        .bind(row.id)
        .bind(franchise_id)
        .bind(&row.tri_code)
        .bind(&row.full_name)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// Upsert a batch of team records into the `teams` table.
pub async fn upsert_teams(
    pool: &sqlx::PgPool,
    records: &[DbTeam],
    pb: &indicatif::ProgressBar,
) -> Result<usize, sqlx::Error> {
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
        pb.suspend(|| println!("{}", record.abbrev));
        pb.inc(1);
    }
    Ok(records.len())
}
