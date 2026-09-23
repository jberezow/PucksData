//! Bounded batch reads under a single read-only repeatable-read snapshot.
use super::types::*;
use std::collections::BTreeMap;

pub const BATCH_SIZE: usize = 32;
pub const SHIFTS_SQL: &str = include_str!("sql/shifts.sql");
pub const EVENTS_SQL: &str = include_str!("sql/events.sql");
pub const OFFICIAL_SQL: &str = include_str!("sql/official.sql");

pub async fn snapshot(
    pool: &sqlx::PgPool,
) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL TIME ZONE 'UTC'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout = '60s'")
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

pub async fn games(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    season: Option<i32>,
    game: Option<i64>,
) -> Result<Vec<Game>, sqlx::Error> {
    sqlx::query_as(include_str!("sql/games.sql"))
        .bind(season)
        .bind(game)
        .fetch_all(&mut **tx)
        .await
}

pub async fn batch(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    games: &[Game],
) -> Result<Vec<GameSource>, sqlx::Error> {
    let ids: Vec<_> = games.iter().map(|g| g.game_id).collect();
    let shifts: Vec<Shift> = sqlx::query_as(SHIFTS_SQL)
        .bind(&ids)
        .fetch_all(&mut **tx)
        .await?;
    let events: Vec<Event> = sqlx::query_as(EVENTS_SQL)
        .bind(&ids)
        .fetch_all(&mut **tx)
        .await?;
    let official: Vec<OfficialToi> = sqlx::query_as(OFFICIAL_SQL)
        .bind(&ids)
        .fetch_all(&mut **tx)
        .await?;
    let mut sources: BTreeMap<_, _> = games
        .iter()
        .map(|g| {
            (
                g.game_id,
                GameSource {
                    game: g.clone(),
                    shifts: vec![],
                    events: vec![],
                    official_toi: vec![],
                },
            )
        })
        .collect();
    for r in shifts {
        sources.get_mut(&r.game_id).unwrap().shifts.push(r);
    }
    for r in events {
        sources.get_mut(&r.game_id).unwrap().events.push(r);
    }
    for r in official {
        sources.get_mut(&r.game_id).unwrap().official_toi.push(r);
    }
    Ok(sources.into_values().collect())
}

pub async fn profile(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    games: &[Game],
) -> Result<BTreeMap<String, serde_json::Value>, sqlx::Error> {
    let ids: Vec<_> = games.iter().map(|g| g.game_id).collect();
    let mut plans = BTreeMap::new();
    for (name, sql) in [
        ("shifts", SHIFTS_SQL),
        ("events", EVENTS_SQL),
        ("official_toi", OFFICIAL_SQL),
    ] {
        // Only compile-time SQL constants are interpolated; game IDs remain bound.
        let query = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}");
        let result: serde_json::Value = sqlx::query_scalar(sqlx::AssertSqlSafe(query.as_str()))
            .bind(&ids)
            .fetch_one(&mut **tx)
            .await?;
        plans.insert(name.into(), result);
    }
    Ok(plans)
}
