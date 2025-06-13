-- Add goal validity tracking to the events.goals table
ALTER TABLE events.goals 
ADD COLUMN is_valid BOOLEAN NOT NULL DEFAULT true,
ADD COLUMN called_back_reason TEXT,
ADD COLUMN challenge_result TEXT,
ADD COLUMN review_timestamp TIMESTAMP;

-- Add index for quick queries on valid goals
CREATE INDEX idx_goals_is_valid ON events.goals(is_valid);

-- Add comment to explain the new columns
COMMENT ON COLUMN events.goals.is_valid IS 'Whether the goal is valid and counts toward the final score (false if called back after review)';
COMMENT ON COLUMN events.goals.called_back_reason IS 'Reason for goal being called back (e.g., "offside", "goaltender interference", "high stick")';
COMMENT ON COLUMN events.goals.challenge_result IS 'Result of any coach challenge on this goal (e.g., "successful challenge", "failed challenge")';
COMMENT ON COLUMN events.goals.review_timestamp IS 'When the goal was reviewed/challenged (if applicable)'; 