-- Complete observations of the NHL's current team-roster endpoints.
--
-- A snapshot is inserted only when every active team's roster was fetched.
-- Keeping observations instead of a mutable `players.active` flag preserves
-- source history and prevents a partial upstream failure from making players
-- appear inactive.

CREATE TABLE roster_snapshots (
    snapshot_id  BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    observed_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    source        TEXT NOT NULL,
    team_count    INTEGER NOT NULL CHECK (team_count > 0),
    player_count  INTEGER NOT NULL CHECK (player_count > 0)
);

CREATE TABLE roster_memberships (
    snapshot_id    BIGINT NOT NULL REFERENCES roster_snapshots(snapshot_id) ON DELETE CASCADE,
    team_id        BIGINT NOT NULL REFERENCES teams(team_id),
    -- Player metadata fetches can fail independently of a roster fetch. Keep
    -- the authentic source identifier even when its landing page has not been
    -- repaired into `players` yet, matching the event participant tables.
    player_id      BIGINT NOT NULL,
    roster_group   TEXT NOT NULL CHECK (roster_group IN ('forward', 'defenseman', 'goalie')),
    position_code  TEXT,
    sweater_number SMALLINT,
    PRIMARY KEY (snapshot_id, team_id, player_id)
);

CREATE INDEX idx_roster_memberships_player
    ON roster_memberships(player_id, snapshot_id DESC);

CREATE VIEW analytics.current_rosters AS
SELECT
    snapshots.snapshot_id,
    snapshots.observed_at,
    teams.team_id,
    teams.abbrev AS team_abbrev,
    memberships.player_id,
    players.first_name,
    players.last_name,
    memberships.roster_group,
    memberships.position_code,
    memberships.sweater_number
FROM roster_memberships AS memberships
JOIN roster_snapshots AS snapshots
  ON snapshots.snapshot_id = memberships.snapshot_id
JOIN teams
  ON teams.team_id = memberships.team_id
LEFT JOIN players
  ON players.player_id = memberships.player_id
WHERE snapshots.snapshot_id = (SELECT MAX(snapshot_id) FROM roster_snapshots);

COMMENT ON TABLE roster_snapshots IS
    'Complete observations of all active NHL team rosters from the NHL web API.';
COMMENT ON TABLE roster_memberships IS
    'Player membership in an NHL current-roster observation; this is source data, not fantasy eligibility.';
COMMENT ON VIEW analytics.current_rosters IS
    'The latest complete NHL current-roster observation, intended for downstream consumers such as fantasy draft eligibility.';

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'pucksstudio_read') THEN
        GRANT SELECT ON roster_snapshots, roster_memberships TO pucksstudio_read;
        GRANT SELECT ON analytics.current_rosters TO pucksstudio_read;
    END IF;
END $$;
