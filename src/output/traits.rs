//! Output handler traits and types
//!
//! This module defines the trait interface for output handlers and
//! associated data structures for crawl summaries.

use crate::state::PageState;
use crate::storage::RunStatus;
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during output operations
#[derive(Debug, Error)]
pub enum OutputError {
    #[error("Failed to write output: {0}")]
    Write(String),

    #[error("Failed to format output: {0}")]
    Format(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Storage error: {0}")]
    Storage(String),
}

/// Result type for output operations
pub type OutputResult<T> = Result<T, OutputError>;

/// Information about a processed page
#[derive(Debug, Clone)]
pub struct ProcessedPage {
    /// The page URL
    pub url: String,

    /// The page domain
    pub domain: String,

    /// Page title (if available)
    pub title: Option<String>,

    /// HTTP status code
    pub status_code: Option<u16>,

    /// Content type
    pub content_type: Option<String>,

    /// Final state of the page
    pub state: PageState,

    /// Depth from quality origins
    pub depths: Vec<(String, u32)>,
}

/// Error information for failed pages
#[derive(Debug, Clone)]
pub struct CrawlError {
    /// The URL that failed
    pub url: String,

    /// The error state
    pub state: PageState,

    /// Error message
    pub message: String,

    /// Number of retries attempted
    pub retry_count: u32,
}

/// Summary statistics for a crawl
#[derive(Debug, Clone, Default)]
pub struct CrawlSummary {
    // Run metadata
    pub run_id: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_seconds: Option<u64>,
    pub status: String,
    pub config_hash: String,

    // Overall statistics
    pub total_pages: u64,
    pub unique_domains: u64,
    pub total_links: u64,
    pub total_errors: u64,

    // State breakdown
    pub pages_discovered: u64,
    pub pages_queued: u64,
    pub pages_processed: u64,
    pub pages_blacklisted: u64,
    pub pages_stubbed: u64,
    pub pages_dead_link: u64,
    pub pages_unreachable: u64,
    pub pages_rate_limited: u64,
    pub pages_failed: u64,
    pub pages_depth_exceeded: u64,
    pub pages_request_limit_hit: u64,
    pub pages_content_mismatch: u64,

    // Depth breakdown (depth -> count)
    pub depth_breakdown: HashMap<u32, u64>,

    // Discovered domains list
    pub discovered_domains: Vec<String>,

    // Top blacklisted URLs with reference counts
    pub top_blacklisted: Vec<(String, u32)>,

    // Top stubbed URLs with reference counts
    pub top_stubbed: Vec<(String, u32)>,

    // Error summary (state -> count)
    pub error_summary: HashMap<PageState, u64>,

    // Rate-limited domains
    pub rate_limited_domains: Vec<String>,

    // Quality domains crawled
    pub quality_domains: Vec<String>,
}

impl CrawlSummary {
    /// Creates a new empty crawl summary
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the total number of pages in terminal states
    pub fn total_terminal_pages(&self) -> u64 {
        self.pages_processed
            + self.pages_blacklisted
            + self.pages_stubbed
            + self.pages_dead_link
            + self.pages_unreachable
            + self.pages_rate_limited
            + self.pages_failed
            + self.pages_depth_exceeded
            + self.pages_request_limit_hit
            + self.pages_content_mismatch
    }

    /// Returns the success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        let terminal = self.total_terminal_pages();
        if terminal == 0 {
            return 0.0;
        }
        (self.pages_processed as f64 / terminal as f64) * 100.0
    }

    /// Returns the error rate as a percentage
    pub fn error_rate(&self) -> f64 {
        let terminal = self.total_terminal_pages();
        if terminal == 0 {
            return 0.0;
        }
        (self.total_errors as f64 / terminal as f64) * 100.0
    }
}

/// Trait for output handlers
///
/// Output handlers are responsible for recording crawl events and
/// generating final summaries. Implementations must be thread-safe.
pub trait OutputHandler {
    /// Records a successfully processed page
    ///
    /// # Arguments
    ///
    /// * `page` - Information about the processed page
    fn record_page(&self, page: &ProcessedPage) -> OutputResult<()>;

    /// Records a link relationship between pages
    ///
    /// # Arguments
    ///
    /// * `from` - The source URL
    /// * `to` - The destination URL
    fn record_link(&self, from: &str, to: &str) -> OutputResult<()>;

    /// Records a blacklisted URL with its referrer
    ///
    /// # Arguments
    ///
    /// * `url` - The blacklisted URL
    /// * `referrer` - The page that linked to it
    fn record_blacklisted(&self, url: &str, referrer: &str) -> OutputResult<()>;

    /// Records a stubbed URL with its referrer
    ///
    /// # Arguments
    ///
    /// * `url` - The stubbed URL
    /// * `referrer` - The page that linked to it
    fn record_stubbed(&self, url: &str, referrer: &str) -> OutputResult<()>;

    /// Records an error that occurred during crawling
    ///
    /// # Arguments
    ///
    /// * `error` - Information about the error
    fn record_error(&self, error: &CrawlError) -> OutputResult<()>;

    /// Generates a summary of the crawl
    ///
    /// # Returns
    ///
    /// A CrawlSummary containing statistics and information about the crawl
    fn generate_summary(&self) -> OutputResult<CrawlSummary>;

    /// Finalizes the output, performing any cleanup or final writes
    ///
    /// # Arguments
    ///
    /// * `status` - The final status of the crawl run
    fn finalize(&self, status: RunStatus) -> OutputResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crawl_summary_new() {
        let summary = CrawlSummary::new();
        assert_eq!(summary.total_pages, 0);
        assert_eq!(summary.unique_domains, 0);
    }

    #[test]
    fn test_total_terminal_pages() {
        let mut summary = CrawlSummary::new();
        summary.pages_processed = 100;
        summary.pages_failed = 10;
        summary.pages_blacklisted = 5;

        assert_eq!(summary.total_terminal_pages(), 115);
    }

    #[test]
    fn test_success_rate() {
        let mut summary = CrawlSummary::new();
        summary.pages_processed = 80;
        summary.pages_failed = 20;

        let rate = summary.success_rate();
        assert!((rate - 80.0).abs() < 0.01);
    }

    #[test]
    fn test_success_rate_zero_pages() {
        let summary = CrawlSummary::new();
        assert_eq!(summary.success_rate(), 0.0);
    }

    #[test]
    fn test_error_rate() {
        let mut summary = CrawlSummary::new();
        summary.pages_processed = 90;
        summary.pages_failed = 5;
        summary.pages_dead_link = 3;
        summary.pages_unreachable = 2;
        summary.total_errors = 10;

        let rate = summary.error_rate();
        assert!((rate - 10.0).abs() < 0.01);
    }
}
