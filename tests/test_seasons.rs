#[tokio::test]
async fn test_seasons_upsert_idempotent() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    let pool = pucksdata::db::get_pool().await.unwrap();
    let record = pucksdata::models::DbSeason {
        season_year: 19001901,
        start_date: None,
        end_date: None,
        regular_season_end_date: None,
    };
    pucksdata::loaders::seasons::upsert_seasons(pool, &[record]).await.unwrap();
    pucksdata::loaders::seasons::upsert_seasons(pool, &[pucksdata::models::DbSeason {
        season_year: 19001901,
        start_date: None,
        end_date: None,
        regular_season_end_date: None,
    }]).await.unwrap();
    let count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM seasons WHERE season_year = 19001901")
        .fetch_one(pool).await.unwrap().unwrap_or(0);
    assert_eq!(count, 1, "upsert produced more than one row");
    sqlx::query!("DELETE FROM seasons WHERE season_year = 19001901").execute(pool).await.unwrap();
}
