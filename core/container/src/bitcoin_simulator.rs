use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub enum BitcoinSimulationMode {
    Mock,
    Testnet,
    Mainnet,
    None,
}

pub struct BitcoinSimulator {
    mode: BitcoinSimulationMode,
    rpc_url: Option<String>,
    mock_blockchain_info: Arc<RwLock<Value>>,
}

impl BitcoinSimulator {
    pub fn new(mode: BitcoinSimulationMode) -> Self {
        let mock_blockchain_info = json!({
            "chain": "main",
            "blocks": 800000,
            "headers": 800000,
            "bestblockhash": "0000000000000000000123456789abcdef0123456789abcdef0123456789abcdef",
            "difficulty": 50000000000.0,
            "mediantime": 1700000000,
            "verificationprogress": 1.0,
            "initialblockdownload": false,
            "chainwork": "0000000000000000000000000000000000000000000000000000000000000000",
            "size_on_disk": 500000000000i64,
            "pruned": false,
            "softforks": {},
            "warnings": ""
        });

        let rpc_url = match mode {
            BitcoinSimulationMode::Mock => None,
            BitcoinSimulationMode::Testnet => Some("http://localhost:18332".to_string()),
            BitcoinSimulationMode::Mainnet => Some("http://localhost:8332".to_string()),
            BitcoinSimulationMode::None => None,
        };

        Self {
            mode,
            rpc_url,
            mock_blockchain_info: Arc::new(RwLock::new(mock_blockchain_info)),
        }
    }

    pub fn is_bitcoin_available(&self) -> bool {
        match self.mode {
            BitcoinSimulationMode::Mock => true,
            BitcoinSimulationMode::Testnet | BitcoinSimulationMode::Mainnet => {
                // In real mode, we'd check if the container is running
                // For now, assume it's available if we have an RPC URL
                self.rpc_url.is_some()
            }
            BitcoinSimulationMode::None => false,
        }
    }

    pub fn get_bitcoin_rpc_url(&self) -> Option<String> {
        self.rpc_url.clone()
    }

    pub async fn simulate_rpc_call(&self, method: &str, params: &[Value]) -> Result<Value> {
        match self.mode {
            BitcoinSimulationMode::Mock => self.mock_rpc_call(method, params).await,
            BitcoinSimulationMode::Testnet | BitcoinSimulationMode::Mainnet => {
                // Make actual RPC call to Bitcoin node
                self.real_rpc_call(method, params).await
            }
            BitcoinSimulationMode::None => Err(anyhow::anyhow!("Bitcoin simulation is disabled")),
        }
    }

    async fn mock_rpc_call(&self, method: &str, _params: &[Value]) -> Result<Value> {
        match method {
            "getblockchaininfo" => {
                let info = self.mock_blockchain_info.read().await;
                Ok(info.clone())
            }
            "getnetworkinfo" => Ok(json!({
                "version": 260000,
                "subversion": "/Bitcoin Core:26.0.0/",
                "protocolversion": 70016,
                "localservices": "000000000000040d",
                "localservicesnames": ["NETWORK", "WITNESS", "NETWORK_LIMITED"],
                "connections": 8,
                "connections_in": 4,
                "connections_out": 4,
                "networkactive": true,
                "networks": [],
                "relayfee": 0.00001000,
                "incrementalfee": 0.00001000,
                "localaddresses": [],
                "warnings": ""
            })),
            "getwalletinfo" => Ok(json!({
                "walletname": "wallet.dat",
                "walletversion": 169900,
                "balance": 0.0,
                "unconfirmed_balance": 0.0,
                "immature_balance": 0.0,
                "txcount": 0,
                "keypoololdest": 1700000000,
                "keypoolsize": 1000,
                "keypoolsize_hd_internal": 1000,
                "paytxfee": 0.00000000,
                "hdseedid": "0000000000000000000000000000000000000000",
                "private_keys_enabled": true,
                "avoid_reuse": false,
                "scanning": false
            })),
            "getblockcount" => Ok(json!(800000)),
            "getblockhash" => Ok(json!(
                "0000000000000000000123456789abcdef0123456789abcdef0123456789abcdef"
            )),
            "getmempoolinfo" => Ok(json!({
                "loaded": true,
                "size": 100,
                "bytes": 100000,
                "usage": 200000,
                "total_fee": 0.00001000,
                "maxmempool": 300000000,
                "mempoolminfee": 0.00001000,
                "minrelaytxfee": 0.00001000
            })),
            "getpeerinfo" => Ok(json!([])),
            "getrawmempool" => Ok(json!([])),
            "estimatesmartfee" => Ok(json!({
                "feerate": 0.00001000,
                "blocks": 6
            })),
            _ => {
                // Default response for unknown methods
                Ok(json!(null))
            }
        }
    }

    async fn real_rpc_call(&self, method: &str, params: &[Value]) -> Result<Value> {
        let url = self
            .rpc_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No RPC URL configured"))?;

        let client = reqwest::Client::new();
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        });

        // TODO: Get RPC credentials from config/secrets
        let response = client
            .post(url)
            .json(&request_body)
            .send()
            .await
            .context("Failed to send RPC request")?;

        let response_json: Value = response
            .json::<Value>()
            .await
            .context("Failed to parse RPC response")?;

        if let Some(error) = response_json.get("error") {
            return Err(anyhow::anyhow!("Bitcoin RPC error: {}", error));
        }

        Ok(response_json.get("result").cloned().unwrap_or(Value::Null))
    }

    pub fn mode(&self) -> &BitcoinSimulationMode {
        &self.mode
    }
}

impl From<&str> for BitcoinSimulationMode {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "mock" => BitcoinSimulationMode::Mock,
            "testnet" => BitcoinSimulationMode::Testnet,
            "mainnet" => BitcoinSimulationMode::Mainnet,
            _ => BitcoinSimulationMode::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_bitcoin_available() {
        let simulator = BitcoinSimulator::new(BitcoinSimulationMode::Mock);
        assert!(simulator.is_bitcoin_available());
    }

    #[tokio::test]
    async fn test_mock_getblockchaininfo() {
        let simulator = BitcoinSimulator::new(BitcoinSimulationMode::Mock);
        let result = simulator
            .simulate_rpc_call("getblockchaininfo", &[])
            .await
            .unwrap();
        assert!(result.get("blocks").is_some());
    }

    #[tokio::test]
    async fn test_none_bitcoin_not_available() {
        let simulator = BitcoinSimulator::new(BitcoinSimulationMode::None);
        assert!(!simulator.is_bitcoin_available());
    }
}
