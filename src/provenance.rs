//! Source response capture scoped to an ingestion attempt. Read-only commands and
//! standalone parsers never connect to a database through the HTTP layer.
use std::future::Future;

#[derive(Clone)]
pub struct Context {
    pub pool: sqlx::PgPool,
    pub attempt_id: i64,
}

tokio::task_local! { static CONTEXT: Context; }

pub async fn scope<T>(context: Context, operation: impl Future<Output = T>) -> T {
    CONTEXT.scope(context, operation).await
}

/// Tokio task locals are not automatically inherited by spawned fetch workers.
pub fn inherit<T>(operation: impl Future<Output = T>) -> impl Future<Output = T> {
    let context = CONTEXT.try_with(Clone::clone).ok();
    async move {
        match context {
            Some(context) => scope(context, operation).await,
            None => operation.await,
        }
    }
}

pub async fn record_response(url: &str, body: &str) -> Result<(), sqlx::Error> {
    let Ok(context) = CONTEXT.try_with(Clone::clone) else {
        return Ok(());
    };
    let mut tx = context.pool.begin().await?;
    let hash: String = sqlx::query_scalar(
        "INSERT INTO history.source_documents(content_sha256, body)
         VALUES (encode(sha256(convert_to($1, 'UTF8')), 'hex'), $1)
         ON CONFLICT (content_sha256) DO NOTHING RETURNING content_sha256",
    )
    .bind(body)
    .fetch_optional(&mut *tx)
    .await?
    .unwrap_or_else(|| {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(body.as_bytes()))
    });
    sqlx::query("INSERT INTO ingestion.source_observations(attempt_id, url, content_sha256) VALUES ($1,$2,$3)")
        .bind(context.attempt_id).bind(url).bind(hash).execute(&mut *tx).await?;
    tx.commit().await
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
