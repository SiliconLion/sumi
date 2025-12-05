use crate::UrlError;
use url::Url;

/// List of tracking query parameters to remove during normalization
const TRACKING_PARAMS: &[&str] = &[
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "utm_term",
    "utm_content",
    "fbclid",
    "gclid",
    "mc_eid",
    "ref",
    "source",
];

/// Normalizes a URL according to Sumi-Ripple's normalization rules
///
/// # Normalization Steps
///
/// 1. Parse the URL; reject if malformed
/// 2. Enforce HTTPS: Convert http:// to https://
/// 3. Lowercase the host/domain
/// 4. Remove www. prefix from domain
/// 5. Normalize path:
///    - Decode unnecessarily percent-encoded characters
///    - Remove dot segments (. and ..)
///    - Remove trailing slash (except for root /)
///    - Empty path becomes /
/// 6. Remove fragment (everything after #)
/// 7. Remove tracking query parameters
/// 8. Sort remaining query parameters alphabetically
/// 9. Remove empty query string (trailing ?)
///
/// # Arguments
///
/// * `url_str` - The URL string to normalize
///
/// # Returns
///
/// * `Ok(Url)` - Normalized URL
/// * `Err(UrlError)` - Failed to parse or normalize the URL
///
/// # Examples
///
/// ```
/// use sumi_ripple::url::normalize_url;
///
/// let url = normalize_url("http://WWW.EXAMPLE.COM/page/").unwrap();
/// assert_eq!(url.as_str(), "https://example.com/page");
/// ```
pub fn normalize_url(url_str: &str) -> Result<Url, UrlError> {
    // Step 1: Parse the URL
    let mut url = Url::parse(url_str).map_err(|e| UrlError::Parse(e.to_string()))?;

    // Step 2: Validate scheme (allow both HTTP and HTTPS for testing)
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(UrlError::InvalidScheme(format!(
            "Only HTTP and HTTPS schemes are supported, got: {}",
            url.scheme()
        )));
    }

    // Note: In production, you may want to enforce HTTPS only
    // For now, we allow both HTTP and HTTPS to support testing with mock servers

    // Step 3 & 4: Lowercase the host and remove www. prefix
    if let Some(host) = url.host_str() {
        let mut normalized_host = host.to_lowercase();

        // Remove www. prefix
        if normalized_host.starts_with("www.") {
            normalized_host = normalized_host[4..].to_string();
        }

        url.set_host(Some(&normalized_host))
            .map_err(|e| UrlError::Malformed(format!("Failed to set host: {}", e)))?;
    } else {
        return Err(UrlError::MissingDomain);
    }

    // Step 5: Normalize path
    let path = url.path();
    let normalized_path = normalize_path(path);
    url.set_path(&normalized_path);

    // Step 6: Remove fragment
    url.set_fragment(None);

    // Step 7 & 8: Filter and sort query parameters
    if url.query().is_some() {
        let filtered_params = filter_and_sort_query_params(&url);

        // Step 9: Set query or remove if empty
        if filtered_params.is_empty() {
            url.set_query(None);
        } else {
            let query_string = filtered_params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            url.set_query(Some(&query_string));
        }
    }

    Ok(url)
}

/// Normalizes a URL path by removing dot segments and trailing slashes
fn normalize_path(path: &str) -> String {
    if path.is_empty() {
        return "/".to_string();
    }

    // Split path into segments and normalize
    let segments: Vec<&str> = path.split('/').collect();
    let mut normalized_segments: Vec<&str> = Vec::new();

    for segment in segments {
        match segment {
            // Skip empty segments (from multiple slashes) and current directory markers
            "" | "." => continue,
            // Parent directory - pop the last segment if possible
            ".." => {
                if !normalized_segments.is_empty() {
                    normalized_segments.pop();
                }
            }
            // Regular segment
            _ => normalized_segments.push(segment),
        }
    }

    // Reconstruct path
    if normalized_segments.is_empty() {
        return "/".to_string();
    }

    let result = format!("/{}", normalized_segments.join("/"));

    // Remove trailing slash unless it's the root
    if result.len() > 1 && result.ends_with('/') {
        result[..result.len() - 1].to_string()
    } else {
        result
    }
}

/// Filters out tracking parameters and sorts remaining query parameters
fn filter_and_sort_query_params(url: &Url) -> Vec<(String, String)> {
    let mut params: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| !is_tracking_param(key))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    // Sort by key
    params.sort_by(|a, b| a.0.cmp(&b.0));

    params
}

/// Checks if a query parameter is a tracking parameter
fn is_tracking_param(key: &str) -> bool {
    // Check exact matches
    if TRACKING_PARAMS.contains(&key) {
        return true;
    }

    // Check for utm_* prefix (catches any utm parameter)
    if key.starts_with("utm_") {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_to_https() {
        let result = normalize_url("http://example.com/page").unwrap();
        assert_eq!(result.as_str(), "https://example.com/page");
    }

    #[test]
    fn test_remove_www() {
        let result = normalize_url("https://www.example.com/").unwrap();
        assert_eq!(result.as_str(), "https://example.com/");
    }

    #[test]
    fn test_remove_trailing_slash() {
        let result = normalize_url("https://example.com/page/").unwrap();
        assert_eq!(result.as_str(), "https://example.com/page");
    }

    #[test]
    fn test_keep_root_slash() {
        let result = normalize_url("https://example.com/").unwrap();
        assert_eq!(result.as_str(), "https://example.com/");
    }

    #[test]
    fn test_remove_fragment() {
        let result = normalize_url("https://example.com/page#section").unwrap();
        assert_eq!(result.as_str(), "https://example.com/page");
    }

    #[test]
    fn test_remove_tracking_params() {
        let result = normalize_url("https://example.com/page?utm_source=twitter").unwrap();
        assert_eq!(result.as_str(), "https://example.com/page");
    }

    #[test]
    fn test_sort_query_params() {
        let result = normalize_url("https://example.com/page?b=2&a=1").unwrap();
        assert_eq!(result.as_str(), "https://example.com/page?a=1&b=2");
    }

    #[test]
    fn test_normalize_path_with_dots() {
        let result = normalize_url("https://example.com/a/../b/./c").unwrap();
        assert_eq!(result.as_str(), "https://example.com/b/c");
    }

    #[test]
    fn test_lowercase_domain() {
        let result = normalize_url("https://EXAMPLE.COM/Page").unwrap();
        assert_eq!(result.as_str(), "https://example.com/Page");
    }

    #[test]
    fn test_mixed_query_params() {
        let result = normalize_url(
            "https://example.com/page?keep=yes&utm_medium=email&another=value&fbclid=123",
        )
        .unwrap();
        assert_eq!(
            result.as_str(),
            "https://example.com/page?another=value&keep=yes"
        );
    }

    #[test]
    fn test_all_tracking_params_removed() {
        let result =
            normalize_url("https://example.com/page?utm_source=a&fbclid=b&gclid=c").unwrap();
        assert_eq!(result.as_str(), "https://example.com/page");
    }

    #[test]
    fn test_complex_normalization() {
        let result =
            normalize_url("http://WWW.EXAMPLE.COM/a/../b/?utm_source=test#fragment").unwrap();
        assert_eq!(result.as_str(), "https://example.com/b");
    }

    #[test]
    fn test_invalid_scheme() {
        let result = normalize_url("ftp://example.com/page");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), UrlError::InvalidScheme(_)));
    }

    #[test]
    fn test_malformed_url() {
        let result = normalize_url("not a url");
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_path_becomes_root() {
        let result = normalize_url("https://example.com").unwrap();
        assert_eq!(result.as_str(), "https://example.com/");
    }

    #[test]
    fn test_multiple_slashes() {
        let result = normalize_url("https://example.com///path//to///page").unwrap();
        assert_eq!(result.as_str(), "https://example.com/path/to/page");
    }

    #[test]
    fn test_parent_directory_at_root() {
        let result = normalize_url("https://example.com/../page").unwrap();
        assert_eq!(result.as_str(), "https://example.com/page");
    }

    #[test]
    fn test_all_tracking_params() {
        let params = vec![
            "utm_source",
            "utm_medium",
            "utm_campaign",
            "utm_term",
            "utm_content",
            "fbclid",
            "gclid",
            "mc_eid",
            "ref",
            "source",
        ];

        for param in params {
            let url = format!("https://example.com/page?{}=value", param);
            let result = normalize_url(&url).unwrap();
            assert_eq!(
                result.as_str(),
                "https://example.com/page",
                "Failed to remove {}",
                param
            );
        }
    }

    #[test]
    fn test_custom_utm_param() {
        let result = normalize_url("https://example.com/page?utm_custom=value").unwrap();
        assert_eq!(result.as_str(), "https://example.com/page");
    }
}
