-- Derived availability only: no canonical event/shift changes or backfill.
CREATE VIEW observability.shift_game_coverage AS
SELECT g.game_id, g.season, g.game_type,
       g.season >= 20102011 AS eligible,
       g.shift_rows,
       f.status AS latest_fetch_status, f.attempted_at,
       CASE
           WHEN g.season < 20102011 THEN 'unsupported'
           WHEN g.shift_rows > 0 THEN 'loaded'
           WHEN f.status = 'unavailable' THEN 'unavailable'
           WHEN f.status = 'failed' THEN 'failed'
           ELSE 'no_stored_shifts'
       END AS availability
FROM (
    -- Aggregate the eligible game/shift join once, rather than executing a
    -- correlated COUNT for each game. Keep season in this relation so a season
    -- predicate can reach games before aggregation instead of counting all shifts.
    SELECT g.game_id, g.season, g.game_type, count(s.game_id) AS shift_rows
    FROM games g
    LEFT JOIN shifts s ON s.game_id = g.game_id
    WHERE g.game_type IN (2, 3) AND g.game_state IN ('OFF', 'OVER', 'FINAL')
    GROUP BY g.game_id, g.season, g.game_type
) g
LEFT JOIN shift_fetch_status f USING (game_id);

CREATE VIEW observability.shift_season_coverage AS
SELECT season, game_type,
       count(*) FILTER (WHERE eligible) AS eligible_games,
       count(*) FILTER (WHERE NOT eligible) AS unsupported_games,
       count(*) FILTER (WHERE availability = 'loaded') AS loaded_games,
       count(*) FILTER (WHERE availability = 'unavailable') AS unavailable_games,
       count(*) FILTER (WHERE availability = 'failed') AS failed_games,
       count(*) FILTER (WHERE availability = 'no_stored_shifts') AS missing_games,
       sum(shift_rows) AS shift_rows,
       CASE WHEN count(*) FILTER (WHERE eligible) = 0 THEN NULL
            ELSE count(*) FILTER (WHERE availability = 'loaded')::double precision
                 / count(*) FILTER (WHERE eligible) END AS loaded_fraction
FROM observability.shift_game_coverage
GROUP BY season, game_type;

COMMENT ON VIEW observability.shift_season_coverage IS
    'Shift availability, not reconstruction correctness. Pre-2010 seasons are unsupported, not unhealthy. Run shifts audit for validation, TOI and event-level reliability; existing event health is unchanged.';
COMMENT ON VIEW observability.shift_game_coverage IS
    'Stored snapshots take precedence over latest fetch outcomes. Absence of a fetch record does not establish that a game was never attempted. Counts aggregate the game/shift join; season filters reach games before aggregation. Games without shifts retain zero counts.';

INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES
    ('event_on_ice_reconstruction', 'measure', 20102011,
     'Derived on demand by shifts reconstruct/audit from NHL events and canonical intervals, not stored on events. Exact-second boundaries retain definite/possible identities. Consult reconstruction status, validation flags, TOI reconciliation and situationCode agreement; availability does not imply complete or independently verified lineups. Shootouts are unsupported.');

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'pucksstudio_read') THEN
        GRANT USAGE ON SCHEMA observability TO pucksstudio_read;
        GRANT SELECT ON observability.shift_game_coverage, observability.shift_season_coverage TO pucksstudio_read;
    END IF;
END $$;
