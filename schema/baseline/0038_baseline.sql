-- Fresh-install schema at version 38. Existing databases retain the legacy migration chain.
SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

CREATE SCHEMA analytics;

COMMENT ON SCHEMA analytics IS 'Supported consumer read contracts: official published facts, derived products, identity projections and coverage metadata. Physical storage may change behind these contracts.';

CREATE SCHEMA history;

COMMENT ON SCHEMA history IS 'Append-only source documents and accepted normalized revisions/observations. Dataset identifiers are stable logical contracts, independent of table names.';

CREATE SCHEMA ingestion;

COMMENT ON SCHEMA ingestion IS 'Ingestion control: attempts, diagnostics, checkpoints, source-observation receipt ledger and pending maintenance. Not a consumer hockey model.';

CREATE SCHEMA observability;

COMMENT ON SCHEMA observability IS 'Read-only operational health, freshness and coverage reports; not ingestion control state.';

SET default_tablespace = '';

SET default_table_access_method = heap;

CREATE TABLE history.snapshots (
    snapshot_id bigint NOT NULL,
    dataset text NOT NULL,
    entity_key text NOT NULL,
    revision bigint NOT NULL,
    recorded_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    method_version text DEFAULT 'normalized-v1'::text NOT NULL,
    content_sha256 text NOT NULL,
    payload jsonb NOT NULL,
    attempt_id bigint,
    CONSTRAINT snapshots_revision_check CHECK ((revision > 0))
);

CREATE FUNCTION history.as_of(p_dataset text, cutoff timestamp with time zone) RETURNS SETOF history.snapshots
    LANGUAGE sql STABLE
    AS $$
    SELECT DISTINCT ON (entity_key) * FROM history.snapshots
    WHERE dataset = p_dataset AND recorded_at <= cutoff
    ORDER BY entity_key, revision DESC
$$;

COMMENT ON FUNCTION history.as_of(p_dataset text, cutoff timestamp with time zone) IS 'Accepted normalized states recorded by the cutoff, including deletion markers. Recorded time is local ingestion time, not NHL publication time or transaction commit time. No pre-capture knowledge is implied.';

CREATE FUNCTION history.capture_entity() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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

CREATE FUNCTION history.next_official_revision(p_game bigint, p_player bigint, p_group text) RETURNS integer
    LANGUAGE sql STABLE
    AS $$
    SELECT COALESCE(MAX((player->>'source_revision')::integer), 0) + 1
    FROM history.snapshots s
    CROSS JOIN LATERAL jsonb_array_elements(s.payload->p_group) player
    WHERE s.dataset = 'official_games' AND s.entity_key = p_game::text
      AND (player->>'player_id')::bigint = p_player
$$;

CREATE FUNCTION history.record(p_dataset text, p_key text, p_payload jsonb, p_method text DEFAULT 'normalized-v1'::text) RETURNS bigint
    LANGUAGE plpgsql
    AS $$
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

CREATE FUNCTION history.reject_mutation() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    RAISE EXCEPTION 'history is append-only';
END $$;

CREATE FUNCTION ingestion.invalidate_backfill_health() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF TG_OP = 'UPDATE' THEN
        IF ROW(OLD.game_id,OLD.season,OLD.status) IS NOT DISTINCT FROM ROW(NEW.game_id,NEW.season,NEW.status) THEN
            RETURN NULL;
        END IF;
    END IF;
    PERFORM ingestion.invalidate_products(ARRAY['observability.season_health']);
    RETURN NULL;
END $$;

CREATE FUNCTION ingestion.invalidate_event_products() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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

CREATE FUNCTION ingestion.invalidate_game_products() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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

CREATE FUNCTION ingestion.invalidate_products(products text[]) RETURNS void
    LANGUAGE sql
    AS $$
    INSERT INTO ingestion.derived_invalidations(product)
    SELECT unnest(products) ON CONFLICT(product, source_transaction) DO NOTHING
$$;

CREATE TABLE analytics.coverage (
    subject text NOT NULL,
    kind text NOT NULL,
    first_season integer,
    note text NOT NULL,
    CONSTRAINT analytics_coverage_first_season_check CHECK (((kind = 'absent'::text) = (first_season IS NULL))),
    CONSTRAINT analytics_coverage_kind_check CHECK ((kind = ANY (ARRAY['event_type'::text, 'measure'::text, 'absent'::text, 'caveat'::text])))
);

COMMENT ON TABLE analytics.coverage IS 'First season each subject is available. Rows with kind=absent are not in the schema at all. Query this before answering a question that spans seasons.';

COMMENT ON COLUMN analytics.coverage.subject IS 'Event type name, derived measure, or the name of an absent concept';

COMMENT ON COLUMN analytics.coverage.kind IS 'event_type: a row in events.event_type; measure: a derived statistic; absent: not present in the schema; caveat: data present but known incomplete at source';

COMMENT ON COLUMN analytics.coverage.first_season IS 'Eight-digit season from which the subject is available; NULL when kind=absent';

CREATE TABLE public.events (
    id bigint NOT NULL,
    game_id bigint NOT NULL,
    event_id_in_game integer NOT NULL,
    period smallint NOT NULL,
    period_type text NOT NULL,
    time_in_period text NOT NULL,
    event_type text NOT NULL,
    x_coord smallint,
    y_coord smallint,
    zone_code text,
    event_owner_team_id bigint,
    home_goalie_present boolean,
    home_skater_count smallint,
    away_skater_count smallint,
    away_goalie_present boolean,
    strength text,
    situation_code text,
    strength_source text DEFAULT 'unavailable'::text NOT NULL,
    season integer NOT NULL,
    game_type smallint NOT NULL,
    game_date date NOT NULL,
    CONSTRAINT events_situation_code_check CHECK ((situation_code ~ '^[01][0-9]{2}[01]$'::text)),
    CONSTRAINT events_strength_check CHECK ((strength = ANY (ARRAY['ev'::text, 'pp'::text, 'sh'::text]))),
    CONSTRAINT events_strength_source_check CHECK ((strength_source = ANY (ARRAY['situation_code'::text, 'scoring_summary'::text, 'html_report'::text, 'unavailable'::text])))
);

COMMENT ON COLUMN public.events.id IS 'Internal generated event-row identity. Replacement ingestion can change this value; not a stable NHL source event ID.';

COMMENT ON COLUMN public.events.event_id_in_game IS 'NHL source event identifier scoped to game_id. Distinct from the internal events.id used by child-table event_id references.';

COMMENT ON COLUMN public.events.event_owner_team_id IS 'NHL franchise ID, referencing public.teams.team_id; not an NHL source team ID.';

COMMENT ON COLUMN public.events.home_goalie_present IS 'Home goalie in net; NULL when the source does not expose on-ice state';

COMMENT ON COLUMN public.events.home_skater_count IS 'Home skaters on ice; NULL when the source does not expose on-ice state';

COMMENT ON COLUMN public.events.away_skater_count IS 'Away skaters on ice; NULL when the source does not expose on-ice state';

COMMENT ON COLUMN public.events.away_goalie_present IS 'Away goalie in net; NULL when the source does not expose on-ice state';

COMMENT ON COLUMN public.events.strength IS 'Manpower from the event owner team perspective using effective skaters: pp, sh, ev, or NULL when the event has no valid owner';

COMMENT ON COLUMN public.events.situation_code IS 'NHL situationCode [away_goalie][away_skaters][home_skaters][home_goalie]; historical values before migration 0012 are reconstructed from decoded fields';

COMMENT ON COLUMN public.events.strength_source IS 'NHL source for strength: situation_code, scoring_summary, html_report, or unavailable';

COMMENT ON COLUMN public.events.season IS 'Denormalized from games.season during ingestion for efficient event filtering';

COMMENT ON COLUMN public.events.game_type IS 'Denormalized from games.game_type during ingestion for efficient event filtering';

COMMENT ON COLUMN public.events.game_date IS 'Denormalized from games.game_date during ingestion for efficient event filtering';

CREATE TABLE public.games (
    game_id bigint NOT NULL,
    season integer NOT NULL,
    game_date date NOT NULL,
    start_time_utc timestamp with time zone,
    home_team_id bigint NOT NULL,
    away_team_id bigint NOT NULL,
    game_type smallint NOT NULL,
    venue text,
    venue_location text,
    game_state text,
    home_score smallint,
    away_score smallint
);

COMMENT ON COLUMN public.games.season IS 'Eight-digit NHL season code, e.g. 20252026; not public.seasons.season_id.';

COMMENT ON COLUMN public.games.home_team_id IS 'NHL franchise ID, referencing public.teams.team_id; not an NHL source team ID.';

COMMENT ON COLUMN public.games.away_team_id IS 'NHL franchise ID, referencing public.teams.team_id; not an NHL source team ID.';

CREATE VIEW analytics.coverage_observed AS
 WITH observed AS (
         SELECT e.event_type,
            min(g.season) AS first_season
           FROM (public.events e
             JOIN public.games g ON ((g.game_id = e.game_id)))
          GROUP BY e.event_type
        )
 SELECT c.subject,
    c.first_season AS declared_first_season,
    observed.first_season AS observed_first_season,
    (observed.first_season IS DISTINCT FROM c.first_season) AS drifted
   FROM (analytics.coverage c
     LEFT JOIN observed ON ((observed.event_type = c.subject)))
  WHERE (c.kind = 'event_type'::text);

COMMENT ON VIEW analytics.coverage_observed IS 'Compares declared event-type coverage with the seasons actually present. Any row with drifted = true means analytics.coverage needs updating. Scans the events table once; expect tens of seconds.';

CREATE TABLE public.players (
    player_id bigint NOT NULL,
    first_name text NOT NULL,
    last_name text NOT NULL,
    "position" text,
    shoots_catches text,
    current_team_abbrev text,
    birth_date date,
    height_cm smallint,
    weight_kg smallint,
    draft_year smallint,
    draft_round smallint,
    draft_pick smallint,
    draft_team_abbrev text,
    draft_overall_pick smallint,
    headshot_url text
);

COMMENT ON COLUMN public.players.headshot_url IS 'Optional NHL-provided player headshot URL from the player landing payload.';

CREATE TABLE public.roster_memberships (
    snapshot_id bigint NOT NULL,
    team_id bigint NOT NULL,
    player_id bigint NOT NULL,
    roster_group text NOT NULL,
    position_code text,
    sweater_number smallint,
    CONSTRAINT roster_memberships_roster_group_check CHECK ((roster_group = ANY (ARRAY['forward'::text, 'defenseman'::text, 'goalie'::text])))
);

COMMENT ON TABLE public.roster_memberships IS 'Player membership in an NHL current-roster observation; this is source data, not fantasy eligibility.';

COMMENT ON COLUMN public.roster_memberships.team_id IS 'NHL franchise ID, referencing public.teams.team_id; not an NHL source team ID.';

CREATE TABLE public.roster_snapshots (
    snapshot_id bigint NOT NULL,
    observed_at timestamp with time zone DEFAULT now() NOT NULL,
    source text NOT NULL,
    team_count integer NOT NULL,
    player_count integer NOT NULL,
    CONSTRAINT roster_snapshots_player_count_check CHECK ((player_count > 0)),
    CONSTRAINT roster_snapshots_team_count_check CHECK ((team_count > 0))
);

COMMENT ON TABLE public.roster_snapshots IS 'Complete observations of all active NHL team rosters from the NHL web API.';

CREATE TABLE public.teams (
    team_id bigint NOT NULL,
    full_name text NOT NULL,
    common_name text NOT NULL,
    place_name text NOT NULL,
    abbrev text NOT NULL
);

COMMENT ON COLUMN public.teams.team_id IS 'Legacy name for the NHL franchise ID. Prefer analytics.franchises.franchise_id in new read contracts.';

CREATE VIEW analytics.current_rosters AS
 SELECT snapshots.snapshot_id,
    snapshots.observed_at,
    teams.team_id,
    teams.abbrev AS team_abbrev,
    memberships.player_id,
    players.first_name,
    players.last_name,
    memberships.roster_group,
    memberships.position_code,
    memberships.sweater_number
   FROM (((public.roster_memberships memberships
     JOIN public.roster_snapshots snapshots ON ((snapshots.snapshot_id = memberships.snapshot_id)))
     JOIN public.teams ON ((teams.team_id = memberships.team_id)))
     LEFT JOIN public.players ON ((players.player_id = memberships.player_id)))
  WHERE (snapshots.snapshot_id = ( SELECT max(roster_snapshots.snapshot_id) AS max
           FROM public.roster_snapshots));

COMMENT ON VIEW analytics.current_rosters IS 'The latest complete NHL current-roster observation, intended for downstream consumers such as fantasy draft eligibility.';

CREATE VIEW analytics.franchises AS
 SELECT team_id AS franchise_id,
    full_name,
    common_name,
    place_name,
    abbrev
   FROM public.teams;

COMMENT ON VIEW analytics.franchises IS 'One row per NHL franchise, with its current display identity. franchise_id is not an NHL source team ID; historical identities live in public.nhl_team_identities.';

CREATE VIEW analytics.game_inventory AS
 SELECT game_id,
    season AS season_code,
    game_date,
    start_time_utc,
    home_team_id AS home_franchise_id,
    away_team_id AS away_franchise_id,
    game_type,
    venue,
    venue_location,
    game_state,
    home_score,
    away_score
   FROM public.games;

COMMENT ON VIEW analytics.game_inventory IS 'One row per game, including unplayed games. Team references use franchise IDs and season_code is the NHL eight-digit season identifier.';

CREATE TABLE analytics.official_goalie_games (
    game_id bigint NOT NULL,
    player_id bigint NOT NULL,
    season integer NOT NULL,
    game_type smallint NOT NULL,
    team_abbrev text,
    full_name text NOT NULL,
    games_started integer,
    wins integer,
    losses integer,
    ties integer,
    ot_losses integer,
    shutouts integer,
    shots_against integer,
    saves integer,
    goals_against integer,
    save_pct double precision,
    time_on_ice_seconds bigint,
    source_revision integer DEFAULT 1 NOT NULL,
    source_observed_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    goals integer,
    assists integer,
    CONSTRAINT official_goalie_games_source_revision_check CHECK ((source_revision > 0))
);

COMMENT ON TABLE analytics.official_goalie_games IS 'Current official NHL goalie totals for one completed game, including decisions and shutouts. source_revision advances only when published values change.';

COMMENT ON COLUMN analytics.official_goalie_games.source_observed_at IS 'Most recent time PucksData successfully observed this row, whether or not its values changed.';

COMMENT ON COLUMN analytics.official_goalie_games.goals IS 'Official goals credited to the goalie in this game';

COMMENT ON COLUMN analytics.official_goalie_games.assists IS 'Official assists credited to the goalie in this game';

CREATE TABLE analytics.official_goalie_seasons (
    player_id bigint NOT NULL,
    season integer NOT NULL,
    game_type smallint NOT NULL,
    full_name text NOT NULL,
    shoots_catches text,
    team_abbrevs text,
    games_played integer,
    games_started integer,
    wins integer,
    losses integer,
    ties integer,
    ot_losses integer,
    shutouts integer,
    shots_against integer,
    saves integer,
    goals_against integer,
    save_pct double precision,
    goals_against_average double precision,
    time_on_ice bigint,
    goals integer,
    assists integer,
    points integer,
    penalty_minutes integer
);

COMMENT ON TABLE analytics.official_goalie_seasons IS 'Official NHL goalie totals per player, season, and game type. Supplies the goalie record (wins, losses, shutouts) that play-by-play cannot establish.';

COMMENT ON COLUMN analytics.official_goalie_seasons.game_type IS '2 = regular season, 3 = playoffs';

COMMENT ON COLUMN analytics.official_goalie_seasons.save_pct IS 'Fraction, not percent: 0.89728 means .897';

COMMENT ON COLUMN analytics.official_goalie_seasons.time_on_ice IS 'Total seconds';

CREATE VIEW analytics.official_player_game_changes AS
 WITH revisions AS (
         SELECT snapshots.snapshot_id,
            snapshots.dataset,
            snapshots.entity_key,
            snapshots.revision,
            snapshots.recorded_at,
            snapshots.method_version,
            snapshots.content_sha256,
            snapshots.payload,
            lag(snapshots.payload) OVER (PARTITION BY snapshots.entity_key ORDER BY snapshots.revision) AS previous
           FROM history.snapshots
          WHERE (snapshots.dataset = 'official_games'::text)
        )
 SELECT (r.entity_key)::bigint AS game_id,
    r.revision AS game_revision,
    r.snapshot_id,
    r.recorded_at,
    changes.player_id,
    changes.stat_code,
    changes.previous_value,
    changes.stat_value,
    changes.change_kind
   FROM (revisions r
     CROSS JOIN LATERAL ( SELECT COALESCE(n.player_id, o.player_id) AS player_id,
            COALESCE(n.stat_code, o.stat_code) AS stat_code,
            o.stat_value AS previous_value,
            n.stat_value,
                CASE
                    WHEN (n.player_id IS NULL) THEN 'retracted'::text
                    ELSE 'set'::text
                END AS change_kind
           FROM (jsonb_to_recordset(COALESCE((r.payload -> 'scoring'::text), '[]'::jsonb)) n(player_id bigint, stat_code text, stat_value double precision)
             FULL JOIN jsonb_to_recordset(COALESCE((r.previous -> 'scoring'::text), '[]'::jsonb)) o(player_id bigint, stat_code text, stat_value double precision) ON (((n.player_id = o.player_id) AND (n.stat_code = o.stat_code))))
          WHERE (n.stat_value IS DISTINCT FROM o.stat_value)) changes);

CREATE TABLE analytics.official_skater_games (
    game_id bigint NOT NULL,
    player_id bigint NOT NULL,
    season integer NOT NULL,
    game_type smallint NOT NULL,
    team_abbrev text,
    full_name text NOT NULL,
    position_code text,
    goals integer,
    assists integer,
    points integer,
    plus_minus integer,
    penalty_minutes integer,
    shots integer,
    ev_goals integer,
    ev_points integer,
    pp_goals integer,
    pp_points integer,
    sh_goals integer,
    sh_points integer,
    ot_goals integer,
    game_winning_goals integer,
    hits integer,
    blocked_shots integer,
    giveaways integer,
    takeaways integer,
    time_on_ice_seconds integer,
    source_revision integer DEFAULT 1 NOT NULL,
    source_observed_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT official_skater_games_source_revision_check CHECK ((source_revision > 0))
);

COMMENT ON TABLE analytics.official_skater_games IS 'Current official NHL skater totals for one completed game. Separate from event-derived facts; source_revision advances only when published values change.';

COMMENT ON COLUMN analytics.official_skater_games.source_observed_at IS 'Most recent time PucksData successfully observed this row, whether or not its values changed.';

CREATE VIEW analytics.official_player_game_stats AS
 SELECT official_skater_games.game_id,
    official_skater_games.player_id,
    official_skater_games.season,
    official_skater_games.game_type,
    official_skater_games.team_abbrev,
    'skater'::text AS player_type,
    stats.stat_code,
    stats.stat_value,
    official_skater_games.source_revision,
    official_skater_games.source_observed_at,
    official_skater_games.updated_at
   FROM (analytics.official_skater_games
     CROSS JOIN LATERAL ( VALUES ('goals'::text,(official_skater_games.goals)::double precision), ('assists'::text,(official_skater_games.assists)::double precision), ('shots'::text,(official_skater_games.shots)::double precision), ('power_play_points'::text,(official_skater_games.pp_points)::double precision), ('short_handed_points'::text,(official_skater_games.sh_points)::double precision), ('game_winning_goals'::text,(official_skater_games.game_winning_goals)::double precision), ('hits'::text,(official_skater_games.hits)::double precision), ('blocks'::text,(official_skater_games.blocked_shots)::double precision), ('plus_minus'::text,(official_skater_games.plus_minus)::double precision)) stats(stat_code, stat_value))
  WHERE (stats.stat_value IS NOT NULL)
UNION ALL
 SELECT official_goalie_games.game_id,
    official_goalie_games.player_id,
    official_goalie_games.season,
    official_goalie_games.game_type,
    official_goalie_games.team_abbrev,
    'goalie'::text AS player_type,
    stats.stat_code,
    stats.stat_value,
    official_goalie_games.source_revision,
    official_goalie_games.source_observed_at,
    official_goalie_games.updated_at
   FROM (analytics.official_goalie_games
     CROSS JOIN LATERAL ( VALUES ('goals'::text,(official_goalie_games.goals)::double precision), ('assists'::text,(official_goalie_games.assists)::double precision), ('wins'::text,(official_goalie_games.wins)::double precision), ('shutouts'::text,(official_goalie_games.shutouts)::double precision), ('saves'::text,(official_goalie_games.saves)::double precision)) stats(stat_code, stat_value))
  WHERE (stats.stat_value IS NOT NULL);

CREATE TABLE analytics.official_skater_seasons (
    player_id bigint NOT NULL,
    season integer NOT NULL,
    game_type smallint NOT NULL,
    full_name text NOT NULL,
    position_code text,
    shoots_catches text,
    team_abbrevs text,
    games_played integer,
    goals integer,
    assists integer,
    points integer,
    plus_minus integer,
    penalty_minutes integer,
    shots integer,
    shooting_pct double precision,
    ev_goals integer,
    ev_points integer,
    pp_goals integer,
    pp_points integer,
    sh_goals integer,
    sh_points integer,
    ot_goals integer,
    game_winning_goals integer,
    points_per_game double precision,
    faceoff_win_pct double precision,
    time_on_ice_per_game double precision
);

COMMENT ON TABLE analytics.official_skater_seasons IS 'Official NHL skater totals per player, season, and game type. Not derived from events; use for season-level answers and for reconciling event-derived figures.';

COMMENT ON COLUMN analytics.official_skater_seasons.game_type IS '2 = regular season, 3 = playoffs';

COMMENT ON COLUMN analytics.official_skater_seasons.team_abbrevs IS 'Comma-separated when the player appeared for more than one team, e.g. COL,CAR,DAL';

COMMENT ON COLUMN analytics.official_skater_seasons.shots IS 'NULL before 1967-68; the NHL did not record shots on goal earlier';

COMMENT ON COLUMN analytics.official_skater_seasons.shooting_pct IS 'Fraction, not percent: 0.06818 means 6.818%';

COMMENT ON COLUMN analytics.official_skater_seasons.time_on_ice_per_game IS 'Seconds per game; NULL before 1997-98';

CREATE TABLE public.blocks (
    event_id bigint NOT NULL,
    blocking_player_id bigint,
    shooting_player_id bigint
);

CREATE TABLE public.faceoffs (
    event_id bigint NOT NULL,
    winning_player_id bigint,
    losing_player_id bigint
);

CREATE TABLE public.goals (
    event_id bigint NOT NULL,
    scorer_player_id bigint,
    assist1_player_id bigint,
    assist2_player_id bigint,
    goalie_id bigint,
    shot_type text
);

CREATE TABLE public.hits (
    event_id bigint NOT NULL,
    hitting_player_id bigint,
    hittee_player_id bigint
);

CREATE TABLE public.penalties (
    event_id bigint NOT NULL,
    committed_by_player_id bigint,
    drawn_by_player_id bigint,
    infraction_type text,
    duration_minutes smallint
);

CREATE TABLE public.shots (
    event_id bigint NOT NULL,
    shooting_player_id bigint,
    goalie_in_net_id bigint,
    shot_type text
);

CREATE MATERIALIZED VIEW analytics.player_event_seasons AS
 WITH participants AS (
         SELECT goals.event_id,
            unnest(ARRAY[goals.scorer_player_id, goals.assist1_player_id, goals.assist2_player_id, goals.goalie_id]) AS player_id
           FROM public.goals
        UNION ALL
         SELECT shots.event_id,
            unnest(ARRAY[shots.shooting_player_id, shots.goalie_in_net_id]) AS unnest
           FROM public.shots
        UNION ALL
         SELECT hits.event_id,
            unnest(ARRAY[hits.hitting_player_id, hits.hittee_player_id]) AS unnest
           FROM public.hits
        UNION ALL
         SELECT blocks.event_id,
            unnest(ARRAY[blocks.blocking_player_id, blocks.shooting_player_id]) AS unnest
           FROM public.blocks
        UNION ALL
         SELECT penalties.event_id,
            unnest(ARRAY[penalties.committed_by_player_id, penalties.drawn_by_player_id]) AS unnest
           FROM public.penalties
        UNION ALL
         SELECT faceoffs.event_id,
            unnest(ARRAY[faceoffs.winning_player_id, faceoffs.losing_player_id]) AS unnest
           FROM public.faceoffs
        )
 SELECT DISTINCT p.player_id,
    g.season,
    g.game_type
   FROM ((participants p
     JOIN public.events e ON ((e.id = p.event_id)))
     JOIN public.games g ON ((g.game_id = e.game_id)))
  WHERE (p.player_id IS NOT NULL)
  WITH NO DATA;

COMMENT ON MATERIALIZED VIEW analytics.player_event_seasons IS 'Seasons and game types in which each player appears in any typed event, in any role. Derived from events only; official season totals are separate. Refreshed by PucksData at the end of every backfill and sync, so it lags ingestion by at most one run.';

COMMENT ON COLUMN analytics.player_event_seasons.game_type IS '1 = preseason, 2 = regular season, 3 = playoffs';

CREATE TABLE public.nhl_team_identities (
    nhl_team_id bigint NOT NULL,
    franchise_id bigint NOT NULL,
    abbrev text NOT NULL,
    full_name text NOT NULL,
    observed_at timestamp with time zone DEFAULT now() NOT NULL
);

COMMENT ON TABLE public.nhl_team_identities IS 'NHL source team identities mapped to franchise IDs used by games and teams. Names and abbreviations describe the NHL identity, not necessarily the current franchise.';

CREATE TABLE public.shifts (
    game_id bigint NOT NULL,
    source_shift_id bigint NOT NULL,
    type_code integer NOT NULL,
    player_id bigint,
    team_id bigint,
    period smallint,
    shift_number integer,
    start_time text,
    end_time text,
    duration text,
    start_time_seconds integer,
    end_time_seconds integer,
    duration_seconds integer,
    ingested_at timestamp with time zone DEFAULT now() NOT NULL,
    event_number integer,
    detail_code integer,
    event_description text,
    event_details text,
    CONSTRAINT shifts_type_code_check CHECK ((type_code = 517))
);

COMMENT ON TABLE public.shifts IS 'Raw typeCode 517 NHL shift-chart rows converted to typed columns without correction or deduplication.';

COMMENT ON COLUMN public.shifts.team_id IS 'NHL teamId as supplied by the source; it is not translated to franchise identity.';

COMMENT ON COLUMN public.shifts.event_number IS 'Source eventNumber, not assumed to be an events table foreign key or a unique chronological ordering.';

COMMENT ON COLUMN public.shifts.detail_code IS 'Source detailCode retained without interpreting undocumented code semantics.';

COMMENT ON COLUMN public.shifts.event_description IS 'Optional source eventDescription for a shift row.';

COMMENT ON COLUMN public.shifts.event_details IS 'Optional source eventDetails for a shift row.';

CREATE VIEW analytics.raw_shift_rows AS
 SELECT s.game_id,
    s.source_shift_id,
    s.type_code,
    s.player_id,
    s.team_id AS nhl_team_id,
    identity.franchise_id,
    s.period,
    s.shift_number,
    s.start_time,
    s.end_time,
    s.duration,
    s.start_time_seconds,
    s.end_time_seconds,
    s.duration_seconds,
    s.event_number,
    s.detail_code,
    s.event_description,
    s.event_details,
    s.ingested_at
   FROM (public.shifts s
     LEFT JOIN public.nhl_team_identities identity ON ((identity.nhl_team_id = s.team_id)));

COMMENT ON VIEW analytics.raw_shift_rows IS 'One raw source row per (game_id, source_shift_id). NHL team IDs and mapped franchise IDs are distinct; unknown identities retain their row with null franchise_id. Times are period-relative. No interval validation, deduplication or on-ice reconstruction is implied.';

CREATE TABLE public.seasons (
    season_id bigint NOT NULL,
    season_year integer NOT NULL,
    start_date date,
    end_date date,
    regular_season_end_date date
);

COMMENT ON COLUMN public.seasons.season_id IS 'Internal generated surrogate; not the NHL eight-digit season code used in games.season.';

COMMENT ON COLUMN public.seasons.season_year IS 'Legacy name for the eight-digit NHL season code, e.g. 20252026. Exposed as analytics.season_catalog.season_code.';

CREATE VIEW analytics.season_catalog AS
 SELECT season_year AS season_code,
    start_date,
    end_date,
    regular_season_end_date
   FROM public.seasons;

COMMENT ON VIEW analytics.season_catalog IS 'One row per eight-digit NHL season code (e.g. 20252026). The internal seasons.season_id surrogate is intentionally omitted.';

CREATE MATERIALIZED VIEW analytics.skater_physical_season_totals AS
 WITH physical_stats AS (
         SELECT hits.hitting_player_id AS player_id,
            events.season,
            events.game_type,
            count(*) AS hits,
            (0)::bigint AS blocks
           FROM (public.hits
             JOIN public.events ON ((events.id = hits.event_id)))
          WHERE (hits.hitting_player_id IS NOT NULL)
          GROUP BY hits.hitting_player_id, events.season, events.game_type
        UNION ALL
         SELECT blocks.blocking_player_id AS player_id,
            events.season,
            events.game_type,
            (0)::bigint AS hits,
            count(*) AS blocks
           FROM (public.blocks
             JOIN public.events ON ((events.id = blocks.event_id)))
          WHERE (blocks.blocking_player_id IS NOT NULL)
          GROUP BY blocks.blocking_player_id, events.season, events.game_type
        )
 SELECT player_id,
    season,
    game_type,
    (sum(hits))::integer AS hits,
    (sum(blocks))::integer AS blocks
   FROM physical_stats
  GROUP BY player_id, season, game_type
  WITH NO DATA;

COMMENT ON MATERIALIZED VIEW analytics.skater_physical_season_totals IS 'Event-derived skater hit and blocked-shot totals by season. Refreshed after event ingestion; coverage begins in 2009-10.';

CREATE TABLE history.observations (
    observation_id bigint NOT NULL,
    snapshot_id bigint NOT NULL,
    method_version text DEFAULT 'normalized-v1'::text NOT NULL,
    recorded_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    attempt_id bigint
);

ALTER TABLE history.observations ALTER COLUMN observation_id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME history.observations_observation_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

ALTER TABLE history.snapshots ALTER COLUMN snapshot_id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME history.snapshots_snapshot_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

CREATE TABLE history.source_documents (
    content_sha256 text NOT NULL,
    body text NOT NULL
);

CREATE TABLE ingestion.attempts (
    attempt_id bigint NOT NULL,
    dataset text NOT NULL,
    entity_key text NOT NULL,
    started_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    finished_at timestamp with time zone,
    outcome text DEFAULT 'running'::text NOT NULL,
    error_message text,
    engine_version text,
    CONSTRAINT attempts_check CHECK (((outcome = 'running'::text) = (finished_at IS NULL))),
    CONSTRAINT attempts_outcome_check CHECK ((outcome = ANY (ARRAY['running'::text, 'complete'::text, 'partial'::text, 'failed'::text, 'unavailable'::text])))
);

ALTER TABLE ingestion.attempts ALTER COLUMN attempt_id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME ingestion.attempts_attempt_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

CREATE TABLE ingestion.backfill_progress (
    game_id bigint NOT NULL,
    season integer NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    error_message text
);

CREATE TABLE ingestion.derived_invalidations (
    invalidation_id bigint NOT NULL,
    product text NOT NULL,
    source_transaction bigint DEFAULT txid_current() NOT NULL
);

ALTER TABLE ingestion.derived_invalidations ALTER COLUMN invalidation_id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME ingestion.derived_invalidations_invalidation_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

CREATE TABLE ingestion.diagnostics (
    diagnostic_id bigint NOT NULL,
    attempt_id bigint NOT NULL,
    code text NOT NULL,
    occurrence_count bigint NOT NULL,
    examples text[] NOT NULL,
    CONSTRAINT diagnostics_occurrence_count_check CHECK ((occurrence_count > 0))
);

ALTER TABLE ingestion.diagnostics ALTER COLUMN diagnostic_id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME ingestion.diagnostics_diagnostic_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

CREATE TABLE ingestion.player_audits (
    season integer NOT NULL,
    completed_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE ingestion.schedule_checks (
    game_id bigint NOT NULL,
    checked_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

CREATE TABLE ingestion.shift_fetch_status (
    game_id bigint NOT NULL,
    status text NOT NULL,
    attempted_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT shift_fetch_status_status_check CHECK ((status = ANY (ARRAY['loaded'::text, 'unavailable'::text, 'failed'::text])))
);

COMMENT ON TABLE ingestion.shift_fetch_status IS 'Latest shift-fetch outcome. Older loaders do not populate this table; absence means no recorded attempt, not no attempt ever.';

CREATE TABLE ingestion.source_observations (
    observation_id bigint NOT NULL,
    attempt_id bigint NOT NULL,
    url text NOT NULL,
    content_sha256 text NOT NULL,
    observed_at timestamp with time zone DEFAULT clock_timestamp() NOT NULL
);

ALTER TABLE ingestion.source_observations ALTER COLUMN observation_id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME ingestion.source_observations_observation_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

CREATE TABLE ingestion.sync_state (
    key text NOT NULL,
    last_sync_at timestamp with time zone,
    last_sync_games integer,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE VIEW observability.season_health_live AS
 WITH games_with_events AS (
         SELECT events.game_id
           FROM public.events
          GROUP BY events.game_id
        ), game_coverage AS (
         SELECT g.season,
            count(*) AS completed_games,
            count(*) FILTER (WHERE (ge.game_id IS NOT NULL)) AS games_with_events,
            count(*) FILTER (WHERE ((ge.game_id IS NULL) AND (bp.game_id IS NOT NULL))) AS acknowledged_gap_games,
            max(g.game_date) AS latest_completed_game_date,
            max(g.game_date) FILTER (WHERE (ge.game_id IS NOT NULL)) AS latest_event_game_date
           FROM ((public.games g
             LEFT JOIN games_with_events ge ON ((ge.game_id = g.game_id)))
             LEFT JOIN ingestion.backfill_progress bp ON (((bp.game_id = g.game_id) AND (bp.status = ANY (ARRAY['done'::text, 'skipped'::text])))))
          WHERE ((g.game_state = ANY (ARRAY['OFF'::text, 'OVER'::text, 'FINAL'::text])) AND (g.game_type <> 1))
          GROUP BY g.season
        ), backfill AS (
         SELECT backfill_progress.season,
            count(*) FILTER (WHERE (backfill_progress.status = 'done'::text)) AS backfill_done,
            count(*) FILTER (WHERE (backfill_progress.status = 'failed'::text)) AS backfill_failed,
            count(*) FILTER (WHERE (backfill_progress.status = 'skipped'::text)) AS backfill_skipped,
            count(*) FILTER (WHERE (backfill_progress.status = 'pending'::text)) AS backfill_pending
           FROM ingestion.backfill_progress
          GROUP BY backfill_progress.season
        ), goal_consistency AS (
         SELECT g.season,
            count(*) AS goals_missing_shots
           FROM ((public.goals goal
             JOIN public.events e ON ((e.id = goal.event_id)))
             JOIN public.games g ON ((g.game_id = e.game_id)))
          WHERE (NOT (EXISTS ( SELECT 1
                   FROM public.shots s
                  WHERE (s.event_id = goal.event_id))))
          GROUP BY g.season
        )
 SELECT coverage.season,
    coverage.completed_games,
    coverage.games_with_events,
    (coverage.completed_games - coverage.games_with_events) AS missing_event_games,
        CASE
            WHEN (coverage.completed_games = 0) THEN (100.0)::double precision
            ELSE (((coverage.games_with_events)::double precision / (coverage.completed_games)::double precision) * (100.0)::double precision)
        END AS event_coverage_pct,
    COALESCE(goals.goals_missing_shots, (0)::bigint) AS goals_missing_shots,
    COALESCE(backfill.backfill_done, (0)::bigint) AS backfill_done,
    COALESCE(backfill.backfill_failed, (0)::bigint) AS backfill_failed,
    COALESCE(backfill.backfill_skipped, (0)::bigint) AS backfill_skipped,
    COALESCE(backfill.backfill_pending, (0)::bigint) AS backfill_pending,
    ((coverage.games_with_events = coverage.completed_games) AND (COALESCE(goals.goals_missing_shots, (0)::bigint) = 0)) AS healthy,
    coverage.acknowledged_gap_games,
    ((coverage.completed_games - coverage.games_with_events) - coverage.acknowledged_gap_games) AS actionable_gap_games,
    coverage.latest_completed_game_date,
    coverage.latest_event_game_date
   FROM ((game_coverage coverage
     LEFT JOIN backfill USING (season))
     LEFT JOIN goal_consistency goals USING (season));

COMMENT ON VIEW observability.season_health_live IS 'Live per-season completeness. Correct but expensive; read observability.season_health instead, which materializes this.';

CREATE MATERIALIZED VIEW observability.season_health AS
 SELECT season,
    completed_games,
    games_with_events,
    missing_event_games,
    event_coverage_pct,
    goals_missing_shots,
    backfill_done,
    backfill_failed,
    backfill_skipped,
    backfill_pending,
    healthy,
    acknowledged_gap_games,
    actionable_gap_games,
    latest_completed_game_date,
    latest_event_game_date,
    now() AS refreshed_at
   FROM observability.season_health_live
  WITH NO DATA;

COMMENT ON MATERIALIZED VIEW observability.season_health IS 'Per-season completeness, materialized. Refreshed by PucksData after every backfill and after any sync that ingested or failed a game; refreshed_at says when.';

CREATE VIEW observability.dataset_health AS
 WITH totals AS (
         SELECT COALESCE((sum(season_health.completed_games))::bigint, (0)::bigint) AS completed_games,
            COALESCE((sum(season_health.games_with_events))::bigint, (0)::bigint) AS games_with_events,
            COALESCE((sum(season_health.missing_event_games))::bigint, (0)::bigint) AS missing_event_games,
            COALESCE((sum(season_health.goals_missing_shots))::bigint, (0)::bigint) AS goals_missing_shots,
            COALESCE((sum(season_health.backfill_failed))::bigint, (0)::bigint) AS backfill_failed,
            COALESCE((sum(season_health.backfill_pending))::bigint, (0)::bigint) AS backfill_pending,
            COALESCE((sum(season_health.backfill_skipped))::bigint, (0)::bigint) AS backfill_skipped,
            COALESCE(bool_and(season_health.healthy), true) AS seasons_healthy,
            COALESCE((sum(season_health.acknowledged_gap_games))::bigint, (0)::bigint) AS acknowledged_gap_games,
            COALESCE((sum(season_health.actionable_gap_games))::bigint, (0)::bigint) AS actionable_gap_games,
            max(season_health.latest_completed_game_date) AS latest_completed_game_date,
            max(season_health.latest_event_game_date) AS latest_event_game_date,
            max(season_health.refreshed_at) AS refreshed_at
           FROM observability.season_health
        )
 SELECT sync.last_sync_at,
    sync.last_sync_games,
    totals.latest_completed_game_date,
    totals.latest_event_game_date,
    totals.completed_games,
    totals.games_with_events,
    totals.missing_event_games,
    totals.goals_missing_shots,
    totals.backfill_failed,
    totals.backfill_pending,
    totals.backfill_skipped,
    ((totals.completed_games > 0) AND totals.seasons_healthy) AS healthy,
    totals.acknowledged_gap_games,
    totals.actionable_gap_games,
    totals.refreshed_at
   FROM ((( VALUES (1)) singleton(value)
     LEFT JOIN ingestion.sync_state sync ON ((sync.key = 'singleton'::text)))
     CROSS JOIN totals);

COMMENT ON COLUMN observability.dataset_health.refreshed_at IS 'When the materialized season figures were last rebuilt. last_sync_at remains live, so freshness checks are unaffected by this lag.';

CREATE VIEW observability.dataset_health_live AS
 WITH totals AS (
         SELECT COALESCE((sum(season_health_live.completed_games))::bigint, (0)::bigint) AS completed_games,
            COALESCE((sum(season_health_live.games_with_events))::bigint, (0)::bigint) AS games_with_events,
            COALESCE((sum(season_health_live.missing_event_games))::bigint, (0)::bigint) AS missing_event_games,
            COALESCE((sum(season_health_live.goals_missing_shots))::bigint, (0)::bigint) AS goals_missing_shots,
            COALESCE((sum(season_health_live.backfill_failed))::bigint, (0)::bigint) AS backfill_failed,
            COALESCE((sum(season_health_live.backfill_pending))::bigint, (0)::bigint) AS backfill_pending,
            COALESCE((sum(season_health_live.backfill_skipped))::bigint, (0)::bigint) AS backfill_skipped,
            COALESCE(bool_and(season_health_live.healthy), true) AS seasons_healthy,
            COALESCE((sum(season_health_live.acknowledged_gap_games))::bigint, (0)::bigint) AS acknowledged_gap_games,
            COALESCE((sum(season_health_live.actionable_gap_games))::bigint, (0)::bigint) AS actionable_gap_games,
            max(season_health_live.latest_completed_game_date) AS latest_completed_game_date,
            max(season_health_live.latest_event_game_date) AS latest_event_game_date,
            NULL::timestamp with time zone AS refreshed_at
           FROM observability.season_health_live
        )
 SELECT sync.last_sync_at,
    sync.last_sync_games,
    totals.latest_completed_game_date,
    totals.latest_event_game_date,
    totals.completed_games,
    totals.games_with_events,
    totals.missing_event_games,
    totals.goals_missing_shots,
    totals.backfill_failed,
    totals.backfill_pending,
    totals.backfill_skipped,
    ((totals.completed_games > 0) AND totals.seasons_healthy) AS healthy,
    totals.acknowledged_gap_games,
    totals.actionable_gap_games,
    totals.refreshed_at
   FROM ((( VALUES (1)) singleton(value)
     LEFT JOIN ingestion.sync_state sync ON ((sync.key = 'singleton'::text)))
     CROSS JOIN totals);

COMMENT ON VIEW observability.dataset_health_live IS 'Live dataset-wide completeness. Correct but expensive; refreshed_at is null because nothing is cached. The status command reads this so an operator never sees a stale verdict.';

CREATE VIEW observability.ingestion_freshness AS
 SELECT DISTINCT ON (dataset, entity_key) dataset,
    entity_key,
    attempt_id,
    started_at AS last_attempt_at,
    finished_at,
    outcome,
    error_message,
    max(finished_at) FILTER (WHERE (outcome = 'complete'::text)) OVER (PARTITION BY dataset, entity_key) AS last_success_at
   FROM ingestion.attempts
  ORDER BY dataset, entity_key, attempt_id DESC;

CREATE VIEW observability.shift_game_coverage AS
 SELECT g.game_id,
    g.season,
    g.game_type,
    (g.season >= 20102011) AS eligible,
    g.shift_rows,
    f.status AS latest_fetch_status,
    f.attempted_at,
        CASE
            WHEN (g.season < 20102011) THEN 'unsupported'::text
            WHEN (g.shift_rows > 0) THEN 'loaded'::text
            WHEN (f.status = 'unavailable'::text) THEN 'unavailable'::text
            WHEN (f.status = 'failed'::text) THEN 'failed'::text
            ELSE 'no_stored_shifts'::text
        END AS availability
   FROM (( SELECT g_1.game_id,
            g_1.season,
            g_1.game_type,
            count(s.game_id) AS shift_rows
           FROM (public.games g_1
             LEFT JOIN public.shifts s ON ((s.game_id = g_1.game_id)))
          WHERE ((g_1.game_type = ANY (ARRAY[2, 3])) AND (g_1.game_state = ANY (ARRAY['OFF'::text, 'OVER'::text, 'FINAL'::text])))
          GROUP BY g_1.game_id, g_1.season, g_1.game_type) g
     LEFT JOIN ingestion.shift_fetch_status f USING (game_id));

COMMENT ON VIEW observability.shift_game_coverage IS 'Stored snapshots take precedence over latest fetch outcomes. Absence of a fetch record does not establish that a game was never attempted. Counts aggregate the game/shift join; season filters reach games before aggregation. Games without shifts retain zero counts.';

CREATE VIEW observability.shift_season_coverage AS
 SELECT season,
    game_type,
    count(*) FILTER (WHERE eligible) AS eligible_games,
    count(*) FILTER (WHERE (NOT eligible)) AS unsupported_games,
    count(*) FILTER (WHERE (availability = 'loaded'::text)) AS loaded_games,
    count(*) FILTER (WHERE (availability = 'unavailable'::text)) AS unavailable_games,
    count(*) FILTER (WHERE (availability = 'failed'::text)) AS failed_games,
    count(*) FILTER (WHERE (availability = 'no_stored_shifts'::text)) AS missing_games,
    sum(shift_rows) AS shift_rows,
        CASE
            WHEN (count(*) FILTER (WHERE eligible) = 0) THEN NULL::double precision
            ELSE ((count(*) FILTER (WHERE (availability = 'loaded'::text)))::double precision / (count(*) FILTER (WHERE eligible))::double precision)
        END AS loaded_fraction
   FROM observability.shift_game_coverage
  GROUP BY season, game_type;

COMMENT ON VIEW observability.shift_season_coverage IS 'Shift availability, not reconstruction correctness. Pre-2010 seasons are unsupported, not unhealthy. Run shifts audit for validation, TOI and event-level reliability; existing event health is unchanged.';

CREATE VIEW public.backfill_progress AS
 SELECT game_id,
    season,
    status,
    updated_at,
    error_message
   FROM ingestion.backfill_progress
 OFFSET 0;

COMMENT ON VIEW public.backfill_progress IS 'Legacy read compatibility. Write ingestion.backfill_progress; do not use this view for upserts.';

ALTER TABLE public.events ALTER COLUMN id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME public.events_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

ALTER TABLE public.roster_snapshots ALTER COLUMN snapshot_id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME public.roster_snapshots_snapshot_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

ALTER TABLE public.seasons ALTER COLUMN season_id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME public.seasons_season_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

CREATE VIEW public.shift_fetch_status AS
 SELECT game_id,
    status,
    attempted_at
   FROM ingestion.shift_fetch_status
 OFFSET 0;

COMMENT ON VIEW public.shift_fetch_status IS 'Legacy read compatibility. Write ingestion.shift_fetch_status; do not use this view for upserts.';

CREATE VIEW public.sync_state AS
 SELECT key,
    last_sync_at,
    last_sync_games,
    updated_at
   FROM ingestion.sync_state
 OFFSET 0;

COMMENT ON VIEW public.sync_state IS 'Legacy read compatibility. Write ingestion.sync_state; do not use this view for upserts.';

ALTER TABLE ONLY analytics.coverage
    ADD CONSTRAINT coverage_pkey PRIMARY KEY (subject);

ALTER TABLE ONLY analytics.official_goalie_games
    ADD CONSTRAINT official_goalie_games_pkey PRIMARY KEY (game_id, player_id);

ALTER TABLE ONLY analytics.official_goalie_seasons
    ADD CONSTRAINT official_goalie_seasons_pkey PRIMARY KEY (player_id, season, game_type);

ALTER TABLE ONLY analytics.official_skater_games
    ADD CONSTRAINT official_skater_games_pkey PRIMARY KEY (game_id, player_id);

ALTER TABLE ONLY analytics.official_skater_seasons
    ADD CONSTRAINT official_skater_seasons_pkey PRIMARY KEY (player_id, season, game_type);

ALTER TABLE ONLY history.observations
    ADD CONSTRAINT observations_pkey PRIMARY KEY (observation_id);

ALTER TABLE ONLY history.snapshots
    ADD CONSTRAINT snapshots_dataset_entity_key_revision_key UNIQUE (dataset, entity_key, revision);

ALTER TABLE ONLY history.snapshots
    ADD CONSTRAINT snapshots_pkey PRIMARY KEY (snapshot_id);

ALTER TABLE ONLY history.source_documents
    ADD CONSTRAINT source_documents_pkey PRIMARY KEY (content_sha256);

ALTER TABLE ONLY ingestion.attempts
    ADD CONSTRAINT attempts_pkey PRIMARY KEY (attempt_id);

ALTER TABLE ONLY ingestion.backfill_progress
    ADD CONSTRAINT backfill_progress_pkey PRIMARY KEY (game_id);

ALTER TABLE ONLY ingestion.derived_invalidations
    ADD CONSTRAINT derived_invalidations_pkey PRIMARY KEY (invalidation_id);

ALTER TABLE ONLY ingestion.derived_invalidations
    ADD CONSTRAINT derived_invalidations_product_source_transaction_key UNIQUE (product, source_transaction);

ALTER TABLE ONLY ingestion.diagnostics
    ADD CONSTRAINT diagnostics_pkey PRIMARY KEY (diagnostic_id);

ALTER TABLE ONLY ingestion.player_audits
    ADD CONSTRAINT player_audits_pkey PRIMARY KEY (season);

ALTER TABLE ONLY ingestion.schedule_checks
    ADD CONSTRAINT schedule_checks_pkey PRIMARY KEY (game_id);

ALTER TABLE ONLY ingestion.shift_fetch_status
    ADD CONSTRAINT shift_fetch_status_pkey PRIMARY KEY (game_id);

ALTER TABLE ONLY ingestion.source_observations
    ADD CONSTRAINT source_observations_pkey PRIMARY KEY (observation_id);

ALTER TABLE ONLY ingestion.sync_state
    ADD CONSTRAINT sync_state_pkey PRIMARY KEY (key);

ALTER TABLE ONLY public.blocks
    ADD CONSTRAINT blocks_pkey PRIMARY KEY (event_id);

ALTER TABLE ONLY public.events
    ADD CONSTRAINT events_game_id_event_id_in_game_key UNIQUE (game_id, event_id_in_game);

ALTER TABLE ONLY public.events
    ADD CONSTRAINT events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.faceoffs
    ADD CONSTRAINT faceoffs_pkey PRIMARY KEY (event_id);

ALTER TABLE ONLY public.games
    ADD CONSTRAINT games_pkey PRIMARY KEY (game_id);

ALTER TABLE ONLY public.goals
    ADD CONSTRAINT goals_pkey PRIMARY KEY (event_id);

ALTER TABLE ONLY public.hits
    ADD CONSTRAINT hits_pkey PRIMARY KEY (event_id);

ALTER TABLE ONLY public.nhl_team_identities
    ADD CONSTRAINT nhl_team_identities_pkey PRIMARY KEY (nhl_team_id);

ALTER TABLE ONLY public.penalties
    ADD CONSTRAINT penalties_pkey PRIMARY KEY (event_id);

ALTER TABLE ONLY public.players
    ADD CONSTRAINT players_pkey PRIMARY KEY (player_id);

ALTER TABLE ONLY public.roster_memberships
    ADD CONSTRAINT roster_memberships_pkey PRIMARY KEY (snapshot_id, team_id, player_id);

ALTER TABLE ONLY public.roster_snapshots
    ADD CONSTRAINT roster_snapshots_pkey PRIMARY KEY (snapshot_id);

ALTER TABLE ONLY public.seasons
    ADD CONSTRAINT seasons_pkey PRIMARY KEY (season_id);

ALTER TABLE ONLY public.seasons
    ADD CONSTRAINT seasons_season_year_key UNIQUE (season_year);

ALTER TABLE ONLY public.shifts
    ADD CONSTRAINT shifts_pkey PRIMARY KEY (game_id, source_shift_id);

ALTER TABLE ONLY public.shots
    ADD CONSTRAINT shots_pkey PRIMARY KEY (event_id);

ALTER TABLE ONLY public.teams
    ADD CONSTRAINT teams_abbrev_key UNIQUE (abbrev);

ALTER TABLE ONLY public.teams
    ADD CONSTRAINT teams_pkey PRIMARY KEY (team_id);

CREATE INDEX idx_official_goalie_games_player ON analytics.official_goalie_games USING btree (player_id, season, game_type, game_id);

CREATE INDEX idx_official_goalie_games_scope ON analytics.official_goalie_games USING btree (season, game_type, game_id);

CREATE INDEX idx_official_goalie_seasons_season ON analytics.official_goalie_seasons USING btree (season);

CREATE INDEX idx_official_skater_games_player ON analytics.official_skater_games USING btree (player_id, season, game_type, game_id);

CREATE INDEX idx_official_skater_games_scope ON analytics.official_skater_games USING btree (season, game_type, game_id);

CREATE INDEX idx_official_skater_seasons_season ON analytics.official_skater_seasons USING btree (season);

CREATE UNIQUE INDEX idx_player_event_seasons_key ON analytics.player_event_seasons USING btree (player_id, season, game_type);

CREATE UNIQUE INDEX idx_skater_physical_season_totals_key ON analytics.skater_physical_season_totals USING btree (player_id, season, game_type);

CREATE INDEX idx_skater_physical_season_totals_season ON analytics.skater_physical_season_totals USING btree (season, game_type, player_id);

CREATE INDEX observations_snapshot_id_recorded_at_idx ON history.observations USING btree (snapshot_id, recorded_at);

CREATE INDEX snapshots_dataset_entity_key_recorded_at_idx ON history.snapshots USING btree (dataset, entity_key, recorded_at DESC);

CREATE INDEX attempts_dataset_entity_key_attempt_id_idx ON ingestion.attempts USING btree (dataset, entity_key, attempt_id DESC);

CREATE INDEX diagnostics_attempt_id_idx ON ingestion.diagnostics USING btree (attempt_id);

CREATE INDEX idx_backfill_progress_season ON ingestion.backfill_progress USING btree (season);

CREATE INDEX idx_backfill_progress_status ON ingestion.backfill_progress USING btree (status);

CREATE INDEX source_observations_attempt_id_idx ON ingestion.source_observations USING btree (attempt_id);

CREATE UNIQUE INDEX idx_season_health_season ON observability.season_health USING btree (season);

CREATE INDEX idx_blocks_blocking_player ON public.blocks USING btree (blocking_player_id) WHERE (blocking_player_id IS NOT NULL);

CREATE INDEX idx_blocks_shooting_player ON public.blocks USING btree (shooting_player_id) WHERE (shooting_player_id IS NOT NULL);

CREATE INDEX idx_events_event_owner_team ON public.events USING btree (event_owner_team_id);

CREATE INDEX idx_events_event_type ON public.events USING btree (event_type);

CREATE INDEX idx_events_game_id ON public.events USING btree (game_id);

CREATE INDEX idx_events_season_game_type_event_type ON public.events USING btree (season, game_type, event_type);

CREATE INDEX idx_faceoffs_losing_player ON public.faceoffs USING btree (losing_player_id) WHERE (losing_player_id IS NOT NULL);

CREATE INDEX idx_faceoffs_winning_player ON public.faceoffs USING btree (winning_player_id) WHERE (winning_player_id IS NOT NULL);

CREATE INDEX idx_games_away_team ON public.games USING btree (away_team_id);

CREATE INDEX idx_games_game_date ON public.games USING btree (game_date);

CREATE INDEX idx_games_home_team ON public.games USING btree (home_team_id);

CREATE INDEX idx_games_season ON public.games USING btree (season);

CREATE INDEX idx_goals_assist1 ON public.goals USING btree (assist1_player_id) WHERE (assist1_player_id IS NOT NULL);

CREATE INDEX idx_goals_assist2 ON public.goals USING btree (assist2_player_id) WHERE (assist2_player_id IS NOT NULL);

CREATE INDEX idx_goals_goalie ON public.goals USING btree (goalie_id) WHERE (goalie_id IS NOT NULL);

CREATE INDEX idx_goals_scorer ON public.goals USING btree (scorer_player_id);

CREATE INDEX idx_hits_hittee_player ON public.hits USING btree (hittee_player_id) WHERE (hittee_player_id IS NOT NULL);

CREATE INDEX idx_hits_hitting_player ON public.hits USING btree (hitting_player_id) WHERE (hitting_player_id IS NOT NULL);

CREATE INDEX idx_nhl_team_identities_franchise ON public.nhl_team_identities USING btree (franchise_id);

CREATE INDEX idx_penalties_committed_by_player ON public.penalties USING btree (committed_by_player_id) WHERE (committed_by_player_id IS NOT NULL);

CREATE INDEX idx_penalties_drawn_by_player ON public.penalties USING btree (drawn_by_player_id) WHERE (drawn_by_player_id IS NOT NULL);

CREATE INDEX idx_players_current_team ON public.players USING btree (current_team_abbrev);

CREATE INDEX idx_roster_memberships_player ON public.roster_memberships USING btree (player_id, snapshot_id DESC);

CREATE INDEX idx_shifts_player_game ON public.shifts USING btree (player_id, game_id);

CREATE INDEX idx_shots_goalie_in_net ON public.shots USING btree (goalie_in_net_id) WHERE (goalie_in_net_id IS NOT NULL);

CREATE INDEX idx_shots_shooter ON public.shots USING btree (shooting_player_id);

-- Static coverage and source identity seeds; observation times use install time.
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('blocked-shot', 'event_type', 20092010, 'Tracked from 2009-10.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('delayed-penalty', 'event_type', 20192020, 'Appears from 2019-20.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('event_on_ice_reconstruction', 'measure', 20102011, 'Derived on demand by shifts reconstruct/audit from NHL events and canonical intervals, not stored on events. Exact-second boundaries retain definite/possible identities. Consult reconstruction status, validation flags, TOI reconciliation and situationCode agreement; availability does not imply complete or independently verified lineups. Shootouts are unsupported.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('faceoff', 'event_type', 20092010, 'Tracked from 2009-10.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('games_played', 'measure', 19171918, 'Not derivable from events, which contain no lineups. Available as official season totals in analytics.official_skater_seasons and analytics.official_goalie_seasons from 1917-18.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('giveaway', 'event_type', 20092010, 'Tracked from 2009-10.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('goal', 'event_type', 19171918, 'Complete from the first NHL season.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('goalie_record', 'measure', 19171918, 'Wins, losses, ties and shutouts are not derivable from events, which contain no goalie of record. Available as official season totals in analytics.official_goalie_seasons from 1917-18.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('historical_team_names', 'absent', NULL, 'Teams are stored at franchise level under their current name. Hartford Whalers resolve to Carolina, Atlanta Thrashers to Winnipeg, the original Winnipeg Jets to Arizona.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('hit', 'event_type', 20092010, 'Tracked from 2009-10.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('missed-shot', 'event_type', 20092010, 'Tracked from 2009-10.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('official_goalie_seasons', 'measure', 19171918, 'Official NHL goalie season totals, including wins, losses, ties and shutouts from 1917-18. Shots against and save percentage are sparse in the earliest seasons.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('official_skater_seasons', 'measure', 19171918, 'Official NHL skater season totals. Games played, goals, assists, points, penalty minutes and game-winning goals from 1917-18; shots, plus-minus and power-play and shorthanded totals from 1967-68; time on ice and faceoff percentage from 1997-98.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('on_ice_skater_counts', 'measure', 20092010, 'events.home_skater_count, away_skater_count and the goalie flags come from situationCode and are NULL before 2009-10.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('penalty', 'event_type', 19171918, 'Complete from the first NHL season. Includes bench and goalie penalties, which per-skater official totals exclude.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('play_by_play_2009_10', 'caveat', 20092010, 'The NHL''s own 2009-10 play-by-play feed is incomplete. Ingestion mirrors it faithfully, but 17 games are missing 23 goals against the official box scores, and some games stop after one or two periods. Prefer analytics.official_skater_seasons and analytics.official_goalie_seasons for 2009-10 season totals.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('plus_minus', 'measure', 19671968, 'Not derivable from events. Available as an official season total in analytics.official_skater_seasons from 1967-68.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('save_percentage', 'measure', 19971998, 'Computes to 0% before 1997-98 because every stored shot against is a goal. Do not report it before 1997-98.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('shifts', 'measure', 20102011, 'Raw NHL shift rows from 2010-11, loaded by season. Source gaps and invalid intervals exist; availability does not certify complete games or reconstructed line combinations.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('shooting_percentage', 'measure', 19971998, 'Computes to 100% before 1997-98 because the only stored shots are goals. Do not report it before 1997-98.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('shootouts', 'absent', NULL, 'Shootout events are excluded during ingestion, so shootout goals and deciders are not represented.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('shot-on-goal', 'event_type', 19971998, 'Shot events begin in 1997-98. Earlier seasons have no shot-on-goal rows.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('shots', 'measure', 19971998, 'The shots table stores every goal as a shot, so before 1997-98 it contains goals only: shot counts equal goal counts. Do not report shot totals before 1997-98.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('stoppage', 'event_type', 20092010, 'Tracked from 2009-10.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('strength', 'measure', 20052006, 'Owner-relative manpower. Exact from 2009-10 via situationCode. For 2005-06 through 2008-09 it is recovered from the NHL scoring summary (goals) and archived play-by-play reports (other events); penalty events are excluded there and remain NULL. Unavailable before 2005-06. See events.strength_source for the source of any row.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('takeaway', 'event_type', 20092010, 'Tracked from 2009-10.');
INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES ('time_on_ice', 'absent', NULL, 'Official skater season averages and goalie totals are available. Raw shifts from 2010-11 support separate validated ice-time reconstruction; events alone do not provide continuous ice time.');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (1, 23, 'NJD', 'New Jersey Devils');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (2, 22, 'NYI', 'New York Islanders');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (3, 10, 'NYR', 'New York Rangers');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (4, 16, 'PHI', 'Philadelphia Flyers');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (5, 17, 'PIT', 'Pittsburgh Penguins');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (6, 6, 'BOS', 'Boston Bruins');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (7, 19, 'BUF', 'Buffalo Sabres');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (8, 1, 'MTL', 'Montréal Canadiens');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (9, 30, 'OTT', 'Ottawa Senators');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (10, 5, 'TOR', 'Toronto Maple Leafs');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (11, 35, 'ATL', 'Atlanta Thrashers');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (12, 26, 'CAR', 'Carolina Hurricanes');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (13, 33, 'FLA', 'Florida Panthers');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (14, 31, 'TBL', 'Tampa Bay Lightning');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (15, 24, 'WSH', 'Washington Capitals');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (16, 11, 'CHI', 'Chicago Blackhawks');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (17, 12, 'DET', 'Detroit Red Wings');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (18, 34, 'NSH', 'Nashville Predators');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (19, 18, 'STL', 'St. Louis Blues');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (20, 21, 'CGY', 'Calgary Flames');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (21, 27, 'COL', 'Colorado Avalanche');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (22, 25, 'EDM', 'Edmonton Oilers');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (23, 20, 'VAN', 'Vancouver Canucks');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (24, 32, 'ANA', 'Anaheim Ducks');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (25, 15, 'DAL', 'Dallas Stars');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (26, 14, 'LAK', 'Los Angeles Kings');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (27, 28, 'PHX', 'Phoenix Coyotes');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (28, 29, 'SJS', 'San Jose Sharks');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (29, 36, 'CBJ', 'Columbus Blue Jackets');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (30, 37, 'MIN', 'Minnesota Wild');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (31, 15, 'MNS', 'Minnesota North Stars');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (32, 27, 'QUE', 'Quebec Nordiques');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (33, 35, 'WIN', 'Winnipeg Jets (1979)');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (34, 26, 'HFD', 'Hartford Whalers');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (35, 23, 'CLR', 'Colorado Rockies');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (36, 3, 'SEN', 'Ottawa Senators (1917)');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (37, 4, 'HAM', 'Hamilton Tigers');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (38, 9, 'PIR', 'Pittsburgh Pirates');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (39, 9, 'QUA', 'Philadelphia Quakers');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (40, 12, 'DCG', 'Detroit Cougars');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (41, 2, 'MWN', 'Montreal Wanderers');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (42, 4, 'QBD', 'Quebec Bulldogs');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (43, 7, 'MMR', 'Montreal Maroons');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (44, 8, 'NYA', 'New York Americans');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (45, 3, 'SLE', 'St. Louis Eagles');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (46, 13, 'OAK', 'Oakland Seals');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (47, 21, 'AFM', 'Atlanta Flames');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (48, 23, 'KCS', 'Kansas City Scouts');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (49, 13, 'CLE', 'Cleveland Barons');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (50, 12, 'DFL', 'Detroit Falcons');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (51, 8, 'BRK', 'Brooklyn Americans');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (52, 35, 'WPG', 'Winnipeg Jets');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (53, 28, 'ARI', 'Arizona Coyotes');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (54, 38, 'VGK', 'Vegas Golden Knights');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (55, 39, 'SEA', 'Seattle Kraken');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (56, 13, 'CGS', 'California Golden Seals');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (57, 5, 'TAN', 'Toronto Arenas');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (58, 5, 'TSP', 'Toronto St. Patricks');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (59, 40, 'UTA', 'Utah Hockey Club');
INSERT INTO public.nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES (68, 40, 'UTA', 'Utah Mammoth');


CREATE TRIGGER history_goalie_seasons AFTER INSERT OR DELETE OR UPDATE ON analytics.official_goalie_seasons FOR EACH ROW EXECUTE FUNCTION history.capture_entity('official_goalie_seasons', 'player_id', 'season', 'game_type');

CREATE TRIGGER history_skater_seasons AFTER INSERT OR DELETE OR UPDATE ON analytics.official_skater_seasons FOR EACH ROW EXECUTE FUNCTION history.capture_entity('official_skater_seasons', 'player_id', 'season', 'game_type');

CREATE TRIGGER immutable_documents BEFORE DELETE OR UPDATE ON history.source_documents FOR EACH ROW EXECUTE FUNCTION history.reject_mutation();

CREATE TRIGGER immutable_observations BEFORE DELETE OR UPDATE ON history.observations FOR EACH ROW EXECUTE FUNCTION history.reject_mutation();

CREATE TRIGGER immutable_snapshots BEFORE DELETE OR UPDATE ON history.snapshots FOR EACH ROW EXECUTE FUNCTION history.reject_mutation();

CREATE TRIGGER immutable_source_observations BEFORE DELETE OR UPDATE ON ingestion.source_observations FOR EACH ROW EXECUTE FUNCTION history.reject_mutation();

CREATE TRIGGER invalidate_backfill AFTER INSERT OR DELETE OR UPDATE ON ingestion.backfill_progress FOR EACH ROW EXECUTE FUNCTION ingestion.invalidate_backfill_health();

CREATE TRIGGER invalidate_truncate AFTER TRUNCATE ON ingestion.backfill_progress FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('observability.season_health');

CREATE TRIGGER history_games AFTER INSERT OR DELETE OR UPDATE ON public.games FOR EACH ROW EXECUTE FUNCTION history.capture_entity('games', 'game_id');

CREATE TRIGGER history_players AFTER INSERT OR DELETE OR UPDATE ON public.players FOR EACH ROW EXECUTE FUNCTION history.capture_entity('players', 'player_id');

CREATE TRIGGER history_seasons AFTER INSERT OR DELETE OR UPDATE ON public.seasons FOR EACH ROW EXECUTE FUNCTION history.capture_entity('seasons', 'season_year');

CREATE TRIGGER history_team_identities AFTER INSERT OR DELETE OR UPDATE ON public.nhl_team_identities FOR EACH ROW EXECUTE FUNCTION history.capture_entity('nhl_team_identities', 'nhl_team_id');

CREATE TRIGGER history_teams AFTER INSERT OR DELETE OR UPDATE ON public.teams FOR EACH ROW EXECUTE FUNCTION history.capture_entity('teams', 'team_id');

CREATE TRIGGER invalidate_delete AFTER DELETE ON public.blocks REFERENCING OLD TABLE AS old_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'analytics.skater_physical_season_totals');

CREATE TRIGGER invalidate_delete AFTER DELETE ON public.events REFERENCING OLD TABLE AS old_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'analytics.skater_physical_season_totals', 'observability.season_health');

CREATE TRIGGER invalidate_delete AFTER DELETE ON public.faceoffs REFERENCING OLD TABLE AS old_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons');

CREATE TRIGGER invalidate_delete AFTER DELETE ON public.goals REFERENCING OLD TABLE AS old_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'observability.season_health');

CREATE TRIGGER invalidate_delete AFTER DELETE ON public.hits REFERENCING OLD TABLE AS old_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'analytics.skater_physical_season_totals');

CREATE TRIGGER invalidate_delete AFTER DELETE ON public.penalties REFERENCING OLD TABLE AS old_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons');

CREATE TRIGGER invalidate_delete AFTER DELETE ON public.shots REFERENCING OLD TABLE AS old_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'observability.season_health');

CREATE TRIGGER invalidate_games AFTER INSERT OR DELETE OR UPDATE ON public.games FOR EACH ROW EXECUTE FUNCTION ingestion.invalidate_game_products();

CREATE TRIGGER invalidate_insert AFTER INSERT ON public.blocks REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'analytics.skater_physical_season_totals');

CREATE TRIGGER invalidate_insert AFTER INSERT ON public.events REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'analytics.skater_physical_season_totals', 'observability.season_health');

CREATE TRIGGER invalidate_insert AFTER INSERT ON public.faceoffs REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons');

CREATE TRIGGER invalidate_insert AFTER INSERT ON public.goals REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'observability.season_health');

CREATE TRIGGER invalidate_insert AFTER INSERT ON public.hits REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'analytics.skater_physical_season_totals');

CREATE TRIGGER invalidate_insert AFTER INSERT ON public.penalties REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons');

CREATE TRIGGER invalidate_insert AFTER INSERT ON public.shots REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'observability.season_health');

CREATE TRIGGER invalidate_truncate AFTER TRUNCATE ON public.blocks FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'analytics.skater_physical_season_totals');

CREATE TRIGGER invalidate_truncate AFTER TRUNCATE ON public.events FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'analytics.skater_physical_season_totals', 'observability.season_health');

CREATE TRIGGER invalidate_truncate AFTER TRUNCATE ON public.faceoffs FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons');

CREATE TRIGGER invalidate_truncate AFTER TRUNCATE ON public.games FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'observability.season_health');

CREATE TRIGGER invalidate_truncate AFTER TRUNCATE ON public.goals FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'observability.season_health');

CREATE TRIGGER invalidate_truncate AFTER TRUNCATE ON public.hits FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'analytics.skater_physical_season_totals');

CREATE TRIGGER invalidate_truncate AFTER TRUNCATE ON public.penalties FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons');

CREATE TRIGGER invalidate_truncate AFTER TRUNCATE ON public.shots FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'observability.season_health');

CREATE TRIGGER invalidate_update AFTER UPDATE ON public.blocks REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'analytics.skater_physical_season_totals');

CREATE TRIGGER invalidate_update AFTER UPDATE ON public.events REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'analytics.skater_physical_season_totals', 'observability.season_health');

CREATE TRIGGER invalidate_update AFTER UPDATE ON public.faceoffs REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons');

CREATE TRIGGER invalidate_update AFTER UPDATE ON public.goals REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'observability.season_health');

CREATE TRIGGER invalidate_update AFTER UPDATE ON public.hits REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'analytics.skater_physical_season_totals');

CREATE TRIGGER invalidate_update AFTER UPDATE ON public.penalties REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons');

CREATE TRIGGER invalidate_update AFTER UPDATE ON public.shots REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION ingestion.invalidate_event_products('analytics.player_event_seasons', 'observability.season_health');

ALTER TABLE ONLY analytics.official_goalie_games
    ADD CONSTRAINT official_goalie_games_game_id_fkey FOREIGN KEY (game_id) REFERENCES public.games(game_id) ON DELETE CASCADE;

ALTER TABLE ONLY analytics.official_skater_games
    ADD CONSTRAINT official_skater_games_game_id_fkey FOREIGN KEY (game_id) REFERENCES public.games(game_id) ON DELETE CASCADE;

ALTER TABLE ONLY history.observations
    ADD CONSTRAINT observations_attempt_id_fkey FOREIGN KEY (attempt_id) REFERENCES ingestion.attempts(attempt_id);

ALTER TABLE ONLY history.observations
    ADD CONSTRAINT observations_snapshot_id_fkey FOREIGN KEY (snapshot_id) REFERENCES history.snapshots(snapshot_id);

ALTER TABLE ONLY history.snapshots
    ADD CONSTRAINT snapshots_attempt_id_fkey FOREIGN KEY (attempt_id) REFERENCES ingestion.attempts(attempt_id);

ALTER TABLE ONLY ingestion.diagnostics
    ADD CONSTRAINT diagnostics_attempt_id_fkey FOREIGN KEY (attempt_id) REFERENCES ingestion.attempts(attempt_id);

ALTER TABLE ONLY ingestion.schedule_checks
    ADD CONSTRAINT schedule_checks_game_id_fkey FOREIGN KEY (game_id) REFERENCES public.games(game_id) ON DELETE CASCADE;

ALTER TABLE ONLY ingestion.shift_fetch_status
    ADD CONSTRAINT shift_fetch_status_game_id_fkey FOREIGN KEY (game_id) REFERENCES public.games(game_id) ON DELETE CASCADE;

ALTER TABLE ONLY ingestion.source_observations
    ADD CONSTRAINT source_observations_attempt_id_fkey FOREIGN KEY (attempt_id) REFERENCES ingestion.attempts(attempt_id);

ALTER TABLE ONLY ingestion.source_observations
    ADD CONSTRAINT source_observations_content_sha256_fkey FOREIGN KEY (content_sha256) REFERENCES history.source_documents(content_sha256);

ALTER TABLE ONLY public.blocks
    ADD CONSTRAINT blocks_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id);

ALTER TABLE ONLY public.events
    ADD CONSTRAINT events_event_owner_team_id_fkey FOREIGN KEY (event_owner_team_id) REFERENCES public.teams(team_id);

ALTER TABLE ONLY public.events
    ADD CONSTRAINT events_game_id_fkey FOREIGN KEY (game_id) REFERENCES public.games(game_id);

ALTER TABLE ONLY public.faceoffs
    ADD CONSTRAINT faceoffs_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id);

ALTER TABLE ONLY public.games
    ADD CONSTRAINT games_away_team_id_fkey FOREIGN KEY (away_team_id) REFERENCES public.teams(team_id);

ALTER TABLE ONLY public.games
    ADD CONSTRAINT games_home_team_id_fkey FOREIGN KEY (home_team_id) REFERENCES public.teams(team_id);

ALTER TABLE ONLY public.goals
    ADD CONSTRAINT goals_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id);

ALTER TABLE ONLY public.hits
    ADD CONSTRAINT hits_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id);

ALTER TABLE ONLY public.penalties
    ADD CONSTRAINT penalties_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id);

ALTER TABLE ONLY public.roster_memberships
    ADD CONSTRAINT roster_memberships_snapshot_id_fkey FOREIGN KEY (snapshot_id) REFERENCES public.roster_snapshots(snapshot_id) ON DELETE CASCADE;

ALTER TABLE ONLY public.roster_memberships
    ADD CONSTRAINT roster_memberships_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.teams(team_id);

ALTER TABLE ONLY public.shifts
    ADD CONSTRAINT shifts_game_id_fkey FOREIGN KEY (game_id) REFERENCES public.games(game_id) ON DELETE CASCADE;

ALTER TABLE ONLY public.shots
    ADD CONSTRAINT shots_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.events(id);


INSERT INTO ingestion.derived_invalidations(product) VALUES ('analytics.player_event_seasons'), ('analytics.skater_physical_season_totals'), ('observability.season_health');

REFRESH MATERIALIZED VIEW analytics.player_event_seasons;
REFRESH MATERIALIZED VIEW analytics.skater_physical_season_totals;
REFRESH MATERIALIZED VIEW observability.season_health;
