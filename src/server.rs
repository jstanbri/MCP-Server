use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ProtocolVersion, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;

use crate::config::Config;
use crate::{
    cosmos::{self, DEFAULT_MAX_ITEMS},
    mssql::{self, DEFAULT_MAX_ROWS},
};

// ---------------------------------------------------------------------------
// Tool parameter types
// ---------------------------------------------------------------------------

/// Parameters for `mssql_execute_query`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MssqlExecuteQueryParams {
    /// SQL query to execute.  Results are capped to `max_rows` rows.
    pub query: String,
    /// Maximum number of rows to return (default: 500, maximum: 10 000).
    pub max_rows: Option<u64>,
}

/// Parameters for `cosmos_list_containers`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CosmosListContainersParams {
    /// Cosmos DB database name.  If omitted the server falls back to
    /// `COSMOS_DEFAULT_DATABASE`.
    pub database: Option<String>,
}

/// Parameters for `cosmos_query_items`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CosmosQueryItemsParams {
    /// SQL-API query string, e.g. `"SELECT * FROM c WHERE c.active = true"`.
    pub query: String,
    /// Container to query.
    pub container: String,
    /// Cosmos DB database name.  Falls back to `COSMOS_DEFAULT_DATABASE` when
    /// omitted.
    pub database: Option<String>,
    /// Partition key value for single-partition queries.  Omit (or set to
    /// `null`) to issue a cross-partition query.
    pub partition_key: Option<String>,
    /// Maximum number of items to return (default: 100, maximum: 5 000).
    pub max_items: Option<u32>,
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

/// MCP server that exposes Azure MSSQL and Cosmos DB as tools.
#[derive(Clone)]
pub struct AzureMcpServer {
    config: Arc<Config>,
    tool_router: ToolRouter<Self>,
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AzureMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation {
                name: "azure-mcp-server".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: None,
                description: None,
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "This MCP server provides tools for querying Azure MSSQL and \
                 Azure Cosmos DB data stores.  Use the mssql_* tools for \
                 relational data and the cosmos_* tools for document data."
                    .into(),
            ),
        }
    }
}

#[tool_router]
impl AzureMcpServer {
    // ------------------------------------------------------------------
    // MSSQL tools
    // ------------------------------------------------------------------

    /// List all user tables in the Azure MSSQL database.
    ///
    /// Returns a JSON array of objects with `schema` and `table_name` fields.
    #[tool(description = "List all user tables in the Azure MSSQL database.")]
    async fn mssql_list_tables(&self) -> Result<String, String> {
        let cfg = self
            .config
            .require_mssql()
            .map_err(|e| e.to_string())?;

        mssql::list_tables(cfg)
            .await
            .map_err(|e| e.to_string())
            .map(|v| v.to_string())
    }

    /// Execute a SQL query against Azure MSSQL and return the results as JSON.
    ///
    /// Results are wrapped in a TOP clause to prevent runaway reads.
    #[tool(description = "Execute a SQL query against Azure MSSQL.  Results are \
                          returned as a JSON array of row objects.  Results are \
                          capped at max_rows (default 500, maximum 10 000).")]
    async fn mssql_execute_query(
        &self,
        Parameters(params): Parameters<MssqlExecuteQueryParams>,
    ) -> Result<String, String> {
        let cfg = self
            .config
            .require_mssql()
            .map_err(|e| e.to_string())?;

        let max_rows = params.max_rows.unwrap_or(DEFAULT_MAX_ROWS);

        mssql::execute_query(cfg, &params.query, max_rows)
            .await
            .map_err(|e| e.to_string())
            .map(|v| v.to_string())
    }

    // ------------------------------------------------------------------
    // Cosmos DB tools
    // ------------------------------------------------------------------

    /// List all databases in the Azure Cosmos DB account.
    ///
    /// Returns a JSON array of database name strings.
    #[tool(description = "List all databases in the Azure Cosmos DB account.")]
    async fn cosmos_list_databases(&self) -> Result<String, String> {
        let cfg = self
            .config
            .require_cosmos()
            .map_err(|e| e.to_string())?;

        cosmos::list_databases(cfg)
            .await
            .map_err(|e| e.to_string())
            .map(|v| v.to_string())
    }

    /// List all containers in an Azure Cosmos DB database.
    ///
    /// Returns a JSON array of container name strings.
    #[tool(description = "List all containers in an Azure Cosmos DB database.  \
                          `database` defaults to COSMOS_DEFAULT_DATABASE when omitted.")]
    async fn cosmos_list_containers(
        &self,
        Parameters(params): Parameters<CosmosListContainersParams>,
    ) -> Result<String, String> {
        let cfg = self
            .config
            .require_cosmos()
            .map_err(|e| e.to_string())?;

        let database = params
            .database
            .as_deref()
            .or(cfg.default_database.as_deref())
            .ok_or_else(|| {
                "database parameter is required when COSMOS_DEFAULT_DATABASE is not set"
                    .to_string()
            })?
            .to_string();

        cosmos::list_containers(cfg, &database)
            .await
            .map_err(|e| e.to_string())
            .map(|v| v.to_string())
    }

    /// Query items in an Azure Cosmos DB container using a SQL-API query.
    ///
    /// Returns a JSON array of matching document objects.
    #[tool(description = "Query items in an Azure Cosmos DB container using a \
                          Cosmos SQL-API query string.  Results are capped at \
                          max_items (default 100, maximum 5 000).")]
    async fn cosmos_query_items(
        &self,
        Parameters(params): Parameters<CosmosQueryItemsParams>,
    ) -> Result<String, String> {
        let cfg = self
            .config
            .require_cosmos()
            .map_err(|e| e.to_string())?;

        let database = params
            .database
            .as_deref()
            .or(cfg.default_database.as_deref())
            .ok_or_else(|| {
                "database parameter is required when COSMOS_DEFAULT_DATABASE is not set"
                    .to_string()
            })?
            .to_string();

        let max_items = params.max_items.unwrap_or(DEFAULT_MAX_ITEMS);

        cosmos::query_items(
            cfg,
            &database,
            &params.container,
            &params.query,
            params.partition_key.as_deref(),
            max_items,
        )
        .await
        .map_err(|e| e.to_string())
        .map(|v| v.to_string())
    }
}

impl AzureMcpServer {
    /// Create a new server instance.
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
            tool_router: Self::tool_router(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CosmosConfig, MssqlConfig};

    fn make_server_mssql_only() -> AzureMcpServer {
        AzureMcpServer::new(Config {
            mssql: Some(MssqlConfig {
                connection_string: "server=localhost;database=test".into(),
            }),
            cosmos: None,
        })
    }

    fn make_server_cosmos_only() -> AzureMcpServer {
        AzureMcpServer::new(Config {
            mssql: None,
            cosmos: Some(CosmosConfig {
                endpoint: "https://example.documents.azure.com:443/".into(),
                key: Some("dGVzdGtleQ==".into()),
                default_database: Some("mydb".into()),
            }),
        })
    }

    #[test]
    fn server_info_contains_correct_name() {
        let server = make_server_mssql_only();
        let info = server.get_info();
        assert_eq!(info.server_info.name, "azure-mcp-server");
    }

    #[test]
    fn server_info_has_tools_capability() {
        let server = make_server_cosmos_only();
        let info = server.get_info();
        assert!(
            info.capabilities.tools.is_some(),
            "tools capability must be present"
        );
    }

    #[test]
    fn tool_router_lists_expected_tools() {
        let server = make_server_mssql_only();
        let tools = server.tool_router.list_all();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

        assert!(names.contains(&"mssql_list_tables"), "mssql_list_tables missing");
        assert!(names.contains(&"mssql_execute_query"), "mssql_execute_query missing");
        assert!(names.contains(&"cosmos_list_databases"), "cosmos_list_databases missing");
        assert!(names.contains(&"cosmos_list_containers"), "cosmos_list_containers missing");
        assert!(names.contains(&"cosmos_query_items"), "cosmos_query_items missing");
    }
}
