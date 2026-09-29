//! Atomically inserts all event types for a game in a single transaction.
use std::collections::{HashMap, HashSet};

use sqlx::Row;

use crate::error::LoadError;

use crate::models::{DbMissedShot, DbTurnover, EventBatch, EventCounts};

/// Replace all events for a game atomically in a single PostgreSQL transaction.
///
/// Existing child and base rows are deleted before the authoritative snapshot
/// is inserted. Any failure rolls back both operations, preserving the prior
/// game state. This removes events that disappear in later NHL feed revisions.
///
/// One UNNEST statement inserts base events and returns their database IDs;
/// one parameterized JSON-recordset statement inserts every child type.
pub async fn upsert_game_events(
    pool: &sqlx::PgPool,
    game_id: i64,
    batch: &EventBatch,
) -> Result<EventCounts, LoadError> {
    let events = &batch.events;
    if events.iter().any(|event| event.game_id != game_id) {
        return Err(LoadError::Validation(
            "event snapshot contains a foreign game".into(),
        ));
    }
    let source_ids: HashSet<_> = events.iter().map(|event| event.event_id_in_game).collect();
    let child_ids = batch
        .goals
        .iter()
        .map(|row| row.event_id_in_game)
        .chain(batch.shots.iter().map(|row| row.event_id_in_game))
        .chain(batch.missed_shots.iter().map(|row| row.event_id_in_game))
        .chain(batch.giveaways.iter().map(|row| row.event_id_in_game))
        .chain(batch.takeaways.iter().map(|row| row.event_id_in_game))
        .chain(batch.hits.iter().map(|row| row.event_id_in_game))
        .chain(batch.blocks.iter().map(|row| row.event_id_in_game))
        .chain(batch.penalties.iter().map(|row| row.event_id_in_game))
        .chain(batch.faceoffs.iter().map(|row| row.event_id_in_game));
    if let Some(id) = child_ids.into_iter().find(|id| !source_ids.contains(id)) {
        return Err(LoadError::Validation(format!(
            "child event {id} has no parent in the snapshot"
        )));
    }
    let mut tx = pool.begin().await?;
    super::history::lock_game(&mut tx, game_id).await?;

    if !events.is_empty() {
        sqlx::query(
            r#"
            WITH target_events AS MATERIALIZED (
                SELECT id FROM events WHERE game_id = $1
            ),
            deleted_goals AS (
                DELETE FROM goals WHERE event_id IN (SELECT id FROM target_events)
            ),
            deleted_shots AS (
                DELETE FROM shots WHERE event_id IN (SELECT id FROM target_events)
            ),
            deleted_missed_shots AS (
                DELETE FROM missed_shots WHERE event_id IN (SELECT id FROM target_events)
            ),
            deleted_giveaways AS (
                DELETE FROM giveaways WHERE event_id IN (SELECT id FROM target_events)
            ),
            deleted_takeaways AS (
                DELETE FROM takeaways WHERE event_id IN (SELECT id FROM target_events)
            ),
            deleted_hits AS (
                DELETE FROM hits WHERE event_id IN (SELECT id FROM target_events)
            ),
            deleted_blocks AS (
                DELETE FROM blocks WHERE event_id IN (SELECT id FROM target_events)
            ),
            deleted_penalties AS (
                DELETE FROM penalties WHERE event_id IN (SELECT id FROM target_events)
            ),
            deleted_faceoffs AS (
                DELETE FROM faceoffs WHERE event_id IN (SELECT id FROM target_events)
            )
            DELETE FROM events WHERE id IN (SELECT id FROM target_events)
            "#,
        )
        .bind(game_id)
        .execute(&mut *tx)
        .await?;
    }

    // Map from event_id_in_game -> events.id (surrogate PK) for child FK lookups.
    let mut event_db_id_map: HashMap<i32, i64> = HashMap::with_capacity(events.len());

    // ── Bulk insert base events ───────────────────────────────────────────────
    if !events.is_empty() {
        let game_ids: Vec<i64> = events.iter().map(|e| e.game_id).collect();
        let event_ids: Vec<i32> = events.iter().map(|e| e.event_id_in_game).collect();
        let periods: Vec<i16> = events.iter().map(|e| e.period).collect();
        let period_types: Vec<&str> = events.iter().map(|e| e.period_type.as_str()).collect();
        let times: Vec<&str> = events.iter().map(|e| e.time_in_period.as_str()).collect();
        let event_types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
        let x_coords: Vec<Option<i16>> = events.iter().map(|e| e.x_coord).collect();
        let y_coords: Vec<Option<i16>> = events.iter().map(|e| e.y_coord).collect();
        let zone_codes: Vec<Option<&str>> = events.iter().map(|e| e.zone_code.as_deref()).collect();
        let owner_ids: Vec<Option<i64>> = events.iter().map(|e| e.event_owner_team_id).collect();
        let home_goalies: Vec<Option<bool>> =
            events.iter().map(|e| e.home_goalie_present).collect();
        let home_sks: Vec<Option<i16>> = events.iter().map(|e| e.home_skater_count).collect();
        let away_sks: Vec<Option<i16>> = events.iter().map(|e| e.away_skater_count).collect();
        let away_goalies: Vec<Option<bool>> =
            events.iter().map(|e| e.away_goalie_present).collect();
        let strengths: Vec<Option<&str>> = events.iter().map(|e| e.strength.as_deref()).collect();
        let strength_sources: Vec<&str> =
            events.iter().map(|e| e.strength_source.as_str()).collect();
        let situation_codes: Vec<Option<&str>> =
            events.iter().map(|e| e.situation_code.as_deref()).collect();

        let rows = sqlx::query(
            r#"
            WITH input_events AS (
                SELECT * FROM UNNEST(
                    $1::bigint[], $2::int[], $3::smallint[], $4::text[], $5::text[],
                    $6::text[], $7::smallint[], $8::smallint[], $9::text[], $10::bigint[],
                    $11::bool[], $12::smallint[], $13::smallint[], $14::bool[], $15::text[],
                    $16::text[], $17::text[]
                ) AS t(game_id, event_id_in_game, period, period_type, time_in_period,
                       event_type, x_coord, y_coord, zone_code, event_owner_team_id,
                       home_goalie_present, home_skater_count, away_skater_count,
                       away_goalie_present, strength, strength_source, situation_code)
            )
            INSERT INTO events
                (game_id, event_id_in_game, period, period_type, time_in_period,
                 event_type, x_coord, y_coord, zone_code, event_owner_team_id,
                 home_goalie_present, home_skater_count, away_skater_count,
                 away_goalie_present, strength, strength_source, situation_code,
                 season, game_type, game_date)
            SELECT i.game_id, i.event_id_in_game, i.period, i.period_type, i.time_in_period,
                   i.event_type, i.x_coord, i.y_coord, i.zone_code, i.event_owner_team_id,
                   i.home_goalie_present, i.home_skater_count, i.away_skater_count,
                   i.away_goalie_present, i.strength, i.strength_source, i.situation_code,
                   g.season, g.game_type, g.game_date
            FROM input_events i
            JOIN games g ON g.game_id = i.game_id
            ON CONFLICT (game_id, event_id_in_game) DO UPDATE SET
                period              = EXCLUDED.period,
                period_type         = EXCLUDED.period_type,
                time_in_period      = EXCLUDED.time_in_period,
                event_type          = EXCLUDED.event_type,
                x_coord             = EXCLUDED.x_coord,
                y_coord             = EXCLUDED.y_coord,
                zone_code           = EXCLUDED.zone_code,
                event_owner_team_id = EXCLUDED.event_owner_team_id,
                home_goalie_present = EXCLUDED.home_goalie_present,
                home_skater_count   = EXCLUDED.home_skater_count,
                away_skater_count   = EXCLUDED.away_skater_count,
                away_goalie_present = EXCLUDED.away_goalie_present,
                strength            = EXCLUDED.strength,
                strength_source     = EXCLUDED.strength_source,
                situation_code      = EXCLUDED.situation_code,
                season              = EXCLUDED.season,
                game_type           = EXCLUDED.game_type,
                game_date           = EXCLUDED.game_date
            RETURNING id, event_id_in_game
            "#,
        )
        .bind(&game_ids)
        .bind(&event_ids)
        .bind(&periods)
        .bind(&period_types)
        .bind(&times)
        .bind(&event_types)
        .bind(&x_coords)
        .bind(&y_coords)
        .bind(&zone_codes)
        .bind(&owner_ids)
        .bind(&home_goalies)
        .bind(&home_sks)
        .bind(&away_sks)
        .bind(&away_goalies)
        .bind(&strengths)
        .bind(&strength_sources)
        .bind(&situation_codes)
        .fetch_all(&mut *tx)
        .await?;

        // The join above is also the source of the denormalized scope fields. A
        // missing game would otherwise turn into a silent zero-row insert.
        if rows.len() != events.len() {
            return Err(LoadError::Validation(format!(
                "inserted {} of {} events; every event must reference a loaded game",
                rows.len(),
                events.len()
            )));
        }

        for row in &rows {
            let id: i64 = row.try_get("id")?;
            let eid: i32 = row.try_get("event_id_in_game")?;
            event_db_id_map.insert(eid, id);
        }
    }

    let counts = if events.is_empty() {
        EventCounts::default()
    } else {
        insert_children(&mut tx, batch, &event_db_id_map).await?
    };

    // Single commit — all events for the game or none (atomic guarantee)
    if !events.is_empty() {
        super::history::events(&mut tx, game_id).await?;
    }
    tx.commit().await?;

    Ok(counts)
}

async fn insert_children(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    batch: &EventBatch,
    ids: &HashMap<i32, i64>,
) -> Result<EventCounts, sqlx::Error> {
    let payload = serde_json::json!({
        "parents": ids,
        "goals": &batch.goals,
        "shots": &batch.shots,
        "missed_shots": &batch.missed_shots,
        "giveaways": &batch.giveaways,
        "takeaways": &batch.takeaways,
        "hits": &batch.hits,
        "blocks": &batch.blocks,
        "penalties": &batch.penalties,
        "faceoffs": &batch.faceoffs,
    });
    let row = sqlx::query(
        r#"WITH parents AS MATERIALIZED (
            SELECT key::integer AS source_id, value::bigint AS id
            FROM jsonb_each_text($1->'parents')
        ),
        inserted_goals AS (
            INSERT INTO goals (event_id, scorer_player_id, assist1_player_id, assist2_player_id, goalie_id, shot_type)
            SELECT p.id, c.scorer_player_id, c.assist1_player_id, c.assist2_player_id, c.goalie_id, c.shot_type
            FROM jsonb_to_recordset($1->'goals') AS c(event_id_in_game integer, scorer_player_id bigint, assist1_player_id bigint, assist2_player_id bigint, goalie_id bigint, shot_type text)
            JOIN parents p ON p.source_id = c.event_id_in_game
            RETURNING 1
        ),
        inserted_shots AS (
            INSERT INTO shots (event_id, shooting_player_id, goalie_in_net_id, shot_type)
            SELECT p.id, c.shooting_player_id, c.goalie_in_net_id, c.shot_type
            FROM jsonb_to_recordset($1->'shots') AS c(event_id_in_game integer, shooting_player_id bigint, goalie_in_net_id bigint, shot_type text)
            JOIN parents p ON p.source_id = c.event_id_in_game
            RETURNING 1
        ),
        inserted_missed_shots AS (
            INSERT INTO missed_shots (event_id, shooting_player_id, goalie_in_net_id, shot_type, miss_reason)
            SELECT p.id, c.shooting_player_id, c.goalie_in_net_id, c.shot_type, c.miss_reason
            FROM jsonb_to_recordset($1->'missed_shots') AS c(event_id_in_game integer, shooting_player_id bigint, goalie_in_net_id bigint, shot_type text, miss_reason text)
            JOIN parents p ON p.source_id = c.event_id_in_game
            RETURNING 1
        ),
        inserted_giveaways AS (
            INSERT INTO giveaways (event_id, player_id)
            SELECT p.id, c.player_id
            FROM jsonb_to_recordset($1->'giveaways') AS c(event_id_in_game integer, player_id bigint)
            JOIN parents p ON p.source_id = c.event_id_in_game
            RETURNING 1
        ),
        inserted_takeaways AS (
            INSERT INTO takeaways (event_id, player_id)
            SELECT p.id, c.player_id
            FROM jsonb_to_recordset($1->'takeaways') AS c(event_id_in_game integer, player_id bigint)
            JOIN parents p ON p.source_id = c.event_id_in_game
            RETURNING 1
        ),
        inserted_hits AS (
            INSERT INTO hits (event_id, hitting_player_id, hittee_player_id)
            SELECT p.id, c.hitting_player_id, c.hittee_player_id
            FROM jsonb_to_recordset($1->'hits') AS c(event_id_in_game integer, hitting_player_id bigint, hittee_player_id bigint)
            JOIN parents p ON p.source_id = c.event_id_in_game
            RETURNING 1
        ),
        inserted_blocks AS (
            INSERT INTO blocks (event_id, blocking_player_id, shooting_player_id)
            SELECT p.id, c.blocking_player_id, c.shooting_player_id
            FROM jsonb_to_recordset($1->'blocks') AS c(event_id_in_game integer, blocking_player_id bigint, shooting_player_id bigint)
            JOIN parents p ON p.source_id = c.event_id_in_game
            RETURNING 1
        ),
        inserted_penalties AS (
            INSERT INTO penalties (event_id, committed_by_player_id, drawn_by_player_id, infraction_type, duration_minutes)
            SELECT p.id, c.committed_by_player_id, c.drawn_by_player_id, c.infraction_type, c.duration_minutes
            FROM jsonb_to_recordset($1->'penalties') AS c(event_id_in_game integer, committed_by_player_id bigint, drawn_by_player_id bigint, infraction_type text, duration_minutes smallint)
            JOIN parents p ON p.source_id = c.event_id_in_game
            RETURNING 1
        ),
        inserted_faceoffs AS (
            INSERT INTO faceoffs (event_id, winning_player_id, losing_player_id)
            SELECT p.id, c.winning_player_id, c.losing_player_id
            FROM jsonb_to_recordset($1->'faceoffs') AS c(event_id_in_game integer, winning_player_id bigint, losing_player_id bigint)
            JOIN parents p ON p.source_id = c.event_id_in_game
            RETURNING 1
        )
        SELECT (SELECT count(*) FROM inserted_goals) AS goals,
               (SELECT count(*) FROM inserted_shots) AS shots,
               (SELECT count(*) FROM inserted_missed_shots) AS missed_shots,
               (SELECT count(*) FROM inserted_giveaways) AS giveaways,
               (SELECT count(*) FROM inserted_takeaways) AS takeaways,
               (SELECT count(*) FROM inserted_hits) AS hits,
               (SELECT count(*) FROM inserted_blocks) AS blocks,
               (SELECT count(*) FROM inserted_penalties) AS penalties,
               (SELECT count(*) FROM inserted_faceoffs) AS faceoffs"#,
    ).bind(payload).fetch_one(&mut **tx).await?;
    Ok(EventCounts {
        events: batch.events.len(),
        goals: row.try_get::<i64, _>("goals")? as usize,
        shots: row.try_get::<i64, _>("shots")? as usize,
        missed_shots: row.try_get::<i64, _>("missed_shots")? as usize,
        giveaways: row.try_get::<i64, _>("giveaways")? as usize,
        takeaways: row.try_get::<i64, _>("takeaways")? as usize,
        hits: row.try_get::<i64, _>("hits")? as usize,
        blocks: row.try_get::<i64, _>("blocks")? as usize,
        penalties: row.try_get::<i64, _>("penalties")? as usize,
        faceoffs: row.try_get::<i64, _>("faceoffs")? as usize,
    })
}

pub(crate) async fn insert_missed_shots(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    rows: &[DbMissedShot],
    ids: &HashMap<i32, i64>,
) -> Result<usize, sqlx::Error> {
    let matched: Vec<_> = rows
        .iter()
        .filter_map(|row| ids.get(&row.event_id_in_game).map(|id| (*id, row)))
        .collect();
    if matched.is_empty() {
        return Ok(0);
    }
    let event_ids: Vec<_> = matched.iter().map(|(id, _)| *id).collect();
    let shooters: Vec<_> = matched
        .iter()
        .map(|(_, row)| row.shooting_player_id)
        .collect();
    let goalies: Vec<_> = matched
        .iter()
        .map(|(_, row)| row.goalie_in_net_id)
        .collect();
    let types: Vec<_> = matched
        .iter()
        .map(|(_, row)| row.shot_type.as_deref())
        .collect();
    let reasons: Vec<_> = matched
        .iter()
        .map(|(_, row)| row.miss_reason.as_deref())
        .collect();
    let result = sqlx::query("INSERT INTO missed_shots (event_id, shooting_player_id, goalie_in_net_id, shot_type, miss_reason)
        SELECT * FROM UNNEST($1::bigint[], $2::bigint[], $3::bigint[], $4::text[], $5::text[])")
        .bind(event_ids).bind(shooters).bind(goalies).bind(types).bind(reasons).execute(&mut **tx).await?;
    Ok(result.rows_affected() as usize)
}

pub(crate) async fn insert_turnovers(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    rows: &[DbTurnover],
    ids: &HashMap<i32, i64>,
    takeaway: bool,
) -> Result<usize, sqlx::Error> {
    let matched: Vec<_> = rows
        .iter()
        .filter_map(|row| ids.get(&row.event_id_in_game).map(|id| (*id, row)))
        .collect();
    if matched.is_empty() {
        return Ok(0);
    }
    let event_ids: Vec<_> = matched.iter().map(|(id, _)| *id).collect();
    let players: Vec<_> = matched.iter().map(|(_, row)| row.player_id).collect();
    let query = if takeaway {
        "INSERT INTO takeaways (event_id, player_id) SELECT * FROM UNNEST($1::bigint[], $2::bigint[])"
    } else {
        "INSERT INTO giveaways (event_id, player_id) SELECT * FROM UNNEST($1::bigint[], $2::bigint[])"
    };
    let result = sqlx::query(query)
        .bind(event_ids)
        .bind(players)
        .execute(&mut **tx)
        .await?;
    Ok(result.rows_affected() as usize)
}
