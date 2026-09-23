-- NHL source IDs and franchise IDs are different namespaces. Keep historical
-- NHL identities so consumers can resolve raw shifts without changing source rows.
-- No franchise FK: source identities can precede ingestion of the teams archive.
CREATE TABLE nhl_team_identities (
    nhl_team_id BIGINT PRIMARY KEY,
    franchise_id BIGINT NOT NULL,
    abbrev TEXT NOT NULL,
    full_name TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_nhl_team_identities_franchise ON nhl_team_identities(franchise_id);

-- Bootstrap existing installations without reloading shifts. Source:
-- https://api.nhle.com/stats/rest/en/team?limit=-1 (2026-09-23).
-- `fetch teams` refreshes this mapping, including future NHL identities.
INSERT INTO nhl_team_identities (nhl_team_id, franchise_id, abbrev, full_name) VALUES
    (1, 23, 'NJD', 'New Jersey Devils'),
    (2, 22, 'NYI', 'New York Islanders'),
    (3, 10, 'NYR', 'New York Rangers'),
    (4, 16, 'PHI', 'Philadelphia Flyers'),
    (5, 17, 'PIT', 'Pittsburgh Penguins'),
    (6, 6, 'BOS', 'Boston Bruins'),
    (7, 19, 'BUF', 'Buffalo Sabres'),
    (8, 1, 'MTL', 'Montréal Canadiens'),
    (9, 30, 'OTT', 'Ottawa Senators'),
    (10, 5, 'TOR', 'Toronto Maple Leafs'),
    (11, 35, 'ATL', 'Atlanta Thrashers'),
    (12, 26, 'CAR', 'Carolina Hurricanes'),
    (13, 33, 'FLA', 'Florida Panthers'),
    (14, 31, 'TBL', 'Tampa Bay Lightning'),
    (15, 24, 'WSH', 'Washington Capitals'),
    (16, 11, 'CHI', 'Chicago Blackhawks'),
    (17, 12, 'DET', 'Detroit Red Wings'),
    (18, 34, 'NSH', 'Nashville Predators'),
    (19, 18, 'STL', 'St. Louis Blues'),
    (20, 21, 'CGY', 'Calgary Flames'),
    (21, 27, 'COL', 'Colorado Avalanche'),
    (22, 25, 'EDM', 'Edmonton Oilers'),
    (23, 20, 'VAN', 'Vancouver Canucks'),
    (24, 32, 'ANA', 'Anaheim Ducks'),
    (25, 15, 'DAL', 'Dallas Stars'),
    (26, 14, 'LAK', 'Los Angeles Kings'),
    (27, 28, 'PHX', 'Phoenix Coyotes'),
    (28, 29, 'SJS', 'San Jose Sharks'),
    (29, 36, 'CBJ', 'Columbus Blue Jackets'),
    (30, 37, 'MIN', 'Minnesota Wild'),
    (31, 15, 'MNS', 'Minnesota North Stars'),
    (32, 27, 'QUE', 'Quebec Nordiques'),
    (33, 28, 'WIN', 'Winnipeg Jets (1979)'),
    (34, 26, 'HFD', 'Hartford Whalers'),
    (35, 23, 'CLR', 'Colorado Rockies'),
    (36, 3, 'SEN', 'Ottawa Senators (1917)'),
    (37, 4, 'HAM', 'Hamilton Tigers'),
    (38, 9, 'PIR', 'Pittsburgh Pirates'),
    (39, 9, 'QUA', 'Philadelphia Quakers'),
    (40, 12, 'DCG', 'Detroit Cougars'),
    (41, 2, 'MWN', 'Montreal Wanderers'),
    (42, 4, 'QBD', 'Quebec Bulldogs'),
    (43, 7, 'MMR', 'Montreal Maroons'),
    (44, 8, 'NYA', 'New York Americans'),
    (45, 3, 'SLE', 'St. Louis Eagles'),
    (46, 13, 'OAK', 'Oakland Seals'),
    (47, 21, 'AFM', 'Atlanta Flames'),
    (48, 23, 'KCS', 'Kansas City Scouts'),
    (49, 13, 'CLE', 'Cleveland Barons'),
    (50, 12, 'DFL', 'Detroit Falcons'),
    (51, 8, 'BRK', 'Brooklyn Americans'),
    (52, 35, 'WPG', 'Winnipeg Jets'),
    (53, 28, 'ARI', 'Arizona Coyotes'),
    (54, 38, 'VGK', 'Vegas Golden Knights'),
    (55, 39, 'SEA', 'Seattle Kraken'),
    (56, 13, 'CGS', 'California Golden Seals'),
    (57, 5, 'TAN', 'Toronto Arenas'),
    (58, 5, 'TSP', 'Toronto St. Patricks'),
    (59, 40, 'UTA', 'Utah Hockey Club'),
    (68, 40, 'UTA', 'Utah Mammoth');

-- Latest attempt is separate from the stored snapshot. An unavailable refresh
-- must not erase usable rows or falsely report that the snapshot was refreshed.
CREATE TABLE shift_fetch_status (
    game_id BIGINT PRIMARY KEY REFERENCES games(game_id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('loaded', 'unavailable', 'failed')),
    attempted_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE shift_fetch_status IS
    'Latest shift-fetch outcome. Older loaders do not populate this table; absence means no recorded attempt, not no attempt ever.';
COMMENT ON TABLE nhl_team_identities IS
    'NHL source team identities mapped to franchise IDs used by games and teams. Names and abbreviations describe the NHL identity, not necessarily the current franchise.';

UPDATE analytics.coverage SET kind = 'measure', first_season = 20102011,
    note = 'Raw NHL shift rows from 2010-11, loaded by season. Source gaps and invalid intervals exist; availability does not certify complete games or reconstructed line combinations.'
WHERE subject = 'shifts';
UPDATE analytics.coverage SET
    note = 'Official skater season averages and goalie totals are available. Raw shifts from 2010-11 support separate validated ice-time reconstruction; events alone do not provide continuous ice time.'
WHERE subject = 'time_on_ice';

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'pucksstudio_read') THEN
        GRANT SELECT ON nhl_team_identities, shift_fetch_status, shifts TO pucksstudio_read;
    END IF;
END $$;
