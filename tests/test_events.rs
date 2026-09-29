mod common;

#[test]
fn test_situation_code_decode() {
    use pucksdata::fetchers::events::decode_situation_code;

    let cases = [
        ("1551", true, 5, 5, true),
        ("1451", true, 4, 5, true),
        ("1541", true, 5, 4, true),
        ("0651", false, 6, 5, true),
        ("1560", true, 5, 6, false),
        ("1331", true, 3, 3, true),
        ("1341", true, 3, 4, true),
        ("0641", false, 6, 4, true),
    ];

    for (code, away_goalie, away_skaters, home_skaters, home_goalie) in cases {
        let situation = decode_situation_code(code).unwrap();
        assert_eq!(situation.away_goalie_present, away_goalie, "{code}");
        assert_eq!(situation.away_skater_count, away_skaters, "{code}");
        assert_eq!(situation.home_skater_count, home_skaters, "{code}");
        assert_eq!(situation.home_goalie_present, home_goalie, "{code}");
    }

    for malformed in ["", "155", "15511", "15x1", "2551", "1552"] {
        assert_eq!(decode_situation_code(malformed), None, "{malformed}");
    }
}

#[test]
fn test_strength_for_owner() {
    use pucksdata::fetchers::events::{decode_situation_code, strength_for_owner};

    let strength = |code, owner_is_home| {
        strength_for_owner(&decode_situation_code(code).unwrap(), owner_is_home)
    };

    assert_eq!(strength("1451", Some(true)), Some("pp"));
    assert_eq!(strength("1451", Some(false)), Some("sh"));
    assert_eq!(strength("0651", Some(true)), Some("ev"));
    assert_eq!(strength("0651", Some(false)), Some("ev"));
    assert_eq!(strength("0641", Some(false)), Some("pp"));
    assert_eq!(strength("0641", Some(true)), Some("sh"));
    assert_eq!(strength("1331", Some(true)), Some("ev"));
    assert_eq!(strength("1331", Some(false)), Some("ev"));
    assert_eq!(strength("1341", Some(true)), Some("pp"));
    assert_eq!(strength("1441", Some(true)), Some("ev"));
    assert_eq!(strength("1441", Some(false)), Some("ev"));
    assert_eq!(strength("1451", None), None);
}

#[test]
fn test_goal_details_deserialize() {
    use pucksdata::fetchers::events::EventDetails;

    let json = r#"{
        "xCoord": -56,
        "yCoord": 8,
        "zoneCode": "O",
        "shotType": "wrist",
        "scoringPlayerId": 8480801,
        "assist1PlayerId": 8478476,
        "assist2PlayerId": 8481533,
        "goalieInNetId": 8480382,
        "eventOwnerTeamId": 14
    }"#;
    let details: EventDetails = serde_json::from_str(json).unwrap();
    assert_eq!(details.scoring_player_id, Some(8480801_i64));
    assert_eq!(details.assist1_player_id, Some(8478476_i64));
    assert_eq!(details.assist2_player_id, Some(8481533_i64));
    assert_eq!(details.goalie_in_net_id, Some(8480382_i64));
    assert_eq!(details.shot_type.as_deref(), Some("wrist"));
    assert_eq!(details.x_coord, Some(-56_i16));
    assert_eq!(details.y_coord, Some(8_i16));
    assert_eq!(details.zone_code.as_deref(), Some("O"));

    let json_minimal = r#"{
        "xCoord": -56,
        "yCoord": 8,
        "zoneCode": "O",
        "shotType": "wrist",
        "scoringPlayerId": 8480801,
        "eventOwnerTeamId": 14
    }"#;
    let d2: EventDetails = serde_json::from_str(json_minimal).unwrap();
    assert_eq!(d2.assist1_player_id, None);
    assert_eq!(d2.assist2_player_id, None);
    assert_eq!(d2.goalie_in_net_id, None);

    let json_en = r#"{
        "xCoord": -70,
        "yCoord": 0,
        "zoneCode": "O",
        "shotType": "wrist",
        "scoringPlayerId": 8480801,
        "goalieInNetId": null,
        "eventOwnerTeamId": 14
    }"#;
    let d3: EventDetails = serde_json::from_str(json_en).unwrap();
    assert_eq!(
        d3.goalie_in_net_id, None,
        "null goalieInNetId should deserialize to None"
    );
}

#[test]
fn test_shot_details_deserialize() {
    use pucksdata::fetchers::events::EventDetails;

    let json = r#"{
        "xCoord": 65,
        "yCoord": -12,
        "zoneCode": "O",
        "shootingPlayerId": 8479318,
        "goalieInNetId": 8477293,
        "shotType": "slap"
    }"#;
    let details: EventDetails = serde_json::from_str(json).unwrap();
    assert_eq!(details.shooting_player_id, Some(8479318_i64));
    assert_eq!(details.goalie_in_net_id, Some(8477293_i64));
    assert_eq!(details.shot_type.as_deref(), Some("slap"));
    assert_eq!(details.x_coord, Some(65_i16));
    assert_eq!(details.y_coord, Some(-12_i16));
    assert_eq!(details.zone_code.as_deref(), Some("O"));
}

#[test]
fn test_hit_details_deserialize() {
    use pucksdata::fetchers::events::EventDetails;

    let json = r#"{
        "xCoord": 20,
        "yCoord": -30,
        "zoneCode": "N",
        "hittingPlayerId": 8478550,
        "hitteePlayerId": 8479355,
        "eventOwnerTeamId": 14
    }"#;
    let details: EventDetails = serde_json::from_str(json).unwrap();
    assert_eq!(details.hitting_player_id, Some(8478550_i64));
    assert_eq!(details.hittee_player_id, Some(8479355_i64));
    assert_eq!(details.x_coord, Some(20_i16));
    assert_eq!(details.y_coord, Some(-30_i16));
    assert_eq!(details.zone_code.as_deref(), Some("N"));
}

#[test]
fn test_block_details_deserialize() {
    use pucksdata::fetchers::events::EventDetails;

    let json = r#"{
        "xCoord": 55,
        "yCoord": 10,
        "zoneCode": "D",
        "blockingPlayerId": 8476412,
        "shootingPlayerId": 8481533,
        "eventOwnerTeamId": 18
    }"#;
    let details: EventDetails = serde_json::from_str(json).unwrap();
    assert_eq!(details.blocking_player_id, Some(8476412_i64));
    assert_eq!(details.shooting_player_id, Some(8481533_i64));
    assert_eq!(details.x_coord, Some(55_i16));
    assert_eq!(details.y_coord, Some(10_i16));
    assert_eq!(details.zone_code.as_deref(), Some("D"));
}

#[test]
fn test_penalty_details_deserialize() {
    use pucksdata::fetchers::events::EventDetails;

    // Penalties use `duration` and `descKey`, unlike several related NHL payloads.
    let json = r#"{
        "xCoord": 10,
        "yCoord": -20,
        "zoneCode": "N",
        "typeCode": "MIN",
        "descKey": "high-sticking",
        "duration": 2,
        "committedByPlayerId": 8479318,
        "drawnByPlayerId": 8480801,
        "eventOwnerTeamId": 18
    }"#;
    let details: EventDetails = serde_json::from_str(json).unwrap();
    assert_eq!(
        details.duration,
        Some(2_i16),
        "duration should be 2 (integer minutes)"
    );
    assert_eq!(
        details.desc_key.as_deref(),
        Some("high-sticking"),
        "descKey → desc_key"
    );
    assert_eq!(details.type_code.as_deref(), Some("MIN"));
    assert_eq!(details.committed_by_player_id, Some(8479318_i64));
    assert_eq!(details.drawn_by_player_id, Some(8480801_i64));

    let json_bench = r#"{
        "typeCode": "MIN",
        "descKey": "too-many-men",
        "duration": 2,
        "committedByPlayerId": null,
        "eventOwnerTeamId": 18
    }"#;
    let d2: EventDetails = serde_json::from_str(json_bench).unwrap();
    assert_eq!(
        d2.drawn_by_player_id, None,
        "bench minor: drawnByPlayerId should be None"
    );
    assert_eq!(
        d2.committed_by_player_id, None,
        "bench minor: committedByPlayerId null → None"
    );
}

#[test]
fn test_faceoff_details_deserialize() {
    use pucksdata::fetchers::events::EventDetails;

    let json = r#"{
        "xCoord": 0,
        "yCoord": 0,
        "zoneCode": "N",
        "winningPlayerId": 8476346,
        "losingPlayerId": 8479323,
        "eventOwnerTeamId": 14
    }"#;
    let details: EventDetails = serde_json::from_str(json).unwrap();
    assert_eq!(details.winning_player_id, Some(8476346_i64));
    assert_eq!(details.losing_player_id, Some(8479323_i64));
    assert_eq!(details.x_coord, Some(0_i16));
    assert_eq!(details.y_coord, Some(0_i16));
    assert_eq!(details.zone_code.as_deref(), Some("N"));
}

// Goal-derived shot idempotency.

#[test]
fn test_goal_produces_shot_entry() {
    use pucksdata::fetchers::events::{
        transform_events, EventDetails, PbpTeam, PeriodDescriptor, Play, PlayByPlay,
    };
    use std::collections::HashMap;

    let pbp = PlayByPlay {
        id: 2025020004,
        home_team: PbpTeam {
            id: 10,
            abbrev: None,
        },
        away_team: PbpTeam {
            id: 8,
            abbrev: None,
        },
        plays: vec![
            Play {
                event_id: 1081,
                period_descriptor: PeriodDescriptor {
                    number: 3,
                    period_type: "REG".to_string(),
                },
                time_in_period: "18:28".to_string(),
                situation_code: Some("0651".to_string()),
                type_desc_key: "goal".to_string(),
                details: Some(EventDetails {
                    x_coord: Some(-87),
                    y_coord: Some(-6),
                    zone_code: Some("O".to_string()),
                    event_owner_team_id: Some(10),
                    scoring_player_id: Some(8479318),
                    assist1_player_id: Some(8477939),
                    assist2_player_id: None,
                    goalie_in_net_id: None,
                    shot_type: Some("wrist".to_string()),
                    shooting_player_id: None,
                    reason: None,
                    player_id: None,
                    hitting_player_id: None,
                    hittee_player_id: None,
                    blocking_player_id: None,
                    type_code: None,
                    desc_key: None,
                    duration: None,
                    committed_by_player_id: None,
                    drawn_by_player_id: None,
                    winning_player_id: None,
                    losing_player_id: None,
                }),
            },
            Play {
                event_id: 200,
                period_descriptor: PeriodDescriptor {
                    number: 1,
                    period_type: "REG".to_string(),
                },
                time_in_period: "15:00".to_string(),
                situation_code: Some("1551".to_string()),
                type_desc_key: "shot-on-goal".to_string(),
                details: Some(EventDetails {
                    x_coord: Some(65),
                    y_coord: Some(-12),
                    zone_code: Some("O".to_string()),
                    event_owner_team_id: Some(8),
                    scoring_player_id: None,
                    assist1_player_id: None,
                    assist2_player_id: None,
                    goalie_in_net_id: Some(8480382),
                    shot_type: Some("slap".to_string()),
                    shooting_player_id: Some(8479318),
                    reason: None,
                    player_id: None,
                    hitting_player_id: None,
                    hittee_player_id: None,
                    blocking_player_id: None,
                    type_code: None,
                    desc_key: None,
                    duration: None,
                    committed_by_player_id: None,
                    drawn_by_player_id: None,
                    winning_player_id: None,
                    losing_player_id: None,
                }),
            },
        ],
    };

    let team_id_map = HashMap::new();
    let pucksdata::models::EventBatch {
        events,
        goals,
        shots,
        warnings,
        ..
    } = transform_events(&pbp, &team_id_map);

    assert!(
        warnings.is_empty(),
        "no skip warnings expected: {:?}",
        warnings
    );
    assert_eq!(goals.len(), 1, "expected 1 goal");
    assert_eq!(
        shots.len(),
        2,
        "expected 2 shots (goal-derived + shot-on-goal)"
    );

    let goal_event = events
        .iter()
        .find(|event| event.event_id_in_game == 1081)
        .expect("goal event must be present");
    assert_eq!(goal_event.away_goalie_present, Some(false));
    assert_eq!(goal_event.away_skater_count, Some(6));
    assert_eq!(goal_event.home_skater_count, Some(5));
    assert_eq!(goal_event.home_goalie_present, Some(true));
    assert_eq!(goal_event.strength.as_deref(), Some("ev"));
    assert_eq!(
        goal_event.strength_source,
        pucksdata::models::StrengthSource::SituationCode
    );
    assert_eq!(goal_event.situation_code.as_deref(), Some("0651"));

    let goal_shot = shots
        .iter()
        .find(|s| s.event_id_in_game == 1081)
        .expect("goal-derived shot must have the goal event ID");
    assert_eq!(
        goal_shot.shooting_player_id,
        Some(8479318),
        "goal scorer maps to shooting_player_id"
    );
    assert_eq!(
        goal_shot.goalie_in_net_id, None,
        "empty-net goal has no goalie_in_net_id"
    );
    assert_eq!(
        goal_shot.shot_type.as_deref(),
        Some("wrist"),
        "shot_type carried through from goal event"
    );

    let reg_shot = shots
        .iter()
        .find(|s| s.event_id_in_game == 200)
        .expect("regular shot-on-goal must be present");
    assert_eq!(reg_shot.shooting_player_id, Some(8479318));
}

#[test]
fn test_missing_situation_uses_goal_summary_without_fabricating_on_ice_state() {
    use pucksdata::fetchers::events::{
        transform_events, transform_events_with_goal_strengths,
        transform_events_with_strength_sources, EventStrength, PlayByPlay,
    };
    use pucksdata::models::StrengthSource;
    use std::collections::HashMap;

    let pbp: PlayByPlay = serde_json::from_str(
        r#"{
            "id": 2005020001,
            "homeTeam": {"id": 6},
            "awayTeam": {"id": 8},
            "plays": [{
                "eventId": 10088563,
                "periodDescriptor": {"number": 3, "periodType": "REG"},
                "timeInPeriod": "19:48",
                "typeDescKey": "goal",
                "details": {
                    "eventOwnerTeamId": 8,
                    "scoringPlayerId": 8467545,
                    "shotType": "slap"
                }
            }]
        }"#,
    )
    .unwrap();
    let teams = HashMap::from([(6, 6), (8, 8)]);

    let pucksdata::models::EventBatch { events, .. } = transform_events(&pbp, &teams);
    let event = &events[0];
    assert_eq!(event.strength, None);
    assert_eq!(event.strength_source, StrengthSource::Unavailable);
    assert_eq!(event.situation_code, None);
    assert_eq!(event.away_goalie_present, None);
    assert_eq!(event.away_skater_count, None);
    assert_eq!(event.home_skater_count, None);
    assert_eq!(event.home_goalie_present, None);

    let strengths = HashMap::from([(10088563, EventStrength::PowerPlay)]);
    let pucksdata::models::EventBatch { events, .. } =
        transform_events_with_goal_strengths(&pbp, &teams, &strengths);
    let event = &events[0];
    assert_eq!(event.strength.as_deref(), Some("pp"));
    assert_eq!(event.strength_source, StrengthSource::ScoringSummary);
    assert_eq!(event.situation_code, None);
    assert_eq!(event.away_goalie_present, None);
    assert_eq!(event.away_skater_count, None);
    assert_eq!(event.home_skater_count, None);
    assert_eq!(event.home_goalie_present, None);

    let report_strengths = HashMap::from([(10088563, EventStrength::ShortHanded)]);
    let pucksdata::models::EventBatch { events, .. } =
        transform_events_with_strength_sources(&pbp, &teams, &HashMap::new(), &report_strengths);
    let event = &events[0];
    assert_eq!(event.strength.as_deref(), Some("sh"));
    assert_eq!(event.strength_source, StrengthSource::HtmlReport);
}

// Database integration tests.

#[tokio::test]
async fn test_events_upsert_idempotent() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;

    sqlx::query!(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99001, 'Test Home', 'Home', 'Testville', 'HME'),
                (99002, 'Test Away', 'Away', 'Testville', 'AWY')
         ON CONFLICT (team_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query!(
        "INSERT INTO games (game_id, season, game_date, home_team_id, away_team_id, game_type)
         VALUES (9900000002, 20232024, '2024-01-01', 99001, 99002, 2)
         ON CONFLICT (game_id) DO NOTHING"
    )
    .execute(pool)
    .await
    .unwrap();

    let missing_scope = sqlx::query(
        "INSERT INTO events
             (game_id, event_id_in_game, period, period_type, time_in_period, event_type)
         VALUES (9900000002, 999, 1, 'REG', '00:00', 'goal')",
    )
    .execute(pool)
    .await;
    assert!(
        missing_scope.is_err(),
        "event scope columns must reject an insert that does not populate them"
    );

    let event = pucksdata::models::DbEvent {
        game_id: 9900000002,
        event_id_in_game: 1,
        period: 1,
        period_type: "REG".into(),
        time_in_period: "05:00".into(),
        event_type: "goal".into(),
        x_coord: None,
        y_coord: None,
        zone_code: None,
        event_owner_team_id: Some(99001),
        home_goalie_present: Some(true),
        home_skater_count: Some(5),
        away_skater_count: Some(5),
        away_goalie_present: Some(true),
        strength: Some("ev".into()),
        strength_source: pucksdata::models::StrengthSource::SituationCode,
        situation_code: Some("1551".into()),
    };
    let goal = pucksdata::models::DbGoal {
        event_id_in_game: 1,
        scorer_player_id: None,
        assist1_player_id: None,
        assist2_player_id: None,
        goalie_id: None,
        shot_type: None,
    };
    let stale_event = pucksdata::models::DbEvent {
        game_id: 9900000002,
        event_id_in_game: 2,
        period: 1,
        period_type: "REG".into(),
        time_in_period: "06:00".into(),
        event_type: "shot-on-goal".into(),
        x_coord: None,
        y_coord: None,
        zone_code: None,
        event_owner_team_id: Some(99002),
        home_goalie_present: Some(true),
        home_skater_count: Some(5),
        away_skater_count: Some(5),
        away_goalie_present: Some(true),
        strength: Some("ev".into()),
        strength_source: pucksdata::models::StrengthSource::SituationCode,
        situation_code: Some("1551".into()),
    };

    // First snapshot contains an event later removed by an NHL feed revision.
    let counts = pucksdata::loaders::events::upsert_game_events(
        pool,
        9900000002,
        &pucksdata::models::EventBatch {
            events: vec![event, stale_event],
            goals: vec![goal],
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(counts.events, 2);
    assert_eq!(counts.goals, 1);
    assert_eq!(counts.shots, 0);

    let event2 = pucksdata::models::DbEvent {
        game_id: 9900000002,
        event_id_in_game: 1,
        period: 1,
        period_type: "REG".into(),
        time_in_period: "05:00".into(),
        event_type: "goal".into(),
        x_coord: None,
        y_coord: None,
        zone_code: None,
        event_owner_team_id: Some(99001),
        home_goalie_present: Some(true),
        home_skater_count: Some(5),
        away_skater_count: Some(4),
        away_goalie_present: Some(true),
        strength: Some("pp".into()),
        strength_source: pucksdata::models::StrengthSource::SituationCode,
        situation_code: Some("1451".into()),
    };
    let goal2 = pucksdata::models::DbGoal {
        event_id_in_game: 1,
        scorer_player_id: None,
        assist1_player_id: None,
        assist2_player_id: None,
        goalie_id: None,
        shot_type: None,
    };

    let mut batch = pucksdata::models::EventBatch {
        events: vec![event2],
        goals: vec![goal2],
        ..Default::default()
    };
    let counts = pucksdata::loaders::events::upsert_game_events(pool, 9900000002, &batch)
        .await
        .unwrap();
    assert_eq!(counts.events, 1);
    assert_eq!(counts.goals, 1);

    // A child insert failure must restore the previously accepted snapshot.
    batch.events[0].strength = Some("sh".into());
    batch.goals.push(pucksdata::models::DbGoal {
        event_id_in_game: 1,
        scorer_player_id: None,
        assist1_player_id: None,
        assist2_player_id: None,
        goalie_id: None,
        shot_type: None,
    });
    assert!(matches!(
        pucksdata::loaders::events::upsert_game_events(pool, 9900000002, &batch).await,
        Err(pucksdata::error::LoadError::Database(_))
    ));

    let empty_counts = pucksdata::loaders::events::upsert_game_events(
        pool,
        9900000002,
        &pucksdata::models::EventBatch::default(),
    )
    .await
    .unwrap();
    assert_eq!(empty_counts, pucksdata::models::EventCounts::default());

    let event_count: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM events WHERE game_id = 9900000002")
            .fetch_one(pool)
            .await
            .unwrap()
            .unwrap_or(0);
    assert_eq!(event_count, 1, "upsert produced more than one event row");

    let goal_count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM goals g JOIN events e ON e.id = g.event_id WHERE e.game_id = 9900000002"
    ).fetch_one(pool).await.unwrap().unwrap_or(0);
    assert_eq!(goal_count, 1, "upsert produced more than one goal row");

    let (strength, strength_source, situation_code, away_skaters, season, game_type, game_date): (
        Option<String>,
        String,
        Option<String>,
        Option<i16>,
        i32,
        i16,
        String,
    ) = sqlx::query_as(
        "SELECT strength, strength_source, situation_code, away_skater_count,
                season, game_type, game_date::text
             FROM events WHERE game_id = 9900000002 AND event_id_in_game = 1",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(strength.as_deref(), Some("pp"));
    assert_eq!(strength_source, "situation_code");
    assert_eq!(situation_code.as_deref(), Some("1451"));
    assert_eq!(away_skaters, Some(4));
    assert_eq!(season, 20232024);
    assert_eq!(game_type, 2);
    assert_eq!(game_date, "2024-01-01");

    sqlx::query!(
        "DELETE FROM goals WHERE event_id IN (SELECT id FROM events WHERE game_id = 9900000002)"
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query!("DELETE FROM events WHERE game_id = 9900000002")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM games WHERE game_id = 9900000002")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query!("DELETE FROM teams WHERE team_id IN (99001, 99002)")
        .execute(pool)
        .await
        .unwrap();
}

fn extended_event_batch(game_id: i64) -> pucksdata::models::EventBatch {
    let pbp = serde_json::from_value(serde_json::json!({
        "id": game_id, "homeTeam": {"id": 1}, "awayTeam": {"id": 7},
        "plays": [
            {"eventId": 1, "periodDescriptor": {"number": 1, "periodType": "REG"},
             "timeInPeriod": "01:00", "typeDescKey": "missed-shot", "details": {
                 "xCoord": 71, "yCoord": -28, "zoneCode": "O", "reason": "short",
                 "shotType": "wrist", "shootingPlayerId": 8479407, "goalieInNetId": 8480045,
                 "eventOwnerTeamId": 1}},
            {"eventId": 2, "periodDescriptor": {"number": 1, "periodType": "REG"},
             "timeInPeriod": "02:00", "typeDescKey": "giveaway", "details": {"playerId": 8474593}},
            {"eventId": 3, "periodDescriptor": {"number": 1, "periodType": "REG"},
             "timeInPeriod": "03:00", "typeDescKey": "takeaway", "details": {"playerId": 8481528}}
        ]
    }))
    .unwrap();
    pucksdata::fetchers::events::transform_events(&pbp, &std::collections::HashMap::new())
}

#[test]
fn extended_events_preserve_nhl_attribution_and_missed_shot_reason() {
    // Detail fields sampled from the NHL feed for game 2024020001.
    let batch = extended_event_batch(2024020001);
    assert_eq!(batch.events.len(), 3);
    assert!(batch.shots.is_empty());
    assert!(batch.warnings.is_empty());
    let missed = &batch.missed_shots[0];
    assert_eq!(missed.shooting_player_id, Some(8479407));
    assert_eq!(missed.goalie_in_net_id, Some(8480045));
    assert_eq!(missed.shot_type.as_deref(), Some("wrist"));
    assert_eq!(missed.miss_reason.as_deref(), Some("short"));
    assert_eq!(batch.giveaways[0].player_id, Some(8474593));
    assert_eq!(batch.takeaways[0].player_id, Some(8481528));
}

#[test]
fn extended_events_distinguish_missing_details_from_unknown_attribution() {
    use pucksdata::fetchers::events::{transform_events, PlayByPlay};
    for kind in ["missed-shot", "giveaway", "takeaway"] {
        let mut value = serde_json::json!({"id": 2024020001, "homeTeam": {"id": 1}, "awayTeam": {"id": 7},
            "plays": [{"eventId": 1, "periodDescriptor": {"number": 1, "periodType": "REG"},
                "timeInPeriod": "01:00", "typeDescKey": kind, "details": {}}]});
        let pbp: PlayByPlay = serde_json::from_value(value.clone()).unwrap();
        let batch = transform_events(&pbp, &Default::default());
        assert!(batch.warnings.is_empty());
        assert_eq!(
            batch.missed_shots.len() + batch.giveaways.len() + batch.takeaways.len(),
            1
        );
        assert!(batch
            .missed_shots
            .iter()
            .all(|r| r.shooting_player_id.is_none() && r.miss_reason.is_none()));
        assert!(batch
            .giveaways
            .iter()
            .chain(batch.takeaways.iter())
            .all(|r| r.player_id.is_none()));
        value["plays"][0]["details"] = serde_json::Value::Null;
        let pbp = serde_json::from_value(value.clone()).unwrap();
        let batch = transform_events(&pbp, &Default::default());
        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.warnings.len(), 1);
        assert_eq!(
            batch.missed_shots.len() + batch.giveaways.len() + batch.takeaways.len(),
            0
        );
        value["plays"][0]["periodDescriptor"]["periodType"] = "SO".into();
        let pbp = serde_json::from_value(value).unwrap();
        assert!(transform_events(&pbp, &Default::default())
            .events
            .is_empty());
    }
}

#[tokio::test]
async fn extended_events_replace_atomically_and_preserve_history() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let game = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as i64;
    sqlx::query(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev) VALUES
        (99391,'Event Home','Home','Test','EHM'), (99392,'Event Away','Away','Test','EAW')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO games (game_id,season,game_date,home_team_id,away_team_id,game_type)
        VALUES ($1,20242025,'2024-10-04',99391,99392,2)",
    )
    .bind(game)
    .execute(pool)
    .await
    .unwrap();
    let mut batch = extended_event_batch(game);
    let counts = pucksdata::process::attempts::track(pool, "events", &game.to_string(), async {
        pucksdata::loaders::events::upsert_game_events(pool, game, &batch)
            .await
            .map_err(|error| -> pucksdata::AnyError { Box::new(error) })
    })
    .await
    .unwrap();
    assert_eq!(
        (counts.missed_shots, counts.giveaways, counts.takeaways),
        (1, 1, 1)
    );
    pucksdata::loaders::events::upsert_game_events(pool, game, &batch)
        .await
        .unwrap();
    let snapshots: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM history.snapshots WHERE dataset='events' AND entity_key=$1",
    )
    .bind(game.to_string())
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(snapshots, 1, "identical replays must not create revisions");
    let (method,payload): (String,serde_json::Value) = sqlx::query_as("SELECT method_version,payload FROM history.snapshots WHERE dataset='events' AND entity_key=$1")
        .bind(game.to_string()).fetch_one(pool).await.unwrap();
    assert_eq!(method, "normalized-events-v2");
    let linked: bool = sqlx::query_scalar("SELECT attempt_id IS NOT NULL FROM history.snapshots WHERE dataset='events' AND entity_key=$1")
        .bind(game.to_string()).fetch_one(pool).await.unwrap();
    assert!(
        linked,
        "new facts must retain their ingestion attempt linkage"
    );
    assert_eq!(payload[0]["missed_shot"]["miss_reason"], "short");
    assert_eq!(payload[1]["giveaway"]["player_id"], 8474593);
    assert_eq!(payload[2]["takeaway"]["player_id"], 8481528);
    let missing = pucksdata::fetchers::players::missing_event_player_ids(pool)
        .await
        .unwrap();
    for player in [8479407, 8480045, 8474593, 8481528] {
        let known: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM players WHERE player_id=$1)")
                .bind(player)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(
            missing.contains(&player),
            !known,
            "new event participant {player} must be discoverable"
        );
    }

    batch.missed_shots[0].miss_reason = Some("wide-right".into());
    batch.events.retain(|e| e.event_id_in_game != 3);
    batch.takeaways.clear();
    pucksdata::loaders::events::upsert_game_events(pool, game, &batch)
        .await
        .unwrap();
    batch.missed_shots[0].miss_reason = Some("invalid-replacement".into());
    batch.giveaways.push(pucksdata::models::DbTurnover {
        event_id_in_game: 2,
        player_id: None,
    });
    assert!(
        pucksdata::loaders::events::upsert_game_events(pool, game, &batch)
            .await
            .is_err()
    );
    let reason: String = sqlx::query_scalar("SELECT miss_reason FROM missed_shots m JOIN events e ON e.id=m.event_id WHERE e.game_id=$1")
        .bind(game).fetch_one(pool).await.unwrap();
    assert_eq!(reason, "wide-right");
    let removed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM takeaways t JOIN events e ON e.id=t.event_id WHERE e.game_id=$1",
    )
    .bind(game)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(removed, 0);
    let snapshots: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM history.snapshots WHERE dataset='events' AND entity_key=$1",
    )
    .bind(game.to_string())
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(snapshots, 2, "failed replacement must not write history");
    let empty = pucksdata::loaders::events::upsert_game_events(pool, game, &Default::default())
        .await
        .unwrap();
    assert_eq!(empty, pucksdata::models::EventCounts::default());
    for query in [
        "DELETE FROM missed_shots WHERE event_id IN (SELECT id FROM events WHERE game_id=$1)",
        "DELETE FROM giveaways WHERE event_id IN (SELECT id FROM events WHERE game_id=$1)",
        "DELETE FROM takeaways WHERE event_id IN (SELECT id FROM events WHERE game_id=$1)",
    ] {
        sqlx::query(query).bind(game).execute(pool).await.unwrap();
    }
    sqlx::query("DELETE FROM events WHERE game_id=$1")
        .bind(game)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM games WHERE game_id=$1")
        .bind(game)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM teams WHERE team_id IN (99391,99392)")
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn all_child_tables_share_atomic_insert_and_preserve_nulls() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let game = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as i64
        + 10_000_000_000_000_000;
    sqlx::query("INSERT INTO teams (team_id,full_name,common_name,place_name,abbrev) VALUES (99491,'Batch Home','Home','Test','BHM'),(99492,'Batch Away','Away','Test','BAW')").execute(pool).await.unwrap();
    sqlx::query("INSERT INTO games (game_id,season,game_date,home_team_id,away_team_id,game_type) VALUES ($1,20242025,'2024-10-04',99491,99492,2)").bind(game).execute(pool).await.unwrap();
    let details = [
        (
            "goal",
            serde_json::json!({"scoringPlayerId":101,"assist1PlayerId":102,"shotType":"wrist"}),
        ),
        (
            "shot-on-goal",
            serde_json::json!({"shootingPlayerId":103,"goalieInNetId":104,"shotType":"slap"}),
        ),
        (
            "missed-shot",
            serde_json::json!({"shootingPlayerId":105,"reason":"wide-left"}),
        ),
        ("giveaway", serde_json::json!({"playerId":106})),
        ("takeaway", serde_json::json!({})),
        (
            "hit",
            serde_json::json!({"hittingPlayerId":107,"hitteePlayerId":108}),
        ),
        (
            "blocked-shot",
            serde_json::json!({"shootingPlayerId":109,"blockingPlayerId":110}),
        ),
        (
            "penalty",
            serde_json::json!({"committedByPlayerId":111,"descKey":"hooking","duration":2}),
        ),
        (
            "faceoff",
            serde_json::json!({"winningPlayerId":112,"losingPlayerId":113}),
        ),
    ];
    let plays: Vec<_> = details.into_iter().enumerate().map(|(index,(kind,details))| serde_json::json!({
        "eventId":index+1,"periodDescriptor":{"number":1,"periodType":"REG"},"timeInPeriod":"01:00","typeDescKey":kind,"details":details
    })).collect();
    let pbp = serde_json::from_value(
        serde_json::json!({"id":game,"homeTeam":{"id":1},"awayTeam":{"id":7},"plays":plays}),
    )
    .unwrap();
    let mut batch = pucksdata::fetchers::events::transform_events(&pbp, &Default::default());
    let counts = pucksdata::loaders::events::upsert_game_events(pool, game, &batch)
        .await
        .unwrap();
    assert_eq!(
        counts,
        pucksdata::models::EventCounts {
            events: 9,
            goals: 1,
            shots: 2,
            missed_shots: 1,
            giveaways: 1,
            takeaways: 1,
            hits: 1,
            blocks: 1,
            penalties: 1,
            faceoffs: 1
        }
    );
    let values: serde_json::Value = sqlx::query_scalar("SELECT jsonb_build_object(
        'scorer',g.scorer_player_id,'assist',g.assist1_player_id,'assist2',g.assist2_player_id,'goalie',g.goalie_id,'type',g.shot_type,
        'hit',h.hitting_player_id,'hittee',h.hittee_player_id,'blocker',b.blocking_player_id,'shooter',b.shooting_player_id,
        'penalty',p.committed_by_player_id,'drawn',p.drawn_by_player_id,'infraction',p.infraction_type,'duration',p.duration_minutes,
        'winner',f.winning_player_id,'loser',f.losing_player_id)
        FROM events e JOIN goals g ON g.event_id=e.id
        JOIN events eh ON eh.game_id=e.game_id AND eh.event_type='hit' JOIN hits h ON h.event_id=eh.id
        JOIN events eb ON eb.game_id=e.game_id AND eb.event_type='blocked-shot' JOIN blocks b ON b.event_id=eb.id
        JOIN events ep ON ep.game_id=e.game_id AND ep.event_type='penalty' JOIN penalties p ON p.event_id=ep.id
        JOIN events ef ON ef.game_id=e.game_id AND ef.event_type='faceoff' JOIN faceoffs f ON f.event_id=ef.id
        WHERE e.game_id=$1").bind(game).fetch_one(pool).await.unwrap();
    assert_eq!(
        values,
        serde_json::json!({"scorer":101,"assist":102,"assist2":null,"goalie":null,"type":"wrist","hit":107,"hittee":108,"blocker":110,"shooter":109,"penalty":111,"drawn":null,"infraction":"hooking","duration":2,"winner":112,"loser":113})
    );
    batch.takeaways.push(pucksdata::models::DbTurnover {
        event_id_in_game: 999,
        player_id: Some(114),
    });
    assert!(matches!(
        pucksdata::loaders::events::upsert_game_events(pool, game, &batch).await,
        Err(pucksdata::error::LoadError::Validation(_))
    ));
    batch.takeaways.pop();
    batch.penalties[0].duration_minutes = Some(5);
    batch.faceoffs.push(pucksdata::models::DbFaceoff {
        event_id_in_game: 9,
        winning_player_id: None,
        losing_player_id: None,
    });
    assert!(matches!(
        pucksdata::loaders::events::upsert_game_events(pool, game, &batch).await,
        Err(pucksdata::error::LoadError::Database(_))
    ));
    let duration:i16 = sqlx::query_scalar("SELECT duration_minutes FROM penalties p JOIN events e ON e.id=p.event_id WHERE e.game_id=$1").bind(game).fetch_one(pool).await.unwrap();
    assert_eq!(
        duration, 2,
        "failure in another child table must roll back every insert"
    );
    for query in [
        "DELETE FROM goals WHERE event_id IN (SELECT id FROM events WHERE game_id=$1)",
        "DELETE FROM shots WHERE event_id IN (SELECT id FROM events WHERE game_id=$1)",
        "DELETE FROM missed_shots WHERE event_id IN (SELECT id FROM events WHERE game_id=$1)",
        "DELETE FROM giveaways WHERE event_id IN (SELECT id FROM events WHERE game_id=$1)",
        "DELETE FROM takeaways WHERE event_id IN (SELECT id FROM events WHERE game_id=$1)",
        "DELETE FROM hits WHERE event_id IN (SELECT id FROM events WHERE game_id=$1)",
        "DELETE FROM blocks WHERE event_id IN (SELECT id FROM events WHERE game_id=$1)",
        "DELETE FROM penalties WHERE event_id IN (SELECT id FROM events WHERE game_id=$1)",
        "DELETE FROM faceoffs WHERE event_id IN (SELECT id FROM events WHERE game_id=$1)",
        "DELETE FROM events WHERE game_id=$1",
        "DELETE FROM games WHERE game_id=$1",
    ] {
        sqlx::query(query).bind(game).execute(pool).await.unwrap();
    }
    sqlx::query("DELETE FROM teams WHERE team_id IN (99491,99492)")
        .execute(pool)
        .await
        .unwrap();
}
