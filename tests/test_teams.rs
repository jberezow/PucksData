#[tokio::test]
async fn test_teams_upsert_idempotent() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let record = pucksdata::models::DbTeam {
        team_id: 999999,
        full_name: "Test Team".into(),
        common_name: "Tests".into(),
        place_name: "Testville".into(),
        abbrev: "TST".into(),
    };
    pucksdata::loaders::teams::upsert_teams(
        pool,
        std::slice::from_ref(&record),
        &indicatif::ProgressBar::hidden(),
    )
    .await
    .unwrap();
    let before: String = sqlx::query_scalar("SELECT xmin::text FROM teams WHERE team_id=999999")
        .fetch_one(pool)
        .await
        .unwrap();
    pucksdata::loaders::teams::upsert_teams(
        pool,
        std::slice::from_ref(&record),
        &indicatif::ProgressBar::hidden(),
    )
    .await
    .unwrap();
    let after: String = sqlx::query_scalar("SELECT xmin::text FROM teams WHERE team_id=999999")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(
        before, after,
        "unchanged metadata must not rewrite the tuple"
    );

    pucksdata::loaders::teams::upsert_teams(
        pool,
        &[pucksdata::models::DbTeam {
            team_id: 999999,
            full_name: "Test Team Updated".into(),
            common_name: "Tests".into(),
            place_name: "Testville".into(),
            abbrev: "TST".into(),
        }],
        &indicatif::ProgressBar::hidden(),
    )
    .await
    .unwrap();
    let count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM teams WHERE team_id = 999999")
        .fetch_one(pool)
        .await
        .unwrap()
        .unwrap_or(0);
    assert_eq!(count, 1, "upsert produced more than one row");
    let name: String = sqlx::query_scalar!("SELECT full_name FROM teams WHERE team_id = 999999")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(name, "Test Team Updated");
    sqlx::query!("DELETE FROM teams WHERE team_id = 999999")
        .execute(pool)
        .await
        .unwrap();
}
mod common;

#[tokio::test]
async fn identity_refresh_rejects_drift_before_any_upsert() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    sqlx::query("INSERT INTO nhl_team_identities(nhl_team_id,franchise_id,abbrev,full_name) VALUES(990033,28,'OLD','Old identity')")
        .execute(pool).await.unwrap();
    let rows: Vec<(i64, i64, String, String)> = sqlx::query_as(
        "SELECT nhl_team_id,franchise_id,abbrev,full_name FROM nhl_team_identities ORDER BY nhl_team_id",
    ).fetch_all(pool).await.unwrap();
    let mut source: Vec<_> = rows
        .into_iter()
        .map(
            |(id, franchise_id, tri_code, full_name)| pucksdata::fetchers::teams::TeamIdentity {
                id,
                franchise_id: Some(franchise_id),
                tri_code,
                full_name,
            },
        )
        .collect();
    source.insert(
        0,
        pucksdata::fetchers::teams::TeamIdentity {
            id: 990052,
            franchise_id: Some(35),
            tri_code: "NEW".into(),
            full_name: "New identity".into(),
        },
    );
    source
        .iter_mut()
        .find(|row| row.id == 990033)
        .unwrap()
        .franchise_id = Some(35);
    let error = pucksdata::loaders::teams::upsert_team_identities(pool, &source)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("990033"));
    let state: Vec<(i64,i64)> = sqlx::query_as("SELECT nhl_team_id,franchise_id FROM nhl_team_identities WHERE nhl_team_id IN (990033,990052)")
        .fetch_all(pool).await.unwrap();
    assert_eq!(state, vec![(990033, 28)]);

    // A stable affiliation permits new source identities and metadata refreshes.
    source
        .iter_mut()
        .find(|row| row.id == 990033)
        .unwrap()
        .franchise_id = Some(28);
    pucksdata::loaders::teams::upsert_team_identities(pool, &source)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM nhl_team_identities WHERE nhl_team_id IN (990033,990052)",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(count, 2);
    let before: Vec<String> =
        sqlx::query_scalar("SELECT xmin::text FROM nhl_team_identities ORDER BY nhl_team_id")
            .fetch_all(pool)
            .await
            .unwrap();
    pucksdata::loaders::teams::upsert_team_identities(pool, &source)
        .await
        .unwrap();
    let after: Vec<String> =
        sqlx::query_scalar("SELECT xmin::text FROM nhl_team_identities ORDER BY nhl_team_id")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(
        before, after,
        "stable identity checks must not rewrite metadata"
    );
    sqlx::query("DELETE FROM nhl_team_identities WHERE nhl_team_id IN (990033,990052)")
        .execute(pool)
        .await
        .unwrap();
}
