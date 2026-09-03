//! Atomically inserts all event types for a game in a single transaction.
use std::collections::HashMap;

use sqlx::Row;

use crate::models::{DbBlock, DbEvent, DbFaceoff, DbGoal, DbHit, DbPenalty, DbShot};

/// Insert all events for a game atomically in a single PostgreSQL transaction.
///
/// Uses UNNEST-based bulk inserts: one SQL roundtrip for base events
/// (with RETURNING id to build the FK map), then one per non-empty child type.
/// This replaces the previous per-row loop pattern (~300+ roundtrips/game).
///
/// Returns (events_inserted, goals_inserted, shots_inserted, hits_inserted,
///          blocks_inserted, penalties_inserted, faceoffs_inserted).
#[allow(clippy::too_many_arguments)]
pub async fn upsert_game_events(
    pool: &sqlx::PgPool,
    _game_id: i64,
    events: &[DbEvent],
    goals: &[DbGoal],
    shots: &[DbShot],
    hits: &[DbHit],
    blocks: &[DbBlock],
    penalties: &[DbPenalty],
    faceoffs: &[DbFaceoff],
) -> Result<(usize, usize, usize, usize, usize, usize, usize), sqlx::Error> {
    let mut tx = pool.begin().await?;

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
        let home_goalies: Vec<bool> = events.iter().map(|e| e.home_goalie_present).collect();
        let home_sks: Vec<i16> = events.iter().map(|e| e.home_skater_count).collect();
        let away_sks: Vec<i16> = events.iter().map(|e| e.away_skater_count).collect();
        let away_goalies: Vec<bool> = events.iter().map(|e| e.away_goalie_present).collect();
        let strengths: Vec<Option<&str>> = events.iter().map(|e| e.strength.as_deref()).collect();
        let situation_codes: Vec<Option<&str>> =
            events.iter().map(|e| e.situation_code.as_deref()).collect();

        let rows = sqlx::query(
            r#"
            INSERT INTO events
                (game_id, event_id_in_game, period, period_type, time_in_period,
                 event_type, x_coord, y_coord, zone_code, event_owner_team_id,
                 home_goalie_present, home_skater_count, away_skater_count,
                 away_goalie_present, strength, situation_code)
            SELECT * FROM UNNEST(
                $1::bigint[], $2::int[], $3::smallint[], $4::text[], $5::text[],
                $6::text[], $7::smallint[], $8::smallint[], $9::text[], $10::bigint[],
                $11::bool[], $12::smallint[], $13::smallint[], $14::bool[], $15::text[],
                $16::text[]
            ) AS t(game_id, event_id_in_game, period, period_type, time_in_period,
                   event_type, x_coord, y_coord, zone_code, event_owner_team_id,
                   home_goalie_present, home_skater_count, away_skater_count,
                   away_goalie_present, strength, situation_code)
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
                situation_code      = EXCLUDED.situation_code
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
        .bind(&situation_codes)
        .fetch_all(&mut *tx)
        .await?;

        for row in &rows {
            let id: i64 = row.try_get("id")?;
            let eid: i32 = row.try_get("event_id_in_game")?;
            event_db_id_map.insert(eid, id);
        }
    }

    let events_inserted = events.len();

    // ── Bulk insert goals ─────────────────────────────────────────────────────
    let goals_matched: Vec<(i64, &DbGoal)> = goals
        .iter()
        .filter_map(|g| event_db_id_map.get(&g.event_id_in_game).map(|&id| (id, g)))
        .collect();

    if !goals_matched.is_empty() {
        let eids: Vec<i64> = goals_matched.iter().map(|(id, _)| *id).collect();
        let scorers: Vec<Option<i64>> = goals_matched
            .iter()
            .map(|(_, g)| g.scorer_player_id)
            .collect();
        let a1: Vec<Option<i64>> = goals_matched
            .iter()
            .map(|(_, g)| g.assist1_player_id)
            .collect();
        let a2: Vec<Option<i64>> = goals_matched
            .iter()
            .map(|(_, g)| g.assist2_player_id)
            .collect();
        let goalies: Vec<Option<i64>> = goals_matched.iter().map(|(_, g)| g.goalie_id).collect();
        let stypes: Vec<Option<&str>> = goals_matched
            .iter()
            .map(|(_, g)| g.shot_type.as_deref())
            .collect();

        sqlx::query(
            r#"
            INSERT INTO goals (event_id, scorer_player_id, assist1_player_id, assist2_player_id,
                               goalie_id, shot_type)
            SELECT * FROM UNNEST($1::bigint[], $2::bigint[], $3::bigint[], $4::bigint[],
                                 $5::bigint[], $6::text[])
              AS t(event_id, scorer_player_id, assist1_player_id, assist2_player_id,
                   goalie_id, shot_type)
            ON CONFLICT (event_id) DO UPDATE SET
                scorer_player_id  = EXCLUDED.scorer_player_id,
                assist1_player_id = EXCLUDED.assist1_player_id,
                assist2_player_id = EXCLUDED.assist2_player_id,
                goalie_id         = EXCLUDED.goalie_id,
                shot_type         = EXCLUDED.shot_type
            "#,
        )
        .bind(&eids)
        .bind(&scorers)
        .bind(&a1)
        .bind(&a2)
        .bind(&goalies)
        .bind(&stypes)
        .execute(&mut *tx)
        .await?;
    }

    let goals_inserted = goals_matched.len();

    // ── Bulk insert shots ─────────────────────────────────────────────────────
    let shots_matched: Vec<(i64, &DbShot)> = shots
        .iter()
        .filter_map(|s| event_db_id_map.get(&s.event_id_in_game).map(|&id| (id, s)))
        .collect();

    if !shots_matched.is_empty() {
        let eids: Vec<i64> = shots_matched.iter().map(|(id, _)| *id).collect();
        let shooters: Vec<Option<i64>> = shots_matched
            .iter()
            .map(|(_, s)| s.shooting_player_id)
            .collect();
        let goalies: Vec<Option<i64>> = shots_matched
            .iter()
            .map(|(_, s)| s.goalie_in_net_id)
            .collect();
        let stypes: Vec<Option<&str>> = shots_matched
            .iter()
            .map(|(_, s)| s.shot_type.as_deref())
            .collect();

        sqlx::query(
            r#"
            INSERT INTO shots (event_id, shooting_player_id, goalie_in_net_id, shot_type)
            SELECT * FROM UNNEST($1::bigint[], $2::bigint[], $3::bigint[], $4::text[])
              AS t(event_id, shooting_player_id, goalie_in_net_id, shot_type)
            ON CONFLICT (event_id) DO UPDATE SET
                shooting_player_id = EXCLUDED.shooting_player_id,
                goalie_in_net_id   = EXCLUDED.goalie_in_net_id,
                shot_type          = EXCLUDED.shot_type
            "#,
        )
        .bind(&eids)
        .bind(&shooters)
        .bind(&goalies)
        .bind(&stypes)
        .execute(&mut *tx)
        .await?;
    }

    let shots_inserted = shots_matched.len();

    // ── Bulk insert hits ──────────────────────────────────────────────────────
    let hits_matched: Vec<(i64, &DbHit)> = hits
        .iter()
        .filter_map(|h| event_db_id_map.get(&h.event_id_in_game).map(|&id| (id, h)))
        .collect();

    if !hits_matched.is_empty() {
        let eids: Vec<i64> = hits_matched.iter().map(|(id, _)| *id).collect();
        let hitters: Vec<Option<i64>> = hits_matched
            .iter()
            .map(|(_, h)| h.hitting_player_id)
            .collect();
        let hittees: Vec<Option<i64>> = hits_matched
            .iter()
            .map(|(_, h)| h.hittee_player_id)
            .collect();

        sqlx::query(
            r#"
            INSERT INTO hits (event_id, hitting_player_id, hittee_player_id)
            SELECT * FROM UNNEST($1::bigint[], $2::bigint[], $3::bigint[])
              AS t(event_id, hitting_player_id, hittee_player_id)
            ON CONFLICT (event_id) DO UPDATE SET
                hitting_player_id = EXCLUDED.hitting_player_id,
                hittee_player_id  = EXCLUDED.hittee_player_id
            "#,
        )
        .bind(&eids)
        .bind(&hitters)
        .bind(&hittees)
        .execute(&mut *tx)
        .await?;
    }

    let hits_inserted = hits_matched.len();

    // ── Bulk insert blocks ────────────────────────────────────────────────────
    let blocks_matched: Vec<(i64, &DbBlock)> = blocks
        .iter()
        .filter_map(|b| event_db_id_map.get(&b.event_id_in_game).map(|&id| (id, b)))
        .collect();

    if !blocks_matched.is_empty() {
        let eids: Vec<i64> = blocks_matched.iter().map(|(id, _)| *id).collect();
        let blockers: Vec<Option<i64>> = blocks_matched
            .iter()
            .map(|(_, b)| b.blocking_player_id)
            .collect();
        let shooters: Vec<Option<i64>> = blocks_matched
            .iter()
            .map(|(_, b)| b.shooting_player_id)
            .collect();

        sqlx::query(
            r#"
            INSERT INTO blocks (event_id, blocking_player_id, shooting_player_id)
            SELECT * FROM UNNEST($1::bigint[], $2::bigint[], $3::bigint[])
              AS t(event_id, blocking_player_id, shooting_player_id)
            ON CONFLICT (event_id) DO UPDATE SET
                blocking_player_id = EXCLUDED.blocking_player_id,
                shooting_player_id = EXCLUDED.shooting_player_id
            "#,
        )
        .bind(&eids)
        .bind(&blockers)
        .bind(&shooters)
        .execute(&mut *tx)
        .await?;
    }

    let blocks_inserted = blocks_matched.len();

    // ── Bulk insert penalties ─────────────────────────────────────────────────
    let penalties_matched: Vec<(i64, &DbPenalty)> = penalties
        .iter()
        .filter_map(|p| event_db_id_map.get(&p.event_id_in_game).map(|&id| (id, p)))
        .collect();

    if !penalties_matched.is_empty() {
        let eids: Vec<i64> = penalties_matched.iter().map(|(id, _)| *id).collect();
        let cbys: Vec<Option<i64>> = penalties_matched
            .iter()
            .map(|(_, p)| p.committed_by_player_id)
            .collect();
        let dbys: Vec<Option<i64>> = penalties_matched
            .iter()
            .map(|(_, p)| p.drawn_by_player_id)
            .collect();
        let infracts: Vec<Option<&str>> = penalties_matched
            .iter()
            .map(|(_, p)| p.infraction_type.as_deref())
            .collect();
        let durs: Vec<Option<i16>> = penalties_matched
            .iter()
            .map(|(_, p)| p.duration_minutes)
            .collect();

        sqlx::query(
            r#"
            INSERT INTO penalties (event_id, committed_by_player_id, drawn_by_player_id,
                                   infraction_type, duration_minutes)
            SELECT * FROM UNNEST($1::bigint[], $2::bigint[], $3::bigint[], $4::text[], $5::smallint[])
              AS t(event_id, committed_by_player_id, drawn_by_player_id,
                   infraction_type, duration_minutes)
            ON CONFLICT (event_id) DO UPDATE SET
                committed_by_player_id = EXCLUDED.committed_by_player_id,
                drawn_by_player_id     = EXCLUDED.drawn_by_player_id,
                infraction_type        = EXCLUDED.infraction_type,
                duration_minutes       = EXCLUDED.duration_minutes
            "#,
        )
        .bind(&eids)
        .bind(&cbys)
        .bind(&dbys)
        .bind(&infracts)
        .bind(&durs)
        .execute(&mut *tx)
        .await?;
    }

    let penalties_inserted = penalties_matched.len();

    // ── Bulk insert faceoffs ──────────────────────────────────────────────────
    let faceoffs_matched: Vec<(i64, &DbFaceoff)> = faceoffs
        .iter()
        .filter_map(|f| event_db_id_map.get(&f.event_id_in_game).map(|&id| (id, f)))
        .collect();

    if !faceoffs_matched.is_empty() {
        let eids: Vec<i64> = faceoffs_matched.iter().map(|(id, _)| *id).collect();
        let winners: Vec<Option<i64>> = faceoffs_matched
            .iter()
            .map(|(_, f)| f.winning_player_id)
            .collect();
        let losers: Vec<Option<i64>> = faceoffs_matched
            .iter()
            .map(|(_, f)| f.losing_player_id)
            .collect();

        sqlx::query(
            r#"
            INSERT INTO faceoffs (event_id, winning_player_id, losing_player_id)
            SELECT * FROM UNNEST($1::bigint[], $2::bigint[], $3::bigint[])
              AS t(event_id, winning_player_id, losing_player_id)
            ON CONFLICT (event_id) DO UPDATE SET
                winning_player_id = EXCLUDED.winning_player_id,
                losing_player_id  = EXCLUDED.losing_player_id
            "#,
        )
        .bind(&eids)
        .bind(&winners)
        .bind(&losers)
        .execute(&mut *tx)
        .await?;
    }

    let faceoffs_inserted = faceoffs_matched.len();

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
