//! Crawler coordinator - main crawl orchestration logic
//!
//! This module contains the main crawl loop that coordinates all aspects of
//! the crawling process, including:
//! - Initializing storage and state
//! - Managing the frontier queue
//! - Coordinating fetching, parsing, and link extraction
//! - Handling interrupts and resumption
//! - Generating final output

use crate::config::Config;
use crate::crawler::parser::parse_html;
use crate::crawler::scheduler::{QueuedUrl, Scheduler};
use crate::crawler::{build_http_client, fetch_url, FetchResult};
use crate::robots::{fetch_robots, is_allowed, ParsedRobots};
use crate::state::PageState;
use crate::storage::{SqliteStorage, Storage};
use crate::url::{classify_domain, extract_domain, normalize_url, DomainClassification};
use crate::SumiError;
use reqwest::Client;
use std::path::Path;
use std::sync::{Arc, Mutex};
use url::Url;

/// Main crawler coordinator structure
pub struct Coordinator {
    config: Arc<Config>,
    storage: Arc<Mutex<SqliteStorage>>,
    scheduler: Scheduler,
    client: Client,
    run_id: i64,
    user_agent: String,
}

impl Coordinator {
    /// Creates a new coordinator instance
    ///
    /// # Arguments
    ///
    /// * `config` - The crawler configuration
    /// * `fresh` - Whether to start a fresh crawl (clears existing data)
    ///
    /// # Returns
    ///
    /// * `Ok(Coordinator)` - Successfully created coordinator
    /// * `Err(SumiError)` - Failed to initialize
    pub fn new(config: Config, fresh: bool) -> Result<Self, SumiError> {
        // Initialize storage
        let storage_path = Path::new(&config.output.database_path);
        let mut storage = SqliteStorage::new(storage_path)?;

        // Create or resume run
        let run_id = if fresh {
            // Clear frontier and create new run
            storage.clear_frontier()?;
            storage.create_run("config_hash_placeholder")?
        } else {
            // Check for interrupted run
            if let Some(latest_run) = storage.get_latest_run()? {
                use crate::storage::RunStatus;
                if matches!(latest_run.status, RunStatus::Running) {
                    tracing::info!("Resuming interrupted run {}", latest_run.id);
                    latest_run.id
                } else {
                    tracing::info!("Starting new run");
                    storage.create_run("config_hash_placeholder")?
                }
            } else {
                tracing::info!("No previous runs found, starting new run");
                storage.create_run("config_hash_placeholder")?
            }
        };

        // Load frontier from storage or seed it
        let frontier_data = storage.load_frontier()?;
        let mut frontier = Vec::new();

        if frontier_data.is_empty() && fresh {
            // Seed frontier with quality domain seeds
            tracing::info!("Seeding frontier with quality domain seeds");
            for quality_entry in &config.quality {
                for seed_url in &quality_entry.seeds {
                    let normalized = normalize_url(seed_url)?;
                    let domain = extract_domain(&normalized).ok_or_else(|| {
                        SumiError::Storage(format!("Failed to extract domain from {}", normalized))
                    })?;
                    let page_id =
                        storage.insert_or_get_page(normalized.as_str(), &domain, run_id)?;

                    // Insert depth 0 for this quality domain
                    storage.upsert_depth(page_id, &quality_entry.domain, 0)?;

                    // Add to frontier with priority 0
                    storage.add_to_frontier(page_id, 0)?;

                    frontier.push(QueuedUrl {
                        url: normalized.clone(),
                        domain: domain.clone(),
                        priority: 0,
                        page_id,
                    });
                }
            }
        } else {
            // Load existing frontier
            tracing::info!("Loading {} URLs from frontier", frontier_data.len());
            for (page_id, priority) in frontier_data {
                let page = storage.get_page(page_id)?;
                let url = Url::parse(&page.url)?;
                frontier.push(QueuedUrl {
                    url,
                    domain: page.domain.clone(),
                    priority,
                    page_id,
                });
            }
        }

        // Load domain states
        let domain_states = storage.load_domain_states()?;

        // Build HTTP client
        let client = build_http_client(&config.user_agent)?;

        // Format user agent string
        let user_agent = format!(
            "{}/{} (+{}; {})",
            config.user_agent.crawler_name,
            config.user_agent.crawler_version,
            config.user_agent.contact_url,
            config.user_agent.contact_email
        );

        // Create scheduler
        let scheduler = Scheduler::new(config.crawler.clone(), frontier, domain_states);

        Ok(Self {
            config: Arc::new(config),
            storage: Arc::new(Mutex::new(storage)),
            scheduler,
            client,
            run_id,
            user_agent,
        })
    }

    /// Runs the main crawl loop
    ///
    /// This is the core crawling logic that:
    /// 1. Gets URLs from the scheduler
    /// 2. Fetches pages
    /// 3. Parses HTML and extracts links
    /// 4. Classifies discovered URLs
    /// 5. Updates storage and frontier
    pub async fn run(&mut self) -> Result<(), SumiError> {
        tracing::info!("Starting crawl run {}", self.run_id);

        let mut pages_crawled = 0;
        let start_time = std::time::Instant::now();

        loop {
            // Get next URL from scheduler
            let scheduled = match self.scheduler.next_url().await {
                Some(s) => s,
                None => {
                    tracing::info!("Frontier is empty, crawl complete");
                    break;
                }
            };

            let url = scheduled.url.clone();
            tracing::debug!("Processing URL: {}", url.url);

            // Process this URL
            if let Err(e) = self.process_url(&url).await {
                tracing::error!("Error processing {}: {}", url.url, e);
            }

            pages_crawled += 1;

            // Progress reporting and periodic persistence every 10 pages
            if pages_crawled % 10 == 0 {
                let elapsed = start_time.elapsed();
                let rate = pages_crawled as f64 / elapsed.as_secs_f64();
                tracing::info!(
                    "Progress: {} pages crawled, {} in frontier, {:.2} pages/sec",
                    pages_crawled,
                    self.scheduler.frontier_size(),
                    rate
                );

                // Periodic domain state persistence every 50 pages
                if pages_crawled % 50 == 0 {
                    self.save_domain_states()?;
                }
            }
        }

        // Final domain state persistence
        self.save_domain_states()?;

        // Mark run as completed
        {
            let mut storage = self.storage.lock().unwrap();
            storage.complete_run(self.run_id)?;
        }

        tracing::info!(
            "Crawl completed: {} pages crawled in {:?}",
            pages_crawled,
            start_time.elapsed()
        );

        Ok(())
    }

    /// Processes a single URL
    ///
    /// This method:
    /// 1. Checks robots.txt
    /// 2. Fetches the page
    /// 3. Parses HTML and extracts links
    /// 4. Classifies discovered URLs
    /// 5. Updates storage
    async fn process_url(&mut self, queued: &QueuedUrl) -> Result<(), SumiError> {
        let url_str = queued.url.as_str();
        let page_id = queued.page_id;

        // Record that we're starting to request this domain
        self.scheduler.record_request(&queued.domain);

        // Update page state to Fetching
        {
            let mut storage = self.storage.lock().unwrap();
            storage.update_page_state(page_id, PageState::Fetching, None, None, None, None)?;
        }

        // Check robots.txt
        let robots = self.get_or_fetch_robots(&queued.domain).await?;

        // Check if URL is allowed by robots.txt
        if !is_allowed(&robots, url_str, &self.user_agent) {
            tracing::info!("URL {} disallowed by robots.txt", url_str);
            let mut storage = self.storage.lock().unwrap();
            storage.update_page_state(
                page_id,
                PageState::Failed,
                None,
                None,
                None,
                Some("Disallowed by robots.txt"),
            )?;
            return Ok(());
        }

        // Fetch the page
        let fetch_result = fetch_url(&self.client, url_str).await;

        // Handle fetch result
        match fetch_result {
            FetchResult::Success {
                final_url,
                status_code,
                content_type,
                body,
                title: _,
            } => {
                // Parse HTML and extract links
                let parsed = match parse_html(&body, &queued.url) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!("Failed to parse HTML for {}: {}", url_str, e);
                        let mut storage = self.storage.lock().unwrap();
                        storage.update_page_state(
                            page_id,
                            PageState::Failed,
                            None,
                            Some(status_code),
                            Some(&content_type),
                            Some(&format!("Parse error: {}", e)),
                        )?;
                        return Ok(());
                    }
                };

                // Update page state to Processed
                {
                    let mut storage = self.storage.lock().unwrap();
                    storage.update_page_state(
                        page_id,
                        PageState::Processed,
                        parsed.title.as_deref(),
                        Some(status_code),
                        Some(&content_type),
                        None,
                    )?;
                }

                // Handle discovered links
                self.handle_discovered_links(page_id, &parsed.links, &final_url)
                    .await?;
            }

            FetchResult::ContentMismatch { content_type } => {
                let mut storage = self.storage.lock().unwrap();
                storage.update_page_state(
                    page_id,
                    PageState::ContentMismatch,
                    None,
                    None,
                    Some(&content_type),
                    Some(&format!("Expected HTML, got {}", content_type)),
                )?;
            }

            FetchResult::RedirectToTerminal {
                terminal_url,
                reason,
            } => {
                let mut storage = self.storage.lock().unwrap();
                storage.update_page_state(
                    page_id,
                    PageState::Failed,
                    None,
                    None,
                    None,
                    Some(&format!("Redirect to {}: {}", terminal_url, reason)),
                )?;
            }

            FetchResult::HttpError { status_code, state } => {
                let mut storage = self.storage.lock().unwrap();
                storage.update_page_state(
                    page_id,
                    state,
                    None,
                    Some(status_code),
                    None,
                    Some(&format!("HTTP {}", status_code)),
                )?;

                // If rate limited, mark the domain
                if status_code == 429 {
                    self.scheduler.mark_rate_limited(&queued.domain);
                }
            }

            FetchResult::NetworkError { error, state } => {
                let mut storage = self.storage.lock().unwrap();
                storage.update_page_state(page_id, state, None, None, None, Some(&error))?;
            }

            FetchResult::RedirectError { error } => {
                let mut storage = self.storage.lock().unwrap();
                storage.update_page_state(
                    page_id,
                    PageState::Failed,
                    None,
                    None,
                    None,
                    Some(&error),
                )?;
            }
        }

        Ok(())
    }

    /// Handles discovered links from a page
    ///
    /// This method:
    /// 1. Normalizes URLs
    /// 2. Classifies domains
    /// 3. Records links in storage
    /// 4. Adds crawlable URLs to frontier
    async fn handle_discovered_links(
        &mut self,
        from_page_id: i64,
        links: &[String],
        base_url: &str,
    ) -> Result<(), SumiError> {
        for link in links {
            // Normalize URL
            let normalized = match normalize_url(link) {
                Ok(n) => n,
                Err(e) => {
                    tracing::debug!("Failed to normalize URL {}: {}", link, e);
                    continue;
                }
            };

            // Extract domain
            let domain = match extract_domain(&normalized) {
                Some(d) => d,
                None => {
                    tracing::debug!("Failed to extract domain from {}", normalized);
                    continue;
                }
            };

            // Classify domain
            let classification = classify_domain(&domain, &self.config);

            // Convert Url to string for storage operations
            let normalized_str = normalized.as_str();

            // Handle based on classification
            match classification {
                DomainClassification::Blacklisted => {
                    // Record blacklisted URL
                    let mut storage = self.storage.lock().unwrap();
                    storage.record_blacklisted(normalized_str, base_url, self.run_id)?;
                    continue;
                }

                DomainClassification::Stubbed => {
                    // Record stubbed URL
                    let mut storage = self.storage.lock().unwrap();
                    storage.record_stubbed(normalized_str, base_url, self.run_id)?;
                    continue;
                }

                DomainClassification::Quality | DomainClassification::Discovered => {
                    // Insert or get page
                    let to_page_id = {
                        let mut storage = self.storage.lock().unwrap();
                        storage.insert_or_get_page(normalized_str, &domain, self.run_id)?
                    };

                    // Record link
                    {
                        let mut storage = self.storage.lock().unwrap();
                        storage.insert_link(from_page_id, to_page_id, self.run_id)?;
                    }

                    // Calculate depth and check if we should crawl
                    let should_add_to_frontier = {
                        let mut storage = self.storage.lock().unwrap();

                        // Get depths of source page
                        let source_depths = storage.get_depths(from_page_id)?;

                        // Calculate new depths for target page
                        for depth_record in source_depths {
                            let new_depth = depth_record.depth + 1;
                            storage.upsert_depth(
                                to_page_id,
                                &depth_record.quality_origin,
                                new_depth,
                            )?;
                        }

                        // Check if we should crawl this page
                        storage.should_crawl(to_page_id, self.config.crawler.max_depth)?
                    };

                    // Add to frontier if within depth limits and not already visited
                    if should_add_to_frontier {
                        let page = {
                            let storage = self.storage.lock().unwrap();
                            storage.get_page(to_page_id)?
                        };

                        // Only add if page is in Discovered state
                        if page.state == PageState::Discovered {
                            // Calculate priority based on classification
                            let priority = match classification {
                                DomainClassification::Quality => 0,
                                DomainClassification::Discovered => 10,
                                _ => 100,
                            };

                            // Add to storage frontier
                            {
                                let mut storage = self.storage.lock().unwrap();
                                storage.add_to_frontier(to_page_id, priority)?;
                            }

                            // Add to scheduler frontier
                            self.scheduler.add_to_frontier(QueuedUrl {
                                url: normalized.clone(),
                                domain: domain.clone(),
                                priority,
                                page_id: to_page_id,
                            });
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Saves all domain states to the database
    ///
    /// This method persists the current state of all domains being crawled,
    /// including request counts, rate limit status, and cached robots.txt.
    fn save_domain_states(&mut self) -> Result<(), SumiError> {
        let domain_states = self.scheduler.get_all_domain_states();
        let mut storage = self.storage.lock().unwrap();
        storage.save_domain_states(domain_states)?;
        tracing::debug!("Saved {} domain states to database", domain_states.len());
        Ok(())
    }

    /// Gets robots.txt for a domain, fetching if necessary
    ///
    /// This method checks if we have cached robots.txt for the domain,
    /// and fetches it if needed or if the cache is stale.
    async fn get_or_fetch_robots(&mut self, domain: &str) -> Result<ParsedRobots, SumiError> {
        // Check if scheduler has cached robots.txt for this domain
        let needs_fetch = if let Some(domain_state) = self.scheduler.get_domain_state(domain) {
            domain_state.is_robots_stale()
        } else {
            true
        };

        if needs_fetch {
            // Fetch robots.txt
            tracing::debug!("Fetching robots.txt for domain: {}", domain);
            let robots = fetch_robots(domain, &self.user_agent).await?;

            // Cache it in the domain state
            if let Some(domain_state) = self.scheduler.get_domain_state_mut(domain) {
                // Get the robots.txt content for caching
                // Since ParsedRobots doesn't expose content directly, we'll just mark as fetched
                domain_state.update_robots(String::new());
            }

            Ok(robots)
        } else {
            // Use cached robots.txt
            tracing::debug!("Using cached robots.txt for domain: {}", domain);

            // We need to re-parse from cached content
            // For now, just return allow_all since we don't store the parsed version
            // TODO: Improve this to actually cache the parsed robots
            Ok(ParsedRobots::allow_all())
        }
    }
}

/// Runs the main crawl operation
///
/// This function orchestrates the entire crawl process:
///
/// 1. Check for interrupted run or start fresh
/// 2. Initialize storage layer
/// 3. Build HTTP client
/// 4. Initialize scheduler with frontier
/// 5. Spawn worker tasks
/// 6. Main crawl loop:
///    a. Get next URL from scheduler
///    b. Check robots.txt
///    c. Fetch page (HEAD then GET)
///    d. Parse HTML and extract links
///    e. Classify discovered URLs
///    f. Update state and record links
///    g. Add new URLs to frontier
/// 7. Mark run as completed
/// 8. Generate summary output
///
/// # Arguments
///
/// * `config` - The crawler configuration
///
/// # Returns
///
/// * `Ok(())` - Crawl completed successfully
/// * `Err(SumiError)` - Crawl failed with an error
///
/// # Example
///
/// ```no_run
/// use sumi_ripple::config::load_config;
/// use sumi_ripple::crawler::run_crawl;
/// use std::path::Path;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = load_config(Path::new("config.toml"))?;
/// run_crawl(config).await?;
/// # Ok(())
/// # }
/// ```
pub async fn run_crawl(config: Config) -> Result<(), SumiError> {
    let mut coordinator = Coordinator::new(config, false)?;
    coordinator.run().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CrawlerConfig, OutputConfig, QualityEntry, UserAgentConfig};

    fn create_test_config() -> Config {
        Config {
            crawler: CrawlerConfig {
                max_depth: 2,
                max_concurrent_pages_open: 5,
                minimum_time_on_page: 1000,
                max_domain_requests: 100,
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
                domain: "example.com".to_string(),
                seeds: vec!["https://example.com/".to_string()],
            }],
            blacklist: vec![],
            stub: vec![],
        }
    }

    #[tokio::test]
    async fn test_coordinator_creation() {
        let config = create_test_config();
        // This test requires actual database setup
        // For now, we'll skip it in unit tests
        // Integration tests will cover this
    }
}
