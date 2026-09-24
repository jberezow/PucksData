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
    let result = crate::provenance::scope(
        crate::provenance::Context {
            pool: pool.clone(),
            attempt_id: id,
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
            error
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

/// Hold a pooler-safe writer lease through fetch and commit, preventing an older
/// response from a concurrent command replacing a more recent observation.
pub async fn exclusive<T>(
    pool: &sqlx::PgPool,
    operation: impl Future<Output = Result<T, crate::AnyError>>,
) -> Result<T, crate::AnyError> {
    if pool.options().get_max_connections() < 2 {
        return Err("ingestion requires at least two database connections".into());
    }
    let mut lease = pool.begin().await?;
    sqlx::query("SET LOCAL idle_in_transaction_session_timeout = '60s'")
        .execute(&mut *lease)
        .await?;
    let acquired: bool = sqlx::query_scalar(
        "SELECT pg_try_advisory_xact_lock(hashtextextended('pucksdata:ingestion', 0))",
    )
    .fetch_one(&mut *lease)
    .await?;
    if !acquired {
        return Err("another ingestion command is running; retry after it finishes".into());
    }
    tokio::pin!(operation);
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(10));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let result = loop {
        tokio::select! {
            result = &mut operation => break result,
            _ = heartbeat.tick() => { sqlx::query("SELECT 1").execute(&mut *lease).await?; }
        }
    };
    lease.rollback().await?;
    result
}
