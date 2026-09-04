//! Parses the NHL's archived HTML play-by-play reports used before the JSON
//! feed exposed complete event and manpower data.

use std::collections::{HashMap, VecDeque};

use scraper::{ElementRef, Html, Selector};

use crate::{
    api::fetch_api_text,
    fetchers::events::{EventStrength, PlayByPlay},
    AnyError,
};

/// A normalized event from an NHL HTML play-by-play report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportEvent {
    pub report_event_id: i32,
    pub period: i16,
    pub time_in_period: String,
    pub event_type: String,
    pub team_abbrev: Option<String>,
    pub strength: Option<EventStrength>,
    pub description: String,
}

/// The NHL report archive is required for the four seasons after the 2005
/// lockout and before JSON situation codes begin in 2009-10.
pub fn is_supported_game(game_id: i64) -> bool {
    matches!(game_id / 1_000_000, 2005..=2008)
}

/// Build the official archived play-by-play report URL for an NHL game ID.
pub fn report_url(game_id: i64) -> Option<String> {
    let season_start = game_id / 1_000_000;
    if !(1900..=2200).contains(&season_start) {
        return None;
    }
    let season = format!("{season_start}{}", season_start + 1);
    let report_id = game_id % 1_000_000;
    Some(format!(
        "https://www.nhl.com/scores/htmlreports/{season}/PL{report_id:06}.HTM"
    ))
}

/// Fetch and parse an archived NHL play-by-play report.
pub async fn fetch_report_events(game_id: i64) -> Result<Vec<ReportEvent>, AnyError> {
    let url = report_url(game_id)
        .ok_or_else(|| format!("historical NHL report is not required for game {game_id}"))?;
    let html = fetch_api_text(&url).await?;
    let events = parse_report(&html);
    if events.is_empty() {
        return Err(format!("NHL historical report for game {game_id} contained no events").into());
    }
    Ok(events)
}

/// Result of aligning archived report rows with JSON events.
#[derive(Debug, Default)]
pub struct ReportReconciliation {
    pub strengths: HashMap<i32, EventStrength>,
    pub report_available: bool,
    pub matched: usize,
    pub unmatched_json: usize,
    pub unmatched_report: usize,
}

/// Fetch and reconcile the archived report when it exists.
///
/// Some otherwise valid historical games have no archived report. A 404 is
/// therefore treated as an unavailable optional source, while network,
/// parsing, and reconciliation failures remain errors.
pub async fn fetch_reconciled_strengths(
    pbp: &PlayByPlay,
) -> Result<ReportReconciliation, AnyError> {
    if !is_supported_game(pbp.id) {
        return Ok(ReportReconciliation::default());
    }

    let report_events = match fetch_report_events(pbp.id).await {
        Ok(events) => events,
        Err(error)
            if error
                .downcast_ref::<crate::api::ApiError>()
                .is_some_and(|error| matches!(error, crate::api::ApiError::NotFound)) =>
        {
            return Ok(ReportReconciliation::default());
        }
        Err(error) => return Err(error),
    };

    let reconciliation = reconcile_report_strengths(pbp, &report_events);
    if reconciliation.matched == 0 && !pbp.plays.is_empty() {
        return Err(format!(
            "NHL historical report for game {} did not match any JSON events",
            pbp.id
        )
        .into());
    }
    Ok(reconciliation)
}

/// Align report events to JSON events by period, clock, event type, and order.
///
/// Historical JSON IDs and report row IDs use different number spaces. The
/// feeds retain the same event ordering, so a queue per normalized event key
/// resolves repeated timestamps deterministically.
///
/// Report strength is stated from one team's perspective and is inverted when
/// JSON assigns event ownership to the other side. Which team the report means
/// is per event type: see [`report_strength_names_opponent`] for blocked shots
/// and [`report_strength_is_usable`] for the types that are skipped entirely.
pub fn reconcile_report_strengths(
    pbp: &PlayByPlay,
    report_events: &[ReportEvent],
) -> ReportReconciliation {
    type EventKey = (i16, String, String);

    let mut report_by_key: HashMap<EventKey, VecDeque<&ReportEvent>> = HashMap::new();
    for event in report_events
        .iter()
        .filter(|event| event.strength.is_some() && report_strength_is_usable(&event.event_type))
    {
        report_by_key
            .entry((
                event.period,
                event.time_in_period.clone(),
                event.event_type.clone(),
            ))
            .or_default()
            .push_back(event);
    }

    let mut result = ReportReconciliation {
        report_available: true,
        ..ReportReconciliation::default()
    };
    for play in &pbp.plays {
        if !report_strength_is_usable(&play.type_desc_key) {
            continue;
        }
        let key = (
            play.period_descriptor.number,
            play.time_in_period.clone(),
            play.type_desc_key.clone(),
        );
        let Some(candidates) = report_by_key.get_mut(&key) else {
            if play.situation_code.is_none() {
                result.unmatched_json += 1;
            }
            continue;
        };
        let Some(report_event) = candidates.pop_front() else {
            if play.situation_code.is_none() {
                result.unmatched_json += 1;
            }
            continue;
        };

        let report_strength = report_event.strength.expect("filtered above");
        let owner_team_id = play
            .details
            .as_ref()
            .and_then(|details| details.event_owner_team_id);
        let owner_is_home = match owner_team_id {
            Some(team_id) if team_id == pbp.home_team.id => Some(true),
            Some(team_id) if team_id == pbp.away_team.id => Some(false),
            _ => None,
        };
        let report_is_home = report_event
            .team_abbrev
            .as_deref()
            .and_then(|abbrev| {
                team_side(
                    abbrev,
                    pbp.home_team.abbrev.as_deref(),
                    pbp.away_team.abbrev.as_deref(),
                )
            })
            .map(|is_home| is_home != report_strength_names_opponent(&play.type_desc_key));

        let owner_strength = match (report_strength, owner_is_home, report_is_home) {
            (EventStrength::Even, _, _) => Some(EventStrength::Even),
            (strength, Some(owner_home), Some(report_home)) => Some(if owner_home == report_home {
                strength
            } else {
                strength.inverted()
            }),
            _ => None,
        };

        if let Some(owner_strength) = owner_strength {
            result.matched += 1;
            result.strengths.insert(play.event_id, owner_strength);
        } else {
            result.unmatched_json += 1;
        }
    }

    result.unmatched_report = report_by_key.values().map(VecDeque::len).sum();
    result
}

/// Whether a report row's strength column can be trusted for an event type.
///
/// Penalty rows state the manpower *before* the penalty is applied, and show
/// even strength for coincidental majors. Measured against 2009-10 situation
/// codes over five games, penalty rows disagreed with the authoritative
/// situation code on 13 of 71 events (18.3%), so they are not used. Every
/// other retained type agrees within about 2%.
fn report_strength_is_usable(event_type: &str) -> bool {
    event_type != "penalty"
}

/// Whether a report row's strength belongs to the team opposite the one its
/// description names first.
///
/// A blocked-shot row reads `SHOOTER BLOCKED BY BLOCKER`, so the description
/// names the shooting team, which is also the team the JSON feed assigns
/// ownership to. Its strength column, however, is stated from the blocking
/// team's perspective. Measured against 2009-10 situation codes over five
/// games, flipping the side for blocked shots reduced their error rate from
/// 24.8% (38 of 153) to 0.7% (1 of 153).
fn report_strength_names_opponent(event_type: &str) -> bool {
    event_type == "blocked-shot"
}

fn team_side(
    report_abbrev: &str,
    home_abbrev: Option<&str>,
    away_abbrev: Option<&str>,
) -> Option<bool> {
    let report = canonical_team_abbrev(report_abbrev);
    if home_abbrev.is_some_and(|value| canonical_team_abbrev(value) == report) {
        Some(true)
    } else if away_abbrev.is_some_and(|value| canonical_team_abbrev(value) == report) {
        Some(false)
    } else {
        None
    }
}

fn canonical_team_abbrev(value: &str) -> String {
    let compact = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_uppercase)
        .collect::<String>();

    match compact.as_str() {
        "LA" => "LAK".to_string(),
        "NJ" => "NJD".to_string(),
        "SJ" => "SJS".to_string(),
        "TB" => "TBL".to_string(),
        _ => compact,
    }
}

/// Parse either historical report layout. NHL used a fixed-width `<pre>`
/// layout in 2005-06 and 2006-07, then a structured table layout.
pub fn parse_report(html: &str) -> Vec<ReportEvent> {
    let document = Html::parse_document(html);
    let mut events = parse_structured_rows(&document);
    if events.is_empty() {
        events = parse_fixed_width_rows(&document);
    }
    events
}

fn parse_structured_rows(document: &Html) -> Vec<ReportEvent> {
    let row_selector = Selector::parse("tr.evenColor, tr.oddColor")
        .expect("historical report row selector must be valid");

    document
        .select(&row_selector)
        .filter_map(|row| {
            let cells: Vec<String> = row
                .children()
                .filter_map(ElementRef::wrap)
                .filter(|element| element.value().name() == "td")
                .map(|cell| normalized_text(cell.text()))
                .collect();

            if cells.len() < 6 {
                return None;
            }

            build_event(
                cells[0].trim(),
                cells[1].trim(),
                cells[3].split_whitespace().next()?,
                cells[4].trim(),
                team_from_description(&cells[5]),
                cells[2].trim(),
                cells[5].trim(),
            )
        })
        .collect()
}

fn parse_fixed_width_rows(document: &Html) -> Vec<ReportEvent> {
    let pre_selector = Selector::parse("pre").expect("pre selector must be valid");
    let Some(pre) = document.select(&pre_selector).next() else {
        return Vec::new();
    };
    let text = pre.text().collect::<String>();

    text.lines()
        .filter_map(|line| {
            if line.len() < 43 || !line.is_ascii() {
                return None;
            }
            build_event(
                line.get(0..5)?.trim(),
                line.get(5..10)?.trim(),
                line.get(10..16)?.trim(),
                line.get(16..32)?.trim(),
                optional_team(line.get(32..38)?.trim()),
                line.get(38..42)?.trim(),
                line.get(42..).unwrap_or_default().trim(),
            )
        })
        .collect()
}

fn build_event(
    event_id: &str,
    period: &str,
    time: &str,
    event_type: &str,
    team_abbrev: Option<String>,
    strength: &str,
    description: &str,
) -> Option<ReportEvent> {
    Some(ReportEvent {
        report_event_id: event_id.parse().ok()?,
        period: period.parse().ok()?,
        time_in_period: normalize_time(time)?,
        event_type: normalize_event_type(event_type)?.to_string(),
        team_abbrev,
        strength: EventStrength::from_nhl(strength),
        description: description.to_string(),
    })
}

fn normalize_time(value: &str) -> Option<String> {
    let (minutes, seconds) = value.split_once(':')?;
    let minutes: u16 = minutes.parse().ok()?;
    let seconds: u8 = seconds.parse().ok()?;
    if seconds > 59 {
        return None;
    }
    Some(format!("{minutes:02}:{seconds:02}"))
}

fn normalize_event_type(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_uppercase().as_str() {
        "GOAL" => Some("goal"),
        "SHOT" | "SHOT (!)" => Some("shot-on-goal"),
        "MISS" | "MISSED SHOT" => Some("missed-shot"),
        "BLOCK" | "BLOCKED SHOT" => Some("blocked-shot"),
        "HIT" | "HIT (!)" => Some("hit"),
        "GIVE" | "GIVEAWAY" => Some("giveaway"),
        "TAKE" | "TAKEAWAY" => Some("takeaway"),
        "FAC" | "FACE-OFF" => Some("faceoff"),
        "PENL" | "PENALTY" => Some("penalty"),
        "STOP" | "STOPPAGE" => Some("stoppage"),
        "PSTR" => Some("period-start"),
        "PEND" => Some("period-end"),
        "GEND" => Some("game-end"),
        "SOC" => Some("shootout-complete"),
        "GOALIE" => Some("goalie-change"),
        _ => None,
    }
}

fn optional_team(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value != "N/A").then(|| value.to_string())
}

fn team_from_description(description: &str) -> Option<String> {
    let first = description.split_whitespace().next()?;
    let candidate = first.trim_matches(|character: char| !character.is_ascii_alphanumeric());
    (2..=4)
        .contains(&candidate.len())
        .then(|| candidate.to_string())
}

fn normalized_text<'a>(parts: impl Iterator<Item = &'a str>) -> String {
    parts
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_supported_report_urls() {
        assert_eq!(
            report_url(2005020001).as_deref(),
            Some("https://www.nhl.com/scores/htmlreports/20052006/PL020001.HTM")
        );
        assert_eq!(
            report_url(2008030417).as_deref(),
            Some("https://www.nhl.com/scores/htmlreports/20082009/PL030417.HTM")
        );
        assert_eq!(
            report_url(2009020001).as_deref(),
            Some("https://www.nhl.com/scores/htmlreports/20092010/PL020001.HTM")
        );
    }

    #[test]
    fn parses_fixed_width_report_rows() {
        let html = r#"<html><body><pre>
  #   Per  Time  Event           Team Type  Description
----- ---  ----- --------------- ---- ----  ----------------
    2   1  00:32 HIT              BOS   EV  37 BERGERON
  162   2  18:35 BLOCKED SHOT     BOS   SH  19 THORNTON
  245   3  19:48 GOAL             MTL   PP  73 RYDER
  246   3  19:48 FACE-OFF         N/A    -  MTL won
</pre></body></html>"#;

        let events = parse_report(html);
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].event_type, "hit");
        assert_eq!(events[0].strength, Some(EventStrength::Even));
        assert_eq!(events[1].event_type, "blocked-shot");
        assert_eq!(events[1].strength, Some(EventStrength::ShortHanded));
        assert_eq!(events[2].report_event_id, 245);
        assert_eq!(events[2].time_in_period, "19:48");
        assert_eq!(events[2].team_abbrev.as_deref(), Some("MTL"));
        assert_eq!(events[2].strength, Some(EventStrength::PowerPlay));
        assert_eq!(events[3].strength, None);
        assert_eq!(events[3].team_abbrev, None);
    }

    #[test]
    fn parses_structured_report_rows() {
        let html = r#"<table>
          <tr class="evenColor">
            <td>4</td><td>1</td><td>EV</td><td>0:17<br>19:43</td>
            <td>SHOT</td><td>L.A ONGOAL - 11 KOPITAR</td><td><table><tr><td>nested</td></tr></table></td>
          </tr>
          <tr class="oddColor">
            <td>25</td><td>1</td><td>PP</td><td>6:14<br>13:46</td>
            <td>GOAL</td><td>ANA #10 PERRY</td><td></td>
          </tr>
        </table>"#;

        let events = parse_report(html);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].report_event_id, 4);
        assert_eq!(events[0].time_in_period, "00:17");
        assert_eq!(events[0].event_type, "shot-on-goal");
        assert_eq!(events[0].team_abbrev.as_deref(), Some("L.A"));
        assert_eq!(events[1].strength, Some(EventStrength::PowerPlay));
    }

    #[test]
    fn reconciles_report_rows_to_historical_json_ids() {
        let pbp: PlayByPlay = serde_json::from_str(
            r#"{
                "id": 2005020001,
                "homeTeam": {"id": 6, "abbrev": "BOS"},
                "awayTeam": {"id": 8, "abbrev": "MTL"},
                "plays": [
                    {
                        "eventId": 10797008,
                        "periodDescriptor": {"number": 1, "periodType": "REG"},
                        "timeInPeriod": "00:41",
                        "typeDescKey": "shot-on-goal",
                        "details": {"eventOwnerTeamId": 6}
                    },
                    {
                        "eventId": 10088563,
                        "periodDescriptor": {"number": 3, "periodType": "REG"},
                        "timeInPeriod": "19:48",
                        "typeDescKey": "goal",
                        "details": {"eventOwnerTeamId": 8}
                    }
                ]
            }"#,
        )
        .unwrap();
        let report = parse_report(
            r#"<html><body><pre>
    3   1  00:41 SHOT             BOS   EV  25 GILL
  245   3  19:48 GOAL             MTL   PP  73 RYDER
</pre></body></html>"#,
        );

        let result = reconcile_report_strengths(&pbp, &report);
        assert_eq!(result.matched, 2);
        assert_eq!(result.unmatched_json, 0);
        assert_eq!(result.unmatched_report, 0);
        assert_eq!(result.strengths.get(&10797008), Some(&EventStrength::Even));
        assert_eq!(
            result.strengths.get(&10088563),
            Some(&EventStrength::PowerPlay)
        );
    }

    #[test]
    fn converts_report_strength_to_the_json_owner_perspective() {
        let pbp: PlayByPlay = serde_json::from_str(
            r#"{
                "id": 2009020001,
                "homeTeam": {"id": 6, "abbrev": "BOS"},
                "awayTeam": {"id": 15, "abbrev": "WSH"},
                "plays": [{
                    "eventId": 41,
                    "periodDescriptor": {"number": 1, "periodType": "REG"},
                    "timeInPeriod": "15:29",
                    "typeDescKey": "blocked-shot",
                    "details": {"eventOwnerTeamId": 15}
                }]
            }"#,
        )
        .unwrap();
        // A blocked-shot row names the shooting team first, so the parser
        // reports WSH here, while the strength column describes BOS, the
        // blocking team. WSH is short-handed at 4-on-5, and JSON assigns
        // ownership to WSH, so the owner-relative answer is short-handed.
        let description = "WSH #8 OVECHKIN BLOCKED BY BOS #53 MORRIS, Snap, Def. Zone";
        let report = vec![ReportEvent {
            report_event_id: 18,
            period: 1,
            time_in_period: "15:29".to_string(),
            event_type: "blocked-shot".to_string(),
            team_abbrev: team_from_description(description),
            strength: Some(EventStrength::PowerPlay),
            description: description.to_string(),
        }];
        assert_eq!(report[0].team_abbrev.as_deref(), Some("WSH"));

        let result = reconcile_report_strengths(&pbp, &report);
        assert_eq!(result.matched, 1);
        assert_eq!(result.strengths.get(&41), Some(&EventStrength::ShortHanded));
    }

    #[test]
    fn ignores_report_strength_for_penalties() {
        let pbp: PlayByPlay = serde_json::from_str(
            r#"{
                "id": 2009020001,
                "homeTeam": {"id": 6, "abbrev": "BOS"},
                "awayTeam": {"id": 15, "abbrev": "WSH"},
                "plays": [{
                    "eventId": 441,
                    "periodDescriptor": {"number": 1, "periodType": "REG"},
                    "timeInPeriod": "09:43",
                    "typeDescKey": "penalty",
                    "details": {"eventOwnerTeamId": 15}
                }]
            }"#,
        )
        .unwrap();
        let report = vec![ReportEvent {
            report_event_id: 22,
            period: 1,
            time_in_period: "09:43".to_string(),
            event_type: "penalty".to_string(),
            team_abbrev: Some("WSH".to_string()),
            strength: Some(EventStrength::Even),
            description: "WSH #4 ERSKINE Fighting (maj)(5 min)".to_string(),
        }];

        let result = reconcile_report_strengths(&pbp, &report);
        assert_eq!(result.matched, 0);
        assert!(result.strengths.is_empty());
        assert_eq!(result.unmatched_json, 0);
        assert_eq!(result.unmatched_report, 0);
    }

    #[tokio::test]
    #[ignore = "requires the live NHL report archive"]
    async fn parses_live_reports_from_both_historical_layouts() {
        for game_id in [2005020001, 2007020001] {
            let pbp = crate::fetchers::events::fetch_play_by_play(game_id)
                .await
                .unwrap();
            let report = fetch_report_events(game_id).await.unwrap();
            assert!(report.len() > 200);

            let reconciliation = reconcile_report_strengths(&pbp, &report);
            let expected_matches = pbp
                .plays
                .iter()
                .filter(|play| matches!(play.type_desc_key.as_str(), "goal" | "shot-on-goal"))
                .count();
            let unmatched: Vec<_> = pbp
                .plays
                .iter()
                .filter(|play| {
                    matches!(play.type_desc_key.as_str(), "goal" | "shot-on-goal")
                        && !reconciliation.strengths.contains_key(&play.event_id)
                })
                .map(|play| {
                    (
                        play.event_id,
                        play.period_descriptor.number,
                        play.time_in_period.as_str(),
                        play.type_desc_key.as_str(),
                    )
                })
                .collect();
            assert!(
                reconciliation.matched >= expected_matches,
                "game {game_id}; unmatched {unmatched:?}"
            );
            assert!(
                unmatched.is_empty(),
                "game {game_id}; unmatched {unmatched:?}"
            );

            let goal_strengths = crate::fetchers::events::fetch_goal_strengths(game_id)
                .await
                .unwrap();
            for play in pbp.plays.iter().filter(|play| play.type_desc_key == "goal") {
                assert_eq!(
                    reconciliation.strengths.get(&play.event_id),
                    goal_strengths.get(&play.event_id),
                    "goal {} in game {game_id}",
                    play.event_id
                );
            }
        }

        let pbp = crate::fetchers::events::fetch_play_by_play(2005020127)
            .await
            .unwrap();
        let missing_report = fetch_reconciled_strengths(&pbp).await.unwrap();
        assert!(!missing_report.report_available);
        assert!(missing_report.strengths.is_empty());

        let game_id = 2009020001;
        let pbp = crate::fetchers::events::fetch_play_by_play(game_id)
            .await
            .unwrap();
        let url = report_url(game_id).unwrap();
        let html = fetch_api_text(&url).await.unwrap();
        let report = parse_report(&html);
        let reconciliation = reconcile_report_strengths(&pbp, &report);
        assert!(reconciliation.matched > 100);

        let mut compared = std::collections::BTreeMap::<String, usize>::new();
        let mut mismatches = std::collections::BTreeMap::<String, usize>::new();
        let mut mismatch_examples = Vec::new();
        for play in &pbp.plays {
            let Some(report_strength) = reconciliation.strengths.get(&play.event_id) else {
                continue;
            };
            let Some(situation) = play
                .situation_code
                .as_deref()
                .and_then(crate::fetchers::events::decode_situation_code)
            else {
                continue;
            };
            let owner = play
                .details
                .as_ref()
                .and_then(|details| details.event_owner_team_id);
            let owner_is_home = match owner {
                Some(team_id) if team_id == pbp.home_team.id => Some(true),
                Some(team_id) if team_id == pbp.away_team.id => Some(false),
                _ => None,
            };
            let expected = crate::fetchers::events::strength_for_owner(&situation, owner_is_home)
                .and_then(EventStrength::from_nhl);
            *compared.entry(play.type_desc_key.clone()).or_default() += 1;
            if Some(*report_strength) != expected {
                *mismatches.entry(play.type_desc_key.clone()).or_default() += 1;
                mismatch_examples.push((
                    play.event_id,
                    play.type_desc_key.as_str(),
                    play.time_in_period.as_str(),
                    play.situation_code.as_deref(),
                    *report_strength,
                    expected,
                ));
            }
        }
        println!("compared={compared:?} mismatches={mismatches:?} examples={mismatch_examples:?}");
        let total_compared: usize = compared.values().sum();
        let total_mismatches: usize = mismatches.values().sum();
        assert!(total_compared > 200);
        assert!(total_mismatches * 20 < total_compared);
        assert_eq!(mismatches.get("shot-on-goal").copied().unwrap_or(0), 0);
        // Blocked shots state strength from the blocking team's perspective;
        // a regression there previously cost about 25% of them.
        assert_eq!(mismatches.get("blocked-shot").copied().unwrap_or(0), 0);
        // Penalty rows are excluded from the report source entirely.
        assert_eq!(compared.get("penalty").copied().unwrap_or(0), 0);

        let goal_strengths = crate::fetchers::events::fetch_goal_strengths(game_id)
            .await
            .unwrap();
        for play in pbp.plays.iter().filter(|play| play.type_desc_key == "goal") {
            let situation = play
                .situation_code
                .as_deref()
                .and_then(crate::fetchers::events::decode_situation_code)
                .unwrap();
            let owner = play
                .details
                .as_ref()
                .and_then(|details| details.event_owner_team_id);
            let owner_is_home = match owner {
                Some(team_id) if team_id == pbp.home_team.id => Some(true),
                Some(team_id) if team_id == pbp.away_team.id => Some(false),
                _ => None,
            };
            let from_code = crate::fetchers::events::strength_for_owner(&situation, owner_is_home)
                .and_then(EventStrength::from_nhl);
            assert_eq!(goal_strengths.get(&play.event_id).copied(), from_code);
        }
    }
}
