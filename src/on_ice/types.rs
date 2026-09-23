use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const METHOD_VERSION: &str = "nhl-on-ice-v1";
pub const FIRST_SEASON: i32 = 20102011;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Game {
    pub game_id: i64,
    pub season: i32,
    pub game_type: i16,
    pub game_date: String,
    pub game_state: Option<String>,
    pub home_team_id: i64,
    pub away_team_id: i64,
    pub fetch_status: Option<String>,
    pub attempted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Shift {
    pub game_id: i64,
    pub source_shift_id: i64,
    pub type_code: i32,
    pub player_id: Option<i64>,
    pub nhl_team_id: Option<i64>,
    pub franchise_id: Option<i64>,
    pub player_position: Option<String>,
    pub period: Option<i16>,
    pub shift_number: Option<i32>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub duration: Option<String>,
    pub start_time_seconds: Option<i32>,
    pub end_time_seconds: Option<i32>,
    pub duration_seconds: Option<i32>,
    pub ingested_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Event {
    pub game_id: i64,
    pub id: i64,
    pub event_id_in_game: i32,
    pub period: i16,
    pub period_type: String,
    pub time_in_period: String,
    pub event_type: String,
    pub situation_code: Option<String>,
    pub strength_source: String,
    pub away_goalie_present: Option<bool>,
    pub away_skater_count: Option<i16>,
    pub home_skater_count: Option<i16>,
    pub home_goalie_present: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OfficialToi {
    pub game_id: i64,
    pub player_id: i64,
    pub player_type: String,
    pub team_abbrev: Option<String>,
    pub position_code: Option<String>,
    pub time_on_ice_seconds: Option<i64>,
    pub source_revision: i32,
    pub source_observed_at: String,
}

/// Replayable analysis inputs, not another canonical ingestion format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSource {
    pub game: Game,
    pub shifts: Vec<Shift>,
    pub events: Vec<Event>,
    pub official_toi: Vec<OfficialToi>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Skater,
    Goalie,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueCode {
    WrongGame,
    UnexpectedType,
    MissingPlayerId,
    InvalidPlayerId,
    MissingTeamId,
    UnknownTeamId,
    UnexpectedTeam,
    MissingPeriod,
    UnexpectedPeriod,
    MalformedStart,
    MalformedEnd,
    MalformedDuration,
    MissingDuration,
    StoredClockDisagreement,
    ReversedInterval,
    OutOfBounds,
    ZeroDuration,
    DurationMismatch,
    DuplicateInterval,
    SamePlayerOverlap,
    SuspiciousLongShift,
    SuspiciousShiftCount,
    UnknownPlayerRole,
    ConflictingPlayerRole,
    PlayerMultipleTeams,
    MissingTeamShifts,
    MissingPeriodShifts,
    ShiftPeriodWithoutEvents,
    SuspiciousOnIceCount,
    ConflictingOfficialRecords,
    InvalidOfficialToi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub code: IssueCode,
    pub severity: String,
    pub source_shift_ids: Vec<i64>,
    pub player_id: Option<i64>,
    pub period: Option<i16>,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct Interval {
    pub source_id: i64,
    pub player_id: i64,
    pub team_id: i64,
    pub role: Role,
    pub period: i16,
    pub start: i32,
    pub end: i32,
    pub flags: Vec<IssueCode>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Side {
    pub skaters: Vec<i64>,
    pub goalies: Vec<i64>,
    pub unknown: Vec<i64>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lineup {
    pub home: Side,
    pub away: Side,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconstructionStatus {
    Resolved,
    Ambiguous,
    Incomplete,
    AmbiguousIncomplete,
    InvalidEventTime,
    Unsupported,
    NoShifts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Agreement {
    Exact,
    BoundaryCompatible,
    Mismatch,
    Unavailable,
    InvalidCode,
    NotComparable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub away_skaters: i16,
    pub away_goalie: bool,
    pub home_skaters: i16,
    pub home_goalie: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLineup {
    pub game_id: i64,
    pub event_id: i64,
    pub event_id_in_game: i32,
    pub event_type: String,
    pub period: i16,
    pub time_in_period: String,
    pub clock: Option<super::clock::Clock>,
    pub status: ReconstructionStatus,
    pub definite: Lineup,
    pub possible: Lineup,
    /// Limits on either side of the timestamp, NOT exhaustive event orderings.
    pub before: Lineup,
    pub after: Lineup,
    pub source_shift_ids: Vec<i64>,
    pub source_flags: Vec<IssueCode>,
    pub incomplete_reasons: Vec<String>,
    pub contexts: Vec<String>,
    pub expected_counts: Option<Counts>,
    pub situation_agreement: Agreement,
    pub mismatch_fields: Vec<String>,
    pub decoded_fields_agree: Option<bool>,
    pub before_counts_match: Option<bool>,
    pub after_counts_match: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToiCategory {
    Exact,
    WithinOneSecond,
    WithinFiveSeconds,
    WithinThirtySeconds,
    LargeDifference,
    MissingOfficial,
    MissingShifts,
    OfficialZeroNoShifts,
    InvalidIntervals,
    PartialIntervals,
    ConflictingOfficial,
    InvalidOfficial,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToiComparison {
    pub player_id: i64,
    pub role: Role,
    pub source_rows: usize,
    pub accepted_rows: usize,
    pub rejected_rows: usize,
    pub interval_sum_seconds: Option<i64>,
    pub interval_union_seconds: Option<i64>,
    pub overlap_excess_seconds: Option<i64>,
    pub source_duration_sum_seconds: Option<i64>,
    pub unparseable_duration_rows: usize,
    pub official_seconds: Option<i64>,
    /// Unioned interval time minus official time. Null when not comparable.
    pub difference_seconds: Option<i64>,
    pub category: ToiCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameReport {
    pub method: String,
    pub source_sha256: String,
    pub game: Game,
    pub supported: bool,
    pub source_rows: usize,
    pub accepted_rows: usize,
    pub rejected_rows: usize,
    pub role_fallback_players: usize,
    pub validation: Vec<Issue>,
    pub validation_counts: BTreeMap<IssueCode, usize>,
    pub toi: Vec<ToiComparison>,
    pub events: Vec<EventLineup>,
}
