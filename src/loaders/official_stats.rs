//! Upserts official NHL season totals into the `analytics` schema.

use crate::models::{DbOfficialGoalieSeason, DbOfficialSkaterSeason};

/// Upsert a batch of official skater season rows.
///
/// Keyed on (player_id, season, game_type), so re-running a season replaces
/// its rows rather than duplicating them. The NHL revises historical totals
/// occasionally, and a refetch should adopt the revision.
pub async fn upsert_skater_seasons(
    pool: &sqlx::PgPool,
    records: &[DbOfficialSkaterSeason],
) -> Result<usize, sqlx::Error> {
    if records.is_empty() {
        return Ok(0);
    }

    let player_ids: Vec<i64> = records.iter().map(|r| r.player_id).collect();
    let seasons: Vec<i32> = records.iter().map(|r| r.season).collect();
    let game_types: Vec<i16> = records.iter().map(|r| r.game_type).collect();
    let full_names: Vec<&str> = records.iter().map(|r| r.full_name.as_str()).collect();
    let positions: Vec<Option<&str>> = records.iter().map(|r| r.position_code.as_deref()).collect();
    let shoots: Vec<Option<&str>> = records
        .iter()
        .map(|r| r.shoots_catches.as_deref())
        .collect();
    let teams: Vec<Option<&str>> = records.iter().map(|r| r.team_abbrevs.as_deref()).collect();
    let games_played: Vec<Option<i32>> = records.iter().map(|r| r.games_played).collect();
    let goals: Vec<Option<i32>> = records.iter().map(|r| r.goals).collect();
    let assists: Vec<Option<i32>> = records.iter().map(|r| r.assists).collect();
    let points: Vec<Option<i32>> = records.iter().map(|r| r.points).collect();
    let plus_minus: Vec<Option<i32>> = records.iter().map(|r| r.plus_minus).collect();
    let pim: Vec<Option<i32>> = records.iter().map(|r| r.penalty_minutes).collect();
    let shots: Vec<Option<i32>> = records.iter().map(|r| r.shots).collect();
    let shooting_pct: Vec<Option<f64>> = records.iter().map(|r| r.shooting_pct).collect();
    let ev_goals: Vec<Option<i32>> = records.iter().map(|r| r.ev_goals).collect();
    let ev_points: Vec<Option<i32>> = records.iter().map(|r| r.ev_points).collect();
    let pp_goals: Vec<Option<i32>> = records.iter().map(|r| r.pp_goals).collect();
    let pp_points: Vec<Option<i32>> = records.iter().map(|r| r.pp_points).collect();
    let sh_goals: Vec<Option<i32>> = records.iter().map(|r| r.sh_goals).collect();
    let sh_points: Vec<Option<i32>> = records.iter().map(|r| r.sh_points).collect();
    let ot_goals: Vec<Option<i32>> = records.iter().map(|r| r.ot_goals).collect();
    let gwg: Vec<Option<i32>> = records.iter().map(|r| r.game_winning_goals).collect();
    let ppg: Vec<Option<f64>> = records.iter().map(|r| r.points_per_game).collect();
    let faceoff: Vec<Option<f64>> = records.iter().map(|r| r.faceoff_win_pct).collect();
    let toi: Vec<Option<f64>> = records.iter().map(|r| r.time_on_ice_per_game).collect();

    sqlx::query(
        r#"
        INSERT INTO analytics.official_skater_seasons
            (player_id, season, game_type, full_name, position_code, shoots_catches,
             team_abbrevs, games_played, goals, assists, points, plus_minus,
             penalty_minutes, shots, shooting_pct, ev_goals, ev_points, pp_goals,
             pp_points, sh_goals, sh_points, ot_goals, game_winning_goals,
             points_per_game, faceoff_win_pct, time_on_ice_per_game)
        SELECT * FROM UNNEST(
            $1::bigint[], $2::int[], $3::smallint[], $4::text[], $5::text[], $6::text[],
            $7::text[], $8::int[], $9::int[], $10::int[], $11::int[], $12::int[],
            $13::int[], $14::int[], $15::double precision[], $16::int[], $17::int[], $18::int[],
            $19::int[], $20::int[], $21::int[], $22::int[], $23::int[],
            $24::double precision[], $25::double precision[], $26::double precision[]
        )
        ON CONFLICT (player_id, season, game_type) DO UPDATE SET
            full_name            = EXCLUDED.full_name,
            position_code        = EXCLUDED.position_code,
            shoots_catches       = EXCLUDED.shoots_catches,
            team_abbrevs         = EXCLUDED.team_abbrevs,
            games_played         = EXCLUDED.games_played,
            goals                = EXCLUDED.goals,
            assists              = EXCLUDED.assists,
            points               = EXCLUDED.points,
            plus_minus           = EXCLUDED.plus_minus,
            penalty_minutes      = EXCLUDED.penalty_minutes,
            shots                = EXCLUDED.shots,
            shooting_pct         = EXCLUDED.shooting_pct,
            ev_goals             = EXCLUDED.ev_goals,
            ev_points            = EXCLUDED.ev_points,
            pp_goals             = EXCLUDED.pp_goals,
            pp_points            = EXCLUDED.pp_points,
            sh_goals             = EXCLUDED.sh_goals,
            sh_points            = EXCLUDED.sh_points,
            ot_goals             = EXCLUDED.ot_goals,
            game_winning_goals   = EXCLUDED.game_winning_goals,
            points_per_game      = EXCLUDED.points_per_game,
            faceoff_win_pct      = EXCLUDED.faceoff_win_pct,
            time_on_ice_per_game = EXCLUDED.time_on_ice_per_game
        "#,
    )
    .bind(&player_ids)
    .bind(&seasons)
    .bind(&game_types)
    .bind(&full_names)
    .bind(&positions)
    .bind(&shoots)
    .bind(&teams)
    .bind(&games_played)
    .bind(&goals)
    .bind(&assists)
    .bind(&points)
    .bind(&plus_minus)
    .bind(&pim)
    .bind(&shots)
    .bind(&shooting_pct)
    .bind(&ev_goals)
    .bind(&ev_points)
    .bind(&pp_goals)
    .bind(&pp_points)
    .bind(&sh_goals)
    .bind(&sh_points)
    .bind(&ot_goals)
    .bind(&gwg)
    .bind(&ppg)
    .bind(&faceoff)
    .bind(&toi)
    .execute(pool)
    .await?;

    Ok(records.len())
}

/// Upsert a batch of official goalie season rows.
pub async fn upsert_goalie_seasons(
    pool: &sqlx::PgPool,
    records: &[DbOfficialGoalieSeason],
) -> Result<usize, sqlx::Error> {
    if records.is_empty() {
        return Ok(0);
    }

    let player_ids: Vec<i64> = records.iter().map(|r| r.player_id).collect();
    let seasons: Vec<i32> = records.iter().map(|r| r.season).collect();
    let game_types: Vec<i16> = records.iter().map(|r| r.game_type).collect();
    let full_names: Vec<&str> = records.iter().map(|r| r.full_name.as_str()).collect();
    let shoots: Vec<Option<&str>> = records
        .iter()
        .map(|r| r.shoots_catches.as_deref())
        .collect();
    let teams: Vec<Option<&str>> = records.iter().map(|r| r.team_abbrevs.as_deref()).collect();
    let games_played: Vec<Option<i32>> = records.iter().map(|r| r.games_played).collect();
    let games_started: Vec<Option<i32>> = records.iter().map(|r| r.games_started).collect();
    let wins: Vec<Option<i32>> = records.iter().map(|r| r.wins).collect();
    let losses: Vec<Option<i32>> = records.iter().map(|r| r.losses).collect();
    let ties: Vec<Option<i32>> = records.iter().map(|r| r.ties).collect();
    let ot_losses: Vec<Option<i32>> = records.iter().map(|r| r.ot_losses).collect();
    let shutouts: Vec<Option<i32>> = records.iter().map(|r| r.shutouts).collect();
    let shots_against: Vec<Option<i32>> = records.iter().map(|r| r.shots_against).collect();
    let saves: Vec<Option<i32>> = records.iter().map(|r| r.saves).collect();
    let goals_against: Vec<Option<i32>> = records.iter().map(|r| r.goals_against).collect();
    let save_pct: Vec<Option<f64>> = records.iter().map(|r| r.save_pct).collect();
    let gaa: Vec<Option<f64>> = records.iter().map(|r| r.goals_against_average).collect();
    let toi: Vec<Option<i64>> = records.iter().map(|r| r.time_on_ice).collect();
    let goals: Vec<Option<i32>> = records.iter().map(|r| r.goals).collect();
    let assists: Vec<Option<i32>> = records.iter().map(|r| r.assists).collect();
    let points: Vec<Option<i32>> = records.iter().map(|r| r.points).collect();
    let pim: Vec<Option<i32>> = records.iter().map(|r| r.penalty_minutes).collect();

    sqlx::query(
        r#"
        INSERT INTO analytics.official_goalie_seasons
            (player_id, season, game_type, full_name, shoots_catches, team_abbrevs,
             games_played, games_started, wins, losses, ties, ot_losses, shutouts,
             shots_against, saves, goals_against, save_pct, goals_against_average,
             time_on_ice, goals, assists, points, penalty_minutes)
        SELECT * FROM UNNEST(
            $1::bigint[], $2::int[], $3::smallint[], $4::text[], $5::text[], $6::text[],
            $7::int[], $8::int[], $9::int[], $10::int[], $11::int[], $12::int[], $13::int[],
            $14::int[], $15::int[], $16::int[], $17::double precision[], $18::double precision[],
            $19::bigint[], $20::int[], $21::int[], $22::int[], $23::int[]
        )
        ON CONFLICT (player_id, season, game_type) DO UPDATE SET
            full_name             = EXCLUDED.full_name,
            shoots_catches        = EXCLUDED.shoots_catches,
            team_abbrevs          = EXCLUDED.team_abbrevs,
            games_played          = EXCLUDED.games_played,
            games_started         = EXCLUDED.games_started,
            wins                  = EXCLUDED.wins,
            losses                = EXCLUDED.losses,
            ties                  = EXCLUDED.ties,
            ot_losses             = EXCLUDED.ot_losses,
            shutouts              = EXCLUDED.shutouts,
            shots_against         = EXCLUDED.shots_against,
            saves                 = EXCLUDED.saves,
            goals_against         = EXCLUDED.goals_against,
            save_pct              = EXCLUDED.save_pct,
            goals_against_average = EXCLUDED.goals_against_average,
            time_on_ice           = EXCLUDED.time_on_ice,
            goals                 = EXCLUDED.goals,
            assists               = EXCLUDED.assists,
            points                = EXCLUDED.points,
            penalty_minutes       = EXCLUDED.penalty_minutes
        "#,
    )
    .bind(&player_ids)
    .bind(&seasons)
    .bind(&game_types)
    .bind(&full_names)
    .bind(&shoots)
    .bind(&teams)
    .bind(&games_played)
    .bind(&games_started)
    .bind(&wins)
    .bind(&losses)
    .bind(&ties)
    .bind(&ot_losses)
    .bind(&shutouts)
    .bind(&shots_against)
    .bind(&saves)
    .bind(&goals_against)
    .bind(&save_pct)
    .bind(&gaa)
    .bind(&toi)
    .bind(&goals)
    .bind(&assists)
    .bind(&points)
    .bind(&pim)
    .execute(pool)
    .await?;

    Ok(records.len())
}
