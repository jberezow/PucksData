SELECT game_id, id, event_id_in_game, period, period_type, time_in_period,
       event_type, situation_code, strength_source, away_goalie_present,
       away_skater_count, home_skater_count, home_goalie_present
FROM events WHERE game_id = ANY($1::bigint[])
ORDER BY game_id, event_id_in_game, id
