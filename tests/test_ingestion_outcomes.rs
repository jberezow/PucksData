mod common;

use pucksdata::process::{attempts, official_games, sync};
use pucksdata::AnyError;
use sqlx::PgPool;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;

type Watermark = (Option<OffsetDateTime>, Option<i32>, OffsetDateTime);

async fn watermark(pool: &PgPool) -> Option<Watermark> {
    sqlx::query_as(
        "SELECT last_sync_at, last_sync_games, updated_at FROM sync_state WHERE key='singleton'",
    )
    .fetch_optional(pool)
    .await
    .unwrap()
}

fn summary(processed: usize, failed: usize) -> Result<sync::SyncSummary, AnyError> {
    Ok(sync::SyncSummary {
        processed,
        failed,
        elapsed: Duration::ZERO,
        candidates: processed + failed,
        events_written: processed * 10,
    })
}

async fn assert_outcome(pool: &PgPool, attempt: i64, outcome: &str, error: Option<&str>) {
    let actual: (String, Option<String>, bool) = sqlx::query_as(
        "SELECT outcome, error_message, finished_at IS NOT NULL FROM ingestion.attempts WHERE attempt_id=$1",
    )
    .bind(attempt)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        actual,
        (outcome.to_string(), error.map(str::to_string), true)
    );
}

#[tokio::test]
async fn sync_watermark_and_correction_candidates_follow_attempt_outcomes() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let previous_watermark = watermark(pool).await;
    sqlx::query(
        "INSERT INTO sync_state(key,last_sync_at,last_sync_games,updated_at)
         VALUES ('singleton','2000-01-01',0,'2000-01-01')
         ON CONFLICT(key) DO UPDATE SET last_sync_at=EXCLUDED.last_sync_at,
         last_sync_games=EXCLUDED.last_sync_games, updated_at=EXCLUDED.updated_at",
    )
    .execute(pool)
    .await
    .unwrap();
    let before = watermark(pool).await.unwrap();
    let complete = attempts::start(pool, "sync", "singleton").await.unwrap();
    sync::record_sync_result(pool, complete, &summary(2, 0))
        .await
        .unwrap();
    assert_outcome(pool, complete, "complete", None).await;
    let successful = watermark(pool).await.unwrap();
    assert!(successful.0.unwrap() > before.0.unwrap());
    assert_eq!(successful.1, Some(2));

    let partial = attempts::start(pool, "sync", "singleton").await.unwrap();
    sync::record_sync_result(pool, partial, &summary(1, 1))
        .await
        .unwrap();
    assert_outcome(pool, partial, "partial", Some("1 game loads failed")).await;
    assert_eq!(watermark(pool).await.unwrap(), successful);

    let failed = attempts::start(pool, "sync", "singleton").await.unwrap();
    sync::record_sync_result(pool, failed, &Err("schedule unavailable".into()))
        .await
        .unwrap();
    assert_outcome(pool, failed, "failed", Some("schedule unavailable")).await;
    assert_eq!(watermark(pool).await.unwrap(), successful);

    let base = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as i64
        * 4;
    let home = base;
    let away = base + 1;
    let recent = base + 2;
    let old = base + 3;
    for (id, name) in [(home, "Home"), (away, "Away")] {
        sqlx::query(
            "INSERT INTO teams(team_id,full_name,common_name,place_name,abbrev)
             VALUES($1,$2,$2,'Testville',$3)",
        )
        .bind(id)
        .bind(name)
        .bind(format!("OUT{id}"))
        .execute(pool)
        .await
        .unwrap();
    }
    let today: time::Date =
        sqlx::query_scalar("SELECT (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')::date")
            .fetch_one(pool)
            .await
            .unwrap();
    let old_date = today - time::Duration::days(60);
    for (id, date) in [(recent, today), (old, old_date)] {
        sqlx::query(
            "INSERT INTO games(game_id,season,game_date,home_team_id,away_team_id,game_type,game_state)
             VALUES($1,20252026,$2,$3,$4,2,'OFF')",
        )
        .bind(id)
        .bind(date)
        .bind(home)
        .bind(away)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO events(game_id,event_id_in_game,period,period_type,time_in_period,
             event_type,season,game_type,game_date)
             VALUES($1,1,1,'REG','00:00','goal',20252026,2,$2)",
        )
        .bind(id)
        .bind(date)
        .execute(pool)
        .await
        .unwrap();
    }

    let audited = sync::query_sync_candidates_with_window(pool, None, 3)
        .await
        .unwrap();
    assert!(audited.iter().any(|(id, _)| *id == recent));
    assert!(!audited.iter().any(|(id, _)| *id == old));
    let explicit = sync::query_sync_candidates_with_window(pool, Some(old_date), 3)
        .await
        .unwrap();
    assert!(explicit.iter().any(|(id, _)| *id == old));
    let gaps = sync::query_sync_candidates(pool, None).await.unwrap();
    assert!(!gaps.iter().any(|(id, _)| *id == old));

    let audit_from = today - time::Duration::days(3);
    let candidates = official_games::query_sync_candidates(pool, audit_from)
        .await
        .unwrap();
    assert!(!candidates.iter().any(|(id, _)| *id == old));
    let key = old.to_string();
    let running = attempts::start(pool, "official_games", &key).await.unwrap();
    let candidates = official_games::query_sync_candidates(pool, audit_from)
        .await
        .unwrap();
    assert!(candidates.iter().any(|(id, _)| *id == old));
    attempts::finish(pool, running, "failed", Some("upstream unavailable"))
        .await
        .unwrap();
    let candidates = official_games::query_sync_candidates(pool, audit_from)
        .await
        .unwrap();
    assert!(candidates.iter().any(|(id, _)| *id == old));
    let retry = attempts::start(pool, "official_games", &key).await.unwrap();
    attempts::finish(pool, retry, "complete", None)
        .await
        .unwrap();
    let candidates = official_games::query_sync_candidates(pool, audit_from)
        .await
        .unwrap();
    assert!(!candidates.iter().any(|(id, _)| *id == old));

    sqlx::query("DELETE FROM events WHERE game_id IN ($1,$2)")
        .bind(recent)
        .bind(old)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM games WHERE game_id IN ($1,$2)")
        .bind(recent)
        .bind(old)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM teams WHERE team_id IN ($1,$2)")
        .bind(home)
        .bind(away)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM ingestion.attempts WHERE attempt_id IN ($1,$2,$3)")
        .bind(complete)
        .bind(partial)
        .bind(failed)
        .execute(pool)
        .await
        .unwrap();
    if let Some((last_sync_at, last_sync_games, updated_at)) = previous_watermark {
        sqlx::query("UPDATE sync_state SET last_sync_at=$1,last_sync_games=$2,updated_at=$3 WHERE key='singleton'")
            .bind(last_sync_at).bind(last_sync_games).bind(updated_at).execute(pool).await.unwrap();
    } else {
        sqlx::query("DELETE FROM sync_state WHERE key='singleton'")
            .execute(pool)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn writer_lease_rejects_overlap_and_releases_after_completion() {
    if !common::test_database_configured() {
        return;
    }
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use tokio::sync::Notify;

    let pool = common::test_pool().await;
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let first_entered = entered.clone();
    let first_release = release.clone();
    let first = tokio::spawn(async move {
        attempts::exclusive(pool, async move {
            first_entered.notify_one();
            first_release.notified().await;
            Ok(1)
        })
        .await
    });
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("first operation should acquire the writer lease");

    let second_ran = AtomicBool::new(false);
    let second = tokio::time::timeout(
        Duration::from_secs(5),
        attempts::exclusive(pool, async {
            second_ran.store(true, Ordering::SeqCst);
            Ok(2)
        }),
    )
    .await
    .expect("overlapping operation should fail without waiting for the first");
    assert!(second
        .unwrap_err()
        .to_string()
        .contains("another ingestion command is running"));
    assert!(!second_ran.load(Ordering::SeqCst));

    release.notify_one();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), first)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        1
    );
    let later = tokio::time::timeout(
        Duration::from_secs(5),
        attempts::exclusive(pool, async { Ok(3) }),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(later, 3);
}
