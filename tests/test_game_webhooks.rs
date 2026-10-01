mod common;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn failed_delivery_retries_the_same_signed_event() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    // Lock unrelated pending events so this test never sends other fixture data.
    let mut isolation = pool.begin().await.unwrap();
    sqlx::query("SELECT event_id FROM ingestion.game_webhook_outbox FOR UPDATE")
        .fetch_all(&mut *isolation)
        .await
        .unwrap();
    let id: String = sqlx::query_scalar("INSERT INTO ingestion.game_webhook_outbox(game_id,revision,payload) VALUES (1900020999,1,'{\"game_id\":1}') RETURNING event_id::text")
        .fetch_one(pool).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    std::env::set_var(
        "PUCKSDATA_WEBHOOK_URL",
        format!("http://{}/", listener.local_addr().unwrap()),
    );
    std::env::set_var(
        "PUCKSDATA_WEBHOOK_SECRET",
        "webhook-test-key-32-bytes-minimum!",
    );
    let server = tokio::spawn(async move {
        for status in [503, 202] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut buf = [0; 4096];
                let size = stream.read(&mut buf).await.unwrap();
                assert!(size > 0);
                bytes.extend_from_slice(&buf[..size]);
                if let Some(pos) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..pos]);
                    let length: usize = headers
                        .lines()
                        .find_map(|h| {
                            h.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .map(|v| v.parse().unwrap())
                        })
                        .unwrap();
                    if bytes.len() >= pos + 4 + length {
                        break;
                    }
                }
            }
            let text = String::from_utf8(bytes).unwrap();
            let (headers, body) = text.split_once("\r\n\r\n").unwrap();
            let timestamp = headers
                .lines()
                .find_map(|h| h.strip_prefix("x-pucksdata-timestamp: "))
                .unwrap();
            let signature = headers
                .lines()
                .find_map(|h| h.strip_prefix("x-pucksdata-signature: v1="))
                .unwrap();
            assert_eq!(
                signature,
                pucksdata::webhooks::signature(
                    b"webhook-test-key-32-bytes-minimum!",
                    timestamp,
                    body.as_bytes()
                )
            );
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 {status} Test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    assert_eq!(pucksdata::webhooks::deliver(pool).await.unwrap(), 0);
    let (attempts,pending):(i32,bool)=sqlx::query_as("SELECT attempts,delivered_at IS NULL FROM ingestion.game_webhook_outbox WHERE event_id=$1::uuid")
        .bind(&id).fetch_one(pool).await.unwrap();
    assert_eq!((attempts, pending), (1, true));
    sqlx::query(
        "UPDATE ingestion.game_webhook_outbox SET available_at=now() WHERE event_id=$1::uuid",
    )
    .bind(&id)
    .execute(pool)
    .await
    .unwrap();
    assert_eq!(pucksdata::webhooks::deliver(pool).await.unwrap(), 1);
    assert_eq!(pucksdata::webhooks::deliver(pool).await.unwrap(), 0);
    server.await.unwrap();
    sqlx::query("DELETE FROM ingestion.game_webhook_outbox WHERE event_id=$1::uuid")
        .bind(&id)
        .execute(pool)
        .await
        .unwrap();
    isolation.rollback().await.unwrap();
}

#[tokio::test]
async fn event_is_invisible_until_commit_and_disappears_on_rollback() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO teams(team_id,full_name,common_name,place_name,abbrev) VALUES (99991,'Test','Test','Test','WHA'),(99992,'Test','Test','Test','WHB') ON CONFLICT DO NOTHING").execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO games(game_id,season,game_date,home_team_id,away_team_id,game_type,game_state) VALUES (1900020998,19001901,'1900-01-01',99991,99992,2,'OFF')").execute(&mut *tx).await.unwrap();
    sqlx::query("SELECT history.record('official_games','1900020998','{\"skaters\":[{}],\"goalies\":[{}]}'::jsonb)").execute(&mut *tx).await.unwrap();
    let staged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ingestion.game_webhook_outbox WHERE game_id=1900020998",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(staged, 1);
    let visible: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ingestion.game_webhook_outbox WHERE game_id=1900020998",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(visible, 0);
    tx.rollback().await.unwrap();
    let visible: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ingestion.game_webhook_outbox WHERE game_id=1900020998",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(visible, 0);
}
