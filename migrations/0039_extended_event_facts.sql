CREATE TABLE public.missed_shots (
    event_id BIGINT PRIMARY KEY REFERENCES public.events(id),
    shooting_player_id BIGINT,
    goalie_in_net_id BIGINT,
    shot_type TEXT,
    miss_reason TEXT
);
CREATE TABLE public.giveaways (
    event_id BIGINT PRIMARY KEY REFERENCES public.events(id),
    player_id BIGINT
);
CREATE TABLE public.takeaways (
    event_id BIGINT PRIMARY KEY REFERENCES public.events(id),
    player_id BIGINT
);
CREATE INDEX idx_missed_shots_shooter ON public.missed_shots(shooting_player_id);
CREATE INDEX idx_giveaways_player ON public.giveaways(player_id);
CREATE INDEX idx_takeaways_player ON public.takeaways(player_id);
COMMENT ON TABLE public.missed_shots IS 'Missed attempts, excluded from shots on goal. Attribution is nullable when absent from the source.';
COMMENT ON COLUMN public.missed_shots.miss_reason IS 'NHL play-by-play details.reason, preserved verbatim when supplied.';

CREATE VIEW analytics.event_fact_coverage AS
SELECT e.season, e.game_type, e.event_type,
       count(*) AS base_event_count,
       count(COALESCE(ms.event_id, gv.event_id, tk.event_id)) AS typed_event_count,
       count(COALESCE(ms.shooting_player_id, gv.player_id, tk.player_id)) AS attributed_event_count,
       count(*) - count(COALESCE(ms.event_id, gv.event_id, tk.event_id)) AS missing_typed_event_count,
       count(DISTINCT e.game_id) AS games_with_base_events
FROM public.events e
LEFT JOIN public.missed_shots ms ON ms.event_id = e.id
LEFT JOIN public.giveaways gv ON gv.event_id = e.id
LEFT JOIN public.takeaways tk ON tk.event_id = e.id
WHERE e.event_type IN ('missed-shot', 'giveaway', 'takeaway')
GROUP BY e.season, e.game_type, e.event_type;
COMMENT ON VIEW analytics.event_fact_coverage IS 'Observed typed attribution for extended event facts. Distinct from declared base-event eras in analytics.coverage: no rows or complete typed rows do not establish full schedule/feed coverage. NULL player attribution is not a zero count.';

-- Existing installations may have explicit grants instead of default ACLs.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'pucksdata_ingest') THEN
        GRANT USAGE ON SCHEMA public TO pucksdata_ingest;
        GRANT SELECT, INSERT, UPDATE, DELETE ON public.missed_shots, public.giveaways, public.takeaways TO pucksdata_ingest;
    END IF;
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'pucksstudio_read') THEN
        GRANT USAGE ON SCHEMA public, analytics TO pucksstudio_read;
        GRANT SELECT ON public.missed_shots, public.giveaways, public.takeaways, analytics.event_fact_coverage TO pucksstudio_read;
    END IF;
END $$;

UPDATE analytics.coverage
SET note = 'Team identifiers use franchise identities rather than season-specific historical names. Consult nhl_team_identities for source NHL team mappings; historical aliases are not a separate naming dimension.'
WHERE subject = 'historical_team_names';
