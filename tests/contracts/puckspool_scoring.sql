-- PucksPool e8edeb9fb7415a7a3c72ad017d3b42ca1e779a9f: scoring.py
WITH eligible_games AS MATERIALIZED (
    SELECT game_id, season, game_type, start_time_utc AS scheduled_start,
           game_state
    FROM games
    WHERE season = ANY(%s)
      AND game_date BETWEEN %s AND %s
      AND game_state IN ('OFF', 'OVER', 'FINAL')
      AND start_time_utc IS NOT NULL
), current_facts AS MATERIALIZED (
    SELECT stats.*
    FROM analytics.official_player_game_stats AS stats
    JOIN eligible_games USING (game_id)
), latest_changes AS (
    SELECT DISTINCT ON (changes.game_id, changes.player_id, changes.stat_code)
           changes.*
    FROM analytics.official_player_game_changes AS changes
    JOIN eligible_games USING (game_id)
    ORDER BY changes.game_id, changes.player_id, changes.stat_code,
             changes.game_revision DESC
), facts AS (
    SELECT game_id, player_id, stat_code, stat_value,
           source_revision::text AS source_revision,
           updated_at AS source_updated_at
    FROM current_facts
    UNION ALL
    SELECT changes.game_id, changes.player_id, changes.stat_code,
           NULL AS stat_value,
           'history:' || changes.game_revision::text AS source_revision,
           changes.recorded_at AS source_updated_at
    FROM latest_changes AS changes
    WHERE changes.change_kind = 'retracted'
      AND NOT EXISTS (
          SELECT 1 FROM current_facts AS current
          WHERE current.game_id = changes.game_id
            AND current.player_id = changes.player_id
            AND current.stat_code = changes.stat_code
      )
)
SELECT games.*, facts.player_id, facts.stat_code, facts.stat_value,
       facts.source_revision, facts.source_updated_at
FROM eligible_games AS games
JOIN facts USING (game_id)
ORDER BY games.game_id, facts.player_id, facts.stat_code
