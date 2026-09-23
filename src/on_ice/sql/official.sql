SELECT game_id, player_id, 'skater'::text AS player_type, team_abbrev, position_code,
       time_on_ice_seconds::bigint, source_revision, source_observed_at::text
FROM analytics.official_skater_games WHERE game_id = ANY($1::bigint[])
UNION ALL
SELECT game_id, player_id, 'goalie'::text AS player_type, team_abbrev, 'G'::text AS position_code,
       time_on_ice_seconds, source_revision, source_observed_at::text
FROM analytics.official_goalie_games WHERE game_id = ANY($1::bigint[])
ORDER BY game_id, player_id, player_type
