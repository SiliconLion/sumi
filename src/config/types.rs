use serde::Deserialize;

/// Main configuration structure for Sumi-Ripple
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub crawler: CrawlerConfig,
    #[serde(rename = "user-agent")]
    pub user_agent: UserAgentConfig,
    pub output: OutputConfig,
    #[serde(default)]
    pub quality: Vec<QualityEntry>,
    #[serde(default)]
    pub blacklist: Vec<DomainEntry>,
    #[serde(default)]
    pub stub: Vec<DomainEntry>,
}

/// Crawler behavior configuration
#[derive(Debug, Clone, Deserialize)]
pub struct CrawlerConfig {
    /// Maximum depth to crawl from seed URLs
    #[serde(rename = "max-depth")]
    pub max_depth: u32,

    /// Maximum number of concurrent page fetches
    #[serde(rename = "max-concurrent-pages-open")]
    pub max_concurrent_pages_open: u32,

    /// Minimum time between requests to the same domain (milliseconds)
    #[serde(rename = "minimum-time-on-page")]
    pub minimum_time_on_page: u64,

    /// Maximum number of requests per domain
    #[serde(rename = "max-domain-requests")]
    pub max_domain_requests: u32,
}

/// User agent identification configuration
#[derive(Debug, Clone, Deserialize)]
pub struct UserAgentConfig {
    /// Name of the crawler
    #[serde(rename = "crawler-name")]
    pub crawler_name: String,

    /// Version of the crawler
    #[serde(rename = "crawler-version")]
    pub crawler_version: String,

    /// URL with information about the crawler
    #[serde(rename = "contact-url")]
    pub contact_url: String,

    /// Email address for crawler-related contact
    #[serde(rename = "contact-email")]
    pub contact_email: String,
}

/// Output configuration
#[derive(Debug, Clone, Deserialize)]
pub struct OutputConfig {
    /// Path to the SQLite database file
    #[serde(rename = "database-path")]
    pub database_path: String,

    /// Path to the markdown summary file
    #[serde(rename = "summary-path")]
    pub summary_path: String,
}

/// Quality domain entry with seed URLs
#[derive(Debug, Clone, Deserialize)]
pub struct QualityEntry {
    /// Domain pattern (e.g., "example.com" or "*.example.com")
    pub domain: String,

    /// List of seed URLs to start crawling from
    pub seeds: Vec<String>,
}

/// Simple domain entry for blacklist and stub lists
#[derive(Debug, Clone, Deserialize)]
pub struct DomainEntry {
    /// Domain pattern (e.g., "example.com" or "*.example.com")
    pub domain: String,
}
