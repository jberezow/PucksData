-- no-transaction
-- PostgreSQL requires concurrent index creation outside a transaction. SQLx
-- recognizes the marker above and does not wrap this migration.

CREATE INDEX CONCURRENTLY idx_events_season_game_type_event_type
    ON events(season, game_type, event_type);
