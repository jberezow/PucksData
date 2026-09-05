-- Answer "which seasons does this player appear in" without walking every event.
--
-- The season selector on a player page asked this by scanning all six typed
-- event tables for the player's fourteen possible roles, joining each match to
-- events and then to games to recover the season, and reducing the result to a
-- distinct list. For a prolific player that read about twenty thousand rows and
-- a hundred thousand buffer pages to return twenty-one, and because each of the
-- six branches hash-joined against games, it scanned the whole games table six
-- times. The cost barely depended on the player: a 1920s skater with almost no
-- events still took three seconds, because the games scans dominate.
--
-- The same work aggregated once for every player is small: about 92,000 rows.
-- Materializing it turns the season selector into a single index lookup.
--
-- This is deliberately event-derived only, and named so. Official season
-- totals live in analytics.official_skater_seasons and
-- analytics.official_goalie_seasons and must stay distinguishable from
-- anything derived from play-by-play.

CREATE MATERIALIZED VIEW analytics.player_event_seasons AS
WITH participants AS (
    SELECT event_id,
           unnest(ARRAY[scorer_player_id, assist1_player_id, assist2_player_id, goalie_id])
               AS player_id
    FROM goals
    UNION ALL
    SELECT event_id, unnest(ARRAY[shooting_player_id, goalie_in_net_id]) FROM shots
    UNION ALL
    SELECT event_id, unnest(ARRAY[hitting_player_id, hittee_player_id]) FROM hits
    UNION ALL
    SELECT event_id, unnest(ARRAY[blocking_player_id, shooting_player_id]) FROM blocks
    UNION ALL
    SELECT event_id, unnest(ARRAY[committed_by_player_id, drawn_by_player_id]) FROM penalties
    UNION ALL
    SELECT event_id, unnest(ARRAY[winning_player_id, losing_player_id]) FROM faceoffs
)
SELECT DISTINCT
    p.player_id,
    g.season,
    g.game_type
FROM participants p
JOIN events e ON e.id = p.event_id
JOIN games  g ON g.game_id = e.game_id
WHERE p.player_id IS NOT NULL;

-- Unique, and therefore refreshable without blocking readers.
CREATE UNIQUE INDEX idx_player_event_seasons_key
    ON analytics.player_event_seasons(player_id, season, game_type);

COMMENT ON MATERIALIZED VIEW analytics.player_event_seasons IS
    'Seasons and game types in which each player appears in any typed event, in any role. Derived from events only; official season totals are separate. Refreshed by PucksData at the end of every backfill and sync, so it lags ingestion by at most one run.';
COMMENT ON COLUMN analytics.player_event_seasons.game_type IS
    '1 = preseason, 2 = regular season, 3 = playoffs';

-- The reading role is created outside migrations and does not exist in the
-- disposable test database, so grant only where it is present.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'pucksstudio_read') THEN
        GRANT SELECT ON analytics.player_event_seasons TO pucksstudio_read;
    END IF;
END $$;
