-- Make the drift check affordable, and record eras whose source data is
-- incomplete.
--
-- The original coverage_observed correlated a LATERAL subquery per coverage
-- row, so it scanned the 8.9-million-row events table once per event type:
-- eleven scans, 207 seconds, well past the 30-second statement timeout the
-- read-only application role runs under. Aggregating once and joining the
-- result returns the same eleven rows in 18 seconds.

CREATE OR REPLACE VIEW analytics.coverage_observed AS
WITH observed AS (
    SELECT e.event_type, MIN(g.season) AS first_season
    FROM events e
    JOIN games g ON g.game_id = e.game_id
    GROUP BY e.event_type
)
SELECT
    c.subject,
    c.first_season          AS declared_first_season,
    observed.first_season   AS observed_first_season,
    observed.first_season IS DISTINCT FROM c.first_season AS drifted
FROM analytics.coverage c
LEFT JOIN observed ON observed.event_type = c.subject
WHERE c.kind = 'event_type';

COMMENT ON VIEW analytics.coverage_observed IS
    'Compares declared event-type coverage with the seasons actually present. Any row with drifted = true means analytics.coverage needs updating. Scans the events table once; expect tens of seconds.';

-- Some eras are complete as far as ingestion goes but incomplete at source.
-- A caveat names that so a consumer can prefer the official season totals
-- without mistaking the gap for an ingestion failure.
ALTER TABLE analytics.coverage
    DROP CONSTRAINT analytics_coverage_kind_check;

ALTER TABLE analytics.coverage
    ADD CONSTRAINT analytics_coverage_kind_check
        CHECK (kind IN ('event_type', 'measure', 'absent', 'caveat'));

COMMENT ON COLUMN analytics.coverage.kind IS
    'event_type: a row in events.event_type; measure: a derived statistic; absent: not present in the schema; caveat: data present but known incomplete at source';

INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES
    ('play_by_play_2009_10', 'caveat', 20092010,
     'The NHL''s own 2009-10 play-by-play feed is incomplete. Ingestion mirrors it faithfully, but 17 games are missing 23 goals against the official box scores, and some games stop after one or two periods. Prefer analytics.official_skater_seasons and analytics.official_goalie_seasons for 2009-10 season totals.');
