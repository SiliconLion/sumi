//! Integration tests for the crawler
//!
//! These tests use wiremock to create mock HTTP servers and test
//! the full crawl cycle end-to-end.

use sumi_ripple::config::{Config, CrawlerConfig, OutputConfig, QualityEntry, UserAgentConfig};
use sumi_ripple::crawler::Coordinator;
use sumi_ripple::state::PageState;
use sumi_ripple::storage::{SqliteStorage, Storage};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Creates a test configuration with the given quality domain and seeds
fn create_test_config(quality_domain: &str, seeds: Vec<String>, db_path: &str) -> Config {
    Config {
        crawler: CrawlerConfig {
            max_depth: 2,
            max_concurrent_pages_open: 5,
            minimum_time_on_page: 10, // Very short for testing
            max_domain_requests: 100,
        },
        user_agent: UserAgentConfig {
            crawler_name: "TestBot".to_string(),
            crawler_version: "1.0.0".to_string(),
            contact_url: "https://example.com/contact".to_string(),
            contact_email: "test@example.com".to_string(),
        },
        output: OutputConfig {
            database_path: db_path.to_string(),
            summary_path: "./test_summary.md".to_string(),
        },
        quality: vec![QualityEntry {
            domain: quality_domain.to_string(),
            seeds,
        }],
        blacklist: vec![],
        stub: vec![],
    }
}

#[tokio::test]
async fn test_full_crawl_single_domain() {
    // Start a mock server
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    // Extract domain from base_url (e.g., "127.0.0.1:12345" from "http://127.0.0.1:12345")
    let domain = url::Url::parse(&base_url)
        .expect("Failed to parse base URL")
        .host_str()
        .expect("Failed to extract host")
        .to_string();

    // Mock robots.txt (GET only, no HEAD)
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string("User-agent: *\nAllow: /"))
        .mount(&mock_server)
        .await;

    // Mock HEAD requests for all pages except robots.txt
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-type", "text/html"))
        .mount(&mock_server)
        .await;

    // Mock index page with links
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!(
                    r#"<html><head><title>Home</title></head><body>
                    <a href="{}/page1">Page 1</a>
                    <a href="{}/page2">Page 2</a>
                    </body></html>"#,
                    base_url, base_url
                ))
                .insert_header("content-type", "text/html"),
        )
        .mount(&mock_server)
        .await;

    // Mock page1
    Mock::given(method("GET"))
        .and(path("/page1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"<html><head><title>Page 1</title></head><body>Content 1</body></html>"#,
                )
                .insert_header("content-type", "text/html"),
        )
        .mount(&mock_server)
        .await;

    // Mock page2
    Mock::given(method("GET"))
        .and(path("/page2"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"<html><head><title>Page 2</title></head><body>Content 2</body></html>"#,
                )
                .insert_header("content-type", "text/html"),
        )
        .mount(&mock_server)
        .await;

    // Create test database
    let db_path = format!("/tmp/test_full_crawl_{}.db", std::process::id());

    // Clean up any existing test database
    let _ = std::fs::remove_file(&db_path);

    // Create config with extracted domain
    let config = create_test_config(&domain, vec![format!("{}/", base_url)], &db_path);

    // Run the crawl
    let mut coordinator = Coordinator::new(config, true).expect("Failed to create coordinator");
    coordinator.run().await.expect("Crawl failed");

    // Verify results
    let storage = SqliteStorage::new(std::path::Path::new(&db_path)).expect("Failed to open DB");

    // Should have discovered 3 pages (/, /page1, /page2)
    let total_pages = storage.count_total_pages().expect("Failed to count pages");
    assert!(
        total_pages >= 3,
        "Expected at least 3 pages, got {}",
        total_pages
    );

    // Should have processed pages successfully
    let processed = storage
        .count_pages_by_state(PageState::Processed)
        .expect("Failed to count processed");
    assert!(
        processed >= 3,
        "Expected at least 3 processed pages, got {}",
        processed
    );

    // Clean up
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_robots_txt_respect() {
    // Start a mock server
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    // Extract domain from base_url
    let domain = url::Url::parse(&base_url)
        .expect("Failed to parse base URL")
        .host_str()
        .expect("Failed to extract host")
        .to_string();

    // Mock robots.txt that disallows /admin (GET only, no HEAD)
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string("User-agent: *\nDisallow: /admin"))
        .mount(&mock_server)
        .await;

    // Mock HEAD requests for pages except robots.txt
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-type", "text/html"))
        .mount(&mock_server)
        .await;

    // Mock index page with link to admin
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!(
                    r#"<html><head><title>Home</title></head><body>
                    <a href="{}/allowed">Allowed Page</a>
                    <a href="{}/admin">Admin Page</a>
                    </body></html>"#,
                    base_url, base_url
                ))
                .insert_header("content-type", "text/html"),
        )
        .mount(&mock_server)
        .await;

    // Mock allowed page
    Mock::given(method("GET"))
        .and(path("/allowed"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"<html><head><title>Allowed</title></head><body>Allowed content</body></html>"#,
                )
                .insert_header("content-type", "text/html"),
        )
        .mount(&mock_server)
        .await;

    // Mock admin page (should never be called)
    Mock::given(method("GET"))
        .and(path("/admin"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"<html><head><title>Admin</title></head><body>Admin content</body></html>"#,
                )
                .insert_header("content-type", "text/html"),
        )
        .expect(0) // Should never be called
        .mount(&mock_server)
        .await;

    // Create test database
    let db_path = format!("/tmp/test_robots_{}.db", std::process::id());
    let _ = std::fs::remove_file(&db_path);

    // Create config with extracted domain
    let config = create_test_config(&domain, vec![format!("{}/", base_url)], &db_path);

    // Run the crawl
    let mut coordinator = Coordinator::new(config, true).expect("Failed to create coordinator");
    coordinator.run().await.expect("Crawl failed");

    // Wiremock will automatically verify expectations when mock_server drops

    // Verify results
    let storage = SqliteStorage::new(std::path::Path::new(&db_path)).expect("Failed to open DB");

    // Should have processed / and /allowed
    let processed = storage
        .count_pages_by_state(PageState::Processed)
        .expect("Failed to count processed");
    assert!(processed >= 2, "Expected at least 2 processed pages");

    // /admin should be in Failed state (disallowed by robots.txt)
    let failed = storage
        .count_pages_by_state(PageState::Failed)
        .expect("Failed to count failed");
    assert!(failed >= 1, "Expected at least 1 failed page (admin)");

    // Clean up
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_crawl_with_depth_limit() {
    // Start a mock server
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    // Extract domain from base_url
    let domain = url::Url::parse(&base_url)
        .expect("Failed to parse base URL")
        .host_str()
        .expect("Failed to extract host")
        .to_string();

    // Mock robots.txt (GET only, no HEAD)
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string("User-agent: *\nAllow: /"))
        .mount(&mock_server)
        .await;

    // Mock HEAD requests for pages except robots.txt
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-type", "text/html"))
        .mount(&mock_server)
        .await;

    // Create a chain: / -> level1 -> level2 -> level3
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!(
                    r#"<html><head><title>Root</title></head><body>
                    <a href="{}/level1">Level 1</a>
                    </body></html>"#,
                    base_url
                ))
                .insert_header("content-type", "text/html"),
        )
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/level1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!(
                    r#"<html><head><title>Level 1</title></head><body>
                    <a href="{}/level2">Level 2</a>
                    </body></html>"#,
                    base_url
                ))
                .insert_header("content-type", "text/html"),
        )
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/level2"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!(
                    r#"<html><head><title>Level 2</title></head><body>
                    <a href="{}/level3">Level 3</a>
                    </body></html>"#,
                    base_url
                ))
                .insert_header("content-type", "text/html"),
        )
        .mount(&mock_server)
        .await;

    // Level3 should not be crawled (depth > 2)
    // Wiremock will automatically verify expect(0) when the mock server drops
    Mock::given(method("GET"))
        .and(path("/level3"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    r#"<html><head><title>Level 3</title></head><body>Level 3</body></html>"#,
                )
                .insert_header("content-type", "text/html"),
        )
        .expect(0) // Should never be called with max_depth=2
        .mount(&mock_server)
        .await;

    // Create test database
    let db_path = format!("/tmp/test_depth_{}.db", std::process::id());
    let _ = std::fs::remove_file(&db_path);

    // Create config with max_depth=2 and extracted domain
    let config = create_test_config(&domain, vec![format!("{}/", base_url)], &db_path);

    // Run the crawl
    let mut coordinator = Coordinator::new(config, true).expect("Failed to create coordinator");
    coordinator.run().await.expect("Crawl failed");

    // Verify results
    let storage = SqliteStorage::new(std::path::Path::new(&db_path)).expect("Failed to open DB");

    // Should have processed /, level1, level2 (depth 0, 1, 2)
    let processed = storage
        .count_pages_by_state(PageState::Processed)
        .expect("Failed to count processed");
    assert_eq!(processed, 3, "Expected exactly 3 processed pages");

    // level3 should be DepthExceeded
    let depth_exceeded = storage
        .count_pages_by_state(PageState::DepthExceeded)
        .expect("Failed to count depth exceeded");
    assert!(
        depth_exceeded >= 1,
        "Expected at least 1 depth exceeded page"
    );

    // Clean up
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_content_type_handling() {
    // Start a mock server
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    // Extract domain from base_url
    let domain = url::Url::parse(&base_url)
        .expect("Failed to parse base URL")
        .host_str()
        .expect("Failed to extract host")
        .to_string();

    // Mock robots.txt (GET only, no HEAD)
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string("User-agent: *\nAllow: /"))
        .mount(&mock_server)
        .await;

    // Mock HEAD request for index page
    Mock::given(method("HEAD"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-type", "text/html"))
        .mount(&mock_server)
        .await;

    // Mock index with link to PDF
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!(
                    r#"<html><head><title>Home</title></head><body>
                    <a href="{}/document.pdf">PDF Document</a>
                    </body></html>"#,
                    base_url
                ))
                .insert_header("content-type", "text/html"),
        )
        .mount(&mock_server)
        .await;

    // Mock PDF HEAD request (crawler checks content-type first)
    Mock::given(method("HEAD"))
        .and(path("/document.pdf"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-type", "application/pdf"))
        .mount(&mock_server)
        .await;

    // Mock PDF GET request (in case HEAD doesn't work)
    Mock::given(method("GET"))
        .and(path("/document.pdf"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(vec![0x25, 0x50, 0x44, 0x46]) // %PDF
                .insert_header("content-type", "application/pdf"),
        )
        .mount(&mock_server)
        .await;

    // Create test database
    let db_path = format!("/tmp/test_content_type_{}.db", std::process::id());
    let _ = std::fs::remove_file(&db_path);

    // Create config with extracted domain
    let config = create_test_config(&domain, vec![format!("{}/", base_url)], &db_path);

    // Run the crawl
    let mut coordinator = Coordinator::new(config, true).expect("Failed to create coordinator");
    coordinator.run().await.expect("Crawl failed");

    // Verify results
    let storage = SqliteStorage::new(std::path::Path::new(&db_path)).expect("Failed to open DB");

    // Debug: print all pages
    let total_pages = storage.count_total_pages().expect("Failed to count pages");
    println!("Total pages: {}", total_pages);

    let processed = storage
        .count_pages_by_state(PageState::Processed)
        .unwrap_or(0);
    let content_mismatch = storage
        .count_pages_by_state(PageState::ContentMismatch)
        .unwrap_or(0);
    let queued = storage.count_pages_by_state(PageState::Queued).unwrap_or(0);
    let failed = storage.count_pages_by_state(PageState::Failed).unwrap_or(0);

    println!(
        "Processed: {}, ContentMismatch: {}, Queued: {}, Failed: {}",
        processed, content_mismatch, queued, failed
    );

    // PDF should be marked as ContentMismatch
    assert!(
        content_mismatch >= 1,
        "Expected at least 1 content mismatch page, got: processed={}, content_mismatch={}, queued={}, failed={}",
        processed, content_mismatch, queued, failed
    );

    // Clean up
    let _ = std::fs::remove_file(&db_path);
}
