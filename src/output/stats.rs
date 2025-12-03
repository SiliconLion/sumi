//! Statistics generation from crawl database
//!
//! This module provides functionality for extracting and displaying
//! crawl statistics from the storage layer.

use crate::state::PageState;
use crate::storage::Storage;
use crate::SumiError;
use std::collections::HashMap;

/// Crawl statistics summary
#[derive(Debug, Clone)]
pub struct CrawlStatistics {
    /// Total number of pages discovered
    pub total_pages: u64,

    /// Count of pages by state
    pub pages_by_state: HashMap<PageState, u64>,

    /// Number of unique domains encountered
    pub unique_domains: u64,

    /// Total number of links discovered
    pub total_links: u64,

    /// Error summary (error states and their counts)
    pub error_summary: HashMap<PageState, u64>,

    /// Domains that were rate limited
    pub rate_limited_domains: Vec<String>,
}

/// Loads statistics from storage
///
/// # Arguments
///
/// * `storage` - The storage backend to query
///
/// # Returns
///
/// * `Ok(CrawlStatistics)` - Successfully loaded statistics
/// * `Err(SumiError)` - Failed to query statistics
pub fn load_statistics(storage: &dyn Storage) -> Result<CrawlStatistics, SumiError> {
    // Get total pages
    let total_pages = storage.count_total_pages()?;

    // Get unique domains
    let unique_domains = storage.count_unique_domains()?;

    // Get total links
    let total_links = storage.count_links()?;

    // Get error summary (includes all states with errors)
    let error_summary = storage.get_error_summary()?;

    // Calculate pages by state
    let mut pages_by_state = HashMap::new();

    // Count each state
    for state in [
        PageState::Discovered,
        PageState::Queued,
        PageState::Fetching,
        PageState::Processed,
        PageState::Blacklisted,
        PageState::Stubbed,
        PageState::DeadLink,
        PageState::Unreachable,
        PageState::RateLimited,
        PageState::Failed,
        PageState::DepthExceeded,
        PageState::RequestLimitHit,
        PageState::ContentMismatch,
    ] {
        let count = storage.count_pages_by_state(state)?;
        if count > 0 {
            pages_by_state.insert(state, count);
        }
    }

    // Get rate limited domains
    let rate_limited_domains = storage.get_rate_limited_domains()?;

    Ok(CrawlStatistics {
        total_pages,
        pages_by_state,
        unique_domains,
        total_links,
        error_summary,
        rate_limited_domains,
    })
}

/// Prints statistics to stdout in a formatted manner
///
/// # Arguments
///
/// * `stats` - The statistics to display
pub fn print_statistics(stats: &CrawlStatistics) {
    println!("=== Crawl Statistics ===\n");

    println!("Overview:");
    println!("  Total pages discovered: {}", stats.total_pages);
    println!("  Unique domains: {}", stats.unique_domains);
    println!("  Total links found: {}", stats.total_links);
    println!();

    println!("Pages by State:");
    // Sort states by count (descending)
    let mut state_counts: Vec<_> = stats.pages_by_state.iter().collect();
    state_counts.sort_by(|a, b| b.1.cmp(a.1));

    for (state, count) in state_counts {
        let percentage = if stats.total_pages > 0 {
            (*count as f64 / stats.total_pages as f64) * 100.0
        } else {
            0.0
        };
        println!("  {:?}: {} ({:.1}%)", state, count, percentage);
    }
    println!();

    if !stats.error_summary.is_empty() {
        println!("Error Summary:");
        let mut error_counts: Vec<_> = stats.error_summary.iter().collect();
        error_counts.sort_by(|a, b| b.1.cmp(a.1));

        for (state, count) in error_counts {
            println!("  {:?}: {}", state, count);
        }
        println!();
    }

    if !stats.rate_limited_domains.is_empty() {
        println!(
            "Rate Limited Domains ({}):",
            stats.rate_limited_domains.len()
        );
        for domain in &stats.rate_limited_domains {
            println!("  - {}", domain);
        }
        println!();
    }

    // Calculate success rate
    let processed = stats
        .pages_by_state
        .get(&PageState::Processed)
        .unwrap_or(&0);
    let success_rate = if stats.total_pages > 0 {
        (*processed as f64 / stats.total_pages as f64) * 100.0
    } else {
        0.0
    };

    println!(
        "Success Rate: {:.1}% ({} / {} pages successfully processed)",
        success_rate, processed, stats.total_pages
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crawl_statistics_creation() {
        let mut pages_by_state = HashMap::new();
        pages_by_state.insert(PageState::Processed, 100);
        pages_by_state.insert(PageState::Discovered, 50);

        let stats = CrawlStatistics {
            total_pages: 150,
            pages_by_state,
            unique_domains: 10,
            total_links: 500,
            error_summary: HashMap::new(),
            rate_limited_domains: vec![],
        };

        assert_eq!(stats.total_pages, 150);
        assert_eq!(stats.unique_domains, 10);
        assert_eq!(stats.total_links, 500);
    }
}
