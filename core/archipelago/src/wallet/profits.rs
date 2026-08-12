//! Networking profit tracking.
//!
//! Aggregates earnings from content sales (ecash) and Lightning routing fees.

use super::ecash;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

const PROFITS_FILE: &str = "wallet/profits.json";

/// Earnings breakdown by source.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfitsSummary {
    /// Total earnings in sats from all sources.
    pub total_sats: u64,
    /// Earnings from ecash content sales.
    pub content_sales_sats: u64,
    /// Earnings from Lightning routing fees.
    pub routing_fees_sats: u64,
    /// Earnings from streaming data payments.
    #[serde(default)]
    pub streaming_revenue_sats: u64,
    /// Recent earning entries (newest first).
    pub recent: Vec<ProfitEntry>,
}

/// A single profit event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfitEntry {
    pub source: ProfitSource,
    pub amount_sats: u64,
    pub timestamp: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfitSource {
    ContentSale,
    RoutingFee,
    StreamingRevenue,
}

/// Load profits summary from disk.
pub async fn load_profits(data_dir: &Path) -> Result<ProfitsSummary> {
    let path = data_dir.join(PROFITS_FILE);
    if !path.exists() {
        return Ok(ProfitsSummary::default());
    }
    let content = fs::read_to_string(&path)
        .await
        .context("Failed to read profits file")?;
    let summary: ProfitsSummary = serde_json::from_str(&content).unwrap_or_default();
    Ok(summary)
}

/// Save profits summary to disk.
#[allow(dead_code)]
pub async fn save_profits(data_dir: &Path, summary: &ProfitsSummary) -> Result<()> {
    let dir = data_dir.join("wallet");
    fs::create_dir_all(&dir)
        .await
        .context("Failed to create wallet directory")?;
    let path = data_dir.join(PROFITS_FILE);
    let content = serde_json::to_string_pretty(summary).context("Failed to serialize profits")?;
    fs::write(&path, content)
        .await
        .context("Failed to write profits file")?;
    Ok(())
}

/// Record a single content sale, updating totals and the recent entries list.
#[allow(dead_code)]
pub async fn record_content_sale(
    data_dir: &Path,
    amount_sats: u64,
    description: &str,
) -> Result<()> {
    let mut summary = load_profits(data_dir).await?;
    let entry = ProfitEntry {
        source: ProfitSource::ContentSale,
        amount_sats,
        timestamp: chrono::Utc::now().to_rfc3339(),
        description: description.to_string(),
    };
    summary.recent.insert(0, entry);
    if summary.recent.len() > 100 {
        summary.recent.truncate(100);
    }
    summary.content_sales_sats += amount_sats;
    summary.total_sats =
        summary.content_sales_sats + summary.routing_fees_sats + summary.streaming_revenue_sats;
    save_profits(data_dir, &summary).await?;
    Ok(())
}

/// Compute a full profits summary including ecash receive transactions.
pub async fn get_networking_profits(data_dir: &Path) -> Result<ProfitsSummary> {
    let mut summary = load_profits(data_dir).await?;

    // Count ecash transactions by type
    let wallet = ecash::load_wallet(data_dir).await?;

    let ecash_received: u64 = wallet
        .transactions
        .iter()
        .filter(|tx| matches!(tx.tx_type, ecash::TransactionType::Receive))
        .map(|tx| tx.amount_sats)
        .sum();

    let streaming_received: u64 = wallet
        .transactions
        .iter()
        .filter(|tx| matches!(tx.tx_type, ecash::TransactionType::StreamingRevenue))
        .map(|tx| tx.amount_sats)
        .sum();

    // Use the higher of tracked profits or ecash receives as content sales
    if ecash_received > summary.content_sales_sats {
        summary.content_sales_sats = ecash_received;
    }
    if streaming_received > summary.streaming_revenue_sats {
        summary.streaming_revenue_sats = streaming_received;
    }
    summary.total_sats =
        summary.content_sales_sats + summary.routing_fees_sats + summary.streaming_revenue_sats;

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_profits_summary_default() {
        let summary = ProfitsSummary::default();
        assert_eq!(summary.total_sats, 0);
        assert_eq!(summary.content_sales_sats, 0);
        assert_eq!(summary.routing_fees_sats, 0);
        assert!(summary.recent.is_empty());
    }

    #[tokio::test]
    async fn test_load_profits_returns_default_when_missing() {
        let tmp = TempDir::new().unwrap();
        let summary = load_profits(tmp.path()).await.unwrap();
        assert_eq!(summary.total_sats, 0);
        assert_eq!(summary.content_sales_sats, 0);
        assert_eq!(summary.routing_fees_sats, 0);
        assert!(summary.recent.is_empty());
    }

    #[tokio::test]
    async fn test_save_and_load_profits_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let summary = ProfitsSummary {
            total_sats: 5000,
            content_sales_sats: 3000,
            routing_fees_sats: 2000,
            streaming_revenue_sats: 0,
            recent: vec![ProfitEntry {
                source: ProfitSource::ContentSale,
                amount_sats: 3000,
                timestamp: "2025-06-01T00:00:00Z".to_string(),
                description: "Test sale".to_string(),
            }],
        };

        save_profits(tmp.path(), &summary).await.unwrap();
        let loaded = load_profits(tmp.path()).await.unwrap();
        assert_eq!(loaded.total_sats, 5000);
        assert_eq!(loaded.content_sales_sats, 3000);
        assert_eq!(loaded.routing_fees_sats, 2000);
        assert_eq!(loaded.recent.len(), 1);
        assert_eq!(loaded.recent[0].amount_sats, 3000);
    }

    #[tokio::test]
    async fn test_save_profits_creates_wallet_dir() {
        let tmp = TempDir::new().unwrap();
        let wallet_dir = tmp.path().join("wallet");
        assert!(!wallet_dir.exists());

        save_profits(tmp.path(), &ProfitsSummary::default())
            .await
            .unwrap();
        assert!(wallet_dir.exists());
    }

    #[tokio::test]
    async fn test_record_content_sale() {
        let tmp = TempDir::new().unwrap();
        record_content_sale(tmp.path(), 500, "First sale")
            .await
            .unwrap();

        let summary = load_profits(tmp.path()).await.unwrap();
        assert_eq!(summary.total_sats, 500);
        assert_eq!(summary.content_sales_sats, 500);
        assert_eq!(summary.routing_fees_sats, 0);
        assert_eq!(summary.recent.len(), 1);
        assert_eq!(summary.recent[0].amount_sats, 500);
        assert_eq!(summary.recent[0].description, "First sale");
        assert!(matches!(
            summary.recent[0].source,
            ProfitSource::ContentSale
        ));
    }

    #[tokio::test]
    async fn test_record_multiple_content_sales() {
        let tmp = TempDir::new().unwrap();
        record_content_sale(tmp.path(), 100, "Sale 1")
            .await
            .unwrap();
        record_content_sale(tmp.path(), 200, "Sale 2")
            .await
            .unwrap();
        record_content_sale(tmp.path(), 300, "Sale 3")
            .await
            .unwrap();

        let summary = load_profits(tmp.path()).await.unwrap();
        assert_eq!(summary.total_sats, 600);
        assert_eq!(summary.content_sales_sats, 600);
        assert_eq!(summary.recent.len(), 3);
        // Newest first (inserted at index 0)
        assert_eq!(summary.recent[0].description, "Sale 3");
        assert_eq!(summary.recent[1].description, "Sale 2");
        assert_eq!(summary.recent[2].description, "Sale 1");
    }

    #[tokio::test]
    async fn test_record_content_sale_truncates_at_100() {
        let tmp = TempDir::new().unwrap();
        for i in 0..110 {
            record_content_sale(tmp.path(), 1, &format!("Sale {}", i))
                .await
                .unwrap();
        }

        let summary = load_profits(tmp.path()).await.unwrap();
        assert_eq!(summary.recent.len(), 100);
        assert_eq!(summary.total_sats, 110);
    }

    #[tokio::test]
    async fn test_get_networking_profits_empty() {
        let tmp = TempDir::new().unwrap();
        let summary = get_networking_profits(tmp.path()).await.unwrap();
        assert_eq!(summary.total_sats, 0);
        assert_eq!(summary.content_sales_sats, 0);
        assert_eq!(summary.routing_fees_sats, 0);
    }

    #[tokio::test]
    async fn test_get_networking_profits_includes_ecash_receives() {
        let tmp = TempDir::new().unwrap();

        // Simulate receiving ecash tokens
        ecash::receive_token(tmp.path(), "cashuSend_500_uuid1_1700000000")
            .await
            .unwrap();
        ecash::receive_token(tmp.path(), "cashuSend_300_uuid2_1700000001")
            .await
            .unwrap();

        let summary = get_networking_profits(tmp.path()).await.unwrap();
        // ecash receives (800) should be reflected as content sales
        assert_eq!(summary.content_sales_sats, 800);
        assert_eq!(summary.total_sats, 800);
    }

    #[tokio::test]
    async fn test_get_networking_profits_uses_higher_of_tracked_or_ecash() {
        let tmp = TempDir::new().unwrap();

        // Record a larger tracked profit
        record_content_sale(tmp.path(), 2000, "Big sale")
            .await
            .unwrap();
        // Receive a smaller ecash amount
        ecash::receive_token(tmp.path(), "cashuSend_100_uuid_170")
            .await
            .unwrap();

        let summary = get_networking_profits(tmp.path()).await.unwrap();
        // Should use tracked (2000) since it's larger than ecash receives (100)
        assert_eq!(summary.content_sales_sats, 2000);
        assert_eq!(summary.total_sats, 2000);
    }

    #[test]
    fn test_profit_source_serialization() {
        let entry = ProfitEntry {
            source: ProfitSource::RoutingFee,
            amount_sats: 42,
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            description: "routing".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"routing_fee\""));

        let parsed: ProfitEntry = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed.source, ProfitSource::RoutingFee));
    }
}
