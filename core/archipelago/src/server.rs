use crate::api::ApiHandler;
use crate::config::{Config, ContainerRuntime};
use crate::container::{
    docker_packages, ContainerOrchestrator, DevContainerOrchestrator, DockerPackageScanner,
};
use crate::identity::{self, NodeIdentity};
use crate::monitoring::MetricsStore;
use crate::node_message;
use crate::nostr_discovery;
use crate::nostr_handshake;
use crate::peers;
use crate::state::StateManager;
use anyhow::Result;
use hyper::server::conn::Http;
use hyper::service::service_fn;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{debug, error, info, warn};

pub struct Server {
    _config: Config,
    _identity: Arc<NodeIdentity>,
    api_handler: Arc<ApiHandler>,
    _state_manager: Arc<StateManager>,
}

struct ContainerScanGuard<'a> {
    scanning: &'a AtomicBool,
}

impl<'a> ContainerScanGuard<'a> {
    fn try_acquire(scanning: &'a AtomicBool) -> Option<Self> {
        scanning
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| Self { scanning })
    }
}

impl Drop for ContainerScanGuard<'_> {
    fn drop(&mut self) {
        self.scanning.store(false, Ordering::Release);
    }
}

impl Server {
    pub async fn new(
        config: Config,
        orchestrator: Option<Arc<dyn ContainerOrchestrator>>,
        dev_orchestrator: Option<Arc<DevContainerOrchestrator>>,
    ) -> Result<Self> {
        let state_manager = Arc::new(StateManager::new());

        // Load node identity and set stable server_info.
        // Detect seed-backed vs legacy vs fresh install.
        let identity_dir = config.data_dir.join("identity");
        let has_seed = crate::seed::seed_exists(&config.data_dir);
        let has_node_key = NodeIdentity::key_exists(&identity_dir);

        let identity = if has_node_key {
            // Existing keys on disk (seed-derived or legacy random) — load them.
            NodeIdentity::load_or_create(&identity_dir).await?
        } else {
            // Fresh install — create a temporary identity.
            // Onboarding will overwrite this with seed-derived keys.
            NodeIdentity::load_or_create(&identity_dir).await?
        };

        let (mut data, _) = state_manager.get_snapshot().await;
        data.server_info.id = identity.node_id();
        data.server_info.pubkey = identity.pubkey_hex();
        data.server_info.seed_backed = has_seed;
        // Load persisted server name
        let name_file = config.data_dir.join("server-name");
        if let Ok(name) = tokio::fs::read_to_string(&name_file).await {
            let name = name.trim().to_string();
            if !name.is_empty() {
                data.server_info.name = Some(name);
            }
        }
        // Load persisted node location (Mesh Map opt-in sharing)
        let location_file = config.data_dir.join("server-location.json");
        if let Ok(bytes) = tokio::fs::read(&location_file).await {
            if let Ok(loc) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                data.server_info.lat = loc.get("lat").and_then(|v| v.as_f64());
                data.server_info.lon = loc.get("lon").and_then(|v| v.as_f64());
                data.server_info.share_location = loc
                    .get("share_location")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
            }
        }
        data.server_info.tor_address = docker_packages::read_tor_address("archipelago").await;
        // Liveness comes from a probe, never from the presence of an onion
        // address: read_tor_address reads a hostname file that outlives the
        // daemon, which is why the dashboard showed "Connected" on nodes whose
        // Tor had been dead for days.
        data.server_info.tor_running = crate::api::rpc::tor::check_tor_running().await;
        if let Some(ref tor) = data.server_info.tor_address {
            data.server_info.node_address = Some(identity.node_address(tor));
        }
        state_manager.update_data(data.clone()).await;

        // Real-time wallet push: stream LND transaction events → websocket
        // revision nudges, so an incoming on-chain tx shows in the UI within
        // seconds of hitting the mempool (works whenever LND is up; retries
        // forever otherwise). User req 2026-07-22.
        crate::api::rpc::lnd::spawn_lnd_tx_watcher(state_manager.clone());
        // LND wedge watchdog — self-heal the silent "RPC up, server never
        // ready" state instead of waiting for a human (100%-uptime req).
        crate::api::rpc::lnd::spawn_lnd_health_watchdog();

        // Retry Tor address in background — Tor may not be ready at startup
        if data.server_info.tor_address.is_none() {
            let sm = state_manager.clone();
            let pubkey = identity.pubkey_hex();
            tokio::spawn(async move {
                for delay in [5, 10, 20, 30, 60] {
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                    if let Some(tor) = docker_packages::read_tor_address("archipelago").await {
                        let (mut d, _) = sm.get_snapshot().await;
                        let addr =
                            format!("archipelago://{}#{}", tor.trim_end_matches('/'), pubkey);
                        d.server_info.tor_address = Some(tor.clone());
                        d.server_info.node_address = Some(addr);
                        sm.update_data(d).await;
                        tracing::info!(
                            "Tor address discovered after startup: {}",
                            &tor[..20.min(tor.len())]
                        );
                        break;
                    }
                }
            });
        }

        // Load persisted messages (Archipelago channel)
        node_message::init(&config.data_dir).await;

        // Auto-create the Node identity on fresh boot, mirroring the node's
        // own signing key (seed-derived when onboarded, random otherwise).
        // This keeps the DID shown on the Identities page, the DID Status
        // card, and the DID used for peer-to-peer connects all aligned on
        // one value — the seed-derived node DID. Idempotent: if the entry
        // already exists from a prior boot, create_from_signing_key returns
        // the existing record unchanged.
        {
            let im = crate::identity_manager::IdentityManager::new(&config.data_dir).await;
            if let Ok(mgr) = im {
                if let Ok((list, _)) = mgr.list().await {
                    if list.is_empty() {
                        let signing_key = ed25519_dalek::SigningKey::from_bytes(
                            &identity.signing_key().to_bytes(),
                        );
                        match mgr
                            .create_from_signing_key(
                                "Node".to_string(),
                                crate::identity_manager::IdentityPurpose::Personal,
                                signing_key,
                            )
                            .await
                        {
                            Ok(record) => {
                                let _ = mgr.create_nostr_key(&record.id).await;
                                tracing::info!(did = %record.did, "Auto-created Node identity mirroring node key");
                            }
                            Err(e) => tracing::debug!("Auto-identity creation (non-fatal): {}", e),
                        }
                    }
                }
            }
        }

        // DHT swarm-assist (Phase 3): build the iroh provider once at startup so
        // release downloads can fetch from peers (origin always wins) and seed
        // what they hold. Inert unless built with `iroh-swarm` AND swarm_enabled.
        if let Err(e) = crate::swarm::init(
            &config.data_dir,
            &config.nostr_relays,
            config.nostr_tor_proxy.as_deref(),
            config.swarm_enabled,
        )
        .await
        {
            tracing::warn!("Swarm init (non-fatal, falling back to origin-only): {}", e);
        }

        // Resume any cross-mint ecash swap interrupted by a previous crash
        // (paid the source mint but never claimed the target tokens). Best-effort.
        match crate::wallet::ecash::resume_pending_swaps(&config.data_dir).await {
            Ok(0) => {}
            Ok(reclaimed) => tracing::info!(
                "Resumed interrupted cross-mint swaps: reclaimed {} sats",
                reclaimed
            ),
            Err(e) => tracing::debug!("resume_pending_swaps (non-fatal): {}", e),
        }

        // Revoke any previously published Nostr data (runs before publish so revocation is not overwritten)
        let identity_dir = config.data_dir.join("identity");
        let tor_proxy_revoke = config.nostr_tor_proxy.clone();
        if let Err(e) =
            nostr_discovery::revoke_if_needed(&identity_dir, tor_proxy_revoke.as_deref()).await
        {
            tracing::debug!("Nostr revoke (non-fatal): {}", e);
        }

        // Publish presence-only to Nostr (DID + Nostr pubkey, NO onion address).
        // Onion addresses are exchanged privately via NIP-44 encrypted DMs.
        //
        // This is a heartbeat, not a one-shot: presence events carry a NIP-40
        // expiration of PRESENCE_TTL_SECS, so a node that stops re-publishing
        // ages out of discovery instead of lingering forever. First tick runs
        // immediately (preserving the old startup-publish behaviour); the
        // runtime toggle (nostr.set-discovery) is re-read every tick, so a
        // node switched on via the UI heartbeats too — not just ones with the
        // config flag baked in.
        {
            let identity_dir = config.data_dir.join("identity");
            let did =
                identity::did_key_from_pubkey_hex(&data.server_info.pubkey).unwrap_or_default();
            let version = data.server_info.version.clone();
            // Merged relay set (config + user-managed) — publish presence
            // where handshake peers actually read (2026-07-22 unification).
            let data_dir_for_relays = config.data_dir.clone();
            let config_relays = config.nostr_relays.clone();
            let config_flag = config.nostr_discovery_enabled;
            let tor_proxy = config.nostr_tor_proxy.clone();
            tokio::spawn(async move {
                const HEARTBEAT_SECS: u64 = 12 * 3600; // < PRESENCE_TTL_SECS/3
                loop {
                    let (enabled_override, display_name) =
                        nostr_handshake::discovery_overrides(&data_dir_for_relays).await;
                    let enabled = enabled_override.unwrap_or(config_flag);
                    if enabled {
                        let relays = crate::nostr_relays::merged_relay_list(
                            &data_dir_for_relays,
                            &config_relays,
                        )
                        .await;
                        if !relays.is_empty() {
                            if let Err(e) = nostr_handshake::publish_presence(
                                &identity_dir,
                                &did,
                                &version,
                                display_name.as_deref(),
                                &relays,
                                tor_proxy.as_deref(),
                            )
                            .await
                            {
                                tracing::debug!("Nostr presence publish (non-fatal): {}", e);
                            }
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(HEARTBEAT_SECS)).await;
                }
            });
        }
        info!(
            "🔑 Node identity: {} (pubkey: {}...)",
            identity.node_id(),
            &identity.pubkey_hex()[..16.min(identity.pubkey_hex().len())]
        );

        let identity = Arc::new(identity);

        // Create metrics store and spawn background collector
        let metrics_store = Arc::new(MetricsStore::with_data_dir(config.data_dir.clone()).await);
        let metrics_for_telemetry = metrics_store.clone();
        crate::monitoring::spawn_metrics_collector(
            metrics_store.clone(),
            Some(state_manager.clone()),
            Some(config.data_dir.clone()),
        );

        let api_handler = Arc::new(
            ApiHandler::new(
                config.clone(),
                state_manager.clone(),
                metrics_store,
                orchestrator,
                dev_orchestrator,
            )
            .await?,
        );

        // Background handshake poll: fetch inbound nostr peer requests every
        // 5 minutes instead of only when a user presses the Federation Poll
        // button (requests used to sit on relays unseen — 2026-07-22). The
        // handler's own discoverability gate makes this a no-op until the
        // user opts in.
        {
            let rpc = api_handler.rpc_handler().clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(300));
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                loop {
                    tick.tick().await;
                    rpc.background_handshake_poll().await;
                }
            });
        }

        // Initialize mesh networking service (if config has enabled: true)
        {
            let data_dir = config.data_dir.clone();
            let did =
                identity::did_key_from_pubkey_hex(&data.server_info.pubkey).unwrap_or_default();
            let pubkey_hex = identity.pubkey_hex();
            let signing_key = identity.signing_key();
            match crate::mesh::MeshService::new(&data_dir, signing_key, &did, &pubkey_hex).await {
                Ok(mut mesh_service) => {
                    // Pass the human-readable server name for mesh adverts
                    mesh_service.set_server_name(data.server_info.name.clone());
                    let mut mesh_config = crate::mesh::load_config(&data_dir)
                        .await
                        .unwrap_or_default();

                    // Auto-enable mesh if a radio is detected and no config exists
                    // yet. Only on a genuinely missing config: an existing file
                    // with enabled=false is an explicit operator decision (e.g.
                    // via mesh.configure) and force-re-enabling it on every boot
                    // made "disable mesh" impossible on any node with a radio
                    // plugged in.
                    if !mesh_config.enabled && !crate::mesh::config_file_exists(&data_dir) {
                        let devices = crate::mesh::detect_devices().await;
                        if !devices.is_empty() {
                            info!("📡 Auto-detected mesh radio: {:?} — enabling mesh", devices);
                            mesh_config.enabled = true;
                            mesh_config.device_path = Some(devices[0].clone());
                            if let Err(e) = crate::mesh::save_config(&data_dir, &mesh_config).await
                            {
                                warn!("Failed to persist auto-detected mesh config: {e:#}");
                            }
                        }
                    }

                    if mesh_config.enabled {
                        if let Err(e) = mesh_service.start() {
                            warn!("Mesh service start failed (non-fatal): {}", e);
                        } else {
                            info!("📡 Mesh networking started");

                            // Push mesh peer changes to open WebSockets instantly
                            // instead of the UI polling every 5s (#48): subscribe to
                            // mesh events and nudge the data-model revision (debounced)
                            // so /ws/db clients refetch peers on discovery/update.
                            let mut rx = mesh_service.state().event_tx.subscribe();
                            let sm = state_manager.clone();
                            tokio::spawn(async move {
                                use tokio::time::{Duration, Instant};
                                let mut last: Option<Instant> = None;
                                loop {
                                    match rx.recv().await {
                                        Ok(crate::mesh::MeshEvent::PeerDiscovered(_))
                                        | Ok(crate::mesh::MeshEvent::PeerUpdated(_)) => {
                                            // Debounce advert storms to ~2 Hz.
                                            if last
                                                .map(|t| t.elapsed() < Duration::from_millis(500))
                                                .unwrap_or(false)
                                            {
                                                continue;
                                            }
                                            last = Some(Instant::now());
                                            let (data, _) = sm.get_snapshot().await;
                                            sm.update_data(data).await;
                                        }
                                        Ok(_) => {}
                                        Err(tokio::sync::broadcast::error::RecvError::Lagged(
                                            _,
                                        )) => continue,
                                        Err(_) => break, // sender dropped → mesh stopped
                                    }
                                }
                            });
                        }
                    }
                    api_handler
                        .rpc_handler()
                        .set_mesh_service(mesh_service)
                        .await;

                    // Mesh-AI assistant (#50): deliver `!ai`-in-chat answers via
                    // the transport-aware send path. The listener can't route
                    // over federation itself (send_message needs the signing key
                    // + Tor client on MeshService), so it emits AssistChatReply
                    // and we fulfil it here through the shared MeshService —
                    // which POSTs over Tor for federation askers and falls back
                    // to LoRa for radio askers, recording the Sent bubble.
                    {
                        let mesh_arc = api_handler.rpc_handler().mesh_service_arc();
                        let mut reply_rx = {
                            let guard = mesh_arc.read().await;
                            guard.as_ref().map(|svc| svc.state().event_tx.subscribe())
                        };
                        if let Some(mut rx) = reply_rx.take() {
                            tokio::spawn(async move {
                                loop {
                                    match rx.recv().await {
                                        Ok(crate::mesh::MeshEvent::AssistChatReply {
                                            contact_id,
                                            text,
                                        }) => {
                                            let guard = mesh_arc.read().await;
                                            if let Some(svc) = guard.as_ref() {
                                                if let Err(e) =
                                                    svc.send_message(contact_id, &text).await
                                                {
                                                    warn!("AI chat reply send failed: {}", e);
                                                }
                                            }
                                        }
                                        Ok(_) => {}
                                        Err(tokio::sync::broadcast::error::RecvError::Lagged(
                                            _,
                                        )) => continue,
                                        Err(_) => break, // sender dropped → mesh stopped
                                    }
                                }
                            });
                        }
                    }
                    info!("📡 Mesh service initialized");
                }
                Err(e) => {
                    warn!("Mesh service init failed (non-fatal): {}", e);
                }
            }
        }

        // Initialize transport router (unified routing: mesh > lan > tor)
        // Hoisted so the FIPS seed-anchor loop below can auto-peer LAN-discovered
        // federation peers directly over FIPS (see that loop).
        let mut fips_peer_registry: Option<std::sync::Arc<crate::transport::PeerRegistry>> = None;
        {
            let data_dir = config.data_dir.clone();
            let did =
                identity::did_key_from_pubkey_hex(&data.server_info.pubkey).unwrap_or_default();
            let pubkey_hex = identity.pubkey_hex();
            let mesh_config = crate::mesh::load_config(&data_dir)
                .await
                .unwrap_or_default();
            let mesh_only = mesh_config.mesh_only_mode.unwrap_or(false);

            match crate::transport::PeerRegistry::load(&data_dir).await {
                Ok(registry) => {
                    let registry = std::sync::Arc::new(registry);
                    fips_peer_registry = Some(registry.clone());
                    let mut transports: Vec<Box<dyn crate::transport::NodeTransport>> = Vec::new();

                    // Tor transport (always register — availability checked dynamically)
                    transports.push(Box::new(crate::transport::tor::TorTransport::new(
                        &pubkey_hex,
                    )));

                    // Mesh transport (wraps the mesh service)
                    transports.push(Box::new(
                        crate::transport::mesh_transport::MeshTransport::new(
                            api_handler.rpc_handler().mesh_service_arc(),
                        ),
                    ));

                    // LAN transport (mDNS discovery). Advertise our FIPS npub in
                    // the TXT record so co-located peers can form a direct FIPS
                    // link (see `lan_fips_anchors`).
                    let local_fips_npub = crate::identity::fips_npub(&data_dir.join("identity"))
                        .await
                        .unwrap_or(None);
                    let mut lan = crate::transport::lan::LanTransport::new(
                        &did,
                        &pubkey_hex,
                        5678,
                        local_fips_npub,
                    );
                    match lan.start(registry.clone()) {
                        Ok(()) => info!("📡 LAN transport (mDNS) started"),
                        Err(e) => debug!("LAN transport init (non-fatal): {}", e),
                    }
                    transports.push(Box::new(lan));

                    let router = std::sync::Arc::new(crate::transport::TransportRouter::new(
                        transports, registry, mesh_only,
                    ));
                    api_handler.rpc_handler().set_transport_router(router).await;
                    info!("📡 Transport router initialized (mesh_only={})", mesh_only);
                }
                Err(e) => {
                    warn!("Transport router init failed (non-fatal): {}", e);
                }
            }
        }

        // Register Archipelago DWN protocols (background, non-blocking)
        {
            let data_dir = config.data_dir.clone();
            tokio::spawn(async move {
                if let Err(e) = register_dwn_protocols(&data_dir).await {
                    debug!("DWN protocol registration (non-fatal): {}", e);
                }
            });
        }

        // Periodic Tor address refresh (runs regardless of dev_mode)
        // Picks up hostname when Tor creates it after startup/rotation (30-60s delay)
        {
            let state = state_manager.clone();
            let identity_clone = identity.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    if let Err(e) = refresh_tor_address(&state, identity_clone.as_ref()).await {
                        debug!("Tor address refresh (non-fatal): {}", e);
                    }
                }
            });
        }

        // Periodic federation auto-sync. Pulls every federated peer's state on a
        // timer so renamed nodes and roster changes propagate WITHOUT a manual
        // "Sync" click. Each sync now fast-fails a dead FIPS path and falls back
        // to Tor (~3-5s), so a full pass over a handful of peers is quick.
        //
        // This is the ONE periodic federation sync loop. A second, near-identical
        // 30-minute loop used to run alongside it and was deleted in FED-02:
        // `git log` shows the 30-min loop landed first (8dd57bcb, 2026-04-19,
        // "periodic sync every 30 minutes") and this 90s loop landed later
        // (837cc028, 2026-06-19) describing itself as "new 90s periodic
        // federation auto-sync (none existed)" — the author simply hadn't seen
        // the existing one. The redundancy was accidental, not load-bearing, and
        // it doubled the write-race exposure against nodes.json that plan 01-01
        // locked down. The deleted loop's one unique behavior — refreshing the
        // live mesh peer table after a pass (#42) — is preserved at the tail of
        // this loop below.
        {
            let data_dir = config.data_dir.clone();
            let state = state_manager.clone();
            // Carried over from the deleted 30-min loop (#42): push the
            // names/roster learned during the pass into the live mesh peer
            // table so chat contacts refresh without a restart.
            let rpc = api_handler.rpc_handler().clone();
            tokio::spawn(async move {
                // Delay the first pass so Tor/onion publishing settles after boot.
                tokio::time::sleep(Duration::from_secs(20)).await;
                let mut interval = tokio::time::interval(Duration::from_secs(90));
                // Carried over from the deleted loop: after a stall (suspend,
                // heavy load) don't fire a burst of catch-up ticks back-to-back,
                // just resume the cadence from now.
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                // Peers that never list us back would otherwise be re-notified
                // every 90s forever (e.g. they hold us at Observer and their
                // exported hints are Trusted-only, so `they_list_us` can never
                // become true). Back off per peer, doubling toward a daily
                // re-assert; reset the moment they do list us.
                let mut notify_backoff: std::collections::HashMap<
                    String,
                    (u32, tokio::time::Instant),
                > = std::collections::HashMap::new();
                loop {
                    interval.tick().await;
                    // Zero federated nodes is a clean no-op: nothing is read
                    // further, nothing is written, and no sync error is recorded
                    // against anyone. Same for a failed load — we simply have no
                    // roster to act on this tick.
                    let nodes = match crate::federation::load_nodes(&data_dir).await {
                        Ok(n) if n.is_empty() => continue,
                        Ok(n) => n,
                        Err(e) => {
                            debug!(error = %e, "federation auto-sync: node load failed");
                            continue;
                        }
                    };
                    let (snap, _) = state.get_snapshot().await;
                    let identity_dir = data_dir.join("identity");
                    let node_identity =
                        match crate::identity::NodeIdentity::load_or_create(&identity_dir).await {
                            Ok(id) => id,
                            Err(_) => continue,
                        };
                    // Advertise the SAME key we sign with. server_info.pubkey
                    // is only seeded at boot; onboarding/seed-restore rewrite
                    // identity/node_key on disk without touching the snapshot,
                    // and a peer-joined that advertises the stale boot key
                    // while signing with the new seed-derived key is
                    // deterministically rejected ("Invalid signature") by
                    // every receiver, once per tick, forever (2026-08-16).
                    let local_pubkey = node_identity.pubkey_hex();
                    let local_did = match crate::identity::did_key_from_pubkey_hex(&local_pubkey) {
                        Ok(d) => d,
                        Err(_) => continue,
                    };
                    // Our own identity, for re-asserting membership to any peer
                    // that doesn't list us back (asymmetry self-heal, below).
                    let local_onion = snap.server_info.tor_address.clone().unwrap_or_default();
                    let local_name = snap.server_info.name.clone();
                    let local_fips_npub = crate::identity::fips_npub(&identity_dir)
                        .await
                        .unwrap_or(None);
                    let mut ok = 0usize;
                    let mut healed = 0usize;
                    for node in &nodes {
                        if node.trust_level == crate::federation::TrustLevel::Untrusted {
                            continue;
                        }
                        match crate::federation::sync_with_peer(&data_dir, node, &local_did, |b| {
                            node_identity.sign(b)
                        })
                        .await
                        {
                            Ok(state) => {
                                ok += 1;
                                // FED-02: clear any error this peer accumulated
                                // while it was unreachable, so the operator's
                                // sync-error badge disappears on recovery
                                // instead of sticking around forever.
                                crate::federation::record_sync_result(&data_dir, &node.did, Ok(()))
                                    .await
                                    .ok();
                                // Asymmetry self-heal: if this peer's exported
                                // trusted list doesn't include us, our original
                                // peer-joined never landed (e.g. it was sent
                                // before the reliable-notify fix, or the peer was
                                // down). Re-assert membership over the now
                                // FIPS-fast-failing/Tor path so they add us back.
                                // Without this, a node that joined everyone stays
                                // invisible to the whole fleet until a manual
                                // re-add (the "peer missing everywhere" case).
                                let they_list_us =
                                    state.federated_peers.iter().any(|h| h.did == local_did);
                                if they_list_us {
                                    notify_backoff.remove(&node.did);
                                } else if !local_onion.is_empty() {
                                    let now = tokio::time::Instant::now();
                                    let due = notify_backoff
                                        .get(&node.did)
                                        .map(|(_, next)| *next <= now)
                                        .unwrap_or(true);
                                    if due {
                                        crate::federation::notify_join(
                                            &node.onion,
                                            node.fips_npub.as_deref(),
                                            &local_did,
                                            &local_onion,
                                            &local_pubkey,
                                            local_fips_npub.as_deref(),
                                            local_name.as_deref(),
                                            // Re-assert at the level WE hold for
                                            // this peer; no invite token on heal.
                                            None,
                                            node.trust_level,
                                            |b| node_identity.sign(b),
                                        )
                                        .await
                                        .ok();
                                        healed += 1;
                                        let attempts = notify_backoff
                                            .get(&node.did)
                                            .map(|(a, _)| *a)
                                            .unwrap_or(0)
                                            + 1;
                                        let delay = (90u64 << attempts.min(10)).min(86_400);
                                        notify_backoff.insert(
                                            node.did.clone(),
                                            (attempts, now + Duration::from_secs(delay)),
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                debug!(peer = %node.did, error = %e, "federation auto-sync (non-fatal)");
                                // FED-02: persist the failure on the peer's own
                                // record too. The debug! line above is kept —
                                // persisting is additive, not a replacement for
                                // logs — but on its own it left a peer that
                                // hadn't synced in days looking identical in the
                                // UI to one that synced a minute ago. The stored
                                // message is the error's display string, bounded
                                // by record_sync_result to MAX_SYNC_ERROR_CHARS.
                                crate::federation::record_sync_result(
                                    &data_dir,
                                    &node.did,
                                    Err(format!("{e:#}")),
                                )
                                .await
                                .ok();
                            }
                        }
                    }
                    debug!(
                        synced = ok,
                        reasserted = healed,
                        total = nodes.len(),
                        "federation auto-sync pass complete"
                    );
                    // After syncing every peer, push the names/roster just
                    // learned (into nodes.json) into the live mesh peer table
                    // so chat contacts refresh without a restart (#42). Moved
                    // here from the deleted 30-min loop — this is the behavior
                    // that loop uniquely carried.
                    rpc.refresh_federation_mesh_peers().await;
                }
            });
        }

        // Periodic TollGate ecash sweep. tollgate-wrt keeps its own separate
        // Cashu wallet on the router — customer payments never land in this
        // node's wallet on their own, and its Lightning auto-payout config is
        // independent and easy to leave misconfigured. This drains whatever
        // TollGate has collected straight into the local wallet on a timer,
        // sidestepping Lightning payout configuration entirely.
        {
            let data_dir = config.data_dir.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let mut interval = tokio::time::interval(Duration::from_secs(300));
                loop {
                    interval.tick().await;
                    match crate::tollgate_sweep::sweep_once(&data_dir).await {
                        Ok(0) => {}
                        Ok(swept) => info!(sats = swept, "tollgate wallet sweep complete"),
                        Err(e) => debug!(error = %e, "tollgate wallet sweep (non-fatal)"),
                    }
                }
            });
        }

        // Initialize container scanner — discovers installed apps from Podman/Docker
        {
            let scanner = create_docker_scanner(&config).await?;
            let state = state_manager.clone();
            let identity_clone = identity.clone();
            let data_dir = config.data_dir.clone();
            let scan_kick = api_handler.rpc_handler().scan_kick();
            let scan_tick = api_handler.rpc_handler().scan_tick();

            // Initial scan (delayed to let crash recovery finish first)
            tokio::spawn(async move {
                // Brief delay for containers to stabilize after boot
                tokio::time::sleep(Duration::from_secs(3)).await;
                info!("🐳 Scanning containers...");
                // Tracks how many consecutive scans each container has been absent from.
                // Prevents UI flapping when podman intermittently returns incomplete results.
                let mut absence_tracker: HashMap<String, u32> = HashMap::new();
                // Tracks when each container first entered a transitional state
                // (Stopping / Starting / Restarting / ...). Used by the merge
                // loop below to ignore podman's live state during a pending
                // lifecycle op, and to break out if the spawned task dies
                // without ever writing a final state.
                let mut transitional_since: HashMap<String, Instant> = HashMap::new();
                let mut scan_backoff_until: Option<Instant> = None;
                if let Err(e) = scan_and_update_packages(
                    &scanner,
                    &state,
                    identity_clone.as_ref(),
                    &data_dir,
                    &mut absence_tracker,
                    &mut transitional_since,
                )
                .await
                {
                    error!("Failed to scan containers: {}", e);
                    if is_podman_scan_timeout(&e) {
                        scan_backoff_until = Some(Instant::now() + Duration::from_secs(30));
                        warn!("Podman container scan timed out; backing off scans for 30s");
                    }
                }
                // Bump the scan-completion counter so any caller waiting on a
                // kicked scan (install/update success path) can proceed.
                scan_tick.send_modify(|n| *n = n.wrapping_add(1));

                // Periodic scan every 60 seconds (only broadcasts if state changed).
                // Also wakes immediately when `scan_kick` fires — install/update
                // success paths poke it so the fresh manifest (with populated
                // interfaces) lands before they flip state to Running.
                // Uses an in-flight guard to skip scans when a previous one is still running
                let mut interval = tokio::time::interval(Duration::from_secs(60));
                // Skip missed ticks instead of catching up — prevents burst of scans
                // after a slow podman response (which causes DB lock storms)
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let scanning = std::sync::Arc::new(AtomicBool::new(false));
                loop {
                    tokio::select! {
                        _ = interval.tick() => {}
                        _ = scan_kick.notified() => {
                            debug!("Scan kicked by install/update success — running immediately");
                        }
                    }
                    if let Some(until) = scan_backoff_until {
                        if Instant::now() < until {
                            debug!("Skipping container scan — Podman scan backoff active");
                            scan_tick.send_modify(|n| *n = n.wrapping_add(1));
                            continue;
                        }
                    }
                    let Some(_scan_guard) = ContainerScanGuard::try_acquire(&scanning) else {
                        debug!("Skipping container scan — previous scan still in progress");
                        scan_tick.send_modify(|n| *n = n.wrapping_add(1));
                        continue;
                    };
                    let scan_result = scan_and_update_packages(
                        &scanner,
                        &state,
                        identity_clone.as_ref(),
                        &data_dir,
                        &mut absence_tracker,
                        &mut transitional_since,
                    )
                    .await;
                    if let Err(e) = scan_result {
                        error!("Failed to update containers: {}", e);
                        if is_podman_scan_timeout(&e) {
                            scan_backoff_until = Some(Instant::now() + Duration::from_secs(30));
                            warn!("Podman container scan timed out; backing off scans for 30s");
                        }
                    } else {
                        scan_backoff_until = None;
                    }
                    scan_tick.send_modify(|n| *n = n.wrapping_add(1));
                }
            });
        }

        // Peer health monitoring — check every 5 minutes
        {
            let state = state_manager.clone();
            let data_dir = config.data_dir.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(300));
                loop {
                    interval.tick().await;
                    if let Err(e) = check_peer_health(&state, &data_dir).await {
                        debug!("Peer health check (non-fatal): {}", e);
                    }
                }
            });
        }

        // LND channel-peer watchdog — every 2 minutes, reconnect the peers
        // of open channels that LND has not re-established on its own. LND's
        // reconnect logic gives up with a long backoff after repeated or
        // extended downtime (an app update, a reboot, reconciler churn), and
        // while the peer link is down BOTH endpoints keep the channel flagged
        // `disabled` in the routing graph — payments fail "no route" in both
        // directions while the node itself looks perfectly healthy. The
        // channel graph is desired state; this keeps it (framework-pt,
        // 2026-09-01: only channel unroutable ~17h after the 0.21.2 update).
        // No-ops quietly on nodes without LND. Per-peer retries are throttled
        // to 10 minutes so an unreachable peer is not hammered every pass.
        {
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(120));
                let mut last_attempt: HashMap<String, Instant> = HashMap::new();
                loop {
                    interval.tick().await;
                    match crate::container::lnd::reconnect_disconnected_channel_peers(
                        &mut last_attempt,
                        Duration::from_secs(600),
                    )
                    .await
                    {
                        Ok(0) => {}
                        Ok(n) => info!(n, "LND channel-peer watchdog reconnected channel peers"),
                        Err(e) => debug!("LND channel-peer watchdog (non-fatal): {}", e),
                    }
                }
            });
        }

        // FIPS seed-anchor apply loop — every 5 minutes we re-push the
        // configured seed anchors into the running fips daemon via
        // `fipsctl connect`. This keeps the mesh bootstrap resilient:
        // operators add cluster-local anchors in the UI, and a daemon
        // restart or a flaky public anchor can't strand the node.
        // First run is delayed 30s so fips has time to come up after
        // onboarding before we start dialing.
        {
            let data_dir = config.data_dir.clone();
            let fips_peer_registry = fips_peer_registry.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(30)).await;
                // Steady cadence, but retry fast right after a daemon restart:
                // regenerating fips.yaml (this build does, once, on first boot
                // after the OTA) restarts the fips daemon, and for a few seconds
                // `/run/fips/control.sock` is gone so every `fipsctl connect`
                // fails and the node islands until the next tick. Detect that
                // exact failure and retry in 15s instead of 5 min — bounded, so a
                // node with no fips daemon falls back to the steady cadence
                // rather than busy-looping.
                const STEADY: Duration = Duration::from_secs(300);
                const FAST: Duration = Duration::from_secs(15);
                const MAX_FAST_RETRIES: u32 = 8; // ≤2 min of fast retries/episode
                let mut fast_retries: u32 = 0;
                loop {
                    let mut daemon_restarting = false;
                    match crate::fips::anchors::load(&data_dir).await {
                        Ok(list) if !list.is_empty() => {
                            let results = crate::fips::anchors::apply(&list).await;
                            daemon_restarting = !results.is_empty()
                                && results
                                    .iter()
                                    .all(|r| !r.ok && r.message.contains("control.sock"));
                        }
                        Ok(_) => { /* no seed anchors configured yet */ }
                        Err(e) => {
                            tracing::debug!("Seed-anchor apply: load failed (non-fatal): {}", e)
                        }
                    }

                    // Auto-peer federation nodes we've discovered on the LAN
                    // directly over FIPS, so co-located peers don't depend on the
                    // (often flaky) global anchor's spanning tree to route to each
                    // other. For every peer the registry knows both a LAN address
                    // AND a FIPS npub for, dial it on its FIPS UDP transport port
                    // at its LAN IP. This is FIPS's own transport over the
                    // LAN — NOT Tailscale, NOT the HTTP/LAN messaging port. Pure
                    // FIPS. `fipsctl connect` is idempotent, so re-applying every
                    // tick just keeps the direct link warm; unknown/remote peers
                    // (no LAN address) are left to the anchor as before.
                    if let Some(reg) = fips_peer_registry.as_ref() {
                        // Hydrate FIPS npubs into the registry from federation
                        // storage (did-keyed). Peers discovered before the mDNS
                        // TXT `fips` key existed — or running builds that don't
                        // advertise it yet — would otherwise never satisfy the
                        // `fips_npub` requirement in lan_fips_anchors(), leaving
                        // direct LAN peering a no-op.
                        if let Ok(nodes) = crate::federation::load_nodes(&data_dir).await {
                            for n in &nodes {
                                if let Some(npub) = n.fips_npub.as_deref() {
                                    reg.set_fips_npub(&n.did, npub).await;
                                }
                            }
                        }
                        let direct = crate::fips::anchors::lan_fips_anchors(&reg.all_peers().await);
                        if !direct.is_empty() {
                            let _ = crate::fips::anchors::apply(&direct).await;
                        }

                        // A3.10 — endpoint fallback for direct peering. Record
                        // where currently-connected peers actually are (their
                        // transport_addr covers LAN, Tailscale, and WAN alike),
                        // then re-dial the last-known-good endpoint of every
                        // federation peer whose live paths are gone: not
                        // connected now, no LAN direct entry this tick. Escala-
                        // tion order is LAN → last-known-good → anchor tree;
                        // a stale candidate costs one bounded failed dial.
                        let connected = crate::fips::service::connected_peer_endpoints().await;
                        let known =
                            crate::fips::endpoints::record_connected(&data_dir, &connected).await;
                        let wanted: Vec<String> = reg
                            .all_peers()
                            .await
                            .iter()
                            .filter_map(|p| p.fips_npub.clone())
                            .collect();
                        let connected_npubs: Vec<String> =
                            connected.iter().map(|c| c.npub.clone()).collect();
                        let fallback = crate::fips::endpoints::fallback_anchors(
                            &known,
                            &wanted,
                            &connected_npubs,
                            &direct,
                        );
                        if !fallback.is_empty() {
                            tracing::info!(
                                count = fallback.len(),
                                "dialing last-known-good endpoints for disconnected federation peers"
                            );
                            let _ = crate::fips::anchors::apply(&fallback).await;
                        }
                    }

                    let next = if daemon_restarting && fast_retries < MAX_FAST_RETRIES {
                        fast_retries += 1;
                        FAST
                    } else {
                        fast_retries = 0;
                        STEADY
                    };
                    tokio::time::sleep(next).await;
                }
            });
        }

        // did:dht auto-refresh — re-publish DHT records every 2 hours
        if config.nostr_discovery_enabled {
            let data_dir = config.data_dir.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(7200));
                loop {
                    interval.tick().await;
                    let identity_dir = data_dir.join("identity");
                    let node_key_path = identity_dir.join("node_key");
                    if !node_key_path.exists() {
                        continue;
                    }
                    match tokio::fs::read(&node_key_path).await {
                        Ok(key_bytes) if key_bytes.len() == 32 => {
                            let mut seed = [0u8; 32];
                            seed.copy_from_slice(&key_bytes);
                            let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
                            match crate::network::did_dht::create_and_publish(&signing_key, &[])
                                .await
                            {
                                Ok(did) => tracing::info!(did = %did, "did:dht record refreshed"),
                                Err(e) => tracing::debug!("did:dht refresh (non-fatal): {}", e),
                            }
                        }
                        _ => {
                            tracing::debug!("did:dht refresh skipped: no valid node key");
                        }
                    }
                }
            });
        }

        // (FED-02) The redundant second periodic federation sync loop that used
        // to live here — 30-minute cadence, otherwise a near-duplicate of the
        // 90s loop above — has been deleted. See that loop's comment for the
        // git-history evidence that its overlap was accidental. Its one unique
        // behavior, `rpc.refresh_federation_mesh_peers()` after a completed
        // pass (#42), now runs at the tail of the surviving loop.
        //
        // Not carried over: the deleted loop's 5s per-peer stagger. Its stated
        // reason was "don't thunder the Tor SOCKS proxy with concurrent
        // connects", but both loops iterate peers sequentially and await each
        // sync, so there were never concurrent connects to stagger. Re-adding
        // it would only push a multi-peer pass past the 90s cadence.

        // Container health monitoring — auto-restart unhealthy containers
        // Respects webhook config: skips when disabled or ContainerCrash not subscribed
        crate::health_monitor::spawn_health_monitor(state_manager.clone(), config.data_dir.clone());

        // Periodic telemetry reporter (every 15 min when opted in)
        crate::monitoring::spawn_telemetry_reporter(
            metrics_for_telemetry,
            Some(state_manager.clone()),
            config.data_dir.clone(),
        );

        // Post-onboarding auto-activation for archipelago-fips. Runs once
        // at startup: if fips_key is on disk, install /etc/fips/fips.yaml
        // (schema-refreshed) and start the service. This removes the
        // need for a user-facing manual Start button — the node comes up
        // with FIPS running whenever the seed has been onboarded. Also
        // self-heals legacy raw-byte fips.key files (load_fips_keys
        // rewrites them as bech32 nsec the first time they're read).
        // Pre-onboarding nodes: ConditionPathExists on the service unit
        // + the `fips_key_exists` guard here keep this quiet.
        {
            let data_dir = config.data_dir.clone();
            tokio::spawn(async move {
                let identity_dir = data_dir.join("identity");
                if !crate::identity::fips_key_exists(&identity_dir) {
                    tracing::debug!("FIPS auto-activate skipped: fips_key not on disk");
                    return;
                }
                // Trigger the migration path in load_fips_keys so old raw-byte
                // key files are rewritten as bech32 before fips.yaml install.
                if let Err(e) = crate::identity::load_fips_keys(&identity_dir).await {
                    tracing::warn!("FIPS key load/migrate failed: {}", e);
                    return;
                }
                // Check if the installed fips.yaml matches what we'd
                // render now. If not, we need to restart the daemon after
                // reinstalling so it picks up schema changes (e.g. the
                // v1.7.25 re-addition of the TCP transport). Without this,
                // OTA'd nodes would be stuck on the old UDP-only config
                // until someone manually clicked Reconnect.
                let expected = crate::fips::config::render_config_yaml();
                let installed = tokio::fs::read_to_string("/etc/fips/fips.yaml").await.ok();
                let config_changed = installed.as_deref() != Some(expected.as_str());

                if let Err(e) = crate::fips::config::install(&identity_dir).await {
                    tracing::warn!("FIPS config install failed on startup: {}", e);
                    return;
                }
                if config_changed {
                    tracing::info!(
                        "FIPS config schema changed on disk — restarting daemon to pick up new transports"
                    );
                    // Restart whichever unit is actually supervising
                    // the daemon (archipelago-fips vs upstream fips).
                    let unit = crate::fips::service::active_unit().await;
                    if let Err(e) = crate::fips::service::restart(unit).await {
                        tracing::warn!(
                            "FIPS restart after config migration failed on {}: {} — user can retry via fips.reconnect",
                            unit,
                            e
                        );
                    }
                }
                let unit = crate::fips::service::activation_unit().await;
                if let Err(e) = crate::fips::service::activate(unit).await {
                    tracing::warn!(
                        "FIPS activate failed on startup via {}: {} — user can retry via fips.install RPC",
                        unit,
                        e
                    );
                    return;
                }
                tracing::info!("FIPS auto-activated on startup via {}", unit);
            });
        }

        Ok(Self {
            _config: config,
            _identity: identity,
            api_handler,
            _state_manager: state_manager,
        })
    }

    /// Serve with a graceful shutdown signal.
    ///
    /// `main_addr` is the primary listener (historically `127.0.0.1:5678`).
    /// The main listener always comes up on `main_addr`. The FIPS peer
    /// listener (path-filtered, bound to `fips0`'s ULA) is managed by a
    /// late-binding task that polls every 30s: if fips0 isn't up at
    /// startup (pre-onboarding install, legacy node pre-fips.install),
    /// it keeps trying until the interface appears — no archipelago
    /// restart required after the user activates FIPS.
    ///
    /// When `shutdown` completes, both listeners stop accepting and drain
    /// in-flight requests (bounded by `DRAIN_TIMEOUT`).
    pub async fn serve_with_shutdown(
        &self,
        main_addr: SocketAddr,
        shutdown: impl std::future::Future<Output = ()>,
    ) -> Result<()> {
        let active_connections = Arc::new(tokio::sync::Semaphore::new(1024));
        let (tx, rx_main) = tokio::sync::watch::channel(false);

        let main_task = tokio::spawn(accept_loop(
            self.api_handler.clone(),
            TcpListener::bind(main_addr).await?,
            active_connections.clone(),
            false, // main listener: no path filter
            rx_main,
            main_addr,
        ));

        // The mesh is IPv6-only: a phone reaching the node over its fips0
        // ULA lands on port 80 over v6, where a 0.0.0.0 listener never
        // answers — the UI was structurally unreachable over the mesh
        // (RST -> ERR_CONNECTION_ABORTED, confirmed 2026-07-26: v4:80 = 200,
        // v6:80 = refused). Mirror an IPv4-any main listener with a
        // V6ONLY [::] socket on the same port — v6-only so it coexists
        // with the v4 listener regardless of net.ipv6.bindv6only.
        let v4_any_port = match main_addr {
            SocketAddr::V4(v4) if v4.ip().is_unspecified() => Some(v4.port()),
            _ => None,
        };
        let v6_task = if let Some(port) = v4_any_port {
            let v6_addr =
                SocketAddr::new(std::net::IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED), port);
            match bind_v6_only(v6_addr) {
                Ok(listener) => {
                    info!("IPv6 web listener bound {} (mesh ULA reachable)", v6_addr);
                    Some(tokio::spawn(accept_loop(
                        self.api_handler.clone(),
                        listener,
                        active_connections.clone(),
                        false, // same semantics as the main listener
                        tx.subscribe(),
                        v6_addr,
                    )))
                }
                Err(e) => {
                    warn!(
                        "IPv6 web listener bind {} failed: {} — UI stays v4-only",
                        v6_addr, e
                    );
                    None
                }
            }
        } else {
            None
        };

        // Peer listener: late-binding so we don't need an archipelago
        // restart when fips0 comes up after onboarding.
        // App UIs over the mesh: rootless podman's port forwarder binds
        // IPv4 only for most apps, so a phone dialing [ULA]:8123 got
        // nothing even with the firewall open (HA/FileBrowser/Gitea/
        // Portainer/Pine all v4-only on 2026-07-26; a few bind [::]
        // themselves). Bridge each catalog launch port on the fips0 ULA
        // only. Binding wildcard [::]:port reserves the same host ports
        // Podman needs and can restart-loop apps that publish those ports.
        let relay_task = tokio::spawn(app_port_v6_relay_loop(tx.subscribe()));

        // The app gate: authentication in front of every app port, on every
        // address the node answers on. It can only claim a port whose app has
        // been pinned to loopback in its manifest — see appgate::listener for
        // why the rollout is necessarily per-app — and it logs a warning plus
        // records `GateStatus::unprotected` for every port it cannot claim,
        // so a partially-rolled-out gate is visible rather than silently
        // ineffective.
        let gate_task = tokio::spawn(crate::appgate::listener::run(
            self.api_handler.rpc_handler().app_gate.clone(),
            crate::appgate::listener::shared_status(),
            tx.subscribe(),
        ));

        let peer_task = tokio::spawn(peer_late_bind_loop(
            self.api_handler.clone(),
            active_connections.clone(),
            tx.subscribe(),
        ));

        shutdown.await;
        info!("Shutdown signal received, draining connections...");
        let _ = tx.send(true);

        // Wait up to 5s for in-flight requests.
        let drain_start = std::time::Instant::now();
        let drain_timeout = std::time::Duration::from_secs(5);
        while active_connections.available_permits() < 1024 {
            if drain_start.elapsed() > drain_timeout {
                warn!("Drain timeout reached, forcing shutdown");
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        let _ = main_task.await;
        if let Some(t) = v6_task {
            let _ = t.await;
        }
        relay_task.abort();
        // Aborted rather than awaited, like the relay loop: the sweep sleeps
        // up to a minute between ticks and its accept loops exit on the
        // shutdown watch, so awaiting it would stall the drain for no gain.
        gate_task.abort();
        let _ = peer_task.await;

        info!("Shutdown complete");
        Ok(())
    }
}

/// Bind a V6ONLY `[::]` TCP listener. V6ONLY is set explicitly so the
/// socket never claims the IPv4 side (which the main listener owns) —
/// without it, Linux hosts with `net.ipv6.bindv6only=0` would fail with
/// EADDRINUSE.
fn bind_v6_only(addr: SocketAddr) -> std::io::Result<tokio::net::TcpListener> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV6,
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )?;
    socket.set_only_v6(true)?;
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024)?;
    tokio::net::TcpListener::from_std(socket.into())
}

fn fips_app_relay_addr(ip: std::net::Ipv6Addr, port: u16) -> SocketAddr {
    SocketAddr::new(std::net::IpAddr::V6(ip), port)
}

/// IPv6→IPv4 relay for catalog app launch ports (see the spawn site for
/// why). Rescans every 60s so ports of freshly installed apps get bridged
/// without a daemon restart. Each relay binds to the fips0 ULA only and
/// forwards raw TCP to the same port on IPv4 loopback.
async fn app_port_v6_relay_loop(mut shutdown_rx: tokio::sync::watch::Receiver<bool>) {
    use std::collections::HashMap;
    let mut bridged: HashMap<u16, tokio::task::JoinHandle<()>> = HashMap::new();
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let Some(fips_ip) = crate::fips::iface::fips0_ula() else { continue };
                // This relay is a raw unauthenticated forward from the mesh to
                // the app's loopback, so it must refuse two classes of port:
                //
                // * `auth: gated` — the app gate owns the fips0 ULA for these,
                //   and bridging one would bypass the login page. Which of the
                //   two won the bind used to be a race.
                // * `auth: local` — host-local BY INTENT. Bridging one makes a
                //   port reachable from the whole mesh that was deliberately
                //   never externally reachable: nbxplorer 32838 answered HTTP
                //   200 over the mesh with no credential (a test node
                //   2026-08-04) purely because it appeared in the static port
                //   list below.
                //
                // Undeclared ports keep today's behaviour — silence is not an
                // instruction in either direction, and this relay predates the
                // declarations.
                let port_map = crate::appgate::identity::build_port_map();
                let gate_owned: std::collections::HashSet<u16> = port_map
                    .gated_ports()
                    .filter(|g| g.declared)
                    .map(|g| g.port)
                    .collect();
                for &port in crate::fips::app_ports::APP_LAUNCH_PORTS {
                    let withhold = if gate_owned.contains(&port) {
                        Some("port is now gate-owned")
                    } else if port_map.is_declared_local(port) {
                        Some("port is declared auth: local (host-local by intent)")
                    } else {
                        None
                    };
                    if let Some(reason) = withhold {
                        if let Some(handle) = bridged.remove(&port) {
                            handle.abort();
                            info!(port, reason, "v6 relay released a bridge");
                        }
                        continue;
                    }
                    if bridged.contains_key(&port) {
                        continue;
                    }
                    // ONLY bridge a port that a running app already answers on
                    // over IPv4. Binding [::]:port for an app that isn't
                    // installed is actively harmful: it makes that app's
                    // later install hit "address already in use", and the
                    // install's port-free step (`fuser -k <port>/tcp`) then
                    // kills THIS daemon, which holds the port — the exact
                    // cause of installs failing + apps vanishing on
                    // a test node 2026-07-27. No v4 listener → skip; the
                    // next rescan picks it up once the app is up.
                    let v4_up = tokio::time::timeout(
                        std::time::Duration::from_millis(300),
                        tokio::net::TcpStream::connect(("127.0.0.1", port)),
                    )
                    .await
                    .ok()
                    .and_then(|r| r.ok())
                    .is_some();
                    if !v4_up {
                        continue;
                    }
                    let addr = fips_app_relay_addr(fips_ip, port);
                    // EADDRINUSE = fipsd or another process already answers
                    // on this mesh address/port, so stay out of the way.
                    let Ok(listener) = bind_v6_only(addr) else { continue };
                    debug!("v6 relay bridging [{fips_ip}]:{port} -> 127.0.0.1:{port}");
                    let mut rx = shutdown_rx.clone();
                    let handle = tokio::spawn(async move {
                        loop {
                            tokio::select! {
                                accepted = listener.accept() => {
                                    let Ok((mut inbound, _)) = accepted else { break };
                                    tokio::spawn(async move {
                                        let Ok(mut outbound) = tokio::net::TcpStream::connect(
                                            ("127.0.0.1", port),
                                        )
                                        .await else { return };
                                        let _ = tokio::io::copy_bidirectional(
                                            &mut inbound,
                                            &mut outbound,
                                        )
                                        .await;
                                    });
                                }
                                _ = rx.changed() => break,
                            }
                        }
                    });
                    bridged.insert(port, handle);
                }
            }
            _ = shutdown_rx.changed() => return,
        }
    }
}

/// Poll every 30s for `fips0`'s ULA; when it appears, bind the peer
/// listener and run the normal accept loop. If the bind fails (port
/// already taken, permissions), log and keep retrying. Returns on
/// shutdown. First tick fires immediately so the hot path for
/// already-up fips0 is still zero-cost.
async fn peer_late_bind_loop(
    handler: Arc<ApiHandler>,
    active_connections: Arc<tokio::sync::Semaphore>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let Some(ip) = crate::fips::iface::fips0_ula() else { continue };
                let addr = SocketAddr::new(
                    std::net::IpAddr::V6(ip),
                    crate::fips::dial::PEER_PORT,
                );
                let listener = match TcpListener::bind(addr).await {
                    Ok(l) => l,
                    Err(e) => {
                        warn!("FIPS peer listener bind {} failed: {} — retrying in 30s", addr, e);
                        continue;
                    }
                };
                info!("FIPS peer listener bound {}", addr);
                // Serve until shutdown, a persistent accept failure, or a
                // fips0 ULA change. The listener must be REBINDABLE: a
                // daemon re-key tears fips0 down and brings it back with a
                // (possibly different) ULA, and the old one-shot bind left
                // the node inbound-dead over FIPS until process restart.
                tokio::select! {
                    _ = accept_loop(
                        handler.clone(),
                        listener,
                        active_connections.clone(),
                        true, // peer listener: apply path filter
                        shutdown_rx.clone(),
                        addr,
                    ) => {
                        if *shutdown_rx.borrow() { return; }
                        warn!("FIPS peer accept loop ended — rebinding");
                    }
                    _ = async {
                        loop {
                            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                            if crate::fips::iface::fips0_ula() != Some(ip) {
                                break;
                            }
                        }
                    } => {
                        info!("fips0 ULA changed — rebinding FIPS peer listener");
                        // Dropping the select arm cancels accept_loop and
                        // frees the socket; the outer loop rebinds fresh.
                    }
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() { return; }
            }
        }
    }
}

/// Whitelist of HTTP paths reachable via the peer-facing (FIPS) listener.
/// Every entry is an endpoint already protected by cryptographic auth
/// (ed25519 signature verification inside the handler, federation DID
/// headers checked by the content server, or JSON-RPC methods whose
/// handlers verify per-message signatures).
///
/// Anything not on this list returns 404 on the peer listener.
pub fn is_peer_allowed_path(path: &str) -> bool {
    // Exact matches
    matches!(
        path,
        "/health"
            | "/rpc/v1"
            | "/archipelago/node-message"
            | "/archipelago/mesh-typed"
            | "/dwn"
            | "/transport/inbox"
            // Content *catalog* — the peer-browse entry point. This is the
            // exact path `/content` (no trailing slash); the prefix match
            // below only covers `/content/<id>` item fetches, so without
            // this the catalog 404s over the mesh and `content.browse-peer`
            // fails with "Peer returned error: 404 Not Found" (and never
            // falls back to Tor, since a 404 is a successful HTTP exchange).
            | "/content"
    )
    // Prefix-matched content endpoints (peer file browse + fetch)
        || path.starts_with("/content/")
        // Mesh file sharing — blob fetch by CID, signature-gated in the
        // handler. Absent from this list it 404'd over FIPS and the feature
        // was 100% Tor by construction.
        || path.starts_with("/blob/")
        // DWN sync — /dwn/health is step 1 of every sync; same story.
        || path.starts_with("/dwn/")
}

async fn accept_loop(
    handler: Arc<ApiHandler>,
    listener: TcpListener,
    active_connections: Arc<tokio::sync::Semaphore>,
    peer_only: bool,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    local_addr: SocketAddr,
) {
    // Consecutive accept-error tracking: a fips0 teardown/re-key leaves the
    // peer listener's socket permanently broken — `continue`-ing forever
    // made the node inbound-dead over FIPS until process restart. After a
    // burst of consecutive errors the peer accept loop returns so its
    // caller (peer_late_bind_loop) can rebind on the current ULA.
    let mut consecutive_errors: u32 = 0;
    loop {
        tokio::select! {
            result = listener.accept() => {
                let (stream, peer_addr) = match result {
                    Ok(c) => { consecutive_errors = 0; c }
                    Err(e) => {
                        error!("{} accept error: {}", local_addr, e);
                        consecutive_errors += 1;
                        if peer_only && consecutive_errors >= 10 {
                            warn!("{} accept failing persistently — returning for rebind", local_addr);
                            return;
                        }
                        // Don't hot-loop on a dead socket.
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        continue;
                    }
                };
                let handler = handler.clone();
                // NEVER park the accept loop on the connection budget.
                // `acquire_owned().await` here froze accept() entirely when
                // permits drained — and permits drained because half-open
                // clients and hung upstreams held them forever (the .228
                // session-flapping / CLOSE-WAIT `inode: 0` signature). Shed
                // load instead: accept, answer 503, close.
                let permit = match active_connections.clone().try_acquire_owned() {
                    Ok(p) => p,
                    Err(_) => {
                        warn!(
                            "{} connection budget exhausted — shedding {}",
                            local_addr, peer_addr
                        );
                        tokio::spawn(async move {
                            use tokio::io::AsyncWriteExt;
                            let mut stream = stream;
                            let _ = tokio::time::timeout(
                                std::time::Duration::from_secs(5),
                                stream.write_all(
                                    b"HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
                                ),
                            )
                            .await;
                            let _ = stream.shutdown().await;
                        });
                        continue;
                    }
                };
                tokio::spawn(async move {
                    let _permit = permit;
                    // Set when a request carries an Upgrade header (websocket):
                    // upgraded connections are legitimately long-lived and are
                    // exempt from the non-upgraded connection deadline below.
                    let upgraded = Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let upgraded_flag = upgraded.clone();
                    let service = service_fn(move |mut req: hyper::Request<hyper::Body>| {
                        let handler = handler.clone();
                        if req.headers().contains_key(hyper::header::UPGRADE) {
                            upgraded_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        async move {
                            // Record the TCP peer so rate limiting only trusts
                            // forwarded headers on loopback (nginx) connections.
                            req.extensions_mut()
                                .insert(crate::api::rpc::PeerAddr(peer_addr));
                            if peer_only && !is_peer_allowed_path(req.uri().path()) {
                                let resp = hyper::Response::builder()
                                    .status(hyper::StatusCode::NOT_FOUND)
                                    .body(hyper::Body::empty())
                                    .expect("static response builds");
                                return Ok::<_, std::io::Error>(resp);
                            }
                            handler
                                .handle_request(req)
                                .await
                                .map_err(|e| std::io::Error::other(format!("{}", e)))
                        }
                    });
                    // header_read_timeout: a client that connects and never
                    // sends a request (slowloris / half-open) is dropped
                    // instead of holding a permit until the heat death of the
                    // node. Long RPCs are safe — the clock only covers header
                    // read.
                    let conn = Http::new()
                        .http1_keep_alive(false)
                        .http1_header_read_timeout(std::time::Duration::from_secs(30))
                        .serve_connection(stream, service)
                        .with_upgrades();
                    tokio::pin!(conn);
                    // Deadline watchdog for NON-upgraded connections. With
                    // keep-alive off a plain connection serves one exchange;
                    // 15 min bounds even the slowest legitimate RPC/stream
                    // while guaranteeing a hung upstream can't hold a permit
                    // forever. Upgraded (websocket) connections are exempt.
                    const NON_UPGRADED_DEADLINE: std::time::Duration =
                        std::time::Duration::from_secs(900);
                    let started = std::time::Instant::now();
                    let watchdog = async {
                        loop {
                            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                            if !upgraded.load(std::sync::atomic::Ordering::Relaxed)
                                && started.elapsed() >= NON_UPGRADED_DEADLINE
                            {
                                return;
                            }
                        }
                    };
                    tokio::select! {
                        r = &mut conn => {
                            if let Err(e) = r {
                                error!("Error serving connection from {}: {}", peer_addr, e);
                            }
                        }
                        _ = watchdog => {
                            warn!(
                                "connection from {} exceeded {}s without completing or upgrading — dropping",
                                peer_addr,
                                NON_UPGRADED_DEADLINE.as_secs()
                            );
                        }
                    }
                });
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    return;
                }
            }
        }
    }
}

async fn create_docker_scanner(config: &Config) -> Result<DockerPackageScanner> {
    let user = std::env::var("USER").unwrap_or_else(|_| "archipelago".to_string());

    let runtime: Arc<dyn archipelago_container::ContainerRuntime> = match &config.container_runtime
    {
        ContainerRuntime::Podman => {
            Arc::new(archipelago_container::PodmanRuntime::new(user.clone()))
        }
        ContainerRuntime::Docker => {
            Arc::new(archipelago_container::DockerRuntime::new(user.clone()))
        }
        ContainerRuntime::Auto => {
            Arc::new(archipelago_container::AutoRuntime::new(user.clone()).await?)
        }
    };

    Ok(DockerPackageScanner::new(runtime))
}

async fn refresh_tor_address(state: &StateManager, identity: &NodeIdentity) -> Result<()> {
    let tor_addr = docker_packages::read_tor_address("archipelago").await;
    let (current_data, _) = state.get_snapshot().await;
    if tor_addr != current_data.server_info.tor_address {
        let mut data = current_data;
        data.server_info.tor_address = tor_addr.clone();
        data.server_info.node_address = tor_addr.as_ref().map(|t| identity.node_address(t));
        state.update_data(data).await;
        if let Some(ref addr) = tor_addr {
            info!("🔒 Tor address updated: {}", addr);
        }
    }
    Ok(())
}

/// Number of consecutive absent scans before removing a container from state.
/// 3 scans × 30s = 90 seconds of absence before removal.
const CONTAINER_ABSENCE_THRESHOLD: u32 = 3;

/// Maximum time a package entry may remain stuck in a transitional state
/// before the scan loop overrides it with podman's live state.
///
/// Rationale: the longest single-container stop timeout is bitcoin-core at
/// 600s. 2× that gives the spawned task ample margin before we assume it
/// died (panic, OOM, process restart mid-stop) and fall back to the
/// scanner's authoritative view. Applies to all transitional variants.
const TRANSITIONAL_STUCK_TIMEOUT: Duration = Duration::from_secs(120);

/// Multi-container installs can legitimately spend several minutes before the
/// primary user-facing container exists. BTCPay, for example, pulls/starts
/// Postgres and NBXplorer before `btcpay-server`; do not erase its installing
/// card just because the primary container is absent during that setup window.
const INSTALLING_STUCK_TIMEOUT: Duration = Duration::from_secs(20 * 60);

fn transitional_stuck_timeout(state: &crate::data_model::PackageState) -> Duration {
    use crate::data_model::PackageState::*;
    match state {
        Installing | Starting | Restarting => INSTALLING_STUCK_TIMEOUT,
        _ => TRANSITIONAL_STUCK_TIMEOUT,
    }
}

/// Returns true if `state` is one of the transitional variants that a
/// `spawn_transitional`-style background task owns. While such a state is
/// set, the package scanner must not overwrite it with whatever podman
/// reports (see `merge_preserving_transitional`).
fn is_transitional(state: &crate::data_model::PackageState) -> bool {
    use crate::data_model::PackageState::*;
    matches!(
        state,
        Installing
            | Stopping
            | Starting
            | Restarting
            | Updating
            | Removing
            | CreatingBackup
            | RestoringBackup
            | BackingUp
    )
}

fn absent_transitional_replacement(
    state: &crate::data_model::PackageState,
) -> Option<crate::data_model::PackageState> {
    match state {
        // A stop operation is complete once the container record disappears.
        // Do not leave the app card wedged in "Stopping..." just because the
        // background task died or the backend restarted before it wrote back.
        crate::data_model::PackageState::Stopping => Some(crate::data_model::PackageState::Stopped),
        _ => None,
    }
}

/// Merge a fresh scan entry `fresh` into `existing` while preserving
/// `existing.state` (which is transitional — the RPC spawn task owns it).
/// Non-state observability fields are taken from `fresh` so the UI still
/// sees live health / exit_code / lan_address readings during a transition.
fn merge_preserving_transitional(
    existing: &crate::data_model::PackageDataEntry,
    fresh: &crate::data_model::PackageDataEntry,
    user_stop_requested: bool,
) -> crate::data_model::PackageDataEntry {
    let state = match (&existing.state, &fresh.state) {
        // A user-initiated stop must keep showing Stopping while podman still
        // reports Running. Repair/restart transitions do not have a user-stop
        // marker, so a fresh Running scan means the app recovered.
        (crate::data_model::PackageState::Stopping, crate::data_model::PackageState::Running)
            if !user_stop_requested =>
        {
            fresh.state.clone()
        }
        // A user-initiated stop whose container podman now reports settled
        // (the scanner maps exited+user-stopped → Stopped) has visibly
        // completed — report it. The stop worker still owns cleanup, but it
        // can trail the actual container exit by minutes when it is queued
        // behind the orchestrator's per-app lock (reconcile host-port repair
        // holds it through multi-minute stability waits). Holding the card
        // (and the lifecycle gate) in "Stopping" that whole time reports a
        // completed stop as stuck (vaultwarden/jellyfin, gate runs C/D,
        // .228 2026-07-09).
        (crate::data_model::PackageState::Stopping, crate::data_model::PackageState::Stopped)
            if user_stop_requested =>
        {
            fresh.state.clone()
        }
        // Same reasoning for start: once podman reports Running, the start
        // has visibly succeeded. The start worker keeps Starting through its
        // full readiness wait (host-port probe budgets reach 420s for
        // uptime-kuma — longer than any UI/test patience) even though the
        // container is up and its live health is already shown separately
        // (uptime-kuma, gate run E). Restarting is deliberately NOT mapped:
        // mid-restart Running readings are the pre-stop container.
        (crate::data_model::PackageState::Starting, crate::data_model::PackageState::Running) => {
            fresh.state.clone()
        }
        // Removing with a live running container is stale: uninstall either
        // failed or Archipelago restarted before the spawned task could revert
        // state. Let the scanner recover the UI immediately instead of
        // keeping the app wedged in Removing for 20 minutes.
        (crate::data_model::PackageState::Removing, crate::data_model::PackageState::Running) => {
            fresh.state.clone()
        }
        _ => existing.state.clone(),
    };

    crate::data_model::PackageDataEntry {
        state: state.clone(),
        // install_progress and uninstall_stage are also owned by the
        // initiating op (same reason as state) — keep them.
        install_progress: existing.install_progress.clone(),
        uninstall_stage: existing.uninstall_stage.clone(),
        // Everything else comes from the fresh scan.
        ui_ready: if state == crate::data_model::PackageState::Running {
            fresh.ui_ready
        } else {
            Some(false)
        },
        health: fresh.health.clone(),
        exit_code: fresh.exit_code,
        static_files: fresh.static_files.clone(),
        manifest: fresh.manifest.clone(),
        installed: fresh.installed.clone(),
        available_update: fresh.available_update.clone(),
    }
}

/// Package ids whose `Restarting` state was written by the scanner's
/// pending-boot-start overlay (not by an RPC restart task). For these, the
/// scan is the owner: once podman reports a settled state and the id is no
/// longer queued for a boot start, the fresh state wins immediately instead
/// of being preserved for the transitional-stuck timeout.
static SCANNER_RESTARTING: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashSet<String>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

fn take_scanner_restarting(id: &str) -> bool {
    SCANNER_RESTARTING
        .lock()
        .map(|mut set| set.remove(id))
        .unwrap_or(false)
}

fn is_podman_scan_timeout(error: &anyhow::Error) -> bool {
    let msg = format!("{:#}", error);
    msg.contains("podman ps") && msg.contains("timed out")
}

async fn scan_and_update_packages(
    scanner: &DockerPackageScanner,
    state: &StateManager,
    identity: &NodeIdentity,
    data_dir: &std::path::Path,
    absence_tracker: &mut HashMap<String, u32>,
    transitional_since: &mut HashMap<String, Instant>,
) -> Result<()> {
    let (before_scan, _) = state.get_snapshot().await;
    let mut packages = scanner
        .scan_containers(data_dir, &before_scan.package_data)
        .await?;
    let user_stopped = crate::crash_recovery::load_user_stopped(data_dir).await;
    for (id, pkg) in packages.iter_mut() {
        if pkg.state == crate::data_model::PackageState::Exited && user_stopped.contains(id) {
            pkg.state = crate::data_model::PackageState::Stopped;
            pkg.exit_code = None;
        }
        // A down container that boot recovery / the reconciler is queued to
        // start is "Restarting", not "Stopped" — after a reboot the sequential
        // recovery pass can take minutes to reach heavyweights, and telling
        // the user their app stopped when it's about to come back is wrong.
        // Ids overlaid here are recorded in SCANNER_RESTARTING so the merge
        // below knows this Restarting is scanner-authored (resolve it as soon
        // as podman reports a settled state) and not owned by an RPC restart
        // task (whose transitional state must be preserved).
        // …but never for a user-stopped app: the stop marker is written
        // before the stop runs, and a reconcile pass that queued the app
        // moments earlier must not repaint the user's deliberate Stopped as
        // Restarting (gate iteration-5 vaultwarden stop-wait, 2026-07-09 —
        // the overlay held 'restarting' past the 120s window).
        if matches!(
            pkg.state,
            crate::data_model::PackageState::Stopped | crate::data_model::PackageState::Exited
        ) && crate::crash_recovery::is_pending_boot_start(id)
            && !user_stopped.contains(id)
        {
            pkg.state = crate::data_model::PackageState::Restarting;
            pkg.exit_code = None;
            if let Ok(mut set) = SCANNER_RESTARTING.lock() {
                set.insert(id.clone());
            }
        }
    }
    normalize_reachable_package_health(&mut packages).await;

    let (current_data, _) = state.get_snapshot().await;
    let tor_addr = docker_packages::read_tor_address("archipelago").await;
    let tor_changed = tor_addr != current_data.server_info.tor_address;
    let first_scan = !current_data.server_info.status_info.containers_scanned;

    // Check if update scheduler has found an available update
    let update_available = crate::update::load_state(std::path::Path::new("/var/lib/archipelago"))
        .await
        .map(|s| s.available_update.is_some())
        .unwrap_or(false);
    let update_changed = update_available != current_data.server_info.status_info.updated;

    // The durable installed set is the truth the scan must never contradict:
    // quadlet renders --rm, so every stop DELETES the container and a scan mid
    // stop->start legitimately sees nothing where an installed app lives.
    let installed_registry = crate::crash_recovery::load_installed_apps(data_dir).await;
    let user_uninstalled = crate::crash_recovery::load_user_uninstalled(data_dir).await;

    // Empty scan result = podman failure or timeout, preserve existing state.
    // The first scan is NOT exempt when the durable registry says apps exist:
    // the daemon restarts mid-churn (gate runs, OTAs), and publishing that
    // first empty scan blanked the whole My Apps map — the dashboard showed a
    // node with zero apps until the next scan (observed twice, 2026-08-09,
    // once at load ~2). Better to keep saying "scanning…" than to say "empty".
    if packages.is_empty() && (!first_scan || !installed_registry.is_empty()) {
        if tor_changed || update_changed {
            state
                .mutate_data(|data| {
                    data.server_info.tor_address = tor_addr.clone();
                    data.server_info.node_address =
                        tor_addr.as_ref().map(|t| identity.node_address(t));
                    data.server_info.status_info.updated = update_available;
                })
                .await;
        }
        return Ok(());
    }

    // Merge scan results with current state instead of full replacement.
    // This prevents containers from vanishing when podman intermittently
    // returns incomplete results under heavy load.
    let mut merged = current_data.package_data.clone();
    let mut changed = false;

    // Update/add containers found in this scan.
    //
    // Transitional states (Stopping, Starting, Restarting, Installing,
    // Updating, Removing, backup variants) are owned by the RPC spawn_task
    // that initiated the operation — podman's live state during the op is
    // meaningless ("running" during a graceful stop, "exited" during a
    // restart, etc.) and must not be written back. See
    // `merge_preserving_transitional` for the exact rule.
    //
    // Escape hatch: if a package has been in a transitional state for
    // longer than TRANSITIONAL_STUCK_TIMEOUT we assume the spawned task
    // died without cleanup and let the scan override it.
    let now = Instant::now();
    for (id, pkg) in &packages {
        if user_uninstalled.contains(id)
            || user_uninstalled.contains(&format!("archy-{id}"))
            || (before_scan.package_data.contains_key(id)
                && !current_data.package_data.contains_key(id))
        {
            continue;
        }
        absence_tracker.remove(id);
        let existing = merged.get(id);
        let overwrite = match existing {
            // Scanner-authored Restarting (the pending-boot-start overlay)
            // resolves as soon as the fresh scan reports anything else: the
            // scan is its owner — no RPC task will ever write a final state
            // back. Without this, a successfully recovered container would
            // sit wedged in "Restarting" until the 20-minute stuck timeout.
            Some(existing_entry)
                if existing_entry.state == crate::data_model::PackageState::Restarting
                    && pkg.state != crate::data_model::PackageState::Restarting
                    && take_scanner_restarting(id) =>
            {
                transitional_since.remove(id);
                true
            }
            Some(existing_entry) if is_transitional(&existing_entry.state) => {
                let entered = *transitional_since.entry(id.clone()).or_insert(now);
                let timeout = transitional_stuck_timeout(&existing_entry.state);
                let stuck = now.duration_since(entered) > timeout;
                if stuck {
                    warn!(
                        "Container {} stuck in {:?} for >{}s; overriding with scan state {:?}",
                        id,
                        existing_entry.state,
                        timeout.as_secs(),
                        pkg.state
                    );
                    transitional_since.remove(id);
                    true
                } else {
                    // Keep existing transitional state, but merge non-state
                    // observability fields (health, exit_code, lan_address
                    // via installed) from the fresh scan so the UI still
                    // sees live readings.
                    let merged_entry = merge_preserving_transitional(
                        existing_entry,
                        pkg,
                        user_stopped.contains(id),
                    );
                    if existing.cloned() != Some(merged_entry.clone()) {
                        merged.insert(id.clone(), merged_entry);
                        changed = true;
                    }
                    false
                }
            }
            Some(_) => {
                // Not transitional: the side-table may hold a stale entry
                // from a previous transition on this id; drop it.
                transitional_since.remove(id);
                existing != Some(pkg)
            }
            None => {
                transitional_since.remove(id);
                true
            }
        };
        if overwrite && merged.get(id) != Some(pkg) {
            merged.insert(id.clone(), pkg.clone());
            changed = true;
        }
    }

    // Track containers in state but missing from this scan.
    // Only remove after CONTAINER_ABSENCE_THRESHOLD consecutive absent scans.
    let current_ids: Vec<String> = merged.keys().cloned().collect();
    for id in current_ids {
        if !packages.contains_key(&id) {
            // Don't evict packages mid-transition: Installing/Updating/Removing
            // legitimately have no live container yet (image still pulling) or
            // briefly (during recreate). The absence-eviction here was racing
            // installs and removing apps from the UI 14s in. The transitional
            // owner (spawn_task) is responsible for clearing state, not us.
            if let Some(entry) = merged.get(&id) {
                if is_transitional(&entry.state) {
                    if let Some(replacement) = absent_transitional_replacement(&entry.state) {
                        let mut updated = entry.clone();
                        updated.state = replacement;
                        updated.health = None;
                        updated.exit_code = None;
                        updated.install_progress = None;
                        updated.uninstall_stage = None;
                        merged.insert(id.clone(), updated);
                        transitional_since.remove(&id);
                        absence_tracker.remove(&id);
                        changed = true;
                        continue;
                    }
                    let entered = *transitional_since.entry(id.clone()).or_insert(now);
                    let timeout = transitional_stuck_timeout(&entry.state);
                    if now.duration_since(entered) > timeout {
                        warn!(
                            "Container {} stuck in {:?} and absent for >{}s; removing stale transitional state",
                            id,
                            entry.state,
                            timeout.as_secs()
                        );
                        merged.remove(&id);
                        transitional_since.remove(&id);
                        changed = true;
                    }
                    absence_tracker.remove(&id);
                    continue;
                }
                // Quadlet-generated units run containers with `--rm`, so a
                // clean user stop removes the Podman record. Keep the package
                // visible as Stopped while the user-stopped marker exists so
                // package.start can recreate it via systemd/Quadlet.
                if entry.state == crate::data_model::PackageState::Stopped
                    && user_stopped.contains(&id)
                {
                    absence_tracker.remove(&id);
                    continue;
                }
            }
            let count = absence_tracker.entry(id.clone()).or_insert(0);
            *count += 1;
            if *count >= CONTAINER_ABSENCE_THRESHOLD {
                // An app the durable registry says is installed (and the user
                // has not uninstalled) must NEVER be dropped from the map just
                // because its container is momentarily gone — with --rm that
                // is every restart's normal window. Dropping it here is what
                // made dashboard tiles vanish mid-restart and gate waits read
                // 'absent' (grafana, 2026-08-09, at load ~2). Hold it as
                // Stopped instead; the next scan that sees the container
                // restores the live state, and desired-state recovery still
                // recreates genuinely lost containers.
                if installed_registry.contains(&id) && !user_uninstalled.contains(&id) {
                    if let Some(entry) = merged.get(&id) {
                        if entry.state != crate::data_model::PackageState::Stopped {
                            let mut held = entry.clone();
                            held.state = crate::data_model::PackageState::Stopped;
                            held.health = None;
                            held.exit_code = None;
                            merged.insert(id.clone(), held);
                            changed = true;
                        }
                    }
                    absence_tracker.remove(&id);
                    continue;
                }
                debug!(
                    "Removing {} from state after {} consecutive absent scans",
                    id, count
                );
                merged.remove(&id);
                absence_tracker.remove(&id);
                transitional_since.remove(&id);
                changed = true;
            }
        }
    }

    if changed || tor_changed || first_scan || update_changed {
        state
            .mutate_data(|data| {
                // A lifecycle operation may have started/finished while this scan
                // awaited probes or disk I/O. Never overwrite that newer entry or
                // resurrect one that an uninstall removed in the meantime.
                apply_scanned_packages(&mut data.package_data, &current_data.package_data, &merged);
                data.server_info.tor_address = tor_addr.clone();
                data.server_info.node_address = tor_addr.as_ref().map(|t| identity.node_address(t));
                data.server_info.status_info.containers_scanned = true;
                data.server_info.status_info.updated = update_available;
            })
            .await;
    }

    Ok(())
}

fn apply_scanned_packages(
    latest: &mut HashMap<String, crate::data_model::PackageDataEntry>,
    base: &HashMap<String, crate::data_model::PackageDataEntry>,
    scanned: &HashMap<String, crate::data_model::PackageDataEntry>,
) {
    for (id, fresh) in scanned {
        if latest.get(id) == base.get(id) {
            latest.insert(id.clone(), fresh.clone());
        }
    }
    for id in base.keys() {
        if !scanned.contains_key(id) && latest.get(id) == base.get(id) {
            latest.remove(id);
        }
    }
}

async fn normalize_reachable_package_health(
    packages: &mut HashMap<String, crate::data_model::PackageDataEntry>,
) {
    for (id, pkg) in packages.iter_mut() {
        if pkg.state != crate::data_model::PackageState::Running {
            continue;
        }
        if !matches!(pkg.health.as_deref(), Some("starting" | "unhealthy" | "1")) {
            continue;
        }
        let Some(port) = pkg
            .installed
            .as_ref()
            .and_then(|i| i.interface_addresses.get("main"))
            .and_then(|a| a.lan_address.as_deref())
            .and_then(port_from_url)
            .or_else(|| fallback_package_port(id))
        else {
            continue;
        };
        if frontend_port_http_ready(port).await {
            debug!(app_id = %id, port, "normalizing reachable package health to healthy");
            pkg.health = Some("healthy".to_string());
            ensure_main_lan_address(pkg, port);
        }
    }
}

async fn frontend_port_http_ready(port: u16) -> bool {
    let Ok(Ok(mut stream)) = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::net::TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    else {
        return false;
    };

    let request = b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    if stream.write_all(request).await.is_err() {
        return false;
    }

    let mut buf = [0u8; 64];
    let Ok(Ok(n)) = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf)).await
    else {
        return false;
    };
    if n == 0 {
        return false;
    }

    let head = String::from_utf8_lossy(&buf[..n]);
    head.starts_with("HTTP/1.1 2")
        || head.starts_with("HTTP/1.1 3")
        || head.starts_with("HTTP/1.0 2")
        || head.starts_with("HTTP/1.0 3")
}

fn ensure_main_lan_address(pkg: &mut crate::data_model::PackageDataEntry, port: u16) {
    let Some(installed) = pkg.installed.as_mut() else {
        return;
    };
    let main = installed
        .interface_addresses
        .entry("main".to_string())
        .or_insert_with(|| crate::data_model::InterfaceAddress {
            tor_address: String::new(),
            lan_address: None,
        });
    if main.lan_address.is_none() {
        main.lan_address = Some(format!("http://localhost:{port}"));
    }
}

fn fallback_package_port(app_id: &str) -> Option<u16> {
    match app_id {
        "fedimint" | "fedimintd" => Some(8175),
        "fedimint-clientd" => Some(8178),
        "barkd" => Some(3535),
        "filebrowser" => Some(8083),
        "indeedhub" => Some(7778),
        "nginx-proxy-manager" => Some(8081),
        "nostr-rs-relay" => Some(18081),
        _ => None,
    }
}

fn port_from_url(url: &str) -> Option<u16> {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
    let port = host_port.rsplit_once(':')?.1;
    port.parse::<u16>().ok()
}

/// Register Archipelago DWN protocols on startup.
async fn register_dwn_protocols(data_dir: &std::path::Path) -> Result<()> {
    use crate::network::dwn_store::{DwnStore, ProtocolDefinition};

    let protocols = [
        ("https://archipelago.dev/protocols/node-identity/v1", true),
        ("https://archipelago.dev/protocols/file-catalog/v1", true),
        ("https://archipelago.dev/protocols/federation/v1", false),
        ("https://archipelago.dev/protocols/app-deploy/v1", false),
    ];

    let store = DwnStore::new(data_dir).await?;
    let existing = store.list_protocols().await?;
    let existing_uris: std::collections::HashSet<String> =
        existing.iter().map(|p| p.protocol.clone()).collect();

    let mut registered = 0;
    for (uri, published) in &protocols {
        if existing_uris.contains(*uri) {
            continue;
        }
        let def = ProtocolDefinition {
            protocol: uri.to_string(),
            published: *published,
            types: std::collections::HashMap::new(),
            structure: std::collections::HashMap::new(),
            date_registered: chrono::Utc::now().to_rfc3339(),
        };
        store.register_protocol(&def).await?;
        registered += 1;
    }

    if registered > 0 {
        info!("📋 Registered {registered} DWN protocols");
    }
    Ok(())
}

/// Periodically check peer reachability and broadcast status changes.
async fn check_peer_health(state: &StateManager, data_dir: &std::path::Path) -> Result<()> {
    let known_peers = peers::load_peers(data_dir).await.unwrap_or_default();
    if known_peers.is_empty() {
        return Ok(());
    }

    let mut new_health = std::collections::HashMap::new();
    for peer in &known_peers {
        let fips_npub = crate::federation::fips_npub_for_onion(data_dir, &peer.onion).await;
        let reachable = node_message::check_peer_reachable(&peer.onion, fips_npub.as_deref())
            .await
            .unwrap_or(false);
        new_health.insert(peer.onion.clone(), reachable);
    }

    let (current_data, _) = state.get_snapshot().await;
    if current_data.peer_health != new_health {
        let mut data = current_data;
        data.peer_health = new_health;
        state.update_data(data).await;
        debug!("🔗 Peer health updated, broadcasting changes");
    }

    Ok(())
}

#[cfg(test)]
mod merge_tests {
    use super::*;
    use crate::data_model::{Description, Manifest, PackageDataEntry, PackageState, StaticFiles};

    fn make_manifest() -> Manifest {
        Manifest {
            id: "lnd".to_string(),
            title: "LND".to_string(),
            version: "0.18.4".to_string(),
            description: Description {
                short: "".to_string(),
                long: "".to_string(),
            },
            release_notes: "".to_string(),
            license: "".to_string(),
            wrapper_repo: "".to_string(),
            upstream_repo: "".to_string(),
            support_site: "".to_string(),
            marketing_site: "".to_string(),
            donation_url: None,
            author: None,
            website: None,
            interfaces: None,
            tier: None,
        }
    }

    fn make_static() -> StaticFiles {
        StaticFiles {
            license: "".to_string(),
            instructions: "".to_string(),
            icon: "".to_string(),
        }
    }

    fn make_entry(state: PackageState, health: Option<&str>) -> PackageDataEntry {
        PackageDataEntry {
            ui_ready: None,
            state,
            health: health.map(|s| s.to_string()),
            exit_code: None,
            static_files: make_static(),
            manifest: make_manifest(),
            installed: None,
            install_progress: None,
            uninstall_stage: None,
            available_update: None,
        }
    }

    #[test]
    fn stale_scan_cannot_remove_new_installs_or_overwrite_lifecycle_changes() {
        let running = make_entry(PackageState::Running, Some("healthy"));
        let restarting = make_entry(PackageState::Restarting, None);
        let base = [
            ("restart".into(), running.clone()),
            ("uninstalled".into(), running.clone()),
        ]
        .into_iter()
        .collect();
        let mut latest = [
            ("restart".into(), restarting.clone()),
            ("new".into(), running.clone()),
        ]
        .into_iter()
        .collect();
        let scanned = [
            ("restart".into(), running.clone()),
            ("uninstalled".into(), running.clone()),
        ]
        .into_iter()
        .collect();
        apply_scanned_packages(&mut latest, &base, &scanned);
        assert_eq!(latest.get("restart"), Some(&restarting));
        assert_eq!(latest.get("new"), Some(&running));
        assert!(!latest.contains_key("uninstalled"));
        apply_scanned_packages(&mut latest, &base, &HashMap::new());
        assert_eq!(latest.get("restart"), Some(&restarting));
        assert!(latest.contains_key("new"));
    }

    #[test]
    fn peer_path_filter_allows_content_catalog_and_items() {
        // Regression: the content *catalog* is exactly "/content" (no trailing
        // slash). It must be reachable over the peer (FIPS) listener, else
        // `content.browse-peer` 404s over the mesh. Item fetches are
        // "/content/<id>".
        assert!(is_peer_allowed_path("/content"), "catalog must be allowed");
        assert!(
            is_peer_allowed_path("/content/abc123"),
            "items must be allowed"
        );
        assert!(is_peer_allowed_path("/rpc/v1"));
        assert!(is_peer_allowed_path("/health"));
        // Mesh blob fetch + DWN sync — both were missing from the allowlist,
        // which made them deterministically 404 over FIPS and therefore
        // 100% Tor by construction.
        assert!(is_peer_allowed_path("/blob/abc123"), "blob fetch by CID");
        assert!(is_peer_allowed_path("/dwn/health"), "DWN sync step 1");
        // Not on the allow-list → rejected (no broad surface over the mesh).
        assert!(!is_peer_allowed_path("/contention"), "must not prefix-leak");
        assert!(!is_peer_allowed_path("/"));
        assert!(!is_peer_allowed_path("/rpc/v2"));
        assert!(!is_peer_allowed_path("/blobber"), "must not prefix-leak");
        assert!(!is_peer_allowed_path("/dwnx"), "must not prefix-leak");
    }

    #[test]
    fn app_relay_binds_to_fips_ula_not_wildcard() {
        let ula = "fd12:3456:789a::1".parse().unwrap();
        let addr = fips_app_relay_addr(ula, 8083);
        assert_eq!(addr.ip(), std::net::IpAddr::V6(ula));
        assert_eq!(addr.port(), 8083);
    }

    #[test]
    fn preserves_transitional_state_on_merge() {
        // existing: user initiated a stop, spawn_transitional set Stopping.
        // fresh: podman hasn't finished the stop yet, still reports Running.
        // Expected: merged state stays Stopping — podman's live view must
        // not clobber the transitional state owned by the RPC spawn task.
        let existing = make_entry(PackageState::Stopping, Some("healthy"));
        let fresh = make_entry(PackageState::Running, Some("starting"));
        let merged = merge_preserving_transitional(&existing, &fresh, true);
        assert_eq!(merged.state, PackageState::Stopping);
    }

    #[test]
    fn non_user_stopping_recovers_when_container_is_running() {
        let existing = make_entry(PackageState::Stopping, Some("unknown"));
        let fresh = make_entry(PackageState::Running, Some("healthy"));
        let merged = merge_preserving_transitional(&existing, &fresh, false);
        assert_eq!(merged.state, PackageState::Running);
        assert_eq!(merged.health.as_deref(), Some("healthy"));
    }

    #[test]
    fn user_stop_resolves_when_container_has_exited() {
        // The container exited and the scanner already normalized
        // exited+user-stopped to Stopped — the stop visibly completed, even
        // if the stop worker is still queued behind the per-app lock.
        let existing = make_entry(PackageState::Stopping, Some("unknown"));
        let fresh = make_entry(PackageState::Stopped, None);
        let merged = merge_preserving_transitional(&existing, &fresh, true);
        assert_eq!(merged.state, PackageState::Stopped);
    }

    #[test]
    fn non_user_stopping_with_exited_container_stays_owned() {
        // No user-stop marker → this Stopping belongs to some other flow;
        // don't resolve it from a scan.
        let existing = make_entry(PackageState::Stopping, Some("unknown"));
        let fresh = make_entry(PackageState::Stopped, None);
        let merged = merge_preserving_transitional(&existing, &fresh, false);
        assert_eq!(merged.state, PackageState::Stopping);
    }

    #[test]
    fn starting_resolves_when_container_is_running() {
        // Start worker may still be inside its readiness wait (up to 420s for
        // uptime-kuma) — but podman reporting Running means the start visibly
        // succeeded; live health is merged separately.
        let existing = make_entry(PackageState::Starting, Some("starting"));
        let fresh = make_entry(PackageState::Running, Some("healthy"));
        let merged = merge_preserving_transitional(&existing, &fresh, false);
        assert_eq!(merged.state, PackageState::Running);
        assert_eq!(merged.health.as_deref(), Some("healthy"));
    }

    #[test]
    fn restarting_is_not_resolved_by_running_scan() {
        // Mid-restart the pre-stop container still reads Running — the
        // restart worker owns this state until it finishes.
        let existing = make_entry(PackageState::Restarting, Some("healthy"));
        let fresh = make_entry(PackageState::Running, Some("healthy"));
        let merged = merge_preserving_transitional(&existing, &fresh, false);
        assert_eq!(merged.state, PackageState::Restarting);
    }

    #[test]
    fn merges_fresh_observability_fields() {
        // Non-state observability fields (health, exit_code, installed)
        // MUST come from the fresh scan even while state is preserved —
        // the UI still shows live health/health during a transition.
        let mut existing = make_entry(PackageState::Stopping, Some("healthy"));
        existing.exit_code = None;
        let mut fresh = make_entry(PackageState::Running, Some("unhealthy"));
        fresh.exit_code = Some(0);
        let merged = merge_preserving_transitional(&existing, &fresh, true);
        assert_eq!(merged.state, PackageState::Stopping);
        assert_eq!(merged.health.as_deref(), Some("unhealthy"));
        assert_eq!(merged.exit_code, Some(0));
    }

    #[test]
    fn stale_removing_recovers_when_container_is_running() {
        let existing = make_entry(PackageState::Removing, Some("unknown"));
        let fresh = make_entry(PackageState::Running, Some("healthy"));
        let merged = merge_preserving_transitional(&existing, &fresh, false);
        assert_eq!(merged.state, PackageState::Running);
        assert_eq!(merged.health.as_deref(), Some("healthy"));
    }

    #[test]
    fn is_transitional_covers_all_variants() {
        for s in [
            PackageState::Installing,
            PackageState::Stopping,
            PackageState::Starting,
            PackageState::Restarting,
            PackageState::Updating,
            PackageState::Removing,
            PackageState::CreatingBackup,
            PackageState::RestoringBackup,
            PackageState::BackingUp,
        ] {
            assert!(is_transitional(&s), "{:?} should be transitional", s);
        }
        for s in [
            PackageState::Installed,
            PackageState::Stopped,
            PackageState::Exited,
            PackageState::Running,
        ] {
            assert!(!is_transitional(&s), "{:?} should NOT be transitional", s);
        }
    }

    #[test]
    fn installing_uses_longer_stale_timeout_than_other_transitions() {
        assert!(transitional_stuck_timeout(&PackageState::Installing) > TRANSITIONAL_STUCK_TIMEOUT);
        assert_eq!(
            transitional_stuck_timeout(&PackageState::Stopping),
            TRANSITIONAL_STUCK_TIMEOUT
        );
    }

    #[test]
    fn absent_stopping_transitions_to_stopped() {
        assert_eq!(
            absent_transitional_replacement(&PackageState::Stopping),
            Some(PackageState::Stopped)
        );
    }

    #[test]
    fn absent_installing_still_waits_for_owner() {
        assert_eq!(
            absent_transitional_replacement(&PackageState::Installing),
            None
        );
    }
}
