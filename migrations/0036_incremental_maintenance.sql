CREATE TABLE ingestion.schedule_checks (
    game_id BIGINT PRIMARY KEY REFERENCES games(game_id) ON DELETE CASCADE,
    checked_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

-- One pending invalidation per product and source transaction. Different game
-- writers never contend on one global dirty flag. Rolled-back writes leave no
-- invalidation; committed writes survive a killed sync or failed refresh.
CREATE TABLE ingestion.derived_invalidations (
    invalidation_id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    product TEXT NOT NULL,
    source_transaction BIGINT NOT NULL DEFAULT txid_current(),
    UNIQUE(product, source_transaction)
);
INSERT INTO ingestion.derived_invalidations(product) VALUES
    ('analytics.player_event_seasons'),
    ('analytics.skater_physical_season_totals'),
    ('observability.season_health');

CREATE FUNCTION ingestion.invalidate_products(products TEXT[]) RETURNS VOID
LANGUAGE sql AS $$
    INSERT INTO ingestion.derived_invalidations(product)
    SELECT unnest(products) ON CONFLICT(product, source_transaction) DO NOTHING
$$;

CREATE FUNCTION ingestion.invalidate_event_products() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
BEGIN
    -- Transition tables avoid dirtying products for zero-row statements.
    IF TG_OP = 'DELETE' THEN
        IF NOT EXISTS(SELECT 1 FROM old_rows) THEN RETURN NULL; END IF;
    ELSIF TG_OP <> 'TRUNCATE' THEN
        IF NOT EXISTS(SELECT 1 FROM new_rows) THEN RETURN NULL; END IF;
    END IF;
    PERFORM ingestion.invalidate_products(TG_ARGV);
    RETURN NULL;
END $$;

DO $$
DECLARE tab TEXT; products TEXT; operation TEXT; transition TEXT;
BEGIN
    FOREACH tab IN ARRAY ARRAY['events','goals','shots','hits','blocks','penalties','faceoffs'] LOOP
        products := quote_literal('analytics.player_event_seasons');
        IF tab IN ('events','hits','blocks') THEN
            products := products || ',' || quote_literal('analytics.skater_physical_season_totals');
        END IF;
        IF tab IN ('events','goals','shots') THEN
            products := products || ',' || quote_literal('observability.season_health');
        END IF;
        FOREACH operation IN ARRAY ARRAY['INSERT','UPDATE','DELETE','TRUNCATE'] LOOP
            transition := CASE WHEN operation='DELETE' THEN 'REFERENCING OLD TABLE AS old_rows'
                               WHEN operation='TRUNCATE' THEN ''
                               ELSE 'REFERENCING NEW TABLE AS new_rows' END;
            EXECUTE format('CREATE TRIGGER invalidate_%s AFTER %s ON %I %s
                FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products(%s)',
                lower(operation), operation, tab, transition, products);
        END LOOP;
    END LOOP;
END $$;

CREATE FUNCTION ingestion.invalidate_game_products() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'UPDATE' THEN
        IF ROW(OLD.game_id,OLD.season,OLD.game_type) IS DISTINCT FROM
           ROW(NEW.game_id,NEW.season,NEW.game_type) THEN
            PERFORM ingestion.invalidate_products(ARRAY['analytics.player_event_seasons']);
        END IF;
        IF ROW(OLD.game_id,OLD.season,OLD.game_type,OLD.game_date,OLD.game_state) IS NOT DISTINCT FROM
           ROW(NEW.game_id,NEW.season,NEW.game_type,NEW.game_date,NEW.game_state) THEN
            RETURN NULL;
        END IF;
    ELSE
        PERFORM ingestion.invalidate_products(ARRAY['analytics.player_event_seasons']);
    END IF;
    PERFORM ingestion.invalidate_products(ARRAY['observability.season_health']);
    RETURN NULL;
END $$;
CREATE TRIGGER invalidate_games AFTER INSERT OR UPDATE OR DELETE ON games
FOR EACH ROW EXECUTE FUNCTION ingestion.invalidate_game_products();

CREATE FUNCTION ingestion.invalidate_backfill_health() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'UPDATE' THEN
        IF ROW(OLD.game_id,OLD.season,OLD.status) IS NOT DISTINCT FROM ROW(NEW.game_id,NEW.season,NEW.status) THEN
            RETURN NULL;
        END IF;
    END IF;
    PERFORM ingestion.invalidate_products(ARRAY['observability.season_health']);
    RETURN NULL;
END $$;
CREATE TRIGGER invalidate_backfill AFTER INSERT OR UPDATE OR DELETE ON backfill_progress
FOR EACH ROW EXECUTE FUNCTION ingestion.invalidate_backfill_health();
CREATE TRIGGER invalidate_truncate AFTER TRUNCATE ON games
FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons','observability.season_health');
CREATE TRIGGER invalidate_truncate AFTER TRUNCATE ON backfill_progress
FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('observability.season_health');
