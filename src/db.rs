//! PostgreSQL connection pool initialization via [`get_pool`].
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;
use tokio::sync::OnceCell;

static POOL: OnceCell<PgPool> = OnceCell::const_new();

/// Create (or return the cached) PostgreSQL connection pool from `DATABASE_URL`.
pub async fn get_pool() -> Result<&'static PgPool, sqlx::Error> {
    POOL.get_or_try_init(|| async {
        dotenvy::dotenv().ok();
        let database_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set");
        let max_connections = std::env::var("DB_POOL_MAX_CONNECTIONS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(5);
        PgPoolOptions::new()
            .max_connections(max_connections)
            .idle_timeout(Duration::from_secs(240))   // recycle before Neon's ~300s compute suspend
            .max_lifetime(Duration::from_secs(1800))  // belt-and-suspenders 30-min recycle
            .connect(&database_url)
            .await
    })
    .await
}
