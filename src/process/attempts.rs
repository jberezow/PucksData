//! Durable attempt outcomes; cancellation leaves a visible unfinished attempt.
use std::future::Future;

pub async fn start(pool: &sqlx::PgPool, dataset: &str, key: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO ingestion.attempts(dataset, entity_key, engine_version) VALUES ($1,$2,$3) RETURNING attempt_id",
    )
    .bind(dataset)
    .bind(key)
    .bind(env!("CARGO_PKG_VERSION"))
    .fetch_one(pool)
    .await
}

pub async fn finish(
    pool: &sqlx::PgPool,
    id: i64,
    outcome: &str,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE ingestion.attempts SET finished_at = clock_timestamp(), outcome = $2, error_message = $3 WHERE attempt_id = $1")
        .bind(id).bind(outcome).bind(error).execute(pool).await?;
    Ok(())
}

pub async fn track<T>(
    pool: &sqlx::PgPool,
    dataset: &str,
    key: &str,
    operation: impl Future<Output = Result<T, crate::AnyError>>,
) -> Result<T, crate::AnyError> {
    let id = start(pool, dataset, key).await?;
    tracing::info!(
        dataset,
        entity = key,
        attempt = id,
        "ingestion attempt started"
    );
    let result = crate::provenance::scope(
        crate::provenance::Context {
            pool: pool.clone(),
            attempt_id: id,
            metrics: Default::default(),
        },
        operation,
    )
    .await;
    let error = result.as_ref().err().map(ToString::to_string);
    finish(
        pool,
        id,
        if result.is_ok() {
            "complete"
        } else if result.as_ref().err().is_some_and(|error| {
            error.is::<crate::error::Deferred>()
                || error
                    .downcast_ref::<crate::fetchers::shifts::ShiftsUnavailable>()
                    .is_some()
                || matches!(
                    error.downcast_ref::<crate::api::ApiError>(),
                    Some(crate::api::ApiError::NotFound)
                )
        }) {
            "unavailable"
        } else {
            "failed"
        },
        error.as_deref(),
    )
    .await?;
    result
}

/// Run an ingestion command with exclusive writes, provenance, and current team identities.
pub async fn command<T>(
    pool: &sqlx::PgPool,
    key: &str,
    operation: impl Future<Output = Result<T, crate::AnyError>>,
) -> Result<T, crate::AnyError> {
    exclusive(
        pool,
        track(
            pool,
            "command",
            key,
            super::team_attribution::with_current_mapping(pool, operation),
        ),
    )
    .await
}

/// Hold a pooler-safe writer lease through fetch and commit, preventing an older
/// response from a concurrent command replacing a more recent observation.
pub async fn exclusive<T>(
    pool: &sqlx::PgPool,
    operation: impl Future<Output = Result<T, crate::AnyError>>,
) -> Result<T, crate::AnyError> {
    crate::db::require_connections(pool, 1, "ingestion")?;
    Lease::acquire(
        pool,
        LeaseKey::Hashed("pucksdata:ingestion"),
        "another ingestion command is running; retry after it finishes",
    )
    .await?
    .run(operation)
    .await
}

/// A transaction-scoped advisory lock, kept alive even while ingestion is idle.
pub struct Lease {
    transaction: sqlx::Transaction<'static, sqlx::Postgres>,
}

pub(crate) enum LeaseKey {
    Hashed(&'static str),
    Numeric(i64),
}

impl Lease {
    pub(crate) async fn acquire(
        pool: &sqlx::PgPool,
        key: LeaseKey,
        contention_message: &str,
    ) -> Result<Self, crate::AnyError> {
        let mut transaction = pool.begin().await?;
        sqlx::query("SET LOCAL idle_in_transaction_session_timeout = '60s'")
            .execute(&mut *transaction)
            .await?;
        let (numeric_key, hashed_key) = match key {
            LeaseKey::Hashed(key) => (None, Some(key)),
            LeaseKey::Numeric(key) => (Some(key), None),
        };
        let acquired: bool = sqlx::query_scalar(
            "SELECT pg_try_advisory_xact_lock(COALESCE($1::bigint, hashtextextended($2, 0)))",
        )
        .bind(numeric_key)
        .bind(hashed_key)
        .fetch_one(&mut *transaction)
        .await?;
        if !acquired {
            return Err(Box::new(crate::error::Deferred(
                contention_message.to_owned(),
            )));
        }
        Ok(Self { transaction })
    }

    /// Cancel the operation if its lease connection is lost or stops responding.
    pub async fn run<T>(
        mut self,
        operation: impl Future<Output = Result<T, crate::AnyError>>,
    ) -> Result<T, crate::AnyError> {
        tokio::pin!(operation);
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(10));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let result = loop {
            tokio::select! {
                biased;
                _ = heartbeat.tick() => {
                    tokio::time::timeout(
                        std::time::Duration::from_secs(10),
                        sqlx::query("SELECT 1").execute(&mut *self.transaction),
                    ).await??;
                }
                result = &mut operation => break result,
            }
        };
        self.transaction.rollback().await?;
        result
    }
}
