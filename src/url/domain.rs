use url::Url;

/// Extracts the domain from a URL
///
/// This function retrieves the host portion of a URL and converts it to lowercase.
/// If the URL has no host (which shouldn't happen for valid HTTP(S) URLs), it returns None.
///
/// # Arguments
///
/// * `url` - The URL to extract the domain from
///
/// # Returns
///
/// * `Some(String)` - The lowercase domain/host
/// * `None` - If the URL has no host
///
/// # Examples
///
/// ```
/// use url::Url;
/// use sumi_ripple::url::extract_domain;
///
/// let url = Url::parse("https://example.com/path").unwrap();
/// assert_eq!(extract_domain(&url), Some("example.com".to_string()));
///
/// let url = Url::parse("https://EXAMPLE.COM/path").unwrap();
/// assert_eq!(extract_domain(&url), Some("example.com".to_string()));
///
/// let url = Url::parse("https://sub.example.com/path").unwrap();
/// assert_eq!(extract_domain(&url), Some("sub.example.com".to_string()));
/// ```
pub fn extract_domain(url: &Url) -> Option<String> {
    url.host_str().map(|h| h.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_domain() {
        let url = Url::parse("https://example.com/").unwrap();
        assert_eq!(extract_domain(&url), Some("example.com".to_string()));
    }

    #[test]
    fn test_extract_subdomain() {
        let url = Url::parse("https://blog.example.com/post").unwrap();
        assert_eq!(extract_domain(&url), Some("blog.example.com".to_string()));
    }

    #[test]
    fn test_extract_nested_subdomain() {
        let url = Url::parse("https://api.v2.example.com/endpoint").unwrap();
        assert_eq!(extract_domain(&url), Some("api.v2.example.com".to_string()));
    }

    #[test]
    fn test_extract_with_port() {
        let url = Url::parse("https://example.com:8080/").unwrap();
        assert_eq!(extract_domain(&url), Some("example.com".to_string()));
    }

    #[test]
    fn test_extract_uppercase_converted_to_lowercase() {
        let url = Url::parse("https://EXAMPLE.COM/").unwrap();
        assert_eq!(extract_domain(&url), Some("example.com".to_string()));
    }

    #[test]
    fn test_extract_mixed_case() {
        let url = Url::parse("https://Example.COM/").unwrap();
        assert_eq!(extract_domain(&url), Some("example.com".to_string()));
    }

    #[test]
    fn test_extract_with_path_and_query() {
        let url = Url::parse("https://example.com/path/to/page?query=value").unwrap();
        assert_eq!(extract_domain(&url), Some("example.com".to_string()));
    }

    #[test]
    fn test_extract_with_fragment() {
        let url = Url::parse("https://example.com/page#section").unwrap();
        assert_eq!(extract_domain(&url), Some("example.com".to_string()));
    }
}
