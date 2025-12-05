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

    /// Returns the raw robots.txt content
    ///
    /// # Returns
    ///
    /// The raw robots.txt content string
    pub fn content(&self) -> String {
        self.content.clone()
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
    pub fn crawl_delay(&self, user_agent: &str) -> Option<f64> {
        if self.allow_all || self.content.is_empty() {
            return None;
        }

        // Parse robots.txt manually to find Crawl-delay directive
        // Format: Crawl-delay: <seconds>
        // This directive applies to the most recent User-agent group

        let mut current_user_agents: Vec<String> = Vec::new();
        let mut crawl_delay_for_wildcard: Option<f64> = None;
        let mut crawl_delay_for_agent: Option<f64> = None;

        let normalized_agent = user_agent.to_lowercase();

        for line in self.content.lines() {
            let trimmed = line.trim();

            // Skip comments and empty lines
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Parse directive
            if let Some((key, value)) = trimmed.split_once(':') {
                let key = key.trim().to_lowercase();
                let value = value.trim();

                match key.as_str() {
                    "user-agent" => {
                        // Add to current user-agent group
                        // Multiple User-agent lines belong to the same group
                        current_user_agents.push(value.to_lowercase());
                    }
                    "crawl-delay" => {
                        // Parse the crawl delay value
                        if let Ok(delay) = value.parse::<f64>() {
                            // Check if this applies to our user agent or wildcard
                            if current_user_agents
                                .iter()
                                .any(|ua| ua == "*" || normalized_agent.contains(ua))
                            {
                                if current_user_agents.contains(&"*".to_string()) {
                                    crawl_delay_for_wildcard = Some(delay);
                                } else {
                                    crawl_delay_for_agent = Some(delay);
                                }
                            }
                        }
                        // After processing crawl-delay, clear current group
                        // The next User-agent directive will start a new group
                        current_user_agents.clear();
                    }
                    _ => {
                        // Other directives (Allow, Disallow, etc.)
                        // These don't affect crawl delay parsing
                    }
                }
            }
        }

        // Prefer specific user-agent delay over wildcard delay
        crawl_delay_for_agent.or(crawl_delay_for_wildcard)
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

    #[test]
    fn test_crawl_delay_wildcard() {
        let content = "User-agent: *\nCrawl-delay: 10\nDisallow: /admin";
        let robots = ParsedRobots::from_content(content);
        assert_eq!(robots.crawl_delay("TestBot"), Some(10.0));
        assert_eq!(robots.crawl_delay("AnyBot"), Some(10.0));
    }

    #[test]
    fn test_crawl_delay_specific_agent() {
        let content = "User-agent: TestBot\nCrawl-delay: 5\n\nUser-agent: *\nCrawl-delay: 10";
        let robots = ParsedRobots::from_content(content);
        assert_eq!(robots.crawl_delay("TestBot"), Some(5.0));
        assert_eq!(robots.crawl_delay("OtherBot"), Some(10.0));
    }

    #[test]
    fn test_crawl_delay_no_delay() {
        let content = "User-agent: *\nDisallow: /admin";
        let robots = ParsedRobots::from_content(content);
        assert_eq!(robots.crawl_delay("TestBot"), None);
    }

    #[test]
    fn test_crawl_delay_decimal() {
        let content = "User-agent: *\nCrawl-delay: 2.5";
        let robots = ParsedRobots::from_content(content);
        assert_eq!(robots.crawl_delay("TestBot"), Some(2.5));
    }

    #[test]
    fn test_crawl_delay_allow_all() {
        let robots = ParsedRobots::allow_all();
        assert_eq!(robots.crawl_delay("TestBot"), None);
    }

    #[test]
    fn test_crawl_delay_case_insensitive() {
        let content = "User-agent: TestBot\ncrawl-delay: 7";
        let robots = ParsedRobots::from_content(content);
        assert_eq!(robots.crawl_delay("testbot"), Some(7.0));
        assert_eq!(robots.crawl_delay("TESTBOT"), Some(7.0));
    }

    #[test]
    fn test_crawl_delay_multiple_user_agents() {
        let content = "User-agent: BotA\nUser-agent: BotB\nCrawl-delay: 3";
        let robots = ParsedRobots::from_content(content);
        assert_eq!(robots.crawl_delay("BotA"), Some(3.0));
        assert_eq!(robots.crawl_delay("BotB"), Some(3.0));
        assert_eq!(robots.crawl_delay("BotC"), None);
    }
}
