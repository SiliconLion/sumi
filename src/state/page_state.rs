/// Page state definitions for tracking crawl progress
///
/// This module defines all possible states a page can be in during the crawl process.
use std::fmt;

/// Represents the current state of a page in the crawl process
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PageState {
    // ===== Active States =====
    /// Page has been discovered but not yet queued for fetching
    Discovered,

    /// Page is queued and waiting to be fetched
    Queued,

    /// Page is currently being fetched
    Fetching,

    // ===== Terminal Success States =====
    /// Page was successfully fetched and processed
    Processed,

    // ===== Terminal Skip States =====
    /// Page is on a blacklisted domain - recorded but skipped
    Blacklisted,

    /// Page is on a stubbed domain - noted but never visited
    Stubbed,

    // ===== Terminal Error States =====
    /// Page returned HTTP 404 or similar (permanent failure)
    DeadLink,

    /// Page could not be reached (connection refused, DNS failure, TLS error)
    Unreachable,

    /// Page returned HTTP 429 or domain is rate limited
    RateLimited,

    /// Page fetch failed for other reasons (redirect loop, parse error, etc.)
    Failed,

    // ===== Special States =====
    /// Page exceeds the maximum crawl depth
    DepthExceeded,

    /// Domain has hit the maximum request limit
    RequestLimitHit,

    /// Page Content-Type is not HTML
    ContentMismatch,
}

impl PageState {
    /// Returns true if this is a terminal state (no further processing needed)
    ///
    /// Active states (Discovered, Queued, Fetching) are not terminal.
    /// All other states are terminal.
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Self::Discovered | Self::Queued | Self::Fetching)
    }

    /// Returns true if this is an active state (page may still be processed)
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Discovered | Self::Queued | Self::Fetching)
    }

    /// Returns true if this represents a successful completion
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Processed)
    }

    /// Returns true if this represents a skip state (blacklist/stub)
    pub fn is_skipped(&self) -> bool {
        matches!(self, Self::Blacklisted | Self::Stubbed)
    }

    /// Returns true if this represents an error state
    pub fn is_error(&self) -> bool {
        matches!(
            self,
            Self::DeadLink
                | Self::Unreachable
                | Self::RateLimited
                | Self::Failed
                | Self::DepthExceeded
                | Self::RequestLimitHit
                | Self::ContentMismatch
        )
    }

    /// Converts the page state to a database string representation
    ///
    /// This is used for storing the state in the SQLite database.
    pub fn to_db_string(&self) -> &'static str {
        match self {
            Self::Discovered => "discovered",
            Self::Queued => "queued",
            Self::Fetching => "fetching",
            Self::Processed => "processed",
            Self::Blacklisted => "blacklisted",
            Self::Stubbed => "stubbed",
            Self::DeadLink => "dead_link",
            Self::Unreachable => "unreachable",
            Self::RateLimited => "rate_limited",
            Self::Failed => "failed",
            Self::DepthExceeded => "depth_exceeded",
            Self::RequestLimitHit => "request_limit_hit",
            Self::ContentMismatch => "content_mismatch",
        }
    }

    /// Parses a page state from a database string representation
    ///
    /// Returns None if the string doesn't match any known state.
    pub fn from_db_string(s: &str) -> Option<Self> {
        match s {
            "discovered" => Some(Self::Discovered),
            "queued" => Some(Self::Queued),
            "fetching" => Some(Self::Fetching),
            "processed" => Some(Self::Processed),
            "blacklisted" => Some(Self::Blacklisted),
            "stubbed" => Some(Self::Stubbed),
            "dead_link" => Some(Self::DeadLink),
            "unreachable" => Some(Self::Unreachable),
            "rate_limited" => Some(Self::RateLimited),
            "failed" => Some(Self::Failed),
            "depth_exceeded" => Some(Self::DepthExceeded),
            "request_limit_hit" => Some(Self::RequestLimitHit),
            "content_mismatch" => Some(Self::ContentMismatch),
            _ => None,
        }
    }

    /// Returns all possible page states
    pub fn all_states() -> Vec<Self> {
        vec![
            Self::Discovered,
            Self::Queued,
            Self::Fetching,
            Self::Processed,
            Self::Blacklisted,
            Self::Stubbed,
            Self::DeadLink,
            Self::Unreachable,
            Self::RateLimited,
            Self::Failed,
            Self::DepthExceeded,
            Self::RequestLimitHit,
            Self::ContentMismatch,
        ]
    }
}

impl fmt::Display for PageState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_db_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_terminal() {
        // Active states are not terminal
        assert!(!PageState::Discovered.is_terminal());
        assert!(!PageState::Queued.is_terminal());
        assert!(!PageState::Fetching.is_terminal());

        // All other states are terminal
        assert!(PageState::Processed.is_terminal());
        assert!(PageState::Blacklisted.is_terminal());
        assert!(PageState::Stubbed.is_terminal());
        assert!(PageState::DeadLink.is_terminal());
        assert!(PageState::Unreachable.is_terminal());
        assert!(PageState::RateLimited.is_terminal());
        assert!(PageState::Failed.is_terminal());
        assert!(PageState::DepthExceeded.is_terminal());
        assert!(PageState::RequestLimitHit.is_terminal());
        assert!(PageState::ContentMismatch.is_terminal());
    }

    #[test]
    fn test_is_active() {
        assert!(PageState::Discovered.is_active());
        assert!(PageState::Queued.is_active());
        assert!(PageState::Fetching.is_active());

        assert!(!PageState::Processed.is_active());
        assert!(!PageState::Failed.is_active());
    }

    #[test]
    fn test_is_success() {
        assert!(PageState::Processed.is_success());

        assert!(!PageState::Discovered.is_success());
        assert!(!PageState::Failed.is_success());
        assert!(!PageState::Blacklisted.is_success());
    }

    #[test]
    fn test_is_skipped() {
        assert!(PageState::Blacklisted.is_skipped());
        assert!(PageState::Stubbed.is_skipped());

        assert!(!PageState::Processed.is_skipped());
        assert!(!PageState::Failed.is_skipped());
    }

    #[test]
    fn test_is_error() {
        assert!(PageState::DeadLink.is_error());
        assert!(PageState::Unreachable.is_error());
        assert!(PageState::RateLimited.is_error());
        assert!(PageState::Failed.is_error());
        assert!(PageState::DepthExceeded.is_error());
        assert!(PageState::RequestLimitHit.is_error());
        assert!(PageState::ContentMismatch.is_error());

        assert!(!PageState::Processed.is_error());
        assert!(!PageState::Blacklisted.is_error());
        assert!(!PageState::Discovered.is_error());
    }

    #[test]
    fn test_to_db_string() {
        assert_eq!(PageState::Discovered.to_db_string(), "discovered");
        assert_eq!(PageState::Queued.to_db_string(), "queued");
        assert_eq!(PageState::Fetching.to_db_string(), "fetching");
        assert_eq!(PageState::Processed.to_db_string(), "processed");
        assert_eq!(PageState::Blacklisted.to_db_string(), "blacklisted");
        assert_eq!(PageState::Stubbed.to_db_string(), "stubbed");
        assert_eq!(PageState::DeadLink.to_db_string(), "dead_link");
        assert_eq!(PageState::Unreachable.to_db_string(), "unreachable");
        assert_eq!(PageState::RateLimited.to_db_string(), "rate_limited");
        assert_eq!(PageState::Failed.to_db_string(), "failed");
        assert_eq!(PageState::DepthExceeded.to_db_string(), "depth_exceeded");
        assert_eq!(
            PageState::RequestLimitHit.to_db_string(),
            "request_limit_hit"
        );
        assert_eq!(
            PageState::ContentMismatch.to_db_string(),
            "content_mismatch"
        );
    }

    #[test]
    fn test_from_db_string() {
        assert_eq!(
            PageState::from_db_string("discovered"),
            Some(PageState::Discovered)
        );
        assert_eq!(PageState::from_db_string("queued"), Some(PageState::Queued));
        assert_eq!(
            PageState::from_db_string("fetching"),
            Some(PageState::Fetching)
        );
        assert_eq!(
            PageState::from_db_string("processed"),
            Some(PageState::Processed)
        );
        assert_eq!(
            PageState::from_db_string("blacklisted"),
            Some(PageState::Blacklisted)
        );
        assert_eq!(
            PageState::from_db_string("stubbed"),
            Some(PageState::Stubbed)
        );
        assert_eq!(
            PageState::from_db_string("dead_link"),
            Some(PageState::DeadLink)
        );
        assert_eq!(
            PageState::from_db_string("unreachable"),
            Some(PageState::Unreachable)
        );
        assert_eq!(
            PageState::from_db_string("rate_limited"),
            Some(PageState::RateLimited)
        );
        assert_eq!(PageState::from_db_string("failed"), Some(PageState::Failed));
        assert_eq!(
            PageState::from_db_string("depth_exceeded"),
            Some(PageState::DepthExceeded)
        );
        assert_eq!(
            PageState::from_db_string("request_limit_hit"),
            Some(PageState::RequestLimitHit)
        );
        assert_eq!(
            PageState::from_db_string("content_mismatch"),
            Some(PageState::ContentMismatch)
        );
        assert_eq!(PageState::from_db_string("invalid"), None);
    }

    #[test]
    fn test_roundtrip_db_string() {
        for state in PageState::all_states() {
            let db_str = state.to_db_string();
            let parsed = PageState::from_db_string(db_str);
            assert_eq!(Some(state), parsed, "Failed roundtrip for {:?}", state);
        }
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", PageState::Discovered), "discovered");
        assert_eq!(format!("{}", PageState::Processed), "processed");
        assert_eq!(format!("{}", PageState::DeadLink), "dead_link");
    }

    #[test]
    fn test_all_states_complete() {
        let all = PageState::all_states();
        assert_eq!(all.len(), 13);

        // Verify no duplicates
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(all[i], all[j], "Duplicate state found");
            }
        }
    }
}
