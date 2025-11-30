//! Robots.txt caching implementation
//!
//! This module provides caching functionality for robots.txt files, including
//! automatic expiration after 24 hours.

use crate::robots::ParsedRobots;
use chrono::{DateTime, Duration, Utc};

/// Cached robots.txt data for a domain
///
/// This structure stores parsed robots.txt content along with the timestamp
/// when it was fetched, allowing for cache expiration checks.
#[derive(Debug, Clone)]
pub struct CachedRobots {
    /// The parsed robots.txt content
    pub content: ParsedRobots,

    /// When the robots.txt was fetched
    pub fetched_at: DateTime<Utc>,
}

impl CachedRobots {
    /// Creates a new CachedRobots instance
    ///
    /// # Arguments
    ///
    /// * `content` - The parsed robots.txt content
    ///
    /// # Returns
    ///
    /// A new CachedRobots instance with the current timestamp
    pub fn new(content: ParsedRobots) -> Self {
        Self {
            content,
            fetched_at: Utc::now(),
        }
    }

    /// Checks if the cached robots.txt is stale (older than 24 hours)
    ///
    /// According to best practices, robots.txt should be refreshed daily
    /// to respect any changes made by the website owner.
    ///
    /// # Returns
    ///
    /// * `true` - If the cache is older than 24 hours
    /// * `false` - If the cache is still fresh
    pub fn is_stale(&self) -> bool {
        let age = Utc::now() - self.fetched_at;
        age > Duration::hours(24)
    }

    /// Returns the age of the cached robots.txt
    ///
    /// # Returns
    ///
    /// A Duration representing how long ago the robots.txt was fetched
    pub fn age(&self) -> Duration {
        Utc::now() - self.fetched_at
    }

    /// Checks if a URL is allowed according to the cached robots.txt
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to check
    /// * `user_agent` - The user agent string
    ///
    /// # Returns
    ///
    /// * `true` - If the URL is allowed
    /// * `false` - If the URL is disallowed
    pub fn is_allowed(&self, url: &str, user_agent: &str) -> bool {
        self.content.is_allowed(url, user_agent)
    }

    /// Gets the crawl delay from the cached robots.txt
    ///
    /// # Arguments
    ///
    /// * `user_agent` - The user agent string
    ///
    /// # Returns
    ///
    /// The crawl delay in seconds, if specified
    pub fn crawl_delay(&self, user_agent: &str) -> Option<f64> {
        self.content.crawl_delay(user_agent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_cache_not_stale() {
        let robots = ParsedRobots::allow_all();
        let cache = CachedRobots::new(robots);
        assert!(!cache.is_stale());
    }

    #[test]
    fn test_cache_is_stale() {
        let robots = ParsedRobots::allow_all();
        let mut cache = CachedRobots::new(robots);

        // Manually set fetched_at to 25 hours ago
        cache.fetched_at = Utc::now() - Duration::hours(25);

        assert!(cache.is_stale());
    }

    #[test]
    fn test_cache_not_stale_at_23_hours() {
        let robots = ParsedRobots::allow_all();
        let mut cache = CachedRobots::new(robots);

        // Set fetched_at to 23 hours ago
        cache.fetched_at = Utc::now() - Duration::hours(23);

        assert!(!cache.is_stale());
    }

    #[test]
    fn test_age() {
        let robots = ParsedRobots::allow_all();
        let mut cache = CachedRobots::new(robots);

        // Set fetched_at to 12 hours ago
        cache.fetched_at = Utc::now() - Duration::hours(12);

        let age = cache.age();
        // Allow some tolerance for test execution time
        assert!(age.num_hours() >= 11 && age.num_hours() <= 13);
    }

    #[test]
    fn test_is_allowed_delegates_to_content() {
        let robots = ParsedRobots::allow_all();
        let cache = CachedRobots::new(robots);

        assert!(cache.is_allowed("/any/path", "TestBot"));
    }

    #[test]
    fn test_crawl_delay_delegates_to_content() {
        let robots = ParsedRobots::allow_all();
        let cache = CachedRobots::new(robots);

        // Currently returns None as we haven't implemented crawl delay parsing
        assert_eq!(cache.crawl_delay("TestBot"), None);
    }
}
