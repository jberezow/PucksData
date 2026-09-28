-- Consumer e8edeb9fb7415a7a3c72ad017d3b42ca1e779a9f: api/routes/pools.py
SELECT rosters.snapshot_id, rosters.observed_at, rosters.player_id,
       rosters.first_name, rosters.last_name, rosters.team_abbrev,
       rosters.roster_group, rosters.position_code,
       skaters.games_played AS skater_games_played, skaters.goals AS skater_goals,
       skaters.assists AS skater_assists, skaters.points AS skater_points,
       skaters.shots AS skater_shots, skaters.plus_minus AS skater_plus_minus,
       skaters.pp_points AS skater_power_play_points,
       skaters.sh_points AS skater_short_handed_points,
       skaters.game_winning_goals AS skater_game_winning_goals,
       physical_stats.hits AS skater_hits,
       physical_stats.blocks AS skater_blocks,
       goalies.games_played AS goalie_games_played,
       goalies.goals AS goalie_goals, goalies.assists AS goalie_assists,
       goalies.points AS goalie_points, goalies.wins AS goalie_wins,
       goalies.shutouts AS goalie_shutouts, goalies.saves AS goalie_saves,
       goalies.save_pct, goalies.goals_against_average
FROM analytics.current_rosters AS rosters
LEFT JOIN analytics.official_skater_seasons AS skaters
  ON skaters.player_id = rosters.player_id
 AND skaters.season = %s AND skaters.game_type = 2
LEFT JOIN analytics.official_goalie_seasons AS goalies
  ON goalies.player_id = rosters.player_id
 AND goalies.season = %s AND goalies.game_type = 2
LEFT JOIN analytics.skater_physical_season_totals AS physical_stats
  ON physical_stats.player_id = rosters.player_id
 AND physical_stats.season = %s AND physical_stats.game_type = 2
ORDER BY team_abbrev, last_name NULLS LAST, first_name NULLS LAST, player_id
