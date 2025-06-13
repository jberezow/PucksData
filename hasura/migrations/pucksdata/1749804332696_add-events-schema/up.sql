SET check_function_bodies = false;
CREATE TABLE public.games (
    game_id bigint NOT NULL,
    season integer NOT NULL,
    game_type integer NOT NULL,
    game_date date NOT NULL,
    start_time_utc timestamp without time zone NOT NULL,
    venue_name text,
    venue_location text,
    venue_timezone text,
    eastern_utc_offset text,
    venue_utc_offset text,
    home_team_id integer NOT NULL,
    away_team_id integer NOT NULL,
    game_state text,
    game_schedule_state text,
    home_score integer,
    away_score integer,
    home_sog integer,
    away_sog integer,
    limited_scoring boolean DEFAULT false,
    shootout_in_use boolean DEFAULT true,
    ot_in_use boolean DEFAULT true,
    ties_in_use boolean DEFAULT false,
    max_periods integer DEFAULT 5,
    reg_periods integer DEFAULT 3,
    final_period_number integer,
    final_period_type text,
    created_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE public.players (
    player_id integer NOT NULL,
    first_name text,
    last_name text,
    is_active boolean,
    current_team_id integer,
    current_team_abbrev text,
    sweater_number integer,
    "position" text,
    height_in_inches integer,
    height_in_centimeters integer,
    weight_in_pounds integer,
    weight_in_kilograms integer,
    birth_date date,
    birth_city text,
    birth_state_province text,
    birth_country text,
    shoots_catches text,
    draft_year integer,
    draft_team_abbrev text,
    draft_round integer,
    draft_pick_in_round integer,
    draft_overall_pick integer
);
CREATE TABLE public.raw_data (
    id integer NOT NULL,
    endpoint text NOT NULL,
    parameters jsonb,
    data jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);
CREATE SEQUENCE public.raw_data_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;
ALTER SEQUENCE public.raw_data_id_seq OWNED BY public.raw_data.id;
CREATE TABLE public.teams (
    team_id integer NOT NULL,
    abbrev character varying(3) NOT NULL,
    common_name text NOT NULL,
    place_name text NOT NULL,
    place_name_with_preposition text,
    logo_light_url text,
    logo_dark_url text,
    created_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP
);
ALTER TABLE ONLY public.raw_data ALTER COLUMN id SET DEFAULT nextval('public.raw_data_id_seq'::regclass);
ALTER TABLE ONLY public.games
    ADD CONSTRAINT games_pkey PRIMARY KEY (game_id);
ALTER TABLE ONLY public.players
    ADD CONSTRAINT players_pkey PRIMARY KEY (player_id);
ALTER TABLE ONLY public.raw_data
    ADD CONSTRAINT raw_data_pkey PRIMARY KEY (id);
ALTER TABLE ONLY public.teams
    ADD CONSTRAINT teams_abbrev_key UNIQUE (abbrev);
ALTER TABLE ONLY public.teams
    ADD CONSTRAINT teams_pkey PRIMARY KEY (team_id);
ALTER TABLE ONLY public.games
    ADD CONSTRAINT games_away_team_id_fkey FOREIGN KEY (away_team_id) REFERENCES public.teams(team_id);
ALTER TABLE ONLY public.games
    ADD CONSTRAINT games_home_team_id_fkey FOREIGN KEY (home_team_id) REFERENCES public.teams(team_id);
