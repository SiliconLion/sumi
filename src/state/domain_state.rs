use crate::config::CrawlerConfig;
use chrono::{DateTime, Utc};
use std::time::{Duration, Instant};

/// Represents the robots.txt content for a domain
///
/// This is a placeholder type that will be properly implemented in the robots module.
/// For now, we just need to track whether we have robots.txt data.
#[derive(Debug, Clone)]
pub struct CachedRobots {
    pub content: String,
    pub fetched_at: DateTime<Utc>,
}

/// Tracks the state of a domain during crawling
///
/// This structure maintains per-domain information needed for rate limiting,
/// request counting, and robots.txt caching.
#[derive(Debug, Clone)]
pub struct DomainState {
    /// Number of requests made to this domain in the current crawl
    pub request_count: u32,

    /// Timestamp of the last request to this domain
    pub last_request_time: Option<Instant>,

    /// Whether this domain has been rate limited (HTTP 429)
    pub rate_limited: bool,

    /// Cached robots.txt data for this domain
    pub robots_txt: Option<CachedRobots>,

    /// When the robots.txt was fetched (for cache expiration)
    pub robots_fetched_at: Option<DateTime<Utc>>,
}

impl DomainState {
    /// Creates a new DomainState with default values
    pub fn new() -> Self {
        Self {
            request_count: 0,
            last_request_time: None,
            rate_limited: false,
            robots_txt: None,
            robots_fetched_at: None,
        }
    }

    /// Checks if a request can be made to this domain
    ///
    /// This method enforces:
    /// - Rate limiting (if domain returned HTTP 429)
    /// - Maximum requests per domain
    /// - Minimum time between requests to the same domain
    ///
    /// # Arguments
    ///
    /// * `config` - The crawler configuration containing limits
    /// * `now` - The current time instant
    ///
    /// # Returns
    ///
    /// * `true` - If a request can be made now
    /// * `false` - If the request should be delayed or blocked
    pub fn can_request(&self, config: &CrawlerConfig, now: Instant) -> bool {
        // Check if domain is rate limited
        if self.rate_limited {
            return false;
        }

        // Check if we've hit the maximum request limit for this domain
        if self.request_count >= config.max_domain_requests {
            return false;
        }

        // Check minimum time between requests
        if let Some(last) = self.last_request_time {
            let min_delay = Duration::from_millis(config.minimum_time_on_page);
            if now.duration_since(last) < min_delay {
                return false;
            }
        }

        true
    }

    /// Records that a request was made to this domain
    ///
    /// Updates the request count and last request time.
    pub fn record_request(&mut self, now: Instant) {
        self.request_count += 1;
        self.last_request_time = Some(now);
    }

    /// Marks this domain as rate limited
    pub fn mark_rate_limited(&mut self) {
        self.rate_limited = true;
    }

    /// Clears the rate limited flag (e.g., after cooldown period)
    pub fn clear_rate_limit(&mut self) {
        self.rate_limited = false;
    }

    /// Checks if this domain has exceeded the request limit
    pub fn has_exceeded_limit(&self, config: &CrawlerConfig) -> bool {
        self.request_count >= config.max_domain_requests
    }

    /// Returns the number of requests remaining for this domain
    pub fn requests_remaining(&self, config: &CrawlerConfig) -> u32 {
        config
            .max_domain_requests
            .saturating_sub(self.request_count)
    }

    /// Calculates the time until the next request can be made
    ///
    /// Returns None if a request can be made now, or the duration to wait otherwise.
    pub fn time_until_next_request(
        &self,
        config: &CrawlerConfig,
        now: Instant,
    ) -> Option<Duration> {
        if let Some(last) = self.last_request_time {
            let min_delay = Duration::from_millis(config.minimum_time_on_page);
            let elapsed = now.duration_since(last);
            if elapsed < min_delay {
                return Some(min_delay - elapsed);
            }
        }
        None
    }

    /// Checks if the robots.txt cache is stale (older than 24 hours)
    pub fn is_robots_stale(&self) -> bool {
        if let Some(fetched_at) = self.robots_fetched_at {
            let age = Utc::now() - fetched_at;
            age > chrono::Duration::hours(24)
        } else {
            true // No robots.txt fetched yet
        }
    }

    /// Updates the robots.txt cache
    pub fn update_robots(&mut self, content: String) {
        let now = Utc::now();
        self.robots_txt = Some(CachedRobots {
            content,
            fetched_at: now,
        });
        self.robots_fetched_at = Some(now);
    }
}

impl Default for DomainState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> CrawlerConfig {
        CrawlerConfig {
            max_depth: 3,
            max_concurrent_pages_open: 10,
            minimum_time_on_page: 1000, // 1 second
            max_domain_requests: 100,
        }
    }

    #[test]
    fn test_new_domain_state() {
        let state = DomainState::new();
        assert_eq!(state.request_count, 0);
        assert!(state.last_request_time.is_none());
        assert!(!state.rate_limited);
        assert!(state.robots_txt.is_none());
        assert!(state.robots_fetched_at.is_none());
    }

    #[test]
    fn test_can_request_initially() {
        let state = DomainState::new();
        let config = create_test_config();
        let now = Instant::now();

        assert!(state.can_request(&config, now));
    }

    #[test]
    fn test_cannot_request_when_rate_limited() {
        let mut state = DomainState::new();
        state.rate_limited = true;

        let config = create_test_config();
        let now = Instant::now();

        assert!(!state.can_request(&config, now));
    }

    #[test]
    fn test_cannot_request_when_limit_reached() {
        let mut state = DomainState::new();
        state.request_count = 100;

        let config = create_test_config();
        let now = Instant::now();

        assert!(!state.can_request(&config, now));
    }

    #[test]
    fn test_cannot_request_too_soon() {
        let mut state = DomainState::new();
        let now = Instant::now();
        state.last_request_time = Some(now);

        let config = create_test_config();

        // Try immediately - should fail
        assert!(!state.can_request(&config, now));

        // Try 500ms later - should still fail (min is 1000ms)
        let soon = now + Duration::from_millis(500);
        assert!(!state.can_request(&config, soon));
    }

    #[test]
    fn test_can_request_after_delay() {
        let mut state = DomainState::new();
        let now = Instant::now();
        state.last_request_time = Some(now);

        let config = create_test_config();

        // Try 1100ms later - should succeed
        let later = now + Duration::from_millis(1100);
        assert!(state.can_request(&config, later));
    }

    #[test]
    fn test_record_request() {
        let mut state = DomainState::new();
        let now = Instant::now();

        assert_eq!(state.request_count, 0);
        assert!(state.last_request_time.is_none());

        state.record_request(now);

        assert_eq!(state.request_count, 1);
        assert_eq!(state.last_request_time, Some(now));

        state.record_request(now);
        assert_eq!(state.request_count, 2);
    }

    #[test]
    fn test_mark_rate_limited() {
        let mut state = DomainState::new();
        assert!(!state.rate_limited);

        state.mark_rate_limited();
        assert!(state.rate_limited);
    }

    #[test]
    fn test_clear_rate_limit() {
        let mut state = DomainState::new();
        state.rate_limited = true;

        state.clear_rate_limit();
        assert!(!state.rate_limited);
    }

    #[test]
    fn test_has_exceeded_limit() {
        let mut state = DomainState::new();
        let config = create_test_config();

        assert!(!state.has_exceeded_limit(&config));

        state.request_count = 99;
        assert!(!state.has_exceeded_limit(&config));

        state.request_count = 100;
        assert!(state.has_exceeded_limit(&config));

        state.request_count = 101;
        assert!(state.has_exceeded_limit(&config));
    }

    #[test]
    fn test_requests_remaining() {
        let mut state = DomainState::new();
        let config = create_test_config();

        assert_eq!(state.requests_remaining(&config), 100);

        state.request_count = 50;
        assert_eq!(state.requests_remaining(&config), 50);

        state.request_count = 100;
        assert_eq!(state.requests_remaining(&config), 0);

        state.request_count = 150;
        assert_eq!(state.requests_remaining(&config), 0); // Saturating sub
    }

    #[test]
    fn test_time_until_next_request() {
        let mut state = DomainState::new();
        let config = create_test_config();
        let now = Instant::now();

        // No previous request
        assert!(state.time_until_next_request(&config, now).is_none());

        // Just made a request
        state.last_request_time = Some(now);
        let wait = state.time_until_next_request(&config, now);
        assert!(wait.is_some());
        assert_eq!(wait.unwrap(), Duration::from_millis(1000));

        // 500ms later
        let soon = now + Duration::from_millis(500);
        let wait = state.time_until_next_request(&config, soon);
        assert!(wait.is_some());
        assert_eq!(wait.unwrap(), Duration::from_millis(500));

        // 1100ms later
        let later = now + Duration::from_millis(1100);
        let wait = state.time_until_next_request(&config, later);
        assert!(wait.is_none());
    }

    #[test]
    fn test_is_robots_stale_no_fetch() {
        let state = DomainState::new();
        assert!(state.is_robots_stale());
    }

    #[test]
    fn test_is_robots_stale_recent() {
        let mut state = DomainState::new();
        state.robots_fetched_at = Some(Utc::now());
        assert!(!state.is_robots_stale());
    }

    #[test]
    fn test_is_robots_stale_old() {
        let mut state = DomainState::new();
        let old_time = Utc::now() - chrono::Duration::hours(25);
        state.robots_fetched_at = Some(old_time);
        assert!(state.is_robots_stale());
    }

    #[test]
    fn test_update_robots() {
        let mut state = DomainState::new();
        assert!(state.robots_txt.is_none());

        state.update_robots("User-agent: *\nDisallow: /admin".to_string());

        assert!(state.robots_txt.is_some());
        assert!(state.robots_fetched_at.is_some());

        let robots = state.robots_txt.unwrap();
        assert_eq!(robots.content, "User-agent: *\nDisallow: /admin");
    }

    #[test]
    fn test_default() {
        let state = DomainState::default();
        assert_eq!(state.request_count, 0);
        assert!(!state.rate_limited);
    }
}
