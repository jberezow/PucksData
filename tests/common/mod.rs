use std::str::FromStr;

use sqlx::{postgres::PgConnectOptions, postgres::PgPoolOptions, PgPool};

#[allow(dead_code)]
pub fn test_database_configured() -> bool {
    std::env::var("TEST_DATABASE_URL").is_ok()
}

/// Create a pool owned by the current test's Tokio runtime.
///
/// Production intentionally uses a process-wide pool. Integration tests use a
/// fresh runtime per `#[tokio::test]`, so sharing that production singleton
/// leaves later tests holding a pool whose runtime has already shut down.
pub async fn test_pool() -> &'static PgPool {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set for database tests");
    let allow_unsafe = std::env::var("PUCKSDATA_ALLOW_UNSAFE_TEST_DATABASE").as_deref() == Ok("1");
    let options = PgConnectOptions::from_str(&database_url)
        .expect("TEST_DATABASE_URL must be a valid PostgreSQL connection URL");
    let database_name = options.get_database().unwrap_or_default();

    assert!(
        allow_unsafe || database_name.to_ascii_lowercase().contains("test"),
        "refusing to run database tests against database '{database_name}'; use a database whose \
         name contains 'test', or set PUCKSDATA_ALLOW_UNSAFE_TEST_DATABASE=1 explicitly"
    );
    if let Ok(application_url) = std::env::var("DATABASE_URL") {
        assert!(
            allow_unsafe || application_url != database_url,
            "refusing to run tests because TEST_DATABASE_URL matches DATABASE_URL"
        );
    }

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("test database connection failed");

    Box::leak(Box::new(pool))
}
