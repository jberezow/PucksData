SELECT s.game_id, s.source_shift_id, s.type_code, s.player_id,
       s.team_id AS nhl_team_id, t.franchise_id, p.position AS player_position,
       s.period, s.shift_number, s.start_time, s.end_time, s.duration,
       s.start_time_seconds, s.end_time_seconds, s.duration_seconds, s.ingested_at::text
FROM shifts s
LEFT JOIN nhl_team_identities t ON t.nhl_team_id = s.team_id
LEFT JOIN players p ON p.player_id = s.player_id
WHERE s.game_id = ANY($1::bigint[])
ORDER BY s.game_id, s.source_shift_id
