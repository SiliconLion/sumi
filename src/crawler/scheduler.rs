//! Scheduler for managing the crawl frontier and rate limiting
//!
//! This module handles:
//! - Priority queue management for URLs to crawl
//! - Global concurrency limiting via semaphores
//! - Per-domain rate limiting and request counting
//! - Respecting minimum delays between requests
//! - Integrating robots.txt crawl delays

use crate::config::CrawlerConfig;
use crate::state::DomainState;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use url::Url;

/// A URL queued for fetching with priority information
#[derive(Debug, Clone)]
pub struct QueuedUrl {
    /// The URL to fetch
    pub url: Url,

    /// The domain of this URL
    pub domain: String,

    /// Priority value (lower is higher priority)
    pub priority: u32,

    /// Database page ID
    pub page_id: i64,
}

// Implement ordering traits for priority queue
// Lower priority values have higher priority (are popped first from BinaryHeap)
impl Ord for QueuedUrl {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse comparison so lower priority values come first
        other
            .priority
            .cmp(&self.priority)
            .then_with(|| self.url.as_str().cmp(other.url.as_str()))
    }
}

impl PartialOrd for QueuedUrl {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for QueuedUrl {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.url == other.url
    }
}

impl Eq for QueuedUrl {}

/// A scheduled fetch with a semaphore permit
pub struct ScheduledFetch {
    /// The URL to fetch
    pub url: QueuedUrl,

    /// The semaphore permit for this fetch
    pub _permit: tokio::sync::OwnedSemaphorePermit,
}

/// Scheduler manages the frontier queue and rate limiting
///
/// The scheduler coordinates:
/// - Global concurrency limits (max concurrent pages open)
/// - Per-domain rate limits (minimum time between requests)
/// - Per-domain request counts (max requests per domain)
/// - Priority-based URL selection from the frontier
pub struct Scheduler {
    /// Global semaphore for limiting concurrent fetches
    global_semaphore: Arc<Semaphore>,

    /// Per-domain state tracking
    domain_states: HashMap<String, DomainState>,

    /// Frontier priority queue of URLs to fetch (lower priority values are fetched first)
    frontier: BinaryHeap<QueuedUrl>,

    /// Crawler configuration
    config: CrawlerConfig,
}

impl Scheduler {
    /// Creates a new scheduler
    ///
    /// # Arguments
    ///
    /// * `config` - The crawler configuration
    /// * `initial_frontier` - Initial URLs to crawl
    /// * `initial_domain_states` - Existing domain states (for resume)
    ///
    /// # Returns
    ///
    /// A new Scheduler instance
    pub fn new(
        config: CrawlerConfig,
        initial_frontier: Vec<QueuedUrl>,
        initial_domain_states: HashMap<String, DomainState>,
    ) -> Self {
        let global_semaphore = Arc::new(Semaphore::new(config.max_concurrent_pages_open as usize));

        Self {
            global_semaphore,
            domain_states: initial_domain_states,
            frontier: BinaryHeap::from(initial_frontier),
            config,
        }
    }

    /// Gets the next URL to fetch
    ///
    /// This method:
    /// 1. Returns None if the frontier is truly empty
    /// 2. Acquires a global semaphore permit
    /// 3. Searches the frontier for a URL whose domain can accept a request
    /// 4. If no domain is ready, waits for the minimum required time and retries
    /// 5. Returns the URL with its permit
    ///
    /// # Returns
    ///
    /// * `Some(ScheduledFetch)` - A URL that's ready to fetch
    /// * `None` - The frontier is empty
    pub async fn next_url(&mut self) -> Option<ScheduledFetch> {
        // Return None only if frontier is truly empty
        if self.frontier.is_empty() {
            return None;
        }

        // Acquire global semaphore permit
        let permit = self.global_semaphore.clone().acquire_owned().await.ok()?;

        // Active wait loop: keep trying until we find a ready domain
        let start_waiting = Instant::now();
        let max_wait_time = Duration::from_secs(30); // Maximum 30 seconds wait

        loop {
            // Check if we've been waiting too long
            if start_waiting.elapsed() > max_wait_time {
                tracing::warn!(
                    "Exceeded maximum wait time of {:?} while waiting for domains. Frontier size: {}",
                    max_wait_time,
                    self.frontier.len()
                );
                // This might indicate a bug, but let's not hang forever
                return None;
            }
            let now = Instant::now();

            // Collect URLs that are not ready yet (need to put them back)
            let mut not_ready = Vec::new();
            let mut found = None;

            // Pop URLs from the heap until we find one that's ready
            // URLs are popped in priority order (lower priority values first)
            while let Some(queued) = self.frontier.pop() {
                let state = self
                    .domain_states
                    .entry(queued.domain.clone())
                    .or_insert_with(DomainState::new);

                let can_req = state.can_request(&self.config, now);
                tracing::trace!(
                    "Checking domain {} for URL {}: can_request={}",
                    queued.domain,
                    queued.url,
                    can_req
                );

                if can_req {
                    // Found a ready URL
                    found = Some(queued);
                    break;
                } else {
                    // Not ready yet, save for later
                    not_ready.push(queued);
                }
            }

            // Put back the URLs we couldn't use
            for queued in not_ready {
                self.frontier.push(queued);
            }

            if let Some(url) = found {
                tracing::debug!("Returning URL: {}", url.url);
                return Some(ScheduledFetch {
                    url,
                    _permit: permit,
                });
            }

            // No domains ready, calculate minimum wait time
            let min_wait = self.calculate_minimum_wait_time(now);

            tracing::debug!(
                "No domains ready, waiting {:?}. Frontier size: {}",
                min_wait,
                self.frontier.len()
            );

            // Sleep for the minimum time needed
            tokio::time::sleep(min_wait).await;

            // Check again if frontier is still not empty after sleep
            if self.frontier.is_empty() {
                return None;
            }
        }
    }

    /// Calculates the minimum time to wait before any domain is ready
    ///
    /// This method iterates through the frontier and finds the domain that will
    /// be ready soonest, returning the time until that domain is ready.
    ///
    /// # Arguments
    ///
    /// * `now` - The current time instant
    ///
    /// # Returns
    ///
    /// The minimum duration to wait before checking again
    fn calculate_minimum_wait_time(&self, now: Instant) -> Duration {
        let mut min_wait = Duration::from_millis(100); // Default 100ms

        for queued in self.frontier.iter() {
            if let Some(state) = self.domain_states.get(&queued.domain) {
                if let Some(wait) = state.time_until_next_request(&self.config, now) {
                    if wait < min_wait {
                        min_wait = wait;
                    }
                } else {
                    // Domain state exists but can request now - return minimal wait
                    return Duration::from_millis(10);
                }
            } else {
                // Domain has no state yet, so it's ready immediately
                return Duration::from_millis(10);
            }
        }

        // Add small buffer to ensure the domain is definitely ready
        min_wait + Duration::from_millis(10)
    }

    /// Adds a URL to the frontier
    ///
    /// The URL is inserted into the priority queue based on its priority value.
    /// URLs with lower priority values will be fetched first.
    ///
    /// # Arguments
    ///
    /// * `url` - The queued URL to add
    pub fn add_to_frontier(&mut self, url: QueuedUrl) {
        self.frontier.push(url);
    }

    /// Records that a request was made to a domain
    ///
    /// # Arguments
    ///
    /// * `domain` - The domain that received the request
    pub fn record_request(&mut self, domain: &str) {
        let now = Instant::now();
        let state = self
            .domain_states
            .entry(domain.to_string())
            .or_insert_with(DomainState::new);

        state.record_request(now);
    }

    /// Marks a domain as rate limited
    ///
    /// # Arguments
    ///
    /// * `domain` - The domain to mark as rate limited
    pub fn mark_rate_limited(&mut self, domain: &str) {
        let state = self
            .domain_states
            .entry(domain.to_string())
            .or_insert_with(DomainState::new);

        state.mark_rate_limited();
    }

    /// Returns the number of URLs in the frontier
    pub fn frontier_size(&self) -> usize {
        self.frontier.len()
    }

    /// Returns whether the frontier is empty
    pub fn is_empty(&self) -> bool {
        self.frontier.is_empty()
    }

    /// Gets the domain state for a specific domain
    pub fn get_domain_state(&self, domain: &str) -> Option<&DomainState> {
        self.domain_states.get(domain)
    }

    /// Gets mutable domain state for a specific domain
    pub fn get_domain_state_mut(&mut self, domain: &str) -> Option<&mut DomainState> {
        self.domain_states.get_mut(domain)
    }

    /// Gets all domain states (for persistence)
    pub fn get_all_domain_states(&self) -> &HashMap<String, DomainState> {
        &self.domain_states
    }
}

/// Calculates the effective delay for a domain
///
/// This takes the maximum of:
/// - The configured minimum time on page
/// - The robots.txt crawl delay (if specified)
///
/// # Arguments
///
/// * `config` - The crawler configuration
/// * `domain_state` - The domain state (which may contain robots.txt data)
/// * `user_agent` - The user agent string to check for crawl delay
///
/// # Returns
///
/// The effective delay duration
#[cfg(test)]
pub fn effective_delay(
    config: &CrawlerConfig,
    domain_state: &DomainState,
    user_agent: &str,
) -> Duration {
    let config_delay = Duration::from_millis(config.minimum_time_on_page);

    // Check for robots.txt crawl delay
    let robots_delay = domain_state
        .robots_txt
        .as_ref()
        .and_then(|cached_robots| {
            // Parse robots.txt and extract crawl delay
            use crate::robots::ParsedRobots;
            let parsed = ParsedRobots::from_content(&cached_robots.content);
            parsed.crawl_delay(user_agent)
        })
        .map(|seconds| Duration::from_secs_f64(seconds))
        .unwrap_or(Duration::ZERO);

    std::cmp::max(config_delay, robots_delay)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> CrawlerConfig {
        CrawlerConfig {
            max_depth: 3,
            max_concurrent_pages_open: 10,
            minimum_time_on_page: 1000,
            max_domain_requests: 500,
        }
    }

    fn create_test_url(domain: &str, path: &str, page_id: i64) -> QueuedUrl {
        let url = Url::parse(&format!("https://{}{}", domain, path)).unwrap();
        QueuedUrl {
            url: url.clone(),
            domain: domain.to_string(),
            priority: 0,
            page_id,
        }
    }

    #[test]
    fn test_new_scheduler() {
        let config = create_test_config();
        let scheduler = Scheduler::new(config, vec![], HashMap::new());

        assert_eq!(scheduler.frontier_size(), 0);
        assert!(scheduler.is_empty());
    }

    #[test]
    fn test_add_to_frontier() {
        let config = create_test_config();
        let mut scheduler = Scheduler::new(config, vec![], HashMap::new());

        let url = create_test_url("example.com", "/page", 1);
        scheduler.add_to_frontier(url);

        assert_eq!(scheduler.frontier_size(), 1);
        assert!(!scheduler.is_empty());
    }

    #[tokio::test]
    async fn test_next_url_from_frontier() {
        let config = create_test_config();
        let url = create_test_url("example.com", "/page", 1);
        let mut scheduler = Scheduler::new(config, vec![url], HashMap::new());

        assert_eq!(scheduler.frontier_size(), 1);

        let scheduled = scheduler.next_url().await;
        assert!(scheduled.is_some());

        assert_eq!(scheduler.frontier_size(), 0);
    }

    #[tokio::test]
    async fn test_next_url_empty_frontier() {
        let config = create_test_config();
        let mut scheduler = Scheduler::new(config, vec![], HashMap::new());

        let scheduled = scheduler.next_url().await;
        assert!(scheduled.is_none());
    }

    #[test]
    fn test_record_request() {
        let config = create_test_config();
        let mut scheduler = Scheduler::new(config, vec![], HashMap::new());

        scheduler.record_request("example.com");

        let state = scheduler.get_domain_state("example.com");
        assert!(state.is_some());
        assert_eq!(state.unwrap().request_count, 1);
    }

    #[test]
    fn test_mark_rate_limited() {
        let config = create_test_config();
        let mut scheduler = Scheduler::new(config, vec![], HashMap::new());

        scheduler.mark_rate_limited("example.com");

        let state = scheduler.get_domain_state("example.com");
        assert!(state.is_some());
        assert!(state.unwrap().rate_limited);
    }

    #[test]
    fn test_effective_delay_uses_config() {
        let config = create_test_config();
        let domain_state = DomainState::new();

        let delay = effective_delay(&config, &domain_state, "TestBot");
        assert_eq!(delay, Duration::from_millis(1000));
    }

    #[test]
    fn test_effective_delay_with_robots_delay() {
        let config = create_test_config();
        let mut domain_state = DomainState::new();

        // Add robots.txt with crawl delay of 5 seconds
        domain_state.update_robots("User-agent: *\nCrawl-delay: 5\nDisallow: /admin".to_string());

        let delay = effective_delay(&config, &domain_state, "TestBot");
        // Should use the maximum of config (1 second) and robots (5 seconds)
        assert_eq!(delay, Duration::from_secs(5));
    }

    #[test]
    fn test_effective_delay_robots_smaller_than_config() {
        let config = create_test_config();
        let mut domain_state = DomainState::new();

        // Add robots.txt with crawl delay of 0.5 seconds (500ms)
        domain_state.update_robots("User-agent: *\nCrawl-delay: 0.5".to_string());

        let delay = effective_delay(&config, &domain_state, "TestBot");
        // Should use the maximum of config (1000ms) and robots (500ms)
        assert_eq!(delay, Duration::from_millis(1000));
    }

    #[test]
    fn test_effective_delay_specific_user_agent() {
        let config = create_test_config();
        let mut domain_state = DomainState::new();

        // Add robots.txt with different delays for different user agents
        domain_state.update_robots(
            "User-agent: TestBot\nCrawl-delay: 10\n\nUser-agent: *\nCrawl-delay: 2".to_string(),
        );

        // TestBot should get 10 seconds
        let delay_testbot = effective_delay(&config, &domain_state, "TestBot");
        assert_eq!(delay_testbot, Duration::from_secs(10));

        // Other bots should get 2 seconds
        let delay_other = effective_delay(&config, &domain_state, "OtherBot");
        assert_eq!(delay_other, Duration::from_secs(2));
    }
}
