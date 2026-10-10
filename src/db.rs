//! PostgreSQL connection pool initialization via [`get_pool`].
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::future::Future;
use std::io::ErrorKind;
use std::time::Duration;
use tokio::sync::OnceCell;

static POOL: OnceCell<PgPool> = OnceCell::const_new();

const STARTUP_RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];

fn transient_connection_error(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Io(error) => matches!(
            error.kind(),
            ErrorKind::UnexpectedEof
                | ErrorKind::ConnectionReset
                | ErrorKind::ConnectionAborted
                | ErrorKind::ConnectionRefused
                | ErrorKind::NotConnected
                | ErrorKind::BrokenPipe
                | ErrorKind::TimedOut
                | ErrorKind::Interrupted
        ),
        sqlx::Error::PoolTimedOut => true,
        sqlx::Error::Database(error) => matches!(
            error.code().as_deref(),
            Some("08000" | "08001" | "08003" | "08006" | "57P01" | "57P02" | "57P03" | "53300")
        ),
        _ => false,
    }
}

// Retry only pool initialization, before any ingestion work can run.
async fn connect_with_retry<T, F, Fut>(
    mut connect: F,
    delays: &[Duration],
) -> Result<T, sqlx::Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, sqlx::Error>>,
{
    for attempt in 0..=delays.len() {
        match connect().await {
            Ok(pool) => return Ok(pool),
            Err(error) => {
                if !transient_connection_error(&error) || attempt == delays.len() {
                    return Err(error);
                }
                // Do not log the connection string or server-provided error text.
                tracing::warn!(
                    attempt = attempt + 1,
                    max_attempts = delays.len() + 1,
                    retry_delay_ms = delays[attempt].as_millis() as u64,
                    "database startup connection failed transiently; retrying"
                );
                tokio::time::sleep(delays[attempt]).await;
            }
        }
    }
    unreachable!("the final connection attempt always returns")
}

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
        let options = PgPoolOptions::new()
            .max_connections(max_connections(configured_max.as_deref())?)
            .idle_timeout(Duration::from_secs(240))
            .max_lifetime(Duration::from_secs(1800));
        connect_with_retry(
            || async {
                tokio::time::timeout(
                    Duration::from_secs(15),
                    options.clone().connect(&database_url),
                )
                .await
                .map_err(|_| sqlx::Error::PoolTimedOut)?
            },
            &STARTUP_RETRY_DELAYS,
        )
        .await
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn startup_recovers_from_eof_before_returning_connection() {
        let mut calls = 0;
        let result = connect_with_retry(
            || {
                calls += 1;
                std::future::ready(if calls < 3 {
                    Err(sqlx::Error::Io(ErrorKind::UnexpectedEof.into()))
                } else {
                    Ok(42)
                })
            },
            &[Duration::ZERO; 3],
        )
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls, 3);
    }

    #[tokio::test]
    async fn startup_stops_after_four_transient_failures() {
        let mut calls = 0;
        let result: Result<(), _> = connect_with_retry(
            || {
                calls += 1;
                std::future::ready(Err(sqlx::Error::Io(ErrorKind::UnexpectedEof.into())))
            },
            &[Duration::ZERO; 3],
        )
        .await;
        assert!(matches!(result, Err(sqlx::Error::Io(e)) if e.kind() == ErrorKind::UnexpectedEof));
        assert_eq!(calls, 4);
    }

    #[tokio::test]
    async fn startup_does_not_retry_permanent_errors() {
        let mut errors = vec![
            configuration_error("invalid configuration"),
            sqlx::Error::Io(ErrorKind::PermissionDenied.into()),
            sqlx::Error::Protocol("invalid protocol".into()),
        ];
        while let Some(error) = errors.pop() {
            let mut error = Some(error);
            let result: Result<(), _> = connect_with_retry(
                || std::future::ready(Err(error.take().expect("must not retry"))),
                &[Duration::ZERO; 3],
            )
            .await;
            assert!(result.is_err());
        }
    }

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
