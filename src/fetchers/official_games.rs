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
    total: Option<usize>,
}

impl<T> ReportResponse<T> {
    fn validate_count(&self) -> Result<(), crate::AnyError> {
        if self.total.is_some_and(|total| total != self.data.len()) {
            return Err("official report count disagrees with returned rows".into());
        }
        Ok(())
    }
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
    goals: Option<i32>,
    #[serde(default)]
    assists: Option<i32>,
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

impl OfficialGameStats {
    /// Structural checks also protect direct library callers of the loader.
    pub fn validate(&self) -> Result<(), String> {
        if self.skaters.is_empty() || self.goalies.is_empty() {
            return Err("official snapshot requires both skater and goalie reports".into());
        }
        let mut players = std::collections::HashSet::new();
        let season = self.skaters[0].season;
        let game_type = self.skaters[0].game_type;
        for (game, player, row_season, kind) in self
            .skaters
            .iter()
            .map(|r| (r.game_id, r.player_id, r.season, r.game_type))
            .chain(
                self.goalies
                    .iter()
                    .map(|r| (r.game_id, r.player_id, r.season, r.game_type)),
            )
        {
            if game != self.game_id
                || player <= 0
                || row_season != season
                || kind != game_type
                || !players.insert(player)
            {
                return Err(
                    "official snapshot contains duplicate or inconsistent identities".into(),
                );
            }
        }
        for skater in &self.skaters {
            if skater.team_abbrev.as_deref().is_none_or(str::is_empty) {
                return Err("official skater team is missing".into());
            }
            if season >= 20092010
                && [
                    skater.goals,
                    skater.assists,
                    skater.shots,
                    skater.pp_points,
                    skater.sh_points,
                    skater.game_winning_goals,
                    skater.hits,
                    skater.blocked_shots,
                    skater.plus_minus,
                ]
                .iter()
                .any(Option::is_none)
            {
                return Err("modern official skater report is missing a scoring category".into());
            }
        }
        for goalie in &self.goalies {
            if goalie.team_abbrev.as_deref().is_none_or(str::is_empty) {
                return Err("official goalie team is missing".into());
            }
            if season >= 20092010
                && [
                    goalie.goals,
                    goalie.assists,
                    goalie.wins,
                    goalie.shutouts,
                    goalie.saves,
                ]
                .iter()
                .any(Option::is_none)
            {
                return Err("modern official goalie report is missing a scoring category".into());
            }
        }
        Ok(())
    }
}

/// A second NHL presentation establishes player inventory before replacement.
/// It is a completeness guard, not an independent statistical accuracy oracle.
fn validate_boxscore(stats: &mut OfficialGameStats, body: &str) -> Result<(), crate::AnyError> {
    stats.validate()?;
    let value: serde_json::Value = serde_json::from_str(body)?;
    if value["id"].as_i64() != Some(stats.game_id)
        || !value["gameState"]
            .as_str()
            .is_some_and(crate::process::sync::is_game_completed)
    {
        return Err("official boxscore is not the requested completed game".into());
    }
    let mut expected = std::collections::HashSet::new();
    let mut allowed = std::collections::HashSet::new();
    let mut game_teams = HashMap::new();
    for side in ["homeTeam", "awayTeam"] {
        let team = value[side]["abbrev"]
            .as_str()
            .ok_or("missing boxscore team")?;
        for group in ["forwards", "defense", "goalies"] {
            let rows = value["playerByGameStats"][side][group]
                .as_array()
                .ok_or("missing boxscore player group")?;
            for row in rows {
                let id = row["playerId"]
                    .as_i64()
                    .ok_or("missing boxscore player ID")?;
                if !allowed.insert(id) {
                    return Err("duplicate boxscore player".into());
                }
                let is_goalie = group == "goalies";
                if !is_goalie
                    || row["toi"]
                        .as_str()
                        .is_some_and(|t| t != "00:00" && t != "0:00")
                {
                    expected.insert(id);
                }
                let skater = stats.skaters.iter().find(|r| r.player_id == id);
                let goalie = stats.goalies.iter().find(|r| r.player_id == id);
                if (is_goalie && skater.is_some()) || (!is_goalie && goalie.is_some()) {
                    return Err("official player role disagrees with boxscore".into());
                }
                let abbrev = skater
                    .and_then(|r| r.team_abbrev.as_deref())
                    .or_else(|| goalie.and_then(|r| r.team_abbrev.as_deref()));
                if abbrev.is_some_and(|abbrev| {
                    !abbrev.split(',').any(|candidate| candidate.trim() == team)
                }) {
                    return Err("official player team disagrees with boxscore".into());
                }
                game_teams.insert(id, team.to_owned());
            }
        }
    }
    let actual: std::collections::HashSet<_> = stats
        .skaters
        .iter()
        .map(|r| r.player_id)
        .chain(stats.goalies.iter().map(|r| r.player_id))
        .collect();
    if expected.is_empty() || !expected.is_subset(&actual) || !actual.is_subset(&allowed) {
        return Err("official reports disagree with completed boxscore player inventory".into());
    }
    // Summary teamAbbrevs can include season-wide trades even with a game filter
    // (Guentzel: PIT,CAR in 2023020345). The boxscore identifies the game team.
    for row in &mut stats.skaters {
        row.team_abbrev = game_teams.get(&row.player_id).cloned();
    }
    for row in &mut stats.goalies {
        row.team_abbrev = game_teams.get(&row.player_id).cloned();
    }
    Ok(())
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
) -> Result<OfficialGameStats, crate::AnyError> {
    let summaries: ReportResponse<SkaterSummaryRow> = serde_json::from_str(skater_summary_json)?;
    let realtime: ReportResponse<SkaterRealtimeRow> = serde_json::from_str(skater_realtime_json)?;
    let goalies: ReportResponse<GoalieSummaryRow> = serde_json::from_str(goalie_summary_json)?;
    summaries.validate_count()?;
    realtime.validate_count()?;
    goalies.validate_count()?;
    let summary_ids: std::collections::HashSet<_> =
        summaries.data.iter().map(|r| r.player_id).collect();
    let realtime_ids: std::collections::HashSet<_> =
        realtime.data.iter().map(|r| r.player_id).collect();
    if summary_ids.len() != summaries.data.len()
        || realtime_ids.len() != realtime.data.len()
        || (summaries.data.iter().any(|r| r.season_id >= 20092010) && summary_ids != realtime_ids)
    {
        return Err("incomplete or duplicate official skater reports".into());
    }
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
            goals: row.goals,
            assists: row.assists,
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
    let mut stats = parse_game_stats(
        game_id,
        game_type,
        &skater_summary,
        &skater_realtime,
        &goalie_summary,
    )?;
    let boxscore = fetch_api_json(&format!(
        "https://api-web.nhle.com/v1/gamecenter/{game_id}/boxscore"
    ))
    .await?;
    validate_boxscore(&mut stats, &boxscore)?;
    Ok(stats)
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
            r#"{"data":[{"playerId":2,"seasonId":20252026,"goalieFullName":"A Goalie","teamAbbrevs":"MTL","goals":1,"assists":2,"gamesStarted":1,"wins":1,"shutouts":1,"saves":27}]}"#,
        )
        .unwrap();

        assert_eq!(stats.skaters[0].hits, Some(4));
        assert_eq!(stats.skaters[0].blocked_shots, Some(2));
        assert_eq!(stats.skaters[0].time_on_ice_seconds, Some(902));
        assert_eq!(stats.goalies[0].wins, Some(1));
        assert_eq!(stats.goalies[0].shutouts, Some(1));
        assert_eq!(stats.goalies[0].goals, Some(1));
        assert_eq!(stats.goalies[0].assists, Some(2));
    }

    fn complete_reports() -> [serde_json::Value; 3] {
        use serde_json::json;
        [
            json!({"total":2,"data":[
                {"playerId":1,"seasonId":20252026,"skaterFullName":"Home Skater","teamAbbrevs":"MTL",
                 "goals":0,"assists":0,"shots":0,"ppPoints":0,"shPoints":0,"gameWinningGoals":0,"plusMinus":0},
                {"playerId":3,"seasonId":20252026,"skaterFullName":"Away Skater","teamAbbrevs":"TOR",
                 "goals":0,"assists":0,"shots":0,"ppPoints":0,"shPoints":0,"gameWinningGoals":0,"plusMinus":0}
            ]}),
            json!({"total":2,"data":[{"playerId":1,"hits":0,"blockedShots":0},{"playerId":3,"hits":0,"blockedShots":0}]}),
            json!({"total":2,"data":[
                {"playerId":2,"seasonId":20252026,"goalieFullName":"Home Goalie","teamAbbrevs":"MTL",
                 "goals":0,"assists":0,"wins":0,"shutouts":0,"saves":0},
                {"playerId":4,"seasonId":20252026,"goalieFullName":"Away Goalie","teamAbbrevs":"TOR",
                 "goals":0,"assists":0,"wins":0,"shutouts":0,"saves":0}
            ]}),
        ]
    }

    fn parse_reports(
        reports: &[serde_json::Value; 3],
    ) -> Result<OfficialGameStats, crate::AnyError> {
        parse_game_stats(
            2025020001,
            2,
            &reports[0].to_string(),
            &reports[1].to_string(),
            &reports[2].to_string(),
        )
    }

    fn completed_boxscore() -> serde_json::Value {
        serde_json::json!({
            "id":2025020001,"gameState":"OFF",
            "homeTeam":{"abbrev":"MTL"},"awayTeam":{"abbrev":"TOR"},
            "playerByGameStats":{
                "homeTeam":{"forwards":[{"playerId":1}],"defense":[],"goalies":[{"playerId":2,"toi":"60:00"}]},
                "awayTeam":{"forwards":[{"playerId":3}],"defense":[],"goalies":[{"playerId":4,"toi":"60:00"}]}
            }
        })
    }

    #[test]
    fn complete_inventory_does_not_allow_missing_modern_scoring_categories() {
        let boxscore = completed_boxscore().to_string();
        validate_boxscore(&mut parse_reports(&complete_reports()).unwrap(), &boxscore).unwrap();
        for (report, fields) in [
            (
                0,
                &[
                    "goals",
                    "assists",
                    "shots",
                    "ppPoints",
                    "shPoints",
                    "gameWinningGoals",
                    "plusMinus",
                ][..],
            ),
            (1, &["hits", "blockedShots"][..]),
            (2, &["goals", "assists", "wins", "shutouts", "saves"][..]),
        ] {
            for field in fields {
                let mut reports = complete_reports();
                reports[report]["data"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove(*field);
                let mut stats = parse_reports(&reports).unwrap();
                assert!(
                    validate_boxscore(&mut stats, &boxscore).is_err(),
                    "accepted missing {field}"
                );
            }
        }
    }

    #[test]
    fn historical_unavailable_categories_remain_null() {
        let mut reports = complete_reports();
        for report in [0, 2] {
            for row in reports[report]["data"].as_array_mut().unwrap() {
                row["seasonId"] = serde_json::json!(19901991);
                row.as_object_mut().unwrap().remove("goals");
            }
        }
        reports[1] = serde_json::json!({"total":0,"data":[]});
        let stats = parse_reports(&reports).unwrap();
        stats.validate().unwrap();
        assert_eq!(stats.skaters[0].goals, None);
        assert_eq!(stats.skaters[0].hits, None);
        assert_eq!(stats.goalies[0].goals, None);
    }

    #[test]
    fn rejects_truncated_reports_when_total_is_supplied() {
        for report in 0..3 {
            let mut reports = complete_reports();
            reports[report]["total"] = serde_json::json!(3);
            assert!(parse_reports(&reports).is_err());
        }
    }

    #[test]
    fn boxscore_roles_and_nonmissing_teams_are_required() {
        let boxscore = completed_boxscore();
        let mut stats = parse_reports(&complete_reports()).unwrap();
        stats.skaters[0].team_abbrev = None;
        assert!(validate_boxscore(&mut stats, &boxscore.to_string()).is_err());
        let mut stats = parse_reports(&complete_reports()).unwrap();
        stats.goalies[0].team_abbrev = None;
        assert!(validate_boxscore(&mut stats, &boxscore.to_string()).is_err());
        let mut stats = parse_reports(&complete_reports()).unwrap();
        let mut swapped = boxscore;
        swapped["playerByGameStats"]["homeTeam"]["forwards"][0]["playerId"] = serde_json::json!(2);
        swapped["playerByGameStats"]["homeTeam"]["goalies"][0]["playerId"] = serde_json::json!(1);
        assert!(validate_boxscore(&mut stats, &swapped.to_string()).is_err());
    }

    #[test]
    fn season_team_lists_are_validated_and_resolved_to_the_boxscore_team() {
        let mut stats = parse_reports(&complete_reports()).unwrap();
        stats.skaters[0].team_abbrev = Some("MTL,CAR".into());
        stats.goalies[0].team_abbrev = Some("CAR,MTL".into());
        validate_boxscore(&mut stats, &completed_boxscore().to_string()).unwrap();
        assert_eq!(stats.skaters[0].team_abbrev.as_deref(), Some("MTL"));
        assert_eq!(stats.goalies[0].team_abbrev.as_deref(), Some("MTL"));
        for invalid in ["PIT,CAR", "XMTL,CAR"] {
            stats.skaters[0].team_abbrev = Some(invalid.into());
            assert!(validate_boxscore(&mut stats, &completed_boxscore().to_string()).is_err());
        }
    }
}
