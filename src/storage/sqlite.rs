//! SQLite storage implementation
//!
//! This module provides a SQLite-based implementation of the Storage trait.

use crate::state::{CachedRobots, DomainState, PageState};
use crate::storage::schema::initialize_schema;
use crate::storage::traits::{Storage, StorageError, StorageResult};
use crate::storage::{DepthRecord, LinkRecord, PageRecord, RunRecord, RunStatus};
use crate::SumiError;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::Path;

/// SQLite storage backend
pub struct SqliteStorage {
    conn: Connection,
}

impl SqliteStorage {
    /// Creates a new SqliteStorage instance
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the SQLite database file
    ///
    /// # Returns
    ///
    /// * `Ok(SqliteStorage)` - Successfully opened/created database
    /// * `Err(SumiError)` - Failed to open database
    pub fn new(path: &Path) -> Result<Self, SumiError> {
        let conn = Connection::open(path)?;

        // Configure SQLite for better performance
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            PRAGMA temp_store = MEMORY;
            PRAGMA mmap_size = 268435456;
        ",
        )?;

        // Initialize schema
        initialize_schema(&conn)?;

        Ok(Self { conn })
    }

    /// Creates an in-memory database (for testing)
    #[cfg(test)]
    pub fn new_in_memory() -> Result<Self, SumiError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        initialize_schema(&conn)?;
        Ok(Self { conn })
    }
}

impl Storage for SqliteStorage {
    // ===== Run Management =====

    fn create_run(&mut self, config_hash: &str) -> StorageResult<i64> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO runs (started_at, config_hash, status) VALUES (?1, ?2, ?3)",
            params![now, config_hash, RunStatus::Running.to_db_string()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn get_run(&self, run_id: i64) -> StorageResult<RunRecord> {
        let mut stmt = self.conn.prepare(
            "SELECT id, started_at, finished_at, config_hash, status FROM runs WHERE id = ?1",
        )?;

        let run = stmt
            .query_row(params![run_id], |row| {
                Ok(RunRecord {
                    id: row.get(0)?,
                    started_at: row.get(1)?,
                    finished_at: row.get(2)?,
                    config_hash: row.get(3)?,
                    status: RunStatus::from_db_string(&row.get::<_, String>(4)?)
                        .unwrap_or(RunStatus::Running),
                })
            })
            .map_err(|_| StorageError::RunNotFound(run_id))?;

        Ok(run)
    }

    fn get_latest_run(&self) -> StorageResult<Option<RunRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, started_at, finished_at, config_hash, status FROM runs ORDER BY id DESC LIMIT 1",
        )?;

        let run = stmt
            .query_row([], |row| {
                Ok(RunRecord {
                    id: row.get(0)?,
                    started_at: row.get(1)?,
                    finished_at: row.get(2)?,
                    config_hash: row.get(3)?,
                    status: RunStatus::from_db_string(&row.get::<_, String>(4)?)
                        .unwrap_or(RunStatus::Running),
                })
            })
            .optional()?;

        Ok(run)
    }

    fn update_run_status(&mut self, run_id: i64, status: RunStatus) -> StorageResult<()> {
        self.conn.execute(
            "UPDATE runs SET status = ?1 WHERE id = ?2",
            params![status.to_db_string(), run_id],
        )?;
        Ok(())
    }

    fn complete_run(&mut self, run_id: i64) -> StorageResult<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE runs SET status = ?1, finished_at = ?2 WHERE id = ?3",
            params![RunStatus::Completed.to_db_string(), now, run_id],
        )?;
        Ok(())
    }

    // ===== Page Management =====

    fn insert_or_get_page(
        &mut self,
        url: &str,
        domain: &str,
        discovered_run: i64,
    ) -> StorageResult<i64> {
        // Try to get existing page
        let existing: Option<i64> = self
            .conn
            .query_row("SELECT id FROM pages WHERE url = ?1", params![url], |row| {
                row.get(0)
            })
            .optional()?;

        if let Some(id) = existing {
            return Ok(id);
        }

        // Insert new page
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO pages (url, domain, state, discovered_at, discovered_run) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![url, domain, PageState::Discovered.to_db_string(), now, discovered_run],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    fn get_page(&self, page_id: i64) -> StorageResult<PageRecord> {
        let mut stmt = self.conn.prepare(
            "SELECT id, url, domain, state, title, status_code, content_type, last_modified,
             visited_at, discovered_at, discovered_run, error_message, retry_count
             FROM pages WHERE id = ?1",
        )?;

        let page = stmt
            .query_row(params![page_id], |row| {
                Ok(PageRecord {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    domain: row.get(2)?,
                    state: PageState::from_db_string(&row.get::<_, String>(3)?)
                        .unwrap_or(PageState::Failed),
                    title: row.get(4)?,
                    status_code: row.get(5)?,
                    content_type: row.get(6)?,
                    last_modified: row.get(7)?,
                    visited_at: row.get(8)?,
                    discovered_at: row.get(9)?,
                    discovered_run: row.get(10)?,
                    error_message: row.get(11)?,
                    retry_count: row.get(12)?,
                })
            })
            .map_err(|_| StorageError::PageNotFound(format!("Page ID {}", page_id)))?;

        Ok(page)
    }

    fn get_page_by_url(&self, url: &str) -> StorageResult<Option<PageRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, url, domain, state, title, status_code, content_type, last_modified,
             visited_at, discovered_at, discovered_run, error_message, retry_count
             FROM pages WHERE url = ?1",
        )?;

        let page = stmt
            .query_row(params![url], |row| {
                Ok(PageRecord {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    domain: row.get(2)?,
                    state: PageState::from_db_string(&row.get::<_, String>(3)?)
                        .unwrap_or(PageState::Failed),
                    title: row.get(4)?,
                    status_code: row.get(5)?,
                    content_type: row.get(6)?,
                    last_modified: row.get(7)?,
                    visited_at: row.get(8)?,
                    discovered_at: row.get(9)?,
                    discovered_run: row.get(10)?,
                    error_message: row.get(11)?,
                    retry_count: row.get(12)?,
                })
            })
            .optional()?;

        Ok(page)
    }

    fn update_page_state(
        &mut self,
        page_id: i64,
        state: PageState,
        title: Option<&str>,
        status_code: Option<u16>,
        content_type: Option<&str>,
        error_message: Option<&str>,
    ) -> StorageResult<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE pages SET state = ?1, title = ?2, status_code = ?3, content_type = ?4,
             visited_at = ?5, error_message = ?6 WHERE id = ?7",
            params![
                state.to_db_string(),
                title,
                status_code,
                content_type,
                now,
                error_message,
                page_id
            ],
        )?;
        Ok(())
    }

    fn increment_retry_count(&mut self, page_id: i64) -> StorageResult<()> {
        self.conn.execute(
            "UPDATE pages SET retry_count = retry_count + 1 WHERE id = ?1",
            params![page_id],
        )?;
        Ok(())
    }

    fn get_pages_by_state(&self, state: PageState) -> StorageResult<Vec<PageRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, url, domain, state, title, status_code, content_type, last_modified,
             visited_at, discovered_at, discovered_run, error_message, retry_count
             FROM pages WHERE state = ?1",
        )?;

        let pages = stmt
            .query_map(params![state.to_db_string()], |row| {
                Ok(PageRecord {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    domain: row.get(2)?,
                    state: PageState::from_db_string(&row.get::<_, String>(3)?)
                        .unwrap_or(PageState::Failed),
                    title: row.get(4)?,
                    status_code: row.get(5)?,
                    content_type: row.get(6)?,
                    last_modified: row.get(7)?,
                    visited_at: row.get(8)?,
                    discovered_at: row.get(9)?,
                    discovered_run: row.get(10)?,
                    error_message: row.get(11)?,
                    retry_count: row.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(pages)
    }

    fn get_interrupted_pages(&self) -> StorageResult<Vec<PageRecord>> {
        self.get_pages_by_state(PageState::Fetching)
    }

    // ===== Depth Tracking =====

    fn upsert_depth(
        &mut self,
        page_id: i64,
        quality_origin: &str,
        depth: u32,
    ) -> StorageResult<()> {
        // Try to insert, on conflict keep the minimum depth
        self.conn.execute(
            "INSERT INTO page_depths (page_id, quality_origin, depth) VALUES (?1, ?2, ?3)
             ON CONFLICT(page_id, quality_origin) DO UPDATE SET depth = MIN(depth, excluded.depth)",
            params![page_id, quality_origin, depth],
        )?;
        Ok(())
    }

    fn get_depths(&self, page_id: i64) -> StorageResult<Vec<DepthRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT page_id, quality_origin, depth FROM page_depths WHERE page_id = ?1")?;

        let depths = stmt
            .query_map(params![page_id], |row| {
                Ok(DepthRecord {
                    page_id: row.get(0)?,
                    quality_origin: row.get(1)?,
                    depth: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(depths)
    }

    fn should_crawl(&self, page_id: i64, max_depth: u32) -> StorageResult<bool> {
        let min_depth: Option<u32> = self
            .conn
            .query_row(
                "SELECT MIN(depth) FROM page_depths WHERE page_id = ?1",
                params![page_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();

        Ok(min_depth.map(|d| d <= max_depth).unwrap_or(false))
    }

    // ===== Link Management =====

    fn insert_link(
        &mut self,
        from_page_id: i64,
        to_page_id: i64,
        run_id: i64,
    ) -> StorageResult<()> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO links (from_page_id, to_page_id, discovered_run) VALUES (?1, ?2, ?3)",
                params![from_page_id, to_page_id, run_id],
            )?;
        Ok(())
    }

    fn get_outgoing_links(&self, page_id: i64) -> StorageResult<Vec<LinkRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT from_page_id, to_page_id, discovered_run FROM links WHERE from_page_id = ?1",
        )?;

        let links = stmt
            .query_map(params![page_id], |row| {
                Ok(LinkRecord {
                    from_page_id: row.get(0)?,
                    to_page_id: row.get(1)?,
                    discovered_run: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(links)
    }

    fn get_incoming_links(&self, page_id: i64) -> StorageResult<Vec<LinkRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT from_page_id, to_page_id, discovered_run FROM links WHERE to_page_id = ?1",
        )?;

        let links = stmt
            .query_map(params![page_id], |row| {
                Ok(LinkRecord {
                    from_page_id: row.get(0)?,
                    to_page_id: row.get(1)?,
                    discovered_run: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(links)
    }

    fn count_links(&self) -> StorageResult<u64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM links", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    // ===== Frontier Management =====

    fn add_to_frontier(&mut self, page_id: i64, priority: u32) -> StorageResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO frontier (page_id, priority) VALUES (?1, ?2)",
            params![page_id, priority],
        )?;
        Ok(())
    }

    fn pop_from_frontier(&mut self) -> StorageResult<Option<i64>> {
        let page_id: Option<i64> = self
            .conn
            .query_row(
                "SELECT page_id FROM frontier ORDER BY priority ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(id) = page_id {
            self.conn
                .execute("DELETE FROM frontier WHERE page_id = ?1", params![id])?;
        }

        Ok(page_id)
    }

    fn load_frontier(&self) -> StorageResult<Vec<(i64, u32)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT page_id, priority FROM frontier ORDER BY priority ASC")?;

        let frontier = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(frontier)
    }

    fn clear_frontier(&mut self) -> StorageResult<()> {
        self.conn.execute("DELETE FROM frontier", [])?;
        Ok(())
    }

    // ===== Domain State Persistence =====

    fn load_domain_states(&self) -> StorageResult<HashMap<String, DomainState>> {
        let mut stmt = self.conn.prepare(
            "SELECT domain, request_count, rate_limited, robots_txt, robots_fetched_at, last_request_time
             FROM domain_states"
        )?;

        let mut states = HashMap::new();
        let rows = stmt.query_map([], |row| {
            let domain: String = row.get(0)?;
            let request_count: u32 = row.get(1)?;
            let rate_limited_int: i32 = row.get(2)?;
            let robots_txt: Option<String> = row.get(3)?;
            let robots_fetched_at: Option<String> = row.get(4)?;
            let _last_request_time: Option<String> = row.get(5)?;

            let robots = if let (Some(content), Some(fetched_str)) = (robots_txt, robots_fetched_at)
            {
                if let Ok(fetched_at) = fetched_str.parse::<DateTime<Utc>>() {
                    Some(CachedRobots {
                        content,
                        fetched_at,
                    })
                } else {
                    None
                }
            } else {
                None
            };

            let state = DomainState {
                request_count,
                last_request_time: None, // We don't persist Instant, will be set on first use
                rate_limited: rate_limited_int != 0,
                robots_txt: robots.clone(),
                robots_fetched_at: robots.as_ref().map(|r| r.fetched_at),
            };

            Ok((domain, state))
        })?;

        for row in rows {
            let (domain, state) = row?;
            states.insert(domain, state);
        }

        Ok(states)
    }

    fn save_domain_states(&mut self, states: &HashMap<String, DomainState>) -> StorageResult<()> {
        // Clear existing domain states
        self.conn.execute("DELETE FROM domain_states", [])?;

        // Insert all current states
        for (domain, state) in states {
            self.update_domain_state(domain, state)?;
        }

        Ok(())
    }

    fn update_domain_state(&mut self, domain: &str, state: &DomainState) -> StorageResult<()> {
        let rate_limited_int = if state.rate_limited { 1 } else { 0 };

        let (robots_txt, robots_fetched_at) = if let Some(robots) = &state.robots_txt {
            (
                Some(robots.content.clone()),
                Some(robots.fetched_at.to_rfc3339()),
            )
        } else {
            (None, None)
        };

        // Note: We don't persist last_request_time (Instant) as it's not serializable
        // It will be reset when domain state is loaded
        self.conn.execute(
            "INSERT OR REPLACE INTO domain_states
             (domain, request_count, rate_limited, robots_txt, robots_fetched_at, last_request_time)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
            params![
                domain,
                state.request_count,
                rate_limited_int,
                robots_txt,
                robots_fetched_at,
            ],
        )?;

        Ok(())
    }

    // ===== Blacklist/Stub Tracking =====

    fn record_blacklisted(&mut self, url: &str, referrer: &str, run_id: i64) -> StorageResult<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO blacklisted_urls (url, referrer, discovered_run, discovered_at) VALUES (?1, ?2, ?3, ?4)",
            params![url, referrer, run_id, now],
        )?;
        Ok(())
    }

    fn record_stubbed(&mut self, url: &str, referrer: &str, run_id: i64) -> StorageResult<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO stubbed_urls (url, referrer, discovered_run, discovered_at) VALUES (?1, ?2, ?3, ?4)",
            params![url, referrer, run_id, now],
        )?;
        Ok(())
    }

    fn get_blacklisted_urls(&self) -> StorageResult<Vec<(String, u32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT url, COUNT(*) as count FROM blacklisted_urls GROUP BY url ORDER BY count DESC",
        )?;

        let urls = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get::<_, i64>(1)? as u32)))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(urls)
    }

    fn get_stubbed_urls(&self) -> StorageResult<Vec<(String, u32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT url, COUNT(*) as count FROM stubbed_urls GROUP BY url ORDER BY count DESC",
        )?;

        let urls = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get::<_, i64>(1)? as u32)))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(urls)
    }

    // ===== Statistics =====

    fn count_pages_by_state(&self, state: PageState) -> StorageResult<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM pages WHERE state = ?1",
            params![state.to_db_string()],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    fn count_total_pages(&self) -> StorageResult<u64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM pages", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    fn count_unique_domains(&self) -> StorageResult<u64> {
        let count: i64 =
            self.conn
                .query_row("SELECT COUNT(DISTINCT domain) FROM pages", [], |row| {
                    row.get(0)
                })?;
        Ok(count as u64)
    }

    fn get_error_summary(&self) -> StorageResult<HashMap<PageState, u64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT state, COUNT(*) FROM pages GROUP BY state")?;

        let mut summary = HashMap::new();
        let rows = stmt.query_map([], |row| {
            let state_str: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((state_str, count))
        })?;

        for row in rows {
            let (state_str, count) = row?;
            if let Some(state) = PageState::from_db_string(&state_str) {
                if state.is_error() {
                    summary.insert(state, count as u64);
                }
            }
        }

        Ok(summary)
    }

    fn get_rate_limited_domains(&self) -> StorageResult<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT domain FROM pages WHERE state = ?1")?;

        let domains = stmt
            .query_map(params![PageState::RateLimited.to_db_string()], |row| {
                row.get(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(domains)
    }

    fn get_depth_breakdown(&self) -> StorageResult<HashMap<u32, usize>> {
        let query = "
            SELECT depth, COUNT(DISTINCT page_id) as count
            FROM page_depths
            GROUP BY depth
            ORDER BY depth
        ";

        let mut stmt = self.conn.prepare(query)?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, u32>(0)?, row.get::<_, usize>(1)?))
        })?;

        let mut breakdown = HashMap::new();
        for row in rows {
            let (depth, count) = row?;
            breakdown.insert(depth, count);
        }

        Ok(breakdown)
    }

    fn get_discovered_domains(&self) -> StorageResult<Vec<String>> {
        let query = "
            SELECT DISTINCT domain
            FROM pages
            ORDER BY domain
        ";

        let mut stmt = self.conn.prepare(query)?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;

        let mut domains = Vec::new();
        for row in rows {
            domains.push(row?);
        }

        Ok(domains)
    }
}

/// Initializes or opens a database at the given path
///
/// # Arguments
///
/// * `path` - Path to the SQLite database file
///
/// # Returns
///
/// * `Ok(Connection)` - Successfully opened/created database
/// * `Err(rusqlite::Error)` - Failed to open database
pub fn init_database(path: &Path) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(path)?;

    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;
    ",
    )?;

    initialize_schema(&conn)?;

    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_in_memory() {
        let storage = SqliteStorage::new_in_memory();
        assert!(storage.is_ok());
    }

    #[test]
    fn test_create_run() {
        let mut storage = SqliteStorage::new_in_memory().unwrap();
        let run_id = storage.create_run("test_hash").unwrap();
        assert!(run_id > 0);
    }

    #[test]
    fn test_insert_page() {
        let mut storage = SqliteStorage::new_in_memory().unwrap();
        let run_id = storage.create_run("test_hash").unwrap();
        let page_id = storage
            .insert_or_get_page("https://example.com/", "example.com", run_id)
            .unwrap();
        assert!(page_id > 0);
    }

    #[test]
    fn test_insert_duplicate_page() {
        let mut storage = SqliteStorage::new_in_memory().unwrap();
        let run_id = storage.create_run("test_hash").unwrap();

        let page_id1 = storage
            .insert_or_get_page("https://example.com/", "example.com", run_id)
            .unwrap();
        let page_id2 = storage
            .insert_or_get_page("https://example.com/", "example.com", run_id)
            .unwrap();

        assert_eq!(page_id1, page_id2);
    }

    #[test]
    fn test_update_page_state() {
        let mut storage = SqliteStorage::new_in_memory().unwrap();
        let run_id = storage.create_run("test_hash").unwrap();
        let page_id = storage
            .insert_or_get_page("https://example.com/", "example.com", run_id)
            .unwrap();

        storage
            .update_page_state(
                page_id,
                PageState::Processed,
                Some("Test Page"),
                Some(200),
                Some("text/html"),
                None,
            )
            .unwrap();

        let page = storage.get_page(page_id).unwrap();
        assert_eq!(page.state, PageState::Processed);
        assert_eq!(page.title, Some("Test Page".to_string()));
    }

    #[test]
    fn test_domain_state_persistence() {
        let mut storage = SqliteStorage::new_in_memory().unwrap();

        // Create a domain state
        let mut state = DomainState::new();
        state.request_count = 42;
        state.rate_limited = true;
        state.update_robots("User-agent: *\nDisallow: /admin".to_string());

        // Save it
        storage.update_domain_state("example.com", &state).unwrap();

        // Load it back
        let loaded_states = storage.load_domain_states().unwrap();
        assert_eq!(loaded_states.len(), 1);

        let loaded_state = loaded_states.get("example.com").unwrap();
        assert_eq!(loaded_state.request_count, 42);
        assert_eq!(loaded_state.rate_limited, true);
        assert!(loaded_state.robots_txt.is_some());
        assert_eq!(
            loaded_state.robots_txt.as_ref().unwrap().content,
            "User-agent: *\nDisallow: /admin"
        );
    }

    #[test]
    fn test_save_multiple_domain_states() {
        let mut storage = SqliteStorage::new_in_memory().unwrap();

        // Create multiple domain states
        let mut states = HashMap::new();

        let mut state1 = DomainState::new();
        state1.request_count = 10;
        states.insert("example.com".to_string(), state1);

        let mut state2 = DomainState::new();
        state2.request_count = 20;
        state2.rate_limited = true;
        states.insert("test.com".to_string(), state2);

        let mut state3 = DomainState::new();
        state3.request_count = 5;
        states.insert("demo.org".to_string(), state3);

        // Save all states
        storage.save_domain_states(&states).unwrap();

        // Load them back
        let loaded_states = storage.load_domain_states().unwrap();
        assert_eq!(loaded_states.len(), 3);

        assert_eq!(loaded_states.get("example.com").unwrap().request_count, 10);
        assert_eq!(loaded_states.get("test.com").unwrap().request_count, 20);
        assert!(loaded_states.get("test.com").unwrap().rate_limited);
        assert_eq!(loaded_states.get("demo.org").unwrap().request_count, 5);
    }

    #[test]
    fn test_update_domain_state_replaces_existing() {
        let mut storage = SqliteStorage::new_in_memory().unwrap();

        // Create and save initial state
        let mut state1 = DomainState::new();
        state1.request_count = 10;
        storage.update_domain_state("example.com", &state1).unwrap();

        // Update with new state
        let mut state2 = DomainState::new();
        state2.request_count = 20;
        state2.rate_limited = true;
        storage.update_domain_state("example.com", &state2).unwrap();

        // Load and verify only latest state exists
        let loaded_states = storage.load_domain_states().unwrap();
        assert_eq!(loaded_states.len(), 1);

        let loaded = loaded_states.get("example.com").unwrap();
        assert_eq!(loaded.request_count, 20);
        assert!(loaded.rate_limited);
    }
}
