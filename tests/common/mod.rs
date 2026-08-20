use sqlx::{postgres::PgPoolOptions, PgPool};

/// Create a pool owned by the current test's Tokio runtime.
///
/// Production intentionally uses a process-wide pool. Integration tests use a
/// fresh runtime per `#[tokio::test]`, so sharing that production singleton
/// leaves later tests holding a pool whose runtime has already shut down.
pub async fn test_pool() -> &'static PgPool {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for database tests");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("test database connection failed");

    Box::leak(Box::new(pool))
}
