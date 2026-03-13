ALTER TABLE backfill_progress
    ADD COLUMN IF NOT EXISTS error_message TEXT;
