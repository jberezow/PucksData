//! Maintenance of derived analytics objects.

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

/// Refresh derived analytics, reporting failure without failing the caller.
///
/// A stale rollup shows a player an out-of-date season list, which is worth a
/// warning but is not a reason to fail a backfill or sync that has already
/// written its events.
pub async fn refresh_derived(pool: &sqlx::PgPool) {
    let started = std::time::Instant::now();
    match refresh_player_event_seasons(pool).await {
        Ok(()) => println!(
            "refreshed analytics.player_event_seasons in {:.1}s",
            started.elapsed().as_secs_f64()
        ),
        Err(error) => {
            eprintln!("warn: analytics.player_event_seasons refresh failed (non-fatal): {error}")
        }
    }
}
