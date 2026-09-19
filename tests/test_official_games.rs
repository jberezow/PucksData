mod common;

use pucksdata::{
    fetchers::official_games::OfficialGameStats,
    models::{DbOfficialGoalieGame, DbOfficialSkaterGame},
};

const GAME_ID: i64 = 1900020001;
const SEASON: i32 = 19001901;

fn skater(points: i32) -> DbOfficialSkaterGame {
    DbOfficialSkaterGame {
        game_id: GAME_ID,
        player_id: 9_998_001,
        season: SEASON,
        game_type: 2,
        team_abbrev: Some("TST".into()),
        full_name: "Test Skater".into(),
        position_code: Some("C".into()),
        goals: Some(1),
        assists: Some(points - 1),
        points: Some(points),
        plus_minus: Some(2),
        penalty_minutes: Some(0),
        shots: Some(4),
        ev_goals: Some(1),
        ev_points: Some(points),
        pp_goals: Some(0),
        pp_points: Some(0),
        sh_goals: Some(0),
        sh_points: Some(0),
        ot_goals: Some(0),
        game_winning_goals: Some(1),
        hits: Some(3),
        blocked_shots: Some(2),
        giveaways: Some(1),
        takeaways: Some(1),
        time_on_ice_seconds: Some(1_200),
    }
}

fn goalie(goals: i32, assists: i32) -> DbOfficialGoalieGame {
    DbOfficialGoalieGame {
        game_id: GAME_ID,
        player_id: 9_998_002,
        season: SEASON,
        game_type: 2,
        team_abbrev: Some("TST".into()),
        full_name: "Test Goalie".into(),
        goals: Some(goals),
        assists: Some(assists),
        games_started: Some(1),
        wins: Some(1),
        losses: Some(0),
        ties: None,
        ot_losses: Some(0),
        shutouts: Some(1),
        shots_against: Some(25),
        saves: Some(25),
        goals_against: Some(0),
        save_pct: Some(1.0),
        time_on_ice_seconds: Some(3_600),
    }
}

async fn prepare_game(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM games WHERE game_id = $1")
        .bind(GAME_ID)
        .execute(pool)
        .await
        .unwrap();
    for team_id in [99_980_i64, 99_981_i64] {
        sqlx::query(
            "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
             VALUES ($1, 'Test Team', 'Test', 'Test', $2)
             ON CONFLICT (team_id) DO NOTHING",
        )
        .bind(team_id)
        .bind(format!("T{team_id}"))
        .execute(pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO games
             (game_id, season, game_date, home_team_id, away_team_id, game_type, game_state)
         VALUES ($1, $2, '1900-01-01', 99980, 99981, 2, 'OFF')",
    )
    .bind(GAME_ID)
    .bind(SEASON)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn official_game_snapshot_is_idempotent_and_revisioned() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    prepare_game(pool).await;

    let first = OfficialGameStats {
        game_id: GAME_ID,
        skaters: vec![skater(2)],
        goalies: vec![goalie(1, 2)],
    };
    pucksdata::loaders::official_games::replace_official_game_stats(pool, &first)
        .await
        .unwrap();
    pucksdata::loaders::official_games::replace_official_game_stats(pool, &first)
        .await
        .unwrap();
    let unchanged_revision: i32 = sqlx::query_scalar(
        "SELECT source_revision FROM analytics.official_skater_games
         WHERE game_id = $1 AND player_id = 9998001",
    )
    .bind(GAME_ID)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(unchanged_revision, 1);

    let corrected = OfficialGameStats {
        game_id: GAME_ID,
        skaters: vec![skater(3)],
        goalies: vec![goalie(2, 2)],
    };
    pucksdata::loaders::official_games::replace_official_game_stats(pool, &corrected)
        .await
        .unwrap();
    let (points, revision): (Option<i32>, i32) = sqlx::query_as(
        "SELECT points, source_revision FROM analytics.official_skater_games
         WHERE game_id = $1 AND player_id = 9998001",
    )
    .bind(GAME_ID)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(points, Some(3));
    assert_eq!(revision, 2);

    let (goalie_goals, goalie_revision): (Option<i32>, i32) = sqlx::query_as(
        "SELECT goals, source_revision FROM analytics.official_goalie_games
         WHERE game_id = $1 AND player_id = 9998002",
    )
    .bind(GAME_ID)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(goalie_goals, Some(2));
    assert_eq!(goalie_revision, 2);

    let values: Vec<(String, f64)> = sqlx::query_as(
        "SELECT stat_code, stat_value FROM analytics.official_player_game_stats
         WHERE game_id = $1 AND player_id = 9998001 ORDER BY stat_code",
    )
    .bind(GAME_ID)
    .fetch_all(pool)
    .await
    .unwrap();
    assert!(values.contains(&("plus_minus".into(), 2.0)));
    assert!(values.contains(&("game_winning_goals".into(), 1.0)));

    let goalie_values: Vec<(String, f64)> = sqlx::query_as(
        "SELECT stat_code, stat_value FROM analytics.official_player_game_stats
         WHERE game_id = $1 AND player_id = 9998002 ORDER BY stat_code",
    )
    .bind(GAME_ID)
    .fetch_all(pool)
    .await
    .unwrap();
    assert!(goalie_values.contains(&("goals".into(), 2.0)));
    assert!(goalie_values.contains(&("assists".into(), 2.0)));

    sqlx::query("DELETE FROM games WHERE game_id = $1")
        .bind(GAME_ID)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires the live NHL stats API"]
async fn live_final_game_reports_cover_fantasy_categories() {
    let stats = pucksdata::fetchers::official_games::fetch_official_game_stats(2025030416, 3)
        .await
        .unwrap();
    assert!(stats.skaters.len() >= 30);
    assert!(stats.goalies.len() >= 2);
    assert!(stats.skaters.iter().any(|row| row.plus_minus.is_some()));
    assert!(stats.skaters.iter().any(|row| row.hits.is_some()));
    assert!(stats.goalies.iter().any(|row| row.wins == Some(1)));
}

#[tokio::test]
#[ignore = "requires the live NHL stats API"]
async fn live_goalie_goal_is_published_by_summary_report() {
    let stats = pucksdata::fetchers::official_games::fetch_official_game_stats(2023020345, 2)
        .await
        .unwrap();
    let jarry = stats
        .goalies
        .iter()
        .find(|row| row.player_id == 8_477_465)
        .expect("Tristan Jarry should appear in the official goalie report");
    assert_eq!(jarry.goals, Some(1));
    assert_eq!(jarry.assists, Some(0));
}
