pub mod client;

use reqwest;
use std::fmt;
use std::time::{Duration, Instant};
use tokio::time::sleep;

static mut LAST_REQUEST_TIME: Option<Instant> = None;
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(100); // 10 requests per second max

#[derive(Debug)]
pub enum ApiError {
    NotFound,
    NetworkError(reqwest::Error),
    Other(u16), // For other HTTP status codes
}

impl std::error::Error for ApiError {}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ApiError::NotFound => write!(f, "Resource not found (404)"),
            ApiError::NetworkError(e) => write!(f, "Network error: {}", e),
            ApiError::Other(code) => write!(f, "HTTP error: {}", code),
        }
    }
}

impl From<reqwest::Error> for ApiError {
    fn from(err: reqwest::Error) -> ApiError {
        ApiError::NetworkError(err)
    }
}

async fn enforce_rate_limit() {
    unsafe {
        if let Some(last_time) = LAST_REQUEST_TIME {
            let elapsed = last_time.elapsed();
            if elapsed < MIN_REQUEST_INTERVAL {
                let sleep_duration = MIN_REQUEST_INTERVAL - elapsed;
                sleep(sleep_duration).await;
            }
        }
        LAST_REQUEST_TIME = Some(Instant::now());
    }
}

pub async fn fetch_api_json(url: &str) -> Result<String, ApiError> {
    // Rate limiting to avoid overwhelming the API
    enforce_rate_limit().await;
    
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("User-Agent", "pucksdata/1.0 (Hockey Statistics Research)")
        .send()
        .await?;
    
    match response.status() {
        reqwest::StatusCode::OK => Ok(response.text().await?),
        reqwest::StatusCode::NOT_FOUND => Err(ApiError::NotFound),
        reqwest::StatusCode::TOO_MANY_REQUESTS => {
            println!("⚠️ Rate limited, waiting 5 seconds...");
            sleep(Duration::from_secs(5)).await;
            Err(ApiError::Other(429))
        },
        status => Err(ApiError::Other(status.as_u16())),
    }
}
