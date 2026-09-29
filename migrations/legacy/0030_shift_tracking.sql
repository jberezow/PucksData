-- Typed rows from the NHL shift-chart endpoint, without canonicalization.
CREATE TABLE shifts (
    game_id               BIGINT      NOT NULL REFERENCES games(game_id) ON DELETE CASCADE,
    source_shift_id       BIGINT      NOT NULL,
    type_code             INTEGER     NOT NULL CHECK (type_code = 517),
    player_id             BIGINT,
    team_id               BIGINT,
    period                SMALLINT,
    shift_number          INTEGER,
    start_time            TEXT,
    end_time              TEXT,
    duration              TEXT,
    start_time_seconds    INTEGER,
    end_time_seconds      INTEGER,
    duration_seconds      INTEGER,
    source_data           JSONB       NOT NULL,
    ingested_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (game_id, source_shift_id)
);

CREATE INDEX idx_shifts_player_game ON shifts(player_id, game_id);

COMMENT ON TABLE shifts IS
    'Raw typeCode 517 NHL shift-chart rows converted to typed columns without correction or deduplication.';
COMMENT ON COLUMN shifts.team_id IS
    'NHL teamId as supplied by the source; it is not translated to franchise identity.';
COMMENT ON COLUMN shifts.source_data IS
    'Complete source object retained for lossless future reprocessing.';
