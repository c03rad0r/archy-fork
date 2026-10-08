use super::LND_REST_BASE_URL;
use crate::{
    api::rpc::RpcHandler,
    content_lightning::{Binding, Invoice, InvoiceNode, Journal, Status},
};
use anyhow::{Context, Result};
use base64::Engine;
struct Node {
    client: reqwest::Client,
    macaroon: String,
}
fn number(v: &serde_json::Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str()?.parse().ok())
}
impl InvoiceNode for Node {
    async fn prepare_creation(&self) -> Result<()> {
        let info: serde_json::Value = self
            .client
            .get(format!("{LND_REST_BASE_URL}/v1/getinfo"))
            .header("Grpc-Metadata-macaroon", &self.macaroon)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        anyhow::ensure!(
            info["identity_pubkey"]
                .as_str()
                .is_some_and(|key| !key.is_empty()),
            "LND invoice service is not ready; original preparation retained"
        );
        Ok(())
    }
    async fn lookup(&self, hash: &str) -> Result<Option<Invoice>> {
        let response = self
            .client
            .get(format!("{LND_REST_BASE_URL}/v1/invoice/{hash}"))
            .header("Grpc-Metadata-macaroon", &self.macaroon)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let body: serde_json::Value = response.error_for_status()?.json().await?;
        let raw = body["r_hash"].as_str().context("Invoice hash omitted")?;
        let payment_hash = hex::encode(base64::engine::general_purpose::STANDARD.decode(raw)?);
        Ok(Some(Invoice {
            payment_hash,
            bolt11: body["payment_request"]
                .as_str()
                .context("Invoice payment request omitted")?
                .into(),
            price_sats: number(&body["value"]).context("Invoice amount omitted")?,
            state: body["state"]
                .as_str()
                .context("Invoice state omitted")?
                .into(),
            paid_sats: number(&body["amt_paid_sat"]),
            paid_msats: number(&body["amt_paid_msat"]),
        }))
    }
    async fn add(&self, binding: &Binding, preimage_hex: &str) -> Result<()> {
        let preimage = base64::engine::general_purpose::STANDARD.encode(hex::decode(preimage_hex)?);
        self.client.post(format!("{LND_REST_BASE_URL}/v1/invoices")).header("Grpc-Metadata-macaroon",&self.macaroon)
            .json(&serde_json::json!({"memo":format!("Archipelago peer file {}",binding.content_id),"value":binding.price_sats.to_string(),"r_preimage":preimage,"private":true,"expiry":"3600"})).send().await?.error_for_status()?;
        Ok(())
    }
    async fn cancel(&self, hash: &str) -> Result<()> {
        self.client.post(format!("{LND_REST_BASE_URL}/v2/invoices/cancel")).header("Grpc-Metadata-macaroon",&self.macaroon)
            .json(&serde_json::json!({"payment_hash":base64::engine::general_purpose::STANDARD.encode(hex::decode(hash)?)})).send().await?.error_for_status()?;
        Ok(())
    }
}
impl RpcHandler {
    pub(crate) async fn drive_external_invoice(
        &self,
        journal: &Journal,
        binding: &Binding,
        cancel: bool,
    ) -> Result<Status> {
        // Configuration/auth preflight before the engine persists dispatch.
        let (client, macaroon) = self.lnd_client().await?;
        crate::content_lightning::drive(journal, binding, &Node { client, macaroon }, cancel).await
    }
}

/// Prepared before the durable native-dispatch marker. Once execute is called,
/// every transport error is ambiguous and only original-hash lookup may follow.
pub(crate) struct PreparedNativePayment {
    client: reqwest::Client,
    request: reqwest::Request,
    hash: String,
    amount: u64,
}
impl PreparedNativePayment {
    pub(crate) async fn execute(self) -> Result<serde_json::Value> {
        let response = self
            .client
            .execute(self.request)
            .await
            .context("Native payment response is unknown; recover the original operation")?;
        let status = response.status();
        let body: serde_json::Value = response
            .json()
            .await
            .context("Native payment response is unknown")?;
        anyhow::ensure!(
            status.is_success(),
            "LND did not confirm the original payment; recover its status"
        );
        let payment = body.get("result").unwrap_or(&body);
        anyhow::ensure!(
            payment
                .get("payment_hash")
                .and_then(|v| v.as_str())
                .is_none_or(|hash| hash == self.hash),
            "LND payment hash changed"
        );
        Ok(super::payments::router_payment_outcome(
            payment,
            &self.hash,
            self.amount as i64,
        ))
    }
}
impl RpcHandler {
    pub(crate) async fn prepare_bound_invoice_payment(
        &self,
        bolt11: &str,
        hash: &str,
        amount: u64,
    ) -> Result<PreparedNativePayment> {
        let invoice: lightning_invoice::Bolt11Invoice =
            bolt11.parse().context("Invalid original invoice")?;
        invoice.check_signature()?;
        anyhow::ensure!(
            invoice.payment_hash().to_string() == hash
                && invoice.amount_milli_satoshis() == amount.checked_mul(1000),
            "Original invoice amount/hash changed"
        );
        anyhow::ensure!(
            !invoice.is_expired(),
            "Original invoice expired; cancel or recover it before choosing another method"
        );
        let (client, macaroon) = self.lnd_client().await?;
        let info: serde_json::Value = client
            .get(format!("{LND_REST_BASE_URL}/v1/getinfo"))
            .header("Grpc-Metadata-macaroon", &macaroon)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let network = match invoice.currency() {
            lightning_invoice::Currency::Bitcoin => "mainnet",
            lightning_invoice::Currency::BitcoinTestnet => "testnet",
            lightning_invoice::Currency::Regtest => "regtest",
            lightning_invoice::Currency::Signet => "signet",
            lightning_invoice::Currency::Simnet => "simnet",
        };
        anyhow::ensure!(
            info["chains"].as_array().is_some_and(|chains| chains
                .iter()
                .any(|chain| chain["chain"] == "bitcoin" && chain["network"] == network)),
            "Original invoice belongs to another Bitcoin network"
        );
        let client = reqwest::Client::builder()
            .no_proxy()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(8))
            .danger_accept_invalid_certs(true)
            .build()?;
        let request=client.post(format!("{LND_REST_BASE_URL}/v2/router/send")).header("Grpc-Metadata-macaroon",macaroon).json(&serde_json::json!({"payment_request":bolt11,"no_inflight_updates":true,"timeout_seconds":120,"fee_limit_sat":amount})).build()?;
        Ok(PreparedNativePayment {
            client,
            request,
            hash: hash.into(),
            amount,
        })
    }
}

impl crate::content_lightning::PreparedPayment for PreparedNativePayment {
    async fn execute(self) -> Result<serde_json::Value> {
        PreparedNativePayment::execute(self).await
    }
}
pub(crate) struct NativeNode<'a>(pub &'a RpcHandler);
impl crate::content_lightning::NativeInvoiceNode for NativeNode<'_> {
    type Prepared = PreparedNativePayment;
    async fn prepare(&self, invoice: &str, hash: &str, amount: u64) -> Result<Self::Prepared> {
        self.0
            .prepare_bound_invoice_payment(invoice, hash, amount)
            .await
    }
    async fn lookup_payment(&self, hash: &str) -> Result<serde_json::Value> {
        self.0
            .handle_lnd_paymentstatus(Some(serde_json::json!({"payment_hash":hash})))
            .await
    }
}
