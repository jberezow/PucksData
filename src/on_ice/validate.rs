use super::{clock, types::*};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Validated {
    pub intervals: Vec<Interval>,
    pub issues: Vec<Issue>,
    pub tainted_periods: BTreeSet<i16>,
    pub all_tainted: bool,
    pub roles: BTreeMap<i64, Role>,
    pub fallback_players: BTreeSet<i64>,
}

fn position_role(position: Option<&str>) -> Role {
    match position {
        Some("G") => Role::Goalie,
        Some("C" | "L" | "R" | "LW" | "RW" | "D" | "F") => Role::Skater,
        _ => Role::Unknown,
    }
}

impl Validated {
    fn issue(&mut self, code: IssueCode, row: Option<&Shift>, error: bool, detail: String) {
        self.issues.push(Issue {
            code,
            severity: if error { "error" } else { "warning" }.into(),
            source_shift_ids: row.map(|r| vec![r.source_shift_id]).unwrap_or_default(),
            player_id: row.and_then(|r| r.player_id),
            period: row.and_then(|r| r.period),
            detail,
        });
    }
}

pub(super) fn validate(source: &GameSource) -> Validated {
    let mut v = Validated {
        intervals: vec![],
        issues: vec![],
        tainted_periods: BTreeSet::new(),
        all_tainted: false,
        roles: BTreeMap::new(),
        fallback_players: BTreeSet::new(),
    };
    let game = &source.game;
    let mut official_roles: BTreeMap<i64, BTreeSet<Role>> = BTreeMap::new();
    let mut official_counts = BTreeMap::new();
    for row in &source.official_toi {
        official_roles.entry(row.player_id).or_default().insert(
            if row.player_type == "goalie" || row.position_code.as_deref() == Some("G") {
                Role::Goalie
            } else {
                Role::Skater
            },
        );
        *official_counts.entry(row.player_id).or_insert(0) += 1;
        if row.time_on_ice_seconds.is_some_and(|s| s < 0) {
            v.issues.push(Issue {
                code: IssueCode::InvalidOfficialToi,
                severity: "error".into(),
                source_shift_ids: vec![],
                player_id: Some(row.player_id),
                period: None,
                detail: "negative official time on ice".into(),
            });
        }
    }
    for (&pid, roles) in &official_roles {
        if official_counts[&pid] > 1 {
            v.issues.push(Issue {
                code: IssueCode::ConflictingOfficialRecords,
                severity: "error".into(),
                source_shift_ids: vec![],
                player_id: Some(pid),
                period: None,
                detail: "multiple official player/game TOI records; no reference selected".into(),
            });
        }
        v.roles.insert(
            pid,
            if roles.len() == 1 {
                *roles.first().unwrap()
            } else {
                Role::Unknown
            },
        );
    }
    let mut metadata_roles: BTreeMap<i64, BTreeSet<Role>> = BTreeMap::new();
    for row in &source.shifts {
        if let Some(pid) = row.player_id {
            metadata_roles
                .entry(pid)
                .or_default()
                .insert(position_role(row.player_position.as_deref()));
        }
    }
    for (&pid, roles) in &metadata_roles {
        if let std::collections::btree_map::Entry::Vacant(entry) = v.roles.entry(pid) {
            entry.insert(if roles.len() == 1 {
                *roles.first().unwrap()
            } else {
                Role::Unknown
            });
            v.fallback_players.insert(pid);
        }
        if official_roles.get(&pid).is_some_and(|r| r.len() > 1)
            || (!official_roles.contains_key(&pid) && roles.len() > 1)
        {
            v.issues.push(Issue {
                code: IssueCode::ConflictingPlayerRole,
                severity: "error".into(),
                source_shift_ids: vec![],
                player_id: Some(pid),
                period: None,
                detail: "cannot choose a unique skater/goalie role".into(),
            });
        }
    }
    for row in &source.shifts {
        let mut errors = Vec::new();
        let mut flags = Vec::new();
        if row.game_id != game.game_id {
            errors.push(IssueCode::WrongGame);
        }
        if row.type_code != 517 {
            errors.push(IssueCode::UnexpectedType);
        }
        match row.player_id {
            None => errors.push(IssueCode::MissingPlayerId),
            Some(p) if p <= 0 => errors.push(IssueCode::InvalidPlayerId),
            _ => (),
        }
        if row.nhl_team_id.is_none() {
            errors.push(IssueCode::MissingTeamId);
        }
        match row.franchise_id {
            None => errors.push(IssueCode::UnknownTeamId),
            Some(t) if t != game.home_team_id && t != game.away_team_id => {
                errors.push(IssueCode::UnexpectedTeam)
            }
            _ => (),
        }
        let length = row
            .period
            .and_then(|p| clock::period_length(game.game_type, p));
        if row.period.is_none() {
            errors.push(IssueCode::MissingPeriod);
        } else if length.is_none() {
            errors.push(IssueCode::UnexpectedPeriod);
        }
        let start = row.start_time.as_deref().and_then(clock::parse_seconds);
        let end = row.end_time.as_deref().and_then(clock::parse_seconds);
        let duration = row.duration.as_deref().and_then(clock::parse_seconds);
        if start.is_none() {
            errors.push(IssueCode::MalformedStart);
        }
        if end.is_none() {
            errors.push(IssueCode::MalformedEnd);
        }
        if row.duration.is_none() {
            flags.push(IssueCode::MissingDuration);
        } else if duration.is_none() {
            flags.push(IssueCode::MalformedDuration);
        }
        if row.start_time_seconds != start
            || row.end_time_seconds != end
            || row.duration_seconds != duration
        {
            errors.push(IssueCode::StoredClockDisagreement);
        }
        if let (Some(start), Some(end)) = (start, end) {
            if start > end {
                errors.push(IssueCode::ReversedInterval);
            }
            if length.is_some_and(|bound| start > bound || end > bound) {
                errors.push(IssueCode::OutOfBounds);
            }
            if start == end {
                flags.push(IssueCode::ZeroDuration);
            }
            if duration.is_some_and(|duration| duration != end - start) {
                flags.push(IssueCode::DurationMismatch);
            }
        }
        let role = row
            .player_id
            .and_then(|p| v.roles.get(&p).copied())
            .unwrap_or(Role::Unknown);
        if role == Role::Unknown {
            flags.push(IssueCode::UnknownPlayerRole);
        }
        if role == Role::Skater && start.zip(end).is_some_and(|(s, e)| e - s > 180) {
            flags.push(IssueCode::SuspiciousLongShift);
        }
        for &code in &errors {
            v.issue(
                code,
                Some(row),
                true,
                "row excluded from interval-derived calculations".into(),
            );
        }
        for &code in &flags {
            v.issue(
                code,
                Some(row),
                false,
                match code {
                    IssueCode::SuspiciousLongShift => {
                        "skater interval exceeds the diagnostic 180-second threshold"
                    }
                    IssueCode::ZeroDuration => "instantaneous candidate only; contributes no TOI",
                    IssueCode::DurationMismatch => {
                        "source duration differs from end minus start; both values retained"
                    }
                    _ => "source warning retained without repair",
                }
                .into(),
            );
        }
        if !errors.is_empty() {
            if let Some(p) = row
                .period
                .filter(|p| clock::period_length(game.game_type, *p).is_some())
            {
                v.tainted_periods.insert(p);
            } else {
                v.all_tainted = true;
            }
            continue;
        }
        v.intervals.push(Interval {
            source_id: row.source_shift_id,
            player_id: row.player_id.unwrap(),
            team_id: row.franchise_id.unwrap(),
            role,
            period: row.period.unwrap(),
            start: start.unwrap(),
            end: end.unwrap(),
            flags,
        });
    }
    let mut by_player: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for (index, interval) in v.intervals.iter().enumerate() {
        by_player.entry(interval.player_id).or_default().push(index);
    }
    for (&pid, indices) in &by_player {
        if indices.len() > 100 {
            v.issues.push(Issue {
                code: IssueCode::SuspiciousShiftCount,
                severity: "warning".into(),
                source_shift_ids: vec![],
                player_id: Some(pid),
                period: None,
                detail: format!(
                    "{} accepted source rows exceeds diagnostic threshold 100",
                    indices.len()
                ),
            });
        }
        let teams: BTreeSet<_> = indices.iter().map(|i| v.intervals[*i].team_id).collect();
        if teams.len() > 1 {
            v.all_tainted = true;
            v.issues.push(Issue {
                code: IssueCode::PlayerMultipleTeams,
                severity: "error".into(),
                source_shift_ids: vec![],
                player_id: Some(pid),
                period: None,
                detail: "one player is assigned to both teams in this game".into(),
            });
        }
        let mut sorted = indices.clone();
        sorted.sort_by_key(|i| {
            (
                v.intervals[*i].period,
                v.intervals[*i].start,
                v.intervals[*i].end,
            )
        });
        // Player-sized lists, not a game/season-wide interval self join.
        for (offset, &a) in sorted.iter().enumerate() {
            for &b in &sorted[offset + 1..] {
                let (x, y) = (&v.intervals[a], &v.intervals[b]);
                if y.period != x.period || y.start > x.end {
                    break;
                }
                let duplicate = x.start == y.start && x.end == y.end;
                let overlap =
                    clock::intersection((x.period, x.start, x.end), (y.period, y.start, y.end));
                if duplicate || overlap > 0 {
                    let code = if duplicate {
                        IssueCode::DuplicateInterval
                    } else {
                        IssueCode::SamePlayerOverlap
                    };
                    v.issues.push(Issue { code, severity: "warning".into(),
                        source_shift_ids: vec![x.source_id,y.source_id], player_id: Some(pid), period: Some(x.period),
                        detail: format!("{overlap} seconds overlap; raw rows retained, union TOI counts the player once") });
                    v.intervals[a].flags.push(code);
                    v.intervals[b].flags.push(code);
                }
            }
        }
    }
    if !source.shifts.is_empty() {
        let mut expected_periods: BTreeSet<i16> = [1, 2, 3].into_iter().collect();
        expected_periods.extend(
            source
                .events
                .iter()
                .filter(|e| {
                    clock::event_clock(game.game_type, e.period, &e.period_type, &e.time_in_period)
                        .is_some()
                })
                .map(|e| e.period),
        );
        for team in [game.home_team_id, game.away_team_id] {
            if !v.intervals.iter().any(|i| i.team_id == team) {
                v.issue(
                    IssueCode::MissingTeamShifts,
                    None,
                    true,
                    format!("no usable intervals for franchise {team}"),
                );
            }
            for &period in &expected_periods {
                if !v
                    .intervals
                    .iter()
                    .any(|i| i.team_id == team && i.period == period)
                {
                    v.tainted_periods.insert(period);
                    v.issues.push(Issue {
                        code: IssueCode::MissingPeriodShifts,
                        severity: "error".into(),
                        source_shift_ids: vec![],
                        player_id: None,
                        period: Some(period),
                        detail: format!(
                            "no usable intervals for franchise {team} in expected period"
                        ),
                    });
                }
            }
        }
        for period in v
            .intervals
            .iter()
            .map(|i| i.period)
            .collect::<BTreeSet<_>>()
        {
            if !source.events.is_empty() && !source.events.iter().any(|e| e.period == period) {
                v.issues.push(Issue {
                    code: IssueCode::ShiftPeriodWithoutEvents,
                    severity: "warning".into(),
                    source_shift_ids: vec![],
                    player_id: None,
                    period: Some(period),
                    detail:
                        "shift period has no stored events; may indicate incomplete play-by-play"
                            .into(),
                });
            }
        }
    }
    count_anomalies(&mut v);
    v
}

// Inspect positive-length segments, counting each player once even with duplicate rows.
// Empty nets are legal; fewer than three skaters or more than six total players are suspicious.
fn count_anomalies(v: &mut Validated) {
    type Changes = BTreeMap<i32, Vec<(usize, bool)>>;
    let mut groups: BTreeMap<(i64, i16), Changes> = BTreeMap::new();
    for (idx, interval) in v
        .intervals
        .iter()
        .enumerate()
        .filter(|(_, i)| i.end > i.start)
    {
        let changes = groups
            .entry((interval.team_id, interval.period))
            .or_default();
        changes.entry(0).or_default();
        changes.entry(interval.start).or_default().push((idx, true));
        changes.entry(interval.end).or_default().push((idx, false));
    }
    for ((team, period), changes) in groups {
        let mut active = BTreeSet::new();
        let mut previous = None;
        for (second, deltas) in changes {
            if let Some(start) = previous {
                let players: BTreeMap<_, _> = active
                    .iter()
                    .map(|&i: &usize| {
                        let interval = &v.intervals[i];
                        (interval.player_id, interval.role)
                    })
                    .collect();
                let skaters = players.values().filter(|&&r| r == Role::Skater).count();
                let goalies = players.values().filter(|&&r| r == Role::Goalie).count();
                if second > start
                    && (!(3..=6).contains(&skaters) || goalies > 1 || players.len() > 6)
                {
                    v.issues.push(Issue {
                        code: IssueCode::SuspiciousOnIceCount, severity: "warning".into(),
                        source_shift_ids: active.iter().map(|&i| v.intervals[i].source_id).collect(),
                        player_id: None, period: Some(period),
                        detail: format!("franchise {team}, seconds {start}..{second}: {skaters} skaters, {goalies} goalies, {} unknown", players.len() - skaters - goalies),
                    });
                }
            }
            for (index, starts) in deltas {
                if starts {
                    active.insert(index);
                } else {
                    active.remove(&index);
                }
            }
            previous = Some(second);
        }
    }
}
