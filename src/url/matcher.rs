/// Checks if a domain matches a wildcard pattern
///
/// This function supports two types of patterns:
/// 1. Exact match: "example.com" matches only "example.com"
/// 2. Wildcard match: "*.example.com" matches:
///    - "example.com" (the bare domain)
///    - "blog.example.com" (single subdomain)
///    - "api.v2.example.com" (nested subdomains)
///
/// # Arguments
///
/// * `pattern` - The domain pattern, optionally starting with "*."
/// * `candidate` - The domain to check against the pattern
///
/// # Returns
///
/// * `true` - If the candidate matches the pattern
/// * `false` - Otherwise
///
/// # Examples
///
/// ```
/// use sumi_ripple::url::matches_wildcard;
///
/// // Exact match
/// assert!(matches_wildcard("example.com", "example.com"));
/// assert!(!matches_wildcard("example.com", "other.com"));
///
/// // Wildcard match
/// assert!(matches_wildcard("*.example.com", "example.com"));
/// assert!(matches_wildcard("*.example.com", "blog.example.com"));
/// assert!(matches_wildcard("*.example.com", "api.v2.example.com"));
/// assert!(!matches_wildcard("*.example.com", "example.org"));
/// ```
pub fn matches_wildcard(pattern: &str, candidate: &str) -> bool {
    if let Some(base) = pattern.strip_prefix("*.") {
        // Wildcard pattern: matches the base domain itself or any subdomain
        candidate == base || candidate.ends_with(&format!(".{}", base))
    } else {
        // Exact match only
        candidate == pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        assert!(matches_wildcard("example.com", "example.com"));
        assert!(matches_wildcard("blog.example.com", "blog.example.com"));
    }

    #[test]
    fn test_exact_no_match() {
        assert!(!matches_wildcard("example.com", "other.com"));
        assert!(!matches_wildcard("example.com", "blog.example.com"));
        assert!(!matches_wildcard("blog.example.com", "example.com"));
    }

    #[test]
    fn test_wildcard_matches_bare_domain() {
        assert!(matches_wildcard("*.example.com", "example.com"));
        assert!(matches_wildcard("*.github.com", "github.com"));
    }

    #[test]
    fn test_wildcard_matches_single_subdomain() {
        assert!(matches_wildcard("*.example.com", "blog.example.com"));
        assert!(matches_wildcard("*.example.com", "api.example.com"));
        assert!(matches_wildcard("*.example.com", "www.example.com"));
    }

    #[test]
    fn test_wildcard_matches_nested_subdomains() {
        assert!(matches_wildcard("*.example.com", "api.v2.example.com"));
        assert!(matches_wildcard(
            "*.example.com",
            "deep.nested.sub.example.com"
        ));
    }

    #[test]
    fn test_wildcard_no_match_different_domain() {
        assert!(!matches_wildcard("*.example.com", "example.org"));
        assert!(!matches_wildcard("*.example.com", "notexample.com"));
        assert!(!matches_wildcard("*.example.com", "examplexcom"));
    }

    #[test]
    fn test_wildcard_no_match_partial() {
        // Should not match if it's just part of the domain name
        assert!(!matches_wildcard("*.example.com", "myexample.com"));
        assert!(!matches_wildcard("*.example.com", "example.com.org"));
    }

    #[test]
    fn test_case_sensitivity() {
        // Domains should be normalized to lowercase before this function,
        // but the function itself is case-sensitive
        assert!(matches_wildcard("example.com", "example.com"));
        assert!(!matches_wildcard("example.com", "EXAMPLE.COM"));
        assert!(!matches_wildcard("example.com", "Example.COM"));
    }

    #[test]
    fn test_empty_strings() {
        assert!(!matches_wildcard("*.example.com", ""));
        assert!(!matches_wildcard("", "example.com"));
        assert!(matches_wildcard("", ""));
    }

    #[test]
    fn test_wildcard_with_tld_only() {
        assert!(matches_wildcard("*.com", "com"));
        assert!(matches_wildcard("*.com", "example.com"));
        assert!(matches_wildcard("*.com", "blog.example.com"));
    }

    #[test]
    fn test_complex_patterns() {
        let pattern = "*.github.io";

        assert!(matches_wildcard(pattern, "github.io"));
        assert!(matches_wildcard(pattern, "username.github.io"));
        assert!(matches_wildcard(pattern, "org.github.io"));
        assert!(!matches_wildcard(pattern, "github.com"));
    }

    #[test]
    fn test_multiple_dots_in_base() {
        let pattern = "*.co.uk";

        assert!(matches_wildcard(pattern, "co.uk"));
        assert!(matches_wildcard(pattern, "example.co.uk"));
        assert!(matches_wildcard(pattern, "blog.example.co.uk"));
        assert!(!matches_wildcard(pattern, "co.jp"));
    }
}
