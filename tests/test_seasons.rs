#[tokio::test]
async fn test_seasons_upsert_idempotent() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let record = pucksdata::models::DbSeason {
        season_year: 19001901,
        start_date: None,
        end_date: None,
        regular_season_end_date: None,
    };
    pucksdata::loaders::seasons::upsert_seasons(pool, &[record], &indicatif::ProgressBar::hidden())
        .await
        .unwrap();
    pucksdata::loaders::seasons::upsert_seasons(
        pool,
        &[pucksdata::models::DbSeason {
            season_year: 19001901,
            start_date: None,
            end_date: None,
            regular_season_end_date: None,
        }],
        &indicatif::ProgressBar::hidden(),
    )
    .await
    .unwrap();
    let count: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM seasons WHERE season_year = 19001901")
            .fetch_one(pool)
            .await
            .unwrap()
            .unwrap_or(0);
    assert_eq!(count, 1, "upsert produced more than one row");
    sqlx::query!("DELETE FROM seasons WHERE season_year = 19001901")
        .execute(pool)
        .await
        .unwrap();
}
mod common;
