//! URL handling module for Sumi-Ripple
//!
//! This module provides URL normalization, domain extraction, wildcard matching,
//! and domain classification functionality.

mod domain;
mod matcher;
mod normalize;

use crate::config::Config;

// Re-export main functions
pub use domain::extract_domain;
pub use matcher::matches_wildcard;
pub use normalize::normalize_url;

/// Domain classification types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DomainClassification {
    /// Quality domain - should be fully crawled
    Quality,
    /// Blacklisted domain - record but skip
    Blacklisted,
    /// Stubbed domain - note but never visit
    Stubbed,
    /// Discovered domain - found during crawl
    Discovered,
}

impl DomainClassification {
    /// Returns true if the domain should be crawled
    pub fn should_crawl(&self) -> bool {
        matches!(self, Self::Quality | Self::Discovered)
    }

    /// Returns true if the domain is terminal (should not be visited)
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Blacklisted | Self::Stubbed)
    }
}

/// Classifies a domain according to the configuration
///
/// This function checks the domain against the configuration's domain lists
/// in the following priority order:
/// 1. Blacklist (highest priority)
/// 2. Stub list
/// 3. Quality list
/// 4. Discovered (default)
///
/// # Arguments
///
/// * `domain` - The domain string to classify (should be lowercase)
/// * `config` - The crawler configuration
///
/// # Returns
///
/// The classification of the domain
///
/// # Examples
///
/// ```no_run
/// use sumi_ripple::config::Config;
/// use sumi_ripple::url::{classify_domain, DomainClassification};
///
/// # fn example(config: &Config) {
/// let classification = classify_domain("example.com", config);
/// match classification {
///     DomainClassification::Quality => println!("Will crawl fully"),
///     DomainClassification::Blacklisted => println!("Will skip"),
///     DomainClassification::Stubbed => println!("Will note but not visit"),
///     DomainClassification::Discovered => println!("New domain found"),
/// }
/// # }
/// ```
pub fn classify_domain(domain: &str, config: &Config) -> DomainClassification {
    // Priority 1: Check blacklist
    for entry in &config.blacklist {
        if matches_wildcard(&entry.domain, domain) {
            return DomainClassification::Blacklisted;
        }
    }

    // Priority 2: Check stub list
    for entry in &config.stub {
        if matches_wildcard(&entry.domain, domain) {
            return DomainClassification::Stubbed;
        }
    }

    // Priority 3: Check quality list
    for entry in &config.quality {
        if matches_wildcard(&entry.domain, domain) {
            return DomainClassification::Quality;
        }
    }

    // Default: Discovered
    DomainClassification::Discovered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CrawlerConfig, DomainEntry, OutputConfig, QualityEntry, UserAgentConfig};

    fn create_test_config() -> Config {
        Config {
            crawler: CrawlerConfig {
                max_depth: 3,
                max_concurrent_pages_open: 10,
                minimum_time_on_page: 1000,
                max_domain_requests: 500,
            },
            user_agent: UserAgentConfig {
                crawler_name: "TestCrawler".to_string(),
                crawler_version: "1.0".to_string(),
                contact_url: "https://example.com/about".to_string(),
                contact_email: "admin@example.com".to_string(),
            },
            output: OutputConfig {
                database_path: "./test.db".to_string(),
                summary_path: "./summary.md".to_string(),
            },
            quality: vec![QualityEntry {
                domain: "quality.com".to_string(),
                seeds: vec!["https://quality.com/".to_string()],
            }],
            blacklist: vec![DomainEntry {
                domain: "bad.com".to_string(),
            }],
            stub: vec![DomainEntry {
                domain: "stub.com".to_string(),
            }],
        }
    }

    #[test]
    fn test_classify_quality_domain() {
        let config = create_test_config();
        assert_eq!(
            classify_domain("quality.com", &config),
            DomainClassification::Quality
        );
    }

    #[test]
    fn test_classify_blacklisted_domain() {
        let config = create_test_config();
        assert_eq!(
            classify_domain("bad.com", &config),
            DomainClassification::Blacklisted
        );
    }

    #[test]
    fn test_classify_stubbed_domain() {
        let config = create_test_config();
        assert_eq!(
            classify_domain("stub.com", &config),
            DomainClassification::Stubbed
        );
    }

    #[test]
    fn test_classify_discovered_domain() {
        let config = create_test_config();
        assert_eq!(
            classify_domain("random.com", &config),
            DomainClassification::Discovered
        );
    }

    #[test]
    fn test_priority_blacklist_over_stub() {
        let mut config = create_test_config();
        config.blacklist.push(DomainEntry {
            domain: "conflict.com".to_string(),
        });
        config.stub.push(DomainEntry {
            domain: "conflict.com".to_string(),
        });

        assert_eq!(
            classify_domain("conflict.com", &config),
            DomainClassification::Blacklisted
        );
    }

    #[test]
    fn test_priority_blacklist_over_quality() {
        let mut config = create_test_config();
        config.blacklist.push(DomainEntry {
            domain: "conflict.com".to_string(),
        });
        config.quality.push(QualityEntry {
            domain: "conflict.com".to_string(),
            seeds: vec!["https://conflict.com/".to_string()],
        });

        assert_eq!(
            classify_domain("conflict.com", &config),
            DomainClassification::Blacklisted
        );
    }

    #[test]
    fn test_priority_stub_over_quality() {
        let mut config = create_test_config();
        config.stub.push(DomainEntry {
            domain: "conflict.com".to_string(),
        });
        config.quality.push(QualityEntry {
            domain: "conflict.com".to_string(),
            seeds: vec!["https://conflict.com/".to_string()],
        });

        assert_eq!(
            classify_domain("conflict.com", &config),
            DomainClassification::Stubbed
        );
    }

    #[test]
    fn test_wildcard_classification() {
        let mut config = create_test_config();
        config.blacklist.push(DomainEntry {
            domain: "*.bad.com".to_string(),
        });

        assert_eq!(
            classify_domain("bad.com", &config),
            DomainClassification::Blacklisted
        );
        assert_eq!(
            classify_domain("sub.bad.com", &config),
            DomainClassification::Blacklisted
        );
        assert_eq!(
            classify_domain("deep.sub.bad.com", &config),
            DomainClassification::Blacklisted
        );
    }

    #[test]
    fn test_should_crawl() {
        assert!(DomainClassification::Quality.should_crawl());
        assert!(DomainClassification::Discovered.should_crawl());
        assert!(!DomainClassification::Blacklisted.should_crawl());
        assert!(!DomainClassification::Stubbed.should_crawl());
    }

    #[test]
    fn test_is_terminal() {
        assert!(!DomainClassification::Quality.is_terminal());
        assert!(!DomainClassification::Discovered.is_terminal());
        assert!(DomainClassification::Blacklisted.is_terminal());
        assert!(DomainClassification::Stubbed.is_terminal());
    }
}
