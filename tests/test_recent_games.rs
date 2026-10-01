mod common;
use sqlx::Row;

#[tokio::test]
async fn polling_retries_incomplete_and_failed_reports_without_reloading_fresh_finals() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let ids: Vec<i64> = (1900020901..=1900020906).collect();
    sqlx::query("INSERT INTO teams(team_id,full_name,common_name,place_name,abbrev) VALUES (99993,'Test','Test','Test','RGA'),(99994,'Test','Test','Test','RGB') ON CONFLICT DO NOTHING").execute(pool).await.unwrap();
    for (i, id) in ids.iter().enumerate() {
        sqlx::query("INSERT INTO games(game_id,season,game_date,start_time_utc,home_team_id,away_team_id,game_type,game_state) VALUES ($1,19001901,CURRENT_DATE,clock_timestamp()-interval '3 hours',99993,99994,2,$2)")
            .bind(id).bind(if i==0 {"LIVE"} else {"OFF"}).execute(pool).await.unwrap();
    }
    // 0 live; 1 no reports; 2 fresh complete; 3 failed correction after success;
    // 4 old accepted report; 5 not yet started.
    for i in [2, 3, 4] {
        sqlx::query("INSERT INTO ingestion.attempts(dataset,entity_key,outcome,finished_at,engine_version) VALUES ('official_games',$1,'complete',clock_timestamp()-make_interval(mins=>$2),'test')")
            .bind(ids[i].to_string()).bind(if i==4 {90i32} else {1i32}).execute(pool).await.unwrap();
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
