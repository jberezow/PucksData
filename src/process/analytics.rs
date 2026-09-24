//! Maintenance of the materialized objects derived from ingested data.

/// Refresh the player season rollup that backs the player season selector.
///
/// Refreshed concurrently: PucksStudio reads this view while a sync is
/// running, and a plain refresh would lock it out for the duration. The
/// concurrent form needs the unique index the migration creates, and needs the
/// view to be populated already, which it is from creation.
///
/// Against the full archive this takes roughly a minute. Call it only when
/// events have actually changed.
pub async fn refresh_player_event_seasons(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("REFRESH MATERIALIZED VIEW CONCURRENTLY analytics.player_event_seasons")
        .execute(pool)
        .await
        .map(|_| ())
}

/// Refresh season-level hits and blocks derived from the event archive.
pub async fn refresh_skater_physical_season_totals(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("REFRESH MATERIALIZED VIEW CONCURRENTLY analytics.skater_physical_season_totals")
        .execute(pool)
        .await
        .map(|_| ())
}

/// Refresh the materialized dataset health snapshot.
///
/// Computing it live costs tens of seconds, mostly reading the events index
/// and the goals-without-shots anti-join, which put the health page past the
/// reading role's statement timeout. The figures only move when ingestion
/// runs, so they are rebuilt here instead.
pub async fn refresh_season_health(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("REFRESH MATERIALIZED VIEW CONCURRENTLY observability.season_health")
        .execute(pool)
        .await
        .map(|_| ())
}

/// Attempt every refresh and persist each outcome; stale products are a partial
/// ingestion outcome even though their underlying source writes remain valid.
pub async fn refresh_derived(pool: &sqlx::PgPool) -> Result<(), crate::AnyError> {
    let mut failures = Vec::new();
    for label in [
        "analytics.player_event_seasons",
        "analytics.skater_physical_season_totals",
        "observability.season_health",
    ] {
        let result = super::attempts::track(pool, "derived", label, async {
            match label {
                "analytics.player_event_seasons" => refresh_player_event_seasons(pool).await?,
                "analytics.skater_physical_season_totals" => {
                    refresh_skater_physical_season_totals(pool).await?
                }
                _ => refresh_season_health(pool).await?,
            }
            Ok(())
        })
        .await;
        if let Err(error) = result {
            failures.push(format!("{label}: {error}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; ").into())
    }
}
