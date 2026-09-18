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

/// Rebuild every derived object, reporting failure without failing the caller.
///
/// Stale rollups show out-of-date player seasons, physical totals, or dataset
/// health. Each is worth a warning, but none is a reason to fail a backfill or
/// sync whose events are already written. Every refresh is attempted even when
/// an earlier one fails.
pub async fn refresh_derived(pool: &sqlx::PgPool) {
    for (label, result) in [
        (
            "analytics.player_event_seasons",
            refresh_player_event_seasons(pool).await,
        ),
        (
            "analytics.skater_physical_season_totals",
            refresh_skater_physical_season_totals(pool).await,
        ),
        (
            "observability.season_health",
            refresh_season_health(pool).await,
        ),
    ] {
        match result {
            Ok(()) => println!("refreshed {label}"),
            Err(error) => eprintln!("warn: {label} refresh failed (non-fatal): {error}"),
        }
    }
}
