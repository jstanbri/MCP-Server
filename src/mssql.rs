use anyhow::{Context, Result};
use serde_json::{Map, Value};
use tiberius::{Client, Config, ColumnData, Query};
use tokio::net::TcpStream;
use tokio_util::compat::TokioAsyncWriteCompatExt;

use crate::config::MssqlConfig;

/// Default number of rows returned when the caller does not specify `max_rows`.
pub const DEFAULT_MAX_ROWS: u64 = 500;
/// Hard upper limit on rows to prevent runaway reads.
pub const HARD_MAX_ROWS: u64 = 10_000;

/// Open a new tiberius client from an ADO.NET connection string.
async fn connect(cfg: &MssqlConfig) -> Result<Client<tokio_util::compat::Compat<TcpStream>>> {
    let config = Config::from_ado_string(&cfg.connection_string)
        .context("Failed to parse MSSQL connection string")?;

    let tcp = TcpStream::connect(config.get_addr())
        .await
        .with_context(|| format!("Failed to connect to MSSQL at {}", config.get_addr()))?;

    tcp.set_nodelay(true)
        .context("Failed to set TCP_NODELAY on MSSQL connection")?;

    let client = Client::connect(config, tcp.compat_write())
        .await
        .context("MSSQL handshake/login failed")?;

    Ok(client)
}

/// Convert a `ColumnData` value to a `serde_json::Value`.
fn column_data_to_json(data: &ColumnData<'static>) -> Value {
    match data {
        ColumnData::U8(v) => v.map(Value::from).unwrap_or(Value::Null),
        ColumnData::I16(v) => v.map(Value::from).unwrap_or(Value::Null),
        ColumnData::I32(v) => v.map(Value::from).unwrap_or(Value::Null),
        ColumnData::I64(v) => v.map(Value::from).unwrap_or(Value::Null),
        ColumnData::F32(v) => v
            .map(|n| {
                serde_json::Number::from_f64(n as f64)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            })
            .unwrap_or(Value::Null),
        ColumnData::F64(v) => v
            .map(|n| {
                serde_json::Number::from_f64(n)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            })
            .unwrap_or(Value::Null),
        ColumnData::Bit(v) => v.map(Value::Bool).unwrap_or(Value::Null),
        ColumnData::String(v) => v
            .as_deref()
            .map(|s| Value::String(s.to_string()))
            .unwrap_or(Value::Null),
        ColumnData::Guid(v) => v
            .map(|g| Value::String(g.to_string()))
            .unwrap_or(Value::Null),
        ColumnData::Numeric(v) => v
            .map(|n| Value::String(n.to_string()))
            .unwrap_or(Value::Null),
        ColumnData::Binary(v) => v
            .as_deref()
            .map(|b| {
                Value::String(b.iter().map(|byte| format!("{byte:02x}")).collect::<String>())
            })
            .unwrap_or(Value::Null),
        // Temporal types: tiberius stores these as internal integer encodings and
        // does not expose a Display implementation.  We use the Debug representation
        // which includes the raw field values.  For human-readable output, cast to
        // varchar in your SQL: CONVERT(varchar, column, 127) for ISO 8601.
        ColumnData::DateTime(v) => v
            .map(|d| Value::String(format!("{d:?}")))
            .unwrap_or(Value::Null),
        ColumnData::SmallDateTime(v) => v
            .map(|d| Value::String(format!("{d:?}")))
            .unwrap_or(Value::Null),
        ColumnData::Time(v) => v
            .map(|t| Value::String(format!("{t:?}")))
            .unwrap_or(Value::Null),
        ColumnData::Date(v) => v
            .map(|d| Value::String(format!("{d:?}")))
            .unwrap_or(Value::Null),
        ColumnData::DateTime2(v) => v
            .map(|d| Value::String(format!("{d:?}")))
            .unwrap_or(Value::Null),
        ColumnData::DateTimeOffset(v) => v
            .map(|d| Value::String(format!("{d:?}")))
            .unwrap_or(Value::Null),
        ColumnData::Xml(v) => v
            .as_deref()
            .map(|x| Value::String(x.to_string()))
            .unwrap_or(Value::Null),
    }
}

/// List all user tables in the connected database.
///
/// Returns a JSON array of objects with `schema` and `table_name` fields.
pub async fn list_tables(cfg: &MssqlConfig) -> Result<Value> {
    let mut client = connect(cfg).await?;

    let rows = client
        .query(
            "SELECT TABLE_SCHEMA, TABLE_NAME \
             FROM INFORMATION_SCHEMA.TABLES \
             WHERE TABLE_TYPE = 'BASE TABLE' \
             ORDER BY TABLE_SCHEMA, TABLE_NAME",
            &[],
        )
        .await
        .context("Failed to query INFORMATION_SCHEMA.TABLES")?
        .into_first_result()
        .await
        .context("Failed to collect table list results")?;

    let tables: Vec<Value> = rows
        .iter()
        .map(|row| {
            let schema: &str = row.get("TABLE_SCHEMA").unwrap_or("");
            let name: &str = row.get("TABLE_NAME").unwrap_or("");
            serde_json::json!({ "schema": schema, "table_name": name })
        })
        .collect();

    Ok(Value::Array(tables))
}

/// Execute an arbitrary SQL query and return results as a JSON array of row objects.
///
/// `max_rows` caps the number of rows returned (default [`DEFAULT_MAX_ROWS`],
/// maximum [`HARD_MAX_ROWS`]).
///
/// # Security note
/// The `sql` parameter is passed directly to the database after being wrapped
/// in a `SELECT TOP … FROM (…)` subquery.  Callers are responsible for
/// ensuring the query is safe to execute against the target database.  The
/// database user configured via `MSSQL_CONNECTION_STRING` should use the
/// principle of least privilege (read-only where possible).
pub async fn execute_query(cfg: &MssqlConfig, sql: &str, max_rows: u64) -> Result<Value> {
    let max_rows = max_rows.min(HARD_MAX_ROWS);

    let mut client = connect(cfg).await?;

    // Wrap the caller-supplied query in a TOP to prevent reading millions of rows.
    let limited_sql = format!(
        "SELECT TOP ({max_rows}) * FROM ({sql}) AS __mcp_query__"
    );

    let rows = Query::new(limited_sql)
        .query(&mut client)
        .await
        .context("Failed to execute SQL query")?
        .into_first_result()
        .await
        .context("Failed to collect query results")?;

    let result: Vec<Value> = rows
        .iter()
        .map(|row| {
            let mut obj = Map::new();
            for (col, data) in row.cells() {
                obj.insert(col.name().to_string(), column_data_to_json(data));
            }
            Value::Object(obj)
        })
        .collect();

    Ok(Value::Array(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiberius::{ColumnData, numeric::Numeric};

    #[test]
    fn column_data_null_variants_become_json_null() {
        assert_eq!(column_data_to_json(&ColumnData::I32(None)), Value::Null);
        assert_eq!(column_data_to_json(&ColumnData::String(None)), Value::Null);
        assert_eq!(column_data_to_json(&ColumnData::Bit(None)), Value::Null);
    }

    #[test]
    fn column_data_scalar_variants_round_trip() {
        assert_eq!(
            column_data_to_json(&ColumnData::I32(Some(42))),
            Value::from(42_i32)
        );
        assert_eq!(
            column_data_to_json(&ColumnData::Bit(Some(true))),
            Value::Bool(true)
        );
        assert_eq!(
            column_data_to_json(&ColumnData::String(Some(
                std::borrow::Cow::Borrowed("hello")
            ))),
            Value::String("hello".to_string())
        );
    }

    #[test]
    fn column_data_numeric_becomes_string() {
        // Numeric implements Display; we just verify it does not panic.
        let n = Numeric::new_with_scale(12345, 2);
        let v = column_data_to_json(&ColumnData::Numeric(Some(n)));
        assert!(v.is_string(), "Numeric should become a JSON string");
    }
}
