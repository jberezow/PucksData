//! Source response capture scoped to an ingestion attempt. Read-only commands and
//! standalone parsers never connect to a database through the HTTP layer.
use std::future::Future;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Metrics {
    requests: AtomicU64,
    retries: AtomicU64,
    http_ms: AtomicU64,
    capture_ms: AtomicU64,
    pool_wait_ms: AtomicU64,
}

pub fn record_http(elapsed: Duration, retry: bool) {
    let _ = CONTEXT.try_with(|context| {
        context.metrics.requests.fetch_add(1, Ordering::Relaxed);
        context
            .metrics
            .retries
            .fetch_add(u64::from(retry), Ordering::Relaxed);
        context
            .metrics
            .http_ms
            .fetch_add(elapsed.as_millis() as u64, Ordering::Relaxed);
    });
}

#[derive(Clone)]
pub struct Context {
    pub pool: sqlx::PgPool,
    pub attempt_id: i64,
    pub metrics: Arc<Metrics>,
}

tokio::task_local! { static CONTEXT: Context; }

pub async fn scope<T>(context: Context, operation: impl Future<Output = T>) -> T {
    let metrics = context.metrics.clone();
    let attempt_id = context.attempt_id;
    let started = Instant::now();
    let result = CONTEXT.scope(context, operation).await;
    println!("[timing] attempt={attempt_id} elapsed_s={:.3} http_requests={} retries={} http_worker_ms={} capture_worker_ms={} capture_pool_wait_ms={}",
        started.elapsed().as_secs_f64(), metrics.requests.load(Ordering::Relaxed),
        metrics.retries.load(Ordering::Relaxed), metrics.http_ms.load(Ordering::Relaxed),
        metrics.capture_ms.load(Ordering::Relaxed), metrics.pool_wait_ms.load(Ordering::Relaxed));
    result
}

/// Tokio task locals are not automatically inherited by spawned fetch workers.
pub fn inherit<T>(operation: impl Future<Output = T>) -> impl Future<Output = T> {
    let context = CONTEXT.try_with(Clone::clone).ok();
    async move {
        match context {
            Some(context) => CONTEXT.scope(context, operation).await,
            None => operation.await,
        }
    }
}

pub async fn record_response(url: &str, body: &str) -> Result<(), sqlx::Error> {
    let Ok(context) = CONTEXT.try_with(Clone::clone) else {
        return Ok(());
    };
    use sha2::{Digest, Sha256};
    let hash = format!("{:x}", Sha256::digest(body.as_bytes()));
    let waiting = Instant::now();
    let mut connection = context.pool.acquire().await?;
    context
        .metrics
        .pool_wait_ms
        .fetch_add(waiting.elapsed().as_millis() as u64, Ordering::Relaxed);
    let started = Instant::now();
    // One atomic statement retains both the deduplicated document and every
    // observation. Capture still completes before parsing/accepting the body.
    let result = sqlx::query(
        "WITH document AS (
            INSERT INTO history.source_documents(content_sha256, body) VALUES ($1, $2)
            ON CONFLICT (content_sha256) DO NOTHING
         ) INSERT INTO ingestion.source_observations(attempt_id, url, content_sha256)
           VALUES ($3, $4, $1)",
    )
    .bind(hash)
    .bind(body)
    .bind(context.attempt_id)
    .bind(url)
    .execute(&mut *connection)
    .await
    .map(|_| ());
    context
        .metrics
        .capture_ms
        .fetch_add(started.elapsed().as_millis() as u64, Ordering::Relaxed);
    result
}

/// Attach accepted normalized versions to the same attempt as their source bodies.
pub async fn set_transaction(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<(), sqlx::Error> {
    if let Ok(id) = CONTEXT.try_with(|context| context.attempt_id) {
        sqlx::query("SELECT set_config('pucksdata.attempt_id', $1, true)")
            .bind(id.to_string())
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

pub async fn record_warnings(code: &str, warnings: &[String]) -> Result<(), sqlx::Error> {
    if warnings.is_empty() {
        return Ok(());
    }
    if let Ok(context) = CONTEXT.try_with(Clone::clone) {
        sqlx::query("INSERT INTO ingestion.diagnostics(attempt_id, code, occurrence_count, examples) VALUES ($1,$2,$3,$4)")
            .bind(context.attempt_id).bind(code).bind(warnings.len() as i64)
            .bind(&warnings[..warnings.len().min(10)]).execute(&context.pool).await?;
    }
    Ok(())
}
