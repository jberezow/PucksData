-- Events are committed with accepted official snapshots, never with partial reports.
CREATE TABLE ingestion.game_webhook_outbox (
    event_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id BIGINT NOT NULL,
    revision BIGINT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    available_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    attempts INTEGER NOT NULL DEFAULT 0,
    delivered_at TIMESTAMPTZ,
    last_error TEXT,
    UNIQUE (game_id, revision)
);
CREATE INDEX game_webhook_pending ON ingestion.game_webhook_outbox(available_at)
    WHERE delivered_at IS NULL;

CREATE FUNCTION ingestion.enqueue_game_webhook() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE event UUID := gen_random_uuid(); metadata RECORD;
BEGIN
    IF NEW.dataset <> 'official_games' OR NEW.method_version <> 'normalized-v1'
       OR jsonb_array_length(NEW.payload->'skaters') = 0
       OR jsonb_array_length(NEW.payload->'goalies') = 0 THEN
        RETURN NULL;
    END IF;
    SELECT season, game_date INTO metadata FROM games
    WHERE game_id = NEW.entity_key::bigint AND game_state IN ('OFF','OVER','FINAL');
    IF NOT FOUND THEN RETURN NULL; END IF;
    INSERT INTO ingestion.game_webhook_outbox(event_id, game_id, revision, payload)
    VALUES (event, NEW.entity_key::bigint, NEW.revision, jsonb_build_object(
        'schema_version', 1, 'event_id', event, 'type', 'game.data.updated',
        'game_id', NEW.entity_key::bigint, 'revision', NEW.revision,
        'season', metadata.season, 'game_date', metadata.game_date));
    RETURN NULL;
END $$;
CREATE TRIGGER game_webhook AFTER INSERT ON history.snapshots
FOR EACH ROW WHEN (NEW.dataset = 'official_games')
EXECUTE FUNCTION ingestion.enqueue_game_webhook();

-- Supported reader contract for validating delivered revisions.
CREATE VIEW analytics.official_game_revisions AS
SELECT DISTINCT ON (entity_key) entity_key::bigint AS game_id, revision, recorded_at
FROM history.snapshots WHERE dataset = 'official_games' AND method_version = 'normalized-v1'
  AND jsonb_array_length(payload->'skaters') > 0 AND jsonb_array_length(payload->'goalies') > 0
ORDER BY entity_key, revision DESC;

-- Preserve the existing writer/reader boundary without hard-coding consumers.
DO $$ DECLARE permission RECORD; BEGIN
    FOR permission IN SELECT DISTINCT acl.grantee FROM pg_class c,
        LATERAL aclexplode(COALESCE(c.relacl, acldefault('r',c.relowner))) acl
        WHERE c.oid='analytics.official_skater_games'::regclass
          AND acl.privilege_type='INSERT' AND acl.grantee<>0
    LOOP
        EXECUTE format('GRANT SELECT,INSERT,UPDATE ON ingestion.game_webhook_outbox TO %I',
                       pg_get_userbyid(permission.grantee));
    END LOOP;
    FOR permission IN SELECT DISTINCT acl.grantee FROM pg_class c,
        LATERAL aclexplode(COALESCE(c.relacl, acldefault('r',c.relowner))) acl
        WHERE c.oid='analytics.official_player_game_stats'::regclass
          AND acl.privilege_type='SELECT' AND acl.grantee<>0
    LOOP
        EXECUTE format('GRANT SELECT ON analytics.official_game_revisions TO %I',
                       pg_get_userbyid(permission.grantee));
    END LOOP;
END $$;
