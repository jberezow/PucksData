use pucksdata::ingest;
use dotenv::dotenv;

fn setup() {
    dotenv::dotenv().ok();
}

async fn setup_pool() -> pucksdata::db::DbPool {
    dotenv().ok();
    pucksdata::db::create_pool().await.expect("Failed to create pool")
}

#[tokio::test]
async fn test_fetch_endpoint_valid() {
    setup();
    let pool = setup_pool().await;
    let endpoint_name = "game_boxscore";
    let params = [("game_id", "2023020001")];

    let result = ingest::fetch_endpoint(endpoint_name, &params, pool).await;
    assert!(result.is_ok(), "fetch_endpoint failed: {:?}", result.err());
}

#[tokio::test]
async fn test_fetch_endpoint_invalid_param() {
    let pool = setup_pool().await;
    let endpoint_name = "game_boxscore";
    let params = [("invalid_param", "1234")]; // Missing game_id

    let result = ingest::fetch_endpoint(endpoint_name, &params, pool).await;
    assert!(result.is_err(), "fetch_endpoint should have failed");
}