-- Hits and blocked shots are available as per-game events but are absent from
-- the NHL season-total endpoint. Materialize their stable season rollup so
-- downstream draft snapshots do not aggregate the event archive on demand.
CREATE MATERIALIZED VIEW analytics.skater_physical_season_totals AS
WITH physical_stats AS (
    SELECT hits.hitting_player_id AS player_id,
           events.season,
           events.game_type,
           COUNT(*) AS hits,
           0::bigint AS blocks
    FROM hits
    JOIN events ON events.id = hits.event_id
    WHERE hits.hitting_player_id IS NOT NULL
    GROUP BY hits.hitting_player_id, events.season, events.game_type

    UNION ALL

    SELECT blocks.blocking_player_id AS player_id,
           events.season,
           events.game_type,
           0::bigint AS hits,
           COUNT(*) AS blocks
    FROM blocks
    JOIN events ON events.id = blocks.event_id
    WHERE blocks.blocking_player_id IS NOT NULL
    GROUP BY blocks.blocking_player_id, events.season, events.game_type
)
SELECT player_id,
       season,
       game_type,
       SUM(hits)::integer AS hits,
       SUM(blocks)::integer AS blocks
FROM physical_stats
GROUP BY player_id, season, game_type;

CREATE UNIQUE INDEX idx_skater_physical_season_totals_key
    ON analytics.skater_physical_season_totals(player_id, season, game_type);

CREATE INDEX idx_skater_physical_season_totals_season
    ON analytics.skater_physical_season_totals(season, game_type, player_id);

COMMENT ON MATERIALIZED VIEW analytics.skater_physical_season_totals IS
    'Event-derived skater hit and blocked-shot totals by season. Refreshed after event ingestion; coverage begins in 2009-10.';

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'pucksstudio_read') THEN
        GRANT SELECT ON analytics.skater_physical_season_totals TO pucksstudio_read;
    END IF;
END $$;
