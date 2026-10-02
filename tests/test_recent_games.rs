mod common;
use sqlx::Row;

#[tokio::test]
async fn polling_retries_incomplete_and_failed_reports_without_reloading_fresh_finals() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let ids: Vec<i64> = (1900020901..=1900020907).collect();
    sqlx::query("INSERT INTO teams(team_id,full_name,common_name,place_name,abbrev) VALUES (99993,'Test','Test','Test','RGA'),(99994,'Test','Test','Test','RGB') ON CONFLICT DO NOTHING").execute(pool).await.unwrap();
    for (i, id) in ids.iter().enumerate() {
        sqlx::query("INSERT INTO games(game_id,season,game_date,start_time_utc,home_team_id,away_team_id,game_type,game_state) VALUES ($1,19001901,CURRENT_DATE,clock_timestamp()-interval '3 hours',99993,99994,2,$2)")
            .bind(id).bind(if i==0 {"LIVE"} else {"OFF"}).execute(pool).await.unwrap();
    }
    // 0 live; 1 no reports; 2 fresh complete; 3 failed correction after success;
    // 4 old accepted report; 5 not yet started; 6 accepted two hours ago.
    for i in [2, 3, 4, 6] {
        sqlx::query("INSERT INTO ingestion.attempts(dataset,entity_key,outcome,finished_at,engine_version) VALUES ('official_games',$1,'complete',clock_timestamp()-make_interval(mins=>$2),'test')")
            .bind(ids[i].to_string()).bind(if i==4 {420i32} else if i==6 {120i32} else {1i32}).execute(pool).await.unwrap();
    }
    sqlx::query("INSERT INTO ingestion.attempts(dataset,entity_key,outcome,finished_at,error_message,engine_version) VALUES ('official_games',$1,'failed',clock_timestamp(),'incomplete test report','test')")
        .bind(ids[3].to_string()).execute(pool).await.unwrap();
    sqlx::query(
        "UPDATE games SET start_time_utc=clock_timestamp()+interval '1 hour' WHERE game_id=$1",
    )
    .bind(ids[5])
    .execute(pool)
    .await
    .unwrap();
    let selected: Vec<i64> = pucksdata::process::recent_games::candidates(pool)
        .await
        .unwrap()
        .iter()
        .map(|r| r.get("game_id"))
        .filter(|id| ids.contains(id))
        .collect();
    assert_eq!(selected, vec![ids[0], ids[1], ids[3], ids[4]]);
    sqlx::query(
        "DELETE FROM ingestion.attempts WHERE engine_version='test' AND entity_key=ANY($1)",
    )
    .bind(ids.iter().map(ToString::to_string).collect::<Vec<_>>())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM games WHERE game_id=ANY($1)")
        .bind(&ids)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn daily_recovers_old_missing_reports_only_in_active_seasons() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let ids: Vec<i64> = (1900020911..=1900020915).collect();
    sqlx::query("INSERT INTO teams(team_id,full_name,common_name,place_name,abbrev) VALUES (99995,'Test','Test','Test','RGC'),(99996,'Test','Test','Test','RGD') ON CONFLICT DO NOTHING").execute(pool).await.unwrap();
    for (i, id) in ids.iter().enumerate() {
        sqlx::query("INSERT INTO games(game_id,season,game_date,home_team_id,away_team_id,game_type,game_state) VALUES ($1,$2,CURRENT_DATE-30,99995,99996,2,$3)")
            .bind(id).bind(if i==3 {18991900} else {19001901}).bind(if i==4 {"FUT"} else {"OFF"}).execute(pool).await.unwrap();
    }
    // Missing and failed old reports recover; accepted old reports, historical
    // seasons and unplayed games are excluded.
    for (i, outcome) in [(1, "failed"), (2, "complete")] {
        sqlx::query("INSERT INTO ingestion.attempts(dataset,entity_key,outcome,finished_at,engine_version) VALUES ('official_games',$1,$2,now(),'test')")
            .bind(ids[i].to_string()).bind(outcome).execute(pool).await.unwrap();
    }
    let selected = pucksdata::process::official_games::query_current_candidates(
        pool,
        time::OffsetDateTime::now_utc().date() - time::Duration::days(3),
        &[19001901],
    )
    .await
    .unwrap();
    let selected: Vec<_> = selected
        .into_iter()
        .map(|(id, _)| id)
        .filter(|id| ids.contains(id))
        .collect();
    assert_eq!(selected, vec![ids[0], ids[1]]);
    sqlx::query("DELETE FROM ingestion.attempts WHERE entity_key=ANY($1)")
        .bind(ids.iter().map(ToString::to_string).collect::<Vec<_>>())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM games WHERE game_id=ANY($1)")
        .bind(&ids)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn daily_success_uses_a_separate_consumer_watermark() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    pucksdata::process::recent_games::record_daily_success(pool, 7)
        .await
        .unwrap();
    let row: (i32,bool) = sqlx::query_as("SELECT last_sync_games,last_sync_at > now()-interval '1 minute' FROM public.sync_state WHERE key='official_games'")
        .fetch_one(pool).await.unwrap();
    assert_eq!(row, (7, true));
    pucksdata::process::recent_games::record_daily_success(pool, 0)
        .await
        .unwrap();
    let games: i32 = sqlx::query_scalar(
        "SELECT last_sync_games FROM public.sync_state WHERE key='official_games'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(games, 0);
    sqlx::query("DELETE FROM ingestion.sync_state WHERE key='official_games'")
        .execute(pool)
        .await
        .unwrap();
}
