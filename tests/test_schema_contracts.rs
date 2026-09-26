mod common;

/// Snapshot of the deployed consumer's relation/column/type contracts. A larger
/// storage migration must preserve these names (possibly through views) until
/// PucksPool has explicitly migrated. Do not regenerate this fixture blindly.
#[tokio::test]
async fn puckspool_read_contract_is_unchanged() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let contract: serde_json::Value =
        serde_json::from_str(include_str!("contracts/puckspool.json")).unwrap();
    for (relation, expected) in contract["relations"].as_object().unwrap() {
        let columns: Vec<(String, String)> = sqlx::query_as(
            "SELECT attname::text, format_type(atttypid,atttypmod)
             FROM pg_attribute WHERE attrelid=to_regclass($1)
               AND attnum>0 AND NOT attisdropped ORDER BY attnum",
        )
        .bind(relation)
        .fetch_all(pool)
        .await
        .unwrap();
        assert_eq!(serde_json::to_value(columns).unwrap(), *expected,
            "PucksPool contract changed: {relation}; coordinate a consumer migration before changing this fixture");
    }
}

#[tokio::test]
async fn logical_history_dataset_survives_table_rename_and_schema_move() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(
        "CREATE TABLE public.schema_history_fixture(source_id bigint PRIMARY KEY, value text);
        CREATE TRIGGER capture AFTER INSERT OR UPDATE OR DELETE ON public.schema_history_fixture
        FOR EACH ROW EXECUTE FUNCTION history.capture_entity('schema_contract_fixture','source_id');
        INSERT INTO public.schema_history_fixture VALUES(1,'before');
        ALTER TABLE public.schema_history_fixture RENAME TO schema_history_renamed;
        ALTER TABLE public.schema_history_renamed SET SCHEMA history;
        UPDATE history.schema_history_renamed SET value='before' WHERE source_id=1;
        UPDATE history.schema_history_renamed SET value='after' WHERE source_id=1;",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    let rows: Vec<(String, i64, String, i64)> = sqlx::query_as(
        "SELECT s.dataset,s.revision,s.payload->>'value',count(o.observation_id)
         FROM history.snapshots s JOIN history.observations o USING(snapshot_id)
         WHERE s.dataset='schema_contract_fixture' AND s.entity_key='1'
         GROUP BY s.snapshot_id ORDER BY s.revision",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            ("schema_contract_fixture".into(), 1, "before".into(), 2),
            ("schema_contract_fixture".into(), 2, "after".into(), 1)
        ]
    );
    sqlx::query("DELETE FROM history.schema_history_renamed WHERE source_id=1")
        .execute(&mut *tx)
        .await
        .unwrap();
    let deleted: bool = sqlx::query_scalar("SELECT (payload->>'deleted')::boolean FROM history.snapshots WHERE dataset='schema_contract_fixture' AND revision=3")
        .fetch_one(&mut *tx).await.unwrap();
    assert!(deleted);
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn explicit_identity_views_preserve_source_rows_and_disambiguate_keys() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql("INSERT INTO public.teams(team_id,full_name,common_name,place_name,abbrev)
        VALUES(99671,'Contract Home','Home','Test','COH'),(99672,'Contract Away','Away','Test','COA');
        INSERT INTO public.nhl_team_identities(nhl_team_id,franchise_id,abbrev,full_name)
        VALUES(99673,99671,'COH','Historical Home');
        INSERT INTO public.seasons(season_year) VALUES(99889989);
        INSERT INTO public.games(game_id,season,game_date,home_team_id,away_team_id,game_type)
        VALUES(9967000001,99889989,'2026-01-01',99671,99672,2);
        INSERT INTO public.shifts(game_id,source_shift_id,type_code,team_id,period,start_time_seconds,end_time_seconds)
        VALUES(9967000001,1,517,99673,1,20,10),(9967000001,2,517,99674,1,20,10);")
        .execute(&mut *tx).await.unwrap();
    let game: (i32,i64,i64) = sqlx::query_as("SELECT season_code,home_franchise_id,away_franchise_id FROM analytics.game_inventory WHERE game_id=9967000001")
        .fetch_one(&mut *tx).await.unwrap();
    assert_eq!(game, (99889989, 99671, 99672));
    type ShiftIdentity = (i64, Option<i64>, Option<i64>, Option<i32>, Option<i32>);
    let ids: Vec<ShiftIdentity> = sqlx::query_as(
        "SELECT source_shift_id,nhl_team_id,franchise_id,start_time_seconds,end_time_seconds
         FROM analytics.raw_shift_rows WHERE game_id=9967000001 ORDER BY source_shift_id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        ids,
        vec![
            (1, Some(99673), Some(99671), Some(20), Some(10)),
            (2, Some(99674), None, Some(20), Some(10))
        ]
    );
    let season: i32 = sqlx::query_scalar(
        "SELECT season_code FROM analytics.season_catalog WHERE season_code=99889989",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(season, 99889989);
    let franchise: i64 =
        sqlx::query_scalar("SELECT franchise_id FROM analytics.franchises WHERE abbrev='COH'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(franchise, 99671);
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn operational_tables_have_canonical_locations_and_read_only_legacy_views() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let tables: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
        WHERE n.nspname='ingestion' AND c.relkind='r' AND c.relname IN('backfill_progress','sync_state','shift_fetch_status')")
        .fetch_one(pool).await.unwrap();
    assert_eq!(tables, 3);
    let read_only: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.views WHERE table_schema='public'
        AND table_name IN('backfill_progress','sync_state','shift_fetch_status') AND is_updatable='NO' AND is_insertable_into='NO'")
        .fetch_one(pool).await.unwrap();
    assert_eq!(read_only, 3);
}
