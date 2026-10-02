//! Durable at-least-once delivery. One configured consumer per outbox.
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use sqlx::Row;
use std::time::{Duration, Instant};

pub fn signature(secret: &[u8], timestamp: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub async fn deliver(pool: &sqlx::PgPool) -> Result<usize, crate::AnyError> {
    let url = std::env::var("PUCKSDATA_WEBHOOK_URL")?;
    let secret = std::env::var("PUCKSDATA_WEBHOOK_SECRET")?;
    if secret.len() < 32 {
        return Err("webhook secret must contain at least 32 bytes".into());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut delivered = 0;
    // A row lock keeps concurrent dispatchers from sending the same event at once.
    // Crash after acceptance and before commit deliberately causes safe redelivery.
    let started = Instant::now();
    for _ in 0..100 {
        if started.elapsed() >= Duration::from_secs(40) {
            break;
        }
        let mut tx = pool.begin().await?;
        let row = sqlx::query(
            r#"SELECT event_id::text, payload::text, attempts
            FROM ingestion.game_webhook_outbox
            WHERE delivered_at IS NULL AND available_at <= clock_timestamp()
            ORDER BY available_at, created_at FOR UPDATE SKIP LOCKED LIMIT 1"#,
        )
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            break;
        };
        let id: String = row.get("event_id");
        let body: String = row.get("payload");
        let timestamp = time::OffsetDateTime::now_utc().unix_timestamp().to_string();
        let result = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-PucksData-Timestamp", &timestamp)
            .header(
                "X-PucksData-Signature",
                format!(
                    "v1={}",
                    signature(secret.as_bytes(), &timestamp, body.as_bytes())
                ),
            )
            .body(body)
            .send()
            .await;
        // Never persist response bodies or URLs: either may contain sensitive data.
        let error = match result {
            Ok(response) if response.status().is_success() => None,
            Ok(response) => Some(format!("HTTP {}", response.status().as_u16())),
            Err(_) => Some("delivery transport failure".to_owned()),
        };
        let attempts: i32 = row.get("attempts");
        let delay = (30_i64 * 2_i64.pow(attempts.clamp(0, 7) as u32)).min(3600);
        sqlx::query(
            r#"UPDATE ingestion.game_webhook_outbox
            SET attempts=attempts+1, last_error=$2,
                delivered_at=CASE WHEN $2::text IS NULL THEN clock_timestamp() ELSE NULL END,
                available_at=clock_timestamp()+make_interval(secs=>$3::double precision)
            WHERE event_id=$1::uuid"#,
        )
        .bind(&id)
        .bind(&error)
        .bind(delay as f64)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        if error.is_none() {
            delivered += 1;
        } else {
            tracing::warn!(event_id = id, error, "webhook delivery deferred");
        }
    }
    tracing::info!(delivered, "webhook delivery pass complete");
    Ok(delivered)
}

#[cfg(test)]
mod tests {
    #[test]
    fn signed_bytes_match_consumer_vector() {
        assert_eq!(
            super::signature(b"test-secret", "1700000000", br#"{"game_id":1}"#),
            "c5890d3e1b2db053a960637ee6062021e369e00b74754135645d357e7f03f8c7"
        );
    }
}
