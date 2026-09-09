\set ON_ERROR_STOP on
\set batch_size 1000

-- Run with psql after migration 0021 and before 0022. \gexec executes every
-- generated UPDATE as a separate statement, so each batch commits separately
-- under psql's default autocommit mode. Re-running is safe: only incomplete
-- rows are selected and updated.
WITH incomplete_games AS (
    SELECT DISTINCT game_id
    FROM events
    WHERE season IS NULL OR game_type IS NULL OR game_date IS NULL
), numbered_games AS (
    SELECT
        game_id,
        (row_number() OVER (ORDER BY game_id) - 1) / :batch_size AS batch_number
    FROM incomplete_games
), batches AS (
    SELECT min(game_id) AS first_game_id, max(game_id) AS last_game_id
    FROM numbered_games
    GROUP BY batch_number
    ORDER BY batch_number
)
SELECT format(
    'UPDATE events AS e
        SET season = g.season,
            game_type = g.game_type,
            game_date = g.game_date
       FROM games AS g
      WHERE g.game_id = e.game_id
        AND e.game_id BETWEEN %s AND %s
        AND (e.season IS NULL OR e.game_type IS NULL OR e.game_date IS NULL);',
    first_game_id,
    last_game_id
)
FROM batches
\gexec

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM events
        WHERE season IS NULL OR game_type IS NULL OR game_date IS NULL
    ) THEN
        RAISE EXCEPTION
            'event scope backfill left incomplete rows; verify every event references games';
    END IF;
END $$;
