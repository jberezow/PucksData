// tests/integration_test.rs

#[tokio::test]
async fn pool_connects_and_queries() {
    if std::env::var("DATABASE_URL").is_err() {
        eprintln!("Skipping pool_connects_and_queries: DATABASE_URL not set");
        return;
    }
    let pool = common::test_pool().await;
    let row: (i32,) = sqlx::query_as("SELECT 1")
        .fetch_one(pool)
        .await
        .expect("SELECT 1 failed");
    assert_eq!(row.0, 1);
}
mod common;
