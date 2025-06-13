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

-- Check if the trigger function is used by other tables before dropping it
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 
        FROM information_schema.triggers 
        WHERE trigger_name = 'set_goals_updated_at' 
        OR action_statement LIKE '%set_current_timestamp_updated_at%'
    ) THEN
        DROP FUNCTION IF EXISTS public.set_current_timestamp_updated_at();
    END IF;
END $$; 