//! Shared HTTP client and [`ApiError`] type for all NHL API requests.
use reqwest;
use std::fmt;
use std::sync::LazyLock;

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent("pucksdata/1.0 (Hockey Statistics Research)")
        .build()
        .expect("failed to build reqwest client")
});

/// Errors returned by NHL API fetch operations.
#[derive(Debug)]
pub enum ApiError {
    NotFound,
    NetworkError(reqwest::Error),
    Other(u16),
}

impl std::error::Error for ApiError {}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ApiError::NotFound => write!(f, "Resource not found (404)"),
            ApiError::NetworkError(e) => write!(f, "Network error: {e}"),
            ApiError::Other(code) => write!(f, "HTTP error: {code}"),
        }
    }
}

impl From<reqwest::Error> for ApiError {
    fn from(err: reqwest::Error) -> ApiError {
        ApiError::NetworkError(err)
    }
}

/// Fetch a URL via the shared HTTP client and deserialize the JSON response.
pub async fn fetch_api_json(url: &str) -> Result<String, ApiError> {
    let response = CLIENT.get(url).send().await?;

    match response.status() {
        reqwest::StatusCode::OK => Ok(response.text().await?),
        reqwest::StatusCode::NOT_FOUND => Err(ApiError::NotFound),
        status => Err(ApiError::Other(status.as_u16())),
    }
}
