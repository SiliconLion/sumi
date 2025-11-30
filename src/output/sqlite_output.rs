//! SQLite-based output handler implementation
//!
//! This module provides an output handler that records crawl events
//! directly to the SQLite storage backend.

use crate::output::traits::{
    CrawlError, CrawlSummary, OutputError, OutputHandler, OutputResult, ProcessedPage,
};
use crate::state::PageState;
use crate::storage::{RunStatus, Storage};
use std::sync::{Arc, Mutex};

/// SQLite-based output handler
///
/// This handler records crawl events directly to the storage backend
/// and generates summaries from the database contents.
pub struct SqliteOutputHandler {
    storage: Arc<Mutex<dyn Storage>>,
    run_id: i64,
}

impl SqliteOutputHandler {
    /// Creates a new SQLite output handler
    ///
    /// # Arguments
    ///
    /// * `storage` - The storage backend to use
    /// * `run_id` - The current run ID
    ///
    /// # Returns
    ///
    /// A new SqliteOutputHandler instance
    pub fn new(storage: Arc<Mutex<dyn Storage>>, run_id: i64) -> Self {
        Self { storage, run_id }
    }
}

impl OutputHandler for SqliteOutputHandler {
    fn record_page(&self, page: &ProcessedPage) -> OutputResult<()> {
        let mut storage = self
            .storage
            .lock()
            .map_err(|e| OutputError::Storage(format!("Failed to lock storage: {}", e)))?;

        // Get or create the page
        let page_id = storage
            .insert_or_get_page(&page.url, &page.domain, self.run_id)
            .map_err(|e| OutputError::Storage(e.to_string()))?;

        // Update the page state
        storage
            .update_page_state(
                page_id,
                page.state,
                page.title.as_deref(),
                page.status_code,
                page.content_type.as_deref(),
                None,
            )
            .map_err(|e| OutputError::Storage(e.to_string()))?;

        // Record depths
        for (quality_origin, depth) in &page.depths {
            storage
                .upsert_depth(page_id, quality_origin, *depth)
                .map_err(|e| OutputError::Storage(e.to_string()))?;
        }

        Ok(())
    }

    fn record_link(&self, from: &str, to: &str) -> OutputResult<()> {
        let mut storage = self
            .storage
            .lock()
            .map_err(|e| OutputError::Storage(format!("Failed to lock storage: {}", e)))?;

        // Get page IDs
        let from_page = storage
            .get_page_by_url(from)
            .map_err(|e| OutputError::Storage(e.to_string()))?
            .ok_or_else(|| OutputError::Storage(format!("Source page not found: {}", from)))?;

        let to_page = storage
            .get_page_by_url(to)
            .map_err(|e| OutputError::Storage(e.to_string()))?
            .ok_or_else(|| OutputError::Storage(format!("Target page not found: {}", to)))?;

        // Insert the link
        storage
            .insert_link(from_page.id, to_page.id, self.run_id)
            .map_err(|e| OutputError::Storage(e.to_string()))?;

        Ok(())
    }

    fn record_blacklisted(&self, url: &str, referrer: &str) -> OutputResult<()> {
        let mut storage = self
            .storage
            .lock()
            .map_err(|e| OutputError::Storage(format!("Failed to lock storage: {}", e)))?;

        storage
            .record_blacklisted(url, referrer, self.run_id)
            .map_err(|e| OutputError::Storage(e.to_string()))?;

        Ok(())
    }

    fn record_stubbed(&self, url: &str, referrer: &str) -> OutputResult<()> {
        let mut storage = self
            .storage
            .lock()
            .map_err(|e| OutputError::Storage(format!("Failed to lock storage: {}", e)))?;

        storage
            .record_stubbed(url, referrer, self.run_id)
            .map_err(|e| OutputError::Storage(e.to_string()))?;

        Ok(())
    }

    fn record_error(&self, error: &CrawlError) -> OutputResult<()> {
        let mut storage = self
            .storage
            .lock()
            .map_err(|e| OutputError::Storage(format!("Failed to lock storage: {}", e)))?;

        // Get or create the page
        let page = storage
            .get_page_by_url(&error.url)
            .map_err(|e| OutputError::Storage(e.to_string()))?;

        if let Some(page_record) = page {
            // Update the page state with error information
            storage
                .update_page_state(
                    page_record.id,
                    error.state,
                    None,
                    None,
                    None,
                    Some(&error.message),
                )
                .map_err(|e| OutputError::Storage(e.to_string()))?;
        }

        Ok(())
    }

    fn generate_summary(&self) -> OutputResult<CrawlSummary> {
        let storage = self
            .storage
            .lock()
            .map_err(|e| OutputError::Storage(format!("Failed to lock storage: {}", e)))?;

        // Get run information
        let run = storage
            .get_run(self.run_id)
            .map_err(|e| OutputError::Storage(e.to_string()))?;

        // Collect statistics
        let mut summary = CrawlSummary::new();
        summary.run_id = run.id;
        summary.started_at = run.started_at;
        summary.finished_at = run.finished_at;
        summary.status = run.status.to_db_string().to_string();
        summary.config_hash = run.config_hash;

        // Total counts
        summary.total_pages = storage
            .count_total_pages()
            .map_err(|e| OutputError::Storage(e.to_string()))?;
        summary.unique_domains = storage
            .count_unique_domains()
            .map_err(|e| OutputError::Storage(e.to_string()))?;
        summary.total_links = storage
            .count_links()
            .map_err(|e| OutputError::Storage(e.to_string()))?;

        // State breakdown
        summary.pages_discovered = storage
            .count_pages_by_state(PageState::Discovered)
            .map_err(|e| OutputError::Storage(e.to_string()))?;
        summary.pages_queued = storage
            .count_pages_by_state(PageState::Queued)
            .map_err(|e| OutputError::Storage(e.to_string()))?;
        summary.pages_processed = storage
            .count_pages_by_state(PageState::Processed)
            .map_err(|e| OutputError::Storage(e.to_string()))?;
        summary.pages_blacklisted = storage
            .count_pages_by_state(PageState::Blacklisted)
            .map_err(|e| OutputError::Storage(e.to_string()))?;
        summary.pages_stubbed = storage
            .count_pages_by_state(PageState::Stubbed)
            .map_err(|e| OutputError::Storage(e.to_string()))?;
        summary.pages_dead_link = storage
            .count_pages_by_state(PageState::DeadLink)
            .map_err(|e| OutputError::Storage(e.to_string()))?;
        summary.pages_unreachable = storage
            .count_pages_by_state(PageState::Unreachable)
            .map_err(|e| OutputError::Storage(e.to_string()))?;
        summary.pages_rate_limited = storage
            .count_pages_by_state(PageState::RateLimited)
            .map_err(|e| OutputError::Storage(e.to_string()))?;
        summary.pages_failed = storage
            .count_pages_by_state(PageState::Failed)
            .map_err(|e| OutputError::Storage(e.to_string()))?;
        summary.pages_depth_exceeded = storage
            .count_pages_by_state(PageState::DepthExceeded)
            .map_err(|e| OutputError::Storage(e.to_string()))?;
        summary.pages_request_limit_hit = storage
            .count_pages_by_state(PageState::RequestLimitHit)
            .map_err(|e| OutputError::Storage(e.to_string()))?;
        summary.pages_content_mismatch = storage
            .count_pages_by_state(PageState::ContentMismatch)
            .map_err(|e| OutputError::Storage(e.to_string()))?;

        // Error summary
        summary.error_summary = storage
            .get_error_summary()
            .map_err(|e| OutputError::Storage(e.to_string()))?;
        summary.total_errors = summary.error_summary.values().sum();

        // Blacklisted and stubbed URLs
        summary.top_blacklisted = storage
            .get_blacklisted_urls()
            .map_err(|e| OutputError::Storage(e.to_string()))?;
        summary.top_stubbed = storage
            .get_stubbed_urls()
            .map_err(|e| OutputError::Storage(e.to_string()))?;

        // Rate-limited domains
        summary.rate_limited_domains = storage
            .get_rate_limited_domains()
            .map_err(|e| OutputError::Storage(e.to_string()))?;

        Ok(summary)
    }

    fn finalize(&self, status: RunStatus) -> OutputResult<()> {
        let mut storage = self
            .storage
            .lock()
            .map_err(|e| OutputError::Storage(format!("Failed to lock storage: {}", e)))?;

        if status == RunStatus::Completed {
            storage
                .complete_run(self.run_id)
                .map_err(|e| OutputError::Storage(e.to_string()))?;
        } else {
            storage
                .update_run_status(self.run_id, status)
                .map_err(|e| OutputError::Storage(e.to_string()))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::SqliteStorage;

    #[test]
    fn test_create_handler() {
        let storage = SqliteStorage::new_in_memory().unwrap();
        let mut storage_mut = storage;
        let run_id = storage_mut.create_run("test_hash").unwrap();

        let storage_arc: Arc<Mutex<dyn Storage>> = Arc::new(Mutex::new(storage_mut));
        let handler = SqliteOutputHandler::new(storage_arc, run_id);

        assert_eq!(handler.run_id, run_id);
    }

    #[test]
    fn test_record_page() {
        let storage = SqliteStorage::new_in_memory().unwrap();
        let mut storage_mut = storage;
        let run_id = storage_mut.create_run("test_hash").unwrap();

        let storage_arc: Arc<Mutex<dyn Storage>> = Arc::new(Mutex::new(storage_mut));
        let handler = SqliteOutputHandler::new(storage_arc, run_id);

        let page = ProcessedPage {
            url: "https://example.com/".to_string(),
            domain: "example.com".to_string(),
            title: Some("Example".to_string()),
            status_code: Some(200),
            content_type: Some("text/html".to_string()),
            state: PageState::Processed,
            depths: vec![("example.com".to_string(), 0)],
        };

        let result = handler.record_page(&page);
        assert!(result.is_ok());
    }

    #[test]
    fn test_finalize() {
        let storage = SqliteStorage::new_in_memory().unwrap();
        let mut storage_mut = storage;
        let run_id = storage_mut.create_run("test_hash").unwrap();

        let storage_arc: Arc<Mutex<dyn Storage>> = Arc::new(Mutex::new(storage_mut));
        let handler = SqliteOutputHandler::new(storage_arc, run_id);

        let result = handler.finalize(RunStatus::Completed);
        assert!(result.is_ok());
    }
}
