//! Storage module for persisting crawl data
//!
//! This module handles all database operations for the crawler, including:
//! - SQLite database initialization and schema management
//! - Page and domain state persistence
//! - Link relationship tracking
//! - Frontier queue management
//! - Run tracking and resumption support

mod schema;
mod sqlite;
mod traits;

pub use sqlite::{init_database, SqliteStorage};
pub use traits::{Storage, StorageError};

use crate::state::PageState;
use crate::SumiError;

use std::path::Path;

/// Initializes or opens a storage database
///
/// # Arguments
///
/// * `path` - Path to the SQLite database file
///
/// # Returns
///
/// * `Ok(SqliteStorage)` - Successfully initialized storage
/// * `Err(SumiError)` - Failed to initialize storage
pub fn open_storage(path: &Path) -> Result<SqliteStorage, SumiError> {
    SqliteStorage::new(path)
}

/// Represents a page in the database
#[derive(Debug, Clone)]
pub struct PageRecord {
    pub id: i64,
    pub url: String,
    pub domain: String,
    pub state: PageState,
    pub title: Option<String>,
    pub status_code: Option<u16>,
    pub content_type: Option<String>,
    pub last_modified: Option<String>,
    pub visited_at: Option<String>,
    pub discovered_at: String,
    pub discovered_run: i64,
    pub error_message: Option<String>,
    pub retry_count: u32,
}

/// Represents a depth record for a page from a quality origin
#[derive(Debug, Clone)]
pub struct DepthRecord {
    pub page_id: i64,
    pub quality_origin: String,
    pub depth: u32,
}

/// Represents a link relationship between pages
#[derive(Debug, Clone)]
pub struct LinkRecord {
    pub from_page_id: i64,
    pub to_page_id: i64,
    pub discovered_run: i64,
}

/// Represents a crawl run
#[derive(Debug, Clone)]
pub struct RunRecord {
    pub id: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub config_hash: String,
    pub status: RunStatus,
}

/// Status of a crawl run
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Running,
    Completed,
    Interrupted,
    Failed,
}

impl RunStatus {
    pub fn to_db_string(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Interrupted => "interrupted",
            Self::Failed => "failed",
        }
    }

    pub fn from_db_string(s: &str) -> Option<Self> {
        match s {
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "interrupted" => Some(Self::Interrupted),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_status_roundtrip() {
        for status in &[
            RunStatus::Running,
            RunStatus::Completed,
            RunStatus::Interrupted,
            RunStatus::Failed,
        ] {
            let db_str = status.to_db_string();
            let parsed = RunStatus::from_db_string(db_str);
            assert_eq!(Some(*status), parsed);
        }
    }

    #[test]
    fn test_run_status_invalid() {
        assert_eq!(RunStatus::from_db_string("invalid"), None);
    }
}
