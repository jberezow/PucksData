-- Make the health snapshot affordable.
--
-- observability.dataset_health took 26 seconds cold and season_health 11, so
-- the health page exceeded the reading role's 30-second statement timeout and
-- returned a server error instead of a dashboard.
--
-- Two separate causes.
--
-- First, "does this game have events" was asked as a correlated subquery
-- inside an aggregate FILTER, once per completed game. A plain EXISTS in a
-- WHERE clause can be flattened into a semi-join; one inside FILTER cannot, so
-- it stayed a subplan re-executed for each of the 72,000 completed games,
-- descending an index over 8.9 million rows every time. Joining against the
-- distinct game IDs once replaces those descents with a single pass.
--
-- Second, dataset_health re-derived from base tables what season_health had
-- already computed, including a second per-game pass for the latest event
-- date. Carrying the dates through season_health lets dataset_health be a pure
-- aggregate over 108 rows.
--
-- Set-based, the live computation is roughly twice as fast, which is still
-- seconds: the work is dominated by reading the events index and the
-- goals-without-shots anti-join, and neither shrinks. So the snapshot is
-- materialized. The figures change only when ingestion runs, and PucksData
-- refreshes them at the end of every backfill and of any sync that ingested or
-- failed a game.
--
-- Staleness is bounded and mostly fails safe: a game whose events were just
-- written still reads as a gap until the refresh that immediately follows, so
-- a stale snapshot over-reports gaps rather than hiding them. It is not
-- entirely safe in the other direction, which is why refreshed_at is exposed:
-- a consumer can see how old the figures are, and a refresh that fails warns.

CREATE VIEW observability.season_health_live AS
WITH games_with_events AS (
    -- One pass, about 72,000 groups, instead of a probe per game.
    SELECT game_id FROM events GROUP BY game_id
),
game_coverage AS (
    SELECT
        g.season,
        COUNT(*) AS completed_games,
        COUNT(*) FILTER (WHERE ge.game_id IS NOT NULL) AS games_with_events,
        COUNT(*) FILTER (
            WHERE ge.game_id IS NULL AND bp.game_id IS NOT NULL
        ) AS acknowledged_gap_games,
        MAX(g.game_date) AS latest_completed_game_date,
        MAX(g.game_date) FILTER (WHERE ge.game_id IS NOT NULL) AS latest_event_game_date
    FROM games g
    LEFT JOIN games_with_events ge ON ge.game_id = g.game_id
    -- backfill_progress is keyed by game_id, so this cannot multiply rows.
    LEFT JOIN backfill_progress bp
           ON bp.game_id = g.game_id
          AND bp.status IN ('done', 'skipped')
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
        AND COALESCE(goals.goals_missing_shots, 0) = 0 AS healthy,
    coverage.acknowledged_gap_games,
    coverage.completed_games - coverage.games_with_events
        - coverage.acknowledged_gap_games AS actionable_gap_games,
    coverage.latest_completed_game_date,
    coverage.latest_event_game_date
FROM game_coverage coverage
LEFT JOIN backfill USING (season)
LEFT JOIN goal_consistency goals USING (season);

COMMENT ON VIEW observability.season_health_live IS
    'Live per-season completeness. Correct but expensive; read observability.season_health instead, which materializes this.';

DROP VIEW observability.dataset_health;
DROP VIEW observability.season_health;

CREATE MATERIALIZED VIEW observability.season_health AS
SELECT *, now() AS refreshed_at FROM observability.season_health_live;

CREATE UNIQUE INDEX idx_season_health_season ON observability.season_health(season);

COMMENT ON MATERIALIZED VIEW observability.season_health IS
    'Per-season completeness, materialized. Refreshed by PucksData after every backfill and after any sync that ingested or failed a game; refreshed_at says when.';

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
        COALESCE(BOOL_AND(healthy), TRUE) AS seasons_healthy,
        COALESCE(SUM(acknowledged_gap_games)::bigint, 0) AS acknowledged_gap_games,
        COALESCE(SUM(actionable_gap_games)::bigint, 0) AS actionable_gap_games,
        MAX(latest_completed_game_date) AS latest_completed_game_date,
        MAX(latest_event_game_date) AS latest_event_game_date,
        MAX(refreshed_at) AS refreshed_at
    FROM observability.season_health
)
SELECT
    sync.last_sync_at,
    sync.last_sync_games,
    totals.latest_completed_game_date,
    totals.latest_event_game_date,
    totals.completed_games,
    totals.games_with_events,
    totals.missing_event_games,
    totals.goals_missing_shots,
    totals.backfill_failed,
    totals.backfill_pending,
    totals.backfill_skipped,
    totals.completed_games > 0 AND totals.seasons_healthy AS healthy,
    totals.acknowledged_gap_games,
    totals.actionable_gap_games,
    totals.refreshed_at
FROM (VALUES (1)) singleton(value)
LEFT JOIN sync_state sync ON sync.key = 'singleton'
CROSS JOIN totals;

CREATE VIEW observability.dataset_health_live AS
WITH totals AS (
    SELECT
        COALESCE(SUM(completed_games)::bigint, 0) AS completed_games,
        COALESCE(SUM(games_with_events)::bigint, 0) AS games_with_events,
        COALESCE(SUM(missing_event_games)::bigint, 0) AS missing_event_games,
        COALESCE(SUM(goals_missing_shots)::bigint, 0) AS goals_missing_shots,
        COALESCE(SUM(backfill_failed)::bigint, 0) AS backfill_failed,
        COALESCE(SUM(backfill_pending)::bigint, 0) AS backfill_pending,
        COALESCE(SUM(backfill_skipped)::bigint, 0) AS backfill_skipped,
        COALESCE(BOOL_AND(healthy), TRUE) AS seasons_healthy,
        COALESCE(SUM(acknowledged_gap_games)::bigint, 0) AS acknowledged_gap_games,
        COALESCE(SUM(actionable_gap_games)::bigint, 0) AS actionable_gap_games,
        MAX(latest_completed_game_date) AS latest_completed_game_date,
        MAX(latest_event_game_date) AS latest_event_game_date,
        NULL::timestamptz AS refreshed_at
    FROM observability.season_health_live
)
SELECT
    sync.last_sync_at,
    sync.last_sync_games,
    totals.latest_completed_game_date,
    totals.latest_event_game_date,
    totals.completed_games,
    totals.games_with_events,
    totals.missing_event_games,
    totals.goals_missing_shots,
    totals.backfill_failed,
    totals.backfill_pending,
    totals.backfill_skipped,
    totals.completed_games > 0 AND totals.seasons_healthy AS healthy,
    totals.acknowledged_gap_games,
    totals.actionable_gap_games,
    totals.refreshed_at
FROM (VALUES (1)) singleton(value)
LEFT JOIN sync_state sync ON sync.key = 'singleton'
CROSS JOIN totals;

COMMENT ON VIEW observability.dataset_health_live IS
    'Live dataset-wide completeness. Correct but expensive; refreshed_at is null because nothing is cached. The status command reads this so an operator never sees a stale verdict.';

COMMENT ON COLUMN observability.dataset_health.refreshed_at IS
    'When the materialized season figures were last rebuilt. last_sync_at remains live, so freshness checks are unaffected by this lag.';

-- Dropping the old views dropped their grants with them.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'pucksstudio_read') THEN
        GRANT USAGE ON SCHEMA observability TO pucksstudio_read;
        GRANT SELECT ON observability.season_health,
                        observability.season_health_live,
                        observability.dataset_health,
                        observability.dataset_health_live
              TO pucksstudio_read;
    END IF;
END $$;
