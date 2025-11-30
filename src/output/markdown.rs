//! Markdown summary generation
//!
//! This module generates human-readable markdown summaries of crawl results,
//! including statistics, error reports, and discovered domains.

use crate::output::traits::{CrawlSummary, OutputResult};
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Generates a markdown summary from crawl statistics
///
/// # Arguments
///
/// * `summary` - The crawl summary data
/// * `output_path` - Path where the markdown file should be written
///
/// # Returns
///
/// * `Ok(())` - Successfully wrote markdown summary
/// * `Err(OutputError)` - Failed to write summary
pub fn generate_markdown_summary(summary: &CrawlSummary, output_path: &Path) -> OutputResult<()> {
    let markdown = format_markdown_summary(summary);

    let mut file = File::create(output_path)?;
    file.write_all(markdown.as_bytes())?;

    Ok(())
}

/// Formats a crawl summary as markdown
///
/// # Arguments
///
/// * `summary` - The crawl summary data
///
/// # Returns
///
/// A formatted markdown string
pub fn format_markdown_summary(summary: &CrawlSummary) -> String {
    let mut md = String::new();

    // Title
    md.push_str("# Sumi-Ripple Crawl Summary\n\n");

    // Run metadata
    md.push_str("## Run Information\n\n");
    md.push_str(&format!("- **Run ID**: {}\n", summary.run_id));
    md.push_str(&format!("- **Started**: {}\n", summary.started_at));
    if let Some(finished) = &summary.finished_at {
        md.push_str(&format!("- **Finished**: {}\n", finished));
    }
    if let Some(duration) = summary.duration_seconds {
        md.push_str(&format!(
            "- **Duration**: {} seconds ({:.2} minutes)\n",
            duration,
            duration as f64 / 60.0
        ));
    }
    md.push_str(&format!("- **Status**: {}\n", summary.status));
    md.push_str(&format!("- **Config Hash**: {}\n\n", summary.config_hash));

    // Overall statistics
    md.push_str("## Overall Statistics\n\n");
    md.push_str(&format!("- **Total Pages**: {}\n", summary.total_pages));
    md.push_str(&format!(
        "- **Unique Domains**: {}\n",
        summary.unique_domains
    ));
    md.push_str(&format!("- **Total Links**: {}\n", summary.total_links));
    md.push_str(&format!("- **Total Errors**: {}\n", summary.total_errors));
    md.push_str(&format!(
        "- **Success Rate**: {:.2}%\n",
        summary.success_rate()
    ));
    md.push_str(&format!(
        "- **Error Rate**: {:.2}%\n\n",
        summary.error_rate()
    ));

    // State breakdown
    md.push_str("## Page State Breakdown\n\n");
    md.push_str("| State | Count |\n");
    md.push_str("|-------|-------|\n");
    md.push_str(&format!("| Processed | {} |\n", summary.pages_processed));
    md.push_str(&format!("| Discovered | {} |\n", summary.pages_discovered));
    md.push_str(&format!("| Queued | {} |\n", summary.pages_queued));
    md.push_str(&format!(
        "| Blacklisted | {} |\n",
        summary.pages_blacklisted
    ));
    md.push_str(&format!("| Stubbed | {} |\n", summary.pages_stubbed));
    md.push_str(&format!(
        "| Dead Link (404) | {} |\n",
        summary.pages_dead_link
    ));
    md.push_str(&format!(
        "| Unreachable | {} |\n",
        summary.pages_unreachable
    ));
    md.push_str(&format!(
        "| Rate Limited | {} |\n",
        summary.pages_rate_limited
    ));
    md.push_str(&format!("| Failed | {} |\n", summary.pages_failed));
    md.push_str(&format!(
        "| Depth Exceeded | {} |\n",
        summary.pages_depth_exceeded
    ));
    md.push_str(&format!(
        "| Request Limit Hit | {} |\n",
        summary.pages_request_limit_hit
    ));
    md.push_str(&format!(
        "| Content Mismatch | {} |\n\n",
        summary.pages_content_mismatch
    ));

    // Depth breakdown
    if !summary.depth_breakdown.is_empty() {
        md.push_str("## Depth Breakdown\n\n");
        md.push_str("| Depth | Pages |\n");
        md.push_str("|-------|-------|\n");

        let mut depths: Vec<_> = summary.depth_breakdown.iter().collect();
        depths.sort_by_key(|(d, _)| *d);

        for (depth, count) in depths {
            md.push_str(&format!("| {} | {} |\n", depth, count));
        }
        md.push_str("\n");
    }

    // Quality domains
    if !summary.quality_domains.is_empty() {
        md.push_str("## Quality Domains Crawled\n\n");
        for domain in &summary.quality_domains {
            md.push_str(&format!("- {}\n", domain));
        }
        md.push_str("\n");
    }

    // Discovered domains
    if !summary.discovered_domains.is_empty() {
        md.push_str("## Discovered Domains\n\n");
        md.push_str(&format!(
            "Total discovered: {}\n\n",
            summary.discovered_domains.len()
        ));
        for domain in summary.discovered_domains.iter().take(50) {
            md.push_str(&format!("- {}\n", domain));
        }
        if summary.discovered_domains.len() > 50 {
            md.push_str(&format!(
                "\n... and {} more\n\n",
                summary.discovered_domains.len() - 50
            ));
        } else {
            md.push_str("\n");
        }
    }

    // Top blacklisted URLs
    if !summary.top_blacklisted.is_empty() {
        md.push_str("## Top 20 Blacklisted URLs\n\n");
        md.push_str("| URL | References |\n");
        md.push_str("|-----|------------|\n");

        for (url, count) in summary.top_blacklisted.iter().take(20) {
            md.push_str(&format!("| {} | {} |\n", url, count));
        }
        md.push_str("\n");
    }

    // Top stubbed URLs
    if !summary.top_stubbed.is_empty() {
        md.push_str("## Top 20 Stubbed URLs\n\n");
        md.push_str("| URL | References |\n");
        md.push_str("|-----|------------|\n");

        for (url, count) in summary.top_stubbed.iter().take(20) {
            md.push_str(&format!("| {} | {} |\n", url, count));
        }
        md.push_str("\n");
    }

    // Error summary
    if !summary.error_summary.is_empty() {
        md.push_str("## Error Summary\n\n");
        md.push_str("| Error Type | Count |\n");
        md.push_str("|------------|-------|\n");

        for (state, count) in &summary.error_summary {
            md.push_str(&format!("| {:?} | {} |\n", state, count));
        }
        md.push_str("\n");
    }

    // Rate-limited domains
    if !summary.rate_limited_domains.is_empty() {
        md.push_str("## Rate-Limited Domains\n\n");
        md.push_str(&format!(
            "Total: {}\n\n",
            summary.rate_limited_domains.len()
        ));
        for domain in &summary.rate_limited_domains {
            md.push_str(&format!("- {}\n", domain));
        }
        md.push_str("\n");
    }

    md
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_summary() -> CrawlSummary {
        let mut summary = CrawlSummary::new();
        summary.run_id = 1;
        summary.started_at = "2024-01-01T00:00:00Z".to_string();
        summary.finished_at = Some("2024-01-01T01:00:00Z".to_string());
        summary.duration_seconds = Some(3600);
        summary.status = "completed".to_string();
        summary.config_hash = "abc123".to_string();
        summary.total_pages = 1000;
        summary.unique_domains = 50;
        summary.total_links = 5000;
        summary.pages_processed = 900;
        summary.pages_failed = 100;
        summary.total_errors = 100;
        summary
    }

    #[test]
    fn test_format_markdown_summary() {
        let summary = create_test_summary();
        let markdown = format_markdown_summary(&summary);

        assert!(markdown.contains("# Sumi-Ripple Crawl Summary"));
        assert!(markdown.contains("Run ID"));
        assert!(markdown.contains("Overall Statistics"));
        assert!(markdown.contains("Total Pages"));
    }

    #[test]
    fn test_markdown_contains_statistics() {
        let summary = create_test_summary();
        let markdown = format_markdown_summary(&summary);

        assert!(markdown.contains("1000")); // Total pages
        assert!(markdown.contains("50")); // Unique domains
        assert!(markdown.contains("5000")); // Total links
    }

    #[test]
    fn test_markdown_with_depth_breakdown() {
        let mut summary = create_test_summary();
        summary.depth_breakdown.insert(0, 100);
        summary.depth_breakdown.insert(1, 200);
        summary.depth_breakdown.insert(2, 300);

        let markdown = format_markdown_summary(&summary);

        assert!(markdown.contains("Depth Breakdown"));
        assert!(markdown.contains("| 0 | 100 |"));
        assert!(markdown.contains("| 1 | 200 |"));
        assert!(markdown.contains("| 2 | 300 |"));
    }

    #[test]
    fn test_markdown_with_discovered_domains() {
        let mut summary = create_test_summary();
        summary.discovered_domains = vec!["example.com".to_string(), "test.org".to_string()];

        let markdown = format_markdown_summary(&summary);

        assert!(markdown.contains("Discovered Domains"));
        assert!(markdown.contains("example.com"));
        assert!(markdown.contains("test.org"));
    }
}
