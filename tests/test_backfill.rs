// tests/test_backfill.rs
// Integration tests for backfill orchestration: BACKFILL-01, BACKFILL-02, QUAL-02

mod common;

/// Verify seeding is idempotent: running seed_backfill_progress twice for the same
/// scope produces the same rows (ON CONFLICT DO NOTHING — no duplicates, no errors).
#[tokio::test]
async fn test_backfill_progress_seed_idempotent() {
    if std::env::var("DATABASE_URL").is_err() {
        return;
    }
    let pool = common::test_pool().await;

    // Insert synthetic prerequisite rows
    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99901, 'Backfill Home', 'Home', 'Testville', 'BFH'),
                (99902, 'Backfill Away', 'Away', 'Testville', 'BFA')
         ON CONFLICT (team_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type, game_state)
         VALUES (9990000001, 99991, '2099-01-01', 99901, 99902, 2, 'OFF'),
                (9990000002, 99991, '2099-01-02', 99901, 99902, 2, 'OFF')
         ON CONFLICT (game_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    // Seed once — should insert 2 rows
    pucksdata::process::backfill::seed_backfill_progress(pool, Some(99991))
        .await
        .unwrap();
    let count1: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM backfill_progress WHERE season = 99991")
            .fetch_one(pool)
            .await
            .unwrap()
            .unwrap_or(0);
    assert_eq!(count1, 2, "first seed should insert 2 rows");

    // Seed again — ON CONFLICT DO NOTHING, still 2 rows
    pucksdata::process::backfill::seed_backfill_progress(pool, Some(99991))
        .await
        .unwrap();
    let count2: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM backfill_progress WHERE season = 99991")
            .fetch_one(pool)
            .await
            .unwrap()
            .unwrap_or(0);
    assert_eq!(count2, 2, "second seed must not duplicate rows");

    // Cleanup
    sqlx::query!("DELETE FROM backfill_progress WHERE season = 99991")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM games WHERE game_id IN (9990000001, 9990000002)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99901, 99902)")
        .execute(pool)
        .await
        .unwrap();
}

/// Verify resume semantics: 'done' games are excluded from query_pending_games;
/// 'pending' and 'failed' games are included.
#[tokio::test]
async fn test_backfill_resume_skips_done() {
    if std::env::var("DATABASE_URL").is_err() {
        return;
    }
    let pool = common::test_pool().await;

    // Insert prerequisite rows
    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99903, 'Resume Home', 'Home', 'Testville', 'RSH'),
                (99904, 'Resume Away', 'Away', 'Testville', 'RSA')
         ON CONFLICT (team_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type, game_state)
         VALUES (9990000003, 99992, '2099-01-03', 99903, 99904, 2, 'OFF'),
                (9990000004, 99992, '2099-01-04', 99903, 99904, 2, 'OFF'),
                (9990000005, 99992, '2099-01-05', 99903, 99904, 2, 'OFF')
         ON CONFLICT (game_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    // Seed all three as 'pending'
    pucksdata::process::backfill::seed_backfill_progress(pool, Some(99992))
        .await
        .unwrap();

    // Mark game 3 as 'done', game 4 as 'failed', game 5 stays 'pending'
    pucksdata::process::backfill::update_progress_status(pool, 9990000003, "done")
        .await
        .unwrap();
    pucksdata::process::backfill::update_progress_status(pool, 9990000004, "failed")
        .await
        .unwrap();

    // query_pending_games should return only games 4 and 5 (status != 'done')
    let pending = pucksdata::process::backfill::query_pending_games(pool, Some(99992))
        .await
        .unwrap();
    let pending_ids: Vec<i64> = pending.iter().map(|g| g.game_id).collect();
    assert!(
        !pending_ids.contains(&9990000003),
        "done game must be excluded"
    );
    assert!(
        pending_ids.contains(&9990000004),
        "failed game must be included for retry"
    );
    assert!(
        pending_ids.contains(&9990000005),
        "pending game must be included"
    );
    assert_eq!(pending_ids.len(), 2, "exactly 2 non-done games expected");

    // Cleanup
    sqlx::query!("DELETE FROM backfill_progress WHERE season = 99992")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM games WHERE game_id IN (9990000003, 9990000004, 9990000005)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99903, 99904)")
        .execute(pool)
        .await
        .unwrap();
}

/// Verify update_progress_status transitions: status column updates correctly.
#[tokio::test]
async fn test_backfill_status_transitions() {
    if std::env::var("DATABASE_URL").is_err() {
        return;
    }
    let pool = common::test_pool().await;

    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99905, 'Status Home', 'Home', 'Testville', 'STH'),
                (99906, 'Status Away', 'Away', 'Testville', 'STA')
         ON CONFLICT (team_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type, game_state)
         VALUES (9990000006, 99993, '2099-01-06', 99905, 99906, 2, 'OFF')
         ON CONFLICT (game_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    pucksdata::process::backfill::seed_backfill_progress(pool, Some(99993))
        .await
        .unwrap();

    // Starts as 'pending'
    let status1: String =
        sqlx::query_scalar!("SELECT status FROM backfill_progress WHERE game_id = 9990000006")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(status1, "pending");

    // Transition to 'done'
    pucksdata::process::backfill::update_progress_status(pool, 9990000006, "done")
        .await
        .unwrap();
    let status2: String =
        sqlx::query_scalar!("SELECT status FROM backfill_progress WHERE game_id = 9990000006")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(status2, "done");

    // Cleanup
    sqlx::query!("DELETE FROM backfill_progress WHERE season = 99993")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM games WHERE game_id = 9990000006")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99905, 99906)")
        .execute(pool)
        .await
        .unwrap();
}

/// Verify query_pending_games returns enriched PendingGame structs with game_date,
/// home_abbrev, and away_abbrev populated via JOIN to games and teams tables.
#[tokio::test]
async fn test_query_pending_games_enriched() {
    if std::env::var("DATABASE_URL").is_err() {
        return;
    }
    let pool = common::test_pool().await;

    // Use distinct synthetic IDs to avoid conflicts with other tests
    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99910, 'Enrich Home', 'EnHome', 'Testville', 'ENH'),
                (99911, 'Enrich Away', 'EnAway', 'Testville', 'ENA')
         ON CONFLICT (team_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type, game_state)
         VALUES (9990000020, 99998, '2099-03-01', 99910, 99911, 2, 'OFF'),
                (9990000021, 99998, '2099-03-02', 99910, 99911, 2, 'OFF')
         ON CONFLICT (game_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    // Seed both as 'pending'
    pucksdata::process::backfill::seed_backfill_progress(pool, Some(99998))
        .await
        .unwrap();

    // Query enriched pending games
    let pending = pucksdata::process::backfill::query_pending_games(pool, Some(99998))
        .await
        .unwrap();

    assert_eq!(pending.len(), 2, "should return exactly 2 non-done games");

    // Results are ordered by game_id ASC
    let first = &pending[0];
    assert_eq!(first.game_id, 9990000020);
    assert_eq!(first.season, 99998);
    assert_eq!(
        first.home_abbrev, "ENH",
        "home_abbrev must match inserted team"
    );
    assert_eq!(
        first.away_abbrev, "ENA",
        "away_abbrev must match inserted team"
    );
    assert!(
        first.game_date.year() > 0,
        "game_date year must be positive"
    );
    assert_eq!(first.game_date.year(), 2099, "game_date year must be 2099");

    let second = &pending[1];
    assert_eq!(second.game_id, 9990000021);
    assert_eq!(second.home_abbrev, "ENH");
    assert_eq!(second.away_abbrev, "ENA");

    // Cleanup
    sqlx::query!("DELETE FROM backfill_progress WHERE season = 99998")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM games WHERE game_id IN (9990000020, 9990000021)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99910, 99911)")
        .execute(pool)
        .await
        .unwrap();
}

/// RES-01: Verify that a failed game's error message is persisted in backfill_progress.
/// update_progress_with_error stores status and error_message atomically.
#[tokio::test]
async fn test_failed_game_records_error_message() {
    if std::env::var("DATABASE_URL").is_err() {
        return;
    }
    let pool = common::test_pool().await;

    // Synthetic teams and game
    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99920, 'Error Home', 'EHome', 'Testville', 'ERH'),
                (99921, 'Error Away', 'EAway', 'Testville', 'ERA')
         ON CONFLICT (team_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type, game_state)
         VALUES (9990000030, 99996, '2099-04-01', 99920, 99921, 2, 'OFF')
         ON CONFLICT (game_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    pucksdata::process::backfill::seed_backfill_progress(pool, Some(99996))
        .await
        .unwrap();

    // Mark as failed with error message
    pucksdata::process::backfill::update_progress_with_error(
        pool,
        9990000030,
        "failed",
        "HTTP error: 500",
    )
    .await
    .unwrap();

    // Verify error_message and status are persisted
    let row = sqlx::query!(
        "SELECT status, error_message FROM backfill_progress WHERE game_id = 9990000030"
    )
    .fetch_one(pool)
    .await
    .unwrap();

    assert_eq!(row.status, "failed", "status must be 'failed'");
    assert_eq!(
        row.error_message.as_deref(),
        Some("HTTP error: 500"),
        "error_message must be 'HTTP error: 500'"
    );

    // Cleanup
    sqlx::query!("DELETE FROM backfill_progress WHERE season = 99996")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM games WHERE game_id = 9990000030")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99920, 99921)")
        .execute(pool)
        .await
        .unwrap();
}

/// RES-03 (unit): Verify is_api_gap_error classification — pure unit test, no DB needed.
/// ApiError::NotFound → true; ApiError::Other(500) → false; io::Error → false.
#[test]
fn test_is_api_gap_error_unit() {
    // ApiError::NotFound wrapped in AnyError → true
    let not_found: pucksdata::AnyError = Box::new(pucksdata::api::ApiError::NotFound);
    assert!(
        pucksdata::process::backfill::is_api_gap_error(&not_found),
        "ApiError::NotFound must classify as api gap error"
    );

    // ApiError::Other(500) wrapped in AnyError → false
    let server_err: pucksdata::AnyError = Box::new(pucksdata::api::ApiError::Other(500));
    assert!(
        !pucksdata::process::backfill::is_api_gap_error(&server_err),
        "ApiError::Other(500) must not classify as api gap error"
    );

    // Plain io::Error wrapped in AnyError → false
    let io_err: pucksdata::AnyError = Box::new(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "file not found",
    ));
    assert!(
        !pucksdata::process::backfill::is_api_gap_error(&io_err),
        "io::Error must not classify as api gap error"
    );
}

/// RES-03 (integration): Verify query_pending_games excludes 'skipped' games but includes 'failed'.
/// Skipped games are terminal (not retried); failed games are still pending retry.
#[tokio::test]
async fn test_skipped_game_excluded_from_pending() {
    if std::env::var("DATABASE_URL").is_err() {
        return;
    }
    let pool = common::test_pool().await;

    // Synthetic teams and games
    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99922, 'Skip Home', 'SHome', 'Testville', 'SKH'),
                (99923, 'Skip Away', 'SAway', 'Testville', 'SKA')
         ON CONFLICT (team_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type, game_state)
         VALUES (9990000031, 99997, '2099-05-01', 99922, 99923, 2, 'OFF'),
                (9990000032, 99997, '2099-05-02', 99922, 99923, 2, 'OFF')
         ON CONFLICT (game_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    pucksdata::process::backfill::seed_backfill_progress(pool, Some(99997))
        .await
        .unwrap();

    // Mark game 31 as 'skipped' (terminal — not retried)
    pucksdata::process::backfill::update_progress_status(pool, 9990000031, "skipped")
        .await
        .unwrap();
    // Mark game 32 as 'failed' (still retried)
    pucksdata::process::backfill::update_progress_status(pool, 9990000032, "failed")
        .await
        .unwrap();

    let pending = pucksdata::process::backfill::query_pending_games(pool, Some(99997))
        .await
        .unwrap();
    let pending_ids: Vec<i64> = pending.iter().map(|g| g.game_id).collect();

    assert!(
        !pending_ids.contains(&9990000031),
        "skipped game must be excluded from pending"
    );
    assert!(
        pending_ids.contains(&9990000032),
        "failed game must be included for retry"
    );
    assert_eq!(pending_ids.len(), 1, "exactly 1 non-terminal game expected");

    // Cleanup
    sqlx::query!("DELETE FROM backfill_progress WHERE season = 99997")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM games WHERE game_id IN (9990000031, 9990000032)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99922, 99923)")
        .execute(pool)
        .await
        .unwrap();
}

/// RES-04: Verify checkpoint/resume guarantee after simulated kill + restart.
/// Done and skipped games survive re-seed unchanged (ON CONFLICT DO NOTHING).
/// Failed and pending games are included in the next run's work list.
#[tokio::test]
async fn test_checkpoint_kill_resume() {
    if std::env::var("DATABASE_URL").is_err() {
        return;
    }
    let pool = common::test_pool().await;

    // Synthetic teams
    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99930, 'Kill Home', 'KHome', 'Testville', 'KLH'),
                (99931, 'Kill Away', 'KAway', 'Testville', 'KLA')
         ON CONFLICT (team_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    // 4 synthetic games in season 99999
    sqlx::query(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type, game_state)
         VALUES (9990000040, 99999, '2099-06-01', 99930, 99931, 2, 'OFF'),
                (9990000041, 99999, '2099-06-02', 99930, 99931, 2, 'OFF'),
                (9990000042, 99999, '2099-06-03', 99930, 99931, 2, 'OFF'),
                (9990000043, 99999, '2099-06-04', 99930, 99931, 2, 'OFF')
         ON CONFLICT (game_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    // Step 1: Initial seed — all 4 games enter as 'pending'
    pucksdata::process::backfill::seed_backfill_progress(pool, Some(99999))
        .await
        .unwrap();

    // Step 2: Simulate partial run completing before kill
    pucksdata::process::backfill::update_progress_status(pool, 9990000040, "done")
        .await
        .unwrap();
    pucksdata::process::backfill::update_progress_status(pool, 9990000041, "skipped")
        .await
        .unwrap();
    pucksdata::process::backfill::update_progress_status(pool, 9990000042, "failed")
        .await
        .unwrap();
    // 9990000043 stays 'pending' (was in-flight when killed)

    // Step 3: Simulate restart — re-seed (ON CONFLICT DO NOTHING preserves done/skipped)
    pucksdata::process::backfill::seed_backfill_progress(pool, Some(99999))
        .await
        .unwrap();

    // Step 4: Query work list for resumed run
    let pending = pucksdata::process::backfill::query_pending_games(pool, Some(99999))
        .await
        .unwrap();
    let pending_ids: Vec<i64> = pending.iter().map(|g| g.game_id).collect();

    // Done game must not be re-queued
    assert!(
        !pending_ids.contains(&9990000040),
        "done game must be excluded after restart"
    );
    // Skipped game (terminal) must not be re-queued
    assert!(
        !pending_ids.contains(&9990000041),
        "skipped game must be excluded after restart"
    );
    // Failed game must be retried
    assert!(
        pending_ids.contains(&9990000042),
        "failed game must be included for retry"
    );
    // Pending game (was in-flight) must be included
    assert!(
        pending_ids.contains(&9990000043),
        "pending game must be included after restart"
    );
    // Only 2 games in work list
    assert_eq!(
        pending_ids.len(),
        2,
        "exactly 2 games should be pending after checkpoint resume"
    );

    // Cleanup
    sqlx::query!("DELETE FROM backfill_progress WHERE season = 99999")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query!(
        "DELETE FROM games WHERE game_id IN (9990000040, 9990000041, 9990000042, 9990000043)"
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99930, 99931)")
        .execute(pool)
        .await
        .unwrap();
}

/// Verify season filter: seeding with Some(season) only touches that season's games.
#[tokio::test]
async fn test_backfill_season_scope() {
    if std::env::var("DATABASE_URL").is_err() {
        return;
    }
    let pool = common::test_pool().await;

    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99907, 'Scope Home', 'Home', 'Testville', 'SCH'),
                (99908, 'Scope Away', 'Away', 'Testville', 'SCA')
         ON CONFLICT (team_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    // Two games in season 99994, one in 99995
    sqlx::query(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type, game_state)
         VALUES (9990000007, 99994, '2099-01-07', 99907, 99908, 2, 'OFF'),
                (9990000008, 99994, '2099-01-08', 99907, 99908, 2, 'OFF'),
                (9990000009, 99995, '2099-01-09', 99907, 99908, 2, 'OFF')
         ON CONFLICT (game_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    // Seed only season 99994
    pucksdata::process::backfill::seed_backfill_progress(pool, Some(99994))
        .await
        .unwrap();

    let count_94: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM backfill_progress WHERE season = 99994")
            .fetch_one(pool)
            .await
            .unwrap()
            .unwrap_or(0);
    assert_eq!(count_94, 2, "season 99994 should have 2 rows");

    let count_95: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM backfill_progress WHERE season = 99995")
            .fetch_one(pool)
            .await
            .unwrap()
            .unwrap_or(0);
    assert_eq!(count_95, 0, "season 99995 should not be seeded");

    // Cleanup
    sqlx::query!("DELETE FROM backfill_progress WHERE season IN (99994, 99995)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM games WHERE game_id IN (9990000007, 9990000008, 9990000009)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99907, 99908)")
        .execute(pool)
        .await
        .unwrap();
}
