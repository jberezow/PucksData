use super::{clock, types::*, validate::Validated};
use crate::fetchers::events::decode_situation_code;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Changes {
    starts: Vec<usize>,
    ends: Vec<usize>,
    instants: Vec<usize>,
}

fn lineup(indices: &BTreeSet<usize>, v: &Validated, home_id: i64) -> Lineup {
    let mut result = Lineup::default();
    for &index in indices {
        let row = &v.intervals[index];
        let side = if row.team_id == home_id {
            &mut result.home
        } else {
            &mut result.away
        };
        match row.role {
            Role::Skater => &mut side.skaters,
            Role::Goalie => &mut side.goalies,
            Role::Unknown => &mut side.unknown,
        }
        .push(row.player_id);
    }
    for side in [&mut result.home, &mut result.away] {
        for ids in [&mut side.skaters, &mut side.goalies, &mut side.unknown] {
            ids.sort_unstable();
            ids.dedup();
        }
    }
    result
}

fn intersection(a: &Lineup, b: &Lineup) -> Lineup {
    fn side(a: &Side, b: &Side) -> Side {
        fn ids(a: &[i64], b: &[i64]) -> Vec<i64> {
            a.iter().copied().filter(|id| b.contains(id)).collect()
        }
        Side {
            skaters: ids(&a.skaters, &b.skaters),
            goalies: ids(&a.goalies, &b.goalies),
            unknown: ids(&a.unknown, &b.unknown),
        }
    }
    Lineup {
        home: side(&a.home, &b.home),
        away: side(&a.away, &b.away),
    }
}

fn exact_counts(lineup: &Lineup, c: &Counts) -> bool {
    lineup.home.unknown.is_empty()
        && lineup.away.unknown.is_empty()
        && lineup.home.skaters.len() == c.home_skaters as usize
        && lineup.away.skaters.len() == c.away_skaters as usize
        && lineup.home.goalies.len() == usize::from(c.home_goalie)
        && lineup.away.goalies.len() == usize::from(c.away_goalie)
}

fn compare(output: &mut EventLineup, event: &Event) {
    if event.strength_source != "situation_code" || event.situation_code.is_none() {
        output.situation_agreement = Agreement::Unavailable;
        return;
    }
    let Some(code) = event
        .situation_code
        .as_deref()
        .and_then(decode_situation_code)
    else {
        output.situation_agreement = Agreement::InvalidCode;
        return;
    };
    let c = Counts {
        away_skaters: code.away_skater_count,
        home_skaters: code.home_skater_count,
        away_goalie: code.away_goalie_present,
        home_goalie: code.home_goalie_present,
    };
    output.decoded_fields_agree = Some(
        event.away_skater_count == Some(c.away_skaters)
            && event.home_skater_count == Some(c.home_skaters)
            && event.away_goalie_present == Some(c.away_goalie)
            && event.home_goalie_present == Some(c.home_goalie),
    );
    if !c.home_goalie || !c.away_goalie {
        output.contexts.push("goalie_absence_expected".into());
    }
    if c.home_skaters == 0 || c.away_skaters == 0 {
        output.contexts.push("zero_skater_situation".into());
    }
    if matches!(
        (c.away_skaters, c.away_goalie, c.home_skaters, c.home_goalie),
        (0, true, 1, false) | (1, false, 0, true)
    ) {
        // A count pattern, not independent proof of the event's cause. Clock-stopped
        // penalty shots cannot be recovered from continuous shift clocks alone.
        output.contexts.push("penalty_shot_count_pattern".into());
    }
    output.expected_counts = Some(c.clone());
    if matches!(
        output.status,
        ReconstructionStatus::InvalidEventTime
            | ReconstructionStatus::Unsupported
            | ReconstructionStatus::NoShifts
    ) || !output.possible.home.unknown.is_empty()
        || !output.possible.away.unknown.is_empty()
    {
        output.situation_agreement = Agreement::NotComparable;
        return;
    }
    output.before_counts_match = Some(exact_counts(&output.before, &c));
    output.after_counts_match = Some(exact_counts(&output.after, &c));
    let d = &output.definite;
    let p = &output.possible;
    for (name, low, high, expected) in [
        (
            "home_skaters",
            d.home.skaters.len(),
            p.home.skaters.len(),
            c.home_skaters as usize,
        ),
        (
            "away_skaters",
            d.away.skaters.len(),
            p.away.skaters.len(),
            c.away_skaters as usize,
        ),
        (
            "home_goalie",
            d.home.goalies.len(),
            p.home.goalies.len(),
            usize::from(c.home_goalie),
        ),
        (
            "away_goalie",
            d.away.goalies.len(),
            p.away.goalies.len(),
            usize::from(c.away_goalie),
        ),
    ] {
        if !(low..=high).contains(&expected) {
            output.mismatch_fields.push(name.into());
        }
    }
    output.situation_agreement = if !output.mismatch_fields.is_empty() {
        Agreement::Mismatch
    } else if d == p {
        Agreement::Exact
    } else {
        Agreement::BoundaryCompatible
    };
}

fn empty(event: &Event) -> EventLineup {
    EventLineup {
        game_id: event.game_id,
        event_id: event.id,
        event_id_in_game: event.event_id_in_game,
        event_type: event.event_type.clone(),
        period: event.period,
        time_in_period: event.time_in_period.clone(),
        clock: None,
        status: ReconstructionStatus::InvalidEventTime,
        definite: Lineup::default(),
        possible: Lineup::default(),
        before: Lineup::default(),
        after: Lineup::default(),
        source_shift_ids: vec![],
        source_flags: vec![],
        incomplete_reasons: vec![],
        contexts: vec![],
        expected_counts: None,
        situation_agreement: Agreement::NotComparable,
        mismatch_fields: vec![],
        decoded_fields_agree: None,
        before_counts_match: None,
        after_counts_match: None,
    }
}

pub(super) fn reconstruct(source: &GameSource, v: &Validated) -> Vec<EventLineup> {
    let game = &source.game;
    let supported = game.season >= FIRST_SEASON && matches!(game.game_type, 2 | 3);
    let mut output = Vec::with_capacity(source.events.len());
    let mut by_time: BTreeMap<(i16, i32), Vec<&Event>> = BTreeMap::new();
    for event in &source.events {
        let c = clock::event_clock(
            game.game_type,
            event.period,
            &event.period_type,
            &event.time_in_period,
        );
        if !supported || c.is_none() || source.shifts.is_empty() || event.game_id != game.game_id {
            let mut row = empty(event);
            row.clock = c;
            row.status = if !supported {
                ReconstructionStatus::Unsupported
            } else if c.is_none() || event.game_id != game.game_id {
                ReconstructionStatus::InvalidEventTime
            } else {
                ReconstructionStatus::NoShifts
            };
            compare(&mut row, event);
            output.push(row);
        } else if let Some(c) = c {
            by_time
                .entry((event.period, c.period_seconds))
                .or_default()
                .push(event);
        }
    }
    let mut changes: BTreeMap<i16, BTreeMap<i32, Changes>> = BTreeMap::new();
    for (index, row) in v.intervals.iter().enumerate() {
        let period = changes.entry(row.period).or_default();
        if row.start == row.end {
            period.entry(row.start).or_default().instants.push(index);
        } else {
            period.entry(row.start).or_default().starts.push(index);
            period.entry(row.end).or_default().ends.push(index);
        }
    }
    let mut current_period = 0;
    let mut active = BTreeSet::new();
    let mut boundary = BTreeMap::new().into_iter().peekable();
    for ((period, seconds), events) in by_time {
        if period != current_period {
            current_period = period;
            active.clear();
            boundary = changes
                .remove(&period)
                .unwrap_or_default()
                .into_iter()
                .peekable();
        }
        while boundary.peek().is_some_and(|(t, _)| *t < seconds) {
            let (_, change) = boundary.next().unwrap();
            for i in change.ends {
                active.remove(&i);
            }
            active.extend(change.starts);
        }
        let before = lineup(&active, v, game.home_team_id);
        let mut candidates = active.clone();
        let mut raw_boundary = false;
        if boundary.peek().is_some_and(|(t, _)| *t == seconds) {
            raw_boundary = true;
            let (_, change) = boundary.next().unwrap();
            candidates.extend(change.instants);
            for i in change.ends {
                active.remove(&i);
            }
            active.extend(change.starts);
        }
        candidates.extend(&active);
        let after = lineup(&active, v, game.home_team_id);
        let definite = intersection(&before, &after);
        let possible = lineup(&candidates, v, game.home_team_id);
        let ambiguous = definite != possible;
        let flags: BTreeSet<_> = candidates
            .iter()
            .flat_map(|i| v.intervals[*i].flags.iter().copied())
            .collect();
        let mut reasons = Vec::new();
        if v.all_tainted || v.tainted_periods.contains(&period) {
            reasons.push("invalid_or_missing_source_coverage".into());
        }
        for (name, d, p) in [
            ("home", &definite.home, &possible.home),
            ("away", &definite.away, &possible.away),
        ] {
            if !p.unknown.is_empty() {
                reasons.push(format!("{name}_unknown_roles"));
            }
            if p.skaters.len() < 3 {
                reasons.push(format!("{name}_missing_skaters"));
            }
            if d.skaters.len() > 6 || d.goalies.len() > 1 || d.skaters.len() + d.goalies.len() > 6 {
                reasons.push(format!("{name}_excess_players"));
            }
        }
        let status = match (ambiguous, reasons.is_empty()) {
            (false, true) => ReconstructionStatus::Resolved,
            (true, true) => ReconstructionStatus::Ambiguous,
            (false, false) => ReconstructionStatus::Incomplete,
            (true, false) => ReconstructionStatus::AmbiguousIncomplete,
        };
        for event in events {
            let mut row = empty(event);
            row.clock = clock::clock(game.game_type, period, seconds);
            row.status = status;
            row.before = before.clone();
            row.after = after.clone();
            row.definite = definite.clone();
            row.possible = possible.clone();
            row.source_shift_ids = candidates
                .iter()
                .map(|i| v.intervals[*i].source_id)
                .collect();
            row.source_shift_ids.sort_unstable();
            row.source_flags = flags.iter().copied().collect();
            row.incomplete_reasons = reasons.clone();
            if raw_boundary {
                row.contexts.push("coincident_shift_boundary".into());
            }
            if seconds == 0
                || Some(seconds) == clock::period_length(game.game_type, period)
                || matches!(
                    event.event_type.as_str(),
                    "period-start" | "period-end" | "game-end"
                )
            {
                row.contexts.push("period_boundary".into());
            }
            if period >= 4 {
                row.contexts.push(
                    if game.game_type == 2 {
                        "regular_season_ot"
                    } else {
                        "playoff_ot"
                    }
                    .into(),
                );
            }
            if event.event_type == "delayed-penalty" {
                row.contexts.push("delayed_penalty_event".into());
            }
            if event.event_type == "penalty" {
                row.contexts.push("penalty_event".into());
            }
            if event.event_type == "goal" {
                row.contexts.push("goal_event".into());
            }
            if event.event_type == "stoppage" {
                row.contexts.push("stoppage".into());
            }
            if !flags.is_empty() {
                row.contexts.push("source_warnings".into());
            }
            compare(&mut row, event);
            output.push(row);
        }
    }
    output.sort_by_key(|e| (e.period, e.event_id_in_game, e.event_id));
    output
}
