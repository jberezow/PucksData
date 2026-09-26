-- Stabilize logical dataset names before any future physical table renames.
-- Existing dataset strings and snapshot payloads are deliberately preserved.
CREATE OR REPLACE FUNCTION history.capture_entity() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE body JSONB; identity TEXT; column_name TEXT;
BEGIN
    IF TG_NARGS < 2 THEN
        RAISE EXCEPTION 'history.capture_entity requires a stable dataset name and identity columns';
    END IF;
    body := CASE WHEN TG_OP = 'DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;
    identity := '';
    FOREACH column_name IN ARRAY TG_ARGV[1:TG_NARGS-1] LOOP
        IF body->>column_name IS NULL THEN
            RAISE EXCEPTION 'history identity column % is missing or null', column_name;
        END IF;
        identity := identity || CASE WHEN identity = '' THEN '' ELSE ':' END || (body->>column_name);
    END LOOP;
    body := CASE WHEN TG_OP = 'DELETE' THEN jsonb_build_object('deleted', true)
                 ELSE body - ARRAY['observed_at', 'updated_at', 'season_id'] END;
    PERFORM history.record(TG_ARGV[0], identity, body);
    RETURN NULL;
END $$;

DROP TRIGGER history_games ON public.games;
CREATE TRIGGER history_games AFTER INSERT OR UPDATE OR DELETE ON public.games
FOR EACH ROW EXECUTE FUNCTION history.capture_entity('games', 'game_id');
DROP TRIGGER history_players ON public.players;
CREATE TRIGGER history_players AFTER INSERT OR UPDATE OR DELETE ON public.players
FOR EACH ROW EXECUTE FUNCTION history.capture_entity('players', 'player_id');
DROP TRIGGER history_teams ON public.teams;
CREATE TRIGGER history_teams AFTER INSERT OR UPDATE OR DELETE ON public.teams
FOR EACH ROW EXECUTE FUNCTION history.capture_entity('teams', 'team_id');
DROP TRIGGER history_seasons ON public.seasons;
CREATE TRIGGER history_seasons AFTER INSERT OR UPDATE OR DELETE ON public.seasons
FOR EACH ROW EXECUTE FUNCTION history.capture_entity('seasons', 'season_year');
DROP TRIGGER history_team_identities ON public.nhl_team_identities;
CREATE TRIGGER history_team_identities AFTER INSERT OR UPDATE OR DELETE ON public.nhl_team_identities
FOR EACH ROW EXECUTE FUNCTION history.capture_entity('nhl_team_identities', 'nhl_team_id');
DROP TRIGGER history_skater_seasons ON analytics.official_skater_seasons;
CREATE TRIGGER history_skater_seasons AFTER INSERT OR UPDATE OR DELETE ON analytics.official_skater_seasons
FOR EACH ROW EXECUTE FUNCTION history.capture_entity('official_skater_seasons', 'player_id', 'season', 'game_type');
DROP TRIGGER history_goalie_seasons ON analytics.official_goalie_seasons;
CREATE TRIGGER history_goalie_seasons AFTER INSERT OR UPDATE OR DELETE ON analytics.official_goalie_seasons
FOR EACH ROW EXECUTE FUNCTION history.capture_entity('official_goalie_seasons', 'player_id', 'season', 'game_type');

COMMENT ON SCHEMA analytics IS
    'Supported consumer read contracts: official published facts, derived products, identity projections and coverage metadata. Physical storage may change behind these contracts.';
COMMENT ON SCHEMA ingestion IS
    'Ingestion control: attempts, diagnostics, checkpoints, source-observation receipt ledger and pending maintenance. Not a consumer hockey model.';
COMMENT ON SCHEMA history IS
    'Append-only source documents and accepted normalized revisions/observations. Dataset identifiers are stable logical contracts, independent of table names.';
COMMENT ON SCHEMA observability IS
    'Read-only operational health, freshness and coverage reports; not ingestion control state.';

-- Additive read contracts. Legacy tables/columns remain unchanged for deployed
-- consumers. Explicit projection lists prevent accidental API expansion.
CREATE VIEW analytics.franchises AS
SELECT team_id AS franchise_id, full_name, common_name, place_name, abbrev
FROM public.teams;
COMMENT ON VIEW analytics.franchises IS
    'One row per NHL franchise, with its current display identity. franchise_id is not an NHL source team ID; historical identities live in public.nhl_team_identities.';

CREATE VIEW analytics.season_catalog AS
SELECT season_year AS season_code, start_date, end_date, regular_season_end_date
FROM public.seasons;
COMMENT ON VIEW analytics.season_catalog IS
    'One row per eight-digit NHL season code (e.g. 20252026). The internal seasons.season_id surrogate is intentionally omitted.';

CREATE VIEW analytics.game_inventory AS
SELECT game_id, season AS season_code, game_date, start_time_utc,
       home_team_id AS home_franchise_id, away_team_id AS away_franchise_id,
       game_type, venue, venue_location, game_state, home_score, away_score
FROM public.games;
COMMENT ON VIEW analytics.game_inventory IS
    'One row per game, including unplayed games. Team references use franchise IDs and season_code is the NHL eight-digit season identifier.';

CREATE VIEW analytics.raw_shift_rows AS
SELECT s.game_id, s.source_shift_id, s.type_code, s.player_id,
       s.team_id AS nhl_team_id, identity.franchise_id,
       s.period, s.shift_number, s.start_time, s.end_time, s.duration,
       s.start_time_seconds, s.end_time_seconds, s.duration_seconds,
       s.event_number, s.detail_code, s.event_description, s.event_details, s.ingested_at
FROM public.shifts s
LEFT JOIN public.nhl_team_identities identity ON identity.nhl_team_id = s.team_id;
COMMENT ON VIEW analytics.raw_shift_rows IS
    'One raw source row per (game_id, source_shift_id). NHL team IDs and mapped franchise IDs are distinct; unknown identities retain their row with null franchise_id. Times are period-relative. No interval validation, deduplication or on-ice reconstruction is implied.';

COMMENT ON COLUMN public.teams.team_id IS 'Legacy name for the NHL franchise ID. Prefer analytics.franchises.franchise_id in new read contracts.';
COMMENT ON COLUMN public.games.home_team_id IS 'NHL franchise ID, referencing public.teams.team_id; not an NHL source team ID.';
COMMENT ON COLUMN public.games.away_team_id IS 'NHL franchise ID, referencing public.teams.team_id; not an NHL source team ID.';
COMMENT ON COLUMN public.events.event_owner_team_id IS 'NHL franchise ID, referencing public.teams.team_id; not an NHL source team ID.';
COMMENT ON COLUMN public.roster_memberships.team_id IS 'NHL franchise ID, referencing public.teams.team_id; not an NHL source team ID.';
COMMENT ON COLUMN public.seasons.season_id IS 'Internal generated surrogate; not the NHL eight-digit season code used in games.season.';
COMMENT ON COLUMN public.seasons.season_year IS 'Legacy name for the eight-digit NHL season code, e.g. 20252026. Exposed as analytics.season_catalog.season_code.';
COMMENT ON COLUMN public.games.season IS 'Eight-digit NHL season code, e.g. 20252026; not public.seasons.season_id.';
COMMENT ON COLUMN public.events.id IS 'Internal generated event-row identity. Replacement ingestion can change this value; not a stable NHL source event ID.';
COMMENT ON COLUMN public.events.event_id_in_game IS 'NHL source event identifier scoped to game_id. Distinct from the internal events.id used by child-table event_id references.';

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'pucksstudio_read') THEN
        GRANT USAGE ON SCHEMA analytics TO pucksstudio_read;
        GRANT SELECT ON analytics.franchises, analytics.season_catalog,
            analytics.game_inventory, analytics.raw_shift_rows TO pucksstudio_read;
    END IF;
END $$;
