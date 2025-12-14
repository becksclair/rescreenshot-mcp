//! screenshot-mcp: Cross-platform screenshot MCP server
//!
//! M1 Phase 9: MCP server with backend integration and screenshot capture tools

use std::sync::Arc;

use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};
use screenshot_mcp::{
    capture::create_default_backend, mcp::ScreenshotMcpServer, util::temp_files::TempFileManager,
};
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

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
        // MCP uses stdout for JSON-RPC frames. Any non-protocol bytes on stdout
        // (like human logs or ANSI color codes) will corrupt the stream and
        // cause clients to fail parsing JSON. Keep logs on stderr.
        .with_writer(std::io::stderr)
        // Some environments force ANSI styling even for non-TTY outputs; make
        // absolutely sure we never emit escape codes that could get mixed into
        // client parsing.
        .with_ansi(false)
        .with_target(false)
        .with_thread_ids(false)
        .with_line_number(false)
        .init();

    info!("screenshot-mcp server starting...");
    info!("Protocol: Model Context Protocol (MCP)");
    info!("Transport: stdio");

    // Initialize backend (auto-detected by platform/session)
    let backend = create_default_backend()?;
    info!("Backend initialized (auto-detected)");

    // Initialize temp file manager
    let temp_files = Arc::new(TempFileManager::new());
    info!("Temp file manager initialized");

    // Create the MCP server with backend and temp file manager
    let server = ScreenshotMcpServer::new(backend, temp_files);

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
