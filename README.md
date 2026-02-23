# MCP-Server

[![GitHub release (latest by date)](https://img.shields.io/github/v/release/jstanbri/MCP-Server?include_prereleases&sort=date)](https://github.com/jstanbri/MCP-Server/releases)
[![CI](https://github.com/jstanbri/MCP-Server/actions/workflows/ci.yml/badge.svg)](https://github.com/jstanbri/MCP-Server/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

MCP-Server built for clients with Azure SQL / Cosmos datastores

## Overview

`azure-mcp-server` is a **Model Context Protocol (MCP)** server written in Rust
that gives AI assistants (Claude, Copilot, etc.) secure, structured access to
private Azure data stores:

| Data Store | Protocol |
|---|---|
| Azure SQL / MSSQL | TDS (via [tiberius](https://crates.io/crates/tiberius)) |
| Azure Cosmos DB (NoSQL API) | HTTPS (via [azure_data_cosmos](https://crates.io/crates/azure_data_cosmos)) |

The server communicates over **stdio** using the
[MCP JSON-RPC protocol](https://spec.modelcontextprotocol.io/) implemented by
the [rmcp](https://crates.io/crates/rmcp) Rust SDK from the
`modelcontextprotocol` project.

---

## Tools exposed

### Azure MSSQL

| Tool | Description |
|---|---|
| `mssql_list_tables` | List all user tables (`TABLE_SCHEMA`, `TABLE_NAME`) |
| `mssql_execute_query` | Execute an arbitrary SQL query; results capped at `max_rows` (default 500, max 10 000) |

### Azure Cosmos DB

| Tool | Description |
|---|---|
| `cosmos_list_databases` | List all databases in the account |
| `cosmos_list_containers` | List all containers in a database |
| `cosmos_query_items` | Run a Cosmos SQL-API query against a container |

---

## Configuration

All configuration is via environment variables.  At least one data store must
be configured.

### Azure MSSQL

| Variable | Required | Description |
|---|---|---|
| `MSSQL_CONNECTION_STRING` | Yes | ADO.NET connection string |

**Example connection strings:**

```
# SQL Server / Azure SQL (username + password)
server=tcp:myserver.database.windows.net,1433;database=mydb;user id=myuser;password=mypassword;encrypt=true;trustservercertificate=false

# Windows integrated auth (local / on-prem)
# ⚠️ TrustServerCertificate=true disables TLS certificate validation.
# Use only in development/testing environments, never in production.
server=tcp:localhost,1433;IntegratedSecurity=true;TrustServerCertificate=true

# Named instance
server=tcp:myserver\INSTANCE,1433;database=mydb;user id=myuser;password=mypassword
```

### Azure Cosmos DB

| Variable | Required | Description |
|---|---|---|
| `COSMOS_ENDPOINT` | Yes | Account endpoint, e.g. `https://myaccount.documents.azure.com:443/` |
| `COSMOS_KEY` | Yes | Primary or secondary account key |
| `COSMOS_DEFAULT_DATABASE` | No | Default database (used when the tool `database` param is omitted) |

---

## Building

```bash
# debug build
cargo build

# release build (recommended for production)
cargo build --release
```

The compiled binary is placed in `target/debug/azure-mcp-server` or
`target/release/azure-mcp-server`.

---

## Docker

### Build the image

```bash
docker build -t azure-mcp-server .
```

### Configure environment variables

Copy the provided sample file and fill in your values:

```bash
cp .env-sample .env
# edit .env with your connection strings / keys
```

> **Note:** `.env` is listed in `.gitignore` and will never be committed to the
> repository.  Use `.env-sample` as a reference template.

### Run the container

Pass the `.env` file to the container at runtime:

```bash
# Both data stores
docker run --rm -i --env-file .env azure-mcp-server

# MSSQL only (override individual variables)
docker run --rm -i \
  -e MSSQL_CONNECTION_STRING="server=tcp:myserver.database.windows.net,1433;..." \
  azure-mcp-server
```

The container reads from **stdin** and writes MCP JSON-RPC responses to
**stdout**, matching the stdio transport expected by MCP clients.

---

## Docker Compose

`docker-compose.yml` wraps the Docker build and environment variable loading
in a single command.  It reads all variables from `.env` (see
[Configure environment variables](#configure-environment-variables) above) and
includes a health check that verifies the binary is present and executable.

### Start with Docker Compose

```bash
# Build the image and start the service
docker compose up --build
```

The service keeps stdin open so it is ready to accept MCP JSON-RPC messages.
To run in the background add the `--detach` flag:

```bash
docker compose up --build --detach
docker compose down   # stop and remove containers
```

---

## Running

```bash
# Example: MSSQL only
MSSQL_CONNECTION_STRING="server=tcp:localhost,1433;..." \
  ./target/release/azure-mcp-server

# Example: Cosmos DB only
COSMOS_ENDPOINT="https://myaccount.documents.azure.com:443/" \
COSMOS_KEY="my_account_key==" \
COSMOS_DEFAULT_DATABASE="mydb" \
  ./target/release/azure-mcp-server

# Example: both
MSSQL_CONNECTION_STRING="..." \
COSMOS_ENDPOINT="..." \
COSMOS_KEY="..." \
  ./target/release/azure-mcp-server
```

Logs are written to **stderr** (so they don't interfere with the stdio MCP
transport).  Set `RUST_LOG=azure_mcp_server=debug` for verbose output.

---

## Connecting to an MCP client

This server uses the **stdio transport**: there is no URL or network port.
The MCP client launches the server as a child process and communicates with it
directly over the process's stdin / stdout.  The client configuration tells the
client *how to start* the server, not *where to reach* it.

### Native binary

If you built the binary locally with `cargo build --release`, point the client
at the compiled executable and supply your credentials as environment variables.

Example for **Claude Desktop**
(`~/Library/Application Support/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "azure-data": {
      "command": "/path/to/azure-mcp-server",
      "env": {
        "MSSQL_CONNECTION_STRING": "server=tcp:...",
        "COSMOS_ENDPOINT": "https://...",
        "COSMOS_KEY": "..."
      }
    }
  }
}
```

### Docker container

When you use the pre-built Docker image, the credentials come from your `.env`
file via `--env-file`.  You do **not** need to repeat them in the client
configuration.

1. Build the image (if you haven't already):

   ```bash
   docker build -t azure-mcp-server .
   ```

2. Make sure your `.env` file is populated (see
   [Configure environment variables](#configure-environment-variables)).

3. Add this block to your MCP client configuration (adjust the path to `.env`
   as needed):

   ```json
   {
     "mcpServers": {
       "azure-data": {
         "command": "docker",
         "args": [
           "run", "--rm", "-i",
           "--env-file", "/absolute/path/to/.env",
           "azure-mcp-server"
         ]
       }
     }
   }
   ```

   The `-i` flag keeps stdin open so the client can send JSON-RPC messages to
   the container.  `--rm` removes the container automatically when the client
   disconnects.

### Docker Compose

If you prefer to manage the image through Compose, use `docker compose run`
instead of `docker run`:

```json
{
  "mcpServers": {
    "azure-data": {
      "command": "docker",
      "args": [
        "compose", "-f", "/absolute/path/to/docker-compose.yml",
        "run", "--rm", "azure-mcp-server"
      ]
    }
  }
}
```

Compose reads credentials from the `.env` file in the same directory as
`docker-compose.yml`, so no environment variables need to appear in the client
configuration.

---

## Development

```bash
# Run unit tests
cargo test

# Lint
cargo clippy

# Check formatting
cargo fmt --check
```

