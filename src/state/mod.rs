//! State module for tracking crawl progress
//!
//! This module provides state management for pages and domains during the crawl process.
//!
//! # Components
//!
//! - `PageState`: Tracks the state of individual pages (discovered, queued, fetching, processed, etc.)
//! - `DomainState`: Tracks per-domain state for rate limiting and request counting
//! - `CachedRobots`: Stores cached robots.txt data for domains

mod domain_state;
mod page_state;

// Re-export main types
pub use domain_state::{CachedRobots, DomainState};
pub use page_state::PageState;
