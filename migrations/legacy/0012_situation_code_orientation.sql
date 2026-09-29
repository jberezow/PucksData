-- Correct the orientation of the decoded NHL situationCode.
--
-- The API code is [away_goalie][away_skaters][home_skaters][home_goalie].
-- Earlier ingestion decoded the home and away positions in reverse, so the
-- existing column pairs contain each other's values. Renaming corrects their
-- meaning without rewriting the table; strength is repaired in 0013.

ALTER TABLE events RENAME COLUMN home_skater_count TO swap_skater_count;
ALTER TABLE events RENAME COLUMN away_skater_count TO home_skater_count;
ALTER TABLE events RENAME COLUMN swap_skater_count TO away_skater_count;

ALTER TABLE events RENAME COLUMN home_goalie_present TO swap_goalie_present;
ALTER TABLE events RENAME COLUMN away_goalie_present TO home_goalie_present;
ALTER TABLE events RENAME COLUMN swap_goalie_present TO away_goalie_present;

ALTER TABLE events ADD COLUMN situation_code TEXT;

ALTER TABLE events
    ADD CONSTRAINT events_strength_check
        CHECK (strength IN ('ev', 'pp', 'sh')) NOT VALID,
    ADD CONSTRAINT events_situation_code_check
        CHECK (situation_code ~ '^[01][0-9]{2}[01]$') NOT VALID;

COMMENT ON COLUMN events.situation_code IS
    'NHL situationCode [away_goalie][away_skaters][home_skaters][home_goalie]; historical values before migration 0012 are reconstructed from decoded fields';
COMMENT ON COLUMN events.away_goalie_present IS
    'Away goalie in net (situationCode position 1)';
COMMENT ON COLUMN events.away_skater_count IS
    'Away skaters on ice (situationCode position 2)';
COMMENT ON COLUMN events.home_skater_count IS
    'Home skaters on ice (situationCode position 3)';
COMMENT ON COLUMN events.home_goalie_present IS
    'Home goalie in net (situationCode position 4)';
COMMENT ON COLUMN events.strength IS
    'Manpower from the event owner team perspective using effective skaters: pp, sh, ev, or NULL when the event has no valid owner';
