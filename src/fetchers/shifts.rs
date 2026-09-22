//! Fetches NHL shift-chart rows and converts source fields to typed values.

use serde::Deserialize;
use serde_json::Value;

use crate::{models::DbShift, AnyError};

pub const SHIFT_TYPE_CODE: i64 = 517;

#[derive(Debug, Deserialize)]
struct ShiftChartResponse {
    #[serde(default)]
    data: Vec<Value>,
}

/// Fetch every typeCode 517 row for a game. No interval correction,
/// deduplication, team translation, or anomaly filtering is performed.
pub async fn fetch_game_shifts(game_id: i64) -> Result<Vec<DbShift>, AnyError> {
    let url = format!(
        "https://api.nhle.com/stats/rest/en/shiftcharts?limit=-1&cayenneExp=gameId={game_id}"
    );
    let mut last_error = None;
    let mut json = None;
    for delay_ms in [0_u64, 250, 1_000, 3_000] {
        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
        match crate::api::fetch_api_json(&url).await {
            Ok(body) => {
                json = Some(body);
                break;
            }
            Err(error @ crate::api::ApiError::NotFound) => return Err(error.into()),
            Err(error) => last_error = Some(error),
        }
    }
    let json = match json {
        Some(json) => json,
        None => match last_error {
            Some(error) => return Err(error.into()),
            None => return Err("shift chart fetch failed without an error".into()),
        },
    };
    let response: ShiftChartResponse = serde_json::from_str(&json)?;

    response
        .data
        .into_iter()
        .filter(|row| row.get("typeCode").and_then(Value::as_i64) == Some(SHIFT_TYPE_CODE))
        .map(raw_shift)
        .collect()
}

fn raw_shift(source_data: Value) -> Result<DbShift, AnyError> {
    let integer = |name: &str| source_data.get(name).and_then(Value::as_i64);
    let string = |name: &str| {
        source_data
            .get(name)
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    };

    let source_shift_id =
        integer("id").ok_or_else(|| "typeCode 517 row has no integer id".to_string())?;
    let game_id = integer("gameId")
        .ok_or_else(|| format!("shift row {source_shift_id} has no integer gameId"))?;
    let start_time = string("startTime");
    let end_time = string("endTime");
    let duration = string("duration");

    Ok(DbShift {
        source_shift_id,
        game_id,
        type_code: SHIFT_TYPE_CODE as i32,
        player_id: integer("playerId"),
        team_id: integer("teamId"),
        period: integer("period").and_then(|value| i16::try_from(value).ok()),
        shift_number: integer("shiftNumber").and_then(|value| i32::try_from(value).ok()),
        start_time_seconds: start_time.as_deref().and_then(parse_clock_seconds),
        end_time_seconds: end_time.as_deref().and_then(parse_clock_seconds),
        duration_seconds: duration.as_deref().and_then(parse_clock_seconds),
        start_time,
        end_time,
        duration,
        source_data,
    })
}

fn parse_clock_seconds(value: &str) -> Option<i32> {
    let (minutes, seconds) = value.split_once(':')?;
    let minutes: i32 = minutes.parse().ok()?;
    let seconds: i32 = seconds.parse().ok()?;
    (minutes >= 0 && (0..60).contains(&seconds)).then_some(minutes * 60 + seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_fields_but_preserves_an_incoherent_source_row() {
        let source = serde_json::json!({
            "id": 10,
            "typeCode": 517,
            "gameId": 2025020001_i64,
            "playerId": 8473419,
            "teamId": 13,
            "period": 1,
            "shiftNumber": 4,
            "startTime": "00:30",
            "endTime": "00:20",
            "duration": "not-a-clock",
            "futureField": {"preserved": true}
        });

        let shift = raw_shift(source.clone()).unwrap();
        assert_eq!(shift.player_id, Some(8473419));
        assert_eq!(shift.start_time_seconds, Some(30));
        assert_eq!(shift.end_time_seconds, Some(20));
        assert_eq!(shift.duration_seconds, None);
        assert_eq!(shift.source_data, source);
    }

    #[test]
    fn parses_valid_clocks_without_reconciling_them() {
        assert_eq!(parse_clock_seconds("20:00"), Some(1200));
        assert_eq!(parse_clock_seconds("01:60"), None);
        assert_eq!(parse_clock_seconds("bad"), None);
    }
}
