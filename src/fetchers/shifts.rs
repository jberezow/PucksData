//! Fetch NHL shift-chart rows as typed source fields, without canonicalization.

use serde::Deserialize;
use serde_json::Value;

use crate::{models::DbShift, AnyError};

pub const SHIFT_TYPE_CODE: i32 = 517;

#[derive(Debug, Deserialize)]
struct ShiftChartResponse {
    data: Vec<Value>,
    total: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceShift {
    id: i64,
    game_id: i64,
    type_code: i32,
    player_id: Option<i64>,
    team_id: Option<i64>,
    period: Option<i16>,
    shift_number: Option<i32>,
    event_number: Option<i32>,
    detail_code: Option<i32>,
    event_description: Option<String>,
    event_details: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    duration: Option<String>,
}

/// Fetch all shift rows for a game. Source inconsistencies are preserved;
/// malformed responses fail before replacing any previously stored rows.
pub async fn fetch_game_shifts(game_id: i64) -> Result<Vec<DbShift>, AnyError> {
    let url = format!(
        "https://api.nhle.com/stats/rest/en/shiftcharts?limit=-1&cayenneExp=gameId={game_id}"
    );
    let mut last_error = None;
    for delay_ms in [0_u64, 250, 1_000, 3_000] {
        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
        match crate::api::fetch_api_json(&url).await {
            Ok(body) => return parse_shift_chart(&body, game_id),
            Err(error @ crate::api::ApiError::NotFound) => return Err(error.into()),
            Err(error) => last_error = Some(error),
        }
    }
    match last_error {
        Some(error) => Err(error.into()),
        None => Err("shift chart fetch failed without an error".into()),
    }
}

fn parse_shift_chart(body: &str, game_id: i64) -> Result<Vec<DbShift>, AnyError> {
    let response: ShiftChartResponse = serde_json::from_str(body)?;
    if response.total != response.data.len() {
        return Err(format!(
            "incomplete shift response for game {game_id}: {} of {} rows",
            response.data.len(),
            response.total
        )
        .into());
    }
    let mut shifts = Vec::new();
    for row in response.data {
        let code = row
            .get("typeCode")
            .and_then(Value::as_i64)
            .ok_or("shift-chart row has no integer typeCode")?;
        if code != i64::from(SHIFT_TYPE_CODE) {
            continue;
        }
        let row: SourceShift = serde_json::from_value(row)?;
        if row.game_id != game_id {
            return Err(format!("shift {} belongs to a different game", row.id).into());
        }
        shifts.push(DbShift {
            source_shift_id: row.id,
            game_id: row.game_id,
            type_code: row.type_code,
            player_id: row.player_id,
            team_id: row.team_id,
            period: row.period,
            shift_number: row.shift_number,
            event_number: row.event_number,
            detail_code: row.detail_code,
            event_description: row.event_description,
            event_details: row.event_details,
            start_time_seconds: row.start_time.as_deref().and_then(parse_clock_seconds),
            end_time_seconds: row.end_time.as_deref().and_then(parse_clock_seconds),
            duration_seconds: row.duration.as_deref().and_then(parse_clock_seconds),
            start_time: row.start_time,
            end_time: row.end_time,
            duration: row.duration,
        });
    }
    Ok(shifts)
}

fn parse_clock_seconds(value: &str) -> Option<i32> {
    let (minutes, seconds) = value.split_once(':')?;
    let minutes: i32 = minutes.parse().ok()?;
    let seconds: i32 = seconds.parse().ok()?;
    if minutes < 0 || !(0..60).contains(&seconds) {
        return None;
    }
    minutes.checked_mul(60)?.checked_add(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const GAME: i64 = 2025020001;

    fn row() -> Value {
        json!({"id":10,"typeCode":517,"gameId":GAME,"playerId":8473419,
            "teamId":13,"period":1,"shiftNumber":4,"eventNumber":105,
            "detailCode":0,"startTime":"00:30","endTime":"00:20",
            "duration":"bad","eventDescription":"source note","eventDetails":"detail"})
    }

    fn response(rows: Vec<Value>) -> String {
        json!({"total":rows.len(),"data":rows}).to_string()
    }

    #[test]
    fn preserves_incoherent_intervals_and_distinct_source_rows() {
        let mut duplicate_interval = row();
        duplicate_interval["id"] = json!(11);
        let rows = parse_shift_chart(
            &response(vec![row(), duplicate_interval, json!({"typeCode":505})]),
            GAME,
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].player_id, Some(8473419));
        assert_eq!(rows[0].event_number, Some(105));
        assert_eq!(rows[0].detail_code, Some(0));
        assert_eq!(rows[0].event_description.as_deref(), Some("source note"));
        assert_eq!(rows[0].event_details.as_deref(), Some("detail"));
        assert_eq!(rows[0].start_time_seconds, Some(30));
        assert_eq!(rows[0].end_time_seconds, Some(20));
        assert_eq!(rows[0].duration_seconds, None);
        assert_eq!(rows[0].duration.as_deref(), Some("bad"));
    }

    #[test]
    fn rejects_incomplete_or_malformed_responses() {
        assert!(parse_shift_chart(r#"{"total":1,"data":[]}"#, GAME).is_err());
        assert!(parse_shift_chart(r#"{"total":0}"#, GAME).is_err());
        for (field, value) in [
            ("playerId", json!("wrong type")),
            ("period", json!(100000)),
            ("typeCode", Value::Null),
            ("gameId", json!(GAME + 1)),
        ] {
            let mut bad = row();
            bad[field] = value;
            assert!(parse_shift_chart(&response(vec![bad]), GAME).is_err());
        }
    }

    #[test]
    fn nullable_source_fields_remain_null() {
        let shifts = parse_shift_chart(
            &response(vec![json!({"id":1,"gameId":GAME,"typeCode":517})]),
            GAME,
        )
        .unwrap();
        assert_eq!(shifts[0].player_id, None);
        assert_eq!(shifts[0].event_number, None);
        assert_eq!(shifts[0].start_time_seconds, None);
    }

    #[test]
    fn parses_clocks_without_overflow() {
        assert_eq!(parse_clock_seconds("20:00"), Some(1200));
        for invalid in ["01:60", "bad", "2147483647:00", "-1:20"] {
            assert_eq!(parse_clock_seconds(invalid), None);
        }
    }
}
