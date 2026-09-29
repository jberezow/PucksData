mod common;

use std::collections::HashMap;

use pucksdata::{fetchers::events, loaders, process::attempts, provenance, replay, AnyError};
use serde_json::{json, Value};
use sqlx::PgPool;

const SEASON: i32 = 20802081;
static DATABASE_TEST: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn unique_game() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as i64
}

fn feed(game: i64, home: i64, away: i64) -> Value {
    json!({"id":game,"gameState":"OFF","homeTeam":{"id":home},"awayTeam":{"id":away},
    "plays":[
        {"eventId":1,"periodDescriptor":{"number":1,"periodType":"REG"},"timeInPeriod":"01:00","situationCode":"1551","typeDescKey":"missed-shot","details":{"shootingPlayerId":101,"goalieInNetId":102,"shotType":"wrist","reason":"wide-right","xCoord":70,"eventOwnerTeamId":home}},
        {"eventId":2,"periodDescriptor":{"number":1,"periodType":"REG"},"timeInPeriod":"02:00","situationCode":"1551","typeDescKey":"giveaway","details":{"playerId":101,"eventOwnerTeamId":home}},
        {"eventId":3,"periodDescriptor":{"number":1,"periodType":"REG"},"timeInPeriod":"03:00","situationCode":"1551","typeDescKey":"takeaway","details":{"playerId":102,"eventOwnerTeamId":away}},
        {"eventId":5,"periodDescriptor":{"number":2,"periodType":"REG"},"timeInPeriod":"02:00","typeDescKey":"goal","details":{"scoringPlayerId":101,"eventOwnerTeamId":home}},
        {"eventId":4,"periodDescriptor":{"number":3,"periodType":"REG"},"timeInPeriod":"20:00","typeDescKey":"game-end"}
    ]})
}

async fn seed(pool: &PgPool, game: i64, source_mismatch: bool) -> (Value, i64, Value) {
    let home = game * 10;
    let away = home + 1;
    for team in [home, away] {
        sqlx::query("INSERT INTO teams(team_id,full_name,common_name,place_name,abbrev) VALUES($1,'Replay','Replay','Replay',$2)")
            .bind(team).bind(format!("R{team}")).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO nhl_team_identities(nhl_team_id,franchise_id,abbrev,full_name) VALUES($1,$1,$2,'Replay')")
            .bind(team).bind(format!("R{team}")).execute(pool).await.unwrap();
    }
    sqlx::query("INSERT INTO games(game_id,season,game_date,home_team_id,away_team_id,game_type,game_state) VALUES($1,$2,'2080-10-01',$3,$4,2,'OFF')")
        .bind(game).bind(SEASON).bind(home).bind(away).execute(pool).await.unwrap();
    let body = feed(game, home, away);
    let pbp = serde_json::from_value(body.clone()).unwrap();
    let mut batch = events::transform_events_with_goal_strengths(
        &pbp,
        &HashMap::from([(home, home), (away, away)]),
        &HashMap::from([(5, events::EventStrength::PowerPlay)]),
    );
    batch.missed_shots.clear();
    batch.giveaways.clear();
    batch.takeaways.clear();
    if source_mismatch {
        batch.events[0].x_coord = Some(30);
    }
    // The second acceptance adds an observation to the same revision. Its source
    // must be selected rather than the snapshot's first-creation attempt.
    for _ in 0..2 {
        attempts::track(pool, "events", &game.to_string(), async {
            provenance::record_response(
                &format!("https://api-web.nhle.com/v1/gamecenter/{game}/play-by-play"),
                &body.to_string(),
            )
            .await?;
            loaders::events::upsert_game_events(pool, game, &batch).await?;
            Ok(())
        })
        .await
        .unwrap();
    }
    let (snapshot, payload): (i64, Value) = sqlx::query_as("SELECT snapshot_id,payload FROM history.snapshots WHERE dataset='events' AND entity_key=$1 ORDER BY revision DESC LIMIT 1")
        .bind(game.to_string()).fetch_one(pool).await.unwrap();
    (body, snapshot, payload)
}

fn options(game: i64, apply: bool) -> replay::Options {
    replay::Options {
        season: SEASON,
        game_id: Some(game),
        after_game_id: None,
        limit: 1,
        apply,
        missing_only: false,
    }
}

async fn counts(pool: &PgPool, game: i64) -> (i64, i64, i64, i64) {
    sqlx::query_as("SELECT
        (SELECT count(*) FROM missed_shots m JOIN events e ON e.id=m.event_id WHERE e.game_id=$1),
        (SELECT count(*) FROM history.snapshots WHERE dataset='events' AND entity_key=$1::text),
        (SELECT count(*) FROM ingestion.event_replays WHERE game_id=$1),
        (SELECT count(*) FROM ingestion.attempts WHERE dataset='event_replay' AND entity_key=$1::text)")
        .bind(game).fetch_one(pool).await.unwrap()
}

#[tokio::test]
async fn replay_uses_accepted_evidence_preserves_parents_and_is_idempotent() {
    if !common::test_database_configured() {
        return;
    }
    let _guard = DATABASE_TEST.lock().await;
    let pool = common::test_pool().await;
    let game = unique_game();
    let (mut body, original_snapshot, original_payload) = seed(pool, game, false).await;
    let accepted_source: i64 = sqlx::query_scalar("SELECT max(o.observation_id) FROM ingestion.source_observations o JOIN ingestion.attempts a USING(attempt_id) WHERE a.entity_key=$1")
        .bind(game.to_string()).fetch_one(pool).await.unwrap();
    let parents: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM events WHERE game_id=$1 ORDER BY id")
            .bind(game)
            .fetch_all(pool)
            .await
            .unwrap();
    body["plays"][0]["details"]["shootingPlayerId"] = json!(999);
    let failure: Result<(), AnyError> = attempts::track(pool, "events", &game.to_string(), async {
        provenance::record_response(
            &format!("https://api-web.nhle.com/v1/gamecenter/{game}/play-by-play"),
            &body.to_string(),
        )
        .await?;
        Err("source failed acceptance".into())
    })
    .await;
    assert!(failure.is_err());
    let before = counts(pool, game).await;
    let preview = replay::run(pool, &options(game, false)).await.unwrap();
    assert_eq!(preview[0].status, "eligible", "{preview:?}");
    assert_eq!(preview[0].source_observation_id, Some(accepted_source));
    assert_eq!(
        counts(pool, game).await,
        before,
        "preview must perform no writes"
    );
    let result = replay::run(pool, &options(game, true)).await.unwrap();
    assert_eq!(result[0].status, "applied", "{result:?}");
    assert_eq!(
        (
            result[0].missed_shots,
            result[0].giveaways,
            result[0].takeaways
        ),
        (1, 1, 1)
    );
    assert_eq!(counts(pool, game).await, (1, 2, 1, 1));
    let after_parents: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM events WHERE game_id=$1 ORDER BY id")
            .bind(game)
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(parents, after_parents);
    let strength: (String, String) = sqlx::query_as(
        "SELECT strength,strength_source FROM events WHERE game_id=$1 AND event_id_in_game=5",
    )
    .bind(game)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(strength, ("pp".into(), "scoring_summary".into()));
    let old_payload: Value =
        sqlx::query_scalar("SELECT payload FROM history.snapshots WHERE snapshot_id=$1")
            .bind(original_snapshot)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(old_payload, original_payload);
    let evidence: (i64,bool) = sqlx::query_as("SELECT r.source_observation_id,r.recorded_at>o.observed_at FROM ingestion.event_replays r JOIN ingestion.source_observations o ON o.observation_id=r.source_observation_id WHERE r.game_id=$1")
        .bind(game).fetch_one(pool).await.unwrap();
    assert_eq!(evidence, (accepted_source, true));
    let repeated = replay::run(pool, &options(game, true)).await.unwrap();
    assert_eq!(repeated[0].status, "unchanged", "{repeated:?}");
    assert_eq!(counts(pool, game).await, (1, 2, 1, 2));
    sqlx::query("UPDATE events SET x_coord=12 WHERE game_id=$1 AND event_id_in_game=1")
        .bind(game)
        .execute(pool)
        .await
        .unwrap();
    let rejected = replay::run(pool, &options(game, true)).await.unwrap();
    assert_eq!(rejected[0].status, "rejected");
    assert!(rejected[0]
        .reason
        .as_deref()
        .unwrap()
        .contains("latest accepted snapshot"));
    assert_eq!(counts(pool, game).await, (1, 2, 1, 3));
}

#[tokio::test]
async fn replay_rejects_source_mismatch_and_rolls_back_failed_evidence_write() {
    if !common::test_database_configured() {
        return;
    }
    let _guard = DATABASE_TEST.lock().await;
    let pool = common::test_pool().await;
    let mismatch_game = unique_game();
    seed(pool, mismatch_game, true).await;
    let result = replay::run(pool, &options(mismatch_game, true))
        .await
        .unwrap();
    assert_eq!(result[0].status, "rejected");
    assert!(result[0].reason.as_deref().unwrap().contains("x_coord"));
    assert_eq!(counts(pool, mismatch_game).await, (0, 1, 0, 1));
    let rollback_game = unique_game();
    seed(pool, rollback_game, false).await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION ingestion.test_replay_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.game_id={rollback_game} THEN RAISE EXCEPTION 'test replay evidence failure'; END IF; RETURN NEW; END $$;
        CREATE TRIGGER test_replay_failure BEFORE INSERT ON ingestion.event_replays FOR EACH ROW EXECUTE FUNCTION ingestion.test_replay_failure();")))
        .execute(pool).await.unwrap();
    let result = replay::run(pool, &options(rollback_game, true))
        .await
        .unwrap();
    sqlx::raw_sql("DROP TRIGGER test_replay_failure ON ingestion.event_replays; DROP FUNCTION ingestion.test_replay_failure();").execute(pool).await.unwrap();
    assert_eq!(result[0].status, "rejected");
    assert!(result[0]
        .reason
        .as_deref()
        .unwrap()
        .contains("test replay evidence failure"));
    assert_eq!(
        counts(pool, rollback_game).await,
        (0, 1, 0, 1),
        "child facts and normalized revision must roll back together"
    );
}

#[tokio::test]
async fn replay_rejects_missing_ambiguous_and_foreign_archives() {
    if !common::test_database_configured() {
        return;
    }
    let _guard = DATABASE_TEST.lock().await;
    let pool = common::test_pool().await;
    for kind in ["missing", "ambiguous", "foreign", "matchup", "incomplete"] {
        let game = unique_game();
        let (body, _, _) = seed(pool, game, false).await;
        let pbp = serde_json::from_value(body.clone()).unwrap();
        let mut batch = events::transform_events_with_goal_strengths(
            &pbp,
            &HashMap::from([(game * 10, game * 10), (game * 10 + 1, game * 10 + 1)]),
            &HashMap::from([(5, events::EventStrength::PowerPlay)]),
        );
        batch.missed_shots.clear();
        batch.giveaways.clear();
        batch.takeaways.clear();
        attempts::track(pool, "events", &game.to_string(), async {
            let url = format!("https://api-web.nhle.com/v1/gamecenter/{game}/play-by-play");
            let mut archive = body.clone();
            match kind {
                "foreign" => archive["id"] = json!(game + 1),
                "matchup" => archive["homeTeam"]["id"] = json!(game * 10 + 1),
                "incomplete" => archive["gameState"] = json!("LIVE"),
                _ => (),
            }
            if kind != "missing" {
                provenance::record_response(&url, &archive.to_string()).await?;
            }
            if kind == "ambiguous" {
                archive["plays"][0]["details"]["shootingPlayerId"] = json!(999);
                provenance::record_response(&url, &archive.to_string()).await?;
            }
            loaders::events::upsert_game_events(pool, game, &batch).await?;
            Ok(())
        })
        .await
        .unwrap();
        let before = counts(pool, game).await;
        let result = replay::run(pool, &options(game, false)).await.unwrap();
        assert_eq!(result[0].status, "rejected", "{kind}: {result:?}");
        assert_eq!(counts(pool, game).await, before);
    }
    let missing_game = unique_game();
    assert!(replay::run(pool, &options(missing_game, false))
        .await
        .is_err());
}

#[test]
fn replay_cli_requires_scope_and_exposes_safe_defaults() {
    let help = std::process::Command::new(env!("CARGO_BIN_EXE_pucksdata"))
        .args(["replay-event-details", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    for flag in [
        "--season",
        "--game-id",
        "--after-game-id",
        "--limit",
        "--apply",
        "--missing-only",
    ] {
        assert!(help.contains(flag), "missing {flag}");
    }
    let invalid = std::process::Command::new(env!("CARGO_BIN_EXE_pucksdata"))
        .args(["replay-event-details", "--limit", "0"])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    for args in [
        vec!["backfill", "--missing-event-details"],
        vec![
            "backfill",
            "--season",
            "20242025",
            "--missing-event-details",
            "--refresh",
        ],
    ] {
        let invalid = std::process::Command::new(env!("CARGO_BIN_EXE_pucksdata"))
            .args(args)
            .output()
            .unwrap();
        assert!(!invalid.status.success());
        assert!(String::from_utf8(invalid.stderr)
            .unwrap()
            .contains("Usage:"));
    }
}

#[tokio::test]
async fn concurrent_replays_keep_game_provenance_and_missing_selection() {
    if !common::test_database_configured() {
        return;
    }
    let _guard = DATABASE_TEST.lock().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(6)
        .connect(&std::env::var("TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    let mut games = Vec::new();
    for _ in 0..4 {
        let game = unique_game();
        seed(&pool, game, false).await;
        games.push(game);
    }
    sqlx::raw_sql("CREATE FUNCTION ingestion.test_replay_delay() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.05); RETURN NEW; END $$;
        CREATE TRIGGER test_replay_delay BEFORE INSERT ON missed_shots FOR EACH ROW EXECUTE FUNCTION ingestion.test_replay_delay();")
        .execute(&pool).await.unwrap();
    let mut scope = replay::Options {
        season: SEASON,
        game_id: None,
        after_game_id: Some(games[0] - 1),
        limit: 4,
        apply: false,
        missing_only: true,
    };
    let preview = replay::run(&pool, &scope).await.unwrap();
    assert_eq!(preview.len(), 4);
    assert!(preview.iter().all(|game| game.status == "eligible"));
    scope.apply = true;
    let results = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        replay::run(&pool, &scope),
    )
    .await
    .expect("bounded workers must not exhaust the connection pool")
    .unwrap();
    sqlx::raw_sql("DROP TRIGGER test_replay_delay ON missed_shots; DROP FUNCTION ingestion.test_replay_delay();")
        .execute(&pool).await.unwrap();
    assert_eq!(
        results.iter().map(|game| game.game_id).collect::<Vec<_>>(),
        games
    );
    assert!(
        results.iter().all(|game| game.status == "applied"),
        "{results:?}"
    );
    let provenance_matches: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ingestion.event_replays r
         JOIN ingestion.attempts a ON a.attempt_id=r.attempt_id
         JOIN history.snapshots s ON s.snapshot_id=r.result_snapshot_id
         JOIN ingestion.source_observations o ON o.observation_id=r.source_observation_id
         JOIN ingestion.attempts original ON original.attempt_id=o.attempt_id
         WHERE r.game_id=ANY($1) AND a.entity_key=r.game_id::text
         AND original.entity_key=r.game_id::text AND s.attempt_id=a.attempt_id
         AND a.outcome='complete'",
    )
    .bind(&games)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(provenance_matches, 4);
    let overlapped: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM ingestion.attempts a JOIN ingestion.attempts b
         ON a.attempt_id<b.attempt_id AND a.started_at<b.finished_at AND b.started_at<a.finished_at
         WHERE a.dataset='event_replay' AND b.dataset='event_replay'
         AND a.entity_key=ANY($1) AND b.entity_key=ANY($1))",
    )
    .bind(games.iter().map(ToString::to_string).collect::<Vec<_>>())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        overlapped,
        "multiple game attempts should execute concurrently"
    );
    assert!(
        replay::run(&pool, &scope).await.unwrap().is_empty(),
        "missing-only skips newly completed games"
    );
    let mut completed_game = options(games[0], false);
    completed_game.missing_only = true;
    assert!(replay::run(&pool, &completed_game)
        .await
        .unwrap()
        .is_empty());
    completed_game.game_id = Some(unique_game());
    assert!(replay::run(&pool, &completed_game).await.is_err());
    pool.close().await;
}
