-- migrations/0005_sync_state.sql
-- Operational metadata store for sync daemon (SCHEMA-15, QUAL-SYNC-02).
-- Single row keyed by 'singleton'. Informational only — not the sync gate.
CREATE TABLE sync_state (
    key              TEXT PRIMARY KEY,
    last_sync_at     TIMESTAMPTZ,
    last_sync_games  INTEGER,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
