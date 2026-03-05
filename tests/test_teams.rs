#[tokio::test]
async fn test_teams_upsert_idempotent() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    let pool = pucksdata::db::get_pool().await.unwrap();
    let record = pucksdata::models::DbTeam {
        team_id: 999999,
        full_name: "Test Team".into(),
        common_name: "Tests".into(),
        place_name: "Testville".into(),
        abbrev: "TST".into(),
    };
    // Insert twice
    pucksdata::loaders::teams::upsert_teams(pool, &[record]).await.unwrap();
    pucksdata::loaders::teams::upsert_teams(pool, &[pucksdata::models::DbTeam {
        team_id: 999999,
        full_name: "Test Team Updated".into(),
        common_name: "Tests".into(),
        place_name: "Testville".into(),
        abbrev: "TST".into(),
    }]).await.unwrap();
    // Verify exactly one row with team_id=999999
    let count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM teams WHERE team_id = 999999")
        .fetch_one(pool).await.unwrap().unwrap_or(0);
    assert_eq!(count, 1, "upsert produced more than one row");
    // Verify the update was applied (full_name updated)
    let name: String = sqlx::query_scalar!("SELECT full_name FROM teams WHERE team_id = 999999")
        .fetch_one(pool).await.unwrap();
    assert_eq!(name, "Test Team Updated");
    // Cleanup
    sqlx::query!("DELETE FROM teams WHERE team_id = 999999").execute(pool).await.unwrap();
}
