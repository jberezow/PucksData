mod common;
use pucksdata::process::sync;

#[test]
fn current_player_seasons_cover_september_rollover() {
    for (date, expected) in [
        ((2026, 8, 31), vec![20252026]),
        ((2026, 9, 1), vec![20252026, 20262027]),
        ((2026, 10, 1), vec![20262027]),
        ((2027, 1, 1), vec![20262027]),
    ] {
        assert_eq!(
            sync::active_seasons(chrono::NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap()),
            expected
        );
    }
}

#[tokio::test]
async fn historical_player_audits_are_bounded_rotate_and_retry_until_checkpointed() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    sqlx::query(
        "INSERT INTO teams(team_id,full_name,common_name,place_name,abbrev)
        VALUES(99781,'Audit H','Audit H','Test','AUH'),(99782,'Audit A','Audit A','Test','AUA')",
    )
    .execute(pool)
    .await
    .unwrap();
    for n in 0..7_i64 {
        sqlx::query(
            "INSERT INTO games(game_id,season,game_date,home_team_id,away_team_id,game_type)
            VALUES($1,$2,'2000-01-01',99781,99782,2)",
        )
        .bind(9978000000 + n)
        .bind(-700 + n as i32)
        .execute(pool)
        .await
        .unwrap();
    }
    // -700 is active, and therefore must never consume the historical budget.
    let expected = vec![-699, -698, -697, -696];
    assert_eq!(
        sync::player_audit_seasons(pool, &[-700]).await.unwrap(),
        expected
    );
    // Fetching (or crashing before accepted writes) does not advance the cursor.
    assert_eq!(
        sync::player_audit_seasons(pool, &[-700]).await.unwrap(),
        expected
    );
    sqlx::query("INSERT INTO ingestion.player_audits(season) SELECT unnest($1::integer[])")
        .bind(&expected)
        .execute(pool)
        .await
        .unwrap();
    let next = sync::player_audit_seasons(pool, &[-700]).await.unwrap();
    assert!(next.starts_with(&[-695, -694]));
    assert!(!next.iter().any(|season| expected.contains(season)));
    sqlx::query("UPDATE ingestion.player_audits SET completed_at=now()-interval '31 days' WHERE season=-699")
        .execute(pool).await.unwrap();
    // Exclude all other seasons to test the age threshold independently.
    let active: Vec<i32> =
        sqlx::query_scalar("SELECT DISTINCT season FROM games WHERE season <> -699")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(
        sync::player_audit_seasons(pool, &active).await.unwrap(),
        vec![-699]
    );
    sqlx::query("DELETE FROM ingestion.player_audits WHERE season BETWEEN -700 AND -694")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM games WHERE home_team_id=99781")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM teams WHERE team_id IN(99781,99782)")
        .execute(pool)
        .await
        .unwrap();
}

/// Refresh acknowledgements must not swallow a write committed during refresh,
/// or a transaction that started earlier but committed afterwards.
#[tokio::test]
async fn derived_refresh_retries_failure_and_preserves_concurrent_invalidations() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let label = "test.incremental_refresh";
    let invalidate = || {
        sqlx::query("SELECT ingestion.invalidate_products(ARRAY[$1])")
            .bind(label)
            .execute(pool)
    };
    invalidate().await.unwrap();
    let failed = pucksdata::process::analytics::refresh_pending_product(pool, label, async {
        Err(sqlx::Error::Protocol("injected refresh failure".into()))
    })
    .await;
    assert!(failed.is_err());
    let mut late = pool.begin().await.unwrap();
    sqlx::query("SELECT ingestion.invalidate_products(ARRAY[$1])")
        .bind(label)
        .execute(&mut *late)
        .await
        .unwrap();
    assert!(
        pucksdata::process::analytics::refresh_pending_product(pool, label, async {
            // This committed invalidation was not visible when refresh began.
            invalidate().await?;
            Ok(())
        })
        .await
        .unwrap()
    );
    late.commit().await.unwrap();
    let pending: i64 =
        sqlx::query_scalar("SELECT count(*) FROM ingestion.derived_invalidations WHERE product=$1")
            .bind(label)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(pending, 2);
    assert!(
        pucksdata::process::analytics::refresh_pending_product(pool, label, async { Ok(()) })
            .await
            .unwrap()
    );
    assert!(
        !pucksdata::process::analytics::refresh_pending_product(pool, label, async {
            panic!("clean product must not refresh")
        })
        .await
        .unwrap()
    );
    invalidate().await.unwrap();
    let (started, entered) = tokio::sync::oneshot::channel();
    let cancelled = tokio::spawn(async move {
        pucksdata::process::analytics::refresh_pending_product(pool, label, async {
            started.send(()).unwrap();
            std::future::pending::<Result<(), sqlx::Error>>().await
        })
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), entered)
        .await
        .unwrap()
        .unwrap();
    cancelled.abort();
    assert!(cancelled.await.unwrap_err().is_cancelled());
    assert!(
        pucksdata::process::analytics::refresh_pending_product(pool, label, async { Ok(()) })
            .await
            .unwrap()
    );
    // Interruption after acknowledgement but before attempt completion must retry.
    pucksdata::process::attempts::start(pool, "derived", label)
        .await
        .unwrap();
    assert!(
        pucksdata::process::analytics::refresh_pending_product(pool, label, async { Ok(()) })
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn invalidations_follow_committed_dependencies_and_ignore_unchanged_schedule() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let xid: i64 = sqlx::query_scalar("SELECT txid_current()::bigint")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO teams(team_id,full_name,common_name,place_name,abbrev)
        VALUES(99783,'Dirty H','Dirty H','Test','DIH'),(99784,'Dirty A','Dirty A','Test','DIA')",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO games(game_id,season,game_date,home_team_id,away_team_id,game_type,game_state)
        VALUES(9978300000,20252026,'2026-01-01',99783,99784,2,'OFF')",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("DELETE FROM ingestion.derived_invalidations WHERE source_transaction=$1")
        .bind(xid)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE games SET venue='Changed venue',game_state='OFF' WHERE game_id=9978300000")
        .execute(&mut *tx)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ingestion.derived_invalidations WHERE source_transaction=$1",
    )
    .bind(xid)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        count, 0,
        "venue-only and unchanged state do not affect derived products"
    );
    sqlx::query("UPDATE games SET game_state='FINAL' WHERE game_id=9978300000")
        .execute(&mut *tx)
        .await
        .unwrap();
    let labels: Vec<String> = sqlx::query_scalar("SELECT product FROM ingestion.derived_invalidations WHERE source_transaction=$1 ORDER BY product")
        .bind(xid).fetch_all(&mut *tx).await.unwrap();
    assert_eq!(labels, ["observability.season_health"]);
    sqlx::query("INSERT INTO events(game_id,event_id_in_game,period,period_type,time_in_period,event_type,season,game_type,game_date)
        VALUES(9978300000,1,1,'REG','00:01','goal',20252026,2,'2026-01-01')")
        .execute(&mut *tx).await.unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ingestion.derived_invalidations WHERE source_transaction=$1",
    )
    .bind(xid)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(count, 3);
    let event_id: i64 = sqlx::query_scalar("SELECT id FROM events WHERE game_id=9978300000")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    for (insert, delete, expected) in [
        (
            "INSERT INTO goals(event_id) VALUES($1)",
            "DELETE FROM goals WHERE event_id=$1",
            vec![
                "analytics.player_event_seasons",
                "observability.season_health",
            ],
        ),
        (
            "INSERT INTO shots(event_id) VALUES($1)",
            "DELETE FROM shots WHERE event_id=$1",
            vec![
                "analytics.player_event_seasons",
                "observability.season_health",
            ],
        ),
        (
            "INSERT INTO hits(event_id) VALUES($1)",
            "DELETE FROM hits WHERE event_id=$1",
            vec![
                "analytics.player_event_seasons",
                "analytics.skater_physical_season_totals",
            ],
        ),
        (
            "INSERT INTO blocks(event_id) VALUES($1)",
            "DELETE FROM blocks WHERE event_id=$1",
            vec![
                "analytics.player_event_seasons",
                "analytics.skater_physical_season_totals",
            ],
        ),
        (
            "INSERT INTO penalties(event_id) VALUES($1)",
            "DELETE FROM penalties WHERE event_id=$1",
            vec!["analytics.player_event_seasons"],
        ),
        (
            "INSERT INTO faceoffs(event_id) VALUES($1)",
            "DELETE FROM faceoffs WHERE event_id=$1",
            vec!["analytics.player_event_seasons"],
        ),
    ] {
        for statement in [insert, delete] {
            sqlx::query("DELETE FROM ingestion.derived_invalidations WHERE source_transaction=$1")
                .bind(xid)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query(statement)
                .bind(event_id)
                .execute(&mut *tx)
                .await
                .unwrap();
            let labels: Vec<String> = sqlx::query_scalar("SELECT product FROM ingestion.derived_invalidations WHERE source_transaction=$1 ORDER BY product")
                .bind(xid).fetch_all(&mut *tx).await.unwrap();
            assert_eq!(labels, expected, "dependency mismatch for {statement}");
        }
    }
    sqlx::query("DELETE FROM ingestion.derived_invalidations WHERE source_transaction=$1")
        .bind(xid)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("DELETE FROM goals WHERE event_id=$1")
        .bind(event_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ingestion.derived_invalidations WHERE source_transaction=$1",
    )
    .bind(xid)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(count, 0, "zero-row statements leave clean products alone");
    sqlx::query(
        "INSERT INTO ingestion.backfill_progress(game_id,season,status) VALUES(9978300000,20252026,'failed')",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    let labels: Vec<String> = sqlx::query_scalar("SELECT product FROM ingestion.derived_invalidations WHERE source_transaction=$1 ORDER BY product")
        .bind(xid).fetch_all(&mut *tx).await.unwrap();
    assert_eq!(labels, ["observability.season_health"]);
    tx.rollback().await.unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ingestion.derived_invalidations WHERE source_transaction=$1",
    )
    .bind(xid)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(count, 0, "rolled-back writes must not invalidate products");
}
