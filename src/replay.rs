//! Offline enrichment from archived evidence that still matches accepted events.
use std::collections::HashMap;

use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Transaction};

use crate::{fetchers::events, loaders, models::EventBatch, process::attempts, AnyError};

#[derive(Debug, Clone)]
pub struct Options {
    pub season: i32,
    pub game_id: Option<i64>,
    pub after_game_id: Option<i64>,
    pub limit: i64,
    pub apply: bool,
}

#[derive(Debug, Serialize)]
pub struct GameResult {
    pub game_id: i64,
    pub status: &'static str,
    pub missed_shots: usize,
    pub giveaways: usize,
    pub takeaways: usize,
    pub source_observation_id: Option<i64>,
    pub reason: Option<String>,
}

/// Dry runs open read-only transactions and never create ingestion attempts.
pub async fn run(pool: &PgPool, options: &Options) -> Result<Vec<GameResult>, AnyError> {
    if options.season / 10000 + 1 != options.season % 10000
        || !(19001901..=29992999).contains(&options.season)
        || !(1..=10000).contains(&options.limit)
    {
        return Err("use an eight-digit consecutive season and a limit between 1 and 10000".into());
    }
    if options.game_id.is_some() && options.after_game_id.is_some() {
        return Err("game-id and after-game-id cannot be combined".into());
    }
    if options.apply {
        attempts::exclusive(pool, run_inner(pool, options)).await
    } else {
        run_inner(pool, options).await
    }
}

async fn run_inner(pool: &PgPool, options: &Options) -> Result<Vec<GameResult>, AnyError> {
    let games: Vec<i64> = sqlx::query_scalar(
        "SELECT game_id FROM games WHERE season=$1 AND ($2::bigint IS NULL OR game_id=$2)
         AND ($4::bigint IS NULL OR game_id>$4)
         AND EXISTS(SELECT 1 FROM events WHERE events.game_id=games.game_id)
         ORDER BY game_id LIMIT $3",
    )
    .bind(options.season)
    .bind(options.game_id)
    .bind(options.limit)
    .bind(options.after_game_id)
    .fetch_all(pool)
    .await?;
    if options.game_id.is_some() && games.is_empty() {
        return Err("requested game has no current events in the selected season".into());
    }
    let mut results = Vec::new();
    for game_id in games {
        let operation = enrich(pool, game_id, options.apply);
        let result = if options.apply {
            attempts::track(pool, "event_replay", &game_id.to_string(), operation).await
        } else {
            operation.await
        };
        results.push(match result {
            Ok(result) => result,
            Err(error) => GameResult {
                game_id,
                status: "rejected",
                missed_shots: 0,
                giveaways: 0,
                takeaways: 0,
                source_observation_id: None,
                reason: Some(error.to_string()),
            },
        });
    }
    Ok(results)
}

async fn enrich(pool: &PgPool, game_id: i64, apply: bool) -> Result<GameResult, AnyError> {
    let mut tx = pool.begin().await?;
    if apply {
        loaders::history::lock_game(&mut tx, game_id).await?;
    } else {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
    }
    let (snapshot_id, accepted): (i64, Value) = sqlx::query_as(
        "SELECT snapshot_id,payload FROM history.snapshots WHERE dataset='events' AND entity_key=$1
         ORDER BY revision DESC LIMIT 1",
    )
    .bind(game_id.to_string())
    .fetch_optional(&mut *tx)
    .await?
    .ok_or("no accepted event history; refresh this game from its source first")?;
    let current = loaders::history::event_payload(&mut tx, game_id).await?;
    if canonical_payload(&accepted)? != canonical_payload(&current)? {
        return Err("current events differ from the latest accepted snapshot".into());
    }
    let (source_snapshot, source_observation, body) = source(&mut tx, game_id, snapshot_id).await?;
    let pbp = events::parse_play_by_play(game_id, &body)?;
    let mapping: HashMap<i64, i64> = sqlx::query_as::<_, (i64, i64)>(
        "SELECT nhl_team_id,franchise_id FROM nhl_team_identities WHERE franchise_id IS NOT NULL",
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    let (home, away): (i64, i64) =
        sqlx::query_as("SELECT home_team_id,away_team_id FROM games WHERE game_id=$1")
            .bind(game_id)
            .fetch_one(&mut *tx)
            .await?;
    if mapping.get(&pbp.home_team.id) != Some(&home)
        || mapping.get(&pbp.away_team.id) != Some(&away)
    {
        return Err("archived matchup disagrees with current game identities".into());
    }
    let mut batch = events::transform_events(&pbp, &mapping);
    if !batch.warnings.is_empty() {
        return Err(format!(
            "archived normalization rejected: {}",
            batch.warnings.join("; ")
        )
        .into());
    }
    validate_source(&current, &batch)?;
    retain_missing(&current, &mut batch)?;
    let result = GameResult {
        game_id,
        status: if batch.missed_shots.is_empty()
            && batch.giveaways.is_empty()
            && batch.takeaways.is_empty()
        {
            "unchanged"
        } else if apply {
            "applied"
        } else {
            "eligible"
        },
        missed_shots: batch.missed_shots.len(),
        giveaways: batch.giveaways.len(),
        takeaways: batch.takeaways.len(),
        source_observation_id: Some(source_observation),
        reason: None,
    };
    if apply && result.status == "applied" {
        let ids: HashMap<i32, i64> =
            sqlx::query_as("SELECT event_id_in_game,id FROM events WHERE game_id=$1")
                .bind(game_id)
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .collect();
        loaders::events::insert_missed_shots(&mut tx, &batch.missed_shots, &ids).await?;
        loaders::events::insert_turnovers(&mut tx, &batch.giveaways, &ids, false).await?;
        loaders::events::insert_turnovers(&mut tx, &batch.takeaways, &ids, true).await?;
        loaders::history::events(&mut tx, game_id).await?;
        sqlx::query(
            "INSERT INTO ingestion.event_replays(game_id,attempt_id,source_snapshot_id,
             source_observation_id,result_snapshot_id)
             SELECT $1,current_setting('pucksdata.attempt_id')::bigint,$2,$3,snapshot_id
             FROM history.snapshots WHERE dataset='events' AND entity_key=$1::text
             ORDER BY revision DESC LIMIT 1",
        )
        .bind(game_id)
        .bind(source_snapshot)
        .bind(source_observation)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    } else {
        tx.rollback().await?;
    }
    Ok(result)
}

async fn source(
    tx: &mut Transaction<'_, Postgres>,
    game_id: i64,
    snapshot_id: i64,
) -> Result<(i64, i64, String), AnyError> {
    let previous: Option<(i64, i64, String)> = sqlx::query_as(
        "SELECT r.source_snapshot_id,o.observation_id,d.body FROM ingestion.event_replays r
         JOIN ingestion.source_observations o ON o.observation_id=r.source_observation_id
         JOIN history.source_documents d USING(content_sha256)
         WHERE r.game_id=$1 AND r.result_snapshot_id=$2 ORDER BY r.replay_id DESC LIMIT 1",
    )
    .bind(game_id)
    .bind(snapshot_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(source) = previous {
        return Ok(source);
    }
    let candidates: Vec<(i64, String, String)> = sqlx::query_as(
        "WITH accepted AS (
             SELECT o.attempt_id,o.recorded_at FROM history.observations o
             JOIN ingestion.attempts a USING(attempt_id)
             WHERE o.snapshot_id=$1 AND a.dataset='events' AND a.entity_key=$2
             ORDER BY o.observation_id DESC LIMIT 1
         ) SELECT s.observation_id,s.content_sha256,d.body FROM accepted a
         JOIN ingestion.source_observations s ON s.attempt_id=a.attempt_id
         JOIN history.source_documents d USING(content_sha256)
         WHERE s.url=$3 AND s.observed_at<=a.recorded_at ORDER BY s.observation_id DESC",
    )
    .bind(snapshot_id)
    .bind(game_id.to_string())
    .bind(format!(
        "https://api-web.nhle.com/v1/gamecenter/{game_id}/play-by-play"
    ))
    .fetch_all(&mut **tx)
    .await?;
    let first = candidates
        .first()
        .ok_or("no archived play-by-play linked to the latest accepted observation")?;
    if candidates.iter().any(|candidate| candidate.1 != first.1) {
        return Err("ambiguous archived responses within the accepted attempt".into());
    }
    Ok((snapshot_id, first.0, first.2.clone()))
}

fn rows(value: &Value) -> Result<&Vec<Value>, AnyError> {
    value
        .as_array()
        .ok_or_else(|| "event history is not an array".into())
}

fn canonical_payload(value: &Value) -> Result<Value, AnyError> {
    let mut result = rows(value)?.clone();
    for row in &mut result {
        let object = row.as_object_mut().ok_or("invalid event history row")?;
        for key in ["missed_shot", "giveaway", "takeaway"] {
            object.entry(key).or_insert(Value::Null);
        }
    }
    Ok(Value::Array(result))
}

fn validate_source(current: &Value, batch: &EventBatch) -> Result<(), AnyError> {
    let current = rows(current)?;
    if current.len() != batch.events.len() {
        return Err("archived event inventory differs from current events".into());
    }
    for event in &batch.events {
        let row = current
            .iter()
            .find(|row| row["event_id_in_game"] == event.event_id_in_game)
            .ok_or("archived event IDs differ from current events")?;
        let fields = json!({
            "game_id":event.game_id,"event_id_in_game":event.event_id_in_game,
            "period":event.period,"period_type":event.period_type,"time_in_period":event.time_in_period,
            "event_type":event.event_type,"x_coord":event.x_coord,"y_coord":event.y_coord,
            "zone_code":event.zone_code,"event_owner_team_id":event.event_owner_team_id,
            "away_goalie_present":event.away_goalie_present,"away_skater_count":event.away_skater_count,
            "home_goalie_present":event.home_goalie_present,"home_skater_count":event.home_skater_count,
            "situation_code":event.situation_code
        });
        for (key, value) in fields.as_object().unwrap() {
            if &row[key] != value {
                return Err(
                    format!("archived event {} differs in {key}", event.event_id_in_game).into(),
                );
            }
        }
        if event.situation_code.is_some()
            && (row["strength"] != json!(event.strength)
                || row["strength_source"] != json!(event.strength_source.as_str()))
        {
            return Err("archived situation strength differs from current events".into());
        }
        let id = event.event_id_in_game;
        let children = [
            (
                "goal",
                batch
                    .goals
                    .iter()
                    .find(|x| x.event_id_in_game == id)
                    .map(|x| {
                        json!({
                            "scorer_player_id":x.scorer_player_id,
                            "assist1_player_id":x.assist1_player_id,
                            "assist2_player_id":x.assist2_player_id,
                            "goalie_id":x.goalie_id,
                            "shot_type":x.shot_type,
                        })
                    }),
            ),
            (
                "shot",
                batch
                    .shots
                    .iter()
                    .find(|x| x.event_id_in_game == id)
                    .map(|x| {
                        json!({
                            "shooting_player_id":x.shooting_player_id,
                            "goalie_in_net_id":x.goalie_in_net_id,
                            "shot_type":x.shot_type,
                        })
                    }),
            ),
            (
                "hit",
                batch
                    .hits
                    .iter()
                    .find(|x| x.event_id_in_game == id)
                    .map(|x| {
                        json!({
                            "hitting_player_id":x.hitting_player_id,
                            "hittee_player_id":x.hittee_player_id,
                        })
                    }),
            ),
            (
                "block",
                batch
                    .blocks
                    .iter()
                    .find(|x| x.event_id_in_game == id)
                    .map(|x| {
                        json!({
                            "blocking_player_id":x.blocking_player_id,
                            "shooting_player_id":x.shooting_player_id,
                        })
                    }),
            ),
            (
                "penalty",
                batch
                    .penalties
                    .iter()
                    .find(|x| x.event_id_in_game == id)
                    .map(|x| {
                        json!({
                            "committed_by_player_id":x.committed_by_player_id,
                            "drawn_by_player_id":x.drawn_by_player_id,
                            "infraction_type":x.infraction_type,
                            "duration_minutes":x.duration_minutes,
                        })
                    }),
            ),
            (
                "faceoff",
                batch
                    .faceoffs
                    .iter()
                    .find(|x| x.event_id_in_game == id)
                    .map(|x| {
                        json!({
                            "winning_player_id":x.winning_player_id,
                            "losing_player_id":x.losing_player_id,
                        })
                    }),
            ),
        ];
        for (key, child) in children {
            if row[key] != child.unwrap_or(Value::Null) {
                return Err(format!("archived event {id} differs in {key} attribution").into());
            }
        }
    }
    Ok(())
}

fn retain_missing(current: &Value, batch: &mut EventBatch) -> Result<(), AnyError> {
    let current = rows(current)?;
    let check = |id: i32, key: &str, expected: Value| -> Result<bool, AnyError> {
        let row = current
            .iter()
            .find(|row| row["event_id_in_game"] == id)
            .ok_or("new child has no current parent")?;
        if row[key].is_null() {
            Ok(true)
        } else if row[key] == expected {
            Ok(false)
        } else {
            Err(format!("existing {key} attribution conflicts with archive for event {id}").into())
        }
    };
    let mut missed = Vec::new();
    for row in batch.missed_shots.drain(..) {
        if check(
            row.event_id_in_game,
            "missed_shot",
            json!({
                "shooting_player_id":row.shooting_player_id,
                "goalie_in_net_id":row.goalie_in_net_id,
                "shot_type":row.shot_type,
                "miss_reason":row.miss_reason,
            }),
        )? {
            missed.push(row);
        }
    }
    batch.missed_shots = missed;
    for (key, children) in [
        ("giveaway", &mut batch.giveaways),
        ("takeaway", &mut batch.takeaways),
    ] {
        let mut missing = Vec::new();
        for row in children.drain(..) {
            if check(
                row.event_id_in_game,
                key,
                json!({
                    "player_id":row.player_id,
                }),
            )? {
                missing.push(row);
            }
        }
        *children = missing;
    }
    Ok(())
}
