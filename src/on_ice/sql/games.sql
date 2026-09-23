SELECT g.game_id, g.season, g.game_type, g.game_date::text, g.game_state,
       g.home_team_id, g.away_team_id, f.status AS fetch_status, f.attempted_at::text
FROM games g LEFT JOIN shift_fetch_status f USING (game_id)
WHERE ($1::integer IS NULL OR g.season = $1)
  AND ($2::bigint IS NULL OR g.game_id = $2)
  AND g.game_type IN (2, 3) AND g.game_state IN ('OFF', 'OVER', 'FINAL')
ORDER BY g.game_id
