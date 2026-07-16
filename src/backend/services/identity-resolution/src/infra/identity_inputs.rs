//! ClickHouse reader for `identity.identity_inputs` — the raw observation
//! stream that feeds the persons-seed. Concrete `IdentityInputsReader` over the
//! shared `insight-clickhouse` client. Query ported from the .NET
//! `ClickHouseIdentityInputsReader`.
//!
//! NOTE: not yet verified against a live ClickHouse — the query text / column
//! names / bind semantics mirror the .NET reader, but the env-gated test below
//! is the only runtime check and it skips without a ClickHouse.

#![allow(dead_code)]

use async_trait::async_trait;
use clickhouse::Row;
use insight_clickhouse::{Client, Config};
use sea_orm::prelude::DateTime;
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::seed::IdentityInputRow;
use crate::domain::seed_service::IdentityInputsReader;

/// Verbatim shape from `ClickHouseIdentityInputsReader`: rows ordered so the
/// FIRST per account is the latest (`_synced_at DESC`), which is exactly what
/// `build_profiles` expects. `insight_tenant_id` / `insight_source_id` are
/// stored as UUIDs, so they are `toString`-ed and reparsed. `is_delete` is
/// derived from `operation_type`.
const STREAM_SQL: &str = r"
    SELECT
        insight_source_type              AS source_type,
        toString(insight_source_id)      AS source_id,
        source_account_id,
        value_type,
        value,
        toString(_synced_at)             AS synced_at,
        operation_type
    FROM identity.identity_inputs
    WHERE insight_tenant_id = ?
      AND operation_type IN ('UPSERT', 'DELETE')
      AND value IS NOT NULL
      AND value != ''
    ORDER BY
        insight_source_type,
        insight_source_id,
        source_account_id,
        _synced_at DESC,
        value_type,
        value
";

#[derive(Debug, Row, Deserialize)]
struct InputRow {
    source_type: String,
    source_id: String,
    source_account_id: String,
    value_type: String,
    value: String,
    synced_at: String,
    operation_type: String,
}

/// Reads `identity_inputs` from ClickHouse via the shared client.
pub struct ClickHouseIdentityInputsReader {
    client: Client,
}

impl ClickHouseIdentityInputsReader {
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Build a reader from connection settings (empty user → no auth).
    #[must_use]
    pub fn connect(url: &str, database: &str, user: &str, password: &str) -> Self {
        let mut config = Config::new(url, database);
        if !user.is_empty() {
            config = config.with_auth(user, password);
        }
        Self::new(Client::new(config))
    }
}

#[async_trait]
impl IdentityInputsReader for ClickHouseIdentityInputsReader {
    async fn stream(&self, tenant_id: Uuid) -> anyhow::Result<Vec<IdentityInputRow>> {
        let rows: Vec<InputRow> = self
            .client
            .query(STREAM_SQL)
            .bind(tenant_id.to_string())
            .fetch_all()
            .await?;
        rows.into_iter().map(map_row).collect()
    }
}

fn map_row(r: InputRow) -> anyhow::Result<IdentityInputRow> {
    Ok(IdentityInputRow {
        source_type: r.source_type,
        source_id: Uuid::parse_str(&r.source_id)?,
        source_account_id: r.source_account_id,
        value_type: r.value_type,
        value: r.value,
        synced_at: parse_ch_datetime(&r.synced_at)?,
        is_delete: r.operation_type == "DELETE",
    })
}

/// Parse a `ClickHouse` `toString(DateTime[64])` value: `"2026-07-16 12:34:56"`
/// or `"…56.123456"`. Tries the fractional form first, then the plain form.
fn parse_ch_datetime(s: &str) -> anyhow::Result<DateTime> {
    DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f")
        .or_else(|_| DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .map_err(|e| anyhow::anyhow!("unparseable _synced_at '{s}': {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clickhouse_datetime_with_and_without_fraction() -> anyhow::Result<()> {
        let with_frac = parse_ch_datetime("2026-07-16 12:34:56.123456")?;
        let no_frac = parse_ch_datetime("2026-07-16 12:34:56")?;
        assert_eq!(with_frac.format("%Y-%m-%d %H:%M:%S").to_string(), "2026-07-16 12:34:56");
        assert_eq!(no_frac.format("%Y-%m-%d %H:%M:%S").to_string(), "2026-07-16 12:34:56");
        assert!(parse_ch_datetime("not-a-date").is_err());
        Ok(())
    }

    /// Live read against a dev ClickHouse. Set `IDENTITY_TEST_CH_URL`,
    /// `IDENTITY_TEST_CH_DB`, `IDENTITY_TEST_TENANT_ID` (+ optional
    /// `IDENTITY_TEST_CH_USER` / `IDENTITY_TEST_CH_PASSWORD`) and a port-forward
    /// to run; skips cleanly otherwise so CI stays green.
    #[tokio::test]
    async fn stream_against_dev_clickhouse() -> anyhow::Result<()> {
        let (Ok(url), Ok(db), Ok(tenant_raw)) = (
            std::env::var("IDENTITY_TEST_CH_URL"),
            std::env::var("IDENTITY_TEST_CH_DB"),
            std::env::var("IDENTITY_TEST_TENANT_ID"),
        ) else {
            eprintln!("skip: set IDENTITY_TEST_CH_URL + IDENTITY_TEST_CH_DB + IDENTITY_TEST_TENANT_ID to run");
            return Ok(());
        };
        let user = std::env::var("IDENTITY_TEST_CH_USER").unwrap_or_default();
        let password = std::env::var("IDENTITY_TEST_CH_PASSWORD").unwrap_or_default();
        let tenant = Uuid::parse_str(tenant_raw.trim())?;

        let reader = ClickHouseIdentityInputsReader::connect(&url, &db, &user, &password);
        let rows = reader.stream(tenant).await?;
        assert!(!rows.is_empty(), "dev tenant should have identity_inputs rows");
        Ok(())
    }
}
