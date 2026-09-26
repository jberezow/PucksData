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

/// Refresh only when a committed dependency change is pending, or a prior
/// refresh failed/interrupted. Acknowledge precisely the invalidations visible
/// before the refresh; concurrent or late-committing writes stay pending.
/// The future is never polled for a clean product.
pub async fn refresh_pending_product<F>(
    pool: &sqlx::PgPool,
    label: &str,
    refresh: F,
) -> Result<bool, crate::AnyError>
where
    F: std::future::Future<Output = Result<(), sqlx::Error>>,
{
    let invalidations: Vec<i64> = sqlx::query_scalar(
        "SELECT invalidation_id FROM ingestion.derived_invalidations WHERE product=$1",
    )
    .bind(label)
    .fetch_all(pool)
    .await?;
    let unfinished: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT outcome <> 'complete' FROM ingestion.attempts
         WHERE dataset='derived' AND entity_key=$1 ORDER BY attempt_id DESC LIMIT 1), false)",
    )
    .bind(label)
    .fetch_one(pool)
    .await?;
    if invalidations.is_empty() && !unfinished {
        println!("[derived] {label} unchanged; skipped");
        return Ok(false);
    }
    super::attempts::track(pool, "derived", label, async {
        refresh.await?;
        sqlx::query("DELETE FROM ingestion.derived_invalidations WHERE invalidation_id=ANY($1)")
            .bind(&invalidations)
            .execute(pool)
            .await?;
        Ok(())
    })
    .await?;
    Ok(true)
}

/// Attempt every dirty product and persist each outcome. Invalidations are
/// transactional with dependency writes, so a later zero-work run repairs
/// products left stale by interruption. All ingestion entry points use this.
pub async fn refresh_derived(pool: &sqlx::PgPool) -> Result<(), crate::AnyError> {
    let mut failures = Vec::new();
    for label in [
        "analytics.player_event_seasons",
        "analytics.skater_physical_season_totals",
        "observability.season_health",
    ] {
        let result = refresh_pending_product(pool, label, async {
            match label {
                "analytics.player_event_seasons" => refresh_player_event_seasons(pool).await,
                "analytics.skater_physical_season_totals" => {
                    refresh_skater_physical_season_totals(pool).await
                }
                _ => refresh_season_health(pool).await,
            }
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
