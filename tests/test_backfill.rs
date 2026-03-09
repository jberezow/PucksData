// tests/test_backfill.rs
// Integration tests for backfill orchestration: BACKFILL-01, BACKFILL-02, QUAL-02

/// Verify seeding is idempotent: running seed_backfill_progress twice for the same
/// scope produces the same rows (ON CONFLICT DO NOTHING — no duplicates, no errors).
#[tokio::test]
async fn test_backfill_progress_seed_idempotent() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    let pool = pucksdata::db::get_pool().await.unwrap();

    // Insert synthetic prerequisite rows
    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99901, 'Backfill Home', 'Home', 'Testville', 'BFH'),
                (99902, 'Backfill Away', 'Away', 'Testville', 'BFA')
         ON CONFLICT (team_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    sqlx::query!(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type)
         VALUES (9990000001, 99991, '2099-01-01', 99901, 99902, 2),
                (9990000002, 99991, '2099-01-02', 99901, 99902, 2)
         ON CONFLICT (game_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    // Seed once — should insert 2 rows
    pucksdata::process::backfill::seed_backfill_progress(pool, Some(99991)).await.unwrap();
    let count1: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM backfill_progress WHERE season = 99991"
    ).fetch_one(pool).await.unwrap().unwrap_or(0);
    assert_eq!(count1, 2, "first seed should insert 2 rows");

    // Seed again — ON CONFLICT DO NOTHING, still 2 rows
    pucksdata::process::backfill::seed_backfill_progress(pool, Some(99991)).await.unwrap();
    let count2: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM backfill_progress WHERE season = 99991"
    ).fetch_one(pool).await.unwrap().unwrap_or(0);
    assert_eq!(count2, 2, "second seed must not duplicate rows");

    // Cleanup
    sqlx::query!("DELETE FROM backfill_progress WHERE season = 99991").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM games WHERE game_id IN (9990000001, 9990000002)").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99901, 99902)").execute(pool).await.unwrap();
}

/// Verify resume semantics: 'done' games are excluded from query_pending_games;
/// 'pending' and 'failed' games are included.
#[tokio::test]
async fn test_backfill_resume_skips_done() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    let pool = pucksdata::db::get_pool().await.unwrap();

    // Insert prerequisite rows
    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99903, 'Resume Home', 'Home', 'Testville', 'RSH'),
                (99904, 'Resume Away', 'Away', 'Testville', 'RSA')
         ON CONFLICT (team_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    sqlx::query!(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type)
         VALUES (9990000003, 99992, '2099-01-03', 99903, 99904, 2),
                (9990000004, 99992, '2099-01-04', 99903, 99904, 2),
                (9990000005, 99992, '2099-01-05', 99903, 99904, 2)
         ON CONFLICT (game_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    // Seed all three as 'pending'
    pucksdata::process::backfill::seed_backfill_progress(pool, Some(99992)).await.unwrap();

    // Mark game 3 as 'done', game 4 as 'failed', game 5 stays 'pending'
    pucksdata::process::backfill::update_progress_status(pool, 9990000003, "done").await.unwrap();
    pucksdata::process::backfill::update_progress_status(pool, 9990000004, "failed").await.unwrap();

    // query_pending_games should return only games 4 and 5 (status != 'done')
    let pending = pucksdata::process::backfill::query_pending_games(pool, Some(99992)).await.unwrap();
    let pending_ids: Vec<i64> = pending.iter().map(|(id, _)| *id).collect();
    assert!(!pending_ids.contains(&9990000003), "done game must be excluded");
    assert!(pending_ids.contains(&9990000004), "failed game must be included for retry");
    assert!(pending_ids.contains(&9990000005), "pending game must be included");
    assert_eq!(pending_ids.len(), 2, "exactly 2 non-done games expected");

    // Cleanup
    sqlx::query!("DELETE FROM backfill_progress WHERE season = 99992").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM games WHERE game_id IN (9990000003, 9990000004, 9990000005)").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99903, 99904)").execute(pool).await.unwrap();
}

/// Verify update_progress_status transitions: status column updates correctly.
#[tokio::test]
async fn test_backfill_status_transitions() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    let pool = pucksdata::db::get_pool().await.unwrap();

    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99905, 'Status Home', 'Home', 'Testville', 'STH'),
                (99906, 'Status Away', 'Away', 'Testville', 'STA')
         ON CONFLICT (team_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    sqlx::query!(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type)
         VALUES (9990000006, 99993, '2099-01-06', 99905, 99906, 2)
         ON CONFLICT (game_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    pucksdata::process::backfill::seed_backfill_progress(pool, Some(99993)).await.unwrap();

    // Starts as 'pending'
    let status1: String = sqlx::query_scalar!(
        "SELECT status FROM backfill_progress WHERE game_id = 9990000006"
    ).fetch_one(pool).await.unwrap();
    assert_eq!(status1, "pending");

    // Transition to 'done'
    pucksdata::process::backfill::update_progress_status(pool, 9990000006, "done").await.unwrap();
    let status2: String = sqlx::query_scalar!(
        "SELECT status FROM backfill_progress WHERE game_id = 9990000006"
    ).fetch_one(pool).await.unwrap();
    assert_eq!(status2, "done");

    // Cleanup
    sqlx::query!("DELETE FROM backfill_progress WHERE season = 99993").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM games WHERE game_id = 9990000006").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99905, 99906)").execute(pool).await.unwrap();
}

/// Verify season filter: seeding with Some(season) only touches that season's games.
#[tokio::test]
async fn test_backfill_season_scope() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    let pool = pucksdata::db::get_pool().await.unwrap();

    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99907, 'Scope Home', 'Home', 'Testville', 'SCH'),
                (99908, 'Scope Away', 'Away', 'Testville', 'SCA')
         ON CONFLICT (team_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    // Two games in season 99994, one in 99995
    sqlx::query!(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type)
         VALUES (9990000007, 99994, '2099-01-07', 99907, 99908, 2),
                (9990000008, 99994, '2099-01-08', 99907, 99908, 2),
                (9990000009, 99995, '2099-01-09', 99907, 99908, 2)
         ON CONFLICT (game_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    // Seed only season 99994
    pucksdata::process::backfill::seed_backfill_progress(pool, Some(99994)).await.unwrap();

    let count_94: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM backfill_progress WHERE season = 99994"
    ).fetch_one(pool).await.unwrap().unwrap_or(0);
    assert_eq!(count_94, 2, "season 99994 should have 2 rows");

    let count_95: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM backfill_progress WHERE season = 99995"
    ).fetch_one(pool).await.unwrap().unwrap_or(0);
    assert_eq!(count_95, 0, "season 99995 should not be seeded");

    // Cleanup
    sqlx::query!("DELETE FROM backfill_progress WHERE season IN (99994, 99995)").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM games WHERE game_id IN (9990000007, 9990000008, 9990000009)").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99907, 99908)").execute(pool).await.unwrap();
}
