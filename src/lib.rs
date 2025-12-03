//! Sumi-Ripple: A polite web terrain mapper
//!
//! This crate implements a web crawler that maps link relationships between websites,
//! respecting robots.txt, rate limits, and domain classifications.

pub mod config;
pub mod crawler;
pub mod output;
pub mod robots;
pub mod state;
pub mod storage;
pub mod url;

use thiserror::Error;

/// Main error type for Sumi-Ripple operations
#[derive(Debug, Error)]
pub enum SumiError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("HTTP error for {url}: {source}")]
    Http { url: String, source: reqwest::Error },

    #[error("Request timeout for {url}")]
    Timeout { url: String },

    #[error("Too many redirects from {url}")]
    RedirectLimit { url: String },

    #[error("Redirect loop detected at {url}")]
    RedirectLoop { url: String },

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Storage error: {0}")]
    StorageError(#[from] storage::StorageError),

    #[error("URL error: {0}")]
    UrlError(#[from] UrlError),

    #[error("URL parse error: {0}")]
    UrlParse(#[from] ::url::ParseError),

    #[error("HTTP client error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("HTML parse error for {url}: {message}")]
    HtmlParse { url: String, message: String },

    #[error("URL disallowed by robots.txt: {url}")]
    RobotsDenied { url: String },

    #[error("Invalid state transition: {from:?} -> {to:?}")]
    InvalidTransition {
        from: state::PageState,
        to: state::PageState,
    },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Robots.txt error: {0}")]
    Robots(String),
}

/// Configuration-specific errors
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to parse TOML: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Invalid URL in config: {0}")]
    InvalidUrl(String),

    #[error("Invalid domain pattern: {0}")]
    InvalidPattern(String),
}

/// URL-specific errors
#[derive(Debug, Error)]
pub enum UrlError {
    #[error("Failed to parse URL: {0}")]
    Parse(String),

    #[error("Invalid URL scheme: {0}")]
    InvalidScheme(String),

    #[error("Missing domain in URL")]
    MissingDomain,

    #[error("Malformed URL: {0}")]
    Malformed(String),
}

/// Result type alias for Sumi-Ripple operations
pub type Result<T> = std::result::Result<T, SumiError>;

/// Result type alias for configuration operations
pub type ConfigResult<T> = std::result::Result<T, ConfigError>;

/// Result type alias for URL operations
pub type UrlResult<T> = std::result::Result<T, UrlError>;

// Re-export commonly used types
pub use config::Config;
pub use state::{DomainState, PageState};
pub use url::{classify_domain, extract_domain, normalize_url, DomainClassification};
