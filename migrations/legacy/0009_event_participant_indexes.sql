-- Player-role lookups across non-scoring event tables.
--
-- These partial indexes support player participation and season discovery
-- without retaining null entries for events where a role was unavailable.

CREATE INDEX idx_hits_hitting_player ON hits(hitting_player_id)
WHERE hitting_player_id IS NOT NULL;

CREATE INDEX idx_hits_hittee_player ON hits(hittee_player_id)
WHERE hittee_player_id IS NOT NULL;

CREATE INDEX idx_blocks_blocking_player ON blocks(blocking_player_id)
WHERE blocking_player_id IS NOT NULL;

CREATE INDEX idx_blocks_shooting_player ON blocks(shooting_player_id)
WHERE shooting_player_id IS NOT NULL;

CREATE INDEX idx_penalties_committed_by_player ON penalties(committed_by_player_id)
WHERE committed_by_player_id IS NOT NULL;

CREATE INDEX idx_penalties_drawn_by_player ON penalties(drawn_by_player_id)
WHERE drawn_by_player_id IS NOT NULL;

CREATE INDEX idx_faceoffs_winning_player ON faceoffs(winning_player_id)
WHERE winning_player_id IS NOT NULL;

CREATE INDEX idx_faceoffs_losing_player ON faceoffs(losing_player_id)
WHERE losing_player_id IS NOT NULL;
