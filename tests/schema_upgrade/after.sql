\set ON_ERROR_STOP on
SELECT set_config('schema_test.reader', :'reader_role', false);
SELECT set_config('schema_test.column_reader', :'column_role', false);
SELECT set_config('schema_test.default_reader', :'default_role', false);
SELECT set_config('schema_test.writer', :'writer_role', false);
DO $$
BEGIN
    IF has_schema_privilege(current_setting('schema_test.reader'),'ingestion','USAGE') THEN
        RAISE EXCEPTION 'fixture reader must have no access to ingestion';
    END IF;
    IF NOT has_table_privilege(current_setting('schema_test.reader'),'public.sync_state','SELECT WITH GRANT OPTION') THEN
        RAISE EXCEPTION 'lost legacy SELECT grant option';
    END IF;
    IF has_table_privilege(current_setting('schema_test.default_reader'),'public.sync_state','SELECT')
       OR has_table_privilege(current_setting('schema_test.default_reader'),'public.sync_state','INSERT') THEN
        RAISE EXCEPTION 'default privileges widened compatibility view access';
    END IF;
    IF has_table_privilege(current_setting('schema_test.column_reader'),'public.shift_fetch_status','SELECT')
       OR NOT has_column_privilege(current_setting('schema_test.column_reader'),'public.shift_fetch_status','status','SELECT') THEN
        RAISE EXCEPTION 'column-only grants were widened or lost';
    END IF;
    IF NOT has_table_privilege(current_setting('schema_test.default_reader'),'public.backfill_progress','SELECT') THEN
        RAISE EXCEPTION 'prior PUBLIC SELECT grant was lost';
    END IF;
    IF has_table_privilege(current_setting('schema_test.writer'),'public.sync_state','INSERT') THEN
        RAISE EXCEPTION 'legacy writer unexpectedly has INSERT on compatibility view';
    END IF;
END $$;

SET ROLE :"reader_role";
SELECT game_id,season,status,error_message FROM public.backfill_progress;
SELECT key,last_sync_at,last_sync_games,updated_at FROM public.sync_state;
SELECT game_id,status,attempted_at FROM public.shift_fetch_status;
-- Existing health views must still follow their moved base tables.
SELECT * FROM observability.dataset_health_live;
SELECT * FROM observability.shift_game_coverage;
RESET ROLE;
SET ROLE :"column_role";
SELECT game_id,status FROM public.shift_fetch_status;
RESET ROLE;

BEGIN;
SET LOCAL ROLE :"writer_role";
INSERT INTO ingestion.backfill_progress(game_id,season,status) VALUES(9968000001,20252026,'failed')
ON CONFLICT(game_id) DO UPDATE SET status=EXCLUDED.status;
INSERT INTO ingestion.sync_state(key,last_sync_games) VALUES('singleton',2)
ON CONFLICT(key) DO UPDATE SET last_sync_games=EXCLUDED.last_sync_games;
INSERT INTO ingestion.shift_fetch_status(game_id,status) VALUES(9968000001,'unavailable')
ON CONFLICT(game_id) DO UPDATE SET status=EXCLUDED.status;
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM ingestion.derived_invalidations
                   WHERE product='observability.season_health' AND source_transaction=txid_current()) THEN
        RAISE EXCEPTION 'moved backfill table lost its invalidation trigger';
    END IF;
END $$;
ROLLBACK;

BEGIN;
-- No-op writes across the capture-function upgrade retain their revision.
UPDATE public.teams SET full_name=full_name WHERE team_id=99681;
DO $$
BEGIN
    IF (SELECT count(*) FROM history.snapshots WHERE dataset='teams' AND entity_key='99681') <> 1
       OR (SELECT count(*) FROM history.observations o JOIN history.snapshots s USING(snapshot_id)
           WHERE s.dataset='teams' AND s.entity_key='99681') <> 2 THEN
        RAISE EXCEPTION 'history identity or payload changed during migration';
    END IF;
END $$;
UPDATE public.teams SET full_name='Upgrade Revised' WHERE team_id=99681;
DO $$
BEGIN
    IF (SELECT max(revision) FROM history.snapshots WHERE dataset='teams' AND entity_key='99681') <> 2 THEN
        RAISE EXCEPTION 'history revisions did not continue after upgrade';
    END IF;
END $$;
DELETE FROM public.games WHERE game_id=9968000001;
DO $$
BEGIN
    IF EXISTS(SELECT 1 FROM ingestion.shift_fetch_status WHERE game_id=9968000001) THEN
        RAISE EXCEPTION 'moved shift status lost its cascade foreign key';
    END IF;
END $$;
ROLLBACK;
