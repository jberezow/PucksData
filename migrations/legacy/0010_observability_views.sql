CREATE SCHEMA observability;

COMMENT ON SCHEMA observability IS
    'Read-only derived metadata describing dataset completeness and consistency';

CREATE VIEW observability.season_health AS
WITH game_coverage AS (
    SELECT
        g.season,
        COUNT(*) AS completed_games,
        COUNT(*) FILTER (
            WHERE EXISTS (SELECT 1 FROM events e WHERE e.game_id = g.game_id)
        ) AS games_with_events
    FROM games g
    WHERE g.game_state IN ('OFF', 'OVER', 'FINAL')
      AND g.game_type != 1
    GROUP BY g.season
),
backfill AS (
    SELECT
        season,
        COUNT(*) FILTER (WHERE status = 'done') AS backfill_done,
        COUNT(*) FILTER (WHERE status = 'failed') AS backfill_failed,
        COUNT(*) FILTER (WHERE status = 'skipped') AS backfill_skipped,
        COUNT(*) FILTER (WHERE status = 'pending') AS backfill_pending
    FROM backfill_progress
    GROUP BY season
),
goal_consistency AS (
    SELECT
        g.season,
        COUNT(*) AS goals_missing_shots
    FROM goals goal
    JOIN events e ON e.id = goal.event_id
    JOIN games g ON g.game_id = e.game_id
    WHERE NOT EXISTS (SELECT 1 FROM shots s WHERE s.event_id = goal.event_id)
    GROUP BY g.season
)
SELECT
    coverage.season,
    coverage.completed_games,
    coverage.games_with_events,
    coverage.completed_games - coverage.games_with_events AS missing_event_games,
    CASE
        WHEN coverage.completed_games = 0 THEN 100.0
        ELSE coverage.games_with_events::double precision
             / coverage.completed_games::double precision * 100.0
    END AS event_coverage_pct,
    COALESCE(goals.goals_missing_shots, 0) AS goals_missing_shots,
    COALESCE(backfill.backfill_done, 0) AS backfill_done,
    COALESCE(backfill.backfill_failed, 0) AS backfill_failed,
    COALESCE(backfill.backfill_skipped, 0) AS backfill_skipped,
    COALESCE(backfill.backfill_pending, 0) AS backfill_pending,
    coverage.games_with_events = coverage.completed_games
        AND COALESCE(goals.goals_missing_shots, 0) = 0 AS healthy
FROM game_coverage coverage
LEFT JOIN backfill USING (season)
LEFT JOIN goal_consistency goals USING (season);

CREATE VIEW observability.dataset_health AS
WITH totals AS (
    SELECT
        COALESCE(SUM(completed_games)::bigint, 0) AS completed_games,
        COALESCE(SUM(games_with_events)::bigint, 0) AS games_with_events,
        COALESCE(SUM(missing_event_games)::bigint, 0) AS missing_event_games,
        COALESCE(SUM(goals_missing_shots)::bigint, 0) AS goals_missing_shots,
        COALESCE(SUM(backfill_failed)::bigint, 0) AS backfill_failed,
        COALESCE(SUM(backfill_pending)::bigint, 0) AS backfill_pending,
        COALESCE(SUM(backfill_skipped)::bigint, 0) AS backfill_skipped,
        COALESCE(BOOL_AND(healthy), TRUE) AS seasons_healthy
    FROM observability.season_health
),
latest_games AS (
    SELECT
        MAX(game_date) AS latest_completed_game_date,
        MAX(game_date) FILTER (
            WHERE EXISTS (SELECT 1 FROM events e WHERE e.game_id = games.game_id)
        ) AS latest_event_game_date
    FROM games
    WHERE game_state IN ('OFF', 'OVER', 'FINAL')
      AND game_type != 1
)
SELECT
    sync.last_sync_at,
    sync.last_sync_games,
    latest_games.latest_completed_game_date,
    latest_games.latest_event_game_date,
    totals.completed_games,
    totals.games_with_events,
    totals.missing_event_games,
    totals.goals_missing_shots,
    totals.backfill_failed,
    totals.backfill_pending,
    totals.backfill_skipped,
    totals.completed_games > 0 AND totals.seasons_healthy AS healthy
FROM (VALUES (1)) singleton(value)
LEFT JOIN sync_state sync ON sync.key = 'singleton'
CROSS JOIN totals
CROSS JOIN latest_games;
