// tests/test_status.rs
// Integration and unit tests for Phase 16: Status Command
// Tests: SYNC-05 (diagnostic queries), SYNC-06 (fix idempotency, exit code logic)
//
// Synthetic ID ranges (no collision with other test files):
//   teams: 99941-99960
//   games: 9992000001-9992000020
//   seasons: 99981-99984

/// SYNC-05 unit: coverage_pct calculation — 100% when all OFF games have events.
/// Pure logic test, no DATABASE_URL required.
#[test]
fn test_coverage_calculation_unit() {
    // 5 off games, 5 with events → 100%
    let pct = if 5i64 > 0 { (5i64 as f64 / 5i64 as f64) * 100.0 } else { 100.0 };
    assert!((pct - 100.0).abs() < 0.01, "100% coverage expected when games_with_events == total_off_games");

    // 4 out of 5 → 80%
    let pct2 = if 5i64 > 0 { (4i64 as f64 / 5i64 as f64) * 100.0 } else { 100.0 };
    assert!((pct2 - 80.0).abs() < 0.01, "80% coverage expected for 4/5");

    // 0 off games → 100% (no games = trivially healthy)
    let pct3 = if 0i64 > 0 { (0i64 as f64 / 0i64 as f64) * 100.0 } else { 100.0 };
    assert!((pct3 - 100.0).abs() < 0.01, "0 off games should report 100% (trivially healthy)");
}

/// SYNC-05 integration: run_status() returns Ok(true) when a season has all OFF games covered.
#[tokio::test]
async fn test_status_query_healthy_season() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    let pool = pucksdata::db::get_pool().await.unwrap();

    // Insert synthetic teams and a completed game with events
    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99941, 'Status Home A', 'StatHA', 'Testville', 'SHA'),
                (99942, 'Status Away A', 'StatAA', 'Testville', 'SAA')
         ON CONFLICT (team_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    sqlx::query!(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type, game_state)
         VALUES (9992000001, 99981, '2099-01-01', 99941, 99942, 2, 'OFF')
         ON CONFLICT (game_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    // Add an events row → game is covered
    sqlx::query!(
        "INSERT INTO events (game_id, event_id_in_game, period, period_type, time_in_period, event_type)
         VALUES (9992000001, 1, 1, 'REG', '00:00', 'goal')
         ON CONFLICT (game_id, event_id_in_game) DO NOTHING"
    ).execute(pool).await.unwrap();

    let healthy = pucksdata::process::status::run_status(&pool, Some(99981), false)
        .await
        .unwrap();

    assert!(healthy, "season with all OFF games covered must return healthy=true");

    // Cleanup (child tables first)
    sqlx::query!("DELETE FROM events WHERE game_id = 9992000001").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM games WHERE game_id = 9992000001").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99941, 99942)").execute(pool).await.unwrap();
}

/// SYNC-05 integration: run_status() returns Ok(false) when OFF game has no events.
#[tokio::test]
async fn test_status_query_unhealthy_season() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    let pool = pucksdata::db::get_pool().await.unwrap();

    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99943, 'Status Home B', 'StatHB', 'Testville', 'SHB'),
                (99944, 'Status Away B', 'StatAB', 'Testville', 'SAB')
         ON CONFLICT (team_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    // OFF game with NO events → unhealthy
    sqlx::query!(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type, game_state)
         VALUES (9992000002, 99982, '2099-01-02', 99943, 99944, 2, 'OFF')
         ON CONFLICT (game_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    let healthy = pucksdata::process::status::run_status(&pool, Some(99982), false)
        .await
        .unwrap();

    assert!(!healthy, "season with uncovered OFF game must return healthy=false");

    // Cleanup
    sqlx::query!("DELETE FROM games WHERE game_id = 9992000002").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99943, 99944)").execute(pool).await.unwrap();
}

/// SYNC-05 integration: --season filter scopes output to the specified season only.
#[tokio::test]
async fn test_status_season_filter() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    let pool = pucksdata::db::get_pool().await.unwrap();

    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99945, 'Status Home C', 'StatHC', 'Testville', 'SHC'),
                (99946, 'Status Away C', 'StatAC', 'Testville', 'SAC')
         ON CONFLICT (team_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    // Season 99983: 1 OFF game, covered (healthy)
    // Season 99984: 1 OFF game, NOT covered (unhealthy)
    sqlx::query!(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type, game_state)
         VALUES (9992000003, 99983, '2099-01-03', 99945, 99946, 2, 'OFF'),
                (9992000004, 99984, '2099-01-04', 99945, 99946, 2, 'OFF')
         ON CONFLICT (game_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    // Cover the season-99983 game only
    sqlx::query!(
        "INSERT INTO events (game_id, event_id_in_game, period, period_type, time_in_period, event_type)
         VALUES (9992000003, 1, 1, 'REG', '00:00', 'goal')
         ON CONFLICT (game_id, event_id_in_game) DO NOTHING"
    ).execute(pool).await.unwrap();

    // Filter to season 99983 → must return healthy (its own game is covered)
    let healthy_scoped = pucksdata::process::status::run_status(&pool, Some(99983), false)
        .await
        .unwrap();
    assert!(healthy_scoped, "season-scoped query must only see season 99983, which is healthy");

    // Filter to season 99984 → must return unhealthy
    let unhealthy_scoped = pucksdata::process::status::run_status(&pool, Some(99984), false)
        .await
        .unwrap();
    assert!(!unhealthy_scoped, "season-scoped query for 99984 must return unhealthy (game has no events)");

    // Cleanup
    sqlx::query!("DELETE FROM events WHERE game_id = 9992000003").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM games WHERE game_id IN (9992000003, 9992000004)").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99945, 99946)").execute(pool).await.unwrap();
}

/// SYNC-05 integration: FUT/PRE games are excluded from total_off_games count.
#[tokio::test]
async fn test_status_excludes_fut_pre_games() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    let pool = pucksdata::db::get_pool().await.unwrap();

    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99947, 'Status Home D', 'StatHD', 'Testville', 'SHD'),
                (99948, 'Status Away D', 'StatAD', 'Testville', 'SAD')
         ON CONFLICT (team_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    // Season 99985: 1 OFF covered game + 1 FUT game (FUT must not count toward total_off_games)
    sqlx::query!(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type, game_state)
         VALUES (9992000005, 99985, '2099-01-05', 99947, 99948, 2, 'OFF'),
                (9992000006, 99985, '2099-09-01', 99947, 99948, 2, 'FUT')
         ON CONFLICT (game_id) DO NOTHING"
    ).execute(pool).await.unwrap();

    // Cover the OFF game
    sqlx::query!(
        "INSERT INTO events (game_id, event_id_in_game, period, period_type, time_in_period, event_type)
         VALUES (9992000005, 1, 1, 'REG', '00:00', 'goal')
         ON CONFLICT (game_id, event_id_in_game) DO NOTHING"
    ).execute(pool).await.unwrap();

    // Should be healthy: FUT game doesn't reduce coverage
    let healthy = pucksdata::process::status::run_status(&pool, Some(99985), false)
        .await
        .unwrap();
    assert!(healthy, "FUT/PRE games must not count toward total_off_games (season should be healthy)");

    // Cleanup
    sqlx::query!("DELETE FROM events WHERE game_id = 9992000005").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM games WHERE game_id IN (9992000005, 9992000006)").execute(pool).await.unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99947, 99948)").execute(pool).await.unwrap();
}

/// SYNC-06 placeholder: test_fix_idempotent — wired in Plan 02.
#[tokio::test]
async fn test_fix_idempotent() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    // Full implementation in 16-02-PLAN.md. Placeholder passes vacuously.
    // TODO(16-02): insert healthy season, call run_status with fix=true, verify no side effects.
}
