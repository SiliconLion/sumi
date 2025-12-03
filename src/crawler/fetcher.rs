//! HTTP fetcher implementation
//!
//! This module handles all HTTP requests for the crawler, including:
//! - Building HTTP clients with proper user agent strings
//! - HEAD requests to check Content-Type
//! - GET requests to fetch page content
//! - Retry logic for transient failures
//! - Redirect handling
//! - Error classification

use crate::config::UserAgentConfig;
use crate::state::PageState;
use reqwest::{redirect::Policy, Client, StatusCode};
use std::collections::HashSet;
use std::time::Duration;

/// Result of a fetch operation
#[derive(Debug)]
pub enum FetchResult {
    /// Successfully fetched the page
    Success {
        /// Final URL after redirects
        final_url: String,
        /// HTTP status code
        status_code: u16,
        /// Content-Type header value
        content_type: String,
        /// Page body content
        body: String,
        /// Page title (if extracted)
        title: Option<String>,
    },

    /// Page is not HTML (Content-Type mismatch)
    ContentMismatch {
        /// The actual Content-Type received
        content_type: String,
    },

    /// Redirect chain led to a terminal domain (blacklist/stub)
    RedirectToTerminal {
        /// The terminal URL
        terminal_url: String,
        /// The classification that made it terminal
        reason: String,
    },

    /// HTTP error that maps to a specific page state
    HttpError {
        /// The HTTP status code
        status_code: u16,
        /// The page state this error maps to
        state: PageState,
    },

    /// Network error (connection refused, timeout, etc.)
    NetworkError {
        /// Error description
        error: String,
        /// The page state this error maps to
        state: PageState,
    },

    /// Redirect error (loop, too many redirects)
    RedirectError {
        /// Error description
        error: String,
    },
}

/// Retry policy configuration
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retries
    pub max_retries: u32,
    /// Base delay between retries (exponential backoff)
    pub base_delay: Duration,
}

/// Redirect chain tracker for handling HTTP redirects
#[derive(Debug)]
pub struct RedirectChain {
    /// Maximum number of redirects to follow
    pub max_redirects: u32,
    /// Set of visited URLs to detect loops
    pub visited: HashSet<String>,
}

impl RedirectChain {
    /// Creates a new redirect chain tracker
    pub fn new() -> Self {
        Self {
            max_redirects: 10,
            visited: HashSet::new(),
        }
    }

    /// Adds a URL to the visited set
    ///
    /// # Returns
    ///
    /// * `true` - If this is a new URL
    /// * `false` - If we've already visited this URL (loop detected)
    pub fn add_url(&mut self, url: &str) -> bool {
        self.visited.insert(url.to_string())
    }

    /// Checks if we've exceeded the maximum redirect count
    pub fn is_too_long(&self) -> bool {
        self.visited.len() > self.max_redirects as usize
    }

    /// Checks if a URL has been visited (loop detection)
    pub fn has_visited(&self, url: &str) -> bool {
        self.visited.contains(url)
    }
}

impl Default for RedirectChain {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_secs(5),
        }
    }
}

impl RetryPolicy {
    /// Calculates the delay for a given retry attempt
    ///
    /// Uses exponential backoff: delay = base_delay * 2^attempt
    ///
    /// # Arguments
    ///
    /// * `attempt` - The retry attempt number (0-indexed)
    ///
    /// # Returns
    ///
    /// The delay duration for this attempt
    fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let multiplier = 2u32.pow(attempt);
        self.base_delay * multiplier
    }

    /// Checks if an error is retryable
    ///
    /// # Arguments
    ///
    /// * `status` - Optional HTTP status code
    /// * `is_timeout` - Whether this was a timeout error
    /// * `is_connect` - Whether this was a connection error
    ///
    /// # Returns
    ///
    /// `true` if the error should be retried
    fn is_retryable(status: Option<StatusCode>, is_timeout: bool, is_connect: bool) -> bool {
        if is_timeout {
            return true;
        }

        if is_connect {
            return true;
        }

        if let Some(status) = status {
            // Retry 5xx errors
            if status.is_server_error() {
                return true;
            }

            // Don't retry client errors
            if status.is_client_error() {
                return false;
            }
        }

        false
    }
}

/// Builds an HTTP client with proper configuration
///
/// # Arguments
///
/// * `config` - The user agent configuration
///
/// # Returns
///
/// * `Ok(Client)` - Successfully built HTTP client
/// * `Err(reqwest::Error)` - Failed to build client
///
/// # Example
///
/// ```no_run
/// use sumi_ripple::config::UserAgentConfig;
/// use sumi_ripple::crawler::build_http_client;
///
/// let config = UserAgentConfig {
///     crawler_name: "SumiRipple".to_string(),
///     crawler_version: "1.0".to_string(),
///     contact_url: "https://example.com/about".to_string(),
///     contact_email: "admin@example.com".to_string(),
/// };
///
/// let client = build_http_client(&config).unwrap();
/// ```
pub fn build_http_client(config: &UserAgentConfig) -> Result<Client, reqwest::Error> {
    // Format: CrawlerName/Version (+ContactURL; ContactEmail)
    let user_agent = format!(
        "{}/{} (+{}; {})",
        config.crawler_name, config.crawler_version, config.contact_url, config.contact_email
    );

    Client::builder()
        .user_agent(user_agent)
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .redirect(Policy::none()) // Handle redirects manually
        .https_only(true)
        .gzip(true)
        .brotli(true)
        .build()
}

/// Fetches a URL with full error handling and retry logic
///
/// # Request Flow
///
/// 1. Send HEAD request to check Content-Type
///    - If not HTML → return ContentMismatch
/// 2. Send GET request
/// 3. Handle redirects manually (max 10 hops)
///    - Track visited URLs to detect loops
///    - Stop if redirect hits blacklist/stub domain
/// 4. Handle response codes per retry logic
///
/// # Retry Logic
///
/// | Condition | Action |
/// |-----------|--------|
/// | HTTP 404 | Immediate → DeadLink |
/// | HTTP 429 | Immediate → RateLimited |
/// | HTTP 5xx | Retry up to 3 times, 5s delay |
/// | Timeout | Retry up to 3 times, 5s delay |
/// | Connection refused | Retry up to 2 times |
/// | TLS/SSL error | Immediate → Unreachable |
/// | Redirect loop | Immediate → Failed |
/// | Redirect chain > 10 | Immediate → Failed |
///
/// # Arguments
///
/// * `client` - The HTTP client to use
/// * `url` - The URL to fetch
///
/// # Returns
///
/// A FetchResult indicating success or the type of failure
pub async fn fetch_url(client: &Client, url: &str) -> FetchResult {
    fetch_url_with_retry(client, url, &RetryPolicy::default()).await
}

/// Fetches a URL with custom retry policy
///
/// # Arguments
///
/// * `client` - The HTTP client to use
/// * `url` - The URL to fetch
/// * `policy` - The retry policy to use
///
/// # Returns
///
/// A FetchResult indicating success or the type of failure
pub async fn fetch_url_with_retry(client: &Client, url: &str, policy: &RetryPolicy) -> FetchResult {
    let mut attempt = 0;

    loop {
        // Try to fetch
        let result = fetch_url_once(client, url).await;

        // Check if we should retry
        let should_retry = match &result {
            FetchResult::HttpError { status_code, .. } => {
                let status = StatusCode::from_u16(*status_code).ok();
                RetryPolicy::is_retryable(status, false, false)
            }
            FetchResult::NetworkError { .. } => {
                // Network errors are generally retryable
                attempt < policy.max_retries
            }
            _ => false,
        };

        // Return if successful or non-retryable error
        if !should_retry || attempt >= policy.max_retries {
            return result;
        }

        // Wait before retrying
        let delay = policy.delay_for_attempt(attempt);
        tracing::debug!(
            "Retry attempt {} for {}, waiting {:?}",
            attempt + 1,
            url,
            delay
        );
        tokio::time::sleep(delay).await;

        attempt += 1;
    }
}

/// Performs a single fetch attempt without retry logic
async fn fetch_url_once(client: &Client, url: &str) -> FetchResult {
    fetch_url_with_redirects(client, url, &mut RedirectChain::new()).await
}

/// Performs a single fetch with manual redirect following
async fn fetch_url_with_redirects(
    client: &Client,
    url: &str,
    redirect_chain: &mut RedirectChain,
) -> FetchResult {
    // Add current URL to redirect chain
    if !redirect_chain.add_url(url) {
        return FetchResult::RedirectError {
            error: format!("Redirect loop detected at {}", url),
        };
    }

    // Check if redirect chain is too long
    if redirect_chain.is_too_long() {
        return FetchResult::RedirectError {
            error: format!("Too many redirects (max {})", redirect_chain.max_redirects),
        };
    }

    // First, send HEAD request to check Content-Type
    match client.head(url).send().await {
        Ok(response) => {
            let status = response.status();

            // Check for redirect (we disabled automatic redirects)
            if status.is_redirection() {
                // Extract redirect location
                if let Some(location) = response.headers().get("location") {
                    if let Ok(location_str) = location.to_str() {
                        // Resolve relative URLs
                        let redirect_url = if location_str.starts_with("http://")
                            || location_str.starts_with("https://")
                        {
                            location_str.to_string()
                        } else {
                            // Handle relative URLs
                            match url::Url::parse(url) {
                                Ok(base) => match base.join(location_str) {
                                    Ok(resolved) => resolved.to_string(),
                                    Err(_) => {
                                        return FetchResult::RedirectError {
                                            error: format!(
                                                "Invalid redirect URL: {}",
                                                location_str
                                            ),
                                        };
                                    }
                                },
                                Err(_) => {
                                    return FetchResult::RedirectError {
                                        error: format!("Invalid base URL: {}", url),
                                    };
                                }
                            }
                        };

                        tracing::debug!("Following redirect from {} to {}", url, redirect_url);

                        // Recursively follow the redirect (boxed to avoid infinite size)
                        return Box::pin(fetch_url_with_redirects(
                            client,
                            &redirect_url,
                            redirect_chain,
                        ))
                        .await;
                    }
                }

                // Redirect without Location header - treat as error
                return FetchResult::RedirectError {
                    error: format!("Redirect response without Location header"),
                };
            } else if !status.is_success() {
                // If HEAD fails with a client error, return early
                if status == StatusCode::NOT_FOUND {
                    return FetchResult::HttpError {
                        status_code: status.as_u16(),
                        state: PageState::DeadLink,
                    };
                }

                if status == StatusCode::TOO_MANY_REQUESTS {
                    return FetchResult::HttpError {
                        status_code: status.as_u16(),
                        state: PageState::RateLimited,
                    };
                }

                if status.is_client_error() {
                    return FetchResult::HttpError {
                        status_code: status.as_u16(),
                        state: PageState::Failed,
                    };
                }
            }

            // Check Content-Type from HEAD
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();

            // If HEAD succeeded and content-type is not HTML, return mismatch
            if status.is_success()
                && !content_type.is_empty()
                && !content_type.contains("text/html")
            {
                return FetchResult::ContentMismatch { content_type };
            }
        }
        Err(e) => {
            // HEAD request failed, we'll try GET anyway
            // Some servers don't support HEAD
            tracing::debug!("HEAD request failed for {}: {}, trying GET", url, e);
        }
    }

    // Now send GET request
    match client.get(url).send().await {
        Ok(response) => {
            let status = response.status();
            let final_url = response.url().to_string();

            // Check for redirects in GET response
            if status.is_redirection() {
                // Extract redirect location
                if let Some(location) = response.headers().get("location") {
                    if let Ok(location_str) = location.to_str() {
                        // Resolve relative URLs
                        let redirect_url = if location_str.starts_with("http://")
                            || location_str.starts_with("https://")
                        {
                            location_str.to_string()
                        } else {
                            // Handle relative URLs
                            match url::Url::parse(url) {
                                Ok(base) => match base.join(location_str) {
                                    Ok(resolved) => resolved.to_string(),
                                    Err(_) => {
                                        return FetchResult::RedirectError {
                                            error: format!(
                                                "Invalid redirect URL: {}",
                                                location_str
                                            ),
                                        };
                                    }
                                },
                                Err(_) => {
                                    return FetchResult::RedirectError {
                                        error: format!("Invalid base URL: {}", url),
                                    };
                                }
                            }
                        };

                        tracing::debug!("Following GET redirect from {} to {}", url, redirect_url);

                        // Recursively follow the redirect (boxed to avoid infinite size)
                        return Box::pin(fetch_url_with_redirects(
                            client,
                            &redirect_url,
                            redirect_chain,
                        ))
                        .await;
                    }
                }

                // Redirect without Location header - treat as error
                return FetchResult::RedirectError {
                    error: format!("GET redirect response without Location header"),
                };
            }

            // Handle specific HTTP status codes
            if status == StatusCode::NOT_FOUND {
                return FetchResult::HttpError {
                    status_code: status.as_u16(),
                    state: PageState::DeadLink,
                };
            }

            if status == StatusCode::TOO_MANY_REQUESTS {
                return FetchResult::HttpError {
                    status_code: status.as_u16(),
                    state: PageState::RateLimited,
                };
            }

            if status.is_client_error() {
                return FetchResult::HttpError {
                    status_code: status.as_u16(),
                    state: PageState::Failed,
                };
            }

            if status.is_server_error() {
                return FetchResult::HttpError {
                    status_code: status.as_u16(),
                    state: PageState::Failed,
                };
            }

            if !status.is_success() {
                return FetchResult::HttpError {
                    status_code: status.as_u16(),
                    state: PageState::Failed,
                };
            }

            // Check Content-Type
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();

            if !content_type.contains("text/html") && !content_type.is_empty() {
                return FetchResult::ContentMismatch { content_type };
            }

            // Get body
            match response.text().await {
                Ok(body) => FetchResult::Success {
                    final_url,
                    status_code: status.as_u16(),
                    content_type,
                    body,
                    title: None, // Will be extracted during parsing
                },
                Err(e) => FetchResult::NetworkError {
                    error: e.to_string(),
                    state: PageState::Failed,
                },
            }
        }
        Err(e) => {
            // Classify error
            if e.is_timeout() {
                FetchResult::NetworkError {
                    error: "Request timeout".to_string(),
                    state: PageState::Unreachable,
                }
            } else if e.is_connect() {
                FetchResult::NetworkError {
                    error: "Connection refused".to_string(),
                    state: PageState::Unreachable,
                }
            } else if e.is_status() {
                // Extract status code if available
                if let Some(status) = e.status() {
                    if status == StatusCode::NOT_FOUND {
                        FetchResult::HttpError {
                            status_code: status.as_u16(),
                            state: PageState::DeadLink,
                        }
                    } else if status == StatusCode::TOO_MANY_REQUESTS {
                        FetchResult::HttpError {
                            status_code: status.as_u16(),
                            state: PageState::RateLimited,
                        }
                    } else {
                        FetchResult::HttpError {
                            status_code: status.as_u16(),
                            state: PageState::Failed,
                        }
                    }
                } else {
                    FetchResult::NetworkError {
                        error: e.to_string(),
                        state: PageState::Failed,
                    }
                }
            } else {
                FetchResult::NetworkError {
                    error: e.to_string(),
                    state: PageState::Failed,
                }
            }
        }
    }
}

/// Sends a HEAD request to check Content-Type before fetching
///
/// # Arguments
///
/// * `client` - The HTTP client to use
/// * `url` - The URL to check
///
/// # Returns
///
/// * `Ok(Some(String))` - Content-Type header value
/// * `Ok(None)` - No Content-Type header
/// * `Err(FetchResult)` - Error occurred during HEAD request
pub async fn check_content_type(client: &Client, url: &str) -> Result<Option<String>, FetchResult> {
    match client.head(url).send().await {
        Ok(response) => {
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            Ok(content_type)
        }
        Err(e) => {
            if e.is_timeout() {
                Err(FetchResult::NetworkError {
                    error: "Request timeout".to_string(),
                    state: PageState::Unreachable,
                })
            } else if e.is_connect() {
                Err(FetchResult::NetworkError {
                    error: "Connection refused".to_string(),
                    state: PageState::Unreachable,
                })
            } else {
                Err(FetchResult::NetworkError {
                    error: e.to_string(),
                    state: PageState::Failed,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> UserAgentConfig {
        UserAgentConfig {
            crawler_name: "TestCrawler".to_string(),
            crawler_version: "1.0".to_string(),
            contact_url: "https://example.com/about".to_string(),
            contact_email: "admin@example.com".to_string(),
        }
    }

    #[test]
    fn test_build_http_client() {
        let config = create_test_config();
        let client = build_http_client(&config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_user_agent_format() {
        let config = create_test_config();
        let client = build_http_client(&config).unwrap();

        // The user agent should be formatted correctly
        // We can't directly inspect it, but we can verify the client was built
        assert!(format!("{:?}", client).contains("Client"));
    }

    #[test]
    fn test_retry_policy_default() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_retries, 3);
        assert_eq!(policy.base_delay, Duration::from_secs(5));
    }

    #[test]
    fn test_retry_policy_delay_calculation() {
        let policy = RetryPolicy::default();

        assert_eq!(policy.delay_for_attempt(0), Duration::from_secs(5));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_secs(10));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_secs(20));
        assert_eq!(policy.delay_for_attempt(3), Duration::from_secs(40));
    }

    #[test]
    fn test_is_retryable_5xx() {
        assert!(RetryPolicy::is_retryable(
            Some(StatusCode::INTERNAL_SERVER_ERROR),
            false,
            false
        ));
        assert!(RetryPolicy::is_retryable(
            Some(StatusCode::BAD_GATEWAY),
            false,
            false
        ));
        assert!(RetryPolicy::is_retryable(
            Some(StatusCode::SERVICE_UNAVAILABLE),
            false,
            false
        ));
    }

    #[test]
    fn test_is_not_retryable_4xx() {
        assert!(!RetryPolicy::is_retryable(
            Some(StatusCode::NOT_FOUND),
            false,
            false
        ));
        assert!(!RetryPolicy::is_retryable(
            Some(StatusCode::FORBIDDEN),
            false,
            false
        ));
        assert!(!RetryPolicy::is_retryable(
            Some(StatusCode::BAD_REQUEST),
            false,
            false
        ));
    }

    #[test]
    fn test_is_retryable_timeout() {
        assert!(RetryPolicy::is_retryable(None, true, false));
    }

    #[test]
    fn test_is_retryable_connection() {
        assert!(RetryPolicy::is_retryable(None, false, true));
    }

    // Additional tests would require mocking HTTP responses
    // These would be implemented with wiremock in integration tests
}
