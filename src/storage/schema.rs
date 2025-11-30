//! Database schema definitions and migrations
//!
//! This module contains all SQL schema definitions for the Sumi-Ripple database.

/// SQL schema for the database
pub const SCHEMA_SQL: &str = r#"
-- Track crawl runs
CREATE TABLE IF NOT EXISTS runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    config_hash TEXT NOT NULL,
    status TEXT NOT NULL
);

-- Track all discovered URLs
CREATE TABLE IF NOT EXISTS pages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    url TEXT NOT NULL UNIQUE,
    domain TEXT NOT NULL,
    state TEXT NOT NULL,
    title TEXT,
    status_code INTEGER,
    content_type TEXT,
    last_modified TEXT,
    visited_at TEXT,
    discovered_at TEXT NOT NULL,
    discovered_run INTEGER NOT NULL REFERENCES runs(id),
    error_message TEXT,
    retry_count INTEGER DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_pages_domain ON pages(domain);
CREATE INDEX IF NOT EXISTS idx_pages_state ON pages(state);
CREATE INDEX IF NOT EXISTS idx_pages_url ON pages(url);

-- Track depth from multiple quality origins
CREATE TABLE IF NOT EXISTS page_depths (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    page_id INTEGER NOT NULL REFERENCES pages(id),
    quality_origin TEXT NOT NULL,
    depth INTEGER NOT NULL,
    UNIQUE(page_id, quality_origin)
);

CREATE INDEX IF NOT EXISTS idx_page_depths_page ON page_depths(page_id);

-- Track link relationships
CREATE TABLE IF NOT EXISTS links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_page_id INTEGER NOT NULL REFERENCES pages(id),
    to_page_id INTEGER NOT NULL REFERENCES pages(id),
    discovered_run INTEGER NOT NULL REFERENCES runs(id),
    UNIQUE(from_page_id, to_page_id)
);

CREATE INDEX IF NOT EXISTS idx_links_from ON links(from_page_id);
CREATE INDEX IF NOT EXISTS idx_links_to ON links(to_page_id);

-- Track blacklisted URLs
CREATE TABLE IF NOT EXISTS blacklisted_urls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    url TEXT NOT NULL,
    referrer TEXT NOT NULL,
    discovered_run INTEGER NOT NULL REFERENCES runs(id),
    discovered_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_blacklisted_url ON blacklisted_urls(url);

-- Track blacklisted referrers (who linked to blacklisted domains)
CREATE TABLE IF NOT EXISTS blacklisted_referrers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    blacklisted_url TEXT NOT NULL,
    referrer_url TEXT NOT NULL,
    discovered_run INTEGER NOT NULL REFERENCES runs(id)
);

-- Track stubbed URLs
CREATE TABLE IF NOT EXISTS stubbed_urls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    url TEXT NOT NULL,
    referrer TEXT NOT NULL,
    discovered_run INTEGER NOT NULL REFERENCES runs(id),
    discovered_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_stubbed_url ON stubbed_urls(url);

-- Track stubbed referrers
CREATE TABLE IF NOT EXISTS stubbed_referrers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    stubbed_url TEXT NOT NULL,
    referrer_url TEXT NOT NULL,
    discovered_run INTEGER NOT NULL REFERENCES runs(id)
);

-- Persist domain states for resumption
CREATE TABLE IF NOT EXISTS domain_states (
    domain TEXT PRIMARY KEY,
    request_count INTEGER NOT NULL DEFAULT 0,
    rate_limited INTEGER NOT NULL DEFAULT 0,
    robots_txt TEXT,
    robots_fetched_at TEXT,
    last_request_time TEXT
);

-- Crawl frontier queue
CREATE TABLE IF NOT EXISTS frontier (
    page_id INTEGER PRIMARY KEY REFERENCES pages(id),
    priority INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_frontier_priority ON frontier(priority);
"#;

/// Initializes the database schema
///
/// # Arguments
///
/// * `conn` - The database connection
///
/// # Returns
///
/// * `Ok(())` - Schema initialized successfully
/// * `Err(rusqlite::Error)` - Failed to initialize schema
pub fn initialize_schema(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(SCHEMA_SQL)?;
    Ok(())
}

/// Gets the current schema version
///
/// This can be used for future migrations if the schema changes.
pub fn get_schema_version() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_schema_initializes() {
        let conn = Connection::open_in_memory().unwrap();
        let result = initialize_schema(&conn);
        assert!(result.is_ok());
    }

    #[test]
    fn test_schema_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();

        // Initialize twice
        initialize_schema(&conn).unwrap();
        let result = initialize_schema(&conn);

        // Should succeed the second time too
        assert!(result.is_ok());
    }

    #[test]
    fn test_tables_exist_after_init() {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();

        // Check that key tables exist
        let tables = vec![
            "runs",
            "pages",
            "page_depths",
            "links",
            "blacklisted_urls",
            "stubbed_urls",
            "domain_states",
            "frontier",
        ];

        for table in tables {
            let count: Result<i64, _> = conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{}'",
                    table
                ),
                [],
                |row| row.get(0),
            );
            assert!(count.is_ok());
            assert_eq!(count.unwrap(), 1, "Table {} should exist", table);
        }
    }
}
