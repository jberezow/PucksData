#[tokio::test]
async fn test_seasons_upsert_idempotent() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    // TODO: Plan 03-01 Task 2 fills in this test body
}
