-- Publish the dataset's coverage boundaries so downstream consumers can scope
-- or decline a question instead of silently returning a truncated answer.
--
-- The NHL began collecting different facts in different eras. A query for
-- "most hits all time" means "since 2009-10", and a player who retired in 2005
-- returns zero hits, which reads as "never hit anybody" rather than
-- "not tracked". These rows make that distinction queryable.

CREATE SCHEMA analytics;

COMMENT ON SCHEMA analytics IS
    'Read-only derived facts and dataset metadata intended for downstream query engines';

CREATE TABLE analytics.coverage (
    subject      TEXT PRIMARY KEY,
    kind         TEXT NOT NULL,
    first_season INTEGER,
    note         TEXT NOT NULL,
    CONSTRAINT analytics_coverage_kind_check
        CHECK (kind IN ('event_type', 'measure', 'absent')),
    -- An absent subject has no data at all, so it has no first season; every
    -- other subject must name the season its data begins.
    CONSTRAINT analytics_coverage_first_season_check
        CHECK ((kind = 'absent') = (first_season IS NULL))
);

COMMENT ON TABLE analytics.coverage IS
    'First season each subject is available. Rows with kind=absent are not in the schema at all. Query this before answering a question that spans seasons.';
COMMENT ON COLUMN analytics.coverage.subject IS
    'Event type name, derived measure, or the name of an absent concept';
COMMENT ON COLUMN analytics.coverage.kind IS
    'event_type: a row in events.event_type; measure: a derived statistic; absent: not present in the schema';
COMMENT ON COLUMN analytics.coverage.first_season IS
    'Eight-digit season from which the subject is available; NULL when kind=absent';

INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES
    ('goal', 'event_type', 19171918,
     'Complete from the first NHL season.'),
    ('penalty', 'event_type', 19171918,
     'Complete from the first NHL season. Includes bench and goalie penalties, which per-skater official totals exclude.'),
    ('shot-on-goal', 'event_type', 19971998,
     'Shot events begin in 1997-98. Earlier seasons have no shot-on-goal rows.'),
    ('blocked-shot', 'event_type', 20092010, 'Tracked from 2009-10.'),
    ('faceoff', 'event_type', 20092010, 'Tracked from 2009-10.'),
    ('giveaway', 'event_type', 20092010, 'Tracked from 2009-10.'),
    ('hit', 'event_type', 20092010, 'Tracked from 2009-10.'),
    ('missed-shot', 'event_type', 20092010, 'Tracked from 2009-10.'),
    ('takeaway', 'event_type', 20092010, 'Tracked from 2009-10.'),
    ('stoppage', 'event_type', 20092010, 'Tracked from 2009-10.'),
    ('delayed-penalty', 'event_type', 20192020, 'Appears from 2019-20.'),

    ('shots', 'measure', 19971998,
     'The shots table stores every goal as a shot, so before 1997-98 it contains goals only: shot counts equal goal counts. Do not report shot totals before 1997-98.'),
    ('shooting_percentage', 'measure', 19971998,
     'Computes to 100% before 1997-98 because the only stored shots are goals. Do not report it before 1997-98.'),
    ('save_percentage', 'measure', 19971998,
     'Computes to 0% before 1997-98 because every stored shot against is a goal. Do not report it before 1997-98.'),
    ('strength', 'measure', 20052006,
     'Owner-relative manpower. Exact from 2009-10 via situationCode. For 2005-06 through 2008-09 it is recovered from the NHL scoring summary (goals) and archived play-by-play reports (other events); penalty events are excluded there and remain NULL. Unavailable before 2005-06. See events.strength_source for the source of any row.'),
    ('on_ice_skater_counts', 'measure', 20092010,
     'events.home_skater_count, away_skater_count and the goalie flags come from situationCode and are NULL before 2009-10.'),

    ('games_played', 'absent', NULL,
     'No roster or lineup data. Participation can only be approximated as games in which a player recorded a tracked event.'),
    ('time_on_ice', 'absent', NULL, 'No shift or ice-time data.'),
    ('shifts', 'absent', NULL, 'No shift data.'),
    ('plus_minus', 'absent', NULL, 'Requires on-ice player lists, which are not stored.'),
    ('goalie_record', 'absent', NULL,
     'Wins, losses and shutouts need the goalie of record, which requires lineups. A goalie who faced no shots is not represented at all.'),
    ('shootouts', 'absent', NULL,
     'Shootout events are excluded during ingestion, so shootout goals and deciders are not represented.'),
    ('historical_team_names', 'absent', NULL,
     'Teams are stored at franchise level under their current name. Hartford Whalers resolve to Carolina, Atlanta Thrashers to Winnipeg, the original Winnipeg Jets to Arizona.');

-- Detect drift between the published contract and what the tables actually
-- hold. Scans events, so run it deliberately rather than per request.
CREATE VIEW analytics.coverage_observed AS
SELECT
    c.subject,
    c.first_season          AS declared_first_season,
    observed.first_season   AS observed_first_season,
    observed.first_season IS DISTINCT FROM c.first_season AS drifted
FROM analytics.coverage c
JOIN LATERAL (
    SELECT MIN(g.season) AS first_season
    FROM events e
    JOIN games g ON g.game_id = e.game_id
    WHERE e.event_type = c.subject
) AS observed ON TRUE
WHERE c.kind = 'event_type';

COMMENT ON VIEW analytics.coverage_observed IS
    'Compares declared event-type coverage with the seasons actually present. Any row with drifted = true means analytics.coverage needs updating.';
