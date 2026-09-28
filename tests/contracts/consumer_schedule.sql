-- Consumer e8edeb9fb7415a7a3c72ad017d3b42ca1e779a9f: lineups.py
WITH next_slate AS (
    SELECT game_date
    FROM games
    WHERE season = %s AND game_type = 2
      AND start_time_utc > %s
    ORDER BY start_time_utc, game_id
    LIMIT 1
)
SELECT games.game_date, games.start_time_utc,
       home.abbrev AS home_abbrev, away.abbrev AS away_abbrev
FROM games
JOIN next_slate ON next_slate.game_date = games.game_date
JOIN teams AS home ON home.team_id = games.home_team_id
JOIN teams AS away ON away.team_id = games.away_team_id
WHERE games.season = %s AND games.game_type = 2
  AND games.start_time_utc IS NOT NULL
ORDER BY games.start_time_utc, games.game_id
