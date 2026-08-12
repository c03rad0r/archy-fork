use super::RpcHandler;
use crate::network::router;
use anyhow::Result;

impl RpcHandler {
    /// Discover UPnP router on the local network.
    pub(super) async fn handle_router_discover(&self) -> Result<serde_json::Value> {
        let info = router::discover_router().await?;
        Ok(serde_json::json!({
            "discovered": info.discovered,
            "device_name": info.device_name,
            "wan_ip": info.wan_ip,
            "upnp_available": info.upnp_available,
        }))
    }

    /// List all configured port forwards.
    pub(super) async fn handle_router_list_forwards(&self) -> Result<serde_json::Value> {
        let forwards = router::list_forwards(&self.config.data_dir).await?;
        let items: Vec<serde_json::Value> = forwards
            .into_iter()
            .map(|f| {
                serde_json::json!({
                    "id": f.id,
                    "service_name": f.service_name,
                    "internal_port": f.internal_port,
                    "external_port": f.external_port,
                    "protocol": f.protocol,
                    "enabled": f.enabled,
                    "created_at": f.created_at,
                })
            })
            .collect();
        Ok(serde_json::json!({ "forwards": items }))
    }

    /// Add a port forward.
    pub(super) async fn handle_router_add_forward(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let service_name = params
            .get("service_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing service_name"))?;
        let internal_port = params
            .get("internal_port")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing internal_port"))?
            as u16;
        let external_port = params
            .get("external_port")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing external_port"))?
            as u16;
        let protocol = params
            .get("protocol")
            .and_then(|v| v.as_str())
            .unwrap_or("TCP");

        let forward = router::add_forward(
            &self.config.data_dir,
            service_name,
            internal_port,
            external_port,
            protocol,
        )
        .await?;

        Ok(serde_json::json!({
            "id": forward.id,
            "service_name": forward.service_name,
            "external_port": forward.external_port,
        }))
    }

    /// Remove a port forward.
    pub(super) async fn handle_router_remove_forward(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing id"))?;

        router::remove_forward(&self.config.data_dir, id).await?;
        Ok(serde_json::json!({ "ok": true }))
    }

    /// Run network diagnostics.
    pub(super) async fn handle_network_diagnostics(&self) -> Result<serde_json::Value> {
        let diag = router::run_diagnostics().await?;
        Ok(serde_json::json!({
            "wan_ip": diag.wan_ip,
            "nat_type": diag.nat_type,
            "upnp_available": diag.upnp_available,
            "tor_connected": diag.tor_connected,
            "dns_working": diag.dns_working,
            "recommendations": diag.recommendations,
            "wifi_ssid": diag.wifi_ssid,
        }))
    }

    /// Detect the type of router at a given gateway address.
    pub(super) async fn handle_router_detect(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let gateway = params
            .as_ref()
            .and_then(|p| p.get("gateway"))
            .and_then(|v| v.as_str())
            .unwrap_or("192.168.1.1");

        let router_type = router::detect_router_type(gateway).await;
        Ok(serde_json::json!({
            "gateway": gateway,
            "router_type": router_type,
        }))
    }

    /// Get router info and capabilities.
    pub(super) async fn handle_router_info(&self) -> Result<serde_json::Value> {
        router::get_router_info(&self.config.data_dir).await
    }

    /// Configure router API access.
    pub(super) async fn handle_router_configure(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let router_type_str = params
            .get("router_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let address = params
            .get("address")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing address"))?;
        let api_key = params.get("api_key").and_then(|v| v.as_str());
        let username = params.get("username").and_then(|v| v.as_str());
        let password = params.get("password").and_then(|v| v.as_str());

        let router_type = match router_type_str {
            "openwrt" => router::RouterType::OpenWrt,
            "pfsense" => router::RouterType::PfSense,
            "opnsense" => router::RouterType::OPNsense,
            "upnp" => router::RouterType::UPnP,
            _ => router::RouterType::Unknown,
        };

        let config = router::configure_router(
            &self.config.data_dir,
            router_type,
            address,
            api_key,
            username,
            password,
        )
        .await?;

        Ok(serde_json::json!({
            "configured": config.configured,
            "router_type": config.router_type,
        }))
    }
}
