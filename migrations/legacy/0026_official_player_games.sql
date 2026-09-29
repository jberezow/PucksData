-- Official final-boxscore statistics at player/game grain.
--
-- These rows complement, rather than replace, the event model. They preserve
-- league-published facts such as plus/minus, game-winning goals, goalie
-- decisions, and shutouts that cannot be reconstructed reliably from events.

CREATE TABLE analytics.official_skater_games (
    game_id               BIGINT   NOT NULL REFERENCES games(game_id) ON DELETE CASCADE,
    player_id             BIGINT   NOT NULL,
    season                INTEGER  NOT NULL,
    game_type             SMALLINT NOT NULL,
    team_abbrev           TEXT,
    full_name             TEXT     NOT NULL,
    position_code         TEXT,
    goals                 INTEGER,
    assists               INTEGER,
    points                INTEGER,
    plus_minus            INTEGER,
    penalty_minutes       INTEGER,
    shots                 INTEGER,
    ev_goals              INTEGER,
    ev_points             INTEGER,
    pp_goals              INTEGER,
    pp_points             INTEGER,
    sh_goals              INTEGER,
    sh_points             INTEGER,
    ot_goals              INTEGER,
    game_winning_goals    INTEGER,
    hits                  INTEGER,
    blocked_shots         INTEGER,
    giveaways             INTEGER,
    takeaways              INTEGER,
    time_on_ice_seconds   INTEGER,
    source_revision       INTEGER NOT NULL DEFAULT 1 CHECK (source_revision > 0),
    source_observed_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (game_id, player_id)
);

CREATE INDEX idx_official_skater_games_player
    ON analytics.official_skater_games(player_id, season, game_type, game_id);
CREATE INDEX idx_official_skater_games_scope
    ON analytics.official_skater_games(season, game_type, game_id);

CREATE TABLE analytics.official_goalie_games (
    game_id               BIGINT   NOT NULL REFERENCES games(game_id) ON DELETE CASCADE,
    player_id             BIGINT   NOT NULL,
    season                INTEGER  NOT NULL,
    game_type             SMALLINT NOT NULL,
    team_abbrev           TEXT,
    full_name             TEXT     NOT NULL,
    games_started         INTEGER,
    wins                  INTEGER,
    losses                INTEGER,
    ties                  INTEGER,
    ot_losses              INTEGER,
    shutouts              INTEGER,
    shots_against         INTEGER,
    saves                 INTEGER,
    goals_against         INTEGER,
    save_pct              DOUBLE PRECISION,
    time_on_ice_seconds   BIGINT,
    source_revision       INTEGER NOT NULL DEFAULT 1 CHECK (source_revision > 0),
    source_observed_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (game_id, player_id)
);

CREATE INDEX idx_official_goalie_games_player
    ON analytics.official_goalie_games(player_id, season, game_type, game_id);
CREATE INDEX idx_official_goalie_games_scope
    ON analytics.official_goalie_games(season, game_type, game_id);

COMMENT ON TABLE analytics.official_skater_games IS
    'Current official NHL skater totals for one completed game. Separate from event-derived facts; source_revision advances only when published values change.';
COMMENT ON TABLE analytics.official_goalie_games IS
    'Current official NHL goalie totals for one completed game, including decisions and shutouts. source_revision advances only when published values change.';
COMMENT ON COLUMN analytics.official_skater_games.source_observed_at IS
    'Most recent time PucksData successfully observed this row, whether or not its values changed.';
COMMENT ON COLUMN analytics.official_goalie_games.source_observed_at IS
    'Most recent time PucksData successfully observed this row, whether or not its values changed.';

-- A stable, long-form scoring contract for downstream applications. Values
-- remain numeric and zero-valued official facts are retained.
CREATE VIEW analytics.official_player_game_stats AS
SELECT game_id, player_id, season, game_type, team_abbrev, 'skater'::TEXT AS player_type,
       stat_code, stat_value, source_revision, source_observed_at, updated_at
FROM analytics.official_skater_games
CROSS JOIN LATERAL (VALUES
    ('goals', goals::DOUBLE PRECISION),
    ('assists', assists::DOUBLE PRECISION),
    ('shots', shots::DOUBLE PRECISION),
    ('power_play_points', pp_points::DOUBLE PRECISION),
    ('short_handed_points', sh_points::DOUBLE PRECISION),
    ('game_winning_goals', game_winning_goals::DOUBLE PRECISION),
    ('hits', hits::DOUBLE PRECISION),
    ('blocks', blocked_shots::DOUBLE PRECISION),
    ('plus_minus', plus_minus::DOUBLE PRECISION)
) AS stats(stat_code, stat_value)
WHERE stat_value IS NOT NULL
UNION ALL
SELECT game_id, player_id, season, game_type, team_abbrev, 'goalie'::TEXT,
       stat_code, stat_value, source_revision, source_observed_at, updated_at
FROM analytics.official_goalie_games
CROSS JOIN LATERAL (VALUES
    ('wins', wins::DOUBLE PRECISION),
    ('shutouts', shutouts::DOUBLE PRECISION),
    ('saves', saves::DOUBLE PRECISION)
) AS stats(stat_code, stat_value)
WHERE stat_value IS NOT NULL;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'pucksstudio_read') THEN
        GRANT SELECT ON analytics.official_skater_games,
                        analytics.official_goalie_games,
                        analytics.official_player_game_stats
        TO pucksstudio_read;
    END IF;
END $$;
