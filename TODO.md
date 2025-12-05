# Sumi-Ripple TODO List

**Project Status:** 85-90% Complete  
**Last Updated:** 2025-12-04  
**Blocking Issues:** 1 Critical  

## Table of Contents

1. [Critical Issues (P0)](#critical-issues-p0)
2. [Important Issues (P1)](#important-issues-p1)
3. [Enhancement Issues (P2)](#enhancement-issues-p2)
4. [Polish & Cleanup (P3)](#polish--cleanup-p3)
5. [Testing Strategy](#testing-strategy)

---

## Critical Issues (P0)

### 🚨 ISSUE #1: Scheduler Returns None When Domains Are Rate-Limited

**Status:** BLOCKING - All integration tests failing  
**Priority:** P0 - Must fix before any deployment  
**File:** `src/crawler/scheduler.rs`  
**Lines:** 103-124 (method `next_url`)

#### Problem Description

The `scheduler.next_url()` method returns `None` when no domain can currently accept a request due to rate limiting delays. This causes the coordinator's main loop to exit prematurely, thinking the crawl is complete when it actually just needs to wait for domains to become available.

**Current Behavior:**
1. First URL is processed successfully
2. Domain is marked with `last_request_time`
3. Next call to `next_url()` finds no domains passing `can_request()` check
4. Returns `None` immediately
5. Coordinator exits: "Frontier is empty, crawl complete"

**Expected Behavior:**
1. If no domains are ready, wait until one becomes ready
2. Continue checking or use a sleep/retry mechanism
3. Only return `None` when frontier is truly empty

#### Root Cause Analysis

```rust
// Current problematic code:
pub async fn next_url(&mut self) -> Option<ScheduledFetch> {
    let permit = self.global_semaphore.clone().acquire_owned().await.ok()?;
    let now = Instant::now();
    
    // This returns None if no domain can accept a request RIGHT NOW
    let position = self.frontier.iter().position(|queued| {
        let state = self.domain_states.entry(queued.domain.clone())
            .or_insert_with(DomainState::new);
        state.can_request(&self.config, now)
    })?; // <-- Problem: returns None instead of waiting
    
    let url = self.frontier.remove(position);
    Some(ScheduledFetch { url, _permit: permit })
}
```

The `?` operator on `position` returns `None` to the caller instead of waiting/retrying.

#### Solution Plan

**Option A: Active Wait Loop (Recommended)**

Implement an active wait loop that retries until a domain becomes available:

```rust
pub async fn next_url(&mut self) -> Option<ScheduledFetch> {
    // Return None only if frontier is truly empty
    if self.frontier.is_empty() {
        return None;
    }
    
    // Acquire global semaphore permit
    let permit = self.global_semaphore.clone().acquire_owned().await.ok()?;
    
    // Active wait loop: keep trying until we find a ready domain
    loop {
        let now = Instant::now();
        
        // Find the first URL whose domain can accept a request
        if let Some(position) = self.frontier.iter().position(|queued| {
            let state = self.domain_states.entry(queued.domain.clone())
                .or_insert_with(DomainState::new);
            state.can_request(&self.config, now)
        }) {
            // Found a ready URL, remove and return it
            let url = self.frontier.remove(position);
            return Some(ScheduledFetch { url, _permit: permit });
        }
        
        // No domains ready, calculate minimum wait time
        let min_wait = self.calculate_minimum_wait_time(now);
        
        // Sleep for the minimum time needed
        tokio::time::sleep(min_wait).await;
        
        // Check again if frontier is still not empty after sleep
        if self.frontier.is_empty() {
            return None;
        }
    }
}

// Helper method to calculate when the next domain will be ready
fn calculate_minimum_wait_time(&self, now: Instant) -> Duration {
    let mut min_wait = Duration::from_millis(100); // Default 100ms
    
    for queued in &self.frontier {
        if let Some(state) = self.domain_states.get(&queued.domain) {
            if let Some(wait) = state.time_until_next_request(&self.config, now) {
                if wait < min_wait {
                    min_wait = wait;
                }
            }
        }
    }
    
    // Add small buffer to ensure the domain is definitely ready
    min_wait + Duration::from_millis(10)
}
```

**Option B: Timeout-Based Approach (Alternative)**

Add a maximum wait timeout and return None only after exhausting retries:

```rust
pub async fn next_url(&mut self) -> Option<ScheduledFetch> {
    if self.frontier.is_empty() {
        return None;
    }
    
    let permit = self.global_semaphore.clone().acquire_owned().await.ok()?;
    let max_wait = Duration::from_secs(60); // Maximum 1 minute wait
    let start = Instant::now();
    
    loop {
        let now = Instant::now();
        
        if let Some(position) = self.frontier.iter().position(|queued| {
            let state = self.domain_states.entry(queued.domain.clone())
                .or_insert_with(DomainState::new);
            state.can_request(&self.config, now)
        }) {
            let url = self.frontier.remove(position);
            return Some(ScheduledFetch { url, _permit: permit });
        }
        
        // Check timeout
        if now.duration_since(start) > max_wait {
            tracing::warn!("Timeout waiting for domains to become available");
            return None;
        }
        
        // Sleep briefly before retry
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        if self.frontier.is_empty() {
            return None;
        }
    }
}
```

#### Implementation Steps

1. **Implement `calculate_minimum_wait_time()` helper method** (Option A)
   - Add to `impl Scheduler` block
   - Iterate through frontier and domain states
   - Find minimum time until any domain is ready

2. **Modify `next_url()` method**
   - Add early return if frontier is empty
   - Replace `?` operator with `if let Some` pattern
   - Add loop with sleep/retry logic
   - Add logging for wait times

3. **Update coordinator to handle potential long waits**
   - Consider adding timeout in coordinator's main loop
   - Add progress logging when waiting

4. **Add configuration option** (optional enhancement)
   - `max_scheduler_wait_seconds` in config
   - Allow users to control wait behavior

#### Testing Plan

**Unit Tests** (`src/crawler/scheduler.rs`):

```rust
#[tokio::test]
async fn test_scheduler_waits_for_rate_limited_domain() {
    // Setup scheduler with one URL
    let config = create_test_config(1, 1000); // 1 concurrent, 1000ms delay
    let url = create_test_queued_url("http://example.com/");
    let mut scheduler = Scheduler::new(config, vec![url.clone()], HashMap::new());
    
    // First request should succeed
    let first = scheduler.next_url().await;
    assert!(first.is_some());
    
    // Record the request
    scheduler.record_request("example.com");
    
    // Re-add URL to frontier
    scheduler.add_to_frontier(url);
    
    // Second request should wait ~1000ms, not return None
    let start = Instant::now();
    let second = scheduler.next_url().await;
    let elapsed = start.elapsed();
    
    assert!(second.is_some(), "Should not return None while waiting");
    assert!(elapsed.as_millis() >= 1000, "Should wait for rate limit");
}

#[tokio::test]
async fn test_scheduler_returns_none_only_when_empty() {
    let config = create_test_config(1, 100);
    let mut scheduler = Scheduler::new(config, vec![], HashMap::new());
    
    // Empty frontier should return None immediately
    let result = scheduler.next_url().await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_scheduler_calculates_minimum_wait() {
    let config = create_test_config(10, 500);
    let mut scheduler = Scheduler::new(config, vec![], HashMap::new());
    
    // Setup multiple domains with different last request times
    let now = Instant::now();
    let state1 = DomainState { 
        last_request_time: Some(now - Duration::from_millis(100)),
        ..Default::default()
    };
    let state2 = DomainState {
        last_request_time: Some(now - Duration::from_millis(300)),
        ..Default::default()
    };
    
    scheduler.domain_states.insert("example.com".to_string(), state1);
    scheduler.domain_states.insert("test.com".to_string(), state2);
    
    let min_wait = scheduler.calculate_minimum_wait_time(now);
    
    // Should be ~200ms (500ms required - 300ms elapsed for test.com)
    assert!(min_wait.as_millis() >= 200 && min_wait.as_millis() <= 250);
}
```

**Integration Tests** (should pass after fix):

- `test_full_crawl_single_domain` - Should process all 3 pages
- `test_robots_txt_respect` - Should process allowed pages with delays
- `test_crawl_with_depth_limit` - Should process pages respecting depth
- `test_content_type_handling` - Should detect content mismatches

**Manual Testing:**

```bash
# Test with real crawl using example config
cargo run --release -- examples/sample_config.toml --fresh

# Check logs for proper waiting behavior
RUST_LOG=debug cargo run -- examples/sample_config.toml --fresh

# Verify pages are processed beyond the first one
cargo run -- examples/sample_config.toml --stats
```

#### Success Criteria

- [ ] All 4 integration tests pass
- [ ] Unit tests for wait logic pass
- [ ] Scheduler waits instead of exiting when domains are rate-limited
- [ ] Real crawls process multiple pages with proper delays
- [ ] No premature exits with "Frontier is empty" message
- [ ] Logging shows "Waiting for domains to become available" when appropriate

#### Estimated Effort

- Implementation: 2-3 hours
- Testing: 1-2 hours
- **Total: 3-5 hours**

---

## Important Issues (P1)

### ISSUE #2: Robots.txt Cache Not Properly Persisted

**Status:** TODO  
**Priority:** P1  
**File:** `src/crawler/coordinator.rs`  
**Lines:** 532-540

#### Problem Description

The coordinator fetches robots.txt but has a TODO comment indicating it doesn't properly cache the parsed `ParsedRobots` object. Currently, it re-parses or returns an allow-all fallback.

```rust
// Current code:
async fn get_or_fetch_robots(&mut self, domain: &str) -> Result<ParsedRobots, SumiError> {
    // ... fetch logic ...
    } else {
        // Use cached robots.txt
        tracing::debug!("Using cached robots.txt for domain: {}", domain);
        
        // TODO: Improve this to actually cache the parsed robots
        Ok(ParsedRobots::allow_all())
    }
}
```

#### Solution Plan

**Option A: Store ParsedRobots in DomainState (Recommended)**

Modify `DomainState` to store the parsed robots instead of just the string:

1. Update `src/state/domain_state.rs`:

```rust
use crate::robots::ParsedRobots;

#[derive(Debug, Clone)]
pub struct DomainState {
    pub request_count: u32,
    pub last_request_time: Option<Instant>,
    pub rate_limited: bool,
    
    // Replace String with ParsedRobots
    pub robots_txt: Option<ParsedRobots>,
    pub robots_fetched_at: Option<DateTime<Utc>>,
}

impl DomainState {
    pub fn update_robots(&mut self, robots: ParsedRobots) {
        let now = Utc::now();
        self.robots_txt = Some(robots);
        self.robots_fetched_at = Some(now);
    }
    
    pub fn get_robots(&self) -> Option<&ParsedRobots> {
        if self.is_robots_stale() {
            None
        } else {
            self.robots_txt.as_ref()
        }
    }
}
```

2. Update `ParsedRobots` to implement `Clone`:

```rust
// In src/robots/parser.rs
#[derive(Debug, Clone)]
pub struct ParsedRobots {
    allow_all: bool,
    content: String,
    parsed: Option<robotstxt::DefaultMatcher>,
}

// Need to handle robotstxt::DefaultMatcher not being Clone
// Solution: Re-parse when cloning
impl Clone for ParsedRobots {
    fn clone(&self) -> Self {
        if self.allow_all {
            return Self::allow_all();
        }
        
        Self::parse(&self.content).unwrap_or_else(|_| Self::allow_all())
    }
}
```

3. Update coordinator to use cached parsed robots:

```rust
async fn get_or_fetch_robots(&mut self, domain: &str) -> Result<ParsedRobots, SumiError> {
    // Check scheduler's domain state first
    if let Some(robots) = self.scheduler.get_cached_robots(domain) {
        return Ok(robots.clone());
    }
    
    // Fetch robots.txt
    let robots_url = format!("https://{}/robots.txt", domain);
    
    match fetch_url(&self.client, &robots_url).await {
        FetchResult::Success { body, .. } => {
            let robots = ParsedRobots::parse(&body)
                .unwrap_or_else(|_| ParsedRobots::allow_all());
            
            // Cache in scheduler
            self.scheduler.cache_robots(domain, robots.clone());
            
            Ok(robots)
        }
        _ => {
            // On fetch failure, use allow-all and cache it
            let robots = ParsedRobots::allow_all();
            self.scheduler.cache_robots(domain, robots.clone());
            Ok(robots)
        }
    }
}
```

4. Add methods to `Scheduler`:

```rust
// In src/crawler/scheduler.rs
impl Scheduler {
    pub fn get_cached_robots(&self, domain: &str) -> Option<&ParsedRobots> {
        self.domain_states.get(domain)?.get_robots()
    }
    
    pub fn cache_robots(&mut self, domain: &str, robots: ParsedRobots) {
        let state = self.domain_states.entry(domain.to_string())
            .or_insert_with(DomainState::new);
        state.update_robots(robots);
    }
}
```

**Option B: Separate Robots Cache**

Create a dedicated cache structure that persists across domain states:

```rust
// Add to coordinator
struct RobotsCache {
    cache: HashMap<String, (ParsedRobots, DateTime<Utc>)>,
}

impl RobotsCache {
    fn get(&self, domain: &str) -> Option<&ParsedRobots> {
        if let Some((robots, fetched_at)) = self.cache.get(domain) {
            let age = Utc::now() - *fetched_at;
            if age < chrono::Duration::hours(24) {
                return Some(robots);
            }
        }
        None
    }
    
    fn insert(&mut self, domain: String, robots: ParsedRobots) {
        self.cache.insert(domain, (robots, Utc::now()));
    }
}
```

#### Implementation Steps

1. Make `ParsedRobots` cloneable
2. Update `DomainState` to store parsed robots
3. Add cache methods to `Scheduler`
4. Update coordinator's `get_or_fetch_robots`
5. Remove TODO comment

#### Testing Plan

```rust
#[tokio::test]
async fn test_robots_cache_is_used() {
    let mut coordinator = create_test_coordinator().await;
    
    // First fetch should hit the network
    let robots1 = coordinator.get_or_fetch_robots("example.com").await.unwrap();
    
    // Second fetch should use cache (verify no network call)
    let robots2 = coordinator.get_or_fetch_robots("example.com").await.unwrap();
    
    // Should be equivalent
    assert_eq!(
        robots1.is_allowed("/test", "TestBot"),
        robots2.is_allowed("/test", "TestBot")
    );
}

#[tokio::test]
async fn test_stale_robots_refetched() {
    // Create coordinator with stale robots in domain state
    // Verify it fetches fresh copy
}
```

#### Success Criteria

- [ ] Robots.txt parsed once per domain per 24 hours
- [ ] Cached parsed robots used on subsequent requests
- [ ] No performance degradation from re-parsing
- [ ] Tests verify caching behavior
- [ ] TODO comment removed

#### Estimated Effort

- Implementation: 2-3 hours
- Testing: 1 hour
- **Total: 3-4 hours**

---

### ISSUE #3: Priority Queue Not Implemented

**Status:** TODO  
**Priority:** P1  
**File:** `src/crawler/scheduler.rs`  
**Lines:** 50-60, 133-136

#### Problem Description

The frontier uses a simple `Vec<QueuedUrl>` instead of a proper priority queue. The TODO comments acknowledge this:

```rust
/// Frontier queue of URLs to fetch
/// TODO: Replace with proper priority queue implementation
frontier: Vec<QueuedUrl>,

pub fn add_to_frontier(&mut self, url: QueuedUrl) {
    // TODO: Insert into priority queue maintaining order
    self.frontier.push(url);
}
```

This means:
- URLs are not processed in priority order
- Quality domains aren't prioritized over discovered domains
- Performance is O(n) for finding next URL instead of O(log n)

#### Solution Plan

**Option A: Use BinaryHeap (Recommended)**

Replace `Vec<QueuedUrl>` with `BinaryHeap<QueuedUrl>`:

```rust
use std::collections::BinaryHeap;
use std::cmp::{Ordering, Reverse};

// Make QueuedUrl orderable by priority
impl Ord for QueuedUrl {
    fn cmp(&self, other: &Self) -> Ordering {
        // Lower priority value = higher priority
        // Reverse so BinaryHeap pops lowest priority value first
        other.priority.cmp(&self.priority)
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

pub struct Scheduler {
    global_semaphore: Arc<Semaphore>,
    domain_states: HashMap<String, DomainState>,
    frontier: BinaryHeap<QueuedUrl>,  // Changed from Vec
    config: CrawlerConfig,
}

impl Scheduler {
    pub fn add_to_frontier(&mut self, url: QueuedUrl) {
        self.frontier.push(url);  // Now O(log n) instead of O(1)
    }
    
    pub fn frontier_size(&self) -> usize {
        self.frontier.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.frontier.is_empty()
    }
}
```

**Challenge:** `BinaryHeap` doesn't allow iterating and removing by position. Need to modify the search logic:

```rust
pub async fn next_url(&mut self) -> Option<ScheduledFetch> {
    if self.frontier.is_empty() {
        return None;
    }
    
    let permit = self.global_semaphore.clone().acquire_owned().await.ok()?;
    
    loop {
        let now = Instant::now();
        
        // Collect URLs that are ready, maintaining priority order
        let mut temp_heap = BinaryHeap::new();
        let mut found = None;
        
        // Pop URLs until we find one that's ready
        while let Some(queued) = self.frontier.pop() {
            let state = self.domain_states.entry(queued.domain.clone())
                .or_insert_with(DomainState::new);
            
            if state.can_request(&self.config, now) {
                // Found a ready URL
                found = Some(queued);
                break;
            } else {
                // Not ready, save for later
                temp_heap.push(queued);
            }
        }
        
        // Put back the URLs we couldn't use
        while let Some(queued) = temp_heap.pop() {
            self.frontier.push(queued);
        }
        
        if let Some(url) = found {
            return Some(ScheduledFetch { url, _permit: permit });
        }
        
        // No domains ready, wait and retry
        if self.frontier.is_empty() {
            return None;
        }
        
        let min_wait = self.calculate_minimum_wait_time(now);
        tokio::time::sleep(min_wait).await;
    }
}
```

**Option B: Custom Priority Queue**

Implement a custom priority queue that allows peeking by priority and removing by domain:

```rust
struct PriorityFrontier {
    queue: Vec<QueuedUrl>,
}

impl PriorityFrontier {
    fn new() -> Self {
        Self { queue: Vec::new() }
    }
    
    fn push(&mut self, url: QueuedUrl) {
        // Insert maintaining sorted order
        let pos = self.queue.binary_search_by(|probe| {
            probe.priority.cmp(&url.priority)
        }).unwrap_or_else(|e| e);
        self.queue.insert(pos, url);
    }
    
    fn find_and_remove<F>(&mut self, predicate: F) -> Option<QueuedUrl>
    where
        F: Fn(&QueuedUrl) -> bool,
    {
        if let Some(pos) = self.queue.iter().position(predicate) {
            Some(self.queue.remove(pos))
        } else {
            None
        }
    }
    
    fn len(&self) -> usize {
        self.queue.len()
    }
    
    fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}
```

#### Implementation Steps

1. Choose approach (Option A recommended for standard lib)
2. Implement `Ord`, `PartialOrd`, `Eq`, `PartialEq` for `QueuedUrl`
3. Replace `Vec<QueuedUrl>` with `BinaryHeap<QueuedUrl>`
4. Update `next_url()` to work with heap semantics
5. Update `add_to_frontier()` (already works with `.push()`)
6. Test priority ordering

#### Testing Plan

```rust
#[test]
fn test_queued_url_ordering() {
    let url1 = QueuedUrl { priority: 0, ..create_test_url("http://a.com") };
    let url2 = QueuedUrl { priority: 10, ..create_test_url("http://b.com") };
    let url3 = QueuedUrl { priority: 5, ..create_test_url("http://c.com") };
    
    let mut heap = BinaryHeap::new();
    heap.push(url1.clone());
    heap.push(url2.clone());
    heap.push(url3.clone());
    
    // Should pop in priority order: 0, 5, 10
    assert_eq!(heap.pop().unwrap().priority, 0);
    assert_eq!(heap.pop().unwrap().priority, 5);
    assert_eq!(heap.pop().unwrap().priority, 10);
}

#[tokio::test]
async fn test_scheduler_respects_priority() {
    let config = create_test_config(10, 0); // No delays for this test
    
    let low_priority = QueuedUrl { priority: 100, ..create_test_url("http://low.com") };
    let high_priority = QueuedUrl { priority: 0, ..create_test_url("http://high.com") };
    
    let mut scheduler = Scheduler::new(config, vec![low_priority, high_priority], HashMap::new());
    
    // Should get high priority first
    let first = scheduler.next_url().await.unwrap();
    assert_eq!(first.url.priority, 0);
}

#[tokio::test]
async fn test_priority_with_rate_limiting() {
    // Add high priority URL for domain that's rate limited
    // Add low priority URL for domain that's available
    // Should get low priority because high priority domain isn't ready
}
```

#### Success Criteria

- [ ] Frontier maintains priority order
- [ ] Quality domains (priority 0) fetched before discovered (priority 10)
- [ ] Tests verify ordering
- [ ] Performance improved for large frontiers
- [ ] TODO comments removed

#### Estimated Effort

- Implementation: 3-4 hours
- Testing: 1-2 hours
- **Total: 4-6 hours**

---

## Enhancement Issues (P2)

### ISSUE #4: Summary Generation Missing Data

**Status:** TODO  
**Priority:** P2  
**File:** `src/output/mod.rs`  
**Lines:** 144-154

#### Problem Description

The `generate_summary` function has TODO comments for missing data:

```rust
depth_breakdown: HashMap::new(), // TODO: Implement depth breakdown
discovered_domains: vec![],      // TODO: Query discovered domains
quality_domains: vec![],         // TODO: Extract from config or storage
```

This means the markdown summary is missing useful information.

#### Solution Plan

**Depth Breakdown:**

```rust
// Add to storage trait
pub trait Storage {
    // ... existing methods ...
    
    /// Returns count of pages at each depth level
    fn get_depth_breakdown(&self) -> Result<HashMap<u32, usize>, StorageError>;
}

// Implement for SqliteStorage
impl Storage for SqliteStorage {
    fn get_depth_breakdown(&self) -> Result<HashMap<u32, usize>, StorageError> {
        let query = "
            SELECT depth, COUNT(DISTINCT page_id) as count
            FROM page_depths
            GROUP BY depth
            ORDER BY depth
        ";
        
        let mut stmt = self.conn.prepare(query)?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, u32>(0)?, row.get::<_, usize>(1)?))
        })?;
        
        let mut breakdown = HashMap::new();
        for row in rows {
            let (depth, count) = row?;
            breakdown.insert(depth, count);
        }
        
        Ok(breakdown)
    }
}
```

**Discovered Domains:**

```rust
// Add to storage trait
fn get_discovered_domains(&self) -> Result<Vec<String>, StorageError>;

// Implement
impl Storage for SqliteStorage {
    fn get_discovered_domains(&self) -> Result<Vec<String>, StorageError> {
        let query = "
            SELECT DISTINCT domain
            FROM pages
            ORDER BY domain
        ";
        
        let mut stmt = self.conn.prepare(query)?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        
        let mut domains = Vec::new();
        for row in rows {
            domains.push(row?);
        }
        
        Ok(domains)
    }
}
```

**Quality Domains:**

```rust
// Pass config to generate_summary or store in database
pub fn generate_summary(
    storage: &dyn Storage,
    config: Option<&Config>
) -> Result<CrawlSummary, SumiError> {
    // ... existing code ...
    
    let quality_domains = if let Some(cfg) = config {
        cfg.quality.iter().map(|q| q.domain.clone()).collect()
    } else {
        // Try to get from database metadata
        storage.get_quality_domains().unwrap_or_default()
    };
    
    // ... rest of summary ...
}
```

#### Implementation Steps

1. Add new storage trait methods
2. Implement for SqliteStorage
3. Update `generate_summary` to call new methods
4. Update callers to pass config if available
5. Update markdown formatter to display new data

#### Testing Plan

```rust
#[test]
fn test_depth_breakdown() {
    let storage = create_test_storage();
    
    // Insert pages at different depths
    let page1 = storage.insert_page("http://a.com/", "a.com", 1).unwrap();
    let page2 = storage.insert_page("http://b.com/", "b.com", 1).unwrap();
    storage.upsert_depth(page1, "a.com", 0).unwrap();
    storage.upsert_depth(page2, "a.com", 1).unwrap();
    
    let breakdown = storage.get_depth_breakdown().unwrap();
    
    assert_eq!(breakdown.get(&0), Some(&1));
    assert_eq!(breakdown.get(&1), Some(&1));
}

#[test]
fn test_discovered_domains() {
    let storage = create_test_storage();
    storage.insert_page("http://a.com/", "a.com", 1).unwrap();
    storage.insert_page("http://b.com/", "b.com", 1).unwrap();
    
    let domains = storage.get_discovered_domains().unwrap();
    
    assert_eq!(domains.len(), 2);
    assert!(domains.contains(&"a.com".to_string()));
    assert!(domains.contains(&"b.com".to_string()));
}
```

#### Success Criteria

- [ ] Depth breakdown shows page counts per depth
- [ ] Discovered domains list is populated
- [ ] Quality domains list is populated
- [ ] Markdown summary displays all data
- [ ] TODO comments removed

#### Estimated Effort

- Implementation: 2-3 hours
- Testing: 1 hour
- **Total: 3-4 hours**

---

## Polish & Cleanup (P3)

### ISSUE #5: Unused Code Warnings

**Status:** TODO  
**Priority:** P3  
**Files:** Multiple

#### Current Warnings

```
warning: method `has_visited` is never used
  --> src/crawler/fetcher.rs:114:12

warning: function `check_content_type` is never used
  --> src/crawler/fetcher.rs:621:14

warning: function `effective_delay` is never used
  --> src/crawler/scheduler.rs:208:8

warning: function `get_schema_version` is never used
  --> src/storage/schema.rs:135:8

warning: unused variable: `config_hash`
  --> src/main.rs:63:18
```

#### Solution Plan

For each warning:

1. **`has_visited` in `src/crawler/fetcher.rs`**
   - Used in redirect chain logic
   - Either use it or mark with `#[allow(dead_code)]` if it's for future use
   - Decision: Remove if not used in redirect handling

2. **`check_content_type` in `src/crawler/fetcher.rs`**
   - Appears to be a helper function
   - Either integrate into `fetch_url` or remove
   - Decision: Integrate into fetch logic or mark as public helper

3. **`effective_delay` in `src/crawler/scheduler.rs`**
   - Calculates delay considering robots.txt crawl-delay
   - Decision: Use in rate limiting or remove

4. **`get_schema_version` in `src/storage/schema.rs`**
   - Useful for schema migrations
   - Decision: Keep and mark with `#[allow(dead_code)]` for future use

5. **`config_hash` in `src/main.rs`**
   - Loaded but not used
   - Decision: Prefix with underscore or use for run tracking

#### Implementation Steps

```bash
# Fix all warnings at once
cargo fix --lib --allow-dirty --allow-staged

# Or fix manually:
# 1. Prefix unused variables with underscore
# 2. Add #[allow(dead_code)] to intentionally unused functions
# 3. Remove truly unused code
```

#### Success Criteria

- [ ] `cargo build` produces no warnings
- [ ] `cargo clippy` passes cleanly
- [ ] No functionality lost

#### Estimated Effort

- **Total: 1 hour**

---

### ISSUE #6: Add More Integration Test Scenarios

**Status:** TODO  
**Priority:** P3

#### Missing Test Scenarios

1. **Resume functionality test**
   - Create interrupted crawl
   - Resume and verify continuation
   - Check no duplicate work

2. **Multiple quality domains**
   - Test with 2+ quality domains
   - Verify independent depth tracking
   - Check priority handling

3. **Error recovery test**
   - Inject network errors
   - Verify retry logic (once implemented)
   - Check error state handling

4. **Blacklist/stub domain test**
   - Verify blacklisted URLs not crawled
   - Verify stubbed URLs recorded but not crawled
   - Check links to these domains tracked

5. **Large frontier test**
   - Add 100+ URLs to frontier
   - Verify memory usage reasonable
   - Check performance acceptable

#### Estimated Effort

- **Total: 4-6 hours**

---

## Testing Strategy

### Overall Testing Approach

#### Test Pyramid

```
                  /\
                 /  \
                /E2E \         5-10 integration tests
               /______\
              /        \
             / Integr.  \      10-20 integration tests
            /____________\
           /              \
          /   Unit Tests   \   100+ unit tests
         /__________________\
```

#### Current Status

- **Unit Tests:** ✅ 165 passing
- **Integration Tests:** ❌ 4 failing (will pass after Issue #1 fixed)
- **E2E Tests:** ❌ None yet

### Test Execution Order

**After fixing each issue, run:**

```bash
# 1. Unit tests (should always pass)
cargo test --lib

# 2. Integration tests (should pass after Issue #1)
cargo test --test integration_tests

# 3. Manual smoke test
cargo run --release -- examples/sample_config.toml --dry-run
cargo run --release -- examples/sample_config.toml --fresh

# 4. Check output
cargo run --release -- examples/sample_config.toml --stats
cargo run --release -- examples/sample_config.toml --export-summary
```

### Regression Testing

Before marking any issue as "Done", verify:

1. All unit tests still pass
2. All integration tests still pass
3. No new warnings introduced
4. Example config still works
5. CLI commands still work

### Performance Testing

Once all P0 and P1 issues are fixed:

```bash
# Create a test config with 10 seed URLs
# Run and measure:
time cargo run --release -- test_config.toml --fresh

# Expected performance:
# - 10+ pages/sec for local mock servers
# - 2-5 pages/sec for real websites (with rate limiting)
# - <100MB memory usage for 1000 pages
```

---

## Implementation Priority & Timeline

### Sprint 1: Critical Fixes (Week 1)

**Goal:** Get crawler fully functional

- [ ] **Day 1-2:** Issue #1 - Fix scheduler rate limiting logic
  - Implement wait loop
  - Add helper methods
  - Write unit tests
  - Verify integration tests pass

- [ ] **Day 3:** Issue #2 - Fix robots.txt caching
  - Update DomainState
  - Modify coordinator
  - Add tests

- [ ] **Day 4-5:** Issue #3 - Implement priority queue
  - Add Ord/PartialOrd to QueuedUrl
  - Switch to BinaryHeap
  - Update next_url logic
  - Test priority ordering

**Deliverable:** Fully functional crawler that can crawl multiple pages respecting rate limits and priority

### Sprint 2: Enhancements (Week 2)

**Goal:** Complete features and improve quality

- [ ] **Day 1-2:** Issue #4 - Complete summary generation
  - Add storage methods
  - Implement queries
  - Update formatter

- [ ] **Day 3:** Issue #5 - Clean up warnings
  - Fix or suppress all warnings
  - Run clippy
  - Document intentionally unused code

- [ ] **Day 4-5:** Issue #6 - Add more tests
  - Resume functionality
  - Multiple domains
  - Error handling

**Deliverable:** Production-ready crawler with comprehensive tests

### Sprint 3: Polish & Documentation (Week 3)

**Goal:** Make it maintainable and deployable

- [ ] Performance optimization
- [ ] Documentation improvements
- [ ] Example configurations
- [ ] Deployment guide
- [ ] Monitoring/logging improvements

---

## Success Metrics

### Code Quality

- [ ] Zero compiler warnings
- [ ] Zero clippy warnings
- [ ] 100% of unit tests passing
- [ ] 100% of integration tests passing
- [ ] Code coverage >80% for critical paths

### Functionality

- [ ] Can crawl multiple pages from single domain
- [ ] Can crawl multiple quality domains
- [ ] Respects robots.txt correctly
- [ ] Enforces rate limiting properly
- [ ] Tracks depth correctly
- [ ] Generates accurate summaries
- [ ] Can resume interrupted crawls

### Performance

- [ ] Processes ≥5 pages/sec (with rate limiting)
- [ ] Memory usage <100MB for 1000 pages
- [ ] Database size reasonable (<1MB per 100 pages)
- [ ] No memory leaks over long runs

### Usability

- [ ] CLI intuitive and well-documented
- [ ] Error messages helpful
- [ ] Logging informative but not overwhelming
- [ ] Configuration validation clear
- [ ] Summary reports useful

---

## Notes & Conventions

### Code Style

- Follow Rust standard formatting (`cargo fmt`)
- Use clippy suggestions (`cargo clippy`)
- Document public APIs with rustdoc
- Add examples to documentation
- Use meaningful variable names

### Git Workflow

- Create branch for each issue: `fix/issue-N-description`
- Commit messages: `Fix #N: Description`
- One issue per PR
- All tests must pass before merge

### Testing Conventions

- Unit tests in same file as code: `#[cfg(test)] mod tests`
- Integration tests in `tests/` directory
- Use descriptive test names: `test_scheduler_waits_for_rate_limited_domain`
- Include both positive and negative test cases
- Mock external dependencies (HTTP, time)

### Documentation

- Update README.md when behavior changes
- Update IMPLEMENTATION_SUMMARY.md when features complete
- Keep TODO.md current
- Add inline comments for complex logic

---

## Questions & Decisions Log

### Q1: Should scheduler block indefinitely or timeout?

**Decision:** Implement timeout with configurable duration (default 60s). This prevents deadlock scenarios while allowing legitimate delays.

### Q2: How to handle robots.txt that's unreachable?

**Decision:** Use allow-all policy with 24-hour cache. Log warning but don't block crawl.

### Q3: Should priority queue be thread-safe?

**Decision:** Not needed yet since Scheduler is single-threaded. Consider for future if parallelizing.

### Q4: How to handle schema migrations in future?

**Decision:** Keep `get_schema_version()` function for future use. Add migration system in v2.0.

---

## Conclusion

The Sumi-Ripple crawler is **85-90% complete** with a solid foundation. The main blocker is the scheduler's rate limiting logic (Issue #1), which prevents the crawler from processing multiple pages. Once fixed, the system should be fully functional.

**Estimated time to production-ready:**
- Sprint 1 (Critical): 5 days
- Sprint 2 (Enhancement): 5 days  
- Sprint 3 (Polish): 5 days
- **Total: 15 days / 3 weeks**

**Next Steps:**
1. Fix Issue #1 immediately (highest priority)
2. Verify all integration tests pass
3. Proceed with Issues #2 and #3
4. Complete Sprint 1 deliverables
5. Reassess and plan Sprint 2

---

**Last Updated:** 2024  
**Maintained By:** Development Team  
**Status:** Active Development