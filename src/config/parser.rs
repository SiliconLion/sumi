use crate::config::types::Config;
use crate::config::validation::validate;
use crate::ConfigError;
use sha2::{Digest, Sha256};
use std::path::Path;

/// Loads and parses a configuration file from the given path
///
/// # Arguments
///
/// * `path` - Path to the TOML configuration file
///
/// # Returns
///
/// * `Ok(Config)` - Successfully loaded and validated configuration
/// * `Err(ConfigError)` - Failed to load, parse, or validate the configuration
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use sumi_ripple::config::load_config;
///
/// let config = load_config(Path::new("config.toml")).unwrap();
/// println!("Max depth: {}", config.crawler.max_depth);
/// ```
pub fn load_config(path: &Path) -> Result<Config, ConfigError> {
    // Read the configuration file
    let content = std::fs::read_to_string(path)?;

    // Parse TOML
    let config: Config = toml::from_str(&content)?;

    // Validate the configuration
    validate(&config)?;

    Ok(config)
}

/// Computes a SHA-256 hash of the configuration file content
///
/// This is used to detect if the configuration has changed between crawl runs.
///
/// # Arguments
///
/// * `path` - Path to the TOML configuration file
///
/// # Returns
///
/// * `Ok(String)` - Hex-encoded SHA-256 hash of the file content
/// * `Err(ConfigError)` - Failed to read the file
pub fn compute_config_hash(path: &Path) -> Result<String, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    Ok(hex::encode(result))
}

/// Loads a configuration and returns both the config and its hash
///
/// # Arguments
///
/// * `path` - Path to the TOML configuration file
///
/// # Returns
///
/// * `Ok((Config, String))` - Successfully loaded configuration and its hash
/// * `Err(ConfigError)` - Failed to load or parse the configuration
pub fn load_config_with_hash(path: &Path) -> Result<(Config, String), ConfigError> {
    let config = load_config(path)?;
    let hash = compute_config_hash(path)?;
    Ok((config, hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_config(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_load_valid_config() {
        let config_content = r#"
[crawler]
max-depth = 3
max-concurrent-pages-open = 10
minimum-time-on-page = 1000
max-domain-requests = 500

[user-agent]
crawler-name = "TestCrawler"
crawler-version = "1.0"
contact-url = "https://example.com/about"
contact-email = "admin@example.com"

[output]
database-path = "./test.db"
summary-path = "./summary.md"

[[quality]]
domain = "example.com"
seeds = ["https://example.com/"]
"#;

        let file = create_temp_config(config_content);
        let config = load_config(file.path()).unwrap();

        assert_eq!(config.crawler.max_depth, 3);
        assert_eq!(config.crawler.max_concurrent_pages_open, 10);
        assert_eq!(config.user_agent.crawler_name, "TestCrawler");
        assert_eq!(config.quality.len(), 1);
    }

    #[test]
    fn test_load_config_with_invalid_path() {
        let result = load_config(Path::new("/nonexistent/config.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_config_with_invalid_toml() {
        let config_content = "this is not valid TOML {{{";
        let file = create_temp_config(config_content);
        let result = load_config(file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_load_config_with_validation_error() {
        let config_content = r#"
[crawler]
max-depth = 3
max-concurrent-pages-open = 0
minimum-time-on-page = 1000
max-domain-requests = 500

[user-agent]
crawler-name = "TestCrawler"
crawler-version = "1.0"
contact-url = "https://example.com/about"
contact-email = "admin@example.com"

[output]
database-path = "./test.db"
summary-path = "./summary.md"
"#;

        let file = create_temp_config(config_content);
        let result = load_config(file.path());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::Validation(_)));
    }

    #[test]
    fn test_compute_config_hash() {
        let config_content = "test content";
        let file = create_temp_config(config_content);

        let hash1 = compute_config_hash(file.path()).unwrap();
        let hash2 = compute_config_hash(file.path()).unwrap();

        // Same content should produce same hash
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA-256 produces 64 hex characters
    }

    #[test]
    fn test_different_content_different_hash() {
        let file1 = create_temp_config("content 1");
        let file2 = create_temp_config("content 2");

        let hash1 = compute_config_hash(file1.path()).unwrap();
        let hash2 = compute_config_hash(file2.path()).unwrap();

        assert_ne!(hash1, hash2);
    }
}
