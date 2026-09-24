-- Prospective, transactional history of accepted normalized facts. No backdated
-- baseline is manufactured from today's archive. Existing roster observations
-- remain authoritative for roster membership history.
CREATE SCHEMA history;
CREATE SCHEMA ingestion;

CREATE TABLE history.snapshots (
    snapshot_id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    dataset TEXT NOT NULL,
    entity_key TEXT NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    method_version TEXT NOT NULL DEFAULT 'normalized-v1',
    content_sha256 TEXT NOT NULL,
    payload JSONB NOT NULL,
    UNIQUE (dataset, entity_key, revision)
);
CREATE TABLE history.observations (
    observation_id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    snapshot_id BIGINT NOT NULL REFERENCES history.snapshots(snapshot_id),
    method_version TEXT NOT NULL DEFAULT 'normalized-v1',
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX ON history.observations(snapshot_id, recorded_at);
CREATE INDEX ON history.snapshots(dataset, entity_key, recorded_at DESC);

CREATE FUNCTION history.record(p_dataset TEXT, p_key TEXT, p_payload JSONB, p_method TEXT DEFAULT 'normalized-v1')
RETURNS BIGINT LANGUAGE plpgsql AS $$
DECLARE previous history.snapshots; result BIGINT; source_attempt BIGINT;
BEGIN
    source_attempt := NULLIF(current_setting('pucksdata.attempt_id', true), '')::bigint;
    PERFORM pg_advisory_xact_lock(hashtextextended('history:' || p_dataset || ':' || p_key, 0));
    SELECT * INTO previous FROM history.snapshots
    WHERE dataset = p_dataset AND entity_key = p_key ORDER BY revision DESC LIMIT 1;
    IF previous.snapshot_id IS NOT NULL AND previous.payload = p_payload THEN
        result := previous.snapshot_id;
    ELSE
        INSERT INTO history.snapshots(dataset, entity_key, revision, content_sha256, payload, attempt_id, method_version)
        VALUES (p_dataset, p_key, COALESCE(previous.revision, 0) + 1,
                encode(sha256(convert_to(p_payload::text, 'UTF8')), 'hex'), p_payload, source_attempt, p_method)
        RETURNING snapshot_id INTO result;
    END IF;
    INSERT INTO history.observations(snapshot_id, attempt_id, method_version) VALUES (result, source_attempt, p_method);
    RETURN result;
END $$;

-- Entity snapshots record accepted state even if an older loader writes it.
CREATE FUNCTION history.capture_entity() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE body JSONB; identity TEXT; column_name TEXT;
BEGIN
    body := CASE WHEN TG_OP = 'DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;
    identity := '';
    FOREACH column_name IN ARRAY TG_ARGV LOOP
        identity := identity || CASE WHEN identity = '' THEN '' ELSE ':' END || (body->>column_name);
    END LOOP;
    body := CASE WHEN TG_OP = 'DELETE' THEN jsonb_build_object('deleted', true)
                 ELSE body - ARRAY['observed_at', 'updated_at', 'season_id'] END;
    PERFORM history.record(TG_TABLE_NAME, identity, body);
    RETURN NULL;
END $$;
CREATE TRIGGER history_games AFTER INSERT OR UPDATE OR DELETE ON games
FOR EACH ROW EXECUTE FUNCTION history.capture_entity('game_id');
CREATE TRIGGER history_players AFTER INSERT OR UPDATE OR DELETE ON players
FOR EACH ROW EXECUTE FUNCTION history.capture_entity('player_id');
CREATE TRIGGER history_teams AFTER INSERT OR UPDATE OR DELETE ON teams
FOR EACH ROW EXECUTE FUNCTION history.capture_entity('team_id');

CREATE FUNCTION history.as_of(p_dataset TEXT, cutoff TIMESTAMPTZ)
RETURNS SETOF history.snapshots LANGUAGE sql STABLE AS $$
    SELECT DISTINCT ON (entity_key) * FROM history.snapshots
    WHERE dataset = p_dataset AND recorded_at <= cutoff
    ORDER BY entity_key, revision DESC
$$;
COMMENT ON FUNCTION history.as_of(TEXT, TIMESTAMPTZ) IS
'Accepted normalized states recorded by the cutoff, including deletion markers. Recorded time is local ingestion time, not NHL publication time or transaction commit time. No pre-capture knowledge is implied.';

CREATE TABLE ingestion.attempts (
    attempt_id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    dataset TEXT NOT NULL,
    entity_key TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    finished_at TIMESTAMPTZ,
    outcome TEXT NOT NULL DEFAULT 'running'
        CHECK (outcome IN ('running','complete','partial','failed','unavailable')),
    error_message TEXT,
    CHECK ((outcome = 'running') = (finished_at IS NULL))
);
CREATE INDEX ON ingestion.attempts(dataset, entity_key, attempt_id DESC);
CREATE VIEW observability.ingestion_freshness AS
SELECT DISTINCT ON (dataset, entity_key)
    dataset, entity_key, attempt_id, started_at AS last_attempt_at,
    finished_at, outcome, error_message,
    MAX(finished_at) FILTER (WHERE outcome = 'complete') OVER
        (PARTITION BY dataset, entity_key) AS last_success_at
FROM ingestion.attempts ORDER BY dataset, entity_key, attempt_id DESC;

-- Complete official game snapshots expose disappearing categories/players.
-- Consumers reconcile whole snapshots or consume these revision-scoped changes;
-- an absent value is never silently represented as an authentic zero.
CREATE VIEW analytics.official_player_game_changes AS
WITH revisions AS (
    SELECT *, lag(payload) OVER (PARTITION BY entity_key ORDER BY revision) AS previous
    FROM history.snapshots WHERE dataset = 'official_games'
)
SELECT r.entity_key::bigint AS game_id, r.revision AS game_revision,
       r.snapshot_id, r.recorded_at, changes.*
FROM revisions r CROSS JOIN LATERAL (
    SELECT COALESCE(n.player_id, o.player_id) AS player_id,
           COALESCE(n.stat_code, o.stat_code) AS stat_code,
           o.stat_value AS previous_value, n.stat_value,
           CASE WHEN n.player_id IS NULL THEN 'retracted' ELSE 'set' END AS change_kind
    FROM jsonb_to_recordset(COALESCE(r.payload->'scoring', '[]'))
        AS n(player_id BIGINT, stat_code TEXT, stat_value DOUBLE PRECISION)
    FULL JOIN jsonb_to_recordset(COALESCE(r.previous->'scoring', '[]'))
        AS o(player_id BIGINT, stat_code TEXT, stat_value DOUBLE PRECISION)
    ON n.player_id = o.player_id AND n.stat_code = o.stat_code
    WHERE n.stat_value IS DISTINCT FROM o.stat_value
) changes;

-- Successful HTTP bodies are retained before parsing, including bodies that
-- subsequently fail validation. Content is deduplicated, observations are not.
CREATE TABLE history.source_documents (
    content_sha256 TEXT PRIMARY KEY,
    body TEXT NOT NULL
);
CREATE TABLE ingestion.source_observations (
    observation_id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    attempt_id BIGINT NOT NULL REFERENCES ingestion.attempts(attempt_id),
    url TEXT NOT NULL,
    content_sha256 TEXT NOT NULL REFERENCES history.source_documents(content_sha256),
    observed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX ON ingestion.source_observations(attempt_id);

CREATE FUNCTION history.reject_mutation() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'history is append-only';
END $$;
CREATE TRIGGER immutable_snapshots BEFORE UPDATE OR DELETE ON history.snapshots
FOR EACH ROW EXECUTE FUNCTION history.reject_mutation();
CREATE TRIGGER immutable_observations BEFORE UPDATE OR DELETE ON history.observations
FOR EACH ROW EXECUTE FUNCTION history.reject_mutation();
CREATE TRIGGER immutable_documents BEFORE UPDATE OR DELETE ON history.source_documents
FOR EACH ROW EXECUTE FUNCTION history.reject_mutation();
CREATE TRIGGER immutable_source_observations BEFORE UPDATE OR DELETE ON ingestion.source_observations
FOR EACH ROW EXECUTE FUNCTION history.reject_mutation();

ALTER TABLE history.snapshots ADD COLUMN attempt_id BIGINT REFERENCES ingestion.attempts(attempt_id);
ALTER TABLE history.observations ADD COLUMN attempt_id BIGINT REFERENCES ingestion.attempts(attempt_id);
ALTER TABLE ingestion.attempts ADD COLUMN engine_version TEXT;

CREATE FUNCTION history.next_official_revision(p_game BIGINT, p_player BIGINT, p_group TEXT)
RETURNS INTEGER LANGUAGE sql STABLE AS $$
    SELECT COALESCE(MAX((player->>'source_revision')::integer), 0) + 1
    FROM history.snapshots s
    CROSS JOIN LATERAL jsonb_array_elements(s.payload->p_group) player
    WHERE s.dataset = 'official_games' AND s.entity_key = p_game::text
      AND (player->>'player_id')::bigint = p_player
$$;

CREATE TRIGGER history_seasons AFTER INSERT OR UPDATE OR DELETE ON seasons
FOR EACH ROW EXECUTE FUNCTION history.capture_entity('season_year');
CREATE TRIGGER history_team_identities AFTER INSERT OR UPDATE OR DELETE ON nhl_team_identities
FOR EACH ROW EXECUTE FUNCTION history.capture_entity('nhl_team_id');
CREATE TRIGGER history_skater_seasons AFTER INSERT OR UPDATE OR DELETE ON analytics.official_skater_seasons
FOR EACH ROW EXECUTE FUNCTION history.capture_entity('player_id', 'season', 'game_type');
CREATE TRIGGER history_goalie_seasons AFTER INSERT OR UPDATE OR DELETE ON analytics.official_goalie_seasons
FOR EACH ROW EXECUTE FUNCTION history.capture_entity('player_id', 'season', 'game_type');

CREATE TABLE ingestion.diagnostics (
    diagnostic_id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    attempt_id BIGINT NOT NULL REFERENCES ingestion.attempts(attempt_id),
    code TEXT NOT NULL,
    occurrence_count BIGINT NOT NULL CHECK (occurrence_count > 0),
    examples TEXT[] NOT NULL
);
CREATE INDEX ON ingestion.diagnostics(attempt_id);
