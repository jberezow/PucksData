mod common;

use pucksdata::models::{DbOfficialGoalieSeason, DbOfficialSkaterSeason};

fn skater(points: Option<i32>) -> DbOfficialSkaterSeason {
    DbOfficialSkaterSeason {
        player_id: 9_999_001,
        season: 19001901,
        game_type: 2,
        full_name: "Test Skater".to_string(),
        position_code: Some("C".to_string()),
        shoots_catches: Some("L".to_string()),
        team_abbrevs: Some("TOR,BOS".to_string()),
        games_played: Some(20),
        goals: Some(10),
        assists: Some(5),
        points,
        plus_minus: None,
        penalty_minutes: Some(12),
        shots: None,
        shooting_pct: None,
        ev_goals: None,
        ev_points: None,
        pp_goals: None,
        pp_points: None,
        sh_goals: None,
        sh_points: None,
        ot_goals: None,
        game_winning_goals: Some(2),
        points_per_game: Some(0.75),
        faceoff_win_pct: None,
        time_on_ice_per_game: None,
    }
}

fn goalie(wins: Option<i32>) -> DbOfficialGoalieSeason {
    DbOfficialGoalieSeason {
        player_id: 9_999_002,
        season: 19001901,
        game_type: 2,
        full_name: "Test Goalie".to_string(),
        shoots_catches: Some("L".to_string()),
        team_abbrevs: Some("MTL".to_string()),
        games_played: Some(18),
        games_started: None,
        wins,
        losses: Some(6),
        ties: Some(2),
        ot_losses: None,
        shutouts: Some(3),
        shots_against: None,
        saves: None,
        goals_against: Some(40),
        save_pct: None,
        goals_against_average: Some(2.22),
        time_on_ice: Some(64_800),
        goals: Some(0),
        assists: Some(1),
        points: Some(1),
        penalty_minutes: Some(0),
    }
}

async fn cleanup(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM analytics.official_skater_seasons WHERE season = 19001901")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM analytics.official_goalie_seasons WHERE season = 19001901")
        .execute(pool)
        .await
        .unwrap();
}

/// Re-running a season must revise its rows, not duplicate them. The NHL
/// restates historical totals occasionally and a refetch should adopt that.
#[tokio::test]
async fn test_official_stats_upsert_is_idempotent_and_revises() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    cleanup(pool).await;

    pucksdata::loaders::official_stats::upsert_skater_seasons(pool, &[skater(Some(15))])
        .await
        .unwrap();
    pucksdata::loaders::official_stats::upsert_skater_seasons(pool, &[skater(Some(16))])
        .await
        .unwrap();
    pucksdata::loaders::official_stats::upsert_goalie_seasons(pool, &[goalie(Some(10))])
        .await
        .unwrap();
    pucksdata::loaders::official_stats::upsert_goalie_seasons(pool, &[goalie(Some(11))])
        .await
        .unwrap();

    let (skater_rows, points): (i64, Option<i32>) = sqlx::query_as(
        "SELECT COUNT(*), MAX(points) FROM analytics.official_skater_seasons WHERE season = 19001901",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(skater_rows, 1, "skater upsert duplicated a row");
    assert_eq!(points, Some(16), "skater upsert did not adopt the revision");

    let (goalie_rows, wins): (i64, Option<i32>) = sqlx::query_as(
        "SELECT COUNT(*), MAX(wins) FROM analytics.official_goalie_seasons WHERE season = 19001901",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(goalie_rows, 1, "goalie upsert duplicated a row");
    assert_eq!(wins, Some(11), "goalie upsert did not adopt the revision");

    cleanup(pool).await;
}

/// Fields the NHL did not record in an era must round-trip as NULL rather
/// than as zero, so consumers can tell "none" from "not tracked".
#[tokio::test]
async fn test_official_stats_preserve_absent_fields_as_null() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    cleanup(pool).await;

    pucksdata::loaders::official_stats::upsert_skater_seasons(pool, &[skater(Some(15))])
        .await
        .unwrap();

    let (shots, plus_minus, toi): (Option<i32>, Option<i32>, Option<f64>) = sqlx::query_as(
        "SELECT shots, plus_minus, time_on_ice_per_game
         FROM analytics.official_skater_seasons WHERE season = 19001901",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(shots, None, "absent shots must stay NULL, not become 0");
    assert_eq!(plus_minus, None, "absent plus-minus must stay NULL");
    assert_eq!(toi, None, "absent ice time must stay NULL");

    cleanup(pool).await;
}

/// The coverage contract must describe the tables that now exist.
#[tokio::test]
async fn test_coverage_describes_official_tables() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;

    let absent_games_played: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM analytics.coverage WHERE subject = 'games_played' AND kind = 'absent'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        absent_games_played, 0,
        "games_played is answerable from official season totals and must no longer be 'absent'"
    );

    let described: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM analytics.coverage
         WHERE subject IN ('official_skater_seasons', 'official_goalie_seasons')",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(described, 2, "coverage must describe both official tables");
}

/// Exercises the live stats API. Ignored by default like the canary.
#[tokio::test]
#[ignore = "requires the live NHL stats API"]
async fn live_official_stats_span_the_archive() {
    // 1917-18 predates shots and ice time but still reports games played.
    let earliest = pucksdata::fetchers::official_stats::fetch_skater_season(19171918, 2)
        .await
        .unwrap();
    assert!(!earliest.is_empty(), "1917-18 returned no skaters");
    assert!(earliest.iter().all(|row| row.shots.is_none()));
    assert!(earliest.iter().any(|row| row.games_played.is_some()));

    // 1967-68 is the first season with shots and plus-minus.
    let expansion = pucksdata::fetchers::official_stats::fetch_skater_season(19671968, 2)
        .await
        .unwrap();
    assert!(expansion.iter().any(|row| row.shots.is_some()));
    assert!(expansion.iter().any(|row| row.plus_minus.is_some()));

    // Goalie records reach back to the first season.
    let goalies = pucksdata::fetchers::official_stats::fetch_goalie_season(19171918, 2)
        .await
        .unwrap();
    assert!(!goalies.is_empty(), "1917-18 returned no goalies");
    assert!(goalies.iter().any(|row| row.wins.is_some()));
    assert!(goalies.iter().any(|row| row.shutouts.is_some()));

    // A modern season carries the full field set.
    let modern = pucksdata::fetchers::official_stats::fetch_skater_season(20242025, 2)
        .await
        .unwrap();
    assert!(modern.len() > 800, "expected a full modern skater roster");
    assert!(modern.iter().any(|row| row.time_on_ice_per_game.is_some()));
}
