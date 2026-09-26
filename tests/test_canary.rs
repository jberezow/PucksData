mod common;

/// Exercise a small production-shaped path against the live NHL APIs and an
/// ephemeral PostgreSQL database.
///
/// This is ignored during normal test runs because it intentionally depends on
/// external services. The scheduled canary workflow runs it explicitly.
#[tokio::test]
#[ignore = "requires live NHL APIs and disposable PostgreSQL"]
async fn live_api_to_postgres_canary() {
    let pool = common::test_pool().await;

    // This production fetcher calls both api-web.nhle.com and api.nhle.com.
    let seasons = pucksdata::fetchers::seasons::fetch_seasons()
        .await
        .expect("NHL season endpoints should return a parseable response");

    assert!(
        seasons.len() >= 100,
        "NHL season catalog unexpectedly contained only {} records",
        seasons.len()
    );
    assert!(
        seasons.iter().all(|season| season.season_year > 19000000),
        "NHL season catalog contained an invalid season identifier"
    );
    assert!(
        seasons.iter().any(|season| season.start_date.is_some()),
        "NHL stats endpoint returned no usable season date metadata"
    );

    let expected = seasons.len() as i64;
    let progress = indicatif::ProgressBar::hidden();
    let written = pucksdata::loaders::seasons::upsert_seasons(pool, &seasons, &progress)
        .await
        .expect("season records should load into PostgreSQL");
    assert_eq!(written, seasons.len());

    let stored = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM seasons")
        .fetch_one(pool)
        .await
        .expect("loaded season records should be queryable");
    assert_eq!(stored, expected, "not every fetched season was persisted");
}

#[tokio::test]
#[ignore = "requires live NHL APIs and disposable PostgreSQL"]
async fn live_completed_game_captures_sources_and_normalized_snapshots() {
    let pool = common::test_pool().await;
    let game_id = 2023020345_i64;
    pucksdata::process::attempts::track(pool, "canary", "completed-game", async {
        let teams = pucksdata::fetchers::teams::fetch_teams().await?;
        pucksdata::loaders::teams::upsert_teams(pool, &teams, &indicatif::ProgressBar::hidden())
            .await?;
        let game = pucksdata::fetchers::games::fetch_single_game(game_id).await?;
        pucksdata::loaders::games::upsert_games(pool, &[game], &indicatif::ProgressBar::hidden())
            .await?;
        let mapping = pucksdata::fetchers::games::fetch_team_id_to_franchise_id_map().await?;
        let events = pucksdata::process::backfill::load_one_game(pool, game_id, &mapping).await?;
        assert!(events > 100);
        pucksdata::process::official_games::run_official_games(pool, Some(game_id), None, None)
            .await?;
        let shifts = pucksdata::fetchers::shifts::fetch_game_shifts(game_id).await?;
        pucksdata::loaders::shifts::replace_game_shifts(pool, game_id, &shifts).await?;
        Ok(())
    })
    .await
    .unwrap();
    let datasets: i64 = sqlx::query_scalar("SELECT count(DISTINCT dataset) FROM history.snapshots WHERE entity_key=$1 AND dataset IN ('events','official_games','shifts')")
        .bind(game_id.to_string()).fetch_one(pool).await.unwrap();
    assert_eq!(datasets, 3);
    let observations: i64 = sqlx::query_scalar("SELECT count(*) FROM ingestion.source_observations o JOIN ingestion.attempts a USING(attempt_id) WHERE (a.dataset IN ('events','official_games') AND a.entity_key=$1) OR (a.dataset='canary' AND a.entity_key='completed-game')")
        .bind(game_id.to_string()).fetch_one(pool).await.unwrap();
    assert!(observations >= 6);
}

/// Seed a synthetic previously-enriched cache from the live catalog, then verify
/// the production selector skips unchanged games and fetches a changed one.
#[tokio::test]
#[ignore = "requires live NHL APIs and disposable PostgreSQL"]
async fn incremental_schedule_skips_cached_games_and_enriches_a_catalog_change() {
    let pool = common::test_pool().await;
    let season = 20252026;
    let progress = indicatif::ProgressBar::hidden();
    let teams = pucksdata::fetchers::teams::fetch_teams().await.unwrap();
    pucksdata::loaders::teams::upsert_teams(pool, &teams, &progress)
        .await
        .unwrap();
    let map = pucksdata::fetchers::games::fetch_team_id_to_franchise_id_map()
        .await
        .unwrap();
    let catalog = pucksdata::fetchers::games::fetch_games_for_season(season)
        .await
        .unwrap();
    let mut games = Vec::new();
    for record in catalog
        .iter()
        .filter(|record| matches!(record.game_type, 1..=4))
    {
        match pucksdata::fetchers::games::transform_game(record, None, &map) {
            Ok(mut game) => {
                // Only a test fixture: all rows simulate a prior completed check.
                game.game_state = Some("OFF".into());
                games.push(game);
            }
            Err(_) if record.game_type == 1 => {}
            Err(error) => panic!("invalid catalog fixture: {error}"),
        }
    }
    assert!(games.len() > 1000);
    let changed = games
        .iter()
        .find(|game| game.game_type == 2 && game.home_score.is_some())
        .unwrap()
        .game_id;
    pucksdata::loaders::games::upsert_games(pool, &games, &progress)
        .await
        .unwrap();
    let ids: Vec<_> = games.iter().map(|game| game.game_id).collect();
    sqlx::query(
        "INSERT INTO ingestion.schedule_checks(game_id) SELECT unnest($1::bigint[])
        ON CONFLICT(game_id) DO UPDATE SET checked_at=clock_timestamp()",
    )
    .bind(&ids)
    .execute(pool)
    .await
    .unwrap();
    let today = time::OffsetDateTime::now_utc().date();
    let cutoff = today - time::Duration::days(3);
    let unchanged =
        pucksdata::fetchers::games::fetch_incremental_games(pool, season, cutoff, today)
            .await
            .unwrap();
    assert!(
        unchanged.is_empty(),
        "unchanged cached catalog should need no boxscores"
    );
    sqlx::query("UPDATE games SET home_score=99 WHERE game_id=$1")
        .bind(changed)
        .execute(pool)
        .await
        .unwrap();
    let refreshed =
        pucksdata::fetchers::games::fetch_incremental_games(pool, season, cutoff, today)
            .await
            .unwrap();
    assert_eq!(refreshed.len(), 1);
    assert_eq!(refreshed[0].game_id, changed);
    assert_ne!(refreshed[0].home_score, Some(99));
    assert!(
        refreshed[0].game_state.is_some(),
        "changed game must actually be enriched"
    );
}
