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
