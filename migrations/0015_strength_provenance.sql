-- Represent unavailable historical manpower data as unknown and record the
-- NHL source used for each strength classification.

ALTER TABLE events
    ALTER COLUMN away_goalie_present DROP NOT NULL,
    ALTER COLUMN away_goalie_present DROP DEFAULT,
    ALTER COLUMN away_skater_count DROP NOT NULL,
    ALTER COLUMN away_skater_count DROP DEFAULT,
    ALTER COLUMN home_skater_count DROP NOT NULL,
    ALTER COLUMN home_skater_count DROP DEFAULT,
    ALTER COLUMN home_goalie_present DROP NOT NULL,
    ALTER COLUMN home_goalie_present DROP DEFAULT;

-- Existing data from 2009-10 onward came from situationCode. A metadata-only
-- default avoids rewriting those rows; new writers always provide a source.
ALTER TABLE events
    ADD COLUMN strength_source TEXT NOT NULL DEFAULT 'situation_code';

ALTER TABLE events
    ALTER COLUMN strength_source SET DEFAULT 'unavailable',
    ADD CONSTRAINT events_strength_source_check
        CHECK (strength_source IN (
            'situation_code',
            'scoring_summary',
            'html_report',
            'unavailable'
        )) NOT VALID;

-- Before 2009-10 the JSON feed omits situationCode. These values were legacy
-- 5-on-5 fallbacks, not observations from the NHL feed.
UPDATE events AS e
SET away_goalie_present = NULL,
    away_skater_count = NULL,
    home_skater_count = NULL,
    home_goalie_present = NULL,
    strength = NULL,
    situation_code = NULL,
    strength_source = 'unavailable'
FROM games AS g
WHERE g.game_id = e.game_id
  AND g.season < 20092010;

ALTER TABLE events VALIDATE CONSTRAINT events_strength_source_check;

COMMENT ON COLUMN events.strength_source IS
    'NHL source for strength: situation_code, scoring_summary, html_report, or unavailable';
COMMENT ON COLUMN events.away_goalie_present IS
    'Away goalie in net; NULL when the source does not expose on-ice state';
COMMENT ON COLUMN events.away_skater_count IS
    'Away skaters on ice; NULL when the source does not expose on-ice state';
COMMENT ON COLUMN events.home_skater_count IS
    'Home skaters on ice; NULL when the source does not expose on-ice state';
COMMENT ON COLUMN events.home_goalie_present IS
    'Home goalie in net; NULL when the source does not expose on-ice state';
