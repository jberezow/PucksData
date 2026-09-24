mod common;

use pucksdata::{process::attempts, provenance, AnyError};
use sqlx::PgPool;
use std::time::{SystemTime, UNIX_EPOCH};

fn entity_key(test: &str) -> String {
    format!(
        "{test}:{}:{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

async fn record(pool: &PgPool, key: &str, payload: &str) -> i64 {
    sqlx::query_scalar("SELECT history.record('history_test', $1, $2::jsonb)")
        .bind(key)
        .bind(payload)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn identical_payload_appends_observations_without_changing_revision() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let key = entity_key("identical");
    let first = record(pool, &key, r#"{"score":2,"final":true}"#).await;
    let second = record(pool, &key, r#"{"final":true,"score":2}"#).await;
    assert_eq!(first, second);
    let (versions, revision): (i64, i64) = sqlx::query_as(
        "SELECT count(*), max(revision) FROM history.snapshots WHERE dataset='history_test' AND entity_key=$1",
    )
    .bind(&key)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!((versions, revision), (1, 1));
    let observations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM history.observations WHERE snapshot_id=$1")
            .bind(first)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(observations, 2);
}

#[tokio::test]
async fn changed_payload_preserves_previous_state_at_its_recording_time() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let key = entity_key("as_of");
    let first = record(pool, &key, r#"{"score":2}"#).await;
    let second = record(pool, &key, r#"{"score":3}"#).await;
    assert_ne!(first, second);
    for (snapshot_id, expected_revision, expected_score) in [(first, 1, 2), (second, 2, 3)] {
        let state: (i64, i32) = sqlx::query_as(
            "SELECT revision, (payload->>'score')::integer
             FROM history.as_of('history_test', (SELECT recorded_at FROM history.snapshots WHERE snapshot_id=$1))
             WHERE entity_key=$2",
        )
        .bind(snapshot_id)
        .bind(&key)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(state, (expected_revision, expected_score));
    }
    let before_capture: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM history.as_of('history_test',
         (SELECT recorded_at - interval '1 microsecond' FROM history.snapshots WHERE snapshot_id=$1))
         WHERE entity_key=$2",
    )
    .bind(first)
    .bind(&key)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(before_capture, 0);
}

#[tokio::test]
async fn rollback_discards_both_revision_and_observation() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let key = entity_key("rollback");
    let mut tx = pool.begin().await.unwrap();
    let snapshot: i64 =
        sqlx::query_scalar("SELECT history.record('history_test', $1, '{\"score\":2}'::jsonb)")
            .bind(&key)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.rollback().await.unwrap();
    for statement in [
        "SELECT count(*) FROM history.snapshots WHERE snapshot_id=$1",
        "SELECT count(*) FROM history.observations WHERE snapshot_id=$1",
    ] {
        let count: i64 = sqlx::query_scalar(statement)
            .bind(snapshot)
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "rollback left rows: {statement}");
    }
    let accepted = record(pool, &key, r#"{"score":3}"#).await;
    let revision: i64 =
        sqlx::query_scalar("SELECT revision FROM history.snapshots WHERE snapshot_id=$1")
            .bind(accepted)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(revision, 1);
}

#[tokio::test]
async fn accepted_history_rejects_updates() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let snapshot = record(pool, &entity_key("immutable"), r#"{"score":2}"#).await;
    for statement in [
        "UPDATE history.snapshots SET payload='{}'::jsonb WHERE snapshot_id=$1",
        "UPDATE history.observations SET recorded_at=clock_timestamp() WHERE snapshot_id=$1",
    ] {
        let error = sqlx::query(statement)
            .bind(snapshot)
            .execute(pool)
            .await
            .expect_err("accepted history must be append-only");
        assert!(error.to_string().contains("history is append-only"));
    }
}

#[tokio::test]
async fn concurrent_identical_observations_share_one_revision() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let key = entity_key("concurrent");
    let (first, second) = tokio::join!(
        record(pool, &key, r#"{"score":2}"#),
        record(pool, &key, r#"{"score":2}"#)
    );
    assert_eq!(first, second);
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT count(DISTINCT s.snapshot_id), count(o.observation_id)
         FROM history.snapshots s JOIN history.observations o USING(snapshot_id)
         WHERE s.dataset='history_test' AND s.entity_key=$1",
    )
    .bind(&key)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(counts, (1, 2));
}

#[tokio::test]
async fn failed_validation_retains_raw_source_and_rejects_normalized_state() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let key = entity_key("failed_capture");
    let body = format!(r#"{{"key":"{key}","players":[]}}"#);
    let result: Result<(), AnyError> = attempts::track(pool, "history_test", &key, async {
        provenance::record_response("https://example.invalid/boxscore", &body).await?;
        let mut tx = pool.begin().await?;
        provenance::set_transaction(&mut tx).await?;
        sqlx::query("SELECT history.record('history_test', $1, '{}'::jsonb)")
            .bind(&key)
            .execute(&mut *tx)
            .await?;
        let linked_attempt: i64 = sqlx::query_scalar(
            "SELECT attempt_id FROM history.snapshots WHERE dataset='history_test' AND entity_key=$1",
        )
        .bind(&key)
        .fetch_one(&mut *tx)
        .await?;
        assert!(linked_attempt > 0);
        tx.rollback().await?;
        Err("incomplete player snapshot".into())
    })
    .await;
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("incomplete player snapshot"));
    let captured: (String, String, String, bool, String) = sqlx::query_as(
        "SELECT a.outcome, a.error_message, d.body, a.finished_at IS NOT NULL, a.engine_version
         FROM ingestion.attempts a
         JOIN ingestion.source_observations o USING(attempt_id)
         JOIN history.source_documents d USING(content_sha256)
         WHERE a.dataset='history_test' AND a.entity_key=$1",
    )
    .bind(&key)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(captured.0, "failed");
    assert_eq!(captured.1, "incomplete player snapshot");
    assert_eq!(captured.2, body);
    assert!(captured.3);
    assert_eq!(captured.4, env!("CARGO_PKG_VERSION"));
    let accepted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM history.snapshots WHERE dataset='history_test' AND entity_key=$1",
    )
    .bind(&key)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(accepted, 0);
}
