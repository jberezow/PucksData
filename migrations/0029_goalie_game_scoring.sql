-- Publish goalie goals and assists through the same finalized player-game
-- contract used for all other fantasy scoring facts.

ALTER TABLE analytics.official_goalie_games
    ADD COLUMN goals   INTEGER,
    ADD COLUMN assists INTEGER;

COMMENT ON COLUMN analytics.official_goalie_games.goals IS
    'Official goals credited to the goalie in this game';
COMMENT ON COLUMN analytics.official_goalie_games.assists IS
    'Official assists credited to the goalie in this game';

CREATE OR REPLACE VIEW analytics.official_player_game_stats AS
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
    ('goals', goals::DOUBLE PRECISION),
    ('assists', assists::DOUBLE PRECISION),
    ('wins', wins::DOUBLE PRECISION),
    ('shutouts', shutouts::DOUBLE PRECISION),
    ('saves', saves::DOUBLE PRECISION)
) AS stats(stat_code, stat_value)
WHERE stat_value IS NOT NULL;
