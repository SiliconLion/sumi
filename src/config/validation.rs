use crate::config::types::{Config, CrawlerConfig, DomainEntry, QualityEntry, UserAgentConfig};
use crate::ConfigError;
use url::Url;

/// Validates the entire configuration
pub fn validate(config: &Config) -> Result<(), ConfigError> {
    validate_crawler_config(&config.crawler)?;
    validate_user_agent_config(&config.user_agent)?;
    validate_output_config(&config.output)?;
    validate_quality_domains(&config.quality)?;
    validate_blacklist_domains(&config.blacklist)?;
    validate_stub_domains(&config.stub)?;
    Ok(())
}

/// Validates crawler configuration
fn validate_crawler_config(config: &CrawlerConfig) -> Result<(), ConfigError> {
    // max_depth >= 0 is always true for u32, so no check needed

    if config.max_concurrent_pages_open < 1 || config.max_concurrent_pages_open > 100 {
        return Err(ConfigError::Validation(format!(
            "max_concurrent_pages_open must be between 1 and 100, got {}",
            config.max_concurrent_pages_open
        )));
    }

    if config.minimum_time_on_page < 100 {
        return Err(ConfigError::Validation(format!(
            "minimum_time_on_page must be >= 100ms, got {}ms",
            config.minimum_time_on_page
        )));
    }

    if config.max_domain_requests < 1 {
        return Err(ConfigError::Validation(format!(
            "max_domain_requests must be >= 1, got {}",
            config.max_domain_requests
        )));
    }

    Ok(())
}

/// Validates user agent configuration
fn validate_user_agent_config(config: &UserAgentConfig) -> Result<(), ConfigError> {
    // Validate crawler name: non-empty, alphanumeric + hyphens only
    if config.crawler_name.is_empty() {
        return Err(ConfigError::Validation(
            "crawler_name cannot be empty".to_string(),
        ));
    }

    if !config
        .crawler_name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-')
    {
        return Err(ConfigError::Validation(format!(
            "crawler_name must contain only alphanumeric characters and hyphens, got '{}'",
            config.crawler_name
        )));
    }

    // Validate contact URL
    Url::parse(&config.contact_url)
        .map_err(|e| ConfigError::InvalidUrl(format!("Invalid contact_url: {}", e)))?;

    // Validate contact email (basic validation)
    validate_email(&config.contact_email)?;

    Ok(())
}

/// Validates output configuration
fn validate_output_config(config: &crate::config::types::OutputConfig) -> Result<(), ConfigError> {
    if config.database_path.is_empty() {
        return Err(ConfigError::Validation(
            "database_path cannot be empty".to_string(),
        ));
    }

    if config.summary_path.is_empty() {
        return Err(ConfigError::Validation(
            "summary_path cannot be empty".to_string(),
        ));
    }

    Ok(())
}

/// Validates quality domain entries
fn validate_quality_domains(domains: &[QualityEntry]) -> Result<(), ConfigError> {
    for entry in domains {
        validate_domain_pattern(&entry.domain)?;

        if entry.seeds.is_empty() {
            return Err(ConfigError::Validation(format!(
                "Quality domain '{}' must have at least one seed URL",
                entry.domain
            )));
        }

        for seed in &entry.seeds {
            let url = Url::parse(seed).map_err(|e| {
                ConfigError::InvalidUrl(format!("Invalid seed URL '{}': {}", seed, e))
            })?;

            if url.scheme() != "https" {
                return Err(ConfigError::Validation(format!(
                    "Seed URL '{}' must use HTTPS scheme",
                    seed
                )));
            }
        }
    }

    Ok(())
}

/// Validates blacklist domain entries
fn validate_blacklist_domains(domains: &[DomainEntry]) -> Result<(), ConfigError> {
    for entry in domains {
        validate_domain_pattern(&entry.domain)?;
    }
    Ok(())
}

/// Validates stub domain entries
fn validate_stub_domains(domains: &[DomainEntry]) -> Result<(), ConfigError> {
    for entry in domains {
        validate_domain_pattern(&entry.domain)?;
    }
    Ok(())
}

/// Validates a domain pattern (supports wildcards)
fn validate_domain_pattern(pattern: &str) -> Result<(), ConfigError> {
    if pattern.is_empty() {
        return Err(ConfigError::InvalidPattern(
            "Domain pattern cannot be empty".to_string(),
        ));
    }

    // Check if it's a wildcard pattern
    if let Some(domain) = pattern.strip_prefix("*.") {
        // Validate the base domain part
        validate_domain_string(domain)?;
    } else {
        // Regular domain
        validate_domain_string(pattern)?;
    }

    Ok(())
}

/// Validates a domain string (without wildcard prefix)
fn validate_domain_string(domain: &str) -> Result<(), ConfigError> {
    if domain.is_empty() {
        return Err(ConfigError::InvalidPattern(
            "Domain cannot be empty".to_string(),
        ));
    }

    // Check for invalid characters
    if !domain
        .chars()
        .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
    {
        return Err(ConfigError::InvalidPattern(format!(
            "Domain '{}' contains invalid characters",
            domain
        )));
    }

    // Check that it doesn't start or end with a dot or hyphen
    if domain.starts_with('.')
        || domain.ends_with('.')
        || domain.starts_with('-')
        || domain.ends_with('-')
    {
        return Err(ConfigError::InvalidPattern(format!(
            "Domain '{}' cannot start or end with '.' or '-'",
            domain
        )));
    }

    // Check for consecutive dots
    if domain.contains("..") {
        return Err(ConfigError::InvalidPattern(format!(
            "Domain '{}' cannot contain consecutive dots",
            domain
        )));
    }

    // Must contain at least one dot (e.g., example.com, not just "example")
    if !domain.contains('.') {
        return Err(ConfigError::InvalidPattern(format!(
            "Domain '{}' must contain at least one dot (e.g., 'example.com')",
            domain
        )));
    }

    Ok(())
}

/// Basic email validation
fn validate_email(email: &str) -> Result<(), ConfigError> {
    if email.is_empty() {
        return Err(ConfigError::Validation(
            "contact_email cannot be empty".to_string(),
        ));
    }

    // Basic email format check: must contain @ and have text on both sides
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 {
        return Err(ConfigError::Validation(format!(
            "Invalid email format: '{}'",
            email
        )));
    }

    let local = parts[0];
    let domain = parts[1];

    if local.is_empty() || domain.is_empty() {
        return Err(ConfigError::Validation(format!(
            "Invalid email format: '{}'",
            email
        )));
    }

    // Domain part should contain at least one dot
    if !domain.contains('.') {
        return Err(ConfigError::Validation(format!(
            "Invalid email domain: '{}'",
            email
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_domain_pattern() {
        assert!(validate_domain_pattern("example.com").is_ok());
        assert!(validate_domain_pattern("*.example.com").is_ok());
        assert!(validate_domain_pattern("sub.example.com").is_ok());

        assert!(validate_domain_pattern("").is_err());
        assert!(validate_domain_pattern("*.").is_err());
        assert!(validate_domain_pattern("example").is_err());
        assert!(validate_domain_pattern(".example.com").is_err());
        assert!(validate_domain_pattern("example.com.").is_err());
    }

    #[test]
    fn test_validate_email() {
        assert!(validate_email("user@example.com").is_ok());
        assert!(validate_email("admin@sub.example.com").is_ok());

        assert!(validate_email("").is_err());
        assert!(validate_email("invalid").is_err());
        assert!(validate_email("@example.com").is_err());
        assert!(validate_email("user@").is_err());
        assert!(validate_email("user@domain").is_err());
    }
}
