//! Output module for generating crawl summaries and reports
//!
//! This module handles:
//! - Generating markdown summaries of crawl results
//! - Exporting data in various formats
//! - Recording crawl statistics and metrics

mod markdown;
mod sqlite_output;
mod traits;

pub use markdown::generate_markdown_summary;
pub use sqlite_output::SqliteOutputHandler;
pub use traits::{CrawlSummary, OutputHandler};

use crate::storage::Storage;
use crate::SumiError;

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
    // TODO: Implement summary generation
    Ok(CrawlSummary::default())
}
