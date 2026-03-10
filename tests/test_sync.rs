// tests/test_sync.rs
// Integration tests for Phase 6: Sync Command Core
// Tests: SYNC-01 (gap detection), SYNC-02 (idempotency), SYNC-04 (--from filter), QUAL-SYNC-01
//
// Synthetic ID ranges (no collision with test_backfill.rs 99901-99908 / 9990000001-9990000009):
//   test_query_sync_candidates_detects_gap:         teams 99911-99912, game 9991000001
//   test_query_sync_candidates_from_date_filter:    teams 99913-99914, games 9991000002-9991000003
//   test_query_sync_candidates_includes_null_state: teams 99915-99916, game 9991000004

/// Unit test: is_game_completed() returns true for known completed states only.
/// Does NOT require DATABASE_URL — pure logic test.
#[test]
fn test_is_game_completed_unit() {
    assert!(pucksdata::process::sync::is_game_completed("OFF"),   "OFF should be completed");
    assert!(pucksdata::process::sync::is_game_completed("OVER"),  "OVER should be completed");
    assert!(pucksdata::process::sync::is_game_completed("FINAL"), "FINAL should be completed");
    assert!(!pucksdata::process::sync::is_game_completed("LIVE"), "LIVE must not be completed");
    assert!(!pucksdata::process::sync::is_game_completed("PPD"),  "PPD must not be completed");
    assert!(!pucksdata::process::sync::is_game_completed("FUT"),  "FUT must not be completed");
    assert!(!pucksdata::process::sync::is_game_completed(""),     "empty string must not be completed");
}

/// SYNC-01 / SYNC-02: A completed game with no events appears in query_sync_candidates.
/// After adding a fake events row, the game disappears (idempotency by construction).
#[tokio::test]
async fn test_query_sync_candidates_detects_gap() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    let pool = pucksdata::db::get_pool().await.unwrap();

    // Insert synthetic prerequisite teams
    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99911, 'Sync Home', 'SyncH', 'Testville', 'SNH'),
                (99912, 'Sync Away', 'SyncA', 'Testville', 'SNA')
         ON CONFLICT (team_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    // Insert a completed game in the past with no events (game_state = 'OFF')
    sqlx::query!(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type, game_state)
         VALUES (9991000001, 99991, '2020-01-01', 99911, 99912, 2, 'OFF')
         ON CONFLICT (game_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    // First call: game has no events — should appear in candidates
    let candidates = pucksdata::process::sync::query_sync_candidates(pool, None)
        .await
        .unwrap();
    let ids: Vec<i64> = candidates.iter().map(|(id, _)| *id).collect();
    assert!(ids.contains(&9991000001), "game with no events must appear in gap detection");

    // Insert a fake events row to simulate the game already being processed
    sqlx::query!(
        "INSERT INTO events (game_id, event_id_in_game, period, period_type, time_in_period, event_type)
         VALUES (9991000001, 1, 1, 'REG', '00:00', 'goal')
         ON CONFLICT (game_id, event_id_in_game) DO NOTHING"
    ).execute(pool).await.unwrap();

    // Second call: game now has an events row — must disappear from candidates (idempotency)
    let candidates2 = pucksdata::process::sync::query_sync_candidates(pool, None)
        .await
        .unwrap();
    let ids2: Vec<i64> = candidates2.iter().map(|(id, _)| *id).collect();
    assert!(!ids2.contains(&9991000001), "game with events must not appear in gap detection (idempotent)");

    // Cleanup (events first — FK constraint)
    sqlx::query!("DELETE FROM events WHERE game_id = 9991000001").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM games WHERE game_id = 9991000001").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99911, 99912)").execute(pool).await.unwrap();
}

/// SYNC-04: --from DATE filter — query_sync_candidates with from_date only returns games on/after that date.
#[tokio::test]
async fn test_query_sync_candidates_from_date_filter() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    let pool = pucksdata::db::get_pool().await.unwrap();

    // Insert synthetic teams (unique range: 99913-99914 for this test)
    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99913, 'Sync Home 2', 'SyncH2', 'Testville', 'SH2'),
                (99914, 'Sync Away 2', 'SyncA2', 'Testville', 'SA2')
         ON CONFLICT (team_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    // Two games: one before cutoff (2020-01-01), one after (2022-06-01)
    sqlx::query!(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type, game_state)
         VALUES (9991000002, 99992, '2020-01-01', 99913, 99914, 2, 'OFF'),
                (9991000003, 99993, '2022-06-01', 99913, 99914, 2, 'FINAL')
         ON CONFLICT (game_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    // Query with from_date = 2022-01-01 — only the 2022-06-01 game should appear
    let cutoff = time::Date::from_calendar_date(2022, time::Month::January, 1).unwrap();
    let candidates = pucksdata::process::sync::query_sync_candidates(pool, Some(cutoff))
        .await
        .unwrap();
    let ids: Vec<i64> = candidates.iter().map(|(id, _)| *id).collect();

    assert!(!ids.contains(&9991000002), "game before cutoff must be excluded by from_date filter");
    assert!(ids.contains(&9991000003), "game after cutoff must be included by from_date filter");

    // Cleanup (games before teams — FK constraint)
    sqlx::query!("DELETE FROM games WHERE game_id IN (9991000002, 9991000003)").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99913, 99914)").execute(pool).await.unwrap();
}

/// QUAL-SYNC-01: Games with NULL game_state still appear in query_sync_candidates (state filtering is in Rust).
/// The SQL does NOT filter by game_state — that is Rust-side in run_sync().
#[tokio::test]
async fn test_query_sync_candidates_includes_null_state() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    let pool = pucksdata::db::get_pool().await.unwrap();

    // Insert synthetic teams (unique range: 99915-99916 for this test)
    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99915, 'Sync Home 3', 'SyncH3', 'Testville', 'SH3'),
                (99916, 'Sync Away 3', 'SyncA3', 'Testville', 'SA3')
         ON CONFLICT (team_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    // Insert game with NULL game_state and no events (unique game id: 9991000004)
    sqlx::query!(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type)
         VALUES (9991000004, 99994, '2020-01-01', 99915, 99916, 2)
         ON CONFLICT (game_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    let candidates = pucksdata::process::sync::query_sync_candidates(pool, None)
        .await
        .unwrap();
    let matching: Vec<_> = candidates.iter().filter(|(id, _)| *id == 9991000004).collect();

    // SQL returns it (state filtering happens in Rust)
    assert!(!matching.is_empty(), "game with NULL state must appear in candidates (Rust filters it)");
    // State should be None
    assert!(matching[0].1.is_none(), "game_state should be None for NULL state");

    // Cleanup (games before teams — FK constraint)
    sqlx::query!("DELETE FROM games WHERE game_id = 9991000004").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99915, 99916)").execute(pool).await.unwrap();
}
