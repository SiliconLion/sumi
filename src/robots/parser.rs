//! Robots.txt parser implementation
//!
//! This module provides functionality for parsing robots.txt content using the robotstxt crate.

use robotstxt::DefaultMatcher;

/// Parsed robots.txt data
///
/// This is a wrapper around the robotstxt crate's types, providing a simplified
/// interface for checking if URLs are allowed.
#[derive(Debug, Clone)]
pub struct ParsedRobots {
    /// Raw robots.txt content (empty string means allow all)
    content: String,
    /// Whether to allow all (true = allow all, false = parse content)
    allow_all: bool,
}

impl ParsedRobots {
    /// Creates a new ParsedRobots from raw robots.txt content
    ///
    /// # Arguments
    ///
    /// * `content` - The raw robots.txt file content
    ///
    /// # Returns
    ///
    /// A ParsedRobots instance that can be used to check URL permissions
    pub fn from_content(content: &str) -> Self {
        Self {
            content: content.to_string(),
            allow_all: false,
        }
    }

    /// Creates a permissive ParsedRobots that allows everything
    ///
    /// This is used as the default when robots.txt cannot be fetched or parsed.
    pub fn allow_all() -> Self {
        Self {
            content: String::new(),
            allow_all: true,
        }
    }

    /// Checks if a URL is allowed for the given user agent
    ///
    /// # Arguments
    ///
    /// * `url` - The URL path to check (e.g., "/page.html")
    /// * `user_agent` - The user agent string
    ///
    /// # Returns
    ///
    /// * `true` - If the URL is allowed
    /// * `false` - If the URL is disallowed
    pub fn is_allowed(&self, url: &str, user_agent: &str) -> bool {
        if self.allow_all || self.content.is_empty() {
            // Empty content or explicit allow-all means allow all
            return true;
        }

        // Parse and check on-demand
        let mut matcher = DefaultMatcher::default();
        matcher.one_agent_allowed_by_robots(&self.content, user_agent, url)
    }

    /// Gets the crawl delay for a specific user agent
    ///
    /// # Arguments
    ///
    /// * `user_agent` - The user agent string
    ///
    /// # Returns
    ///
    /// * `Some(f64)` - The crawl delay in seconds
    /// * `None` - If no crawl delay is specified
    pub fn crawl_delay(&self, _user_agent: &str) -> Option<f64> {
        // TODO: Implement crawl delay extraction from robotstxt
        // The robotstxt crate doesn't directly expose crawl-delay,
        // so we may need to parse it manually
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allow_all() {
        let robots = ParsedRobots::allow_all();
        assert!(robots.is_allowed("/any/path", "TestBot"));
        assert!(robots.is_allowed("/admin", "TestBot"));
    }

    #[test]
    fn test_parse_disallow_all() {
        let content = "User-agent: *\nDisallow: /";
        let robots = ParsedRobots::from_content(content);
        assert!(!robots.is_allowed("/", "TestBot"));
        assert!(!robots.is_allowed("/page", "TestBot"));
    }

    #[test]
    fn test_parse_disallow_specific() {
        let content = "User-agent: *\nDisallow: /admin";
        let robots = ParsedRobots::from_content(content);
        assert!(robots.is_allowed("/", "TestBot"));
        assert!(robots.is_allowed("/page", "TestBot"));
        assert!(!robots.is_allowed("/admin", "TestBot"));
        assert!(!robots.is_allowed("/admin/users", "TestBot"));
    }

    #[test]
    fn test_parse_allow_and_disallow() {
        let content = "User-agent: *\nDisallow: /private\nAllow: /private/public";
        let robots = ParsedRobots::from_content(content);
        assert!(robots.is_allowed("/", "TestBot"));
        assert!(!robots.is_allowed("/private", "TestBot"));
        assert!(robots.is_allowed("/private/public", "TestBot"));
    }

    #[test]
    fn test_parse_specific_user_agent() {
        let content = "User-agent: BadBot\nDisallow: /\n\nUser-agent: *\nAllow: /";
        let robots = ParsedRobots::from_content(content);
        assert!(robots.is_allowed("/page", "GoodBot"));
        assert!(!robots.is_allowed("/page", "BadBot"));
    }

    #[test]
    fn test_invalid_robots_txt() {
        let content = "This is not valid robots.txt {{{";
        let robots = ParsedRobots::from_content(content);
        // Should fall back to allow_all behavior
        assert!(robots.is_allowed("/any/path", "TestBot"));
    }

    #[test]
    fn test_empty_robots_txt() {
        let content = "";
        let robots = ParsedRobots::from_content(content);
        assert!(robots.is_allowed("/any/path", "TestBot"));
    }
}
