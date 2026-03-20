//! Fetches and transforms play-by-play JSON into typed DB event structs.
use std::collections::HashMap;
use serde::Deserialize;

use crate::{
    api::fetch_api_json,
    models::{DbBlock, DbEvent, DbFaceoff, DbGoal, DbHit, DbPenalty, DbShot},
    AnyError,
};

// ── Play-by-Play deserialization structs ─────────────────────────────────────

/// Top-level play-by-play response from api-web.nhle.com/v1/gamecenter/{id}/play-by-play
#[derive(Deserialize)]
pub struct PlayByPlay {
    pub id: i64,
    #[serde(rename = "homeTeam")]
    pub home_team: PbpTeam,
    #[serde(rename = "awayTeam")]
    pub away_team: PbpTeam,
    pub plays: Vec<Play>,
}

/// Team sub-object in the play-by-play response.
/// Contains the NHL team ID (NOT franchise ID — must translate via team_id_map).
#[derive(Deserialize)]
pub struct PbpTeam {
    pub id: i64,  // NHL team ID — NOT franchise ID
}

/// A single play/event from the plays array.
#[derive(Deserialize)]
pub struct Play {
    #[serde(rename = "eventId")]
    pub event_id: i32,
    #[serde(rename = "periodDescriptor")]
    pub period_descriptor: PeriodDescriptor,
    #[serde(rename = "timeInPeriod")]
    pub time_in_period: String,  // "MM:SS"
    #[serde(rename = "situationCode")]
    pub situation_code: Option<String>,  // "1551", "1541", "0651", etc.
    #[serde(rename = "typeDescKey")]
    pub type_desc_key: String,  // "goal", "shot-on-goal", "hit", "blocked-shot", "penalty", "faceoff"
    pub details: Option<EventDetails>,
}

/// Period descriptor sub-object.
#[derive(Deserialize)]
pub struct PeriodDescriptor {
    pub number: i16,
    #[serde(rename = "periodType")]
    pub period_type: String,  // "REG", "OT", "SO"
}

/// Unified flat struct for all possible event detail fields.
///
/// Uses #[serde(default)] on every optional field so missing JSON keys
/// deserialize to None without error (Pitfall 3 avoidance).
/// All six event types share this single struct.
#[derive(Deserialize)]
pub struct EventDetails {
    #[serde(rename = "xCoord", default)]
    pub x_coord: Option<i16>,
    #[serde(rename = "yCoord", default)]
    pub y_coord: Option<i16>,
    #[serde(rename = "zoneCode", default)]
    pub zone_code: Option<String>,
    #[serde(rename = "eventOwnerTeamId", default)]
    pub event_owner_team_id: Option<i64>,

    // Goal fields
    #[serde(rename = "scoringPlayerId", default)]
    pub scoring_player_id: Option<i64>,
    #[serde(rename = "assist1PlayerId", default)]
    pub assist1_player_id: Option<i64>,
    #[serde(rename = "assist2PlayerId", default)]
    pub assist2_player_id: Option<i64>,
    #[serde(rename = "goalieInNetId", default)]
    pub goalie_in_net_id: Option<i64>,
    #[serde(rename = "shotType", default)]
    pub shot_type: Option<String>,

    // Shot fields
    #[serde(rename = "shootingPlayerId", default)]
    pub shooting_player_id: Option<i64>,

    // Hit fields
    #[serde(rename = "hittingPlayerId", default)]
    pub hitting_player_id: Option<i64>,
    #[serde(rename = "hitteePlayerId", default)]
    pub hittee_player_id: Option<i64>,

    // Blocked-shot fields
    #[serde(rename = "blockingPlayerId", default)]
    pub blocking_player_id: Option<i64>,

    // Penalty fields — CRITICAL: field is "duration" not "durationMinutes"; "descKey" not "description"
    #[serde(rename = "typeCode", default)]
    pub type_code: Option<String>,
    #[serde(rename = "descKey", default)]
    pub desc_key: Option<String>,
    #[serde(default)]
    pub duration: Option<i16>,
    #[serde(rename = "committedByPlayerId", default)]
    pub committed_by_player_id: Option<i64>,
    #[serde(rename = "drawnByPlayerId", default)]
    pub drawn_by_player_id: Option<i64>,

    // Faceoff fields
    #[serde(rename = "winningPlayerId", default)]
    pub winning_player_id: Option<i64>,
    #[serde(rename = "losingPlayerId", default)]
    pub losing_player_id: Option<i64>,
}

// ── situationCode decode ──────────────────────────────────────────────────────

/// Decode a 4-character situationCode into game-state components.
///
/// Format: `[home_goalie][home_skaters][away_skaters][away_goalie]`
/// - home_goalie: '1' = goalie present, '0' = pulled
/// - home_skaters: digit (3-6)
/// - away_skaters: digit (3-6)
/// - away_goalie: '1' = goalie present, '0' = pulled
///
/// strength is derived from skater counts:
/// - home == away: "ev" (even strength)
/// - home > away:  "pp" (home team on power play)
/// - home < away:  "sh" (home team short-handed)
///
/// Returns (home_goalie, home_skaters, away_skaters, away_goalie, strength).
/// Falls back to (true, 5, 5, true, "ev") if code is not exactly 4 bytes.
pub fn decode_situation_code(code: &str) -> (bool, i16, i16, bool, String) {
    if code.len() != 4 {
        return (true, 5, 5, true, "ev".to_string());
    }
    let bytes = code.as_bytes();
    let home_goalie  = bytes[0] == b'1';
    let home_skaters = (bytes[1] - b'0') as i16;
    let away_skaters = (bytes[2] - b'0') as i16;
    let away_goalie  = bytes[3] == b'1';
    let strength = match home_skaters.cmp(&away_skaters) {
        std::cmp::Ordering::Equal   => "ev".to_string(),
        std::cmp::Ordering::Greater => "pp".to_string(),
        std::cmp::Ordering::Less    => "sh".to_string(),
    };
    (home_goalie, home_skaters, away_skaters, away_goalie, strength)
}

// ── Fetch ─────────────────────────────────────────────────────────────────────

/// Fetch play-by-play JSON for a single game and deserialize it.
///
/// URL: `https://api-web.nhle.com/v1/gamecenter/{game_id}/play-by-play`
pub async fn fetch_play_by_play(game_id: i64) -> Result<PlayByPlay, AnyError> {
    let url = format!("https://api-web.nhle.com/v1/gamecenter/{game_id}/play-by-play");
    let json = fetch_api_json(&url).await?;
    let pbp: PlayByPlay = serde_json::from_str(&json)?;
    Ok(pbp)
}

// ── Transform ─────────────────────────────────────────────────────────────────

/// Transform a PlayByPlay response into typed Db model vectors.
///
/// Single pass over pbp.plays — classifies each event by typeDescKey and
/// populates all seven return vectors.
///
/// Shootout events (periodType == "SO") are skipped per REQUIREMENTS.md.
/// Events with a recognized typeDescKey but missing details are skipped with a
/// warning string collected in the returned skip_warnings vector.
///
/// eventOwnerTeamId is translated from NHL team ID to franchise ID via
/// team_id_map. Unmapped team IDs store as NULL (nullable column).
///
/// Returns (events, goals, shots, hits, blocks, penalties, faceoffs, skip_warnings).
#[allow(clippy::type_complexity)]
pub fn transform_events(
    pbp: &PlayByPlay,
    team_id_map: &HashMap<i64, i64>,
) -> (
    Vec<DbEvent>,
    Vec<DbGoal>,
    Vec<DbShot>,
    Vec<DbHit>,
    Vec<DbBlock>,
    Vec<DbPenalty>,
    Vec<DbFaceoff>,
    Vec<String>,
) {
    let game_id = pbp.id;
    let mut events   = Vec::new();
    let mut goals    = Vec::new();
    let mut shots    = Vec::new();
    let mut hits     = Vec::new();
    let mut blocks   = Vec::new();
    let mut penalties= Vec::new();
    let mut faceoffs = Vec::new();
    let mut skip_warnings = Vec::new();

    for play in &pbp.plays {
        // Skip shootout events — out of scope per REQUIREMENTS.md
        if play.period_descriptor.period_type == "SO" {
            continue;
        }

        // Decode situationCode (default to even strength when absent)
        let code = play.situation_code.as_deref().unwrap_or("");
        let (home_goalie, home_sk, away_sk, away_goalie, strength) =
            decode_situation_code(code);

        // Translate event_owner_team_id from NHL team ID to franchise ID
        let event_owner_team_id: Option<i64> = play
            .details
            .as_ref()
            .and_then(|d| d.event_owner_team_id)
            .and_then(|tid| team_id_map.get(&tid).copied());

        // Extract coordinates and zone from details (all Optional)
        let (x_coord, y_coord, zone_code) = play.details.as_ref().map_or(
            (None, None, None),
            |d| (d.x_coord, d.y_coord, d.zone_code.clone()),
        );

        let base = DbEvent {
            game_id,
            event_id_in_game: play.event_id,
            period: play.period_descriptor.number,
            period_type: play.period_descriptor.period_type.clone(),
            time_in_period: play.time_in_period.clone(),
            event_type: play.type_desc_key.clone(),
            x_coord,
            y_coord,
            zone_code,
            event_owner_team_id,
            home_goalie_present: home_goalie,
            home_skater_count: home_sk,
            away_skater_count: away_sk,
            away_goalie_present: away_goalie,
            strength,
        };
        events.push(base);

        // Classify into child type vectors
        match play.type_desc_key.as_str() {
            "goal" => {
                match &play.details {
                    None => {
                        skip_warnings.push(format!(
                            "skip: goal event {} in game {} has no details",
                            play.event_id, game_id
                        ));
                    }
                    Some(d) => {
                        goals.push(DbGoal {
                            event_id_in_game: play.event_id,
                            scorer_player_id: d.scoring_player_id,
                            assist1_player_id: d.assist1_player_id,
                            assist2_player_id: d.assist2_player_id,
                            goalie_id: d.goalie_in_net_id,
                            shot_type: d.shot_type.clone(),
                        });
                        // Goals are also shots on net — double-insert into shots vector.
                        // Map: d.scoring_player_id → shooting_player_id (NHL API uses different key for goal events).
                        // Do NOT use d.shooting_player_id here — it deserializes from "shootingPlayerId" which is
                        // absent in goal events and will always be None.
                        shots.push(DbShot {
                            event_id_in_game: play.event_id,
                            shooting_player_id: d.scoring_player_id,
                            goalie_in_net_id: d.goalie_in_net_id,
                            shot_type: d.shot_type.clone(),
                        });
                    }
                }
            }
            "shot-on-goal" => {
                match &play.details {
                    None => {
                        skip_warnings.push(format!(
                            "skip: shot-on-goal event {} in game {} has no details",
                            play.event_id, game_id
                        ));
                    }
                    Some(d) => {
                        shots.push(DbShot {
                            event_id_in_game: play.event_id,
                            shooting_player_id: d.shooting_player_id,
                            goalie_in_net_id: d.goalie_in_net_id,
                            shot_type: d.shot_type.clone(),
                        });
                    }
                }
            }
            "hit" => {
                match &play.details {
                    None => {
                        skip_warnings.push(format!(
                            "skip: hit event {} in game {} has no details",
                            play.event_id, game_id
                        ));
                    }
                    Some(d) => {
                        hits.push(DbHit {
                            event_id_in_game: play.event_id,
                            hitting_player_id: d.hitting_player_id,
                            hittee_player_id: d.hittee_player_id,
                        });
                    }
                }
            }
            "blocked-shot" => {
                match &play.details {
                    None => {
                        skip_warnings.push(format!(
                            "skip: blocked-shot event {} in game {} has no details",
                            play.event_id, game_id
                        ));
                    }
                    Some(d) => {
                        blocks.push(DbBlock {
                            event_id_in_game: play.event_id,
                            blocking_player_id: d.blocking_player_id,
                            shooting_player_id: d.shooting_player_id,
                        });
                    }
                }
            }
            "penalty" => {
                match &play.details {
                    None => {
                        skip_warnings.push(format!(
                            "skip: penalty event {} in game {} has no details",
                            play.event_id, game_id
                        ));
                    }
                    Some(d) => {
                        penalties.push(DbPenalty {
                            event_id_in_game: play.event_id,
                            committed_by_player_id: d.committed_by_player_id,
                            drawn_by_player_id: d.drawn_by_player_id,
                            infraction_type: d.desc_key.clone(),
                            duration_minutes: d.duration,
                        });
                    }
                }
            }
            "faceoff" => {
                match &play.details {
                    None => {
                        skip_warnings.push(format!(
                            "skip: faceoff event {} in game {} has no details",
                            play.event_id, game_id
                        ));
                    }
                    Some(d) => {
                        faceoffs.push(DbFaceoff {
                            event_id_in_game: play.event_id,
                            winning_player_id: d.winning_player_id,
                            losing_player_id: d.losing_player_id,
                        });
                    }
                }
            }
            _ => {
                // Unknown or untracked event type (stoppage, period-start, missed-shot, etc.)
                // These are stored in the base events table but have no child record.
            }
        }
    }

    (events, goals, shots, hits, blocks, penalties, faceoffs, skip_warnings)
}
