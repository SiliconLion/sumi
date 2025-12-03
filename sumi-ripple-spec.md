# Sumi-Ripple: Web Terrain Mapper

## Implementation Specification v1.0

---

## 1. Project Overview

**Sumi-Ripple** is a Rust-based web crawler designed to map the "web terrain" surrounding a set of curated domains (the "quality list"). It discovers and records the link structure between websites while respecting rate limits, robots.txt directives, and general web politeness standards.

### 1.1 Core Concepts

| Term | Definition |
|------|------------|
| **Quality Domain** | A domain we want to fully explore. All reachable pages are crawled, and all outbound links are followed (up to `max-depth`). |
| **Blacklisted Domain** | A domain we have no interest in visiting. Links to these domains are recorded but never followed. |
| **Stub Domain** | A domain we don't want to crawl, but whose pages may be of interest. Individual page URLs are recorded but never visited. |
| **Depth** | The link distance from a quality domain. Quality domains are depth 0; pages they link to are depth 1; and so on. |
| **Terrain Map** | The resulting graph of pages and their link relationships. |

### 1.2 High-Level Behavior

1. Load configuration from a TOML file
2. Restore state from persistent storage (if resuming)
3. For each quality domain, begin crawling from the specified seed URLs
4. Recursively discover and crawl new domains up to `max-depth`
5. Respect all rate limits and politeness constraints
6. Persist state continuously to survive crashes
7. Output results to SQLite database and human-readable Markdown summary

---

## 2. Configuration

### 2.1 TOML Structure

```toml
# =============================================================================
# Sumi-Ripple Configuration
# =============================================================================

[crawler]
max-depth = 3
max-concurrent-pages-open = 10
minimum-time-on-page = 1000          # milliseconds between requests to same domain
max-domain-requests = 500            # per run, per domain

[user-agent]
crawler-name = "SumiRipple"
crawler-version = "1.0"
contact-url = "https://example.com/about-our-crawler"
contact-email = "crawler-admin@example.com"

# Generated User-Agent string:
# "SumiRipple/1.0 (+https://example.com/about-our-crawler; crawler-admin@example.com)"

[output]
database-path = "./sumi-ripple.db"
summary-path = "./crawl-summary.md"

# =============================================================================
# Quality List - Domains to fully explore
# =============================================================================

[[quality]]
domain = "example.com"
seeds = [
    "https://example.com/",
    "https://example.com/blog/"
]

[[quality]]
domain = "*.another-site.org"        # Wildcard: matches all subdomains
seeds = [
    "https://blog.another-site.org/",
    "https://docs.another-site.org/getting-started"
]

# =============================================================================
# Blacklist - Domains to never visit (but record references to)
# =============================================================================

[[blacklist]]
domain = "ads.example.net"

[[blacklist]]
domain = "*.tracking-service.com"    # Wildcard supported

[[blacklist]]
domain = "known-spam-site.org"

# =============================================================================
# Stub List - Record page URLs but don't visit
# =============================================================================

[[stub]]
domain = "github.com"                # We want to know what repos are linked

[[stub]]
domain = "*.wikipedia.org"           # Record which articles are referenced

[[stub]]
domain = "twitter.com"
```

### 2.2 Configuration Validation Rules

| Field | Type | Constraints |
|-------|------|-------------|
| `max-depth` | u32 | ≥ 0 |
| `max-concurrent-pages-open` | u32 | ≥ 1, ≤ 100 |
| `minimum-time-on-page` | u64 (ms) | ≥ 100 |
| `max-domain-requests` | u32 | ≥ 1 |
| `crawler-name` | String | Non-empty, alphanumeric + hyphens |
| `contact-url` | String | Valid URL (optional but recommended) |
| `contact-email` | String | Valid email format (optional but recommended) |
| `domain` | String | Valid domain or wildcard pattern |
| `seeds` | Vec<String> | Non-empty for quality domains, valid HTTPS URLs |

### 2.3 Wildcard Pattern Semantics

The pattern `*.example.com` matches:
- `example.com` (the bare domain)
- `blog.example.com` (single subdomain)
- `api.v2.example.com` (nested subdomains)

Implementation: Strip the `*.` prefix and check if the candidate domain equals the base or ends with `.{base}`.

```rust
fn matches_wildcard(pattern: &str, candidate: &str) -> bool {
    if let Some(base) = pattern.strip_prefix("*.") {
        candidate == base || candidate.ends_with(&format!(".{}", base))
    } else {
        candidate == pattern
    }
}
```

---

## 3. URL Handling

### 3.1 URL Normalization

All URLs must be normalized before storage or comparison. The normalization process (in order):

1. **Parse** the URL; reject if malformed
2. **Enforce HTTPS**: If scheme is `http`, attempt `https`. If HTTPS is unavailable, mark URL as unreachable
3. **Lowercase** the host/domain
4. **Remove `www.` prefix** from domain (e.g., `www.example.com` → `example.com`)
5. **Normalize path**:
   - Decode unnecessarily percent-encoded characters
   - Remove dot segments (`.` and `..`)
   - Remove trailing slash (except for root path `/`)
   - Empty path becomes `/`
6. **Remove fragment** (everything after `#`)
7. **Remove tracking query parameters**:
   - `utm_*` (all UTM parameters)
   - `fbclid`, `gclid`, `mc_eid`
   - `ref`, `source` (configurable in future versions)
8. **Sort remaining query parameters** alphabetically
9. **Remove empty query string** (trailing `?`)

#### Examples

| Input | Normalized Output |
|-------|-------------------|
| `http://example.com/page` | `https://example.com/page` |
| `https://www.example.com/` | `https://example.com/` |
| `https://example.com/page/` | `https://example.com/page` |
| `https://example.com/page#section` | `https://example.com/page` |
| `https://example.com/page?utm_source=twitter` | `https://example.com/page` |
| `https://example.com/page?b=2&a=1` | `https://example.com/page?a=1&b=2` |
| `https://example.com/a/../b/./c` | `https://example.com/b/c` |
| `https://EXAMPLE.COM/Page` | `https://example.com/Page` (path case preserved) |

### 3.2 Domain Extraction

Extract the domain from a normalized URL for:
- Rate limiting decisions
- Domain classification (quality/blacklist/stub)
- robots.txt lookup

```rust
fn extract_domain(url: &Url) -> Option<String> {
    url.host_str().map(|h| h.to_lowercase())
}
```

### 3.3 Domain Classification

When encountering a URL, classify its domain:

```rust
enum DomainClassification {
    Quality,      // In quality list
    Blacklisted,  // In blacklist
    Stubbed,      // In stub list  
    Discovered,   // New domain, not in any list
}
```

Classification priority (first match wins):
1. Check blacklist (including wildcards)
2. Check stub list (including wildcards)
3. Check quality list (including wildcards)
4. Default to `Discovered`

---

## 4. State Machine

### 4.1 Page States

Each URL in the system has a state:

```rust
enum PageState {
    // Active states
    Discovered,      // URL found, not yet queued for crawling
    Queued,          // In the crawl queue, waiting to be fetched
    Fetching,        // HTTP request in progress
    
    // Terminal success states  
    Processed,       // Successfully crawled and links extracted
    
    // Terminal skip states
    Blacklisted,     // Domain is blacklisted; recorded but not visited
    Stubbed,         // Domain is stubbed; recorded but not visited
    
    // Terminal error states
    DeadLink,        // 404 Not Found
    Unreachable,     // Connection refused, no HTTPS, or other permanent failure
    RateLimited,     // Received 429; domain crawling suspended
    Failed,          // Other errors after retries exhausted
    
    // Special states
    DepthExceeded,   // Beyond max-depth from all quality origins
    RequestLimitHit, // max-domain-requests reached for this domain
    ContentMismatch, // HEAD indicated HTML but actual content wasn't, or vice versa
}
```

### 4.2 State Transitions

```
                                    ┌─────────────┐
                                    │  Discovered │
                                    └──────┬──────┘
                                           │
                         ┌─────────────────┼─────────────────┐
                         │                 │                 │
                         ▼                 ▼                 ▼
                  ┌────────────┐    ┌────────────┐    ┌────────────┐
                  │ Blacklisted│    │  Stubbed   │    │  Queued    │
                  └────────────┘    └────────────┘    └─────┬──────┘
                                                           │
                                           ┌───────────────┼───────────────┐
                                           │               │               │
                                           ▼               ▼               ▼
                                    ┌────────────┐  ┌────────────┐  ┌────────────┐
                                    │DepthExceed │  │RequestLimit│  │  Fetching  │
                                    └────────────┘  └────────────┘  └─────┬──────┘
                                                                         │
                         ┌──────────────┬──────────────┬─────────────────┼────────────────┐
                         │              │              │                 │                │
                         ▼              ▼              ▼                 ▼                ▼
                  ┌────────────┐ ┌────────────┐ ┌────────────┐   ┌────────────┐   ┌────────────┐
                  │  DeadLink  │ │Unreachable │ │RateLimited │   │   Failed   │   │ Processed  │
                  │   (404)    │ │(conn fail) │ │   (429)    │   │ (5xx,etc)  │   │            │
                  └────────────┘ └────────────┘ └────────────┘   └────────────┘   └────────────┘
```

### 4.3 Retry Logic

| Condition | Action |
|-----------|--------|
| HTTP 404 | Immediate → `DeadLink` |
| HTTP 429 | Immediate → `RateLimited`, suspend domain |
| HTTP 5xx | Retry up to 3 times, 5 second delay between attempts |
| Timeout | Retry up to 3 times, 5 second delay between attempts |
| Connection refused | Immediate → `Unreachable` |
| TLS/SSL error | Immediate → `Unreachable` |
| Redirect loop detected | Immediate → `Failed` with reason |
| Redirect chain > 10 | Immediate → `Failed` with reason |
| Redirect to blacklist | Stop redirect, mark target as `Blacklisted` |
| Redirect to stub | Stop redirect, mark target as `Stubbed` |

---

## 5. Depth Tracking

### 5.1 Multi-Origin Depth

A single URL can be reachable from multiple quality domains at different depths. We track depth separately per quality origin.

```rust
struct DepthRecord {
    url: String,
    quality_origin: String,  // The quality domain this depth is relative to
    depth: u32,
}
```

Example:
- `quality-a.com` links to `new-site.com/page1` → depth 1 from quality-a.com
- `quality-b.com` also links to `new-site.com/page1` → depth 1 from quality-b.com
- `new-site.com/page1` links to `other.com/x` → depth 2 from both origins

### 5.2 Depth Rules

1. Seed URLs of quality domains start at depth 0
2. All internal pages of a quality domain are depth 0
3. A link from a depth N page to a new page creates depth N+1
4. If a page already has a depth record for an origin, keep the minimum
5. A page is crawlable if ANY of its depth records is ≤ `max-depth`

### 5.3 Quality-to-Quality Links

When `quality-A.com` links to `quality-B.com`:
- From A's perspective: B's pages are depth 1
- B's own exploration: B's pages are depth 0

Both depth records are stored. B is crawled fully because it has a depth 0 record.

---

## 6. Crawling Logic

### 6.1 Initialization

```
1. Parse and validate configuration
2. Connect to SQLite database (create if needed)
3. Check for existing run state:
   a. If resuming: load frontier and domain states
   b. If fresh: initialize new run record
4. Build domain classifiers from config
5. Initialize HTTP client with User-Agent
6. Start scheduler and worker pool
```

### 6.2 Main Crawl Loop

```
while frontier is not empty AND not terminated:
    1. Scheduler selects next URL respecting:
       - max-concurrent-pages-open global limit
       - minimum-time-on-page per-domain delay
       - max-domain-requests per-domain limit
       - robots.txt Crawl-delay
    
    2. For selected URL:
       a. Check robots.txt (fetch and cache if needed)
       b. If disallowed by robots.txt → mark as Failed (robots denied)
       
       c. Send HEAD request
       d. If not HTML Content-Type → mark as ContentMismatch, continue
       
       e. Send GET request (follows redirects)
       f. Handle response based on status code
       
       g. If successful:
          - Parse HTML for title and links
          - Normalize all discovered URLs
          - Classify each URL's domain
          - For each discovered URL:
            • If blacklisted: record in blacklist table
            • If stubbed: record in stub table
            • If new and within depth: add to frontier
            • Record the link relationship
          - Mark page as Processed
       
       h. Persist state to database
    
    3. Handle any 429 responses:
       - Mark domain as rate-limited
       - Mark all pending URLs for that domain as RateLimited
```

### 6.3 Link Extraction

Extract links from HTML that represent navigation to other HTML pages:

**Include:**
- `<a href="...">` tags in document body
- `<a href="...">` tags in `<nav>` elements
- `<a href="...">` tags in `<header>` and `<footer>`
- `<link rel="canonical" href="...">`

**Exclude:**
- `<link rel="stylesheet" href="...">`
- `<script src="...">`
- `<img src="...">`
- `<a href="...">` with `download` attribute
- `<a href="javascript:...">` 
- `<a href="mailto:...">`
- `<a href="tel:...">`
- Data URIs

**Note:** `rel="nofollow"` links ARE followed (per requirements).

### 6.4 Redirect Handling

```rust
struct RedirectPolicy {
    max_redirects: u32,  // Default: 10
    visited_in_chain: HashSet<String>,
}

fn handle_redirect(from: &str, to: &str, policy: &mut RedirectPolicy) -> RedirectAction {
    let normalized_to = normalize_url(to);
    
    // Check for loop
    if policy.visited_in_chain.contains(&normalized_to) {
        return RedirectAction::Abort(RedirectError::Loop);
    }
    
    // Check chain length
    if policy.visited_in_chain.len() >= policy.max_redirects as usize {
        return RedirectAction::Abort(RedirectError::ChainTooLong);
    }
    
    // Check destination domain
    let domain = extract_domain(&normalized_to);
    match classify_domain(&domain) {
        DomainClassification::Blacklisted => {
            record_blacklisted(&normalized_to, from);
            RedirectAction::Abort(RedirectError::HitBlacklist)
        }
        DomainClassification::Stubbed => {
            record_stubbed(&normalized_to, from);
            RedirectAction::Abort(RedirectError::HitStub)
        }
        _ => {
            policy.visited_in_chain.insert(normalized_to.clone());
            RedirectAction::Follow(normalized_to)
        }
    }
}
```

---

## 7. Rate Limiting & Politeness

### 7.1 Rate Limit Hierarchy

For each domain, the effective delay between requests is:

```rust
fn effective_delay(domain: &str, config: &Config, robots: &RobotsCache) -> Duration {
    let config_delay = config.minimum_time_on_page;
    let robots_delay = robots.get_crawl_delay(domain)
        .map(|d| Duration::from_secs_f64(d))
        .unwrap_or(Duration::ZERO);
    
    std::cmp::max(config_delay, robots_delay)
}
```

### 7.2 Domain Request Tracking

```rust
struct DomainState {
    request_count: u32,
    last_request_time: Option<Instant>,
    rate_limited: bool,
    robots_txt: Option<CachedRobots>,
    robots_fetched_at: Option<DateTime<Utc>>,
}

impl DomainState {
    fn can_request(&self, config: &Config, now: Instant) -> bool {
        if self.rate_limited {
            return false;
        }
        if self.request_count >= config.max_domain_requests {
            return false;
        }
        if let Some(last) = self.last_request_time {
            let delay = self.effective_delay(config);
            if now.duration_since(last) < delay {
                return false;
            }
        }
        true
    }
}
```

### 7.3 Robots.txt Handling

```rust
struct CachedRobots {
    content: ParsedRobots,
    fetched_at: DateTime<Utc>,
}

impl CachedRobots {
    fn is_stale(&self) -> bool {
        let age = Utc::now() - self.fetched_at;
        age > chrono::Duration::days(1)
    }
}

fn check_robots(url: &str, domain_state: &mut DomainState) -> RobotsResult {
    // Refresh if stale or missing
    if domain_state.robots_txt.is_none() || 
       domain_state.robots_txt.as_ref().unwrap().is_stale() {
        let robots_url = format!("https://{}/robots.txt", domain);
        match fetch_robots(&robots_url) {
            Ok(content) => {
                domain_state.robots_txt = Some(CachedRobots {
                    content: parse_robots(&content),
                    fetched_at: Utc::now(),
                });
            }
            Err(_) => {
                // No robots.txt = everything allowed
                domain_state.robots_txt = Some(CachedRobots {
                    content: ParsedRobots::allow_all(),
                    fetched_at: Utc::now(),
                });
            }
        }
    }
    
    let robots = domain_state.robots_txt.as_ref().unwrap();
    if robots.content.is_allowed(url, &config.user_agent.crawler_name) {
        RobotsResult::Allowed
    } else {
        RobotsResult::Disallowed
    }
}
```

### 7.4 Global Concurrency

The scheduler maintains a semaphore for `max-concurrent-pages-open`:

```rust
struct Scheduler {
    global_semaphore: Arc<Semaphore>,  // max-concurrent-pages-open permits
    domain_states: HashMap<String, DomainState>,
    frontier: PriorityQueue<QueuedUrl>,
}

impl Scheduler {
    async fn next_url(&mut self) -> Option<ScheduledFetch> {
        // Wait for a global permit
        let permit = self.global_semaphore.acquire().await.ok()?;
        
        // Find a domain that can accept a request
        let now = Instant::now();
        for url in self.frontier.iter_by_priority() {
            let domain = extract_domain(&url.url);
            if let Some(state) = self.domain_states.get(&domain) {
                if state.can_request(&self.config, now) {
                    let url = self.frontier.remove(&url.url);
                    return Some(ScheduledFetch { url, permit });
                }
            }
        }
        
        // No domain ready; release permit and wait
        drop(permit);
        None
    }
}
```

---

## 8. Persistence & Resumability

### 8.1 SQLite Schema

```sql
-- =============================================================================
-- Run Tracking
-- =============================================================================

CREATE TABLE runs (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at      TEXT NOT NULL,      -- ISO 8601 timestamp
    finished_at     TEXT,               -- NULL if still running
    config_hash     TEXT NOT NULL,      -- SHA256 of config for integrity
    status          TEXT NOT NULL       -- 'running', 'completed', 'interrupted'
);

CREATE INDEX idx_runs_status ON runs(status);

-- =============================================================================
-- Pages (Visited URLs)
-- =============================================================================

CREATE TABLE pages (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    url             TEXT NOT NULL UNIQUE,
    domain          TEXT NOT NULL,
    state           TEXT NOT NULL,      -- PageState enum value
    title           TEXT,
    status_code     INTEGER,
    content_type    TEXT,
    last_modified   TEXT,               -- From HTTP header, if present
    visited_at      TEXT,               -- When we fetched it
    discovered_at   TEXT NOT NULL,      -- When we first saw the URL
    discovered_run  INTEGER NOT NULL REFERENCES runs(id),
    error_message   TEXT,               -- If state is an error state
    retry_count     INTEGER DEFAULT 0
);

CREATE INDEX idx_pages_domain ON pages(domain);
CREATE INDEX idx_pages_state ON pages(state);
CREATE INDEX idx_pages_discovered_run ON pages(discovered_run);

-- =============================================================================
-- Depth Tracking (Multi-Origin)
-- =============================================================================

CREATE TABLE page_depths (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    page_id         INTEGER NOT NULL REFERENCES pages(id),
    quality_origin  TEXT NOT NULL,      -- The quality domain this is relative to
    depth           INTEGER NOT NULL,
    UNIQUE(page_id, quality_origin)
);

CREATE INDEX idx_page_depths_origin ON page_depths(quality_origin);
CREATE INDEX idx_page_depths_depth ON page_depths(depth);

-- =============================================================================
-- Links (Edges in the Graph)
-- =============================================================================

CREATE TABLE links (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    from_page_id    INTEGER NOT NULL REFERENCES pages(id),
    to_page_id      INTEGER NOT NULL REFERENCES pages(id),
    discovered_run  INTEGER NOT NULL REFERENCES runs(id),
    UNIQUE(from_page_id, to_page_id)
);

CREATE INDEX idx_links_from ON links(from_page_id);
CREATE INDEX idx_links_to ON links(to_page_id);

-- =============================================================================
-- Blacklisted URLs (Not Visited)
-- =============================================================================

CREATE TABLE blacklisted_urls (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    url             TEXT NOT NULL UNIQUE,
    domain          TEXT NOT NULL,
    reference_count INTEGER DEFAULT 1,
    first_seen_run  INTEGER NOT NULL REFERENCES runs(id)
);

CREATE INDEX idx_blacklisted_domain ON blacklisted_urls(domain);
CREATE INDEX idx_blacklisted_refs ON blacklisted_urls(reference_count DESC);

CREATE TABLE blacklisted_referrers (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    blacklisted_url_id  INTEGER NOT NULL REFERENCES blacklisted_urls(id),
    referrer_page_id    INTEGER NOT NULL REFERENCES pages(id),
    discovered_run      INTEGER NOT NULL REFERENCES runs(id),
    UNIQUE(blacklisted_url_id, referrer_page_id)
);

-- =============================================================================
-- Stubbed URLs (Not Visited)
-- =============================================================================

CREATE TABLE stubbed_urls (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    url             TEXT NOT NULL UNIQUE,
    domain          TEXT NOT NULL,
    reference_count INTEGER DEFAULT 1,
    first_seen_run  INTEGER NOT NULL REFERENCES runs(id)
);

CREATE INDEX idx_stubbed_domain ON stubbed_urls(domain);
CREATE INDEX idx_stubbed_refs ON stubbed_urls(reference_count DESC);

CREATE TABLE stubbed_referrers (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    stubbed_url_id  INTEGER NOT NULL REFERENCES stubbed_urls(id),
    referrer_page_id INTEGER NOT NULL REFERENCES pages(id),
    discovered_run  INTEGER NOT NULL REFERENCES runs(id),
    UNIQUE(stubbed_url_id, referrer_page_id)
);

-- =============================================================================
-- Domain State (For Resumability)
-- =============================================================================

CREATE TABLE domain_states (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    domain              TEXT NOT NULL UNIQUE,
    request_count       INTEGER DEFAULT 0,
    rate_limited        INTEGER DEFAULT 0,  -- Boolean
    robots_txt          TEXT,               -- Cached content
    robots_fetched_at   TEXT
);

-- =============================================================================
-- Crawl Queue (Frontier Persistence)
-- =============================================================================

CREATE TABLE frontier (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    page_id     INTEGER NOT NULL REFERENCES pages(id),
    priority    INTEGER NOT NULL,           -- Lower = higher priority
    added_at    TEXT NOT NULL
);

CREATE INDEX idx_frontier_priority ON frontier(priority, added_at);
```

### 8.2 Persistence Strategy

**Write-Ahead Logging:** Enable SQLite WAL mode for crash safety and concurrent reads.

```rust
fn init_database(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;
    ")?;
    Ok(conn)
}
```

**Transactional Updates:** Group related operations:

```rust
fn record_page_processed(
    conn: &Connection,
    page: &ProcessedPage,
    links: &[DiscoveredLink],
) -> Result<()> {
    let tx = conn.transaction()?;
    
    // Update page record
    tx.execute(
        "UPDATE pages SET state = ?, title = ?, status_code = ?, 
         content_type = ?, last_modified = ?, visited_at = ? WHERE id = ?",
        params![...]
    )?;
    
    // Insert discovered links
    for link in links {
        // Insert or get target page
        // Insert link relationship
        // Add to frontier if appropriate
    }
    
    tx.commit()
}
```

### 8.3 Resume Logic

```rust
fn resume_or_start(config: &Config, conn: &Connection) -> Result<CrawlState> {
    // Check for interrupted run
    let interrupted: Option<i64> = conn.query_row(
        "SELECT id FROM runs WHERE status = 'running' ORDER BY started_at DESC LIMIT 1",
        [],
        |row| row.get(0)
    ).optional()?;
    
    if let Some(run_id) = interrupted {
        // Load frontier
        let frontier = load_frontier(conn, run_id)?;
        // Load domain states
        let domain_states = load_domain_states(conn)?;
        
        Ok(CrawlState::Resume { run_id, frontier, domain_states })
    } else {
        // Start new run
        let run_id = conn.execute(
            "INSERT INTO runs (started_at, config_hash, status) VALUES (?, ?, 'running')",
            params![Utc::now().to_rfc3339(), hash_config(config)]
        )?;
        
        // Seed the frontier with quality domain seeds
        seed_frontier(conn, config, run_id)?;
        
        Ok(CrawlState::Fresh { run_id })
    }
}
```

---

## 9. Output

### 9.1 Output Trait

```rust
pub trait OutputHandler: Send + Sync {
    /// Record a successfully processed page
    fn record_page(&self, page: &ProcessedPage) -> Result<()>;
    
    /// Record a link between two pages
    fn record_link(&self, from: &str, to: &str) -> Result<()>;
    
    /// Record a blacklisted URL reference
    fn record_blacklisted(&self, url: &str, referrer: &str) -> Result<()>;
    
    /// Record a stubbed URL reference
    fn record_stubbed(&self, url: &str, referrer: &str) -> Result<()>;
    
    /// Record an error for a URL
    fn record_error(&self, url: &str, error: &CrawlError) -> Result<()>;
    
    /// Generate the final summary
    fn generate_summary(&self) -> Result<CrawlSummary>;
    
    /// Finalize and close (called at end of run)
    fn finalize(&self, status: RunStatus) -> Result<()>;
}
```

### 9.2 SQLite Output Handler

The primary storage implementation using the schema above.

### 9.3 Markdown Summary Format

```markdown
# Sumi-Ripple Crawl Summary

**Run ID:** 42
**Started:** 2025-01-15T10:30:00Z
**Finished:** 2025-01-15T14:45:23Z
**Duration:** 4h 15m 23s
**Status:** Completed

---

## Overall Statistics

| Metric | Count |
|--------|-------|
| Pages Crawled | 12,847 |
| Unique Domains Discovered | 234 |
| Total Links Recorded | 87,432 |
| Blacklisted URLs Found | 1,293 |
| Stubbed URLs Found | 4,521 |
| Errors Encountered | 156 |

---

## Depth Breakdown

| Depth | Pages | New Domains |
|-------|-------|-------------|
| 0 (Quality) | 2,341 | 5 |
| 1 | 4,892 | 89 |
| 2 | 3,764 | 98 |
| 3 | 1,850 | 42 |

---

## Discovered Domains

### Quality Domains (Fully Crawled)
- example.com (1,245 pages)
- blog.another-site.org (567 pages)
- docs.another-site.org (529 pages)

### Other Domains
- interesting-site.net (423 pages, depth 1-3)
- related-topic.org (312 pages, depth 1-3)
- ... (231 more domains)

---

## Top Blacklisted URLs

| URL | Reference Count |
|-----|-----------------|
| https://ads.example.net/tracker.js | 892 |
| https://tracking-service.com/pixel | 445 |
| ... | ... |

*(Showing top 20 of 1,293)*

---

## Top Stubbed URLs

| URL | Reference Count |
|-----|-----------------|
| https://github.com/rust-lang/rust | 234 |
| https://en.wikipedia.org/wiki/Web_crawler | 189 |
| ... | ... |

*(Showing top 20 of 4,521)*

---

## Error Summary

| Error Type | Count |
|------------|-------|
| Dead Links (404) | 89 |
| Rate Limited (429) | 12 |
| Connection Refused | 23 |
| Timeout | 18 |
| Robots Denied | 8 |
| Other | 6 |

### Rate-Limited Domains
- slow-server.com (request #45)
- overloaded-api.net (request #12)

---

*Generated by Sumi-Ripple v1.0*
```

---

## 10. HTTP Client Configuration

### 10.1 Client Setup

```rust
fn build_http_client(config: &Config) -> Result<Client> {
    let user_agent = format!(
        "{}/{} (+{}; {})",
        config.user_agent.crawler_name,
        config.user_agent.crawler_version,
        config.user_agent.contact_url,
        config.user_agent.contact_email
    );
    
    Client::builder()
        .user_agent(user_agent)
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .redirect(Policy::none())  // Handle redirects manually
        .https_only(true)
        .build()
}
```

### 10.2 Request Flow

```
1. HEAD request to check Content-Type
   ├─ Not HTML → Skip, mark ContentMismatch
   ├─ Error → Handle per retry logic
   └─ HTML → Continue

2. GET request for content
   ├─ Follow redirects manually (max 10)
   │   ├─ Loop detected → Abort, mark Failed
   │   ├─ Chain too long → Abort, mark Failed
   │   ├─ Hits blacklist → Stop, record blacklist
   │   └─ Hits stub → Stop, record stub
   ├─ Success → Parse HTML
   └─ Error → Handle per retry logic
```

---

## 11. Module Structure

```
sumi-ripple/
├── Cargo.toml
├── src/
│   ├── main.rs                 # Entry point, CLI handling
│   ├── lib.rs                  # Library root, public API
│   │
│   ├── config/
│   │   ├── mod.rs
│   │   ├── parser.rs           # TOML parsing
│   │   ├── validation.rs       # Config validation
│   │   └── types.rs            # Config structs
│   │
│   ├── url/
│   │   ├── mod.rs
│   │   ├── normalize.rs        # URL normalization
│   │   ├── domain.rs           # Domain extraction
│   │   └── matcher.rs          # Wildcard pattern matching
│   │
│   ├── state/
│   │   ├── mod.rs
│   │   ├── page_state.rs       # PageState enum and transitions
│   │   └── domain_state.rs     # Per-domain tracking
│   │
│   ├── robots/
│   │   ├── mod.rs
│   │   ├── parser.rs           # robots.txt parsing
│   │   └── cache.rs            # Caching logic
│   │
│   ├── crawler/
│   │   ├── mod.rs
│   │   ├── scheduler.rs        # Priority queue, domain scheduling
│   │   ├── fetcher.rs          # HTTP client, retries
│   │   ├── parser.rs           # HTML parsing, link extraction
│   │   └── coordinator.rs      # Main crawl loop orchestration
│   │
│   ├── storage/
│   │   ├── mod.rs
│   │   ├── traits.rs           # Storage trait definitions
│   │   ├── sqlite.rs           # SQLite implementation
│   │   └── schema.rs           # Schema definitions, migrations
│   │
│   └── output/
│       ├── mod.rs
│       ├── traits.rs           # OutputHandler trait
│       ├── sqlite_output.rs    # SQLite output implementation
│       └── markdown.rs         # Markdown summary generator
│
├── tests/
│   ├── integration/
│   │   ├── crawl_tests.rs
│   │   └── resume_tests.rs
│   └── unit/
│       ├── url_tests.rs
│       ├── config_tests.rs
│       └── state_tests.rs
│
└── examples/
    └── sample_config.toml
```

---

## 12. Recommended Crates

| Purpose | Crate | Notes |
|---------|-------|-------|
| HTTP Client | `reqwest` | Async, robust, widely used |
| Async Runtime | `tokio` | Industry standard |
| HTML Parsing | `scraper` | CSS selector based, built on `html5ever` |
| URL Parsing | `url` | Standard URL handling |
| TOML Parsing | `toml` | Official TOML parser |
| SQLite | `rusqlite` | Mature, feature-complete |
| Robots.txt | `robotstxt` | Google's parser, Rust bindings |
| Date/Time | `chrono` | Comprehensive datetime handling |
| Logging | `tracing` | Structured logging, async-aware |
| CLI | `clap` | Argument parsing |
| Error Handling | `thiserror` / `anyhow` | Ergonomic errors |
| Serialization | `serde` | With `serde_json` for any JSON needs |

---

## 13. CLI Interface

```
sumi-ripple 1.0
A polite web terrain mapper

USAGE:
    sumi-ripple [OPTIONS] <CONFIG>

ARGS:
    <CONFIG>    Path to TOML configuration file

OPTIONS:
    -h, --help              Print help information
    -V, --version           Print version information
    -v, --verbose           Increase logging verbosity (can be repeated)
    -q, --quiet             Suppress non-error output
    --resume                Resume an interrupted crawl (default behavior)
    --fresh                 Start a fresh crawl, ignoring any previous state
    --dry-run               Validate config and show what would be crawled
    --stats                 Show statistics from the database and exit
    --export-summary        Generate markdown summary from existing data and exit
```

---

## 14. Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum SumiError {
    // Configuration errors
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),
    
    // Network errors
    #[error("HTTP error for {url}: {source}")]
    Http { url: String, source: reqwest::Error },
    
    #[error("Request timeout for {url}")]
    Timeout { url: String },
    
    #[error("Too many redirects from {url}")]
    RedirectLimit { url: String },
    
    #[error("Redirect loop detected at {url}")]
    RedirectLoop { url: String },
    
    // Storage errors
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    
    // Parsing errors
    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),
    
    #[error("HTML parse error for {url}: {message}")]
    HtmlParse { url: String, message: String },
    
    // Robots errors
    #[error("URL disallowed by robots.txt: {url}")]
    RobotsDenied { url: String },
    
    // State errors
    #[error("Invalid state transition: {from:?} -> {to:?}")]
    InvalidTransition { from: PageState, to: PageState },
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Failed to parse TOML: {0}")]
    Parse(#[from] toml::de::Error),
    
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("Invalid URL in config: {0}")]
    InvalidUrl(String),
    
    #[error("Invalid domain pattern: {0}")]
    InvalidPattern(String),
}
```

---

## 15. Testing Strategy

### 15.1 Unit Tests

- URL normalization (extensive edge cases)
- Wildcard pattern matching
- State machine transitions
- Configuration parsing and validation
- robots.txt parsing

### 15.2 Integration Tests

- Full crawl of mock server
- Resume after simulated crash
- Rate limiting behavior
- Redirect chain handling
- Blacklist/stub classification

### 15.3 Test Infrastructure

```rust
// Use wiremock for HTTP mocking
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path};

async fn setup_mock_server() -> MockServer {
    let server = MockServer::start().await;
    
    // Setup mock pages
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200)
            .set_body_string(r#"
                <html>
                <head><title>Home</title></head>
                <body><a href="/about">About</a></body>
                </html>
            "#))
        .mount(&server)
        .await;
    
    server
}
```

---

## 16. Future Considerations

These features are out of scope for v1.0 but should be kept in mind for the architecture:

1. **JavaScript rendering** - Could add optional headless browser support
2. **Distributed crawling** - Architecture supports splitting work across machines
3. **Content storage** - Database schema could add content column
4. **Incremental updates** - Detect changed pages between runs
5. **Export formats** - GraphML, GEXF for graph visualization tools
6. **Webhooks/notifications** - Alert on crawl completion or errors
7. **Web UI** - Dashboard for monitoring active crawls

---

## Appendix A: Example Session

```bash
# First run
$ sumi-ripple config.toml -v
[INFO] Starting Sumi-Ripple v1.0
[INFO] Loaded configuration with 3 quality domains, 5 blacklisted, 2 stubbed
[INFO] Creating new database at ./sumi-ripple.db
[INFO] Starting run #1
[INFO] Seeding frontier with 4 URLs
[INFO] Crawling https://example.com/...
[INFO] Discovered 23 links on https://example.com/
...
[INFO] Run #1 completed successfully
[INFO] Summary written to ./crawl-summary.md

# Resume after interrupt
$ sumi-ripple config.toml -v
[INFO] Starting Sumi-Ripple v1.0
[INFO] Found interrupted run #2, resuming...
[INFO] Loaded 1,234 URLs from frontier
[INFO] Continuing crawl...

# Generate summary from existing data
$ sumi-ripple config.toml --export-summary
[INFO] Generated summary at ./crawl-summary.md
```

---

*End of Specification*
