use anyhow::{Context, Result};
use std::env;

/// Configuration for connecting to Azure SQL / MSSQL via an ADO.NET connection string.
///
/// Set the `MSSQL_CONNECTION_STRING` environment variable.  Example:
/// ```text
/// server=tcp:myserver.database.windows.net,1433;database=mydb;user id=myuser;password=mypassword;encrypt=true;trustservercertificate=false
/// ```
#[derive(Debug, Clone)]
pub struct MssqlConfig {
    pub connection_string: String,
}

/// Configuration for connecting to Azure Cosmos DB.
///
/// Required environment variables:
/// - `COSMOS_ENDPOINT` — e.g. `https://myaccount.documents.azure.com:443/`
/// - `COSMOS_KEY` — Primary or secondary account key (key-based auth).
///
/// Optional:
/// - `COSMOS_DEFAULT_DATABASE` — database name used when callers omit the `database`
///   parameter in tool calls.
#[derive(Debug, Clone)]
pub struct CosmosConfig {
    pub endpoint: String,
    pub key: Option<String>,
    pub default_database: Option<String>,
}

/// Top-level server configuration assembled from environment variables at startup.
#[derive(Debug, Clone)]
pub struct Config {
    pub mssql: Option<MssqlConfig>,
    pub cosmos: Option<CosmosConfig>,
}

impl Config {
    /// Build configuration from the current process environment.
    ///
    /// At least one of MSSQL or Cosmos must be configured; returns an error if
    /// neither is present.
    pub fn from_env() -> Result<Self> {
        let mssql = env::var("MSSQL_CONNECTION_STRING").ok().map(|conn| {
            tracing::info!("MSSQL connection string found — MSSQL tools will be available");
            MssqlConfig {
                connection_string: conn,
            }
        });

        let cosmos = env::var("COSMOS_ENDPOINT").ok().map(|endpoint| {
            let key = env::var("COSMOS_KEY").ok();
            let default_database = env::var("COSMOS_DEFAULT_DATABASE").ok();
            if key.is_some() {
                tracing::info!(
                    "Cosmos DB endpoint + account key found — Cosmos tools will be available"
                );
            } else {
                tracing::warn!(
                    "COSMOS_ENDPOINT is set but COSMOS_KEY is missing — \
                     Cosmos DB tools will return an error until COSMOS_KEY is configured"
                );
            }
            CosmosConfig {
                endpoint,
                key,
                default_database,
            }
        });

        anyhow::ensure!(
            mssql.is_some() || cosmos.is_some(),
            "No data-store configuration found.  Set at least one of \
             MSSQL_CONNECTION_STRING or COSMOS_ENDPOINT."
        );

        Ok(Config { mssql, cosmos })
    }

    /// Convenience: return a reference to the MSSQL config or an error.
    pub fn require_mssql(&self) -> Result<&MssqlConfig> {
        self.mssql
            .as_ref()
            .context("MSSQL is not configured (MSSQL_CONNECTION_STRING not set)")
    }

    /// Convenience: return a reference to the Cosmos config or an error.
    pub fn require_cosmos(&self) -> Result<&CosmosConfig> {
        self.cosmos
            .as_ref()
            .context("Cosmos DB is not configured (COSMOS_ENDPOINT not set)")
    }
}
