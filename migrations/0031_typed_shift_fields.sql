-- Preserve meaningful source fields before discarding duplicated source JSON.
-- SQLx runs this migration transactionally: a failed conversion leaves the
-- old table and source objects intact. Pause shift writers during deployment.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM public.shifts
        WHERE COALESCE(jsonb_typeof(source_data->'eventNumber'), 'null') NOT IN ('null', 'number')
           OR COALESCE(jsonb_typeof(source_data->'detailCode'), 'null') NOT IN ('null', 'number')
           OR COALESCE(jsonb_typeof(source_data->'eventDescription'), 'null') NOT IN ('null', 'string')
           OR COALESCE(jsonb_typeof(source_data->'eventDetails'), 'null') NOT IN ('null', 'string')
    ) THEN
        RAISE EXCEPTION 'Unexpected shift metadata type; source_data retained';
    END IF;
END $$;

ALTER TABLE public.shifts
    ADD COLUMN event_number INTEGER,
    ADD COLUMN detail_code INTEGER,
    ADD COLUMN event_description TEXT,
    ADD COLUMN event_details TEXT;

UPDATE public.shifts
SET event_number = (source_data->>'eventNumber')::INTEGER,
    detail_code = (source_data->>'detailCode')::INTEGER,
    event_description = source_data->>'eventDescription',
    event_details = source_data->>'eventDetails';

ALTER TABLE public.shifts DROP COLUMN source_data;

COMMENT ON COLUMN public.shifts.event_number IS
    'Source eventNumber, not assumed to be an events table foreign key or a unique chronological ordering.';
COMMENT ON COLUMN public.shifts.detail_code IS
    'Source detailCode retained without interpreting undocumented code semantics.';
COMMENT ON COLUMN public.shifts.event_description IS
    'Optional source eventDescription for a shift row.';
COMMENT ON COLUMN public.shifts.event_details IS
    'Optional source eventDetails for a shift row.';
