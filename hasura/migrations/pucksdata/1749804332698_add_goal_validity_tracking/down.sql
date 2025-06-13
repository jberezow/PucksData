-- Remove goal validity tracking from the events.goals table
DROP INDEX IF EXISTS idx_goals_is_valid;

ALTER TABLE events.goals 
DROP COLUMN IF EXISTS is_valid,
DROP COLUMN IF EXISTS called_back_reason,
DROP COLUMN IF EXISTS challenge_result,
DROP COLUMN IF EXISTS review_timestamp; 