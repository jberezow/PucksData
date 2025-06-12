use crate::storage::DbPool;

/// Get games by team for a season
pub async fn get_games_by_team(team_id: i32, season: Option<i32>, pool: &DbPool) -> Result<Vec<i64>, sqlx::Error> {
    if let Some(s) = season {
        let rows = sqlx::query!(
            "SELECT game_id FROM games WHERE (home_team_id = $1 OR away_team_id = $1) AND season = $2 ORDER BY game_date",
            team_id, s
        )
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|row| row.game_id).collect())
    } else {
        let rows = sqlx::query!(
            "SELECT game_id FROM games WHERE (home_team_id = $1 OR away_team_id = $1) ORDER BY game_date",
            team_id
        )
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|row| row.game_id).collect())
    }
}

/// Get team by ID
pub async fn get_team_by_id(team_id: i32, pool: &DbPool) -> Result<Option<(String, String)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT abbrev, common_name FROM teams WHERE team_id = $1",
        team_id
    )
    .fetch_optional(pool)
    .await?;
    
    Ok(row.map(|r| (r.abbrev, r.common_name)))
} 