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
    pub missing_only: bool,
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
         AND EXISTS(SELECT 1 FROM events e WHERE e.game_id=games.game_id
             AND (NOT $5 OR
                  (e.event_type='missed-shot' AND NOT EXISTS(SELECT 1 FROM missed_shots m WHERE m.event_id=e.id)) OR
                  (e.event_type='giveaway' AND NOT EXISTS(SELECT 1 FROM giveaways g WHERE g.event_id=e.id)) OR
                  (e.event_type='takeaway' AND NOT EXISTS(SELECT 1 FROM takeaways t WHERE t.event_id=e.id))))
         ORDER BY game_id LIMIT $3",
    )
    .bind(options.season)
    .bind(options.game_id)
    .bind(options.limit)
    .bind(options.after_game_id)
    .bind(options.missing_only)
    .fetch_all(pool)
    .await?;
    if options.game_id.is_some() && games.is_empty() {
        let exists = options.missing_only
            && sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM games g WHERE g.game_id=$1 AND g.season=$2
             AND EXISTS(SELECT 1 FROM events e WHERE e.game_id=g.game_id))",
            )
            .bind(options.game_id)
            .bind(options.season)
            .fetch_one(pool)
            .await?;
        if !exists {
            return Err("requested game has no current events in the selected season".into());
        }
    }
    let concurrency = worker_limit(pool.options().get_max_connections(), options.apply);
    let total = games.len();
    tracing::info!(
        total,
        concurrency,
        apply = options.apply,
        "archived event replay started"
    );
    let mut pending = games.into_iter();
    let mut workers = tokio::task::JoinSet::new();
    let mut results = Vec::with_capacity(total);
    loop {
        while workers.len() < concurrency {
            let Some(game_id) = pending.next() else { break };
            let pool = pool.clone();
            let apply = options.apply;
            workers.spawn(async move {
                let operation = enrich(&pool, game_id, apply);
                let result = if apply {
                    attempts::track(&pool, "event_replay", &game_id.to_string(), operation).await
                } else {
                    operation.await
                };
                match result {
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
                }
            });
        }
        let Some(result) = workers.join_next().await else {
            break;
        };
        let result = result?;
        tracing::info!(
            game_id = result.game_id,
            status = result.status,
            completed = results.len() + 1,
            total,
            missed_shots = result.missed_shots,
            giveaways = result.giveaways,
            takeaways = result.takeaways,
            "archived event replay progress"
        );
        results.push(result);
    }
    results.sort_by_key(|result| result.game_id);
    Ok(results)
}

fn worker_limit(max_connections: u32, apply: bool) -> usize {
    max_connections
        .saturating_sub(if apply { 2 } else { 1 })
        .clamp(1, 4) as usize
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
    let (home, away, source_ids, franchise_ids): (i64, i64, Vec<i64>, Vec<i64>) =
        sqlx::query_as(
            "SELECT g.home_team_id,g.away_team_id,i.source_ids,i.franchise_ids
             FROM games g CROSS JOIN LATERAL (
                 SELECT COALESCE(array_agg(nhl_team_id ORDER BY nhl_team_id),'{}'::bigint[]) AS source_ids,
                        COALESCE(array_agg(franchise_id ORDER BY nhl_team_id),'{}'::bigint[]) AS franchise_ids
                 FROM nhl_team_identities WHERE franchise_id IS NOT NULL
             ) i WHERE g.game_id=$1",
        )
        .bind(game_id)
        .fetch_one(&mut *tx)
        .await?;
    let mapping: HashMap<i64, i64> = source_ids.into_iter().zip(franchise_ids).collect();
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
    let candidates: Vec<(i64, i64, String, String)> = sqlx::query_as(
        "WITH previous AS MATERIALIZED (
             SELECT r.source_snapshot_id,o.observation_id,o.content_sha256,d.body
             FROM ingestion.event_replays r
             JOIN ingestion.source_observations o ON o.observation_id=r.source_observation_id
             JOIN history.source_documents d USING(content_sha256)
             WHERE r.game_id=$1 AND r.result_snapshot_id=$2 ORDER BY r.replay_id DESC LIMIT 1
         ), accepted AS (
             SELECT o.attempt_id,o.recorded_at FROM history.observations o
             JOIN ingestion.attempts a USING(attempt_id)
             WHERE o.snapshot_id=$2 AND a.dataset='events' AND a.entity_key=$1::text
             AND NOT EXISTS(SELECT 1 FROM previous)
             ORDER BY o.observation_id DESC LIMIT 1
         ) SELECT source_snapshot_id,observation_id,content_sha256,body FROM previous
         UNION ALL
         SELECT $2,s.observation_id,s.content_sha256,d.body FROM accepted a
         JOIN ingestion.source_observations s ON s.attempt_id=a.attempt_id
         JOIN history.source_documents d USING(content_sha256)
         WHERE s.url=$3 AND s.observed_at<=a.recorded_at ORDER BY observation_id DESC",
    )
    .bind(game_id)
    .bind(snapshot_id)
    .bind(format!(
        "https://api-web.nhle.com/v1/gamecenter/{game_id}/play-by-play"
    ))
    .fetch_all(&mut **tx)
    .await?;
    let first = candidates
        .first()
        .ok_or("no archived play-by-play linked to the latest accepted observation")?;
    if candidates.iter().any(|candidate| candidate.2 != first.2) {
        return Err("ambiguous archived responses within the accepted attempt".into());
    }
    Ok((first.0, first.1, first.3.clone()))
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

#[cfg(test)]
mod tests {
    use super::worker_limit;

    #[test]
    fn workers_reserve_lease_and_spare_connections() {
        assert_eq!(worker_limit(2, true), 1);
        assert_eq!(worker_limit(3, true), 1);
        assert_eq!(worker_limit(5, true), 3);
        assert_eq!(worker_limit(6, true), 4);
        assert_eq!(worker_limit(100, true), 4);
        assert_eq!(worker_limit(1, false), 1);
        assert_eq!(worker_limit(5, false), 4);
    }
}
