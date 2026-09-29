-- Player-role lookups used by downstream event analytics.
--
-- Scorer and shooter indexes were created with the event-type tables. These
-- additional roles support assist and goaltending queries without scanning
-- the complete goals or shots tables.

CREATE INDEX idx_goals_assist1 ON goals(assist1_player_id)
WHERE assist1_player_id IS NOT NULL;

CREATE INDEX idx_goals_assist2 ON goals(assist2_player_id)
WHERE assist2_player_id IS NOT NULL;

CREATE INDEX idx_goals_goalie ON goals(goalie_id)
WHERE goalie_id IS NOT NULL;

CREATE INDEX idx_shots_goalie_in_net ON shots(goalie_in_net_id)
WHERE goalie_in_net_id IS NOT NULL;
