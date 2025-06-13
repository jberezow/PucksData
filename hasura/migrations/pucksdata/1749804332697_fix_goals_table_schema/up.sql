-- Drop the goals table from public schema if it exists
DROP TABLE IF EXISTS public.goals CASCADE;

-- Create the events schema if it doesn't exist
CREATE SCHEMA IF NOT EXISTS events;

-- Create the updated_at trigger function if it doesn't exist
CREATE OR REPLACE FUNCTION public.set_current_timestamp_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create goals table in events schema
CREATE TABLE events.goals (
    id SERIAL PRIMARY KEY,
    game_id bigint NOT NULL REFERENCES public.games(game_id),
    period integer NOT NULL,
    period_type text NOT NULL, -- REG, OT, SO
    time_in_period interval NOT NULL, -- Time in the period when goal was scored
    situation_code text NOT NULL, -- e.g. "1551" for even strength, "1552" for power play, etc.
    scoring_team_id integer NOT NULL REFERENCES public.teams(team_id),
    defending_team_id integer NOT NULL REFERENCES public.teams(team_id),
    scorer_id integer NOT NULL REFERENCES public.players(player_id),
    primary_assist_id integer REFERENCES public.players(player_id),
    secondary_assist_id integer REFERENCES public.players(player_id),
    goalie_id integer REFERENCES public.players(player_id),
    strength text NOT NULL, -- EV, PP, SH, EN
    shot_type text, -- wrist, slap, snap, tip-in, etc.
    x_coord integer, -- Shot location x coordinate
    y_coord integer, -- Shot location y coordinate
    zone_code text, -- O, D, N (Offensive, Defensive, Neutral)
    game_winning_goal boolean DEFAULT false,
    insurance_goal boolean DEFAULT false,
    empty_net boolean DEFAULT false,
    created_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT goals_period_type_check CHECK (period_type IN ('REG', 'OT', 'SO')),
    CONSTRAINT goals_strength_check CHECK (strength IN ('EV', 'PP', 'SH', 'EN')),
    CONSTRAINT goals_zone_code_check CHECK (zone_code IN ('O', 'D', 'N'))
);

-- Add indexes for common queries
CREATE INDEX goals_game_id_idx ON events.goals(game_id);
CREATE INDEX goals_scorer_id_idx ON events.goals(scorer_id);
CREATE INDEX goals_scoring_team_id_idx ON events.goals(scoring_team_id);
CREATE INDEX goals_period_idx ON events.goals(period, period_type);

-- Add trigger for updated_at
CREATE TRIGGER set_goals_updated_at
    BEFORE UPDATE ON events.goals
    FOR EACH ROW
    EXECUTE FUNCTION public.set_current_timestamp_updated_at();

-- Add comments
COMMENT ON TABLE events.goals IS 'Records all goals scored in games, including detailed information about the goal, players involved, and game situation.';
COMMENT ON COLUMN events.goals.scoring_team_id IS 'Must be either the home or away team from the associated game. Application-level validation ensures this.';
COMMENT ON COLUMN events.goals.defending_team_id IS 'Must be the opposite team of scoring_team_id from the associated game. Application-level validation ensures this.'; 