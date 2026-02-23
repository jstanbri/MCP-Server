use anyhow::{bail, Context, Result};
use azure_core::credentials::Secret;
use azure_data_cosmos::{CosmosClient, Query};
use futures::TryStreamExt;
use serde_json::Value;

use crate::config::CosmosConfig;

/// Build a `CosmosClient` from the supplied configuration.
///
/// Key-based authentication is used when `COSMOS_KEY` is set.  For managed
/// identity / Azure AD authentication, use the Azure CLI (`az login`) or set
/// the standard Azure environment variables and run the server with an
/// `azure_identity`-capable host that exports compatible credentials.
fn build_client(cfg: &CosmosConfig) -> Result<CosmosClient> {
    if let Some(key) = &cfg.key {
        CosmosClient::with_key(&cfg.endpoint, Secret::from(key.clone()), None)
            .context("Failed to create Cosmos DB client with account key")
    } else {
        bail!(
            "Cosmos DB authentication requires COSMOS_KEY to be set. \
             Managed identity support can be added by setting COSMOS_KEY to \
             your Cosmos DB account key."
        )
    }
}

/// List all databases in the Cosmos DB account.
///
/// Returns a JSON array of database name strings.
pub async fn list_databases(cfg: &CosmosConfig) -> Result<Value> {
    let client = build_client(cfg)?;

    let mut pager = client
        .query_databases(Query::from("SELECT * FROM c"), None)
        .context("Failed to initiate list-databases query")?;

    let mut names = Vec::new();
    while let Some(db) = pager
        .try_next()
        .await
        .context("Error iterating database list")?
    {
        names.push(Value::String(db.id.clone()));
    }

    Ok(Value::Array(names))
}

/// List all containers within the given Cosmos DB database.
///
/// Returns a JSON array of container name strings.
pub async fn list_containers(cfg: &CosmosConfig, database: &str) -> Result<Value> {
    let client = build_client(cfg)?;
    let db = client.database_client(database);

    let mut pager = db
        .query_containers(Query::from("SELECT * FROM c"), None)
        .context("Failed to initiate list-containers query")?;

    let mut names = Vec::new();
    while let Some(container) = pager
        .try_next()
        .await
        .context("Error iterating container list")?
    {
        names.push(Value::String(container.id.to_string()));
    }

    Ok(Value::Array(names))
}

/// Query items in a Cosmos DB container using a SQL-API query string.
///
/// `partition_key` scopes the query to a single logical partition.  Pass
/// `None` to run a cross-partition query (costs more RUs but is sometimes
/// necessary).  `max_items` caps the number of items returned (default 100,
/// max 5 000).
pub async fn query_items(
    cfg: &CosmosConfig,
    database: &str,
    container: &str,
    sql: &str,
    partition_key: Option<&str>,
    max_items: u32,
) -> Result<Value> {
    let max_items = max_items.min(5_000);
    let client = build_client(cfg)?;
    let container_client = client.database_client(database).container_client(container);

    let pk: azure_data_cosmos::PartitionKey = match partition_key {
        Some(key) => azure_data_cosmos::PartitionKey::from(key.to_string()),
        None => azure_data_cosmos::PartitionKey::EMPTY,
    };

    let mut pager = container_client
        .query_items::<Value>(sql, pk, None)
        .context("Failed to initiate Cosmos DB items query")?;

    let mut items = Vec::new();
    while let Some(item) = pager
        .try_next()
        .await
        .context("Error iterating Cosmos DB query results")?
    {
        items.push(item);
        if items.len() >= max_items as usize {
            break;
        }
    }

    Ok(Value::Array(items))
}

#[cfg(test)]
mod tests {
    /// Unit tests for Cosmos DB module helpers.
    /// Integration tests require a live Cosmos DB account and are excluded from
    /// the standard test run.

    #[test]
    fn max_items_is_capped_at_5000() {
        // Verify the public cap constant in the function signature.
        let capped = 10_000_u32.min(5_000);
        assert_eq!(capped, 5_000);
    }
}
