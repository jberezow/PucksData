//! Plain-Rust DB model structs that map 1-to-1 to database table columns.
use time::Date;
use chrono::DateTime;
use chrono::Utc;

pub struct DbTeam {
    pub team_id: i64,
    pub full_name: String,
    pub common_name: String,
    pub place_name: String,
    pub abbrev: String,
}

pub struct DbSeason {
    pub season_year: i32,
    pub start_date: Option<Date>,
    pub end_date: Option<Date>,
    pub regular_season_end_date: Option<Date>,
}

pub struct DbPlayer {
    pub player_id: i64,
    pub first_name: String,
    pub last_name: String,
    pub position: Option<String>,
    pub shoots_catches: Option<String>,
    pub current_team_abbrev: Option<String>,
    pub birth_date: Option<Date>,
    pub height_cm: Option<i16>,
    pub weight_kg: Option<i16>,
    pub draft_year: Option<i16>,
    pub draft_round: Option<i16>,
    pub draft_pick: Option<i16>,
    pub draft_team_abbrev: Option<String>,
    pub draft_overall_pick: Option<i16>,
}

pub struct DbGame {
    pub game_id: i64,
    pub season: i32,
    pub game_date: Date,
    pub start_time_utc: Option<DateTime<Utc>>,
    pub home_team_id: i64,
    pub away_team_id: i64,
    pub game_type: i16,
    pub venue: Option<String>,
    pub venue_location: Option<String>,
    pub game_state: Option<String>,
    pub home_score: Option<i16>,
    pub away_score: Option<i16>,
}

// ── Event model structs ───────────────────────────────────────────────────────

/// Base event row — maps to the `events` table.
pub struct DbEvent {
    pub game_id: i64,
    pub event_id_in_game: i32,
    pub period: i16,
    pub period_type: String,
    pub time_in_period: String,
    pub event_type: String,
    pub x_coord: Option<i16>,
    pub y_coord: Option<i16>,
    pub zone_code: Option<String>,
    pub event_owner_team_id: Option<i64>,  // franchise ID after translation
    pub home_goalie_present: bool,
    pub home_skater_count: i16,
    pub away_skater_count: i16,
    pub away_goalie_present: bool,
    pub strength: String,
}

/// Goal child row — maps to the `goals` table.
pub struct DbGoal {
    pub event_id_in_game: i32,  // used to look up events(id) after base insert
    pub scorer_player_id: Option<i64>,
    pub assist1_player_id: Option<i64>,
    pub assist2_player_id: Option<i64>,
    pub goalie_id: Option<i64>,
    pub shot_type: Option<String>,
}

/// Shot-on-goal child row — maps to the `shots` table.
pub struct DbShot {
    pub event_id_in_game: i32,
    pub shooting_player_id: Option<i64>,
    pub goalie_in_net_id: Option<i64>,
    pub shot_type: Option<String>,
}

/// Hit child row — maps to the `hits` table.
pub struct DbHit {
    pub event_id_in_game: i32,
    pub hitting_player_id: Option<i64>,
    pub hittee_player_id: Option<i64>,
}

/// Blocked-shot child row — maps to the `blocks` table.
pub struct DbBlock {
    pub event_id_in_game: i32,
    pub blocking_player_id: Option<i64>,
    pub shooting_player_id: Option<i64>,
}

/// Penalty child row — maps to the `penalties` table.
pub struct DbPenalty {
    pub event_id_in_game: i32,
    pub committed_by_player_id: Option<i64>,
    pub drawn_by_player_id: Option<i64>,
    pub infraction_type: Option<String>,
    pub duration_minutes: Option<i16>,
}

/// Faceoff child row — maps to the `faceoffs` table.
pub struct DbFaceoff {
    pub event_id_in_game: i32,
    pub winning_player_id: Option<i64>,
    pub losing_player_id: Option<i64>,
}
