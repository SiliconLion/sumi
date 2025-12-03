//! Crawler module for web page fetching and processing
//!
//! This module contains the core crawling logic, including:
//! - HTTP fetching with retry logic
//! - HTML parsing and link extraction
//! - Request scheduling and rate limiting
//! - Overall crawl coordination

mod coordinator;
mod fetcher;
mod parser;
mod scheduler;

pub use coordinator::{run_crawl, Coordinator};
pub use fetcher::{build_http_client, fetch_url, FetchResult};
pub use parser::{extract_links_simple, parse_html};
pub use scheduler::Scheduler;

use crate::config::Config;
use crate::SumiError;

/// Runs a complete crawl operation
///
/// This is the main entry point for starting a crawl. It will:
/// 1. Initialize the storage layer
/// 2. Load or create a crawl run
/// 3. Build the HTTP client
/// 4. Schedule and fetch pages
/// 5. Extract and follow links
/// 6. Generate summary output
///
/// # Arguments
///
/// * `config` - The crawler configuration
///
/// # Returns
///
/// * `Ok(())` - Crawl completed successfully
/// * `Err(SumiError)` - Crawl failed
pub async fn crawl(config: Config) -> Result<(), SumiError> {
    run_crawl(config).await
}
