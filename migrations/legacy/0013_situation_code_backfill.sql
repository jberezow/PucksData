-- Recompute owner-relative strength and reconstruct situationCode for existing
-- rows after the orientation correction in 0012.

UPDATE events AS e
SET strength = CASE
        WHEN e.event_owner_team_id = g.home_team_id THEN
            CASE SIGN(
                (e.home_skater_count - CASE WHEN e.home_goalie_present THEN 0 ELSE 1 END)
              - (e.away_skater_count - CASE WHEN e.away_goalie_present THEN 0 ELSE 1 END)
            )
                WHEN 1 THEN 'pp'
                WHEN -1 THEN 'sh'
                ELSE 'ev'
            END
        WHEN e.event_owner_team_id = g.away_team_id THEN
            CASE SIGN(
                (e.away_skater_count - CASE WHEN e.away_goalie_present THEN 0 ELSE 1 END)
              - (e.home_skater_count - CASE WHEN e.home_goalie_present THEN 0 ELSE 1 END)
            )
                WHEN 1 THEN 'pp'
                WHEN -1 THEN 'sh'
                ELSE 'ev'
            END
        ELSE NULL
    END,
    situation_code =
           CASE WHEN e.away_goalie_present THEN '1' ELSE '0' END
        || e.away_skater_count::text
        || e.home_skater_count::text
        || CASE WHEN e.home_goalie_present THEN '1' ELSE '0' END
FROM games AS g
WHERE g.game_id = e.game_id;
