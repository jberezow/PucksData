\set ON_ERROR_STOP on
BEGIN;
GRANT USAGE ON SCHEMA public, analytics, observability TO :"reader_role";
GRANT SELECT ON ALL TABLES IN SCHEMA public, analytics, observability TO :"reader_role";
GRANT SELECT ON public.sync_state TO :"reader_role" WITH GRANT OPTION;
GRANT SELECT ON public.backfill_progress TO PUBLIC;
GRANT USAGE ON SCHEMA public TO :"column_role";
GRANT SELECT(game_id,status) ON public.shift_fetch_status TO :"column_role";
GRANT USAGE ON SCHEMA public TO :"default_role";
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT ON TABLES TO :"default_role";
GRANT USAGE ON SCHEMA public, ingestion TO :"writer_role";
GRANT SELECT, INSERT, UPDATE, DELETE ON public.backfill_progress, public.sync_state, public.shift_fetch_status TO :"writer_role";
GRANT SELECT, INSERT ON ingestion.derived_invalidations TO :"writer_role";
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA ingestion TO :"writer_role";

INSERT INTO public.teams(team_id,full_name,common_name,place_name,abbrev)
VALUES(99681,'Upgrade Home','Home','Test','UPH'),(99682,'Upgrade Away','Away','Test','UPA');
INSERT INTO public.players(player_id,first_name,last_name,position,headshot_url)
VALUES(99685,'Test','Skater','C','https://example.invalid/skater'),(99686,'Test','Goalie','G',null);
INSERT INTO public.seasons(season_year) VALUES(20252026);
INSERT INTO public.games(game_id,season,game_date,start_time_utc,home_team_id,away_team_id,game_type,game_state,home_score,away_score)
VALUES(9968000001,20252026,'2026-01-01','2026-01-01 20:00Z',99681,99682,2,'OFF',3,2);
INSERT INTO public.roster_snapshots(source,team_count,player_count) VALUES('upgrade-test',2,2) RETURNING snapshot_id \gset
INSERT INTO public.roster_memberships(snapshot_id,team_id,player_id,roster_group,position_code)
VALUES(:snapshot_id,99681,99685,'forward','C'),(:snapshot_id,99682,99686,'goalie','G');
INSERT INTO analytics.official_skater_seasons(player_id,season,game_type,full_name,games_played,goals,points)
VALUES(99685,20252026,2,'Test Skater',1,1,2);
INSERT INTO analytics.official_goalie_seasons(player_id,season,game_type,full_name,games_played,wins,saves)
VALUES(99686,20252026,2,'Test Goalie',1,1,20);
INSERT INTO analytics.official_skater_games(game_id,player_id,season,game_type,full_name,goals,points)
VALUES(9968000001,99685,20252026,2,'Test Skater',1,2);
INSERT INTO analytics.official_goalie_games(game_id,player_id,season,game_type,full_name,wins,saves)
VALUES(9968000001,99686,20252026,2,'Test Goalie',1,20);
SELECT history.record('official_games','9968000001','{"scoring":[{"player_id":99685,"stat_code":"assists","stat_value":1}]}');
SELECT history.record('official_games','9968000001','{"scoring":[]}');
INSERT INTO public.backfill_progress(game_id,season,status) VALUES(9968000001,20252026,'done');
INSERT INTO public.sync_state(key,last_sync_at,last_sync_games) VALUES('singleton','2026-01-02',1);
INSERT INTO public.shift_fetch_status(game_id,status) VALUES(9968000001,'loaded');
INSERT INTO public.shifts(game_id,source_shift_id,type_code,player_id,team_id) VALUES(9968000001,1,517,99685,1);
COMMIT;
