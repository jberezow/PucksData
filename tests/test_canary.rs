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
