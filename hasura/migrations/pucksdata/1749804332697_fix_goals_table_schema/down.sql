-- Drop the trigger first
DROP TRIGGER IF EXISTS set_goals_updated_at ON events.goals;

-- Drop the goals table
DROP TABLE IF EXISTS events.goals;

-- Check if any other tables exist in the events schema before dropping it
DO $$
BEGIN
    -- Only drop the events schema if it's empty
    IF NOT EXISTS (
        SELECT 1 
        FROM information_schema.tables 
        WHERE table_schema = 'events'
    ) THEN
        DROP SCHEMA IF EXISTS events;
    END IF;
END $$; 