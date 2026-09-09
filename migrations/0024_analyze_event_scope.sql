-- Populate planner statistics for the newly backfilled columns immediately.
-- Waiting for autovacuum/analyze would make the new index's first plans
-- unnecessarily dependent on deployment timing.

ANALYZE events (season, game_type, event_type);
