mod config;
mod cosmos;
mod mssql;
mod server;

use rmcp::transport::stdio;
use rmcp::ServiceExt;
use server::AzureMcpServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Write structured logs to stderr so stdout stays clean for MCP JSON-RPC.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_env("RUST_LOG")
                .add_directive("azure_mcp_server=info".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("Starting azure-mcp-server v{}", env!("CARGO_PKG_VERSION"));

    let config = config::Config::from_env()?;
    let server = AzureMcpServer::new(config);

    let transport = stdio();

    tracing::info!("MCP server listening on stdio");

    let running = server.serve(transport).await?;
    running.waiting().await?;

    Ok(())
}
