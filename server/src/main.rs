//! Markify Server — REST API + MCP Server
//!
//! The MIT-licensed web data layer for AI agents.
//!
//! Usage:
//!   markify --server          # Start REST API server
//!   markify --mcp             # Start MCP server for Claude/Cursor
//!   markify --scrape <url>    # One-shot scrape from CLI
//!   markify --version         # Show version

use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod mcp;
mod rest;

/// Markify — The web data layer for AI agents
#[derive(Parser)]
#[command(name = "markify", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Bind address (default: 0.0.0.0:3000 — all IPv4 interfaces). Connect with
    /// `127.0.0.1` rather than `localhost`: on macOS `localhost` resolves to
    /// IPv6 `::1` first, which this IPv4 bind does not serve.
    #[arg(long, default_value = "0.0.0.0:3000")]
    bind: String,

    /// Log level (default: info)
    #[arg(long, default_value = "info")]
    log: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the REST API server
    Server {
        /// Enable MCP mode
        #[arg(long)]
        mcp: bool,
    },
    /// Start MCP server for Claude/Cursor/Windsurf
    Mcp,
    /// Scrape a single URL from CLI
    Scrape {
        /// URL to scrape
        url: String,
        /// Output format: markdown, json, both
        #[arg(long, default_value = "markdown")]
        format: String,
        /// Extraction mode: article, full, smart
        #[arg(long, default_value = "smart")]
        mode: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,markify_core=debug")),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Server { mcp }) => {
            start_server(&cli.bind, mcp).await?;
        }
        Some(Commands::Mcp) => {
            start_mcp().await?;
        }
        Some(Commands::Scrape { url, format, mode }) => {
            cli_scrape(&url, &format, &mode).await?;
        }
        None => {
            // Default: start the REST server
            start_server(&cli.bind, false).await?;
        }
    }

    Ok(())
}

/// Start the REST API server
async fn start_server(bind: &str, _mcp: bool) -> anyhow::Result<()> {
    info!("Starting Markify server on {}", bind);

    let app = rest::create_router();

    let listener = tokio::net::TcpListener::bind(bind).await?;
    // Advertise a URL that's actually reachable. A wildcard bind (0.0.0.0 / [::])
    // isn't connectable, and on macOS `localhost` resolves to IPv6 ::1 first --
    // which an IPv4 bind doesn't serve -- so point users at 127.0.0.1.
    let (host, port) = bind.rsplit_once(':').unwrap_or((bind, "3000"));
    let url_host = if matches!(host, "0.0.0.0" | "[::]" | "::" | "") {
        "127.0.0.1"
    } else {
        host
    };
    info!("Markify server running on http://{}:{}", url_host, port);
    info!("Health check:   http://{}:{}/health", url_host, port);

    axum::serve(listener, app).await?;

    Ok(())
}

/// Start MCP server
async fn start_mcp() -> anyhow::Result<()> {
    mcp::start_mcp_server().await
}

/// CLI scrape command
async fn cli_scrape(url: &str, format: &str, mode: &str) -> anyhow::Result<()> {
    use markify_core::{
        CacheConfig, ExtractionMode, FetchConfig, Markify, OutputFormat, ScrapeRequest,
    };

    let output_format = match format {
        "json" => OutputFormat::Json,
        "both" => OutputFormat::Both,
        _ => OutputFormat::Markdown,
    };

    let extraction_mode = match mode {
        "article" => ExtractionMode::Article,
        "full" => ExtractionMode::Full,
        "links" => ExtractionMode::Links,
        "metadata" => ExtractionMode::Metadata,
        _ => ExtractionMode::Smart,
    };

    let client = Markify::new(FetchConfig::default(), CacheConfig::default());

    let result = client
        .scrape(ScrapeRequest {
            url: url.to_string(),
            formats: vec![output_format.clone()],
            mode: extraction_mode,
            ..Default::default()
        })
        .await?;

    match output_format {
        OutputFormat::Markdown | OutputFormat::Both => {
            if let Some(md) = &result.0.markdown {
                println!("{}", md);
            }
        }
        OutputFormat::Json => {
            if let Some(json) = &result.0.json_content {
                println!("{}", serde_json::to_string_pretty(json)?);
            }
        }
    }

    Ok(())
}
