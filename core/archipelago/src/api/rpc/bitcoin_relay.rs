use super::RpcHandler;
use crate::container::docker_packages;
use crate::data_model::{Notification, NotificationLevel};
use crate::{bitcoin_status, identity, peers};
use anyhow::{Context, Result};
use archipelago_container::ContainerState;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Sha256;
use std::path::{Path, PathBuf};
use tokio::fs;

const RELAY_DIR: &str = "bitcoin-relay";
const RELAY_STATE_FILE: &str = "state.json";
const TXRELAY_USER: &str = "txrelay";
const TXRELAY_PASSWORD_FILE: &str = "bitcoin-rpc-txrelay-password";
const TXRELAY_RPCAUTH_FILE: &str = "bitcoin-rpc-txrelay-rpcauth";
const TXRELAY_CLIENT_ENV_FILE: &str = "bitcoin-rpc-txrelay-client.env";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct BitcoinRelayState {
    settings: BitcoinRelaySettings,
    requests: Vec<BitcoinRelayRequest>,
    updated_at: Option<String>,
}

impl Default for BitcoinRelayState {
    fn default() -> Self {
        Self {
            settings: BitcoinRelaySettings::default(),
            requests: Vec::new(),
            updated_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct BitcoinRelaySettings {
    enabled_for_peers: bool,
    allow_peer_requests: bool,
    allow_http: bool,
    allow_https: bool,
    allow_tor: bool,
    selected_peer_pubkey: Option<String>,
    http_endpoint: Option<String>,
    https_endpoint: Option<String>,
    tor_endpoint: Option<String>,
}

impl Default for BitcoinRelaySettings {
    fn default() -> Self {
        Self {
            enabled_for_peers: false,
            allow_peer_requests: false,
            allow_http: false,
            allow_https: true,
            allow_tor: false,
            selected_peer_pubkey: None,
            http_endpoint: None,
            https_endpoint: None,
            tor_endpoint: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BitcoinRelayRequest {
    id: String,
    direction: RelayRequestDirection,
    status: RelayRequestStatus,
    peer_pubkey: String,
    peer_onion: String,
    peer_name: Option<String>,
    message: Option<String>,
    approved_endpoint: Option<String>,
    credential_secret_path: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RelayRequestDirection {
    Incoming,
    Outbound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RelayRequestStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Serialize)]
struct TrustedRelayPeer {
    pubkey: String,
    onion: String,
    name: Option<String>,
    relay_approved: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct TxRelayCredentials {
    username: String,
    password: String,
}

impl RpcHandler {
    pub(super) async fn handle_bitcoin_relay_status(&self) -> Result<serde_json::Value> {
        let mut state = load_relay_state(&self.config.data_dir).await?;
        hydrate_tor_endpoint(&self.config.data_dir, &mut state).await;
        let known_peers = peers::load_peers(&self.config.data_dir)
            .await
            .unwrap_or_default();
        let trusted_nodes = trusted_relay_peers(&known_peers, &state);
        let local_node = local_sync_status().await;
        let credential_status = txrelay_credential_status(&self.config.data_dir).await;

        Ok(json!({
            "settings": state.settings,
            "trusted_nodes": trusted_nodes,
            "requests": state.requests,
            "local_node": local_node,
            "credentials": credential_status,
        }))
    }

    pub(super) async fn handle_bitcoin_relay_update_settings(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let mut state = load_relay_state(&self.config.data_dir).await?;
        let known_peers = peers::load_peers(&self.config.data_dir)
            .await
            .unwrap_or_default();

        update_bool(
            &params,
            "enabled_for_peers",
            &mut state.settings.enabled_for_peers,
        );
        update_bool(
            &params,
            "allow_peer_requests",
            &mut state.settings.allow_peer_requests,
        );
        update_bool(&params, "allow_http", &mut state.settings.allow_http);
        update_bool(&params, "allow_https", &mut state.settings.allow_https);
        update_bool(&params, "allow_tor", &mut state.settings.allow_tor);

        update_endpoint(&params, "http_endpoint", &mut state.settings.http_endpoint)?;
        update_endpoint(
            &params,
            "https_endpoint",
            &mut state.settings.https_endpoint,
        )?;
        update_endpoint(&params, "tor_endpoint", &mut state.settings.tor_endpoint)?;

        if state.settings.enabled_for_peers {
            let credentials_were_ready = txrelay_credentials_available(&self.config.data_dir).await;
            ensure_txrelay_credentials(&self.config.data_dir).await?;
            if !credentials_were_ready {
                self.restart_bitcoin_backends_for_txrelay().await;
            }
        }

        if params.get("selected_peer_pubkey").is_some() {
            let selected = params
                .get("selected_peer_pubkey")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            if let Some(pubkey) = selected {
                if !known_peers.iter().any(|p| p.pubkey == pubkey) {
                    anyhow::bail!("Selected relay peer is not in trusted nodes");
                }
                state.settings.selected_peer_pubkey = Some(pubkey.to_string());
            } else {
                state.settings.selected_peer_pubkey = None;
            }
        }

        state.updated_at = Some(now());
        save_relay_state(&self.config.data_dir, &state).await?;
        self.notify(
            "Bitcoin relay settings updated",
            "Transaction relay sharing preferences were saved.",
        )
        .await;
        self.handle_bitcoin_relay_status().await
    }

    pub(super) async fn handle_bitcoin_relay_request_peer(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let peer_pubkey = params
            .get("peer_pubkey")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: peer_pubkey"))?;
        let message = params
            .get("message")
            .and_then(|v| v.as_str())
            .map(sanitize_optional_text)
            .transpose()?;
        let peer = peers::load_peers(&self.config.data_dir)
            .await
            .unwrap_or_default()
            .into_iter()
            .find(|p| p.pubkey == peer_pubkey)
            .ok_or_else(|| anyhow::anyhow!("Peer is not in trusted nodes"))?;

        let mut state = load_relay_state(&self.config.data_dir).await?;
        let existing = state.requests.iter_mut().find(|r| {
            r.direction == RelayRequestDirection::Outbound
                && r.peer_pubkey == peer.pubkey
                && r.status == RelayRequestStatus::Pending
        });
        let request_id = if let Some(req) = existing {
            req.message = message.clone();
            req.updated_at = now();
            req.id.clone()
        } else {
            let timestamp = now();
            let req = BitcoinRelayRequest {
                id: uuid::Uuid::new_v4().to_string(),
                direction: RelayRequestDirection::Outbound,
                status: RelayRequestStatus::Pending,
                peer_pubkey: peer.pubkey.clone(),
                peer_onion: peer.onion.clone(),
                peer_name: peer.name.clone(),
                message: message.clone(),
                approved_endpoint: None,
                credential_secret_path: None,
                created_at: timestamp.clone(),
                updated_at: timestamp,
            };
            let id = req.id.clone();
            state.requests.push(req);
            id
        };
        state.updated_at = Some(now());
        save_relay_state(&self.config.data_dir, &state).await?;

        if let Err(e) = self
            .send_relay_peer_message(
                &peer,
                json!({
                    "type": "bitcoin_relay_request",
                    "request_id": request_id,
                    "message": message,
                }),
            )
            .await
        {
            tracing::warn!(peer = %peer.onion, error = %e, "Failed to send Bitcoin relay request");
        }

        self.notify(
            "Bitcoin relay request sent",
            "A trusted peer was asked to approve transaction relay access.",
        )
        .await;
        Ok(json!({ "ok": true, "request_id": request_id }))
    }

    pub(super) async fn handle_bitcoin_relay_approve_request(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        self.update_relay_request_status(params, RelayRequestStatus::Approved)
            .await
    }

    pub(super) async fn handle_bitcoin_relay_reject_request(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        self.update_relay_request_status(params, RelayRequestStatus::Rejected)
            .await
    }

    pub(super) async fn handle_bitcoin_relay_create_tor_service(
        &self,
    ) -> Result<serde_json::Value> {
        let params = json!({
            "name": "bitcoin-rpc",
            "local_port": 80,
            "remote_port": 80,
        });
        let created = match self.handle_tor_create_service(Some(params)).await {
            Ok(v) => v,
            Err(e) if e.to_string().contains("already exists") => {
                self.handle_tor_get_onion_address(Some(json!({ "name": "bitcoin-rpc" })))
                    .await?
            }
            Err(e) => return Err(e),
        };

        let onion = created
            .get("onion_address")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        if let Some(onion) = onion {
            let mut state = load_relay_state(&self.config.data_dir).await?;
            state.settings.allow_tor = true;
            state.settings.tor_endpoint = Some(format!("http://{onion}/"));
            state.updated_at = Some(now());
            save_relay_state(&self.config.data_dir, &state).await?;
        }

        self.notify(
            "Bitcoin relay Tor service enabled",
            "A Tor endpoint was created for Bitcoin transaction relay access.",
        )
        .await;
        Ok(created)
    }

    async fn update_relay_request_status(
        &self,
        params: Option<serde_json::Value>,
        status: RelayRequestStatus,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let request_id = params
            .get("id")
            .or_else(|| params.get("request_id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: id"))?;
        let mut state = load_relay_state(&self.config.data_dir).await?;
        let serving_endpoint = if status == RelayRequestStatus::Approved {
            preferred_endpoint(&state.settings)
        } else {
            None
        };
        let request_direction = state
            .requests
            .iter()
            .find(|r| r.id == request_id)
            .ok_or_else(|| anyhow::anyhow!("Request not found: {}", request_id))?
            .direction;
        if status == RelayRequestStatus::Approved
            && request_direction == RelayRequestDirection::Incoming
            && serving_endpoint.is_none()
        {
            anyhow::bail!(
                "Configure an HTTP, HTTPS, or Tor relay endpoint before approving access"
            );
        }
        let credentials = if status == RelayRequestStatus::Approved {
            let credentials = ensure_txrelay_credentials(&self.config.data_dir).await?;
            if request_direction == RelayRequestDirection::Incoming {
                self.restart_bitcoin_backends_for_txrelay().await;
            }
            Some(credentials)
        } else {
            None
        };
        let (peer_pubkey, peer_onion, peer_name, direction) = {
            let req = state
                .requests
                .iter_mut()
                .find(|r| r.id == request_id)
                .ok_or_else(|| anyhow::anyhow!("Request not found: {}", request_id))?;
            req.status = status;
            req.updated_at = now();
            if let Some(endpoint) = &serving_endpoint {
                req.approved_endpoint = Some(endpoint.clone());
            }
            (
                req.peer_pubkey.clone(),
                req.peer_onion.clone(),
                req.peer_name.clone(),
                req.direction,
            )
        };
        let peer = peers::load_peers(&self.config.data_dir)
            .await
            .unwrap_or_default()
            .into_iter()
            .find(|p| p.pubkey == peer_pubkey);
        let peer_name = peer_name.unwrap_or_else(|| peer_onion.clone());
        state.updated_at = Some(now());
        save_relay_state(&self.config.data_dir, &state).await?;

        if let Some(peer) = peer {
            let message_type = match status {
                RelayRequestStatus::Approved => "bitcoin_relay_approved",
                RelayRequestStatus::Rejected => "bitcoin_relay_rejected",
                RelayRequestStatus::Pending => "bitcoin_relay_pending",
            };
            if let Err(e) = self
                .send_relay_peer_message(
                    &peer,
                    relay_response_payload(
                        message_type,
                        request_id,
                        direction,
                        serving_endpoint.as_deref(),
                        credentials.as_ref(),
                    ),
                )
                .await
            {
                tracing::warn!(peer = %peer.onion, error = %e, "Failed to send Bitcoin relay response");
            }
        }

        let title = match status {
            RelayRequestStatus::Approved => "Bitcoin relay request approved",
            RelayRequestStatus::Rejected => "Bitcoin relay request rejected",
            RelayRequestStatus::Pending => "Bitcoin relay request updated",
        };
        self.notify(
            title,
            &format!("Relay access request for {peer_name} was updated."),
        )
        .await;
        Ok(json!({ "ok": true, "request_id": request_id }))
    }

    async fn send_relay_peer_message(
        &self,
        peer: &peers::KnownPeer,
        mut payload: serde_json::Value,
    ) -> Result<()> {
        let (data, _) = self.state_manager.get_snapshot().await;
        let my_pubkey = data.server_info.pubkey.clone();
        let my_did = identity::did_key_from_pubkey_hex(&my_pubkey).ok();
        let my_onion = docker_packages::read_tor_address("archipelago")
            .await
            .unwrap_or_default();
        payload["from_did"] = my_did.map(serde_json::Value::String).unwrap_or_default();
        payload["from_pubkey"] = serde_json::Value::String(my_pubkey.clone());
        payload["from_onion"] = serde_json::Value::String(my_onion);
        payload["from_name"] = data
            .server_info
            .name
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or_default();

        let to_fips_npub =
            crate::federation::fips_npub_for_onion(&self.config.data_dir, &peer.onion).await;
        let identity_dir = self.config.data_dir.join("identity");
        let signing_key = crate::identity::NodeIdentity::load_or_create(&identity_dir)
            .await
            .ok();
        crate::node_message::send_to_peer(
            &peer.onion,
            to_fips_npub.as_deref(),
            &my_pubkey,
            &payload.to_string(),
            signing_key.as_ref().map(|i| i.signing_key()),
            Some(&peer.pubkey),
            data.server_info.name.as_deref(),
            Some(&self.config.data_dir),
        )
        .await
    }

    async fn notify(&self, title: &str, message: &str) {
        let (mut data, _) = self.state_manager.get_snapshot().await;
        data.notifications.push(Notification {
            id: format!("bitcoin-relay-{}", uuid::Uuid::new_v4()),
            level: NotificationLevel::Info,
            title: title.to_string(),
            message: message.to_string(),
            timestamp: now(),
            app_id: Some("bitcoin-knots".to_string()),
        });
        let len = data.notifications.len();
        if len > 30 {
            data.notifications.drain(0..len - 30);
        }
        self.state_manager.update_data(data).await;
    }

    async fn restart_bitcoin_backends_for_txrelay(&self) {
        let Some(orchestrator) = self.orchestrator.as_ref().cloned() else {
            tracing::debug!("Skipping txrelay backend restart; orchestrator unavailable");
            return;
        };
        tokio::spawn(async move {
            for app_id in ["bitcoin-knots", "bitcoin-core"] {
                let Ok(status) = orchestrator.status(app_id).await else {
                    continue;
                };
                if status.state != ContainerState::Running {
                    continue;
                }
                match orchestrator.restart(app_id).await {
                    Ok(()) => tracing::info!(
                        app_id,
                        "Restarted Bitcoin backend to load txrelay RPC credentials"
                    ),
                    Err(e) => tracing::warn!(
                        app_id,
                        error = %e,
                        "Failed to restart Bitcoin backend after txrelay credential update"
                    ),
                }
            }
        });
    }
}

pub(crate) async fn record_incoming_relay_message(
    data_dir: &Path,
    from_pubkey: &str,
    from_name: Option<&str>,
    payload: &serde_json::Value,
) -> Result<Option<&'static str>> {
    let msg_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match msg_type {
        "bitcoin_relay_request" => {
            let from_onion = payload
                .get("from_onion")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let message = payload
                .get("message")
                .and_then(|v| v.as_str())
                .map(sanitize_optional_text)
                .transpose()?;
            let remote_request_id = payload
                .get("request_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let mut state = load_relay_state(data_dir).await?;
            if !state.settings.allow_peer_requests {
                return Ok(Some("bitcoin_relay_request_disabled"));
            }
            if !state.requests.iter().any(|r| {
                r.direction == RelayRequestDirection::Incoming
                    && r.peer_pubkey == from_pubkey
                    && r.status == RelayRequestStatus::Pending
            }) {
                let timestamp = now();
                state.requests.push(BitcoinRelayRequest {
                    id: if remote_request_id.is_empty() {
                        uuid::Uuid::new_v4().to_string()
                    } else {
                        remote_request_id.to_string()
                    },
                    direction: RelayRequestDirection::Incoming,
                    status: RelayRequestStatus::Pending,
                    peer_pubkey: from_pubkey.to_string(),
                    peer_onion: from_onion,
                    peer_name: from_name.map(String::from),
                    message,
                    approved_endpoint: None,
                    credential_secret_path: None,
                    created_at: timestamp.clone(),
                    updated_at: timestamp,
                });
                state.updated_at = Some(now());
                save_relay_state(data_dir, &state).await?;
            }
            Ok(Some("bitcoin_relay_request"))
        }
        "bitcoin_relay_approved" | "bitcoin_relay_rejected" => {
            let request_id = payload.get("request_id").and_then(|v| v.as_str());
            let mut state = load_relay_state(data_dir).await?;
            let status = if msg_type == "bitcoin_relay_approved" {
                RelayRequestStatus::Approved
            } else {
                RelayRequestStatus::Rejected
            };
            let approved_access = if status == RelayRequestStatus::Approved {
                save_peer_relay_access(data_dir, from_pubkey, payload).await?
            } else {
                None
            };
            if let Some(req) = state.requests.iter_mut().find(|r| {
                r.direction == RelayRequestDirection::Outbound
                    && r.peer_pubkey == from_pubkey
                    && request_id.map(|id| id == r.id).unwrap_or(true)
            }) {
                req.status = status;
                req.updated_at = now();
                if let Some((endpoint, secret_path)) = approved_access {
                    req.approved_endpoint = Some(endpoint);
                    req.credential_secret_path = Some(secret_path);
                }
                state.updated_at = Some(now());
                save_relay_state(data_dir, &state).await?;
            }
            Ok(Some(if msg_type == "bitcoin_relay_approved" {
                "bitcoin_relay_approved"
            } else {
                "bitcoin_relay_rejected"
            }))
        }
        _ => Ok(None),
    }
}

fn trusted_relay_peers(
    known_peers: &[peers::KnownPeer],
    state: &BitcoinRelayState,
) -> Vec<TrustedRelayPeer> {
    known_peers
        .iter()
        .map(|peer| TrustedRelayPeer {
            pubkey: peer.pubkey.clone(),
            onion: peer.onion.clone(),
            name: peer.name.clone(),
            relay_approved: state.requests.iter().any(|req| {
                req.peer_pubkey == peer.pubkey && req.status == RelayRequestStatus::Approved
            }),
        })
        .collect()
}

async fn txrelay_credential_status(data_dir: &Path) -> serde_json::Value {
    let credentials_available = txrelay_credentials_available(data_dir).await;
    let (password_path, rpcauth_path, client_env_path) = txrelay_secret_paths(data_dir);
    let password_available = fs::metadata(&password_path).await.is_ok();
    let rpcauth_available = fs::metadata(&rpcauth_path).await.is_ok();
    let client_env_available = fs::metadata(&client_env_path).await.is_ok();
    json!({
        "username": TXRELAY_USER,
        "available": credentials_available,
        "password_available": password_available,
        "rpcauth_available": rpcauth_available,
        "client_env_available": client_env_available,
        "client_env_path": client_env_path.display().to_string(),
        "restart_hint": "Archipelago restarts the active Bitcoin backend after generating txrelay credentials so bitcoind loads the restricted rpcauth whitelist.",
    })
}

async fn txrelay_credentials_available(data_dir: &Path) -> bool {
    let (password_path, rpcauth_path, client_env_path) = txrelay_secret_paths(data_dir);
    fs::metadata(&password_path).await.is_ok()
        && fs::metadata(&rpcauth_path).await.is_ok()
        && fs::metadata(&client_env_path).await.is_ok()
}

/// Idempotently ensure the tx-relay credential trio exists in the secrets dir:
/// the random password, its derived `rpcauth` line, and the client env file.
/// Bitcoin backend manifests reference `bitcoin-rpc-txrelay-rpcauth` as a
/// required `secret_env`, so this must run before bitcoind starts — otherwise
/// secret resolution hard-fails and the whole Bitcoin stack cascades (the .198
/// failure). Safe to call repeatedly; it only writes what's missing or stale.
pub(crate) async fn ensure_txrelay_credentials(data_dir: &Path) -> Result<TxRelayCredentials> {
    let (password_path, rpcauth_path, client_env_path) = txrelay_secret_paths(data_dir);
    let password = match read_trimmed(&password_path).await {
        Some(value) => value,
        None => {
            let generated = generate_random_password();
            write_secret_file(&password_path, &generated).await?;
            generated
        }
    };
    let rpcauth = match read_trimmed(&rpcauth_path).await {
        Some(value) if rpcauth_matches_password(&value, TXRELAY_USER, &password) => value,
        _ => {
            let generated = generate_rpcauth(TXRELAY_USER, &password);
            write_secret_file(&rpcauth_path, &generated).await?;
            generated
        }
    };
    let client_env = format!(
        "BITCOIN_RPC_TXRELAY_USER={}\nBITCOIN_RPC_TXRELAY_PASSWORD={}\nBITCOIN_RPC_TXRELAY_RPCAUTH={}\n",
        TXRELAY_USER, password, rpcauth
    );
    write_secret_file(&client_env_path, &client_env).await?;

    Ok(TxRelayCredentials {
        username: TXRELAY_USER.to_string(),
        password,
    })
}

fn txrelay_secret_paths(data_dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let secrets_dir = data_dir.join("secrets");
    (
        secrets_dir.join(TXRELAY_PASSWORD_FILE),
        secrets_dir.join(TXRELAY_RPCAUTH_FILE),
        secrets_dir.join(TXRELAY_CLIENT_ENV_FILE),
    )
}

async fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

async fn write_secret_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(path, contents).await?;
    set_private_permissions(path).await;
    Ok(())
}

async fn set_private_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await;
    }
}

fn generate_random_password() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    BASE64.encode(bytes)
}

fn generate_rpcauth(username: &str, password: &str) -> String {
    let mut salt_bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut salt_bytes);
    let salt_hex = hex::encode(salt_bytes);
    let mut mac =
        Hmac::<Sha256>::new_from_slice(salt_hex.as_bytes()).expect("HMAC accepts any key length");
    mac.update(password.as_bytes());
    let hash_hex = hex::encode(mac.finalize().into_bytes());
    format!("{username}:{salt_hex}${hash_hex}")
}

fn rpcauth_matches_password(rpcauth: &str, username: &str, password: &str) -> bool {
    let Some(rest) = rpcauth.strip_prefix(&format!("{username}:")) else {
        return false;
    };
    let Some((salt_hex, expected_hash)) = rest.split_once('$') else {
        return false;
    };
    if salt_hex.is_empty() || expected_hash.is_empty() {
        return false;
    }
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(salt_hex.as_bytes()) else {
        return false;
    };
    mac.update(password.as_bytes());
    let hash_hex = hex::encode(mac.finalize().into_bytes());
    hash_hex.eq_ignore_ascii_case(expected_hash)
}

fn preferred_endpoint(settings: &BitcoinRelaySettings) -> Option<String> {
    if settings.allow_https {
        if let Some(endpoint) = settings.https_endpoint.clone() {
            return Some(endpoint);
        }
    }
    if settings.allow_tor {
        if let Some(endpoint) = settings.tor_endpoint.clone() {
            return Some(endpoint);
        }
    }
    if settings.allow_http {
        if let Some(endpoint) = settings.http_endpoint.clone() {
            return Some(endpoint);
        }
    }
    settings
        .https_endpoint
        .clone()
        .or_else(|| settings.tor_endpoint.clone())
        .or_else(|| settings.http_endpoint.clone())
}

fn relay_response_payload(
    message_type: &str,
    request_id: &str,
    request_direction: RelayRequestDirection,
    endpoint: Option<&str>,
    credentials: Option<&TxRelayCredentials>,
) -> serde_json::Value {
    let mut payload = json!({
        "type": message_type,
        "request_id": request_id,
    });
    if message_type == "bitcoin_relay_approved"
        && request_direction == RelayRequestDirection::Incoming
    {
        if let (Some(endpoint), Some(credentials)) = (endpoint, credentials) {
            payload["relay_access"] = json!({
                "endpoint": endpoint,
                "username": &credentials.username,
                "password": &credentials.password,
            });
        }
    }
    payload
}

async fn save_peer_relay_access(
    data_dir: &Path,
    from_pubkey: &str,
    payload: &serde_json::Value,
) -> Result<Option<(String, String)>> {
    let Some(access) = payload.get("relay_access") else {
        return Ok(None);
    };
    let endpoint = access
        .get("endpoint")
        .and_then(|v| v.as_str())
        .map(validate_endpoint)
        .transpose()?;
    let username = access.get("username").and_then(|v| v.as_str());
    let password = access.get("password").and_then(|v| v.as_str());
    let (Some(endpoint), Some(username), Some(password)) = (endpoint, username, password) else {
        return Ok(None);
    };
    validate_env_value(username)?;
    validate_env_value(password)?;

    let secret_path = data_dir.join("secrets").join(format!(
        "bitcoin-relay-peer-{}.env",
        safe_pubkey_fragment(from_pubkey)
    ));
    let contents = format!(
        "BITCOIN_RELAY_PEER_PUBKEY={}\nBITCOIN_RELAY_ENDPOINT={}\nBITCOIN_RELAY_USERNAME={}\nBITCOIN_RELAY_PASSWORD={}\n",
        from_pubkey, endpoint, username, password
    );
    write_secret_file(&secret_path, &contents).await?;
    Ok(Some((endpoint, secret_path.display().to_string())))
}

fn validate_env_value(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 1024 || value.contains('\n') || value.contains('\r') {
        anyhow::bail!("Invalid relay credential value");
    }
    Ok(())
}

fn safe_pubkey_fragment(pubkey: &str) -> String {
    let fragment = pubkey
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(24)
        .collect::<String>();
    if fragment.is_empty() {
        "unknown".to_string()
    } else {
        fragment
    }
}

async fn hydrate_tor_endpoint(data_dir: &Path, state: &mut BitcoinRelayState) {
    if state.settings.tor_endpoint.is_some() {
        return;
    }
    if let Some(onion) = docker_packages::read_tor_address("bitcoin-rpc").await {
        let onion = onion.trim().trim_end_matches('/').to_string();
        if !onion.is_empty() {
            state.settings.tor_endpoint = Some(format!("http://{onion}/"));
            if let Err(e) = save_relay_state(data_dir, state).await {
                tracing::warn!("Failed to persist relay tor endpoint: {e:#}");
            }
        }
    }
}

async fn local_sync_status() -> serde_json::Value {
    let status = bitcoin_status::get_bitcoin_status().await;
    let blockchain = status.blockchain_info.as_ref();
    let blocks = blockchain
        .and_then(|v| v.get("blocks"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let headers = blockchain
        .and_then(|v| v.get("headers"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let initial_block_download = blockchain
        .and_then(|v| v.get("initialblockdownload"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let synced =
        status.ok && headers > 0 && blocks >= headers.saturating_sub(1) && !initial_block_download;

    json!({
        "synced": synced,
        "blocks": blocks,
        "headers": headers,
        "chain": blockchain
            .and_then(|v| v.get("chain"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown"),
        "status_ok": status.ok,
        "status_stale": status.stale,
        "error": status.error,
    })
}

async fn load_relay_state(data_dir: &Path) -> Result<BitcoinRelayState> {
    let path = state_path(data_dir);
    if !path.exists() {
        return Ok(BitcoinRelayState::default());
    }
    let content = fs::read_to_string(&path)
        .await
        .with_context(|| format!("Failed to read {}", path.display()))?;
    Ok(serde_json::from_str(&content).unwrap_or_default())
}

async fn save_relay_state(data_dir: &Path, state: &BitcoinRelayState) -> Result<()> {
    let dir = data_dir.join(RELAY_DIR);
    fs::create_dir_all(&dir).await?;
    let content = serde_json::to_string_pretty(state)?;
    fs::write(dir.join(RELAY_STATE_FILE), content).await?;
    Ok(())
}

fn state_path(data_dir: &Path) -> PathBuf {
    data_dir.join(RELAY_DIR).join(RELAY_STATE_FILE)
}

fn update_bool(params: &serde_json::Value, key: &str, target: &mut bool) {
    if let Some(value) = params.get(key).and_then(|v| v.as_bool()) {
        *target = value;
    }
}

fn update_endpoint(
    params: &serde_json::Value,
    key: &str,
    target: &mut Option<String>,
) -> Result<()> {
    if !params.get(key).is_some() {
        return Ok(());
    }
    let endpoint = params
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    *target = endpoint.map(validate_endpoint).transpose()?;
    Ok(())
}

fn validate_endpoint(endpoint: &str) -> Result<String> {
    if endpoint.len() > 512 || endpoint.contains('\n') || endpoint.contains('\r') {
        anyhow::bail!("Invalid endpoint");
    }
    let lower = endpoint.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        anyhow::bail!("Endpoint must start with http:// or https://");
    }
    Ok(endpoint.to_string())
}

fn sanitize_optional_text(value: &str) -> Result<String> {
    let value = value.trim();
    if value.len() > 500 || value.contains('\0') {
        anyhow::bail!("Invalid message");
    }
    Ok(value.to_string())
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
