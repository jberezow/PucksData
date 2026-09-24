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
