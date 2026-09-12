//! Fetches league-published player statistics at one-game grain.
//!
//! The stats API can filter its standard reports by `gameId`. Combining the
//! skater summary and realtime reports supplies the final-boxscore categories
//! downstream consumers need without weakening the event-first schema.

use std::collections::HashMap;

use serde::Deserialize;

use crate::{
    api::fetch_api_json,
    models::{DbOfficialGoalieGame, DbOfficialSkaterGame},
    AnyError,
};

#[derive(Deserialize)]
struct ReportResponse<T> {
    data: Vec<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkaterSummaryRow {
    player_id: i64,
    season_id: i32,
    skater_full_name: String,
    #[serde(default)]
    position_code: Option<String>,
    #[serde(default)]
    team_abbrevs: Option<String>,
    #[serde(default)]
    goals: Option<i32>,
    #[serde(default)]
    assists: Option<i32>,
    #[serde(default)]
    points: Option<i32>,
    #[serde(default)]
    plus_minus: Option<i32>,
    #[serde(default)]
    penalty_minutes: Option<i32>,
    #[serde(default)]
    shots: Option<i32>,
    #[serde(default)]
    ev_goals: Option<i32>,
    #[serde(default)]
    ev_points: Option<i32>,
    #[serde(default)]
    pp_goals: Option<i32>,
    #[serde(default)]
    pp_points: Option<i32>,
    #[serde(default)]
    sh_goals: Option<i32>,
    #[serde(default)]
    sh_points: Option<i32>,
    #[serde(default)]
    ot_goals: Option<i32>,
    #[serde(default)]
    game_winning_goals: Option<i32>,
    #[serde(default)]
    time_on_ice_per_game: Option<f64>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct SkaterRealtimeRow {
    player_id: i64,
    #[serde(default)]
    hits: Option<i32>,
    #[serde(default)]
    blocked_shots: Option<i32>,
    #[serde(default)]
    giveaways: Option<i32>,
    #[serde(default)]
    takeaways: Option<i32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoalieSummaryRow {
    player_id: i64,
    season_id: i32,
    goalie_full_name: String,
    #[serde(default)]
    team_abbrevs: Option<String>,
    #[serde(default)]
    games_started: Option<i32>,
    #[serde(default)]
    wins: Option<i32>,
    #[serde(default)]
    losses: Option<i32>,
    #[serde(default)]
    ties: Option<i32>,
    #[serde(default)]
    ot_losses: Option<i32>,
    #[serde(default)]
    shutouts: Option<i32>,
    #[serde(default)]
    shots_against: Option<i32>,
    #[serde(default)]
    saves: Option<i32>,
    #[serde(default)]
    goals_against: Option<i32>,
    #[serde(default)]
    save_pct: Option<f64>,
    #[serde(default)]
    time_on_ice: Option<i64>,
}

pub struct OfficialGameStats {
    pub game_id: i64,
    pub skaters: Vec<DbOfficialSkaterGame>,
    pub goalies: Vec<DbOfficialGoalieGame>,
}

pub fn report_url(entity: &str, report: &str, game_id: i64) -> String {
    format!(
        "https://api.nhle.com/stats/rest/en/{entity}/{report}?limit=-1&cayenneExp=gameId%3D{game_id}"
    )
}

fn parse_game_stats(
    game_id: i64,
    game_type: i16,
    skater_summary_json: &str,
    skater_realtime_json: &str,
    goalie_summary_json: &str,
) -> Result<OfficialGameStats, serde_json::Error> {
    let summaries: ReportResponse<SkaterSummaryRow> = serde_json::from_str(skater_summary_json)?;
    let realtime: ReportResponse<SkaterRealtimeRow> = serde_json::from_str(skater_realtime_json)?;
    let goalies: ReportResponse<GoalieSummaryRow> = serde_json::from_str(goalie_summary_json)?;
    let mut realtime_by_player: HashMap<i64, SkaterRealtimeRow> = realtime
        .data
        .into_iter()
        .map(|row| (row.player_id, row))
        .collect();

    let skaters = summaries
        .data
        .into_iter()
        .map(|row| {
            let realtime = realtime_by_player
                .remove(&row.player_id)
                .unwrap_or_default();
            DbOfficialSkaterGame {
                game_id,
                player_id: row.player_id,
                season: row.season_id,
                game_type,
                team_abbrev: row.team_abbrevs,
                full_name: row.skater_full_name,
                position_code: row.position_code,
                goals: row.goals,
                assists: row.assists,
                points: row.points,
                plus_minus: row.plus_minus,
                penalty_minutes: row.penalty_minutes,
                shots: row.shots,
                ev_goals: row.ev_goals,
                ev_points: row.ev_points,
                pp_goals: row.pp_goals,
                pp_points: row.pp_points,
                sh_goals: row.sh_goals,
                sh_points: row.sh_points,
                ot_goals: row.ot_goals,
                game_winning_goals: row.game_winning_goals,
                hits: realtime.hits,
                blocked_shots: realtime.blocked_shots,
                giveaways: realtime.giveaways,
                takeaways: realtime.takeaways,
                time_on_ice_seconds: row
                    .time_on_ice_per_game
                    .map(|seconds| seconds.round() as i32),
            }
        })
        .collect();
    let goalies = goalies
        .data
        .into_iter()
        .map(|row| DbOfficialGoalieGame {
            game_id,
            player_id: row.player_id,
            season: row.season_id,
            game_type,
            team_abbrev: row.team_abbrevs,
            full_name: row.goalie_full_name,
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
            time_on_ice_seconds: row.time_on_ice,
        })
        .collect();

    Ok(OfficialGameStats {
        game_id,
        skaters,
        goalies,
    })
}

/// Fetch all official player statistics for one completed NHL game.
pub async fn fetch_official_game_stats(
    game_id: i64,
    game_type: i16,
) -> Result<OfficialGameStats, AnyError> {
    let skater_summary_url = report_url("skater", "summary", game_id);
    let skater_realtime_url = report_url("skater", "realtime", game_id);
    let goalie_summary_url = report_url("goalie", "summary", game_id);
    let (skater_summary, skater_realtime, goalie_summary) = tokio::try_join!(
        fetch_api_json(&skater_summary_url),
        fetch_api_json(&skater_realtime_url),
        fetch_api_json(&goalie_summary_url),
    )?;
    Ok(parse_game_stats(
        game_id,
        game_type,
        &skater_summary,
        &skater_realtime,
        &goalie_summary,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_game_filtered_report_url() {
        assert_eq!(
            report_url("skater", "summary", 2025020001),
            "https://api.nhle.com/stats/rest/en/skater/summary?limit=-1&cayenneExp=gameId%3D2025020001"
        );
    }

    #[test]
    fn combines_skater_reports_and_goalie_summary() {
        let stats = parse_game_stats(
            2025020001,
            2,
            r#"{"data":[{"playerId":1,"seasonId":20252026,"skaterFullName":"A Skater","teamAbbrevs":"MTL","goals":1,"assists":2,"points":3,"plusMinus":2,"ppPoints":1,"shPoints":0,"gameWinningGoals":1,"timeOnIcePerGame":901.6}]}"#,
            r#"{"data":[{"playerId":1,"hits":4,"blockedShots":2,"giveaways":1,"takeaways":3}]}"#,
            r#"{"data":[{"playerId":2,"seasonId":20252026,"goalieFullName":"A Goalie","teamAbbrevs":"MTL","gamesStarted":1,"wins":1,"shutouts":1,"saves":27}]}"#,
        )
        .unwrap();

        assert_eq!(stats.skaters[0].hits, Some(4));
        assert_eq!(stats.skaters[0].blocked_shots, Some(2));
        assert_eq!(stats.skaters[0].time_on_ice_seconds, Some(902));
        assert_eq!(stats.goalies[0].wins, Some(1));
        assert_eq!(stats.goalies[0].shutouts, Some(1));
    }
}
