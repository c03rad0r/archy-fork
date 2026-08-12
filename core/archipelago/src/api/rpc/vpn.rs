use super::RpcHandler;
use crate::vpn;
use anyhow::Result;
use tracing::info;

impl RpcHandler {
    /// vpn.status — Get current VPN connection status.
    pub(super) async fn handle_vpn_status(&self) -> Result<serde_json::Value> {
        let status = vpn::get_status().await;
        let config = vpn::load_config(&self.config.data_dir).await?;

        // Check WireGuard wg0 interface for its IP
        let wg_ip = match tokio::process::Command::new("ip")
            .args(["-4", "addr", "show", "wg0"])
            .output()
            .await
        {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                let parsed = stdout
                    .lines()
                    .find(|l| l.contains("inet "))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .map(|ip| ip.split('/').next().unwrap_or(ip).to_string());
                if parsed.is_none() && !stdout.is_empty() {
                    tracing::debug!("wg0 exists but no inet address found");
                }
                // Fallback: if wg0 exists but has no server IP, read from config
                parsed.or_else(|| {
                    // If wg0 link is up, report the static server IP
                    if stdout.contains("UP") || stdout.contains("POINTOPOINT") {
                        Some("10.44.0.1".to_string())
                    } else {
                        None
                    }
                })
            }
            Err(_) => None,
        };

        let node_npub = vpn::read_nvpn_config_value("nostr", "public_key")
            .await
            .map(|k| vpn::ensure_npub(&k));
        let (relay_onion, relay_direct) = vpn::get_relay_urls().await;
        // Prefer onion (always works), fall back to direct IP
        let relay_url = relay_onion.clone().or(relay_direct.clone());

        // Standalone WireGuard public key
        let wg_pubkey = tokio::fs::read_to_string("/var/lib/archipelago/wireguard/public.key")
            .await
            .ok()
            .map(|s| s.trim().to_string());

        // Check if nvpn0 tunnel interface actually exists and has an IP
        let nvpn0_ip = tokio::process::Command::new("ip")
            .args(["-4", "addr", "show", "nvpn0"])
            .output()
            .await
            .ok()
            .and_then(|o| {
                let out = String::from_utf8_lossy(&o.stdout).to_string();
                out.lines()
                    .find(|l| l.contains("inet "))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .map(|s| s.split('/').next().unwrap_or(s).to_string())
            });

        // NostrVPN IP: only report if nvpn0 tunnel is actually up with its own IP,
        // and that IP is distinct from the standalone WireGuard IP
        let nvpn_ip = nvpn0_ip.as_ref().and_then(|ip| {
            if wg_ip.as_deref() == Some(ip.as_str()) {
                None
            } else {
                Some(ip.clone())
            }
        });

        // NostrVPN is connected only if its dedicated tunnel (nvpn0) has a distinct IP
        let nvpn_connected = status.provider.as_deref() == Some("nostr-vpn") && nvpn_ip.is_some();

        // connected = NostrVPN tunnel is up OR another VPN provider is active OR standalone WireGuard is up
        let is_connected = if status.provider.as_deref() == Some("nostr-vpn") {
            nvpn_connected || wg_ip.is_some()
        } else {
            status.connected || wg_ip.is_some()
        };

        Ok(serde_json::json!({
            "connected": is_connected,
            "provider": status.provider,
            "interface": status.interface,
            "ip_address": nvpn_ip,
            "hostname": status.hostname,
            "peers_connected": status.peers_connected,
            "bytes_in": status.bytes_in,
            "bytes_out": status.bytes_out,
            "configured": config.enabled,
            "configured_provider": format!("{:?}", config.provider).to_lowercase(),
            "wg_ip": wg_ip,
            "wg_pubkey": wg_pubkey,
            "node_npub": node_npub,
            "relay_url": relay_url,
            "relay_onion": relay_onion,
            "relay_direct": relay_direct,
        }))
    }

    /// vpn.configure — Configure VPN (Tailscale or WireGuard).
    pub(super) async fn handle_vpn_configure(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let provider = params
            .get("provider")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'provider' (tailscale or wireguard)"))?;

        match provider {
            "tailscale" => {
                let auth_key = params
                    .get("auth_key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'auth_key' for Tailscale"))?;

                vpn::configure_tailscale(auth_key, &self.config.data_dir).await?;
                info!("Tailscale VPN configured");

                Ok(serde_json::json!({
                    "configured": true,
                    "provider": "tailscale",
                }))
            }
            "wireguard" => {
                let address = params
                    .get("address")
                    .and_then(|v| v.as_str())
                    .unwrap_or("10.0.0.1/24");
                let dns = params
                    .get("dns")
                    .and_then(|v| v.as_str())
                    .unwrap_or("1.1.1.1");

                let peer = if let Some(peer_obj) = params.get("peer") {
                    let public_key = peer_obj
                        .get("public_key")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::anyhow!("Missing peer public_key"))?;
                    let endpoint = peer_obj
                        .get("endpoint")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::anyhow!("Missing peer endpoint"))?;
                    let allowed_ips = peer_obj
                        .get("allowed_ips")
                        .and_then(|v| v.as_str())
                        .unwrap_or("0.0.0.0/0");
                    let keepalive = peer_obj
                        .get("persistent_keepalive")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u16);

                    Some(vpn::WireGuardPeer {
                        public_key: public_key.to_string(),
                        endpoint: endpoint.to_string(),
                        allowed_ips: allowed_ips.to_string(),
                        persistent_keepalive: keepalive,
                    })
                } else {
                    None
                };

                let wg_config =
                    vpn::configure_wireguard(&self.config.data_dir, address, dns, peer).await?;

                info!("WireGuard VPN configured");
                Ok(serde_json::json!({
                    "configured": true,
                    "provider": "wireguard",
                    "public_key": wg_config.public_key,
                    "address": wg_config.address,
                }))
            }
            _ => {
                anyhow::bail!(
                    "Unknown provider: {} (expected tailscale or wireguard)",
                    provider
                );
            }
        }
    }

    /// remote.setup — One-click Tailscale remote access setup.
    /// Accepts an auth key, configures Tailscale, and restricts access to ports 80/443.
    pub(super) async fn handle_remote_setup(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let auth_key = params
            .get("auth_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'auth_key' — get one from https://login.tailscale.com/admin/settings/keys"))?;

        // Configure Tailscale
        vpn::configure_tailscale(auth_key, &self.config.data_dir).await?;
        info!("Remote access: Tailscale configured");

        // Set ACL-like port restrictions via iptables on tailscale0
        // Allow only HTTP (80) and HTTPS (443) on the Tailscale interface
        let restrict_cmds = [
            "sudo iptables -D INPUT -i tailscale0 -p tcp --dport 80 -j ACCEPT 2>/dev/null; true",
            "sudo iptables -D INPUT -i tailscale0 -p tcp --dport 443 -j ACCEPT 2>/dev/null; true",
            "sudo iptables -D INPUT -i tailscale0 -j DROP 2>/dev/null; true",
            "sudo iptables -A INPUT -i tailscale0 -p tcp --dport 80 -j ACCEPT",
            "sudo iptables -A INPUT -i tailscale0 -p tcp --dport 443 -j ACCEPT",
            "sudo iptables -A INPUT -i tailscale0 -p tcp -m state --state ESTABLISHED,RELATED -j ACCEPT",
            "sudo iptables -A INPUT -i tailscale0 -j DROP",
        ];

        for cmd in &restrict_cmds {
            let _ = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .output()
                .await;
        }
        info!("Remote access: Restricted Tailscale to ports 80/443");

        // Get the Tailscale IP for display
        let status = vpn::get_status().await;
        let tailscale_ip = status.ip_address.clone().unwrap_or_default();
        let hostname = status.hostname.clone().unwrap_or_default();

        // Build the remote access URL
        let remote_url = if !hostname.is_empty() {
            format!("http://{}", hostname)
        } else if !tailscale_ip.is_empty() {
            format!("http://{}", tailscale_ip)
        } else {
            String::new()
        };

        Ok(serde_json::json!({
            "configured": true,
            "provider": "tailscale",
            "tailscale_ip": tailscale_ip,
            "hostname": hostname,
            "remote_url": remote_url,
            "ports_exposed": [80, 443],
        }))
    }

    /// vpn.disconnect — Disable VPN.
    pub(super) async fn handle_vpn_disconnect(&self) -> Result<serde_json::Value> {
        let mut config = vpn::load_config(&self.config.data_dir).await?;
        config.enabled = false;
        vpn::save_config(&self.config.data_dir, &config).await?;

        // Try to bring down the interface
        match config.provider {
            vpn::VpnProvider::Tailscale => {
                let _ = tokio::process::Command::new("podman")
                    .args(["exec", "tailscale", "tailscale", "down"])
                    .output()
                    .await;
            }
            vpn::VpnProvider::Wireguard => {
                let _ = tokio::process::Command::new("wg-quick")
                    .args(["down", "wg0"])
                    .output()
                    .await;
            }
            vpn::VpnProvider::NostrVpn => {
                let _ = tokio::process::Command::new("systemctl")
                    .args(["stop", "nostr-vpn"])
                    .output()
                    .await;
            }
        }

        info!("VPN disconnected");
        Ok(serde_json::json!({ "disconnected": true }))
    }

    /// vpn.invite — Generate a NostrVPN invite URL + QR for the mobile app.
    /// Optionally accepts `npub` param to add the phone as a participant in the same call.
    pub(super) async fn handle_vpn_invite(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        // If an npub was provided, add it as a participant first
        if let Some(ref p) = params {
            if let Some(peer_npub) = p.get("npub").and_then(|v| v.as_str()) {
                if !peer_npub.is_empty() {
                    // Reuse add-participant logic
                    self.handle_vpn_add_participant(Some(serde_json::json!({ "npub": peer_npub })))
                        .await?;
                }
            }
        }

        // Read nvpn config to build invite (convert hex to npub1 if needed)
        let npub = vpn::read_nvpn_config_value("nostr", "public_key")
            .await
            .map(|k| vpn::ensure_npub(&k))
            .ok_or_else(|| {
                anyhow::anyhow!("No Nostr public key in nvpn config — VPN not configured")
            })?;
        // network_id is in [[networks]] array — read first entry
        let network_id = vpn::read_nvpn_config_list_entry("networks", "network_id")
            .await
            .unwrap_or_else(|| "nostr-vpn".to_string());

        // Read relays from config — filter out localhost relays (unreachable from phone)
        let relays = vpn::read_nvpn_config_list("nostr", "relays").await;
        let reachable: Vec<String> = relays
            .iter()
            .filter(|r| !r.contains("127.0.0.1") && !r.contains("localhost"))
            .cloned()
            .collect();
        let invite_relays = if reachable.is_empty() {
            vec![
                "wss://relay.damus.io".to_string(),
                "wss://relay.primal.net".to_string(),
            ]
        } else {
            reachable
        };

        // Build invite as base64-encoded JSON (nvpn v2 format, no padding)
        use base64::Engine;
        let invite_payload = serde_json::json!({
            "v": 2,
            "networkName": network_id,
            "networkId": network_id,
            "inviterNpub": npub,
            "inviterNodeName": "archipelago",
            "admins": [npub],
            "participants": [npub],
            "relays": invite_relays,
        });
        let invite_b64 = base64::engine::general_purpose::STANDARD_NO_PAD
            .encode(invite_payload.to_string().as_bytes());
        let invite_url = format!("nvpn://invite/{}", invite_b64);

        // Generate QR code
        let qr = qrcode::QrCode::new(invite_url.as_bytes())
            .map_err(|e| anyhow::anyhow!("QR generation failed: {}", e))?;
        let svg = qr
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(256, 256)
            .build();

        Ok(serde_json::json!({
            "invite_url": invite_url,
            "qr_svg": svg,
            "npub": npub,
            "network_id": network_id,
            "relays": invite_relays,
        }))
    }

    /// vpn.add-participant — Add an npub to the mesh network.
    pub(super) async fn handle_vpn_add_participant(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let npub = params
            .get("npub")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'npub'"))?;

        // Validate npub format
        if !npub.starts_with("npub1") || npub.len() < 60 {
            anyhow::bail!("Invalid npub format");
        }

        // Add participant by editing TOML config directly (nvpn set --participant replaces, not appends)
        for config_path in vpn::NVPN_CONFIG_PATHS {
            if let Ok(content) = tokio::fs::read_to_string(config_path).await {
                if let Ok(mut table) = content.parse::<toml::Table>() {
                    if let Some(networks) = table.get_mut("networks").and_then(|v| v.as_array_mut())
                    {
                        for net in networks.iter_mut() {
                            if let Some(net_table) = net.as_table_mut() {
                                let participants = net_table
                                    .entry("participants")
                                    .or_insert_with(|| toml::Value::Array(vec![]));
                                if let Some(arr) = participants.as_array_mut() {
                                    let npub_val = toml::Value::String(npub.to_string());
                                    if !arr.contains(&npub_val) {
                                        arr.push(npub_val);
                                    }
                                }
                            }
                        }
                    }
                    if let Ok(new_content) = toml::to_string_pretty(&table) {
                        // Try direct write first; fall back to sudo cp for root-owned daemon config
                        if tokio::fs::write(config_path, &new_content).await.is_ok() {
                            info!("Added participant to {}", config_path);
                        } else {
                            // Write to temp file, then sudo cp to target
                            let tmp = format!("/tmp/.nvpn-config-{}", std::process::id());
                            if tokio::fs::write(&tmp, &new_content).await.is_ok() {
                                let cp = tokio::process::Command::new("sudo")
                                    .args(["cp", &tmp, config_path])
                                    .output()
                                    .await;
                                let _ = tokio::fs::remove_file(&tmp).await;
                                match cp {
                                    Ok(ref out) if out.status.success() => {
                                        info!("Added participant to {} (via sudo)", config_path);
                                    }
                                    _ => {
                                        tracing::warn!(
                                            "Failed to write {} (even with sudo)",
                                            config_path
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Restart daemon to pick up the new participant
        let _ = tokio::process::Command::new("sudo")
            .args(["systemctl", "restart", "nostr-vpn"])
            .output()
            .await;

        info!("VPN participant added: {}", npub);
        Ok(serde_json::json!({ "added": true, "npub": npub }))
    }

    /// The host address a WireGuard peer should dial — prefer the configured
    /// host IP, then public-IP lookup, then first local address.
    async fn current_wg_endpoint_host(&self) -> String {
        if self.config.host_ip != "127.0.0.1" {
            return self.config.host_ip.clone();
        }
        tokio::process::Command::new("sh")
            .arg("-c")
            .arg("curl -s --connect-timeout 5 https://api.ipify.org 2>/dev/null || hostname -I | awk '{print $1}'")
            .output()
            .await
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.config.host_ip.clone())
    }

    /// vpn.create-peer — Generate a WireGuard peer config + QR code for mobile devices.
    pub(super) async fn handle_vpn_create_peer(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or(serde_json::json!({}));
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Mobile");

        // Check that wg0 is up (standalone WireGuard)
        let wg0_up = tokio::process::Command::new("ip")
            .args(["link", "show", "wg0"])
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !wg0_up {
            anyhow::bail!("WireGuard (wg0) is not running. Wait for first-boot to complete.");
        }

        // Generate a keypair for the new peer using wg genkey/pubkey
        let genkey = tokio::process::Command::new("wg")
            .arg("genkey")
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("wg genkey failed: {}", e))?;
        if !genkey.status.success() {
            anyhow::bail!(
                "wg genkey failed: {}",
                String::from_utf8_lossy(&genkey.stderr)
            );
        }
        let peer_private = String::from_utf8_lossy(&genkey.stdout).trim().to_string();

        let mut pubkey_cmd = tokio::process::Command::new("wg");
        pubkey_cmd.arg("pubkey");
        pubkey_cmd.stdin(std::process::Stdio::piped());
        pubkey_cmd.stdout(std::process::Stdio::piped());
        let mut pubkey_child = pubkey_cmd
            .spawn()
            .map_err(|e| anyhow::anyhow!("wg pubkey spawn failed: {}", e))?;
        if let Some(ref mut stdin) = pubkey_child.stdin {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(peer_private.as_bytes()).await?;
            stdin.shutdown().await?;
        }
        let pubkey_out = pubkey_child.wait_with_output().await?;
        let peer_public = String::from_utf8_lossy(&pubkey_out.stdout)
            .trim()
            .to_string();

        // Read server's WireGuard public key (standalone WG key, then fall back to nvpn)
        let server_pubkey = if let Ok(key) =
            tokio::fs::read_to_string("/var/lib/archipelago/wireguard/public.key").await
        {
            key.trim().to_string()
        } else {
            vpn::read_nvpn_config_value("node", "public_key")
                .await
                .ok_or_else(|| anyhow::anyhow!("Cannot read server public key"))?
        };

        let endpoint = format!("{}:51820", self.current_wg_endpoint_host().await);

        // Allocate a peer IP (simple: hash the peer name)
        let peer_num = (name.bytes().map(|b| b as u32).sum::<u32>() % 253) + 2;
        let peer_ip = format!("10.44.0.{}/32", peer_num);

        // Build WireGuard config for the mobile device
        let wg_config = format!(
            "[Interface]\nPrivateKey = {}\nAddress = {}\nDNS = 1.1.1.1\n\n[Peer]\nPublicKey = {}\nEndpoint = {}\nAllowedIPs = 10.44.0.0/16\nPersistentKeepalive = 25\n",
            peer_private, peer_ip, server_pubkey, endpoint
        );

        // Generate QR code as SVG
        let qr = qrcode::QrCode::new(wg_config.as_bytes())
            .map_err(|e| anyhow::anyhow!("QR generation failed: {}", e))?;
        let svg = qr
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(256, 256)
            .build();

        // Save peer info
        let peers_dir = self.config.data_dir.join("nostr-vpn/peers");
        tokio::fs::create_dir_all(&peers_dir).await.ok();
        let peer_info = serde_json::json!({
            "name": name,
            "public_key": peer_public,
            "ip": peer_ip,
            "config": wg_config,
            "created": chrono::Utc::now().to_rfc3339(),
        });
        tokio::fs::write(
            peers_dir.join(format!("{}.json", name.to_lowercase().replace(' ', "-"))),
            serde_json::to_string_pretty(&peer_info)?,
        )
        .await
        .ok();

        // Add this peer to the server's WireGuard interface (managed by nvpn).
        // Try add-peer first; if wg0 doesn't exist, run setup then retry.
        let peer_filename = format!("{}.json", name.to_lowercase().replace(' ', "-"));
        let mut peer_added = false;
        for attempt in 0..2 {
            let add = tokio::process::Command::new("sudo")
                .args(["archipelago-wg", "add-peer", &peer_public, &peer_ip])
                .output()
                .await;
            match add {
                Ok(ref out) if out.status.success() => {
                    peer_added = true;
                    break;
                }
                Ok(ref out) => {
                    let err = String::from_utf8_lossy(&out.stderr);
                    tracing::warn!("add-peer attempt {}: {}", attempt + 1, err);
                    if attempt == 0 {
                        // wg0 may not exist yet — try creating it
                        let server_privkey = vpn::read_nvpn_config_value("node", "private_key")
                            .await
                            .unwrap_or_default();
                        if !server_privkey.is_empty() {
                            let key_path = "/tmp/.wg-server-key";
                            tokio::fs::write(key_path, &server_privkey).await.ok();
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                std::fs::set_permissions(
                                    key_path,
                                    std::fs::Permissions::from_mode(0o600),
                                )
                                .ok();
                            }
                            let _ = tokio::process::Command::new("sudo")
                                .args(["archipelago-wg", "setup", key_path])
                                .output()
                                .await;
                            tokio::fs::remove_file(key_path).await.ok();
                        }
                        // Brief pause before retry
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
                Err(e) => {
                    tracing::warn!("add-peer command error: {}", e);
                    break;
                }
            }
        }
        if !peer_added {
            let _ = tokio::fs::remove_file(peers_dir.join(&peer_filename)).await;
            anyhow::bail!(
                "Failed to register peer with WireGuard. Check that wg0 interface is up."
            );
        }

        info!("VPN peer created: {} ({})", name, peer_ip);

        Ok(serde_json::json!({
            "name": name,
            "peer_ip": peer_ip,
            "config": wg_config,
            "qr_svg": svg,
            "public_key": peer_public,
        }))
    }

    /// vpn.list-peers — List configured VPN peers (WireGuard + NostrVPN participants).
    pub(super) async fn handle_vpn_list_peers(&self) -> Result<serde_json::Value> {
        let peers_dir = self.config.data_dir.join("nostr-vpn/peers");
        let mut peers = Vec::new();

        // WireGuard manual peers (from JSON files)
        if let Ok(mut entries) = tokio::fs::read_dir(&peers_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry
                    .path()
                    .extension()
                    .map(|e| e == "json")
                    .unwrap_or(false)
                {
                    if let Ok(content) = tokio::fs::read_to_string(entry.path()).await {
                        if let Ok(mut peer) = serde_json::from_str::<serde_json::Value>(&content) {
                            peer.as_object_mut()
                                .map(|o| o.insert("type".to_string(), "wireguard".into()));
                            peers.push(peer);
                        }
                    }
                }
            }
        }

        // NostrVPN peer loading removed — standalone WireGuard only
        Ok(serde_json::json!({ "peers": peers }))
    }

    /// vpn.peer-config — Retrieve stored config + QR for an existing peer.
    pub(super) async fn handle_vpn_peer_config(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'name'"))?;

        let filename = format!("{}.json", name.to_lowercase().replace(' ', "-"));
        let peer_file = self.config.data_dir.join("nostr-vpn/peers").join(&filename);

        let content = tokio::fs::read_to_string(&peer_file)
            .await
            .map_err(|_| anyhow::anyhow!("Peer '{}' not found", name))?;
        let mut peer: serde_json::Value = serde_json::from_str(&content)?;

        let stored = peer.get("config").and_then(|v| v.as_str()).ok_or_else(|| {
            anyhow::anyhow!(
                "No config stored for peer '{}' — recreate the device to get a new QR code",
                name
            )
        })?;

        // The stored Endpoint is the node's address at creation time; after
        // the node moves networks it points at a dead IP and the QR produces
        // a tunnel that can never connect. Refresh it to the current address.
        let endpoint = format!("{}:51820", self.current_wg_endpoint_host().await);
        let config: String = stored
            .lines()
            .map(|l| {
                if l.trim_start().starts_with("Endpoint") {
                    format!("Endpoint = {}", endpoint)
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        if config != stored {
            if let Some(obj) = peer.as_object_mut() {
                obj.insert("config".to_string(), config.clone().into());
            }
            if let Ok(json) = serde_json::to_string_pretty(&peer) {
                if tokio::fs::write(&peer_file, json).await.is_ok() {
                    info!("VPN peer '{}' endpoint refreshed to {}", name, endpoint);
                }
            }
        }

        let qr = qrcode::QrCode::new(config.as_bytes())
            .map_err(|e| anyhow::anyhow!("QR generation failed: {}", e))?;
        let svg = qr
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(256, 256)
            .build();

        Ok(serde_json::json!({
            "name": name,
            "peer_ip": peer.get("ip").and_then(|v| v.as_str()).unwrap_or(""),
            "config": config,
            "qr_svg": svg,
        }))
    }

    /// vpn.remove-peer — Remove a VPN peer by name.
    pub(super) async fn handle_vpn_remove_peer(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'name'"))?;

        let filename = format!("{}.json", name.to_lowercase().replace(' ', "-"));
        let peer_file = self.config.data_dir.join("nostr-vpn/peers").join(&filename);

        // Read peer's public key before deleting, to remove from WireGuard interface
        let peer_pubkey = tokio::fs::read_to_string(&peer_file)
            .await
            .ok()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
            .and_then(|v| {
                v.get("public_key")
                    .and_then(|k| k.as_str())
                    .map(|s| s.to_string())
            });

        if tokio::fs::remove_file(&peer_file).await.is_ok() {
            // Remove peer from WireGuard interface
            if let Some(pubkey) = peer_pubkey {
                let _ = tokio::process::Command::new("sudo")
                    .args(["archipelago-wg", "remove-peer", &pubkey])
                    .output()
                    .await;
            }
            info!("VPN peer removed: {}", name);
            Ok(serde_json::json!({ "removed": true }))
        } else {
            anyhow::bail!("Peer '{}' not found", name);
        }
    }
}
