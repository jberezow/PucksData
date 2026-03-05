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
