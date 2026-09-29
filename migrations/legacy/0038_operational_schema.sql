-- Pause old writers before applying: legacy names below preserve reads, not
-- INSERT ... ON CONFLICT. Existing rows, OIDs, constraints, triggers, indexes
-- and base-table ACLs remain attached to the moved tables.
ALTER TABLE public.backfill_progress SET SCHEMA ingestion;
ALTER TABLE public.sync_state SET SCHEMA ingestion;
ALTER TABLE public.shift_fetch_status SET SCHEMA ingestion;

-- OFFSET 0 makes these compatibility views non-updatable. New writers must
-- address the canonical ingestion tables explicitly.
CREATE VIEW public.backfill_progress AS
SELECT game_id, season, status, updated_at, error_message
FROM ingestion.backfill_progress OFFSET 0;
CREATE VIEW public.sync_state AS
SELECT key, last_sync_at, last_sync_games, updated_at
FROM ingestion.sync_state OFFSET 0;
CREATE VIEW public.shift_fetch_status AS
SELECT game_id, status, attempted_at
FROM ingestion.shift_fetch_status OFFSET 0;

COMMENT ON VIEW public.backfill_progress IS 'Legacy read compatibility. Write ingestion.backfill_progress; do not use this view for upserts.';
COMMENT ON VIEW public.sync_state IS 'Legacy read compatibility. Write ingestion.sync_state; do not use this view for upserts.';
COMMENT ON VIEW public.shift_fetch_status IS 'Legacy read compatibility. Write ingestion.shift_fetch_status; do not use this view for upserts.';

-- Preserve existing SELECT grants, including column-only grants and grant
-- options. Do not infer reader names or propagate write grants to the views.
-- Readers of these owner-rights views need no new ingestion schema privileges.
DO $$
DECLARE object_name TEXT; permission RECORD; recipient TEXT;
BEGIN
    FOREACH object_name IN ARRAY ARRAY['backfill_progress','sync_state','shift_fetch_status'] LOOP
        -- Remove default grants from each new compatibility view before copying
        -- its predecessor's read ACL, so default privileges cannot widen access.
        FOR permission IN
            SELECT DISTINCT acl.grantee FROM pg_class c
            CROSS JOIN LATERAL aclexplode(c.relacl) acl
            WHERE c.oid = format('public.%I', object_name)::regclass AND acl.grantee <> c.relowner
        LOOP
            recipient := CASE WHEN permission.grantee=0 THEN 'PUBLIC'
                              ELSE quote_ident(pg_get_userbyid(permission.grantee)) END;
            EXECUTE format('REVOKE ALL ON public.%I FROM %s', object_name, recipient);
        END LOOP;
        FOR permission IN
            SELECT acl.grantee, acl.is_grantable, NULL::text AS column_name
            FROM pg_class c
            CROSS JOIN LATERAL aclexplode(COALESCE(c.relacl, acldefault('r', c.relowner))) acl
            WHERE c.oid = format('ingestion.%I', object_name)::regclass AND acl.privilege_type='SELECT'
            UNION ALL
            SELECT acl.grantee, acl.is_grantable, a.attname::text
            FROM pg_attribute a CROSS JOIN LATERAL aclexplode(a.attacl) acl
            WHERE a.attrelid = format('ingestion.%I', object_name)::regclass AND acl.privilege_type='SELECT'
        LOOP
            recipient := CASE WHEN permission.grantee=0 THEN 'PUBLIC'
                              ELSE quote_ident(pg_get_userbyid(permission.grantee)) END;
            EXECUTE format('GRANT SELECT%s ON public.%I TO %s%s',
                CASE WHEN permission.column_name IS NULL THEN '' ELSE format(' (%I)', permission.column_name) END,
                object_name, recipient, CASE WHEN permission.is_grantable THEN ' WITH GRANT OPTION' ELSE '' END);
        END LOOP;
    END LOOP;
END $$;
