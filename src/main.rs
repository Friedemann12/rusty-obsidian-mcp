mod cli;
mod server;
mod types;

#[cfg(feature = "http")]
mod auth;

use clap::Parser;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::cli::ObsidianCli;
use crate::server::ObsidianServer;

#[derive(Parser)]
#[command(
    name = "rusty-obsidian-mcp",
    about = "MCP server for Obsidian, powered by the Obsidian CLI (v1.12+)",
    version
)]
struct Args {
    /// Start a local HTTP server instead of stdio
    #[arg(long)]
    http: bool,

    /// Start an ngrok tunnel. Optionally pass your stable domain (e.g., my-name.ngrok-free.app).
    /// Falls back to NGROK_DOMAIN env var. If neither is set, ngrok assigns a random URL.
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    tunnel: Option<String>,

    /// Port for HTTP/tunnel server (default: 8000)
    #[arg(short, long, default_value_t = 8000)]
    port: u16,

    /// Host to bind for HTTP server (default: 127.0.0.1)
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Obsidian vault name (overrides OBSIDIAN_VAULT env var)
    #[arg(short, long)]
    vault: Option<String>,

    /// API key for HTTP/tunnel authentication (overrides MCP_API_KEY env var).
    /// Auto-generated if not provided. Required for --http and --tunnel modes.
    #[arg(long)]
    api_key: Option<String>,

    /// Disable API key authentication (NOT recommended for tunnel mode!)
    #[arg(long)]
    no_auth: bool,

    /// Skip the CLI health check at startup
    #[arg(long)]
    skip_health_check: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(true),
        )
        .init();

    let args = Args::parse();

    let mut obsidian_cli = ObsidianCli::from_env()?;
    if let Some(vault) = args.vault {
        obsidian_cli = obsidian_cli.with_vault(vault);
    }

    let transport = if args.tunnel.is_some() {
        "tunnel"
    } else if args.http {
        "http"
    } else {
        "stdio"
    };

    tracing::info!(
        vault = obsidian_cli.vault_name(),
        transport,
        "Starting rusty-obsidian-mcp"
    );

    if !args.skip_health_check {
        obsidian_cli.startup_check().await?;
    }

    match transport {
        "stdio" => run_stdio(obsidian_cli).await,

        #[cfg(feature = "http")]
        "http" => {
            let api_key = if args.no_auth {
                None
            } else {
                let key = auth::ApiKey::resolve(args.api_key.as_deref());
                print_auth_info(&key.0, &format!("http://{}:{}/mcp", args.host, args.port));
                Some(key)
            };
            if args.no_auth {
                tracing::warn!("Auth is DISABLED -- endpoint is unprotected!");
                eprintln!(
                    "\n  MCP endpoint: http://{}:{}/mcp (NO AUTH)\n",
                    args.host, args.port
                );
            }
            run_http(obsidian_cli, &args.host, args.port, api_key).await
        }

        #[cfg(not(feature = "http"))]
        "http" => {
            anyhow::bail!(
                "HTTP transport not available. Rebuild with: cargo build --features http"
            );
        }

        #[cfg(feature = "tunnel")]
        "tunnel" => {
            let api_key = if args.no_auth {
                tracing::warn!("Auth is DISABLED on a public tunnel -- THIS IS DANGEROUS!");
                None
            } else {
                Some(auth::ApiKey::resolve(args.api_key.as_deref()))
            };
            let domain = args.tunnel.unwrap_or_default();
            run_tunnel(obsidian_cli, args.port, &domain, api_key).await
        }

        #[cfg(not(feature = "tunnel"))]
        "tunnel" => {
            anyhow::bail!(
                "Tunnel transport not available. Rebuild with: cargo build --features tunnel"
            );
        }

        _ => unreachable!(),
    }
}

#[cfg(feature = "http")]
fn print_auth_info(api_key: &str, endpoint: &str) {
    let auth_hint = "Authorization: Bearer <api_key>";
    let lines = [
        ("MCP endpoint", endpoint),
        ("API key     ", api_key),
        ("", ""),
        ("Auth header ", auth_hint),
    ];
    let w = lines
        .iter()
        .map(|(l, v)| {
            if v.is_empty() {
                0
            } else {
                l.len() + 3 + v.len()
            }
        })
        .max()
        .unwrap_or(40);

    let bar = "─".repeat(w + 3);
    eprintln!();
    eprintln!("  ┌{bar}┐");
    for (label, value) in &lines {
        if value.is_empty() {
            eprintln!("  │  {} │", " ".repeat(w));
        } else {
            let content = format!("{label} : {value}");
            let pad = w.saturating_sub(content.len());
            eprintln!("  │  {content}{} │", " ".repeat(pad));
        }
    }
    eprintln!("  └{bar}┘");
    eprintln!();
}

async fn run_stdio(cli: ObsidianCli) -> anyhow::Result<()> {
    use rmcp::{ServiceExt, transport::stdio};

    tracing::info!("MCP server running on stdio (stdin/stdout) -- no auth required");
    let service = ObsidianServer::new(cli)
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!("Server error: {:?}", e))?;
    service.waiting().await?;
    tracing::info!("Server shut down");
    Ok(())
}

#[cfg(feature = "http")]
async fn run_http(
    cli: ObsidianCli,
    host: &str,
    port: u16,
    api_key: Option<auth::ApiKey>,
) -> anyhow::Result<()> {
    use rmcp::transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    };

    let ct = tokio_util::sync::CancellationToken::new();
    let mcp_service = StreamableHttpService::new(
        {
            let cli = cli.clone();
            move || Ok(ObsidianServer::new(cli.clone()))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig {
            cancellation_token: ct.child_token(),
            ..Default::default()
        },
    );

    let router = if let Some(key) = api_key {
        use axum::middleware;
        axum::Router::new()
            .nest_service("/mcp", mcp_service)
            .layer(middleware::from_fn_with_state(
                key.clone(),
                auth::require_api_key,
            ))
            .with_state(key)
    } else {
        axum::Router::new().nest_service("/mcp", mcp_service)
    };

    let addr = format!("{}:{}", host, port);
    tracing::info!("MCP server listening on http://{}/mcp", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c().await.ok();
            tracing::info!("Shutting down...");
            ct.cancel();
        })
        .await?;

    Ok(())
}

#[cfg(feature = "tunnel")]
async fn run_tunnel(
    cli: ObsidianCli,
    port: u16,
    domain_arg: &str,
    api_key: Option<auth::ApiKey>,
) -> anyhow::Result<()> {
    use ngrok::config::ForwarderBuilder;
    use rmcp::transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    };
    use url::Url;

    let ct = tokio_util::sync::CancellationToken::new();
    let mcp_service = StreamableHttpService::new(
        {
            let cli = cli.clone();
            move || Ok(ObsidianServer::new(cli.clone()))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig {
            cancellation_token: ct.child_token(),
            ..Default::default()
        },
    );

    let router = if let Some(ref key) = api_key {
        use axum::middleware;
        axum::Router::new()
            .nest_service("/mcp", mcp_service)
            .layer(middleware::from_fn_with_state(
                key.clone(),
                auth::require_api_key,
            ))
            .with_state(key.clone())
    } else {
        axum::Router::new().nest_service("/mcp", mcp_service)
    };

    let local_addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&local_addr).await?;
    tracing::info!("Local HTTP server bound to {}", local_addr);

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            tracing::error!("HTTP server error: {:?}", e);
        }
    });

    let domain = if !domain_arg.is_empty() {
        Some(domain_arg.to_string())
    } else {
        std::env::var("NGROK_DOMAIN").ok()
    };

    tracing::info!("Connecting to ngrok...");

    let session = ngrok::Session::builder()
        .authtoken_from_env()
        .connect()
        .await?;

    let mut endpoint = session.http_endpoint();
    if let Some(ref d) = domain {
        endpoint.domain(d);
    }

    let _tunnel = endpoint
        .listen_and_forward(Url::parse(&format!("http://127.0.0.1:{}", port))?)
        .await?;

    let public_url = domain
        .as_ref()
        .map(|d| format!("https://{}/mcp", d))
        .unwrap_or_else(|| "(check ngrok dashboard for URL)/mcp".into());

    tracing::info!("ngrok tunnel active: {}", public_url);
    if let Some(ref key) = api_key {
        print_auth_info(&key.0, &public_url);
    } else {
        eprintln!("\n  MCP endpoint: {} (NO AUTH)\n", public_url);
    }

    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutting down...");
    ct.cancel();

    Ok(())
}
