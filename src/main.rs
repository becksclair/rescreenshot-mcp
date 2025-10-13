//! screenshot-mcp: Cross-platform screenshot MCP server
//!
//! M0: Basic MCP server with health_check tool for platform detection

use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};
use screenshot_mcp::mcp::ScreenshotMcpServer;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    // Respects RUST_LOG environment variable
    // Default level: info
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("screenshot_mcp=info")),
        )
        .with_target(false)
        .with_thread_ids(false)
        .with_line_number(false)
        .init();

    info!("screenshot-mcp server starting...");
    info!("Protocol: Model Context Protocol (MCP)");
    info!("Transport: stdio");

    // Create the MCP server
    let server = ScreenshotMcpServer::new();

    info!("Initializing stdio transport...");

    // Start the server with stdio transport
    // This will handle MCP protocol communication via stdin/stdout
    let service = server.serve(stdio()).await?;

    info!("screenshot-mcp server initialized successfully");
    info!("Server info: {:?}", service.peer_info());
    info!("Waiting for MCP requests...");

    // Wait for the service to complete (blocks until shutdown)
    service.waiting().await?;

    info!("screenshot-mcp server shutting down");
    Ok(())
}
