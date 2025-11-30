# Sumi-Ripple

A polite web terrain mapper that crawls websites while respecting robots.txt, rate limits, and domain classifications.

## Overview

Sumi-Ripple is a Rust-based web crawler designed to map link relationships between websites. It crawls "quality" domains fully, records but skips "blacklisted" domains, and notes but never visits "stubbed" domains. The crawler respects rate limits, robots.txt directives, and persists state to survive crashes and interruptions.

## Features

- **Domain Classification**: Three-tier system for quality, blacklisted, and stubbed domains
- **Wildcard Support**: Use `*.example.com` patterns to match entire domain trees
- **Robots.txt Compliance**: Automatically fetches and respects robots.txt directives
- **Rate Limiting**: Configurable per-domain request limits and delays
- **Multi-Origin Depth Tracking**: Track crawl depth from multiple quality domain origins
- **State Persistence**: SQLite-based storage allows resuming interrupted crawls
- **Comprehensive Reporting**: Generate detailed markdown summaries of crawl results
- **Polite Crawling**: Respects HTTP 429 responses and implements configurable delays

## Installation

### Prerequisites

- Rust 1.70 or later
- Cargo (comes with Rust)

### Building from Source

```bash
git clone <repository-url>
cd sumi/sumi
cargo build --release
```

The compiled binary will be at `target/release/sumi-ripple`.

## Configuration

Sumi-Ripple uses TOML configuration files. See `examples/sample_config.toml` for a complete example.

### Configuration Structure

```toml
[crawler]
max-depth = 3                       # Maximum crawl depth from seeds
max-concurrent-pages-open = 10      # Concurrent page fetches
minimum-time-on-page = 1000         # Min delay between requests (ms)
max-domain-requests = 500           # Max requests per domain

[user-agent]
crawler-name = "SumiRipple"
crawler-version = "1.0"
contact-url = "https://example.com/about"
contact-email = "admin@example.com"

[output]
database-path = "./sumi-ripple.db"
summary-path = "./crawl-summary.md"

# Quality domains - fully crawled
[[quality]]
domain = "example.com"
seeds = ["https://example.com/", "https://example.com/blog/"]

# Wildcard quality domain
[[quality]]
domain = "*.example.org"
seeds = ["https://example.org/"]

# Blacklisted domains - recorded but not visited
[[blacklist]]
domain = "ads.example.net"

# Stubbed domains - noted but never visited
[[stub]]
domain = "github.com"
```

### Domain Classification Priority

Domains are classified in the following priority order:
1. **Blacklist** (highest priority)
2. **Stub**
3. **Quality**
4. **Discovered** (default)

## Usage

### Validate Configuration

```bash
sumi-ripple config.toml --dry-run
```

### Start a Fresh Crawl

```bash
sumi-ripple config.toml --fresh
```

### Resume an Interrupted Crawl

```bash
sumi-ripple config.toml --resume
```

Or simply:

```bash
sumi-ripple config.toml
```

(Resume is the default behavior)

### View Statistics

```bash
sumi-ripple config.toml --stats
```

### Export Summary

```bash
sumi-ripple config.toml --export-summary
```

### Logging Verbosity

```bash
# Normal output
sumi-ripple config.toml

# Verbose output
sumi-ripple config.toml -v

# Very verbose output
sumi-ripple config.toml -vv

# Trace-level output
sumi-ripple config.toml -vvv

# Quiet mode (errors only)
sumi-ripple config.toml --quiet
```

## Architecture

### Module Structure

```
sumi-ripple/
├── config/          # Configuration parsing and validation
├── url/             # URL normalization and domain classification
├── state/           # Page and domain state management
├── robots/          # Robots.txt fetching and caching
├── crawler/         # Core crawling logic
│   ├── coordinator  # Main crawl orchestration
│   ├── fetcher      # HTTP client and retry logic
│   ├── parser       # HTML parsing and link extraction
│   └── scheduler    # Frontier management and rate limiting
├── storage/         # SQLite persistence layer
└── output/          # Summary generation and reporting
```

### Key Concepts

#### URL Normalization

All URLs are normalized before processing:
- HTTP → HTTPS conversion
- Remove `www.` prefix
- Lowercase domain
- Remove tracking parameters (`utm_*`, `fbclid`, etc.)
- Sort query parameters
- Remove fragments
- Normalize paths (remove `.` and `..` segments)

#### Depth Tracking

Pages can have multiple depth values, one for each quality domain origin:
- Seed URLs start at depth 0
- All internal pages of a quality domain are depth 0
- Links from depth N pages create depth N+1 for discovered pages
- Pages are crawled if ANY depth value is ≤ max_depth

#### State Machine

Pages progress through these states:
- **Active**: Discovered → Queued → Fetching
- **Success**: Processed
- **Skip**: Blacklisted, Stubbed
- **Error**: DeadLink, Unreachable, RateLimited, Failed
- **Special**: DepthExceeded, RequestLimitHit, ContentMismatch

## Database Schema

Sumi-Ripple uses SQLite with the following key tables:

- `runs` - Crawl run metadata
- `pages` - All discovered URLs and their states
- `page_depths` - Multi-origin depth tracking
- `links` - Link relationships between pages
- `blacklisted_urls` - Recorded blacklisted URLs
- `stubbed_urls` - Recorded stubbed URLs
- `domain_states` - Per-domain crawl state
- `frontier` - Crawl queue

## Development Status

### Implemented

✅ Configuration parsing and validation  
✅ URL normalization and domain classification  
✅ Page and domain state management  
✅ Robots.txt parsing (basic)  
✅ SQLite storage layer with full schema  
✅ HTTP client with user agent configuration  
✅ HTML parsing and link extraction  
✅ Scheduler with rate limiting support  
✅ Markdown summary generation  
✅ CLI with multiple operation modes  

### In Progress

🔧 Full crawler coordinator implementation  
🔧 HTTP retry logic and redirect handling  
🔧 Robots.txt crawl delay support  
🔧 Domain state persistence  
🔧 Frontier queue management  
🔧 Multi-threaded crawling  

### Planned

📋 Resume functionality for interrupted crawls  
📋 Statistics dashboard  
📋 Export to additional formats (JSON, CSV)  
📋 Web UI for monitoring  
📋 Distributed crawling support  

## Testing

Run the test suite:

```bash
cargo test
```

Run tests with output:

```bash
cargo test -- --nocapture
```

Run specific test module:

```bash
cargo test url::tests
```

## Performance Considerations

- SQLite WAL mode for better concurrency
- Async I/O with Tokio runtime
- Connection pooling for HTTP requests
- Efficient frontier queue with priority sorting
- Memory-mapped database for large datasets

## Best Practices

1. **Start Small**: Begin with a low `max-concurrent-pages-open` (5-10)
2. **Respect Rate Limits**: Use at least 1000ms for `minimum-time-on-page`
3. **Monitor Progress**: Use `-v` flag to see crawl progress
4. **Backup Database**: SQLite database can be copied while crawler is running (WAL mode)
5. **Review robots.txt**: Check that your crawler respects domain policies
6. **Set Contact Info**: Always provide valid contact information in config

## Troubleshooting

### "Failed to load configuration"
- Verify TOML syntax with a validator
- Check that all required fields are present
- Ensure URLs use HTTPS scheme
- Verify domain patterns don't have typos

### "Database locked"
- Ensure only one crawler instance is running
- Check that database file has write permissions
- WAL mode should prevent most locking issues

### "Too many open files"
- Reduce `max-concurrent-pages-open`
- Check system ulimit settings

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass
5. Submit a pull request

## License

[License information to be added]

## Acknowledgments

- Built with [Tokio](https://tokio.rs/) for async runtime
- Uses [reqwest](https://docs.rs/reqwest/) for HTTP client
- HTML parsing with [scraper](https://docs.rs/scraper/)
- SQLite via [rusqlite](https://docs.rs/rusqlite/)
- Robots.txt parsing with [robotstxt](https://docs.rs/robotstxt/)

## Contact

For questions, issues, or suggestions, please open an issue on the repository.

---

**Note**: This is a development version. The crawler is functional but some advanced features are still being implemented. Use with caution on production websites.