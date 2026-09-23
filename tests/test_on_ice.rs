mod common;
use pucksdata::on_ice::{self, audit, clock, *};

fn text(seconds: i32) -> String {
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}
fn shift(id: i64, player: i64, team: i64, period: i16, start: i32, end: i32) -> Shift {
    Shift {
        game_id: 2025020001,
        source_shift_id: id,
        type_code: 517,
        player_id: Some(player),
        nhl_team_id: Some(if team == 38 { 54 } else { 55 }),
        franchise_id: Some(team),
        player_position: Some(if player % 10 == 6 { "G" } else { "C" }.into()),
        period: Some(period),
        shift_number: Some(1),
        start_time: Some(text(start)),
        end_time: Some(text(end)),
        duration: Some(text(end - start)),
        start_time_seconds: Some(start),
        end_time_seconds: Some(end),
        duration_seconds: Some(end - start),
        ingested_at: "2026-09-23 00:00:00+00".into(),
    }
}
fn event(id: i32, period: i16, seconds: i32) -> Event {
    Event {
        game_id: 2025020001,
        id: id.into(),
        event_id_in_game: id,
        period,
        period_type: if period <= 3 { "REG" } else { "OT" }.into(),
        time_in_period: text(seconds),
        event_type: "shot-on-goal".into(),
        situation_code: Some("1551".into()),
        strength_source: "situation_code".into(),
        away_goalie_present: Some(true),
        away_skater_count: Some(5),
        home_skater_count: Some(5),
        home_goalie_present: Some(true),
    }
}
fn fixture() -> GameSource {
    let mut source = GameSource {
        game: Game {
            game_id: 2025020001,
            season: 20252026,
            game_type: 2,
            game_date: "2025-10-07".into(),
            game_state: Some("OFF".into()),
            home_team_id: 38,
            away_team_id: 39,
            fetch_status: Some("loaded".into()),
            attempted_at: None,
        },
        shifts: vec![],
        events: vec![event(1, 1, 30)],
        official_toi: vec![],
    };
    for (team, base) in [(38, 10), (39, 20)] {
        for player in base + 1..=base + 6 {
            for period in 1..=3 {
                source.shifts.push(shift(
                    player * 10 + i64::from(period),
                    player,
                    team,
                    period,
                    0,
                    1200,
                ));
            }
            source.official_toi.push(OfficialToi {
                game_id: source.game.game_id,
                player_id: player,
                player_type: if player % 10 == 6 { "goalie" } else { "skater" }.into(),
                position_code: Some(if player % 10 == 6 { "G" } else { "C" }.into()),
                team_abbrev: Some(if team == 38 { "VGK" } else { "SEA" }.into()),
                time_on_ice_seconds: Some(3600),
                source_revision: 1,
                source_observed_at: "2026-09-23 00:00:00+00".into(),
            });
        }
    }
    source
}

#[test]
fn temporal_semantics_preserve_period_identity_and_ot_limits() {
    assert_eq!(clock::parse_seconds("20:00"), Some(1200));
    for bad in [
        "",
        "-1:00",
        "00:60",
        "1:2",
        " 01:00",
        "01:00:00",
        "999999999999:00",
    ] {
        assert_eq!(clock::parse_seconds(bad), None, "{bad}");
    }
    let end = clock::event_clock(2, 1, "REG", "20:00").unwrap();
    let start = clock::event_clock(2, 2, "REG", "00:00").unwrap();
    assert_eq!(end.game_seconds, start.game_seconds);
    assert!(end < start);
    assert_eq!(
        clock::event_clock(2, 4, "OT", "05:00")
            .unwrap()
            .game_seconds,
        3900
    );
    assert!(clock::event_clock(2, 4, "OT", "05:01").is_none());
    assert!(clock::event_clock(2, 5, "SO", "00:00").is_none());
    assert_eq!(
        clock::event_clock(3, 5, "OT", "20:00")
            .unwrap()
            .game_seconds,
        6000
    );
    assert!(clock::event_clock(3, 4, "REG", "00:01").is_none());
    assert!(clock::clock(2, 0, 0).is_none());
    assert!(clock::clock(2, 1, -1).is_none());
    assert_eq!(clock::intersection((1, 0, 30), (1, 20, 40)), 10);
    assert_eq!(clock::intersection((1, 0, 30), (1, 30, 40)), 0);
    assert_eq!(clock::intersection((1, 0, 30), (2, 0, 30)), 0);
    assert_eq!(clock::intersection((1, 30, 0), (1, 0, 40)), 0);
}

#[test]
fn stable_lineups_and_official_toi_are_resolved_without_mutation() {
    let source = fixture();
    let before = serde_json::to_vec(&source).unwrap();
    let report = on_ice::analyze(&source);
    assert_eq!(report.events[0].status, ReconstructionStatus::Resolved);
    assert_eq!(report.events[0].situation_agreement, Agreement::Exact);
    assert_eq!(
        report.events[0].definite.home.skaters,
        vec![11, 12, 13, 14, 15]
    );
    assert_eq!(report.events[0].definite.away.goalies, vec![26]);
    assert!(report
        .toi
        .iter()
        .all(|p| p.category == ToiCategory::Exact && p.interval_union_seconds == Some(3600)));
    assert_eq!(before, serde_json::to_vec(&source).unwrap());
    assert_eq!(report.source_sha256, on_ice::analyze(&source).source_sha256);
}

#[test]
fn coincident_changes_do_not_invent_an_order_or_select_a_lineup_from_counts() {
    let mut source = fixture();
    let old = source
        .shifts
        .iter()
        .position(|r| r.player_id == Some(11) && r.period == Some(1))
        .unwrap();
    source.shifts[old] = shift(111, 11, 38, 1, 0, 30);
    source.shifts.push(shift(999, 17, 38, 1, 30, 1200));
    source.events.extend([event(2, 1, 29), event(3, 1, 31)]);
    let report = on_ice::analyze(&source);
    let boundary = &report.events[0];
    assert_eq!(boundary.status, ReconstructionStatus::Ambiguous);
    assert_eq!(boundary.situation_agreement, Agreement::BoundaryCompatible);
    assert_eq!(boundary.definite.home.skaters, vec![12, 13, 14, 15]);
    assert_eq!(boundary.possible.home.skaters, vec![11, 12, 13, 14, 15, 17]);
    assert_eq!(boundary.before_counts_match, Some(true));
    assert_eq!(boundary.after_counts_match, Some(true));
    assert_eq!(report.events[1].status, ReconstructionStatus::Resolved);
    assert_eq!(
        report.events[2].definite.home.skaters,
        vec![12, 13, 14, 15, 17]
    );
}

#[test]
fn adjacent_rows_for_same_player_are_continuous_but_zero_rows_only_possible() {
    let mut source = fixture();
    source.shifts[0] = shift(111, 11, 38, 1, 0, 30);
    source.shifts.push(shift(998, 11, 38, 1, 30, 1200));
    assert_eq!(
        on_ice::analyze(&source).events[0].status,
        ReconstructionStatus::Resolved
    );
    source.shifts.push(shift(999, 17, 38, 1, 30, 30));
    source.events.push(event(2, 1, 31));
    let report = on_ice::analyze(&source);
    assert_eq!(report.events[0].status, ReconstructionStatus::Ambiguous);
    assert!(report.events[0].possible.home.skaters.contains(&17));
    assert!(!report.events[0].definite.home.skaters.contains(&17));
    assert!(!report.events[1].possible.home.skaters.contains(&17));
    assert_eq!(
        report
            .toi
            .iter()
            .find(|p| p.player_id == 17)
            .unwrap()
            .interval_union_seconds,
        Some(0)
    );
}

#[test]
fn period_boundaries_never_blend_adjacent_periods() {
    let mut source = fixture();
    source.events = vec![event(1, 1, 1200), event(2, 2, 0)];
    source
        .shifts
        .retain(|r| r.player_id != Some(11) || r.period != Some(2));
    source.shifts.push(shift(999, 17, 38, 2, 0, 1200));
    let report = on_ice::analyze(&source);
    assert!(report.events[0].possible.home.skaters.contains(&11));
    assert!(!report.events[0].possible.home.skaters.contains(&17));
    assert!(report.events[1].possible.home.skaters.contains(&17));
    assert!(!report.events[1].possible.home.skaters.contains(&11));
    assert!(report
        .events
        .iter()
        .all(|e| e.status == ReconstructionStatus::Ambiguous));
}

#[test]
fn overlapping_rows_expose_sum_union_duration_and_do_not_double_count_players() {
    let mut source = fixture();
    source.shifts.push(shift(999, 11, 38, 1, 20, 40));
    let mut duplicate = source.shifts[0].clone();
    duplicate.source_shift_id = 998;
    source.shifts.push(duplicate);
    let report = on_ice::analyze(&source);
    let p = report.toi.iter().find(|p| p.player_id == 11).unwrap();
    assert_eq!(p.interval_sum_seconds, Some(4820));
    assert_eq!(p.interval_union_seconds, Some(3600));
    assert_eq!(p.overlap_excess_seconds, Some(1220));
    assert_eq!(p.source_duration_sum_seconds, Some(4820));
    assert_eq!(report.events[0].definite.home.skaters.len(), 5);
    assert!(report
        .validation_counts
        .contains_key(&IssueCode::SamePlayerOverlap));
    assert!(report
        .validation_counts
        .contains_key(&IssueCode::DuplicateInterval));
}

#[test]
fn malformed_shifts_are_excluded_and_reported_without_repair() {
    let mut source = fixture();
    source.shifts[0].start_time = Some("broken".into());
    source.shifts[1] = shift(112, 11, 38, 2, 50, 20);
    source.shifts[2].duration = Some("19:59".into());
    source.shifts[2].duration_seconds = Some(1199);
    let before = serde_json::to_vec(&source).unwrap();
    let report = on_ice::analyze(&source);
    assert_eq!(report.rejected_rows, 2);
    for code in [
        IssueCode::MalformedStart,
        IssueCode::ReversedInterval,
        IssueCode::StoredClockDisagreement,
        IssueCode::DurationMismatch,
    ] {
        assert!(report.validation_counts.contains_key(&code), "{code:?}");
    }
    assert_eq!(report.events[0].status, ReconstructionStatus::Incomplete);
    assert_eq!(
        report
            .toi
            .iter()
            .find(|p| p.player_id == 11)
            .unwrap()
            .category,
        ToiCategory::PartialIntervals
    );
    assert_eq!(serde_json::to_vec(&source).unwrap(), before);
}

#[test]
fn missing_identity_unexpected_team_period_and_role_are_visible() {
    type Case = (IssueCode, fn(&mut Shift));
    let cases: Vec<Case> = vec![
        (IssueCode::MissingPlayerId, |s| s.player_id = None),
        (IssueCode::MissingTeamId, |s| s.nhl_team_id = None),
        (IssueCode::UnknownTeamId, |s| s.franchise_id = None),
        (IssueCode::UnexpectedTeam, |s| s.franchise_id = Some(999)),
        (IssueCode::MissingPeriod, |s| s.period = None),
        (IssueCode::UnexpectedPeriod, |s| s.period = Some(5)),
        (IssueCode::OutOfBounds, |s| {
            s.end_time = Some("20:01".into());
            s.end_time_seconds = Some(1201);
        }),
    ];
    for (code, modify) in cases {
        let mut source = fixture();
        modify(&mut source.shifts[0]);
        let report = on_ice::analyze(&source);
        assert!(report.validation_counts.contains_key(&code), "{code:?}");
        assert_eq!(report.events[0].status, ReconstructionStatus::Incomplete);
    }
    let mut source = fixture();
    source.official_toi.retain(|p| p.player_id != 11);
    for s in source.shifts.iter_mut().filter(|s| s.player_id == Some(11)) {
        s.player_position = None;
    }
    let report = on_ice::analyze(&source);
    assert_eq!(report.events[0].possible.home.unknown, vec![11]);
    assert_eq!(
        report.events[0].situation_agreement,
        Agreement::NotComparable
    );
}

#[test]
fn pulls_delayed_penalties_and_ot_are_contexts_not_repairs() {
    let mut source = fixture();
    source
        .shifts
        .retain(|s| s.player_id != Some(26) || s.period != Some(1));
    source.shifts.push(shift(999, 27, 39, 1, 0, 1200));
    source.events[0].event_type = "delayed-penalty".into();
    source.events[0].situation_code = Some("0651".into());
    source.events[0].away_skater_count = Some(6);
    source.events[0].away_goalie_present = Some(false);
    let report = on_ice::analyze(&source);
    assert_eq!(report.events[0].situation_agreement, Agreement::Exact);
    assert!(report.events[0]
        .contexts
        .contains(&"goalie_absence_expected".into()));
    assert!(report.events[0]
        .contexts
        .contains(&"delayed_penalty_event".into()));
    source.events[0].situation_code = Some("1010".into());
    assert!(on_ice::analyze(&source).events[0]
        .contexts
        .contains(&"penalty_shot_count_pattern".into()));
    source.events[0].situation_code = Some("1551".into());
    assert_eq!(
        on_ice::analyze(&source).events[0].situation_agreement,
        Agreement::Mismatch
    );
    source.events = vec![event(1, 4, 60)];
    source.events[0].situation_code = Some("1331".into());
    source.events[0].home_skater_count = Some(3);
    source.events[0].away_skater_count = Some(3);
    for (team, base) in [(38, 10), (39, 20)] {
        for n in [1, 2, 3, 6] {
            source
                .shifts
                .push(shift(1000 + base + n, base + n, team, 4, 0, 300));
        }
    }
    assert_eq!(
        on_ice::analyze(&source).events[0].situation_agreement,
        Agreement::Exact
    );
    source.game.game_type = 3;
    source.events[0].time_in_period = "06:00".into();
    for s in source.shifts.iter_mut().filter(|s| s.period == Some(4)) {
        s.end_time = Some("20:00".into());
        s.end_time_seconds = Some(1200);
        s.duration = Some("20:00".into());
        s.duration_seconds = Some(1200);
    }
    assert_eq!(
        on_ice::analyze(&source).events[0].status,
        ReconstructionStatus::Resolved
    );
}

#[test]
fn absent_sources_and_official_zero_are_distinct_from_invalid_intervals() {
    let mut source = fixture();
    source.official_toi[0].time_on_ice_seconds = Some(0);
    source.shifts.clear();
    let report = on_ice::analyze(&source);
    assert_eq!(report.events[0].status, ReconstructionStatus::NoShifts);
    assert_eq!(report.toi[0].category, ToiCategory::OfficialZeroNoShifts);
    assert_eq!(report.toi[1].category, ToiCategory::MissingShifts);
    assert_eq!(report.toi[1].difference_seconds, None);
    source.game.season = 20092010;
    assert!(!on_ice::analyze(&source).supported);
    assert_eq!(
        on_ice::analyze(&source).events[0].status,
        ReconstructionStatus::Unsupported
    );
}

#[tokio::test]
async fn offline_audit_is_replayable_and_rejects_truncation_and_overwrite() {
    let path = std::env::temp_dir().join(format!("pucks-on-ice-{}.jsonl", std::process::id()));
    let source = fixture();
    let header = audit::SnapshotHeader {
        snapshot_version: 1,
        season: 20252026,
        games: 1,
        source_snapshot_at: "fixture".into(),
    };
    std::fs::write(
        &path,
        format!(
            "{}\n{}\n",
            serde_json::to_string(&header).unwrap(),
            serde_json::to_string(&source).unwrap()
        ),
    )
    .unwrap();
    let options = || audit::AuditOptions {
        season: 20252026,
        input: Some(path.clone()),
        ..Default::default()
    };
    let first = audit::run(None, options()).await.unwrap();
    let second = audit::run(None, options()).await.unwrap();
    assert_eq!(first.source_sha256, second.source_sha256);
    assert_eq!(first.eligible_games, 1);
    assert_eq!(first.resolved_with_matching_counts, 1);
    assert_eq!(first.toi_distribution.compared_players, 12);
    assert_eq!(first.toi_distribution.p95_absolute_seconds, Some(0));
    assert!(audit::write_report(Some(&path), &first).is_err());
    for status in ["unavailable", "failed"] {
        let mut missing = fixture();
        missing.shifts.clear();
        missing.game.fetch_status = Some(status.into());
        std::fs::write(
            &path,
            format!(
                "{}\n{}\n",
                serde_json::to_string(&header).unwrap(),
                serde_json::to_string(&missing).unwrap()
            ),
        )
        .unwrap();
        let report = audit::run(None, options()).await.unwrap();
        assert_eq!(report.availability[status], 1);
        assert_eq!(report.reconstruction[&ReconstructionStatus::NoShifts], 1);
    }
    let mut unsupported = fixture();
    unsupported.game.season = 20092010;
    let old_header = audit::SnapshotHeader {
        season: 20092010,
        ..header.clone()
    };
    std::fs::write(
        &path,
        format!(
            "{}\n{}\n",
            serde_json::to_string(&old_header).unwrap(),
            serde_json::to_string(&unsupported).unwrap()
        ),
    )
    .unwrap();
    let report = audit::run(
        None,
        audit::AuditOptions {
            season: 20092010,
            ..options()
        },
    )
    .await
    .unwrap();
    assert_eq!(report.eligible_games, 0);
    assert_eq!(report.unsupported_games, 1);
    assert_eq!(report.availability["unsupported"], 1);

    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string(&header).unwrap()),
    )
    .unwrap();
    assert!(audit::run(None, options())
        .await
        .unwrap_err()
        .to_string()
        .contains("incomplete"));
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn database_audit_is_read_only_and_coverage_excludes_unsupported_seasons() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let gid = 1900020888_i64;
    let old_gid = 1900020889_i64;
    sqlx::query("DELETE FROM events WHERE game_id IN ($1,$2)")
        .bind(gid)
        .bind(old_gid)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM games WHERE game_id IN ($1,$2)")
        .bind(gid)
        .bind(old_gid)
        .execute(pool)
        .await
        .unwrap();
    for (id, abbrev) in [(38, "VGK"), (39, "SEA")] {
        sqlx::query(
            "INSERT INTO teams (team_id,full_name,common_name,place_name,abbrev)
                    VALUES ($1,$2,$2,'Test',$2) ON CONFLICT (team_id) DO NOTHING",
        )
        .bind(id as i64)
        .bind(abbrev)
        .execute(pool)
        .await
        .unwrap();
    }
    for (id, season) in [(gid, 20252026), (old_gid, 20092010)] {
        sqlx::query("INSERT INTO games (game_id,season,game_date,home_team_id,away_team_id,game_type,game_state)
                    VALUES ($1,$2,'2025-10-07',38,39,2,'OFF')")
            .bind(id).bind(season).execute(pool).await.unwrap();
    }
    sqlx::query("INSERT INTO events (game_id,event_id_in_game,period,period_type,time_in_period,event_type,
                    situation_code,strength_source,season,game_type,game_date)
                    VALUES ($1,1,1,'REG','00:30','shot-on-goal','1551','situation_code',20252026,2,'2025-10-07')")
        .bind(gid).execute(pool).await.unwrap();
    for s in fixture().shifts {
        sqlx::query("INSERT INTO shifts (game_id,source_shift_id,type_code,player_id,team_id,period,
                   start_time,end_time,duration,start_time_seconds,end_time_seconds,duration_seconds,event_details)
                   VALUES ($1,$2,517,$3,$4,$5,$6,$7,$8,$9,$10,$11,'unchanged source metadata')")
            .bind(gid).bind(s.source_shift_id).bind(s.player_id).bind(s.nhl_team_id).bind(s.period)
            .bind(s.start_time).bind(s.end_time).bind(s.duration)
            .bind(s.start_time_seconds).bind(s.end_time_seconds).bind(s.duration_seconds)
            .execute(pool).await.unwrap();
    }
    for p in fixture().official_toi {
        if p.player_type == "goalie" {
            sqlx::query(
                "INSERT INTO analytics.official_goalie_games
                        (game_id,player_id,season,game_type,full_name,time_on_ice_seconds)
                        VALUES ($1,$2,20252026,2,'Test Goalie',3600)",
            )
            .bind(gid)
            .bind(p.player_id)
            .execute(pool)
            .await
            .unwrap();
        } else {
            sqlx::query("INSERT INTO analytics.official_skater_games
                        (game_id,player_id,season,game_type,full_name,position_code,time_on_ice_seconds)
                        VALUES ($1,$2,20252026,2,'Test Skater','C',3600)")
                .bind(gid).bind(p.player_id).execute(pool).await.unwrap();
        }
    }
    // Fingerprint all canonical columns, including metadata unused by analysis.
    const FINGERPRINT: &str = "SELECT jsonb_build_object(
        'shifts',(SELECT jsonb_agg(to_jsonb(s) ORDER BY source_shift_id) FROM shifts s WHERE game_id=$1),
        'events',(SELECT jsonb_agg(to_jsonb(e) ORDER BY id) FROM events e WHERE game_id=$1),
        'skaters',(SELECT jsonb_agg(to_jsonb(p) ORDER BY player_id) FROM analytics.official_skater_games p WHERE game_id=$1),
        'goalies',(SELECT jsonb_agg(to_jsonb(p) ORDER BY player_id) FROM analytics.official_goalie_games p WHERE game_id=$1))";
    let before: serde_json::Value = sqlx::query_scalar(FINGERPRINT)
        .bind(gid)
        .fetch_one(pool)
        .await
        .unwrap();
    let report = audit::game(pool, gid).await.unwrap();
    assert_eq!(report.events[0].status, ReconstructionStatus::Resolved);
    assert_eq!(report.events[0].situation_agreement, Agreement::Exact);
    assert_eq!(report.role_fallback_players, 0);
    assert!(report.toi.iter().all(|p| p.category == ToiCategory::Exact));
    let after: serde_json::Value = sqlx::query_scalar(FINGERPRINT)
        .bind(gid)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    let old: (bool, String) = sqlx::query_as(
        "SELECT eligible,availability FROM observability.shift_game_coverage WHERE game_id=$1",
    )
    .bind(old_gid)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(old, (false, "unsupported".into()));
    let loaded: String = sqlx::query_scalar(
        "SELECT availability FROM observability.shift_game_coverage WHERE game_id=$1",
    )
    .bind(gid)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(loaded, "loaded");
    let mut tx = on_ice::read::snapshot(pool).await.unwrap();
    let games = on_ice::read::games(&mut tx, None, Some(gid)).await.unwrap();
    let plans = on_ice::read::profile(&mut tx, &games).await.unwrap();
    assert_eq!(plans.len(), 3);
    let err = sqlx::query("UPDATE shifts SET duration='00:00' WHERE game_id=$1")
        .bind(gid)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(
        err.as_database_error().unwrap().code().as_deref(),
        Some("25006")
    );
    tx.rollback().await.unwrap();
    sqlx::query("DELETE FROM events WHERE game_id=$1")
        .bind(gid)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM games WHERE game_id IN ($1,$2)")
        .bind(gid)
        .bind(old_gid)
        .execute(pool)
        .await
        .unwrap();
}

#[test]
fn toi_categories_distinguish_small_large_missing_and_conflicting_references() {
    for (official, category, difference) in [
        (Some(3600), ToiCategory::Exact, Some(0)),
        (Some(3599), ToiCategory::WithinOneSecond, Some(1)),
        (Some(3605), ToiCategory::WithinFiveSeconds, Some(-5)),
        (Some(3570), ToiCategory::WithinThirtySeconds, Some(30)),
        (Some(4000), ToiCategory::LargeDifference, Some(-400)),
        (None, ToiCategory::MissingOfficial, None),
        (Some(-1), ToiCategory::InvalidOfficial, None),
    ] {
        let mut source = fixture();
        source.official_toi[0].time_on_ice_seconds = official;
        let report = on_ice::analyze(&source);
        let p = &report.toi[0];
        assert_eq!(p.category, category);
        assert_eq!(p.difference_seconds, difference);
    }
    let mut source = fixture();
    let mut conflicting = source.official_toi[0].clone();
    conflicting.player_type = "goalie".into();
    conflicting.position_code = Some("G".into());
    source.official_toi.push(conflicting);
    let report = on_ice::analyze(&source);
    assert_eq!(report.toi[0].category, ToiCategory::ConflictingOfficial);
    assert_eq!(report.toi[0].official_seconds, None);
    assert_eq!(report.events[0].status, ReconstructionStatus::Incomplete);
    assert!(report
        .validation_counts
        .contains_key(&IssueCode::ConflictingPlayerRole));
}

#[test]
fn incomplete_counts_missing_periods_and_invalid_events_are_not_certified() {
    let mut source = fixture();
    source.shifts.retain(|s| s.franchise_id != Some(39));
    let report = on_ice::analyze(&source);
    assert_eq!(report.events[0].status, ReconstructionStatus::Incomplete);
    assert!(report
        .validation_counts
        .contains_key(&IssueCode::MissingTeamShifts));
    assert!(report
        .validation_counts
        .contains_key(&IssueCode::MissingPeriodShifts));
    source.events[0].time_in_period = "20:01".into();
    assert_eq!(
        on_ice::analyze(&source).events[0].status,
        ReconstructionStatus::InvalidEventTime
    );
    let mut source = fixture();
    source.events[0].situation_code = Some("garbage".into());
    assert_eq!(
        on_ice::analyze(&source).events[0].situation_agreement,
        Agreement::InvalidCode
    );
    source.events[0].situation_code = Some("1551".into());
    source.events[0].strength_source = "html_report".into();
    assert_eq!(
        on_ice::analyze(&source).events[0].situation_agreement,
        Agreement::Unavailable
    );
    source.events[0].strength_source = "situation_code".into();
    source.events[0].home_skater_count = Some(4);
    assert_eq!(
        on_ice::analyze(&source).events[0].decoded_fields_agree,
        Some(false)
    );
}

#[test]
fn sweep_agrees_with_independent_point_queries_over_overlaps_gaps_and_boundaries() {
    // Direct point queries are deliberately slower and independent of the production sweep.
    let mut seed = 19_u64;
    for _ in 0..20 {
        let mut source = fixture();
        source
            .shifts
            .retain(|s| s.player_id != Some(11) || s.period != Some(1));
        let mut intervals = vec![];
        for id in 0..15 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let a = ((seed >> 32) % 60) as i32;
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let b = ((seed >> 32) % 60) as i32;
            let (start, end) = (a.min(b), a.max(b));
            intervals.push((start, end));
            source.shifts.push(shift(1000 + id, 11, 38, 1, start, end));
        }
        source.events = (0..=60).map(|t| event(t + 1, 1, t)).collect();
        let report = on_ice::analyze(&source);
        for (t, e) in report.events.iter().enumerate() {
            let t = t as i32;
            let before = intervals.iter().any(|&(s, end)| s < t && t <= end);
            let after = intervals.iter().any(|&(s, end)| s <= t && t < end);
            let possible = intervals.iter().any(|&(s, end)| s <= t && t <= end);
            assert_eq!(e.definite.home.skaters.contains(&11), before && after);
            assert_eq!(e.possible.home.skaters.contains(&11), possible);
        }
    }
}

#[test]
fn period_start_gaps_and_missing_durations_remain_explicit() {
    let mut source = fixture();
    for row in source.shifts.iter_mut().filter(|s| s.period == Some(1)) {
        row.start_time = Some("00:10".into());
        row.start_time_seconds = Some(10);
        row.duration = None;
        row.duration_seconds = None;
    }
    source.events = vec![event(1, 1, 5), event(2, 1, 30)];
    let report = on_ice::analyze(&source);
    assert_eq!(report.rejected_rows, 0);
    assert_eq!(report.validation_counts[&IssueCode::MissingDuration], 12);
    assert_eq!(
        report.validation_counts[&IssueCode::SuspiciousOnIceCount],
        2
    );
    assert_eq!(report.events[0].status, ReconstructionStatus::Incomplete);
    assert_eq!(report.events[1].status, ReconstructionStatus::Resolved);
}
