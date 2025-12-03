//! Output module for generating crawl summaries and reports
//!
//! This module handles:
//! - Generating markdown summaries of crawl results
//! - Exporting data in various formats
//! - Recording crawl statistics and metrics

mod markdown;
mod sqlite_output;
pub mod stats;
mod traits;

pub use markdown::generate_markdown_summary;
pub use sqlite_output::SqliteOutputHandler;
pub use stats::{load_statistics, print_statistics, CrawlStatistics};
pub use traits::{CrawlSummary, OutputHandler};

use crate::storage::Storage;
use crate::SumiError;
use std::collections::HashMap;

/// Generates a crawl summary from storage
///
/// # Arguments
///
/// * `storage` - The storage backend containing crawl data
///
/// # Returns
///
/// * `Ok(CrawlSummary)` - Successfully generated summary
/// * `Err(SumiError)` - Failed to generate summary
pub fn generate_summary(storage: &dyn Storage) -> Result<CrawlSummary, SumiError> {
    use crate::state::PageState;

    // Get the latest run
    let run = storage
        .get_latest_run()?
        .ok_or_else(|| SumiError::Storage("No crawl runs found in database".to_string()))?;

    // Calculate duration if finished
    let duration_seconds = if let (Ok(started), Some(finished_str)) = (
        run.started_at.parse::<chrono::DateTime<chrono::Utc>>(),
        &run.finished_at,
    ) {
        if let Ok(finished) = finished_str.parse::<chrono::DateTime<chrono::Utc>>() {
            Some((finished - started).num_seconds() as u64)
        } else {
            None
        }
    } else {
        None
    };

    // Load statistics
    let stats = stats::load_statistics(storage)?;

    // Get page counts by state
    let pages_discovered = stats
        .pages_by_state
        .get(&PageState::Discovered)
        .copied()
        .unwrap_or(0);
    let pages_queued = stats
        .pages_by_state
        .get(&PageState::Queued)
        .copied()
        .unwrap_or(0);
    let pages_processed = stats
        .pages_by_state
        .get(&PageState::Processed)
        .copied()
        .unwrap_or(0);
    let pages_blacklisted = stats
        .pages_by_state
        .get(&PageState::Blacklisted)
        .copied()
        .unwrap_or(0);
    let pages_stubbed = stats
        .pages_by_state
        .get(&PageState::Stubbed)
        .copied()
        .unwrap_or(0);
    let pages_dead_link = stats
        .pages_by_state
        .get(&PageState::DeadLink)
        .copied()
        .unwrap_or(0);
    let pages_unreachable = stats
        .pages_by_state
        .get(&PageState::Unreachable)
        .copied()
        .unwrap_or(0);
    let pages_rate_limited = stats
        .pages_by_state
        .get(&PageState::RateLimited)
        .copied()
        .unwrap_or(0);
    let pages_failed = stats
        .pages_by_state
        .get(&PageState::Failed)
        .copied()
        .unwrap_or(0);
    let pages_depth_exceeded = stats
        .pages_by_state
        .get(&PageState::DepthExceeded)
        .copied()
        .unwrap_or(0);
    let pages_request_limit_hit = stats
        .pages_by_state
        .get(&PageState::RequestLimitHit)
        .copied()
        .unwrap_or(0);
    let pages_content_mismatch = stats
        .pages_by_state
        .get(&PageState::ContentMismatch)
        .copied()
        .unwrap_or(0);

    // Get blacklisted and stubbed URLs
    let top_blacklisted = storage.get_blacklisted_urls()?;
    let top_stubbed = storage.get_stubbed_urls()?;

    Ok(CrawlSummary {
        run_id: run.id,
        started_at: run.started_at,
        finished_at: run.finished_at,
        duration_seconds,
        status: run.status.to_db_string().to_string(),
        config_hash: run.config_hash,
        total_pages: stats.total_pages,
        unique_domains: stats.unique_domains,
        total_links: stats.total_links,
        total_errors: stats.error_summary.values().sum(),
        pages_discovered,
        pages_queued,
        pages_processed,
        pages_blacklisted,
        pages_stubbed,
        pages_dead_link,
        pages_unreachable,
        pages_rate_limited,
        pages_failed,
        pages_depth_exceeded,
        pages_request_limit_hit,
        pages_content_mismatch,
        depth_breakdown: HashMap::new(), // TODO: Implement depth breakdown
        discovered_domains: vec![],      // TODO: Query discovered domains
        top_blacklisted,
        top_stubbed,
        error_summary: stats.error_summary.clone(),
        rate_limited_domains: stats.rate_limited_domains.clone(),
        quality_domains: vec![], // TODO: Extract from config or storage
    })
}
