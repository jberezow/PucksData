-- Apply only after scripts/backfill-event-scope.sql has completed. Keeping this
-- separate from 0021 permits a controlled, restartable production backfill.

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM events
        WHERE season IS NULL OR game_type IS NULL OR game_date IS NULL
    ) THEN
        RAISE EXCEPTION
            'events scope backfill is incomplete; run scripts/backfill-event-scope.sql';
    END IF;
END $$;

ALTER TABLE events
    VALIDATE CONSTRAINT events_scope_not_null_check;

-- PostgreSQL can use the validated check as proof when setting the column
-- flags, avoiding another full-table validation scan.
ALTER TABLE events
    ALTER COLUMN season SET NOT NULL,
    ALTER COLUMN game_type SET NOT NULL,
    ALTER COLUMN game_date SET NOT NULL,
    DROP CONSTRAINT events_scope_not_null_check;
