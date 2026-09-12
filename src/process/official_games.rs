//! Orchestration for loading and auditing official player/game statistics.

use std::sync::Arc;

use tokio::{sync::Semaphore, task::JoinSet};

pub struct OfficialGamesSummary {
    pub games: usize,
    pub skaters: usize,
    pub goalies: usize,
}

async fn load_one(
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

/// Load one game, or every completed game in an inclusive calendar-date range.
pub async fn run_official_games(
    pool: &sqlx::PgPool,
    game_id: Option<i64>,
    from: Option<time::Date>,
    to: Option<time::Date>,
) -> Result<OfficialGamesSummary, crate::AnyError> {
    let candidates: Vec<(i64, i16)> = if let Some(game_id) = game_id {
        sqlx::query_as::<_, (i64, i16)>(
            "SELECT game_id, game_type FROM games WHERE game_id = $1 AND game_state IN ('OFF','OVER','FINAL')",
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
    let mut failures = Vec::new();
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((_, Ok((game_skaters, game_goalies)))) => {
                skaters += game_skaters;
                goalies += game_goalies;
            }
            Ok((game_id, Err(error))) => failures.push(format!("game {game_id}: {error}")),
            Err(error) => failures.push(format!("task failed: {error}")),
        }
    }
    if !failures.is_empty() {
        return Err(format!(
            "{} official game loads failed: {}",
            failures.len(),
            failures.join("; ")
        )
        .into());
    }

    Ok(OfficialGamesSummary {
        games: candidates.len(),
        skaters,
        goalies,
    })
}
