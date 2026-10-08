-- Current branding belongs to a franchise, independent of historical NHL team IDs.
CREATE TABLE public.team_branding (
    team_id BIGINT PRIMARY KEY REFERENCES public.teams(team_id),
    logo_url TEXT,
    dark_logo_url TEXT,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE VIEW analytics.nhl_team_branding AS
SELECT teams.team_id AS franchise_id, teams.abbrev, teams.full_name,
       branding.logo_url, branding.dark_logo_url, branding.observed_at
FROM public.teams
LEFT JOIN public.team_branding AS branding USING (team_id);
COMMENT ON VIEW analytics.nhl_team_branding IS
    'Current franchise name and optional NHL-provided logo URLs. One row per abbreviation; not historical branding.';

-- Carry existing entity-writer and reader permissions onto the new contract.
DO $$ DECLARE permission RECORD; BEGIN
    FOR permission IN SELECT DISTINCT acl.grantee FROM pg_class c,
        LATERAL aclexplode(COALESCE(c.relacl, acldefault('r', c.relowner))) acl
        WHERE c.oid = 'public.teams'::regclass
          AND acl.privilege_type = 'INSERT' AND acl.grantee <> 0
    LOOP
        EXECUTE format('GRANT SELECT, INSERT, UPDATE ON public.team_branding TO %I',
                       pg_get_userbyid(permission.grantee));
    END LOOP;
    FOR permission IN SELECT DISTINCT acl.grantee FROM pg_class c,
        LATERAL aclexplode(COALESCE(c.relacl, acldefault('r', c.relowner))) acl
        WHERE c.oid IN ('analytics.franchises'::regclass, 'public.teams'::regclass)
          AND acl.privilege_type = 'SELECT' AND acl.grantee <> 0
    LOOP
        EXECUTE format('GRANT SELECT ON analytics.nhl_team_branding TO %I',
                       pg_get_userbyid(permission.grantee));
    END LOOP;
END $$;
