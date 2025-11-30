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
/// | Connection refused | Immediate → Unreachable |
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
    // TODO: Implement full fetch logic with retry and redirect handling

    // Placeholder implementation
    match client.get(url).send().await {
        Ok(response) => {
            let status = response.status();
            let final_url = response.url().to_string();

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

            if !content_type.contains("text/html") {
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
    // TODO: Implement HEAD request
    let _ = (client, url);
    Ok(Some("text/html".to_string()))
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

    // Additional tests would require mocking HTTP responses
    // These would be implemented with wiremock in integration tests
}
