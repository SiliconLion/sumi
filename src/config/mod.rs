//! Configuration module for Sumi-Ripple
//!
//! This module handles loading, parsing, and validating TOML configuration files.
//!
//! # Example
//!
//! ```no_run
//! use sumi_ripple::config::load_config;
//! use std::path::Path;
//!
//! let config = load_config(Path::new("config.toml")).unwrap();
//! println!("Crawler will use max depth: {}", config.crawler.max_depth);
//! ```

mod parser;
mod types;
mod validation;

// Re-export types
pub use types::{Config, CrawlerConfig, DomainEntry, OutputConfig, QualityEntry, UserAgentConfig};

// Re-export parser functions
pub use parser::{compute_config_hash, load_config, load_config_with_hash};
