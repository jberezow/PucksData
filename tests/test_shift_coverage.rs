mod common;

#[tokio::test]
async fn shift_coverage_preserves_zero_counts_status_precedence_and_unsupported_seasons() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let ids: Vec<i64> = (1900020890..=1900020898).collect();
    sqlx::query("DELETE FROM games WHERE game_id = ANY($1)")
        .bind(&ids)
        .execute(pool)
        .await
        .unwrap();
    for (id, abbrev) in [(38_i64, "VGK"), (39, "SEA")] {
        sqlx::query(
            "INSERT INTO teams (team_id,full_name,common_name,place_name,abbrev)
                     VALUES ($1,$2,$2,'Test',$2) ON CONFLICT (team_id) DO NOTHING",
        )
        .bind(id)
        .bind(abbrev)
        .execute(pool)
        .await
        .unwrap();
    }
    for (index, id) in ids.iter().enumerate() {
        sqlx::query("INSERT INTO games (game_id,season,game_date,home_team_id,away_team_id,game_type,game_state)
                     VALUES ($1,$2,'2099-10-07',38,39,$3,$4)")
            .bind(id)
            .bind(if index == 6 { 19001901 } else { 20992100 })
            .bind(if index == 8 { 1_i16 } else { 2_i16 })
            .bind(if index == 7 { "FUT" } else { "OFF" })
            .execute(pool).await.unwrap();
    }
    for (index, status) in [
        (1, "unavailable"),
        (2, "failed"),
        (4, "failed"),
        (5, "loaded"),
    ] {
        sqlx::query("INSERT INTO shift_fetch_status (game_id,status) VALUES ($1,$2)")
            .bind(ids[index])
            .bind(status)
            .execute(pool)
            .await
            .unwrap();
    }
    for (game, shift) in [(ids[0], 1_i64), (ids[0], 2), (ids[4], 1)] {
        sqlx::query("INSERT INTO shifts (game_id,source_shift_id,type_code) VALUES ($1,$2,517)")
            .bind(game)
            .bind(shift)
            .execute(pool)
            .await
            .unwrap();
    }
    let rows: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT game_id,shift_rows,availability FROM observability.shift_game_coverage
         WHERE game_id = ANY($1) ORDER BY game_id",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            (ids[0], 2, "loaded".into()),
            (ids[1], 0, "unavailable".into()),
            (ids[2], 0, "failed".into()),
            (ids[3], 0, "no_stored_shifts".into()),
            (ids[4], 1, "loaded".into()), // stored snapshot survives a failed refresh
            (ids[5], 0, "no_stored_shifts".into()), // outcome alone does not imply stored data
            (ids[6], 0, "unsupported".into()),
        ]
    );
    let summary: (i64, i64, i64, i64, i64, i64, i64, Option<f64>) = sqlx::query_as(
        "SELECT eligible_games,unsupported_games,loaded_games,unavailable_games,failed_games,
                missing_games,shift_rows::bigint,loaded_fraction
         FROM observability.shift_season_coverage WHERE season=20992100 AND game_type=2",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(summary, (6, 0, 2, 1, 1, 2, 3, Some(2.0 / 6.0)));
    let unsupported: (i64, i64, i64, Option<f64>) = sqlx::query_as(
        "SELECT eligible_games,unsupported_games,shift_rows::bigint,loaded_fraction
         FROM observability.shift_season_coverage WHERE season=19001901 AND game_type=2",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(unsupported, (0, 1, 0, None));
    sqlx::query("DELETE FROM games WHERE game_id = ANY($1)")
        .bind(&ids)
        .execute(pool)
        .await
        .unwrap();
}
