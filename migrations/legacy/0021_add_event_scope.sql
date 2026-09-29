-- Give event-heavy consumers a way to apply the universal season and game-type
-- scope before joining typed facts. Existing rows are filled separately by
-- scripts/backfill-event-scope.sql so the large rewrite can commit in batches.

ALTER TABLE events
    ADD COLUMN season INTEGER,
    ADD COLUMN game_type SMALLINT,
    ADD COLUMN game_date DATE,
    ADD CONSTRAINT events_scope_not_null_check CHECK (
        season IS NOT NULL AND game_type IS NOT NULL AND game_date IS NOT NULL
    ) NOT VALID;

-- NOT VALID admits the pre-existing rows that still need backfilling while
-- enforcing complete scope on every row inserted or changed from this point.

COMMENT ON COLUMN events.season IS
    'Denormalized from games.season during ingestion for efficient event filtering';
COMMENT ON COLUMN events.game_type IS
    'Denormalized from games.game_type during ingestion for efficient event filtering';
COMMENT ON COLUMN events.game_date IS
    'Denormalized from games.game_date during ingestion for efficient event filtering';
