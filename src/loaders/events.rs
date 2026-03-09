// src/loaders/events.rs
// Transactional loader for game events and all six child event types.

use std::collections::HashMap;

use crate::models::{DbBlock, DbEvent, DbFaceoff, DbGoal, DbHit, DbPenalty, DbShot};

/// Insert all events for a game atomically in a single PostgreSQL transaction.
///
/// Returns (events_inserted, goals_inserted, shots_inserted, hits_inserted,
///          blocks_inserted, penalties_inserted, faceoffs_inserted).
///
/// On any error the transaction rolls back automatically (sqlx Transaction
/// drops with a rollback on Err). This enforces the locked constraint that
/// partial game loads never exist in the database.
///
/// Base events are inserted with ON CONFLICT (game_id, event_id_in_game) DO UPDATE
/// to ensure idempotency (QUAL-01). The RETURNING id clause captures the surrogate
/// PK for use as the FK in child tables.
pub async fn upsert_game_events(
    pool: &sqlx::PgPool,
    events: &[DbEvent],
    goals: &[DbGoal],
    shots: &[DbShot],
    hits: &[DbHit],
    blocks: &[DbBlock],
    penalties: &[DbPenalty],
    faceoffs: &[DbFaceoff],
) -> Result<(usize, usize, usize, usize, usize, usize, usize), sqlx::Error> {
    let mut tx = pool.begin().await?;

    // Map from event_id_in_game -> events.id (surrogate PK) for child FK lookups
    let mut event_db_id_map: HashMap<i32, i64> = HashMap::with_capacity(events.len());

    // ── Insert base events ────────────────────────────────────────────────────
    for e in events {
        let row = sqlx::query!(
            r#"
            INSERT INTO events
                (game_id, event_id_in_game, period, period_type, time_in_period,
                 event_type, x_coord, y_coord, zone_code, event_owner_team_id,
                 home_goalie_present, home_skater_count, away_skater_count,
                 away_goalie_present, strength)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
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
                strength            = EXCLUDED.strength
            RETURNING id
            "#,
            e.game_id,
            e.event_id_in_game,
            e.period,
            e.period_type.as_str(),
            e.time_in_period.as_str(),
            e.event_type.as_str(),
            e.x_coord,
            e.y_coord,
            e.zone_code.as_deref(),
            e.event_owner_team_id,
            e.home_goalie_present,
            e.home_skater_count,
            e.away_skater_count,
            e.away_goalie_present,
            e.strength.as_str(),
        )
        .fetch_one(&mut *tx)
        .await?;

        event_db_id_map.insert(e.event_id_in_game, row.id);
    }

    let events_inserted = events.len();

    // ── Insert child event records ────────────────────────────────────────────
    // Plan 02 implements goals and shots; Plan 03 implements hits, blocks,
    // penalties, and faceoffs. Stub functions below return Ok(()) immediately
    // as placeholders — they will be filled in by subsequent plans.

    let mut goals_inserted = 0usize;
    for goal in goals {
        if let Some(&event_db_id) = event_db_id_map.get(&goal.event_id_in_game) {
            upsert_goal(&mut tx, event_db_id, goal).await?;
            goals_inserted += 1;
        }
    }

    let mut shots_inserted = 0usize;
    for shot in shots {
        if let Some(&event_db_id) = event_db_id_map.get(&shot.event_id_in_game) {
            upsert_shot(&mut tx, event_db_id, shot).await?;
            shots_inserted += 1;
        }
    }

    let mut hits_inserted = 0usize;
    for hit in hits {
        if let Some(&event_db_id) = event_db_id_map.get(&hit.event_id_in_game) {
            upsert_hit(&mut tx, event_db_id, hit).await?;
            hits_inserted += 1;
        }
    }

    let mut blocks_inserted = 0usize;
    for block in blocks {
        if let Some(&event_db_id) = event_db_id_map.get(&block.event_id_in_game) {
            upsert_block(&mut tx, event_db_id, block).await?;
            blocks_inserted += 1;
        }
    }

    let mut penalties_inserted = 0usize;
    for penalty in penalties {
        if let Some(&event_db_id) = event_db_id_map.get(&penalty.event_id_in_game) {
            upsert_penalty(&mut tx, event_db_id, penalty).await?;
            penalties_inserted += 1;
        }
    }

    let mut faceoffs_inserted = 0usize;
    for faceoff in faceoffs {
        if let Some(&event_db_id) = event_db_id_map.get(&faceoff.event_id_in_game) {
            upsert_faceoff(&mut tx, event_db_id, faceoff).await?;
            faceoffs_inserted += 1;
        }
    }

    // Single commit — all events for the game or none (atomic guarantee)
    tx.commit().await?;

    Ok((
        events_inserted,
        goals_inserted,
        shots_inserted,
        hits_inserted,
        blocks_inserted,
        penalties_inserted,
        faceoffs_inserted,
    ))
}

// ── Child upsert stubs (Plan 02 fills in goals and shots; Plan 03 fills in the rest) ──

/// Stub: Insert a goal child record. Plan 02 implements the full SQL.
pub async fn upsert_goal(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event_db_id: i64,
    goal: &DbGoal,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO goals (event_id, scorer_player_id, assist1_player_id, assist2_player_id,
                           goalie_id, shot_type)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (event_id) DO UPDATE SET
            scorer_player_id  = EXCLUDED.scorer_player_id,
            assist1_player_id = EXCLUDED.assist1_player_id,
            assist2_player_id = EXCLUDED.assist2_player_id,
            goalie_id         = EXCLUDED.goalie_id,
            shot_type         = EXCLUDED.shot_type
        "#,
        event_db_id,
        goal.scorer_player_id,
        goal.assist1_player_id,
        goal.assist2_player_id,
        goal.goalie_id,
        goal.shot_type.as_deref(),
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Stub: Insert a shot child record. Plan 02 implements the full SQL.
pub async fn upsert_shot(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event_db_id: i64,
    shot: &DbShot,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO shots (event_id, shooting_player_id, goalie_in_net_id, shot_type)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (event_id) DO UPDATE SET
            shooting_player_id = EXCLUDED.shooting_player_id,
            goalie_in_net_id   = EXCLUDED.goalie_in_net_id,
            shot_type          = EXCLUDED.shot_type
        "#,
        event_db_id,
        shot.shooting_player_id,
        shot.goalie_in_net_id,
        shot.shot_type.as_deref(),
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Stub: Insert a hit child record. Plan 03 implements the full SQL.
pub async fn upsert_hit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event_db_id: i64,
    hit: &DbHit,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO hits (event_id, hitting_player_id, hittee_player_id)
        VALUES ($1, $2, $3)
        ON CONFLICT (event_id) DO UPDATE SET
            hitting_player_id = EXCLUDED.hitting_player_id,
            hittee_player_id  = EXCLUDED.hittee_player_id
        "#,
        event_db_id,
        hit.hitting_player_id,
        hit.hittee_player_id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Stub: Insert a blocked-shot child record. Plan 03 implements the full SQL.
pub async fn upsert_block(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event_db_id: i64,
    block: &DbBlock,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO blocks (event_id, blocking_player_id, shooting_player_id)
        VALUES ($1, $2, $3)
        ON CONFLICT (event_id) DO UPDATE SET
            blocking_player_id = EXCLUDED.blocking_player_id,
            shooting_player_id = EXCLUDED.shooting_player_id
        "#,
        event_db_id,
        block.blocking_player_id,
        block.shooting_player_id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Stub: Insert a penalty child record. Plan 03 implements the full SQL.
pub async fn upsert_penalty(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event_db_id: i64,
    penalty: &DbPenalty,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO penalties (event_id, committed_by_player_id, drawn_by_player_id,
                               infraction_type, duration_minutes)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (event_id) DO UPDATE SET
            committed_by_player_id = EXCLUDED.committed_by_player_id,
            drawn_by_player_id     = EXCLUDED.drawn_by_player_id,
            infraction_type        = EXCLUDED.infraction_type,
            duration_minutes       = EXCLUDED.duration_minutes
        "#,
        event_db_id,
        penalty.committed_by_player_id,
        penalty.drawn_by_player_id,
        penalty.infraction_type.as_deref(),
        penalty.duration_minutes,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Stub: Insert a faceoff child record. Plan 03 implements the full SQL.
pub async fn upsert_faceoff(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event_db_id: i64,
    faceoff: &DbFaceoff,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO faceoffs (event_id, winning_player_id, losing_player_id)
        VALUES ($1, $2, $3)
        ON CONFLICT (event_id) DO UPDATE SET
            winning_player_id = EXCLUDED.winning_player_id,
            losing_player_id  = EXCLUDED.losing_player_id
        "#,
        event_db_id,
        faceoff.winning_player_id,
        faceoff.losing_player_id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
