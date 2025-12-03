//! Robots.txt handling module
//!
//! This module provides functionality for fetching, parsing, and caching robots.txt files.
//! It respects robots.txt directives when crawling websites.

mod cache;
mod parser;

pub use cache::CachedRobots;
pub use parser::ParsedRobots;

use crate::SumiError;

/// Fetches robots.txt for a domain
///
/// # Arguments
///
/// * `domain` - The domain to fetch robots.txt from
/// * `user_agent` - The user agent string to use
///
/// # Returns
///
/// * `Ok(ParsedRobots)` - Successfully fetched and parsed robots.txt
/// * `Err(SumiError)` - Failed to fetch or parse
pub async fn fetch_robots(domain: &str, user_agent: &str) -> Result<ParsedRobots, SumiError> {
    let robots_url = format!("https://{}/robots.txt", domain);

    tracing::debug!("Fetching robots.txt from {}", robots_url);

    // Build a simple HTTP client for robots.txt fetching
    let client = reqwest::Client::builder()
        .user_agent(user_agent)
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    // Fetch robots.txt
    match client.get(&robots_url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.text().await {
                    Ok(content) => {
                        tracing::debug!("Successfully fetched robots.txt for {}", domain);
                        Ok(ParsedRobots::from_content(&content))
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read robots.txt body for {}: {}", domain, e);
                        Ok(ParsedRobots::allow_all())
                    }
                }
            } else {
                tracing::debug!(
                    "robots.txt not found for {} (status {}), allowing all",
                    domain,
                    response.status()
                );
                Ok(ParsedRobots::allow_all())
            }
        }
        Err(e) => {
            tracing::debug!(
                "Failed to fetch robots.txt for {}: {}, allowing all",
                domain,
                e
            );
            Ok(ParsedRobots::allow_all())
        }
    }
}

/// Checks if a URL is allowed by robots.txt
///
/// # Arguments
///
/// * `robots` - The parsed robots.txt data
/// * `url` - The URL to check
/// * `user_agent` - The user agent string
///
/// # Returns
///
/// * `true` - If the URL is allowed
/// * `false` - If the URL is disallowed
pub fn is_allowed(robots: &ParsedRobots, url: &str, user_agent: &str) -> bool {
    robots.is_allowed(url, user_agent)
}
