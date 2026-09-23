use super::{clock, types::*, validate::Validated};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn reconcile(source: &GameSource, v: &Validated) -> Vec<ToiComparison> {
    let mut players: BTreeSet<i64> = source
        .shifts
        .iter()
        .filter_map(|r| r.player_id)
        .filter(|p| *p > 0)
        .collect();
    players.extend(source.official_toi.iter().map(|r| r.player_id));
    players
        .into_iter()
        .map(|pid| {
            let rows: Vec<_> = source
                .shifts
                .iter()
                .filter(|r| r.player_id == Some(pid))
                .collect();
            let official: Vec<_> = source
                .official_toi
                .iter()
                .filter(|r| r.player_id == pid)
                .collect();
            let accepted: Vec<_> = v.intervals.iter().filter(|r| r.player_id == pid).collect();
            let rejected = rows.len() - accepted.len();
            let mut periods: BTreeMap<i16, Vec<(i32, i32)>> = BTreeMap::new();
            for i in &accepted {
                periods.entry(i.period).or_default().push((i.start, i.end));
            }
            let mut union = 0_i64;
            for intervals in periods.values_mut() {
                intervals.sort_unstable();
                let mut end = 0;
                for &(start, stop) in intervals.iter() {
                    union += i64::from((stop - start.max(end)).max(0));
                    end = end.max(stop);
                }
            }
            let reference = (official.len() == 1)
                .then(|| official[0].time_on_ice_seconds)
                .flatten();
            let zero_no_shifts = rows.is_empty() && reference == Some(0);
            let union = (!accepted.is_empty() || zero_no_shifts).then_some(union);
            let sum = (!accepted.is_empty() || zero_no_shifts).then(|| {
                accepted
                    .iter()
                    .map(|i| i64::from(i.end - i.start))
                    .sum::<i64>()
            });
            let difference = union.zip(reference.filter(|s| *s >= 0)).map(|(u, o)| u - o);
            let category = if official.len() > 1 {
                ToiCategory::ConflictingOfficial
            } else if reference.is_some_and(|s| s < 0) {
                ToiCategory::InvalidOfficial
            } else if zero_no_shifts {
                ToiCategory::OfficialZeroNoShifts
            } else if rows.is_empty() {
                ToiCategory::MissingShifts
            } else if accepted.is_empty() {
                ToiCategory::InvalidIntervals
            } else if rejected > 0 {
                ToiCategory::PartialIntervals
            } else if let Some(difference) = difference {
                match difference.abs() {
                    0 => ToiCategory::Exact,
                    1 => ToiCategory::WithinOneSecond,
                    2..=5 => ToiCategory::WithinFiveSeconds,
                    6..=30 => ToiCategory::WithinThirtySeconds,
                    _ => ToiCategory::LargeDifference,
                }
            } else {
                ToiCategory::MissingOfficial
            };
            let durations: Vec<_> = rows
                .iter()
                .filter_map(|r| r.duration.as_deref().and_then(clock::parse_seconds))
                .collect();
            ToiComparison {
                player_id: pid,
                role: v.roles.get(&pid).copied().unwrap_or(Role::Unknown),
                source_rows: rows.len(),
                accepted_rows: accepted.len(),
                rejected_rows: rejected,
                interval_sum_seconds: sum,
                interval_union_seconds: union,
                overlap_excess_seconds: sum.zip(union).map(|(s, u)| s - u),
                source_duration_sum_seconds: (!durations.is_empty())
                    .then(|| durations.iter().map(|s| i64::from(*s)).sum()),
                unparseable_duration_rows: rows.len() - durations.len(),
                official_seconds: reference,
                difference_seconds: difference,
                category,
            }
        })
        .collect()
}
