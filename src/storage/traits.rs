//! Storage traits and error types
//!
//! This module defines the trait interface for storage backends and
//! associated error types.

use crate::state::{DomainState, PageState};
use crate::storage::{DepthRecord, LinkRecord, PageRecord, RunRecord, RunStatus};
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during storage operations
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Page not found: {0}")]
    PageNotFound(String),

    #[error("Run not found: {0}")]
    RunNotFound(i64),

    #[error("Invalid state transition: {from:?} -> {to:?}")]
    InvalidTransition { from: PageState, to: PageState },

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Constraint violation: {0}")]
    ConstraintViolation(String),
}

/// Result type for storage operations
pub type StorageResult<T> = Result<T, StorageError>;

/// Trait for storage backend implementations
///
/// This trait defines all database operations needed by the crawler.
/// Implementations should provide thread-safe access to the underlying storage.
pub trait Storage {
    // ===== Run Management =====

    /// Creates a new crawl run
    ///
    /// # Arguments
    ///
    /// * `config_hash` - Hash of the configuration file
    ///
    /// # Returns
    ///
    /// The ID of the newly created run
    fn create_run(&mut self, config_hash: &str) -> StorageResult<i64>;

    /// Gets a run by ID
    fn get_run(&self, run_id: i64) -> StorageResult<RunRecord>;

    /// Gets the most recent run
    fn get_latest_run(&self) -> StorageResult<Option<RunRecord>>;

    /// Updates the status of a run
    fn update_run_status(&mut self, run_id: i64, status: RunStatus) -> StorageResult<()>;

    /// Marks a run as completed with a finish timestamp
    fn complete_run(&mut self, run_id: i64) -> StorageResult<()>;

    // ===== Page Management =====

    /// Inserts a new page or gets the existing page ID
    ///
    /// # Arguments
    ///
    /// * `url` - The normalized URL
    /// * `domain` - The domain extracted from the URL
    /// * `discovered_run` - The run ID that discovered this page
    ///
    /// # Returns
    ///
    /// The page ID (either newly created or existing)
    fn insert_or_get_page(
        &mut self,
        url: &str,
        domain: &str,
        discovered_run: i64,
    ) -> StorageResult<i64>;

    /// Gets a page by ID
    fn get_page(&self, page_id: i64) -> StorageResult<PageRecord>;

    /// Gets a page by URL
    fn get_page_by_url(&self, url: &str) -> StorageResult<Option<PageRecord>>;

    /// Updates the state of a page
    fn update_page_state(
        &mut self,
        page_id: i64,
        state: PageState,
        title: Option<&str>,
        status_code: Option<u16>,
        content_type: Option<&str>,
        error_message: Option<&str>,
    ) -> StorageResult<()>;

    /// Increments the retry count for a page
    fn increment_retry_count(&mut self, page_id: i64) -> StorageResult<()>;

    /// Gets all pages in a specific state
    fn get_pages_by_state(&self, state: PageState) -> StorageResult<Vec<PageRecord>>;

    /// Gets pages that were being fetched (for crash recovery)
    fn get_interrupted_pages(&self) -> StorageResult<Vec<PageRecord>>;

    // ===== Depth Tracking =====

    /// Inserts or updates a depth record for a page
    ///
    /// If a depth record already exists for this page and origin,
    /// keeps the minimum depth value.
    ///
    /// # Arguments
    ///
    /// * `page_id` - The page ID
    /// * `quality_origin` - The quality domain this depth is relative to
    /// * `depth` - The depth value
    fn upsert_depth(&mut self, page_id: i64, quality_origin: &str, depth: u32)
        -> StorageResult<()>;

    /// Gets all depth records for a page
    fn get_depths(&self, page_id: i64) -> StorageResult<Vec<DepthRecord>>;

    /// Checks if a page should be crawled based on depth limits
    ///
    /// Returns true if ANY depth record for this page is within max_depth
    fn should_crawl(&self, page_id: i64, max_depth: u32) -> StorageResult<bool>;

    // ===== Link Management =====

    /// Inserts a link between two pages
    ///
    /// # Arguments
    ///
    /// * `from_page_id` - The source page ID
    /// * `to_page_id` - The destination page ID
    /// * `run_id` - The run ID that discovered this link
    fn insert_link(&mut self, from_page_id: i64, to_page_id: i64, run_id: i64)
        -> StorageResult<()>;

    /// Gets all outgoing links from a page
    fn get_outgoing_links(&self, page_id: i64) -> StorageResult<Vec<LinkRecord>>;

    /// Gets all incoming links to a page
    fn get_incoming_links(&self, page_id: i64) -> StorageResult<Vec<LinkRecord>>;

    /// Counts the total number of links
    fn count_links(&self) -> StorageResult<u64>;

    // ===== Frontier Management =====

    /// Adds a page to the crawl frontier
    ///
    /// # Arguments
    ///
    /// * `page_id` - The page ID to add
    /// * `priority` - Priority value (lower is higher priority)
    fn add_to_frontier(&mut self, page_id: i64, priority: u32) -> StorageResult<()>;

    /// Removes and returns the highest priority page from the frontier
    fn pop_from_frontier(&mut self) -> StorageResult<Option<i64>>;

    /// Loads the entire frontier into memory
    ///
    /// This is used for scheduler initialization
    fn load_frontier(&self) -> StorageResult<Vec<(i64, u32)>>;

    /// Clears the frontier
    fn clear_frontier(&mut self) -> StorageResult<()>;

    // ===== Domain State Persistence =====

    /// Loads all domain states from the database
    fn load_domain_states(&self) -> StorageResult<HashMap<String, DomainState>>;

    /// Saves domain states to the database
    fn save_domain_states(&mut self, states: &HashMap<String, DomainState>) -> StorageResult<()>;

    /// Updates a single domain state
    fn update_domain_state(&mut self, domain: &str, state: &DomainState) -> StorageResult<()>;

    // ===== Blacklist/Stub Tracking =====

    /// Records a blacklisted URL with its referrer
    fn record_blacklisted(&mut self, url: &str, referrer: &str, run_id: i64) -> StorageResult<()>;

    /// Records a stubbed URL with its referrer
    fn record_stubbed(&mut self, url: &str, referrer: &str, run_id: i64) -> StorageResult<()>;

    /// Gets all blacklisted URLs with reference counts
    fn get_blacklisted_urls(&self) -> StorageResult<Vec<(String, u32)>>;

    /// Gets all stubbed URLs with reference counts
    fn get_stubbed_urls(&self) -> StorageResult<Vec<(String, u32)>>;

    // ===== Statistics =====

    /// Counts pages by state
    fn count_pages_by_state(&self, state: PageState) -> StorageResult<u64>;

    /// Gets total page count
    fn count_total_pages(&self) -> StorageResult<u64>;

    /// Gets count of unique domains discovered
    fn count_unique_domains(&self) -> StorageResult<u64>;

    /// Gets error summary (state -> count)
    fn get_error_summary(&self) -> StorageResult<HashMap<PageState, u64>>;

    /// Gets domains that hit the request limit
    fn get_rate_limited_domains(&self) -> StorageResult<Vec<String>>;

    /// Gets page count breakdown by depth
    ///
    /// Returns a map of depth -> number of pages at that depth
    fn get_depth_breakdown(&self) -> StorageResult<HashMap<u32, usize>>;

    /// Gets list of all discovered domains
    ///
    /// Returns a sorted list of unique domains found during the crawl
    fn get_discovered_domains(&self) -> StorageResult<Vec<String>>;
}
