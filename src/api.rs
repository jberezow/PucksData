use reqwest;
use std::fmt;

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

pub fn fetch_api_json(url: &str) -> Result<String, ApiError> {
    let response = reqwest::blocking::get(url)?;
    
    match response.status() {
        reqwest::StatusCode::OK => Ok(response.text()?),
        reqwest::StatusCode::NOT_FOUND => Err(ApiError::NotFound),
        status => Err(ApiError::Other(status.as_u16())),
    }
}
