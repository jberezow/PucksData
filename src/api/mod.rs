//! Shared HTTP client and [`ApiError`] type for all NHL API requests.
use reqwest;
use std::fmt;
use std::sync::LazyLock;

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent(concat!(
            "pucksdata/",
            env!("CARGO_PKG_VERSION"),
            " (Hockey Statistics Research)"
        ))
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("failed to build reqwest client")
});

/// Errors returned by NHL API fetch operations.
#[derive(Debug)]
pub enum ApiError {
    NotFound,
    NetworkError(reqwest::Error),
    Other(u16),
    Archive(String),
}

impl std::error::Error for ApiError {}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ApiError::Archive(error) => write!(f, "source capture failed: {error}"),
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

/// Fetch a URL via the shared HTTP client and return its response body.
pub async fn fetch_api_text(url: &str) -> Result<String, ApiError> {
    fetch_text_with_client(&CLIENT, url).await
}

struct RequestTimer {
    started: std::time::Instant,
    retry: bool,
}

impl Drop for RequestTimer {
    fn drop(&mut self) {
        crate::provenance::record_http(self.started.elapsed(), self.retry);
    }
}

async fn fetch_text_with_client(client: &reqwest::Client, url: &str) -> Result<String, ApiError> {
    for attempt in 0..3u32 {
        let timer = RequestTimer {
            started: std::time::Instant::now(),
            retry: attempt > 0,
        };
        let response = client.get(url).send().await;
        let mut delay = std::time::Duration::from_millis(500 * (1 << attempt));
        let result = match response {
            Ok(response) => {
                let status = response.status();
                if status.as_u16() == 429 || status.is_server_error() {
                    if let Some(value) = response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                    {
                        let retry_after = value
                            .parse::<u64>()
                            .ok()
                            .map(std::time::Duration::from_secs)
                            .or_else(|| {
                                chrono::DateTime::parse_from_rfc2822(value)
                                    .ok()
                                    .map(|date| {
                                        (date.with_timezone(&chrono::Utc) - chrono::Utc::now())
                                            .to_std()
                                            .unwrap_or_default()
                                    })
                            });
                        if let Some(retry_after) = retry_after {
                            // Leave long waits to the next run without retrying early.
                            if retry_after > std::time::Duration::from_secs(30) {
                                return Err(ApiError::Other(status.as_u16()));
                            }
                            delay = delay.max(retry_after);
                        }
                    }
                }
                match status {
                    reqwest::StatusCode::OK => response.text().await.map_err(ApiError::from),
                    reqwest::StatusCode::NOT_FOUND => return Err(ApiError::NotFound),
                    status if status.as_u16() == 429 || status.is_server_error() => {
                        Err(ApiError::Other(status.as_u16()))
                    }
                    status => return Err(ApiError::Other(status.as_u16())),
                }
            }
            Err(error) => Err(ApiError::NetworkError(error)),
        };
        drop(timer);
        match result {
            Ok(body) => {
                crate::provenance::record_response(url, &body)
                    .await
                    .map_err(|e| ApiError::Archive(e.to_string()))?;
                return Ok(body);
            }
            Err(error) if attempt == 2 => return Err(error),
            Err(ApiError::NetworkError(ref error))
                if !(error.is_timeout() || error.is_connect() || error.is_body()) =>
            {
                return result
            }
            Err(_) => {}
        }
        let jitter = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as u64
            % 250;
        tokio::time::sleep(delay + std::time::Duration::from_millis(jitter)).await;
    }
    unreachable!("bounded retry loop always returns")
}

/// Fetch a JSON endpoint via the shared HTTP client.
pub async fn fetch_api_json(url: &str) -> Result<String, ApiError> {
    fetch_api_text(url).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    struct TestServer {
        url: String,
        requests: Arc<AtomicUsize>,
        task: tokio::task::JoinHandle<()>,
    }

    impl TestServer {
        async fn start(responses: Vec<&'static str>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/", listener.local_addr().unwrap());
            let requests = Arc::new(AtomicUsize::new(0));
            let count = requests.clone();
            let task = tokio::spawn(async move {
                loop {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let mut request = Vec::new();
                    let mut buffer = [0; 1024];
                    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                        let size = stream.read(&mut buffer).await.unwrap();
                        assert_ne!(size, 0, "client closed before sending request headers");
                        request.extend_from_slice(&buffer[..size]);
                    }
                    let index = count.fetch_add(1, Ordering::SeqCst);
                    let response = responses[index.min(responses.len() - 1)];
                    stream.write_all(response.as_bytes()).await.unwrap();
                    stream.shutdown().await.unwrap();
                }
            });
            Self {
                url,
                requests,
                task,
            }
        }

        async fn fetch(&self) -> Result<String, ApiError> {
            let client = reqwest::Client::builder()
                .no_proxy()
                .timeout(std::time::Duration::from_secs(2))
                .build()
                .unwrap();
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                fetch_text_with_client(&client, &self.url),
            )
            .await
            .expect("bounded retries should finish within five seconds")
        }

        fn request_count(&self) -> usize {
            self.requests.load(Ordering::SeqCst)
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    const UNAVAILABLE: &str =
        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    const OK: &str = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";

    #[tokio::test]
    async fn transient_server_errors_retry_then_succeed() {
        let server = TestServer::start(vec![UNAVAILABLE, UNAVAILABLE, OK]).await;
        assert_eq!(server.fetch().await.unwrap(), "ok");
        assert_eq!(server.request_count(), 3);
    }

    #[tokio::test]
    async fn retries_stop_after_three_attempts() {
        let server = TestServer::start(vec![UNAVAILABLE]).await;
        assert!(matches!(server.fetch().await, Err(ApiError::Other(503))));
        assert_eq!(server.request_count(), 3);
    }

    #[tokio::test]
    async fn not_found_does_not_retry_even_with_retry_after() {
        let server = TestServer::start(vec![
            "HTTP/1.1 404 Not Found\r\nRetry-After: 60\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            OK,
        ])
        .await;
        assert!(matches!(server.fetch().await, Err(ApiError::NotFound)));
        assert_eq!(server.request_count(), 1);
    }

    #[tokio::test]
    async fn long_retry_after_defers_to_a_later_run() {
        let server = TestServer::start(vec![
            "HTTP/1.1 503 Service Unavailable\r\nRetry-After: 31\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            OK,
        ])
        .await;
        assert!(matches!(server.fetch().await, Err(ApiError::Other(503))));
        assert_eq!(server.request_count(), 1);
    }

    #[tokio::test]
    async fn successful_responses_ignore_retry_after() {
        let server = TestServer::start(vec![
            "HTTP/1.1 200 OK\r\nRetry-After: 60\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
        ])
        .await;
        assert_eq!(server.fetch().await.unwrap(), "ok");
        assert_eq!(server.request_count(), 1);
    }
}
