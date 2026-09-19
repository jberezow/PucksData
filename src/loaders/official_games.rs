//! Transactional replacement of league-published player/game statistics.

use crate::fetchers::official_games::OfficialGameStats;

/// Replace the complete official snapshot for one game.
///
/// Re-observing identical values only advances `source_observed_at`. A material
/// correction increments `source_revision`, allowing consumers to import a
/// correction without maintaining their own source diff.
pub async fn replace_official_game_stats(
    pool: &sqlx::PgPool,
    stats: &OfficialGameStats,
) -> Result<(usize, usize), sqlx::Error> {
    let game_id = stats.game_id;
    let mut transaction = pool.begin().await?;

    for row in &stats.skaters {
        sqlx::query(
            r#"
            INSERT INTO analytics.official_skater_games AS current
                (game_id, player_id, season, game_type, team_abbrev, full_name,
                 position_code, goals, assists, points, plus_minus, penalty_minutes,
                 shots, ev_goals, ev_points, pp_goals, pp_points, sh_goals, sh_points,
                 ot_goals, game_winning_goals, hits, blocked_shots, giveaways,
                 takeaways, time_on_ice_seconds)
            VALUES
                ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,
                 $18,$19,$20,$21,$22,$23,$24,$25,$26)
            ON CONFLICT (game_id, player_id) DO UPDATE SET
                season = EXCLUDED.season,
                game_type = EXCLUDED.game_type,
                team_abbrev = EXCLUDED.team_abbrev,
                full_name = EXCLUDED.full_name,
                position_code = EXCLUDED.position_code,
                goals = EXCLUDED.goals,
                assists = EXCLUDED.assists,
                points = EXCLUDED.points,
                plus_minus = EXCLUDED.plus_minus,
                penalty_minutes = EXCLUDED.penalty_minutes,
                shots = EXCLUDED.shots,
                ev_goals = EXCLUDED.ev_goals,
                ev_points = EXCLUDED.ev_points,
                pp_goals = EXCLUDED.pp_goals,
                pp_points = EXCLUDED.pp_points,
                sh_goals = EXCLUDED.sh_goals,
                sh_points = EXCLUDED.sh_points,
                ot_goals = EXCLUDED.ot_goals,
                game_winning_goals = EXCLUDED.game_winning_goals,
                hits = EXCLUDED.hits,
                blocked_shots = EXCLUDED.blocked_shots,
                giveaways = EXCLUDED.giveaways,
                takeaways = EXCLUDED.takeaways,
                time_on_ice_seconds = EXCLUDED.time_on_ice_seconds,
                source_revision = current.source_revision + CASE WHEN
                    ROW(current.season, current.game_type, current.team_abbrev,
                        current.full_name, current.position_code, current.goals,
                        current.assists, current.points, current.plus_minus,
                        current.penalty_minutes, current.shots, current.ev_goals,
                        current.ev_points, current.pp_goals, current.pp_points,
                        current.sh_goals, current.sh_points, current.ot_goals,
                        current.game_winning_goals, current.hits, current.blocked_shots,
                        current.giveaways, current.takeaways, current.time_on_ice_seconds)
                    IS DISTINCT FROM
                    ROW(EXCLUDED.season, EXCLUDED.game_type, EXCLUDED.team_abbrev,
                        EXCLUDED.full_name, EXCLUDED.position_code, EXCLUDED.goals,
                        EXCLUDED.assists, EXCLUDED.points, EXCLUDED.plus_minus,
                        EXCLUDED.penalty_minutes, EXCLUDED.shots, EXCLUDED.ev_goals,
                        EXCLUDED.ev_points, EXCLUDED.pp_goals, EXCLUDED.pp_points,
                        EXCLUDED.sh_goals, EXCLUDED.sh_points, EXCLUDED.ot_goals,
                        EXCLUDED.game_winning_goals, EXCLUDED.hits, EXCLUDED.blocked_shots,
                        EXCLUDED.giveaways, EXCLUDED.takeaways, EXCLUDED.time_on_ice_seconds)
                    THEN 1 ELSE 0 END,
                source_observed_at = NOW(),
                updated_at = CASE WHEN
                    ROW(current.goals, current.assists, current.points, current.plus_minus,
                        current.penalty_minutes, current.shots, current.ev_goals,
                        current.ev_points, current.pp_goals, current.pp_points,
                        current.sh_goals, current.sh_points, current.ot_goals,
                        current.game_winning_goals, current.hits, current.blocked_shots,
                        current.giveaways, current.takeaways, current.time_on_ice_seconds)
                    IS DISTINCT FROM
                    ROW(EXCLUDED.goals, EXCLUDED.assists, EXCLUDED.points, EXCLUDED.plus_minus,
                        EXCLUDED.penalty_minutes, EXCLUDED.shots, EXCLUDED.ev_goals,
                        EXCLUDED.ev_points, EXCLUDED.pp_goals, EXCLUDED.pp_points,
                        EXCLUDED.sh_goals, EXCLUDED.sh_points, EXCLUDED.ot_goals,
                        EXCLUDED.game_winning_goals, EXCLUDED.hits, EXCLUDED.blocked_shots,
                        EXCLUDED.giveaways, EXCLUDED.takeaways, EXCLUDED.time_on_ice_seconds)
                    THEN NOW() ELSE current.updated_at END
            "#,
        )
        .bind(row.game_id)
        .bind(row.player_id)
        .bind(row.season)
        .bind(row.game_type)
        .bind(&row.team_abbrev)
        .bind(&row.full_name)
        .bind(&row.position_code)
        .bind(row.goals)
        .bind(row.assists)
        .bind(row.points)
        .bind(row.plus_minus)
        .bind(row.penalty_minutes)
        .bind(row.shots)
        .bind(row.ev_goals)
        .bind(row.ev_points)
        .bind(row.pp_goals)
        .bind(row.pp_points)
        .bind(row.sh_goals)
        .bind(row.sh_points)
        .bind(row.ot_goals)
        .bind(row.game_winning_goals)
        .bind(row.hits)
        .bind(row.blocked_shots)
        .bind(row.giveaways)
        .bind(row.takeaways)
        .bind(row.time_on_ice_seconds)
        .execute(&mut *transaction)
        .await?;
    }

    for row in &stats.goalies {
        sqlx::query(
            r#"
            INSERT INTO analytics.official_goalie_games AS current
                (game_id, player_id, season, game_type, team_abbrev, full_name,
                 goals, assists, games_started, wins, losses, ties, ot_losses, shutouts,
                 shots_against, saves, goals_against, save_pct, time_on_ice_seconds)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
            ON CONFLICT (game_id, player_id) DO UPDATE SET
                season = EXCLUDED.season,
                game_type = EXCLUDED.game_type,
                team_abbrev = EXCLUDED.team_abbrev,
                full_name = EXCLUDED.full_name,
                goals = EXCLUDED.goals,
                assists = EXCLUDED.assists,
                games_started = EXCLUDED.games_started,
                wins = EXCLUDED.wins,
                losses = EXCLUDED.losses,
                ties = EXCLUDED.ties,
                ot_losses = EXCLUDED.ot_losses,
                shutouts = EXCLUDED.shutouts,
                shots_against = EXCLUDED.shots_against,
                saves = EXCLUDED.saves,
                goals_against = EXCLUDED.goals_against,
                save_pct = EXCLUDED.save_pct,
                time_on_ice_seconds = EXCLUDED.time_on_ice_seconds,
                source_revision = current.source_revision + CASE WHEN
                    ROW(current.season, current.game_type, current.team_abbrev,
                        current.full_name, current.goals, current.assists,
                        current.games_started, current.wins,
                        current.losses, current.ties, current.ot_losses, current.shutouts,
                        current.shots_against, current.saves, current.goals_against,
                        current.save_pct, current.time_on_ice_seconds)
                    IS DISTINCT FROM
                    ROW(EXCLUDED.season, EXCLUDED.game_type, EXCLUDED.team_abbrev,
                        EXCLUDED.full_name, EXCLUDED.goals, EXCLUDED.assists,
                        EXCLUDED.games_started, EXCLUDED.wins,
                        EXCLUDED.losses, EXCLUDED.ties, EXCLUDED.ot_losses, EXCLUDED.shutouts,
                        EXCLUDED.shots_against, EXCLUDED.saves, EXCLUDED.goals_against,
                        EXCLUDED.save_pct, EXCLUDED.time_on_ice_seconds)
                    THEN 1 ELSE 0 END,
                source_observed_at = NOW(),
                updated_at = CASE WHEN
                    ROW(current.goals, current.assists, current.games_started,
                        current.wins, current.losses, current.ties,
                        current.ot_losses, current.shutouts, current.shots_against,
                        current.saves, current.goals_against, current.save_pct,
                        current.time_on_ice_seconds)
                    IS DISTINCT FROM
                    ROW(EXCLUDED.goals, EXCLUDED.assists, EXCLUDED.games_started,
                        EXCLUDED.wins, EXCLUDED.losses, EXCLUDED.ties,
                        EXCLUDED.ot_losses, EXCLUDED.shutouts, EXCLUDED.shots_against,
                        EXCLUDED.saves, EXCLUDED.goals_against, EXCLUDED.save_pct,
                        EXCLUDED.time_on_ice_seconds)
                    THEN NOW() ELSE current.updated_at END
            "#,
        )
        .bind(row.game_id)
        .bind(row.player_id)
        .bind(row.season)
        .bind(row.game_type)
        .bind(&row.team_abbrev)
        .bind(&row.full_name)
        .bind(row.goals)
        .bind(row.assists)
        .bind(row.games_started)
        .bind(row.wins)
        .bind(row.losses)
        .bind(row.ties)
        .bind(row.ot_losses)
        .bind(row.shutouts)
        .bind(row.shots_against)
        .bind(row.saves)
        .bind(row.goals_against)
        .bind(row.save_pct)
        .bind(row.time_on_ice_seconds)
        .execute(&mut *transaction)
        .await?;
    }

    let skater_ids: Vec<i64> = stats.skaters.iter().map(|row| row.player_id).collect();
    let goalie_ids: Vec<i64> = stats.goalies.iter().map(|row| row.player_id).collect();
    sqlx::query(
        "DELETE FROM analytics.official_skater_games WHERE game_id = $1 AND NOT (player_id = ANY($2))",
    )
    .bind(game_id)
    .bind(&skater_ids)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "DELETE FROM analytics.official_goalie_games WHERE game_id = $1 AND NOT (player_id = ANY($2))",
    )
    .bind(game_id)
    .bind(&goalie_ids)
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;
    Ok((stats.skaters.len(), stats.goalies.len()))
}
