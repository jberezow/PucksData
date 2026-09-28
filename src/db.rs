//! PostgreSQL connection pool initialization via [`get_pool`].
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;
use tokio::sync::OnceCell;

static POOL: OnceCell<PgPool> = OnceCell::const_new();

fn configuration_error(message: impl Into<String>) -> sqlx::Error {
    sqlx::Error::Configuration(message.into().into())
}

fn max_connections(value: Option<&str>) -> Result<u32, sqlx::Error> {
    match value {
        None => Ok(5),
        Some(value) => value
            .parse::<u32>()
            .ok()
            .filter(|count| *count > 0)
            .ok_or_else(|| {
                configuration_error("DB_POOL_MAX_CONNECTIONS must be a positive integer")
            }),
    }
}

/// Reserve connections for long-lived leases while leaving one for database work.
pub fn require_connections(
    pool: &PgPool,
    reserved_connections: u32,
    operation: &str,
) -> Result<(), crate::AnyError> {
    let minimum = reserved_connections.saturating_add(1);
    if pool.options().get_max_connections() < minimum {
        return Err(configuration_error(format!(
            "{operation} requires at least {minimum} database connections; increase DB_POOL_MAX_CONNECTIONS"
        )).into());
    }
    Ok(())
}

/// Create (or return the cached) PostgreSQL connection pool from `DATABASE_URL`.
pub async fn get_pool() -> Result<&'static PgPool, sqlx::Error> {
    POOL.get_or_try_init(|| async {
        dotenvy::dotenv().ok();
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| configuration_error("DATABASE_URL must be set"))?;
        if database_url.trim().is_empty() {
            return Err(configuration_error("DATABASE_URL must not be empty"));
        }
        let configured_max =
            std::env::var("DB_POOL_MAX_CONNECTIONS")
                .map(Some)
                .or_else(|error| match error {
                    std::env::VarError::NotPresent => Ok(None),
                    _ => Err(configuration_error(
                        "DB_POOL_MAX_CONNECTIONS must be valid Unicode",
                    )),
                })?;
        PgPoolOptions::new()
            .max_connections(max_connections(configured_max.as_deref())?)
            .idle_timeout(Duration::from_secs(240))
            .max_lifetime(Duration::from_secs(1800))
            .connect(&database_url)
            .await
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_size_rejects_invalid_configuration() {
        assert_eq!(max_connections(None).unwrap(), 5);
        assert_eq!(max_connections(Some("3")).unwrap(), 3);
        for value in ["0", "", "-1", "several", "4294967296"] {
            assert!(matches!(
                max_connections(Some(value)),
                Err(sqlx::Error::Configuration(_))
            ));
        }
    }

    #[tokio::test]
    async fn pool_capacity_accounts_for_reserved_connections() {
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect_lazy("postgres://localhost/pool_size_test")
            .unwrap();
        assert!(require_connections(&pool, 1, "ingestion").is_ok());
        assert!(require_connections(&pool, 2, "daemon").is_err());
    }
}
