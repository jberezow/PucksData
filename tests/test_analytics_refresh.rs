mod common;

/// The rollup must be refreshable without locking out readers, which the
/// concurrent form only allows when a unique index exists on the view. A
/// migration that created the view without that index would still apply
/// cleanly and only fail later, in production, at refresh time.
#[tokio::test]
async fn test_player_event_seasons_refreshes_concurrently() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;

    pucksdata::process::analytics::refresh_player_event_seasons(pool)
        .await
        .expect("concurrent refresh must succeed");
    pucksdata::process::analytics::refresh_season_health(pool)
        .await
        .expect("concurrent refresh must succeed");
}

/// The health page reads these columns by name. Renaming or dropping one
/// breaks PucksStudio and the status command at runtime, not at build time.
#[tokio::test]
async fn test_dataset_health_exposes_its_contract() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;

    sqlx::query(
        "SELECT last_sync_at, last_sync_games, latest_completed_game_date,
                latest_event_game_date, completed_games, games_with_events,
                missing_event_games, goals_missing_shots, backfill_failed,
                backfill_pending, backfill_skipped, healthy,
                acknowledged_gap_games, actionable_gap_games, refreshed_at
         FROM observability.dataset_health",
    )
    .fetch_one(pool)
    .await
    .expect("dataset_health must expose every column its consumers select");
}

/// The materialized snapshot must agree with the live computation it caches.
/// A refresh has just run, so any difference is a defect in one definition.
#[tokio::test]
async fn test_materialized_health_matches_the_live_view() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;

    pucksdata::process::analytics::refresh_season_health(pool)
        .await
        .unwrap();

    let differences: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM (
             SELECT season, completed_games, games_with_events, goals_missing_shots,
                    acknowledged_gap_games, actionable_gap_games, healthy
             FROM observability.season_health
             EXCEPT
             SELECT season, completed_games, games_with_events, goals_missing_shots,
                    acknowledged_gap_games, actionable_gap_games, healthy
             FROM observability.season_health_live
         ) drifted",
    )
    .fetch_one(pool)
    .await
    .unwrap();

    assert_eq!(
        differences, 0,
        "materialized health drifted from the live view"
    );
}

/// The view answers the season selector, so it must be keyed the way that
/// lookup filters: by player.
#[tokio::test]
async fn test_player_event_seasons_is_keyed_by_player() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;

    let leading_column: Option<String> = sqlx::query_scalar(
        "SELECT a.attname
         FROM pg_index i
         JOIN pg_class c ON c.oid = i.indrelid
         JOIN pg_namespace n ON n.oid = c.relnamespace
         JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = i.indkey[0]
         WHERE n.nspname = 'analytics'
           AND c.relname = 'player_event_seasons'
           AND i.indisunique",
    )
    .fetch_optional(pool)
    .await
    .unwrap();

    assert_eq!(
        leading_column.as_deref(),
        Some("player_id"),
        "the unique index must lead with player_id or the season lookup cannot use it"
    );
}
