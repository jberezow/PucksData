//! Orchestration for loading and auditing official player/game statistics.

use std::sync::Arc;

use tokio::{sync::Semaphore, task::JoinSet};

pub struct OfficialGamesSummary {
    pub games: usize,
    pub skaters: usize,
    pub goalies: usize,
}

async fn load_one_inner(
    pool: &sqlx::PgPool,
    game_id: i64,
    game_type: i16,
) -> Result<(usize, usize), crate::AnyError> {
    let stats =
        crate::fetchers::official_games::fetch_official_game_stats(game_id, game_type).await?;
    if stats.skaters.is_empty() && stats.goalies.is_empty() {
        return Err(format!("official reports returned no players for game {game_id}").into());
    }
    Ok(crate::loaders::official_games::replace_official_game_stats(pool, &stats).await?)
}

async fn load_one(
    pool: &sqlx::PgPool,
    game_id: i64,
    game_type: i16,
) -> Result<(usize, usize), crate::AnyError> {
    super::attempts::track(
        pool,
        "official_games",
        &game_id.to_string(),
        load_one_inner(pool, game_id, game_type),
    )
    .await
}

/// Load one game, or every completed game in an inclusive calendar-date range.
pub async fn run_official_games(
    pool: &sqlx::PgPool,
    game_id: Option<i64>,
    from: Option<time::Date>,
    to: Option<time::Date>,
) -> Result<OfficialGamesSummary, crate::AnyError> {
    if from.zip(to).is_some_and(|(start, end)| start > end) {
        return Err("official audit start date must not follow end date".into());
    }
    let candidates: Vec<(i64, i16)> = if let Some(game_id) = game_id {
        sqlx::query_as::<_, (i64, i16)>(
            "SELECT game_id, game_type FROM games WHERE game_id = $1 AND game_state IN ('OFF','OVER','FINAL') AND game_type IN (2,3)",
        )
        .bind(game_id)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, (i64, i16)>(
            r#"SELECT game_id, game_type
               FROM games
               WHERE game_state IN ('OFF','OVER','FINAL')
                 AND game_type IN (2, 3)
                 AND game_date >= $1
                 AND ($2::date IS NULL OR game_date <= $2)
               ORDER BY game_date, game_id"#,
        )
        .bind(from.ok_or("--from is required unless --game is supplied")?)
        .bind(to)
        .fetch_all(pool)
        .await?
    };
    if game_id.is_some() && candidates.is_empty() {
        return Err("game does not exist or is not complete".into());
    }

    load_candidates(pool, candidates).await
}

/// Sync retries unsuccessful observations even after they leave the audit window.
pub async fn query_sync_candidates(
    pool: &sqlx::PgPool,
    from: time::Date,
) -> Result<Vec<(i64, i16)>, sqlx::Error> {
    sqlx::query_as(
        r#"WITH latest AS MATERIALIZED (
            SELECT DISTINCT ON(entity_key) entity_key, outcome
            FROM ingestion.attempts WHERE dataset='official_games'
            ORDER BY entity_key, attempt_id DESC
        ), candidates AS (
            SELECT game_id FROM games WHERE game_date >= $1
            UNION
            SELECT g.game_id FROM games g JOIN latest a ON a.entity_key=g.game_id::text
            WHERE a.outcome IN ('failed','running','unavailable')
        )
        SELECT g.game_id, g.game_type FROM games g JOIN candidates c USING(game_id)
        WHERE g.game_state IN ('OFF','OVER','FINAL') AND g.game_type IN (2,3)
        ORDER BY g.game_date, g.game_id"#,
    )
    .bind(from)
    .fetch_all(pool)
    .await
}

pub async fn sync_official_games(
    pool: &sqlx::PgPool,
    from: time::Date,
) -> Result<OfficialGamesSummary, crate::AnyError> {
    load_candidates(pool, query_sync_candidates(pool, from).await?).await
}

/// Recover missed initial reports throughout active seasons, including outages
/// longer than the correction window. Historical archive repairs remain manual.
pub async fn query_current_candidates(
    pool: &sqlx::PgPool,
    from: time::Date,
    seasons: &[i32],
) -> Result<Vec<(i64, i16)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT g.game_id, g.game_type FROM games g
         LEFT JOIN LATERAL (
             SELECT outcome FROM ingestion.attempts
             WHERE dataset='official_games' AND entity_key=g.game_id::text
             ORDER BY attempt_id DESC LIMIT 1
         ) latest ON TRUE
         WHERE g.season=ANY($2) AND g.game_type IN (2,3)
           AND g.game_state IN ('OFF','OVER','FINAL')
           AND (g.game_date >= $1 OR latest.outcome IS DISTINCT FROM 'complete')
         ORDER BY g.game_date,g.game_id",
    )
    .bind(from)
    .bind(seasons)
    .fetch_all(pool)
    .await
}

pub async fn sync_current_official_games(
    pool: &sqlx::PgPool,
    from: time::Date,
    seasons: &[i32],
) -> Result<OfficialGamesSummary, crate::AnyError> {
    load_candidates(pool, query_current_candidates(pool, from, seasons).await?).await
}

async fn load_candidates(
    pool: &sqlx::PgPool,
    candidates: Vec<(i64, i16)>,
) -> Result<OfficialGamesSummary, crate::AnyError> {
    let semaphore = Arc::new(Semaphore::new(5));
    let mut tasks = JoinSet::new();
    for (game_id, game_type) in candidates.iter().copied() {
        let permit = semaphore.clone().acquire_owned().await?;
        let pool = pool.clone();
        tasks.spawn(async move {
            let _permit = permit;
            (game_id, load_one(&pool, game_id, game_type).await)
        });
    }

    let mut skaters = 0;
    let mut goalies = 0;
    let mut failures: Vec<crate::AnyError> = Vec::new();
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((_, Ok((game_skaters, game_goalies)))) => {
                skaters += game_skaters;
                goalies += game_goalies;
            }
            Ok((game_id, Err(error))) => {
                if error.is::<crate::error::Deferred>() {
                    tracing::warn!(game_id, error = %error, "official report pending; retry on next pass");
                } else {
                    tracing::error!(game_id, error = %error, "official game load failed");
                }
                failures.push(error);
            }
            Err(error) => failures.push(Box::new(error)),
        }
    }
    if !failures.is_empty() {
        return Err(crate::error::batch_error(
            "official game loads incomplete",
            failures,
        ));
    }

    Ok(OfficialGamesSummary {
        games: candidates.len(),
        skaters,
        goalies,
    })
}
