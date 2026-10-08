//! Upserts team records to the `teams` table.
use crate::models::DbTeam;

/// Refresh known source identities without removing historical mappings.
pub async fn upsert_team_identities(
    pool: &sqlx::PgPool,
    records: &[crate::fetchers::teams::TeamIdentity],
) -> Result<(), crate::AnyError> {
    let mut tx = pool.begin().await?;
    crate::provenance::set_transaction(&mut tx).await?;
    // Validate before any upsert: metadata refresh must never accept a reassignment
    // without reconciling games and event owners in the same operation.
    sqlx::query("LOCK TABLE nhl_team_identities IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    let stored: Vec<(i64, i64)> =
        sqlx::query_as("SELECT nhl_team_id, franchise_id FROM nhl_team_identities")
            .fetch_all(&mut *tx)
            .await?;
    crate::process::team_attribution::validate_mapping(&stored, records)?;
    let mapped: Vec<_> = records
        .iter()
        .filter(|row| row.franchise_id.is_some())
        .collect();
    let ids: Vec<_> = mapped.iter().map(|row| row.id).collect();
    let franchises: Vec<_> = mapped.iter().map(|row| row.franchise_id.unwrap()).collect();
    let abbrevs: Vec<_> = mapped.iter().map(|row| row.tri_code.clone()).collect();
    let names: Vec<_> = mapped.iter().map(|row| row.full_name.clone()).collect();
    sqlx::query(
        "INSERT INTO nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name)
         SELECT * FROM unnest($1::bigint[], $2::bigint[], $3::text[], $4::text[])
         ON CONFLICT (nhl_team_id) DO UPDATE SET
             franchise_id = EXCLUDED.franchise_id, abbrev = EXCLUDED.abbrev,
             full_name = EXCLUDED.full_name, observed_at = NOW()
         WHERE (nhl_team_identities.franchise_id, nhl_team_identities.abbrev, nhl_team_identities.full_name)
            IS DISTINCT FROM (EXCLUDED.franchise_id, EXCLUDED.abbrev, EXCLUDED.full_name)",
    )
    .bind(&ids)
    .bind(&franchises)
    .bind(&abbrevs)
    .bind(&names)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Upsert a batch of team records into the `teams` table.
pub async fn upsert_teams(
    pool: &sqlx::PgPool,
    records: &[DbTeam],
    pb: &indicatif::ProgressBar,
) -> Result<usize, sqlx::Error> {
    let mut tx = pool.begin().await?;
    crate::provenance::set_transaction(&mut tx).await?;
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
            WHERE (teams.full_name, teams.common_name, teams.place_name, teams.abbrev)
                IS DISTINCT FROM (EXCLUDED.full_name, EXCLUDED.common_name, EXCLUDED.place_name, EXCLUDED.abbrev)
            "#,
            record.team_id,
            record.full_name,
            record.common_name,
            record.place_name,
            record.abbrev,
        )
        .execute(&mut *tx)
        .await?;
        pb.suspend(|| tracing::info!("{}", record.abbrev));
        pb.inc(1);
    }
    tx.commit().await?;
    Ok(records.len())
}

/// Resolve branding against current franchises, never the historical identity table.
pub async fn upsert_team_branding(
    pool: &sqlx::PgPool,
    records: &[crate::fetchers::teams::TeamBranding],
) -> Result<(), crate::AnyError> {
    let mut tx = pool.begin().await?;
    crate::provenance::set_transaction(&mut tx).await?;
    for record in records {
        let team_id: i64 = sqlx::query_scalar("SELECT team_id FROM public.teams WHERE abbrev = $1")
            .bind(&record.abbrev)
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO public.team_branding AS current (team_id, logo_url, dark_logo_url)
             VALUES ($1, $2, $3)
             ON CONFLICT (team_id) DO UPDATE SET
                 logo_url = COALESCE(EXCLUDED.logo_url, current.logo_url),
                 dark_logo_url = COALESCE(EXCLUDED.dark_logo_url, current.dark_logo_url),
                 observed_at = NOW()
             WHERE (current.logo_url, current.dark_logo_url) IS DISTINCT FROM
                   (COALESCE(EXCLUDED.logo_url, current.logo_url), COALESCE(EXCLUDED.dark_logo_url, current.dark_logo_url))"
        ).bind(team_id).bind(&record.logo_url).bind(&record.dark_logo_url)
        .execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}
