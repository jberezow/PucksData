-- Official NHL per-player season totals, loaded from the stats API.
--
-- These are the league's own published aggregates, not values derived from
-- play-by-play. They serve two purposes: they answer season-level questions
-- the event schema cannot (games played from 1917-18, shots from 1967-68,
-- goalie wins and shutouts from 1917-18), and they are the reconciliation
-- oracle for event-derived figures.
--
-- They are deliberately kept in separate tables. Event-derived and
-- season-aggregate numbers must stay distinguishable, or a careless join
-- double counts.

COMMENT ON SCHEMA analytics IS
    'Read-only dataset metadata, derived facts, and official NHL season aggregates intended for downstream query engines';

CREATE TABLE analytics.official_skater_seasons (
    player_id            BIGINT   NOT NULL,
    season               INTEGER  NOT NULL,
    game_type            SMALLINT NOT NULL,
    full_name            TEXT     NOT NULL,
    position_code        TEXT,
    shoots_catches       TEXT,
    team_abbrevs         TEXT,
    games_played         INTEGER,
    goals                INTEGER,
    assists              INTEGER,
    points               INTEGER,
    plus_minus           INTEGER,
    penalty_minutes      INTEGER,
    shots                INTEGER,
    shooting_pct         DOUBLE PRECISION,
    ev_goals             INTEGER,
    ev_points            INTEGER,
    pp_goals             INTEGER,
    pp_points            INTEGER,
    sh_goals             INTEGER,
    sh_points            INTEGER,
    ot_goals             INTEGER,
    game_winning_goals   INTEGER,
    points_per_game      DOUBLE PRECISION,
    faceoff_win_pct      DOUBLE PRECISION,
    time_on_ice_per_game DOUBLE PRECISION,
    PRIMARY KEY (player_id, season, game_type)
);

CREATE INDEX idx_official_skater_seasons_season ON analytics.official_skater_seasons(season);

COMMENT ON TABLE analytics.official_skater_seasons IS
    'Official NHL skater totals per player, season, and game type. Not derived from events; use for season-level answers and for reconciling event-derived figures.';
COMMENT ON COLUMN analytics.official_skater_seasons.game_type IS
    '2 = regular season, 3 = playoffs';
COMMENT ON COLUMN analytics.official_skater_seasons.team_abbrevs IS
    'Comma-separated when the player appeared for more than one team, e.g. COL,CAR,DAL';
COMMENT ON COLUMN analytics.official_skater_seasons.shots IS
    'NULL before 1967-68; the NHL did not record shots on goal earlier';
COMMENT ON COLUMN analytics.official_skater_seasons.time_on_ice_per_game IS
    'Seconds per game; NULL before 1997-98';
COMMENT ON COLUMN analytics.official_skater_seasons.shooting_pct IS
    'Fraction, not percent: 0.06818 means 6.818%';

CREATE TABLE analytics.official_goalie_seasons (
    player_id              BIGINT   NOT NULL,
    season                 INTEGER  NOT NULL,
    game_type              SMALLINT NOT NULL,
    full_name              TEXT     NOT NULL,
    shoots_catches         TEXT,
    team_abbrevs           TEXT,
    games_played           INTEGER,
    games_started          INTEGER,
    wins                   INTEGER,
    losses                 INTEGER,
    ties                   INTEGER,
    ot_losses              INTEGER,
    shutouts               INTEGER,
    shots_against          INTEGER,
    saves                  INTEGER,
    goals_against          INTEGER,
    save_pct               DOUBLE PRECISION,
    goals_against_average  DOUBLE PRECISION,
    time_on_ice            BIGINT,
    goals                  INTEGER,
    assists                INTEGER,
    points                 INTEGER,
    penalty_minutes        INTEGER,
    PRIMARY KEY (player_id, season, game_type)
);

CREATE INDEX idx_official_goalie_seasons_season ON analytics.official_goalie_seasons(season);

COMMENT ON TABLE analytics.official_goalie_seasons IS
    'Official NHL goalie totals per player, season, and game type. Supplies the goalie record (wins, losses, shutouts) that play-by-play cannot establish.';
COMMENT ON COLUMN analytics.official_goalie_seasons.game_type IS
    '2 = regular season, 3 = playoffs';
COMMENT ON COLUMN analytics.official_goalie_seasons.time_on_ice IS
    'Total seconds';
COMMENT ON COLUMN analytics.official_goalie_seasons.save_pct IS
    'Fraction, not percent: 0.89728 means .897';

-- Several concepts the event schema cannot express are now answerable as
-- season aggregates. Reclassify them and record where the official data
-- itself begins.
UPDATE analytics.coverage
SET kind = 'measure',
    first_season = 19171918,
    note = 'Not derivable from events, which contain no lineups. Available as official season totals in analytics.official_skater_seasons and analytics.official_goalie_seasons from 1917-18.'
WHERE subject = 'games_played';

UPDATE analytics.coverage
SET kind = 'measure',
    first_season = 19671968,
    note = 'Not derivable from events. Available as an official season total in analytics.official_skater_seasons from 1967-68.'
WHERE subject = 'plus_minus';

UPDATE analytics.coverage
SET kind = 'measure',
    first_season = 19171918,
    note = 'Wins, losses, ties and shutouts are not derivable from events, which contain no goalie of record. Available as official season totals in analytics.official_goalie_seasons from 1917-18.'
WHERE subject = 'goalie_record';

UPDATE analytics.coverage
SET note = 'Event-derived time on ice is unavailable in any season. Official season averages exist in analytics.official_skater_seasons from 1997-98 and totals in analytics.official_goalie_seasons.'
WHERE subject = 'time_on_ice';

INSERT INTO analytics.coverage (subject, kind, first_season, note) VALUES
    ('official_skater_seasons', 'measure', 19171918,
     'Official NHL skater season totals. Games played, goals, assists, points, penalty minutes and game-winning goals from 1917-18; shots, plus-minus and power-play and shorthanded totals from 1967-68; time on ice and faceoff percentage from 1997-98.'),
    ('official_goalie_seasons', 'measure', 19171918,
     'Official NHL goalie season totals, including wins, losses, ties and shutouts from 1917-18. Shots against and save percentage are sparse in the earliest seasons.');
