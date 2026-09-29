-- Replay adds knowledge now while retaining the identity of the original receipt.
CREATE TABLE ingestion.event_replays (
    replay_id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    game_id BIGINT NOT NULL REFERENCES games(game_id),
    attempt_id BIGINT NOT NULL REFERENCES ingestion.attempts(attempt_id),
    source_snapshot_id BIGINT NOT NULL REFERENCES history.snapshots(snapshot_id),
    source_observation_id BIGINT NOT NULL REFERENCES ingestion.source_observations(observation_id),
    result_snapshot_id BIGINT NOT NULL REFERENCES history.snapshots(snapshot_id),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (attempt_id, game_id)
);
CREATE INDEX ON ingestion.event_replays(result_snapshot_id);
CREATE TRIGGER immutable_event_replays BEFORE UPDATE OR DELETE ON ingestion.event_replays
FOR EACH ROW EXECUTE FUNCTION history.reject_mutation();
COMMENT ON TABLE ingestion.event_replays IS
'Offline child-fact enrichment accepted at recorded_at. Original HTTP receipt time remains on source_observations; facts are not backdated.';

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'pucksdata_ingest') THEN
        GRANT SELECT, INSERT ON ingestion.event_replays TO pucksdata_ingest;
        GRANT USAGE, SELECT ON SEQUENCE ingestion.event_replays_replay_id_seq TO pucksdata_ingest;
    END IF;
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'pucksstudio_read') THEN
        GRANT SELECT ON ingestion.event_replays TO pucksstudio_read;
    END IF;
END $$;
