# Quick Start: Critical Fixes for Sumi-Ripple

## 🚨 BLOCKING ISSUE: Scheduler Exits Prematurely

**Problem:** The crawler processes only 1 page then exits with "Frontier is empty, crawl complete"

**Root Cause:** `scheduler.next_url()` returns `None` when domains are rate-limited instead of waiting

**File:** `src/crawler/scheduler.rs`, lines 103-124

---

## Quick Fix (30 minutes)

### Step 1: Add Helper Method

Add this method to `impl Scheduler` in `src/crawler/scheduler.rs`:

```rust
/// Calculates the minimum time to wait before any domain is ready
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
    
    // Add small buffer
    min_wait + Duration::from_millis(10)
}
```

### Step 2: Fix `next_url()` Method

Replace the current `next_url()` method (lines 103-124) with:

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
            let state = self.domain_states
                .entry(queued.domain.clone())
                .or_insert_with(DomainState::new);
            state.can_request(&self.config, now)
        }) {
            // Found a ready URL, remove and return it
            let url = self.frontier.remove(position);
            return Some(ScheduledFetch { url, _permit: permit });
        }
        
        // No domains ready, calculate minimum wait time
        let min_wait = self.calculate_minimum_wait_time(now);
        
        tracing::debug!("No domains ready, waiting {:?}", min_wait);
        
        // Sleep for the minimum time needed
        tokio::time::sleep(min_wait).await;
        
        // Check again if frontier is still not empty after sleep
        if self.frontier.is_empty() {
            return None;
        }
    }
}
```

---

## Verify the Fix

### Test 1: Integration Tests Should Pass

```bash
cargo test --test integration_tests
```

**Expected:** All 4 tests pass
- `test_full_crawl_single_domain`
- `test_robots_txt_respect`
- `test_crawl_with_depth_limit`
- `test_content_type_handling`

### Test 2: Real Crawl

```bash
# Dry run to verify config
cargo run --release -- examples/sample_config.toml --dry-run

# Run actual crawl
cargo run --release -- examples/sample_config.toml --fresh

# Check results
cargo run --release -- examples/sample_config.toml --stats
```

**Expected:** Should see multiple pages processed (not just 1)

### Test 3: Check Logs

```bash
RUST_LOG=debug cargo run -- examples/sample_config.toml --fresh
```

**Expected:** Should see:
- "Processing URL: ..." messages for multiple URLs
- "Progress: X pages crawled..." messages
- NO premature "Frontier is empty" after 1 page

---

## Success Criteria

✅ **All 4 integration tests pass**  
✅ **Crawl processes >1 page from seeds**  
✅ **Scheduler waits instead of returning None**  
✅ **Logs show "No domains ready, waiting" when appropriate**  
✅ **No compiler warnings from changes**

---

## After This Fix

Once the scheduler fix is working:

1. **Issue #2:** Fix robots.txt caching (3-4 hours)
   - See TODO.md line 294+

2. **Issue #3:** Implement priority queue (4-6 hours)
   - See TODO.md line 756+

3. **Issue #4:** Complete summary generation (3-4 hours)
   - See TODO.md line 922+

---

## Troubleshooting

### If tests still fail:

1. Check domain extraction:
   ```bash
   cargo test url::domain --lib -- --nocapture
   ```

2. Check rate limiting logic:
   ```bash
   cargo test domain_state --lib -- --nocapture
   ```

3. Check if frontier is being seeded:
   ```bash
   RUST_LOG=sumi_ripple=trace cargo test test_full_crawl_single_domain -- --nocapture
   ```

### If compilation fails:

1. Make sure `Instant` and `Duration` are imported:
   ```rust
   use std::time::{Duration, Instant};
   ```

2. Make sure `time_until_next_request` exists in `DomainState`
   - It should be in `src/state/domain_state.rs`
   - Already implemented in the codebase

---

## Quick Command Reference

```bash
# Run all unit tests
cargo test --lib

# Run specific integration test
cargo test test_full_crawl_single_domain

# Run with logging
RUST_LOG=debug cargo test test_full_crawl_single_domain -- --nocapture

# Check compilation
cargo check

# Format code
cargo fmt

# Run clippy
cargo clippy

# Full test suite
cargo test --all
```

---

## Need Help?

See detailed explanations in:
- **TODO.md** - Complete issue tracking and solutions
- **IMPLEMENTATION_SUMMARY.md** - Current state of implementation
- **sumi-ripple-spec.md** - Original specifications

**Estimated time to fix:** 30-60 minutes  
**Impact:** Unblocks all crawling functionality  
**Priority:** 🚨 CRITICAL - Do this first!