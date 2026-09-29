-- Successful historical player audits advance only after accepted player and
-- roster writes. Interrupted runs retry the same seasons.
CREATE TABLE ingestion.player_audits (
    season INTEGER PRIMARY KEY,
    completed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
