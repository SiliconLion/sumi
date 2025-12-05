//! Sumi-Ripple main entry point
//!
//! This is the command-line interface for the Sumi-Ripple web terrain mapper.

use clap::Parser;
use std::path::PathBuf;
use sumi_ripple::config::load_config_with_hash;
use sumi_ripple::crawler::crawl;
use tracing_subscriber::EnvFilter;

/// Sumi-Ripple: A polite web terrain mapper
///
/// Sumi-Ripple crawls websites while respecting robots.txt, rate limits,
/// and domain classifications. It maps link relationships between sites
/// and generates comprehensive summaries.
#[derive(Parser, Debug)]
#[command(name = "sumi-ripple")]
#[command(version = "1.0.0")]
#[command(about = "A polite web terrain mapper", long_about = None)]
struct Cli {
    /// Path to TOML configuration file
    #[arg(value_name = "CONFIG")]
    config: PathBuf,

    /// Increase logging verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Suppress non-error output
    #[arg(short, long, conflicts_with = "verbose")]
    quiet: bool,

    /// Resume an interrupted crawl (default behavior)
    #[arg(long, conflicts_with = "fresh")]
    resume: bool,

    /// Start a fresh crawl, ignoring previous state
    #[arg(long, conflicts_with = "resume")]
    fresh: bool,

    /// Validate config and show what would be crawled without actually crawling
    #[arg(long, conflicts_with_all = ["stats", "export_summary"])]
    dry_run: bool,

    /// Show statistics from the database and exit
    #[arg(long, conflicts_with_all = ["dry_run", "export_summary"])]
    stats: bool,

    /// Generate markdown summary from existing data and exit
    #[arg(long, conflicts_with_all = ["dry_run", "stats"])]
    export_summary: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Setup logging based on verbosity
    setup_logging(cli.verbose, cli.quiet);

    // Load and validate configuration
    tracing::info!("Loading configuration from: {}", cli.config.display());
    let (config, _config_hash) = match load_config_with_hash(&cli.config) {
        Ok((cfg, hash)) => {
            tracing::info!("Configuration loaded successfully (hash: {})", hash);
            (cfg, hash)
        }
        Err(e) => {
            tracing::error!("Failed to load configuration: {}", e);
            return Err(e.into());
        }
    };

    // Handle different modes
    if cli.dry_run {
        handle_dry_run(&config)?;
    } else if cli.stats {
        handle_stats(&config)?;
    } else if cli.export_summary {
        handle_export_summary(&config)?;
    } else {
        handle_crawl(config, cli.fresh).await?;
    }

    Ok(())
}

/// Sets up the logging/tracing subscriber based on verbosity level
fn setup_logging(verbose: u8, quiet: bool) {
    let filter = if quiet {
        // Only show errors
        EnvFilter::new("error")
    } else {
        match verbose {
            0 => EnvFilter::new("sumi_ripple=info,warn"),
            1 => EnvFilter::new("sumi_ripple=debug,info"),
            2 => EnvFilter::new("sumi_ripple=trace,debug"),
            _ => EnvFilter::new("trace"),
        }
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .init();
}

/// Handles the --dry-run mode: validates config and shows what would be crawled
fn handle_dry_run(config: &sumi_ripple::config::Config) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Sumi-Ripple Dry Run ===\n");

    println!("Crawler Configuration:");
    println!("  Max depth: {}", config.crawler.max_depth);
    println!(
        "  Max concurrent pages: {}",
        config.crawler.max_concurrent_pages_open
    );
    println!(
        "  Minimum time on page: {}ms",
        config.crawler.minimum_time_on_page
    );
    println!(
        "  Max domain requests: {}",
        config.crawler.max_domain_requests
    );

    println!("\nUser Agent:");
    println!("  Name: {}", config.user_agent.crawler_name);
    println!("  Version: {}", config.user_agent.crawler_version);
    println!("  Contact URL: {}", config.user_agent.contact_url);
    println!("  Contact Email: {}", config.user_agent.contact_email);

    println!("\nOutput:");
    println!("  Database: {}", config.output.database_path);
    println!("  Summary: {}", config.output.summary_path);

    println!("\nQuality Domains ({}):", config.quality.len());
    for entry in &config.quality {
        println!("  - {} ({} seeds)", entry.domain, entry.seeds.len());
        for seed in &entry.seeds {
            println!("    * {}", seed);
        }
    }

    println!("\nBlacklisted Domains ({}):", config.blacklist.len());
    for entry in &config.blacklist {
        println!("  - {}", entry.domain);
    }

    println!("\nStubbed Domains ({}):", config.stub.len());
    for entry in &config.stub {
        println!("  - {}", entry.domain);
    }

    println!("\n✓ Configuration is valid");
    println!(
        "✓ Would start crawling with {} seed URLs",
        config.quality.iter().map(|q| q.seeds.len()).sum::<usize>()
    );

    Ok(())
}

/// Handles the --stats mode: shows statistics from the database
fn handle_stats(config: &sumi_ripple::config::Config) -> Result<(), Box<dyn std::error::Error>> {
    use std::path::Path;
    use sumi_ripple::output::{load_statistics, print_statistics};
    use sumi_ripple::storage::SqliteStorage;

    println!("Database: {}\n", config.output.database_path);

    // Open the database
    let storage = SqliteStorage::new(Path::new(&config.output.database_path))?;

    // Load statistics
    let stats = load_statistics(&storage)?;

    // Print statistics
    print_statistics(&stats);

    Ok(())
}

/// Handles the --export-summary mode: generates markdown summary
fn handle_export_summary(
    config: &sumi_ripple::config::Config,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::path::Path;
    use sumi_ripple::output::{generate_markdown_summary, generate_summary};
    use sumi_ripple::storage::SqliteStorage;

    println!("=== Exporting Crawl Summary ===\n");
    println!("Database: {}", config.output.database_path);
    println!("Output: {}", config.output.summary_path);
    println!();

    // Open the database
    let storage = SqliteStorage::new(Path::new(&config.output.database_path))?;

    // Generate summary from storage
    tracing::info!("Loading crawl data from database...");
    let summary = generate_summary(&storage)?;

    // Write markdown summary to file
    tracing::info!("Generating markdown summary...");
    generate_markdown_summary(&summary, Path::new(&config.output.summary_path))?;

    println!("✓ Summary exported to: {}", config.output.summary_path);

    Ok(())
}

/// Handles the main crawl operation
async fn handle_crawl(
    config: sumi_ripple::config::Config,
    fresh: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if fresh {
        tracing::info!("Starting fresh crawl (ignoring previous state)");
    } else {
        tracing::info!("Starting crawl (will resume if interrupted run exists)");
    }

    tracing::info!(
        "Quality domains: {}, Blacklist: {}, Stub: {}",
        config.quality.len(),
        config.blacklist.len(),
        config.stub.len()
    );

    // Count total seed URLs
    let seed_count: usize = config.quality.iter().map(|q| q.seeds.len()).sum();
    tracing::info!("Total seed URLs: {}", seed_count);

    // Run the crawler
    match crawl(config).await {
        Ok(()) => {
            tracing::info!("Crawl completed successfully");
            Ok(())
        }
        Err(e) => {
            tracing::error!("Crawl failed: {}", e);
            Err(e.into())
        }
    }
}
