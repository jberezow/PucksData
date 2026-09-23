//! NHL elapsed period clocks. Period identity is never discarded at a boundary.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Clock {
    pub period: i16,
    pub period_seconds: i32,
    pub game_seconds: i32,
}

/// Parse a source MM:SS clock without rounding, saturation or overflow.
/// Canonical parsed columns are checked against this interpretation, never rewritten.
pub fn parse_seconds(value: &str) -> Option<i32> {
    let (minutes, seconds) = value.split_once(':')?;
    if minutes.is_empty()
        || seconds.len() != 2
        || !minutes.bytes().all(|b| b.is_ascii_digit())
        || !seconds.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let minutes: i32 = minutes.parse().ok()?;
    let seconds: i32 = seconds.parse().ok()?;
    if seconds >= 60 {
        return None;
    }
    minutes.checked_mul(60)?.checked_add(seconds)
}

/// Timed periods only. Regular-season shootouts are not an overtime period.
pub fn period_length(game_type: i16, period: i16) -> Option<i32> {
    match (game_type, period) {
        (2 | 3, 1..=3) => Some(1200),
        (2, 4) => Some(300),
        (3, 4..) => Some(1200),
        _ => None,
    }
}

pub fn clock(game_type: i16, period: i16, seconds: i32) -> Option<Clock> {
    let length = period_length(game_type, period)?;
    if !(0..=length).contains(&seconds) {
        return None;
    }
    // Prior periods in either supported competition are always twenty minutes.
    Some(Clock {
        period,
        period_seconds: seconds,
        game_seconds: (i32::from(period) - 1) * 1200 + seconds,
    })
}

pub fn event_clock(game_type: i16, period: i16, period_type: &str, text: &str) -> Option<Clock> {
    if !matches!((period, period_type), (1..=3, "REG") | (4.., "OT")) {
        return None;
    }
    clock(game_type, period, parse_seconds(text)?)
}

/// Positive-duration intersection, never across a period boundary.
pub fn intersection(a: (i16, i32, i32), b: (i16, i32, i32)) -> i32 {
    if a.0 != b.0 {
        return 0;
    }
    (a.2.min(b.2) - a.1.max(b.1)).max(0)
}
