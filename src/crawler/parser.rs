//! HTML parser for extracting links and metadata
//!
//! This module handles parsing HTML content to extract:
//! - Links to follow (from <a> tags and canonical links)
//! - Page title
//! - Other metadata as needed

use scraper::{Html, Selector};
use url::Url;

/// Extracted information from an HTML page
#[derive(Debug, Clone)]
pub struct ParsedPage {
    /// The page title (from <title> tag)
    pub title: Option<String>,

    /// All links found on the page (absolute URLs)
    pub links: Vec<String>,
}

/// Parses HTML content and extracts links and metadata
///
/// # Link Extraction Rules
///
/// **Include:**
/// - `<a href="...">` tags in body, nav, header, footer
/// - `<link rel="canonical" href="...">`
///
/// **Exclude:**
/// - `<link rel="stylesheet" ...>`
/// - `<script src="...">`
/// - `<img src="...">`
/// - `<a href="..." download>`
/// - `javascript:`, `mailto:`, `tel:` links
/// - Data URIs
///
/// **Note:** `rel="nofollow"` links ARE followed per spec
///
/// # Arguments
///
/// * `html` - The HTML content to parse
/// * `base_url` - The base URL for resolving relative links
///
/// # Returns
///
/// * `Ok(ParsedPage)` - Successfully parsed page
/// * `Err(String)` - Failed to parse HTML
///
/// # Example
///
/// ```no_run
/// use sumi_ripple::crawler::parse_html;
/// use url::Url;
///
/// let html = r#"<html><head><title>Test</title></head><body><a href="/page">Link</a></body></html>"#;
/// let base_url = Url::parse("https://example.com/").unwrap();
/// let parsed = parse_html(html, &base_url).unwrap();
/// assert_eq!(parsed.title, Some("Test".to_string()));
/// ```
pub fn parse_html(html: &str, base_url: &Url) -> Result<ParsedPage, String> {
    let document = Html::parse_document(html);

    // Extract title
    let title = extract_title(&document);

    // Extract links
    let links = extract_links(&document, base_url)?;

    Ok(ParsedPage { title, links })
}

/// Extracts the page title from the HTML document
fn extract_title(document: &Html) -> Option<String> {
    let title_selector = Selector::parse("title").ok()?;

    document
        .select(&title_selector)
        .next()
        .map(|element| element.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Extracts all valid links from the HTML document
fn extract_links(document: &Html, base_url: &Url) -> Result<Vec<String>, String> {
    let mut links = Vec::new();

    // Extract links from <a> tags
    if let Ok(a_selector) = Selector::parse("a[href]") {
        for element in document.select(&a_selector) {
            // Skip if it has the download attribute
            if element.value().attr("download").is_some() {
                continue;
            }

            if let Some(href) = element.value().attr("href") {
                if let Some(absolute_url) = resolve_link(href, base_url) {
                    links.push(absolute_url);
                }
            }
        }
    }

    // Extract canonical link
    if let Ok(canonical_selector) = Selector::parse("link[rel='canonical'][href]") {
        for element in document.select(&canonical_selector) {
            if let Some(href) = element.value().attr("href") {
                if let Some(absolute_url) = resolve_link(href, base_url) {
                    links.push(absolute_url);
                }
            }
        }
    }

    Ok(links)
}

/// Resolves a link href to an absolute URL and validates it
///
/// Returns None if the link should be excluded:
/// - javascript:, mailto:, tel: schemes
/// - data: URIs
/// - Invalid URLs
/// - Non-HTTP(S) URLs after resolution
fn resolve_link(href: &str, base_url: &Url) -> Option<String> {
    let href = href.trim();

    // Skip empty hrefs
    if href.is_empty() {
        return None;
    }

    // Skip special schemes
    if href.starts_with("javascript:")
        || href.starts_with("mailto:")
        || href.starts_with("tel:")
        || href.starts_with("data:")
    {
        return None;
    }

    // Skip fragment-only links (same page anchors)
    if href.starts_with('#') {
        return None;
    }

    // Try to resolve the URL
    match base_url.join(href) {
        Ok(absolute_url) => {
            // Only accept HTTP and HTTPS URLs
            if absolute_url.scheme() == "http" || absolute_url.scheme() == "https" {
                Some(absolute_url.to_string())
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Convenience function for extracting just the links from HTML
///
/// # Arguments
///
/// * `html` - The HTML content
/// * `base_url` - The base URL for resolving relative links
///
/// # Returns
///
/// A vector of absolute URLs found in the HTML
pub fn extract_links_simple(html: &str, base_url: &Url) -> Vec<String> {
    parse_html(html, base_url)
        .map(|parsed| parsed.links)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_url() -> Url {
        Url::parse("https://example.com/page").unwrap()
    }

    #[test]
    fn test_extract_title() {
        let html = r#"<html><head><title>Test Page</title></head><body></body></html>"#;
        let parsed = parse_html(html, &base_url()).unwrap();
        assert_eq!(parsed.title, Some("Test Page".to_string()));
    }

    #[test]
    fn test_extract_title_with_whitespace() {
        let html = r#"<html><head><title>  Test Page  </title></head><body></body></html>"#;
        let parsed = parse_html(html, &base_url()).unwrap();
        assert_eq!(parsed.title, Some("Test Page".to_string()));
    }

    #[test]
    fn test_no_title() {
        let html = r#"<html><head></head><body></body></html>"#;
        let parsed = parse_html(html, &base_url()).unwrap();
        assert_eq!(parsed.title, None);
    }

    #[test]
    fn test_extract_absolute_link() {
        let html = r#"<html><body><a href="https://other.com/page">Link</a></body></html>"#;
        let parsed = parse_html(html, &base_url()).unwrap();
        assert_eq!(parsed.links.len(), 1);
        assert_eq!(parsed.links[0], "https://other.com/page");
    }

    #[test]
    fn test_extract_relative_link() {
        let html = r#"<html><body><a href="/other">Link</a></body></html>"#;
        let parsed = parse_html(html, &base_url()).unwrap();
        assert_eq!(parsed.links.len(), 1);
        assert_eq!(parsed.links[0], "https://example.com/other");
    }

    #[test]
    fn test_extract_relative_path_link() {
        let html = r#"<html><body><a href="other">Link</a></body></html>"#;
        let parsed = parse_html(html, &base_url()).unwrap();
        assert_eq!(parsed.links.len(), 1);
        assert_eq!(parsed.links[0], "https://example.com/other");
    }

    #[test]
    fn test_skip_javascript_link() {
        let html = r#"<html><body><a href="javascript:void(0)">Link</a></body></html>"#;
        let parsed = parse_html(html, &base_url()).unwrap();
        assert_eq!(parsed.links.len(), 0);
    }

    #[test]
    fn test_skip_mailto_link() {
        let html = r#"<html><body><a href="mailto:test@example.com">Email</a></body></html>"#;
        let parsed = parse_html(html, &base_url()).unwrap();
        assert_eq!(parsed.links.len(), 0);
    }

    #[test]
    fn test_skip_tel_link() {
        let html = r#"<html><body><a href="tel:+1234567890">Call</a></body></html>"#;
        let parsed = parse_html(html, &base_url()).unwrap();
        assert_eq!(parsed.links.len(), 0);
    }

    #[test]
    fn test_skip_data_uri() {
        let html = r#"<html><body><a href="data:text/html,<h1>Test</h1>">Data</a></body></html>"#;
        let parsed = parse_html(html, &base_url()).unwrap();
        assert_eq!(parsed.links.len(), 0);
    }

    #[test]
    fn test_skip_download_link() {
        let html = r#"<html><body><a href="/file.pdf" download>Download</a></body></html>"#;
        let parsed = parse_html(html, &base_url()).unwrap();
        assert_eq!(parsed.links.len(), 0);
    }

    #[test]
    fn test_skip_fragment_only() {
        let html = r##"<html><body><a href="#section">Jump</a></body></html>"##;
        let parsed = parse_html(html, &base_url()).unwrap();
        assert_eq!(parsed.links.len(), 0);
    }

    #[test]
    fn test_follow_nofollow_links() {
        let html = r#"<html><body><a href="/page" rel="nofollow">Link</a></body></html>"#;
        let parsed = parse_html(html, &base_url()).unwrap();
        assert_eq!(parsed.links.len(), 1);
        assert_eq!(parsed.links[0], "https://example.com/page");
    }

    #[test]
    fn test_extract_canonical_link() {
        let html = r#"<html><head><link rel="canonical" href="https://example.com/canonical" /></head><body></body></html>"#;
        let parsed = parse_html(html, &base_url()).unwrap();
        assert!(parsed
            .links
            .contains(&"https://example.com/canonical".to_string()));
    }

    #[test]
    fn test_multiple_links() {
        let html = r#"
            <html>
            <body>
                <a href="/page1">Link 1</a>
                <a href="/page2">Link 2</a>
                <a href="https://other.com/page3">Link 3</a>
            </body>
            </html>
        "#;
        let parsed = parse_html(html, &base_url()).unwrap();
        assert_eq!(parsed.links.len(), 3);
    }

    #[test]
    fn test_mixed_valid_and_invalid_links() {
        let html = r#"
            <html>
            <body>
                <a href="/valid">Valid</a>
                <a href="javascript:alert('no')">Invalid</a>
                <a href="mailto:test@example.com">Invalid</a>
                <a href="/another-valid">Valid</a>
            </body>
            </html>
        "#;
        let parsed = parse_html(html, &base_url()).unwrap();
        assert_eq!(parsed.links.len(), 2);
    }
}
