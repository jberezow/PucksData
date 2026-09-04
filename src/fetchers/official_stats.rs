//! Fetches official NHL per-player season totals from the stats API.
//!
//! These are the league's published aggregates. They answer season-level
//! questions the play-by-play schema cannot, and they are the reconciliation
//! oracle for event-derived figures.

use serde::Deserialize;

use crate::{
    api::fetch_api_json,
    models::{DbOfficialGoalieSeason, DbOfficialSkaterSeason},
    AnyError,
};

/// Game types the stats API publishes season summaries for.
pub const OFFICIAL_GAME_TYPES: [i16; 2] = [2, 3];

#[derive(Deserialize)]
struct SummaryResponse<T> {
    data: Vec<T>,
}

#[derive(Deserialize)]
struct SkaterSummary {
    #[serde(rename = "playerId")]
    player_id: i64,
    #[serde(rename = "seasonId")]
    season_id: i32,
    #[serde(rename = "skaterFullName")]
    full_name: String,
    #[serde(rename = "positionCode", default)]
    position_code: Option<String>,
    #[serde(rename = "shootsCatches", default)]
    shoots_catches: Option<String>,
    #[serde(rename = "teamAbbrevs", default)]
    team_abbrevs: Option<String>,
    #[serde(rename = "gamesPlayed", default)]
    games_played: Option<i32>,
    #[serde(default)]
    goals: Option<i32>,
    #[serde(default)]
    assists: Option<i32>,
    #[serde(default)]
    points: Option<i32>,
    #[serde(rename = "plusMinus", default)]
    plus_minus: Option<i32>,
    #[serde(rename = "penaltyMinutes", default)]
    penalty_minutes: Option<i32>,
    #[serde(default)]
    shots: Option<i32>,
    #[serde(rename = "shootingPct", default)]
    shooting_pct: Option<f64>,
    #[serde(rename = "evGoals", default)]
    ev_goals: Option<i32>,
    #[serde(rename = "evPoints", default)]
    ev_points: Option<i32>,
    #[serde(rename = "ppGoals", default)]
    pp_goals: Option<i32>,
    #[serde(rename = "ppPoints", default)]
    pp_points: Option<i32>,
    #[serde(rename = "shGoals", default)]
    sh_goals: Option<i32>,
    #[serde(rename = "shPoints", default)]
    sh_points: Option<i32>,
    #[serde(rename = "otGoals", default)]
    ot_goals: Option<i32>,
    #[serde(rename = "gameWinningGoals", default)]
    game_winning_goals: Option<i32>,
    #[serde(rename = "pointsPerGame", default)]
    points_per_game: Option<f64>,
    #[serde(rename = "faceoffWinPct", default)]
    faceoff_win_pct: Option<f64>,
    #[serde(rename = "timeOnIcePerGame", default)]
    time_on_ice_per_game: Option<f64>,
}

#[derive(Deserialize)]
struct GoalieSummary {
    #[serde(rename = "playerId")]
    player_id: i64,
    #[serde(rename = "seasonId")]
    season_id: i32,
    #[serde(rename = "goalieFullName")]
    full_name: String,
    #[serde(rename = "shootsCatches", default)]
    shoots_catches: Option<String>,
    #[serde(rename = "teamAbbrevs", default)]
    team_abbrevs: Option<String>,
    #[serde(rename = "gamesPlayed", default)]
    games_played: Option<i32>,
    #[serde(rename = "gamesStarted", default)]
    games_started: Option<i32>,
    #[serde(default)]
    wins: Option<i32>,
    #[serde(default)]
    losses: Option<i32>,
    #[serde(default)]
    ties: Option<i32>,
    #[serde(rename = "otLosses", default)]
    ot_losses: Option<i32>,
    #[serde(default)]
    shutouts: Option<i32>,
    #[serde(rename = "shotsAgainst", default)]
    shots_against: Option<i32>,
    #[serde(default)]
    saves: Option<i32>,
    #[serde(rename = "goalsAgainst", default)]
    goals_against: Option<i32>,
    #[serde(rename = "savePct", default)]
    save_pct: Option<f64>,
    #[serde(rename = "goalsAgainstAverage", default)]
    goals_against_average: Option<f64>,
    #[serde(rename = "timeOnIce", default)]
    time_on_ice: Option<i64>,
    #[serde(default)]
    goals: Option<i32>,
    #[serde(default)]
    assists: Option<i32>,
    #[serde(default)]
    points: Option<i32>,
    #[serde(rename = "penaltyMinutes", default)]
    penalty_minutes: Option<i32>,
}

/// Build the stats API summary URL for one report, season, and game type.
pub fn summary_url(report: &str, season: i32, game_type: i16) -> String {
    format!(
        "https://api.nhle.com/stats/rest/en/{report}/summary?limit=-1\
         &cayenneExp=seasonId%3D{season}%20and%20gameTypeId%3D{game_type}"
    )
}

/// Fetch official skater totals for one season and game type.
///
/// A season the NHL has no summary for returns an empty vector rather than an
/// error; lockout seasons and game types a season never played are normal.
pub async fn fetch_skater_season(
    season: i32,
    game_type: i16,
) -> Result<Vec<DbOfficialSkaterSeason>, AnyError> {
    let json = fetch_api_json(&summary_url("skater", season, game_type)).await?;
    let response: SummaryResponse<SkaterSummary> = serde_json::from_str(&json)?;

    Ok(response
        .data
        .into_iter()
        .map(|row| DbOfficialSkaterSeason {
            player_id: row.player_id,
            season: row.season_id,
            game_type,
            full_name: row.full_name,
            position_code: row.position_code,
            shoots_catches: row.shoots_catches,
            team_abbrevs: row.team_abbrevs,
            games_played: row.games_played,
            goals: row.goals,
            assists: row.assists,
            points: row.points,
            plus_minus: row.plus_minus,
            penalty_minutes: row.penalty_minutes,
            shots: row.shots,
            shooting_pct: row.shooting_pct,
            ev_goals: row.ev_goals,
            ev_points: row.ev_points,
            pp_goals: row.pp_goals,
            pp_points: row.pp_points,
            sh_goals: row.sh_goals,
            sh_points: row.sh_points,
            ot_goals: row.ot_goals,
            game_winning_goals: row.game_winning_goals,
            points_per_game: row.points_per_game,
            faceoff_win_pct: row.faceoff_win_pct,
            time_on_ice_per_game: row.time_on_ice_per_game,
        })
        .collect())
}

/// Fetch official goalie totals for one season and game type.
pub async fn fetch_goalie_season(
    season: i32,
    game_type: i16,
) -> Result<Vec<DbOfficialGoalieSeason>, AnyError> {
    let json = fetch_api_json(&summary_url("goalie", season, game_type)).await?;
    let response: SummaryResponse<GoalieSummary> = serde_json::from_str(&json)?;

    Ok(response
        .data
        .into_iter()
        .map(|row| DbOfficialGoalieSeason {
            player_id: row.player_id,
            season: row.season_id,
            game_type,
            full_name: row.full_name,
            shoots_catches: row.shoots_catches,
            team_abbrevs: row.team_abbrevs,
            games_played: row.games_played,
            games_started: row.games_started,
            wins: row.wins,
            losses: row.losses,
            ties: row.ties,
            ot_losses: row.ot_losses,
            shutouts: row.shutouts,
            shots_against: row.shots_against,
            saves: row.saves,
            goals_against: row.goals_against,
            save_pct: row.save_pct,
            goals_against_average: row.goals_against_average,
            time_on_ice: row.time_on_ice,
            goals: row.goals,
            assists: row.assists,
            points: row.points,
            penalty_minutes: row.penalty_minutes,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_summary_urls_for_both_reports() {
        assert_eq!(
            summary_url("skater", 20242025, 2),
            "https://api.nhle.com/stats/rest/en/skater/summary?limit=-1\
             &cayenneExp=seasonId%3D20242025%20and%20gameTypeId%3D2"
        );
        assert!(summary_url("goalie", 19171918, 3).contains("goalie/summary"));
        assert!(summary_url("goalie", 19171918, 3).contains("gameTypeId%3D3"));
    }

    #[test]
    fn parses_a_modern_skater_row() {
        let json = r#"{"data":[{
            "playerId": 8478402, "seasonId": 20242025, "skaterFullName": "Connor McDavid",
            "positionCode": "C", "shootsCatches": "L", "teamAbbrevs": "EDM",
            "gamesPlayed": 67, "goals": 26, "assists": 74, "points": 100,
            "plusMinus": 20, "penaltyMinutes": 46, "shots": 191,
            "shootingPct": 0.13612, "evGoals": 17, "evPoints": 63,
            "ppGoals": 9, "ppPoints": 36, "shGoals": 0, "shPoints": 1,
            "otGoals": 2, "gameWinningGoals": 3, "pointsPerGame": 1.49253,
            "faceoffWinPct": 0.50675, "timeOnIcePerGame": 1273.4477
        }]}"#;
        let response: SummaryResponse<SkaterSummary> = serde_json::from_str(json).unwrap();
        let row = &response.data[0];

        assert_eq!(row.player_id, 8478402);
        assert_eq!(row.points, Some(100));
        assert_eq!(row.time_on_ice_per_game, Some(1273.4477));
    }

    #[test]
    fn parses_an_early_season_row_with_absent_fields() {
        // 1917-18 predates shots, plus-minus, and ice time.
        let json = r#"{"data":[{
            "playerId": 8445000, "seasonId": 19171918, "skaterFullName": "Joe Malone",
            "gamesPlayed": 20, "goals": 44, "assists": 4, "points": 48,
            "penaltyMinutes": 30, "gameWinningGoals": 5,
            "plusMinus": null, "shots": null, "timeOnIcePerGame": null
        }]}"#;
        let response: SummaryResponse<SkaterSummary> = serde_json::from_str(json).unwrap();
        let row = &response.data[0];

        assert_eq!(row.goals, Some(44));
        assert_eq!(row.shots, None);
        assert_eq!(row.plus_minus, None);
        assert_eq!(row.time_on_ice_per_game, None);
        assert_eq!(row.position_code, None);
    }

    #[test]
    fn parses_a_goalie_row_with_ties() {
        let json = r#"{"data":[{
            "playerId": 8449000, "seasonId": 19671968, "goalieFullName": "Gump Worsley",
            "shootsCatches": "L", "teamAbbrevs": "MTL", "gamesPlayed": 40,
            "gamesStarted": null, "wins": 19, "losses": 9, "ties": 8,
            "otLosses": null, "shutouts": 6, "shotsAgainst": null, "saves": null,
            "goalsAgainst": 73, "savePct": null, "goalsAgainstAverage": 1.98,
            "timeOnIce": 132720, "goals": 0, "assists": 1, "points": 1,
            "penaltyMinutes": 0
        }]}"#;
        let response: SummaryResponse<GoalieSummary> = serde_json::from_str(json).unwrap();
        let row = &response.data[0];

        assert_eq!(row.wins, Some(19));
        assert_eq!(row.ties, Some(8));
        assert_eq!(row.ot_losses, None);
        assert_eq!(row.time_on_ice, Some(132720));
    }
}
