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
    // TODO: Implement robots.txt fetching
    let _ = (domain, user_agent);
    Ok(ParsedRobots::allow_all())
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
