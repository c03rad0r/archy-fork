use super::RpcHandler;
use anyhow::Result;
use std::sync::Arc;

impl RpcHandler {
    /// Route an RPC method name to its handler, returning the result value.
    pub(super) async fn dispatch(
        self: &Arc<Self>,
        method: &str,
        params: Option<serde_json::Value>,
        session_token: &Option<String>,
    ) -> Result<serde_json::Value> {
        match method {
            "echo" => self.handle_echo(params).await,
            "server.echo" => self.handle_echo(params).await,
            "server.get-state" => self.handle_server_get_state().await,
            "health" => self.handle_health().await,
            "auth.login" => self.handle_auth_login(params).await,
            "auth.logout" => self.handle_auth_logout().await,
            "auth.changePassword" => {
                self.handle_auth_change_password(params, session_token)
                    .await
            }
            "auth.isSetup" => self.handle_auth_is_setup().await,
            "auth.setup" => self.handle_auth_setup(params).await,
            "auth.onboardingComplete" => self.handle_auth_onboarding_complete().await,
            "auth.isOnboardingComplete" => self.handle_auth_is_onboarding_complete().await,
            "auth.resetOnboarding" => self.handle_auth_reset_onboarding(params).await,
            "auth.createDeviceToken" => self.handle_auth_create_device_token(params).await,
            "auth.listDeviceTokens" => self.handle_auth_list_device_tokens().await,
            "auth.revokeDeviceToken" => self.handle_auth_revoke_device_token(params).await,

            // Seed management (BIP-39 mnemonic)
            "seed.generate" => self.handle_seed_generate().await,
            "seed.verify" => self.handle_seed_verify(params).await,
            "seed.restore" => self.handle_seed_restore(params).await,
            "seed.save-encrypted" => self.handle_seed_save_encrypted(params).await,
            "seed.status" => self.handle_seed_status().await,
            "seed.reveal" => self.handle_seed_reveal(params).await,

            // Container orchestration (for Archipelago-managed containers)
            "container-install" => self.handle_container_install(params).await,
            "container-start" => self.handle_container_start(params).await,
            "container-stop" => self.handle_container_stop(params).await,
            "container-restart" => self.handle_container_restart(params).await,
            "container-remove" => self.handle_container_remove(params).await,
            "container-list" => self.handle_container_list().await,
            "container-status" => self.handle_container_status(params).await,
            "container-logs" => self.handle_container_logs(params).await,
            "container-health" => self.handle_container_health(params).await,

            // Package management (for docker-compose apps).
            // install/uninstall/update return immediately with a
            // transitional status; the actual work runs in a background
            // tokio::spawn so the HTTP request doesn't block for minutes.
            "package.install" => self.clone().spawn_package_install(params).await,
            "package.start" => self.handle_package_start(params).await,
            "package.stop" => self.handle_package_stop(params).await,
            "package.restart" => self.handle_package_restart(params).await,
            "package.uninstall" => self.clone().spawn_package_uninstall(params).await,
            "package.update" => self.clone().spawn_package_update(params).await,
            "package.check-updates" => self.handle_package_check_updates(params).await,
            "package.versions" => self.handle_package_versions(params).await,
            "package.set-config" => self.clone().handle_package_set_config(params).await,
            "package.credentials" => self.handle_package_credentials(params).await,
            "app.filebrowser-token" => self.handle_filebrowser_token().await,

            // Bundled app management (for pre-loaded container images)
            "bundled-app-start" => self.handle_bundled_app_start(params).await,
            "bundled-app-stop" => self.handle_bundled_app_stop(params).await,

            // Node identity and P2P peers
            "node-add-peer" => self.handle_node_add_peer(params).await,
            "node-list-peers" => self.handle_node_list_peers().await,
            "node-remove-peer" => self.handle_node_remove_peer(params).await,
            "node-send-message" => self.handle_node_send_message(params).await,
            "node-check-peer" => self.handle_node_check_peer(params).await,
            "node-messages-received" => self.handle_node_messages_received().await,
            "node-store-sent" => self.handle_node_store_sent(params).await,
            "node-nostr-discover" => self.handle_node_nostr_discover().await,
            "node.did" => self.handle_node_did().await,
            "node.signChallenge" => self.handle_node_sign_challenge(params).await,
            "node.createBackup" => self.handle_node_create_backup(params).await,
            "node.tor-address" => self.handle_node_tor_address().await,
            "node.nostr-publish" => self.handle_node_nostr_publish().await,
            "node.nostr-pubkey" => self.handle_node_nostr_pubkey().await,
            "node.nostr-sign" => self.handle_node_nostr_sign(params).await,
            "node-nostr-verify-revoked" => self.handle_node_nostr_verify_revoked().await,
            "node.rotate-did" => self.handle_node_rotate_did(params).await,

            // Encrypted peer handshake (NIP-44)
            "handshake.discover" => self.handle_handshake_discover().await,
            "handshake.connect" => self.handle_handshake_connect(params).await,
            "handshake.poll" => self.handle_handshake_poll().await,
            "nostr.discovery-status" => self.handle_nostr_discovery_status().await,
            "nostr.set-discovery" => self.handle_nostr_set_discovery(params).await,

            // TOTP 2FA
            "auth.totp.setup.begin" => self.handle_totp_setup_begin(params).await,
            "auth.totp.setup.confirm" => self.handle_totp_setup_confirm(params).await,
            "auth.totp.disable" => self.handle_totp_disable(params).await,
            "auth.totp.status" => self.handle_totp_status().await,
            "auth.login.totp" => self.handle_login_totp(params, session_token).await,
            "auth.login.backup" => self.handle_login_backup(params, session_token).await,

            // Bitcoin & Lightning deep data
            "bitcoin.getinfo" => self.handle_bitcoin_getinfo().await,
            "bitcoin.relay-status" => self.handle_bitcoin_relay_status().await,
            "bitcoin.relay-update-settings" => {
                self.handle_bitcoin_relay_update_settings(params).await
            }
            "bitcoin.relay-request-peer" => self.handle_bitcoin_relay_request_peer(params).await,
            "bitcoin.relay-approve-request" => {
                self.handle_bitcoin_relay_approve_request(params).await
            }
            "bitcoin.relay-reject-request" => {
                self.handle_bitcoin_relay_reject_request(params).await
            }
            "bitcoin.relay-create-tor-service" => {
                self.handle_bitcoin_relay_create_tor_service().await
            }
            // NOTE: the Bitcoin Core wallet-init arm that used to sit here was deleted in
            // Phase 10 (D-07b, F-13). Its handler derived the BIP-84 account xprv and
            // imported it into Core's wallet.dat, duplicating the spending key outside the
            // Argon2 envelope. It had no caller. The `lnd.` arm below is a different,
            // still-live endpoint. See docs/security/KEY-03-SIGNING-POSTURE.md.
            "lnd.getinfo" => self.handle_lnd_getinfo().await,
            "lnd.listchannels" => self.handle_lnd_listchannels().await,
            "lnd.closedchannels" => self.handle_lnd_closedchannels().await,
            "lnd.openchannel" => self.handle_lnd_openchannel(params).await,
            "lnd.closechannel" => self.handle_lnd_closechannel(params).await,
            "lnd.newaddress" => self.handle_lnd_newaddress().await,
            "lnd.sendcoins" => self.handle_lnd_sendcoins(params).await,
            "lnd.estimatefee" => self.handle_lnd_estimatefee(params).await,
            "lnd.bump-quote" => self.handle_lnd_bump_quote(params).await,
            "lnd.bump-submit" => self.handle_lnd_bump_submit(params).await,
            "lnd.bump-status" => self.handle_lnd_bump_status(params).await,
            "lnd.createinvoice" => self.handle_lnd_createinvoice(params).await,
            "lnd.invoicestatus" => self.handle_lnd_invoicestatus(params).await,
            "lnd.payinvoice" => self.handle_lnd_payinvoice(params).await,
            "lnd.paymentstatus" => self.handle_lnd_paymentstatus(params).await,
            "lnd.create-psbt" => self.handle_lnd_create_psbt(params).await,
            "lnd.finalize-psbt" => self.handle_lnd_finalize_psbt(params).await,
            "lnd.create-raw-tx" => self.handle_lnd_create_raw_tx(params).await,
            "lnd.gettransactions" => self.handle_lnd_gettransactions().await,
            "lnd.lightning-history" => self.handle_lnd_lightning_history().await,
            "lnd.connect-info" => self.handle_lnd_connect_info().await,
            "lnd.export-channel-backup" => self.handle_lnd_export_channel_backup().await,
            "lnd.init-wallet-from-seed" => self.handle_lnd_init_wallet_from_seed(params).await,
            "lnd.seed-backup-status" => self.handle_lnd_seed_backup_status().await,
            "lnd.seed-reveal" => self.handle_lnd_seed_reveal(params).await,
            "lnd.seed-backup-ack" => self.handle_lnd_seed_backup_ack().await,
            // Lightning credential rotation. `rotate` re-verifies the node
            // password and returns immediately; the work runs in the
            // background and the UI polls `-progress`.
            "lnd.macaroon-status" => self.handle_lnd_macaroon_status().await,
            "lnd.rotate-macaroons" => self.handle_lnd_rotate_macaroons(params).await,
            "lnd.macaroon-rotation-progress" => self.handle_lnd_macaroon_rotation_progress().await,

            // Multi-identity management
            "identity.list" => self.handle_identity_list(params).await,
            "identity.create" => self.handle_identity_create(params).await,
            "identity.get" => self.handle_identity_get(params).await,
            "identity.delete" => self.handle_identity_delete(params).await,
            "identity.set-default" => self.handle_identity_set_default(params).await,
            "identity.sign" => self.handle_identity_sign(params).await,
            "identity.verify" => self.handle_identity_verify(params).await,
            "identity.resolve-did" => self.handle_identity_resolve_did(params).await,
            "identity.resolve-remote-did" => self.handle_identity_resolve_remote_did(params).await,
            "identity.verify-did-document" => {
                self.handle_identity_verify_did_document(params).await
            }
            "identity.create-dht-did" => self.handle_identity_create_dht_did(params).await,
            "identity.resolve-dht-did" => self.handle_identity_resolve_dht_did(params).await,
            "identity.refresh-dht-did" => self.handle_identity_refresh_dht_did(params).await,
            "identity.dht-status" => self.handle_identity_dht_status(params).await,
            "identity.update-profile" => self.handle_identity_update_profile(params).await,
            "identity.publish-profile" => self.handle_identity_publish_profile(params).await,
            "identity.export-keys" => self.handle_identity_export_keys(params).await,
            "identity.create-nostr-key" => self.handle_identity_create_nostr_key(params).await,
            "identity.nostr-sign" => self.handle_identity_nostr_sign(params).await,
            "identity.nostr-encrypt-nip04" => {
                self.handle_identity_nostr_encrypt_nip04(params).await
            }
            "identity.nostr-decrypt-nip04" => {
                self.handle_identity_nostr_decrypt_nip04(params).await
            }
            "identity.nostr-encrypt-nip44" => {
                self.handle_identity_nostr_encrypt_nip44(params).await
            }
            "identity.nostr-decrypt-nip44" => {
                self.handle_identity_nostr_decrypt_nip44(params).await
            }

            // Bitcoin domain names (NIP-05)
            "identity.register-name" => self.handle_identity_register_name(params).await,
            "identity.remove-name" => self.handle_identity_remove_name(params).await,
            "identity.resolve-name" => self.handle_identity_resolve_name(params).await,
            "identity.list-names" => self.handle_identity_list_names(params).await,
            "identity.link-name" => self.handle_identity_link_name(params).await,

            // Verifiable Credentials
            "identity.issue-credential" => self.handle_identity_issue_credential(params).await,
            "identity.verify-credential" => self.handle_identity_verify_credential(params).await,
            "identity.list-credentials" => self.handle_identity_list_credentials(params).await,
            "identity.revoke-credential" => self.handle_identity_revoke_credential(params).await,
            "identity.create-presentation" => {
                self.handle_identity_create_presentation(params).await
            }
            "identity.verify-presentation" => {
                self.handle_identity_verify_presentation(params).await
            }

            // Network overlay
            "network.get-visibility" => self.handle_network_get_visibility().await,
            "network.set-visibility" => self.handle_network_set_visibility(params).await,
            "network.request-connection" => self.handle_network_request_connection(params).await,
            "network.list-requests" => self.handle_network_list_requests().await,
            "network.accept-request" => self.handle_network_accept_request(params).await,
            "network.reject-request" => self.handle_network_reject_request(params).await,

            // Tor hidden services
            "tor.list-services" => self.handle_tor_list_services().await,
            "tor.create-service" => self.handle_tor_create_service(params).await,
            "tor.delete-service" => self.handle_tor_delete_service(params).await,
            "tor.get-onion-address" => self.handle_tor_get_onion_address(params).await,
            "tor.rotate-service" => self.handle_tor_rotate_service(params).await,
            "tor.cleanup-rotated" => self.handle_tor_cleanup_rotated().await,
            "tor.toggle-app" => self.handle_tor_toggle_app(params).await,
            "tor.restart" => self.handle_tor_restart().await,

            // Nostr relay management
            "nostr.list-relays" => self.handle_nostr_list_relays().await,
            "nostr.add-relay" => self.handle_nostr_add_relay(params).await,
            "nostr.remove-relay" => self.handle_nostr_remove_relay(params).await,
            "nostr.toggle-relay" => self.handle_nostr_toggle_relay(params).await,
            "nostr.get-stats" => self.handle_nostr_get_stats().await,

            // Router / UPnP
            "router.discover" => self.handle_router_discover().await,
            "router.list-forwards" => self.handle_router_list_forwards().await,
            "router.add-forward" => self.handle_router_add_forward(params).await,
            "router.remove-forward" => self.handle_router_remove_forward(params).await,
            "network.diagnostics" => self.handle_network_diagnostics().await,
            "network.list-interfaces" => self.handle_network_list_interfaces().await,
            "network.scan-wifi" => self.handle_network_scan_wifi().await,
            "network.configure-wifi" => self.handle_network_configure_wifi(params).await,
            "network.set-wifi-radio" => self.handle_network_set_wifi_radio(params).await,
            "network.configure-ethernet" => self.handle_network_configure_ethernet(params).await,
            "network.dns-status" => self.handle_network_dns_status().await,
            "network.configure-dns" => self.handle_network_configure_dns(params).await,
            "router.detect" => self.handle_router_detect(params).await,
            "router.info" => self.handle_router_info().await,
            "router.configure" => self.handle_router_configure(params).await,

            // OpenWrt / TollGate
            "openwrt.scan" => self.handle_openwrt_scan(params).await,
            "openwrt.get-status" => self.handle_openwrt_get_status(params).await,
            "openwrt.provision-tollgate" => self.handle_openwrt_provision_tollgate(params).await,
            "openwrt.scan-wifi" => self.handle_openwrt_scan_wifi(params).await,
            "openwrt.configure-wan" => self.handle_openwrt_configure_wan(params).await,

            // Ecash wallet
            "wallet.ecash-balance" => self.handle_wallet_ecash_balance().await,
            "wallet.ecash-mint" => self.handle_wallet_ecash_mint(params).await,
            "wallet.ecash-mint-claim" => self.handle_wallet_ecash_mint_claim(params).await,
            "wallet.ecash-melt" => self.handle_wallet_ecash_melt(params).await,
            "wallet.ecash-melt-confirm" => self.handle_wallet_ecash_melt_confirm(params).await,
            "wallet.ecash-send" => self.handle_wallet_ecash_send(params).await,
            "wallet.ecash-receive" => self.handle_wallet_ecash_receive(params).await,
            "wallet.ecash-history" => self.handle_wallet_ecash_history().await,
            "wallet.ecash-network" => self.handle_wallet_ecash_network().await,
            "wallet.ecash-set-network" => self.handle_wallet_ecash_set_network(params).await,
            "wallet.ecash-seed-status" => self.handle_wallet_ecash_seed_status().await,
            "wallet.ecash-lnaddress" => self.handle_wallet_ecash_lnaddress().await,
            "wallet.ecash-lnaddress-claim" => self.handle_wallet_ecash_lnaddress_claim().await,
            "wallet.ecash-seed-reveal" => self.handle_wallet_ecash_seed_reveal(params).await,
            "wallet.ecash-restore" => self.handle_wallet_ecash_restore(params).await,
            "wallet.ecash-seed-import" => self.handle_wallet_ecash_seed_import(params).await,
            "wallet.networking-profits" => self.handle_wallet_networking_profits().await,
            // Fedimint ecash (via fedimint-clientd sidecar)
            "wallet.fedimint-list" => self.handle_wallet_fedimint_list().await,
            "wallet.fedimint-join" => self.handle_wallet_fedimint_join(params).await,
            "wallet.fedimint-leave" => self.handle_wallet_fedimint_leave(params).await,
            "wallet.fedimint-balance" => self.handle_wallet_fedimint_balance().await,
            "wallet.fedimint-send" => self.handle_wallet_fedimint_send(params).await,

            // Ark protocol (via barkd sidecar)
            "wallet.ark-status" => self.handle_wallet_ark_status().await,
            "wallet.ark-balance" => self.handle_wallet_ark_balance().await,
            "wallet.ark-address" => self.handle_wallet_ark_address(params).await,
            "wallet.ark-send" => self.handle_wallet_ark_send(params).await,
            "wallet.ark-invoice" => self.handle_wallet_ark_invoice(params).await,
            "wallet.ark-board" => self.handle_wallet_ark_board(params).await,
            "wallet.ark-offboard" => self.handle_wallet_ark_offboard(params).await,
            "wallet.ark-history" => self.handle_wallet_ark_history().await,
            "wallet.ark-configure" => self.handle_wallet_ark_configure(params).await,

            // Container registries
            "registry.list" => self.handle_registry_list().await,
            "registry.add" => self.handle_registry_add(params).await,
            "registry.remove" => self.handle_registry_remove(params).await,
            "registry.set-primary" => self.handle_registry_set_primary(params).await,
            "registry.test" => self.handle_registry_test(params).await,

            // Streaming ecash payments
            "streaming.list-services" => self.handle_streaming_list_services().await,
            "streaming.configure-service" => self.handle_streaming_configure_service(params).await,
            "streaming.toggle-service" => self.handle_streaming_toggle_service(params).await,
            "streaming.pay" => self.handle_streaming_pay(params).await,
            "streaming.prepare-payment" => self.handle_streaming_prepare_payment(params).await,
            "streaming.discover" => self.handle_streaming_discover().await,
            "streaming.usage" => self.handle_streaming_usage(params).await,
            "streaming.session" => self.handle_streaming_session(params).await,
            "streaming.list-sessions" => self.handle_streaming_list_sessions().await,
            "streaming.close-session" => self.handle_streaming_close_session(params).await,
            "streaming.advertise" => self.handle_streaming_advertise().await,
            "streaming.list-mints" => self.handle_streaming_list_mints().await,
            "streaming.configure-mints" => self.handle_streaming_configure_mints(params).await,
            "streaming.maintenance" => self.handle_streaming_maintenance().await,

            // Content catalog management
            "content.list-mine" => self.handle_content_list_mine().await,
            "content.add" => self.handle_content_add(params).await,
            "content.publish" => self.handle_content_publish(params).await,
            "content.configure" => self.handle_content_configure(params).await,
            "content.remove" => self.handle_content_remove(params).await,
            "content.set-pricing" => self.handle_content_set_pricing(params).await,
            "content.set-availability" => self.handle_content_set_availability(params).await,
            "content.browse-peer" => self.handle_content_browse_peer(params).await,
            "content.download-peer" => self.handle_content_download_peer(params).await,
            "content.download-peer-paid" => self.handle_content_download_peer_paid(params).await,
            "content.indeehub-projects" => self.handle_content_indeehub_projects().await,
            "content.browse-all-peers" => self.handle_content_browse_all_peers().await,
            "content.owned-list" => self.handle_content_owned_list().await,
            "content.owned-get" => self.handle_content_owned_get(params).await,
            "content.request-invoice" => self.handle_content_request_invoice(params).await,
            "content.invoice-status" => self.handle_content_invoice_status(params).await,
            "content.download-peer-invoice" => {
                self.handle_content_download_peer_invoice(params).await
            }
            "content.request-onchain" => self.handle_content_request_onchain(params).await,
            "content.onchain-status" => self.handle_content_onchain_status(params).await,
            "content.download-peer-onchain" => {
                self.handle_content_download_peer_onchain(params).await
            }
            "content.preview-peer" => self.handle_content_preview_peer(params).await,

            // Music library (13-07, D-13): the whole `music.*` surface
            // lives in music.rs, not as new arms here — this single
            // guarded arm is the ONLY dispatcher.rs registration point
            // for it, mirroring the `assistant.` arm below. Adjacent to
            // the content.* block so the media surfaces read together.
            m if m.starts_with("music.") => self.handle_music(m, params).await,

            // DWN (Decentralized Web Node)
            "dwn.status" => self.handle_dwn_status().await,
            "dwn.sync" => self.handle_dwn_sync().await,
            "dwn.register-protocol" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_dwn_register_protocol(&p).await
            }
            "dwn.list-protocols" => self.handle_dwn_list_protocols().await,
            "dwn.remove-protocol" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_dwn_remove_protocol(&p).await
            }
            "dwn.query-messages" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_dwn_query_messages(&p).await
            }
            "dwn.write-message" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_dwn_write_message(&p).await
            }

            // Federation
            "federation.invite" => self.handle_federation_invite(params).await,
            "federation.join" => self.handle_federation_join(params).await,
            "federation.list-nodes" => self.handle_federation_list_nodes().await,
            "federation.remove-node" => self.handle_federation_remove_node(params).await,
            "federation.set-trust" => self.handle_federation_set_trust(params).await,
            "federation.sync-state" => self.handle_federation_sync_state().await,
            "federation.get-state" => self.handle_federation_get_state().await,
            "federation.peer-joined" => self.handle_federation_peer_joined(params).await,
            "federation.deploy-app" => self.handle_federation_deploy_app(params).await,
            "federation.peer-address-changed" => {
                self.handle_federation_peer_address_changed(params).await
            }
            "federation.notify-did-change" => {
                self.handle_federation_notify_did_change(params).await
            }
            "federation.peer-did-changed" => self.handle_federation_peer_did_changed(params).await,
            "federation.list-pending-requests" => {
                self.handle_federation_list_pending_requests().await
            }
            "federation.approve-request" => self.handle_federation_approve_request(params).await,
            "federation.reject-request" => self.handle_federation_reject_request(params).await,
            "federation.cancel-request" => self.handle_federation_cancel_request(params).await,

            // VPN & Remote Access
            "vpn.status" => self.handle_vpn_status().await,
            "vpn.configure" => self.handle_vpn_configure(params).await,
            "vpn.disconnect" => self.handle_vpn_disconnect().await,
            "vpn.invite" => self.handle_vpn_invite(params).await,
            "vpn.add-participant" => self.handle_vpn_add_participant(params).await,
            "vpn.create-peer" => self.handle_vpn_create_peer(params).await,
            "vpn.list-peers" => self.handle_vpn_list_peers().await,
            "vpn.peer-config" => self.handle_vpn_peer_config(params).await,
            "vpn.remove-peer" => self.handle_vpn_remove_peer(params).await,
            "remote.setup" => self.handle_remote_setup(params).await,

            // Marketplace
            "marketplace.discover" => self.handle_marketplace_discover().await,
            "marketplace.publish" => self.handle_marketplace_publish(params).await,
            "marketplace.get-manifest" => self.handle_marketplace_get_manifest(params).await,
            "marketplace.list-published" => self.handle_marketplace_list_published().await,
            "marketplace.verify" => self.handle_marketplace_verify(params).await,
            "marketplace.create-invoice" => self.handle_marketplace_create_invoice(params).await,
            "marketplace.check-payment" => self.handle_marketplace_check_payment(params).await,

            // Mesh networking (Meshcore LoRa)
            "mesh.status" => self.handle_mesh_status().await,
            "mesh.probe-device" => self.handle_mesh_probe_device(params).await,
            "mesh.flash-list-firmware" => self.handle_mesh_flash_list_firmware(params).await,
            "mesh.flash-device" => self.handle_mesh_flash_device(params).await,
            "mesh.flash-status" => self.handle_mesh_flash_status().await,
            "mesh.flash-cancel" => self.handle_mesh_flash_cancel().await,
            "mesh.peers" => self.handle_mesh_peers().await,
            "mesh.refresh" => self.handle_mesh_refresh().await,
            "mesh.messages" => self.handle_mesh_messages(params).await,
            "mesh.debug-dump" => self.handle_mesh_debug_dump().await,
            "mesh.send" => self.handle_mesh_send(params).await,
            "mesh.send-channel" => self.handle_mesh_send_channel(params).await,
            "mesh.broadcast" => self.handle_mesh_broadcast().await,
            "mesh.reboot-radio" => self.handle_mesh_reboot_radio(params).await,
            "mesh.rnode-config" => self.handle_mesh_rnode_config().await,
            "mesh.rnode-config-apply" => self.handle_mesh_rnode_config_apply(params).await,
            "mesh.configure" => self.handle_mesh_configure(params).await,
            "mesh.send-invoice" => self.handle_mesh_send_invoice(params).await,
            "mesh.send-coordinate" => self.handle_mesh_send_coordinate(params).await,
            "mesh.send-alert" => self.handle_mesh_send_alert(params).await,
            "mesh.send-content" => self.handle_mesh_send_content(params).await,
            "mesh.send-content-inline" => self.handle_mesh_send_content_inline(params).await,
            "mesh.transport-advice" => self.handle_mesh_transport_advice(params).await,
            "mesh.fetch-content" => self.handle_mesh_fetch_content(params).await,
            "mesh.send-reply" => self.handle_mesh_send_reply(params).await,
            "mesh.send-reaction" => self.handle_mesh_send_reaction(params).await,
            "mesh.send-read-receipt" => self.handle_mesh_send_read_receipt(params).await,
            "mesh.forward-message" => self.handle_mesh_forward_message(params).await,
            "mesh.edit-message" => self.handle_mesh_edit_message(params).await,
            "mesh.delete-message" => self.handle_mesh_delete_message(params).await,
            "mesh.send-psbt" => self.handle_mesh_send_psbt(params).await,
            "mesh.broadcast-presence" => self.handle_mesh_broadcast_presence(params).await,
            "mesh.presence-list" => self.handle_mesh_presence_list(params).await,
            "mesh.lightning-peers" => self.handle_mesh_lightning_peers(params).await,
            "mesh.send-lightning-info" => self.handle_mesh_send_lightning_info(params).await,
            "mesh.contacts-list" => self.handle_mesh_contacts_list(params).await,
            "mesh.contacts-save" => self.handle_mesh_contacts_save(params).await,
            "mesh.contacts-block" => self.handle_mesh_contacts_block(params).await,
            "mesh.send-channel-invite" => self.handle_mesh_send_channel_invite(params).await,
            "conversations.list" => self.handle_conversations_list(params).await,
            "conversations.messages" => self.handle_conversations_messages(params).await,
            "mesh.clear-all" => self.handle_mesh_clear_all().await,
            "mesh.outbox" => self.handle_mesh_outbox(params).await,
            "mesh.session-status" => self.handle_mesh_session_status(params).await,
            "mesh.rotate-prekeys" => self.handle_mesh_rotate_prekeys().await,
            // Phase 4: Off-grid Bitcoin operations
            "mesh.relay-tx" => self.handle_mesh_relay_tx(params).await,
            "mesh.relay-status" => self.handle_mesh_relay_status(params).await,
            "mesh.block-headers" => self.handle_mesh_block_headers(params).await,
            "mesh.relay-lightning" => self.handle_mesh_relay_lightning(params).await,
            "mesh.deadman-status" => self.handle_mesh_deadman_status().await,
            "mesh.deadman-configure" => self.handle_mesh_deadman_configure(params).await,
            "mesh.deadman-checkin" => self.handle_mesh_deadman_checkin().await,
            "mesh.assistant-status" => self.handle_mesh_assistant_status().await,
            "mesh.assistant-configure" => self.handle_mesh_assistant_configure(params).await,
            // Phase 13 (D-01/D-02): the whole `assistant.*` surface lives in
            // assistant_chat.rs, not as new arms here — this is the ONLY
            // dispatcher.rs registration point for it. Every later
            // assistant.* method (13-05, 13-08, 13-10) is added inside
            // assistant_chat.rs's own match, never as a new arm in this file.
            m if m.starts_with("assistant.") => {
                self.handle_assistant(m, params, session_token).await
            }
            "mesh.schedule-message" => self.handle_mesh_schedule_message(params).await,
            "mesh.list-scheduled" => self.handle_mesh_list_scheduled().await,
            "mesh.cancel-scheduled" => self.handle_mesh_cancel_scheduled(params).await,
            "mesh.test-send" => self.handle_mesh_test_send(params).await,

            // Transport layer (unified routing)
            "transport.status" => self.handle_transport_status().await,
            "transport.peers" => self.handle_transport_peers().await,
            "transport.send" => self.handle_transport_send(params).await,
            "transport.set-mode" => self.handle_transport_set_mode(params).await,
            "transport.preferences" => self.handle_transport_preferences().await,
            "transport.set-preference" => self.handle_transport_set_preference(params).await,

            // Server settings
            "server.set-name" => self.handle_server_set_name(params).await,
            "server.set-location" => self.handle_server_set_location(params).await,

            // System monitoring
            "security.app-gate-status" => self.handle_app_gate_status().await,
            "security.set-app-gate" => self.handle_set_app_gate(params).await,
            "system.get-hostname" => self.handle_system_get_hostname().await,
            "system.stats" => self.handle_system_stats().await,
            "system.processes" => self.handle_system_processes().await,
            "system.temperature" => self.handle_system_temperature().await,
            "system.detect-usb-devices" => self.handle_system_detect_usb_devices().await,
            "system.disk-status" => self.handle_system_disk_status().await,
            "system.disk-cleanup" => self.handle_system_disk_cleanup().await,
            "system.reboot" => self.handle_system_reboot(params).await,
            "system.factory-reset" => self.handle_system_factory_reset(params).await,
            "auth.session-policy.get" => self.handle_session_policy_get().await,
            "auth.session-policy.set" => self.handle_session_policy_set(params).await,
            "ai.permissions.get" => self.handle_ai_permissions_get().await,
            "ai.permissions.set" => self.handle_ai_permissions_set(params).await,
            "system.settings.get" => self.handle_system_settings_get(params).await,
            "system.settings.set" => self.handle_system_settings_set(params).await,
            "system.node-ca.generate" => self.handle_system_node_ca_generate().await,
            "system.kiosk-display.get" => self.handle_system_kiosk_display_get().await,
            "system.kiosk-display.set" => self.handle_system_kiosk_display_set(params).await,

            // Opt-in anonymous analytics
            "analytics.get-status" => self.handle_analytics_get_status().await,
            "analytics.enable" => self.handle_analytics_enable().await,
            "analytics.disable" => self.handle_analytics_disable().await,
            "analytics.get-snapshot" => self.handle_analytics_get_snapshot().await,
            "telemetry.report" => self.handle_telemetry_report().await,
            "telemetry.ingest" => self.handle_telemetry_ingest(params).await,
            "telemetry.fleet-status" => self.handle_telemetry_fleet_status().await,
            "telemetry.fleet-node-history" => {
                self.handle_telemetry_fleet_node_history(params).await
            }
            "telemetry.fleet-alerts" => self.handle_telemetry_fleet_alerts().await,

            // Real-time metrics monitoring
            "monitoring.current" => self.handle_monitoring_current().await,
            "monitoring.history" => self.handle_monitoring_history(params).await,
            "monitoring.containers" => self.handle_monitoring_containers().await,
            "monitoring.alerts" => self.handle_monitoring_alerts(params).await,
            "monitoring.alert-rules" => self.handle_monitoring_alert_rules().await,
            "monitoring.configure-alert" => self.handle_monitoring_configure_alert(params).await,
            "monitoring.acknowledge-alert" => {
                self.handle_monitoring_acknowledge_alert(params).await
            }
            "monitoring.export" => self.handle_monitoring_export(params).await,

            // FIPS mesh transport
            "fips.status" => self.handle_fips_status().await,
            "fips.pair-info" => self.handle_fips_pair_info().await,
            "fips.check-update" => self.handle_fips_check_update().await,
            "fips.apply-update" => self.handle_fips_apply_update().await,
            "fips.install" => self.handle_fips_install().await,
            "fips.restart" => self.handle_fips_restart().await,
            "fips.reconnect" => self.handle_fips_reconnect().await,
            "fips.list-seed-anchors" => self.handle_fips_list_seed_anchors().await,
            "fips.add-seed-anchor" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_fips_add_seed_anchor(&p).await
            }
            "fips.remove-seed-anchor" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_fips_remove_seed_anchor(&p).await
            }
            "fips.apply-seed-anchors" => self.handle_fips_apply_seed_anchors().await,
            "fips.ssh-over-mesh.get" => self.handle_fips_ssh_over_mesh_get().await,
            "fips.ssh-over-mesh.set" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_fips_ssh_over_mesh_set(&p).await
            }

            // System updates
            "update.check" => self.handle_update_check().await,
            "update.status" => self.handle_update_status().await,
            "update.dismiss" => self.handle_update_dismiss().await,
            "update.download" => self.handle_update_download().await,
            "update.cancel-download" => self.handle_update_cancel_download().await,
            "update.list-mirrors" => self.handle_update_list_mirrors().await,
            "update.add-mirror" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_update_add_mirror(&p).await
            }
            "update.remove-mirror" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_update_remove_mirror(&p).await
            }
            "update.set-primary-mirror" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_update_set_primary_mirror(&p).await
            }
            "update.test-mirror" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_update_test_mirror(&p).await
            }
            "update.get-source" => self.handle_update_get_source().await,
            "update.set-source" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_update_set_source(&p).await
            }
            "update.apply" => self.handle_update_apply().await,
            "update.git-apply" => self.handle_update_git_apply().await,
            "update.rollback" => self.handle_update_rollback().await,
            "update.get-schedule" => self.handle_update_get_schedule().await,
            "update.set-schedule" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_update_set_schedule(&p).await
            }

            // Backup & Restore
            "backup.create" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_backup_create(&p).await
            }
            "backup.list" => self.handle_backup_list().await,
            "backup.verify" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_backup_verify(&p).await
            }
            "backup.restore" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_backup_restore(&p).await
            }
            "backup.restore-identity" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_backup_restore_identity(&p).await
            }
            "backup.delete" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_backup_delete(&p).await
            }
            "backup.list-drives" => self.handle_backup_list_drives().await,
            "backup.to-usb" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_backup_to_usb(&p).await
            }
            "backup.upload-s3" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_backup_upload_s3(&p).await
            }
            "backup.download-s3" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_backup_download_s3(&p).await
            }

            // Security / secrets
            "security.rotate-secrets" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_security_rotate_secrets(&p).await
            }
            "security.list-expiring" => {
                let p = params.unwrap_or(serde_json::json!({}));
                self.handle_security_list_expiring(&p).await
            }

            // Webhooks
            "webhook.get-config" => self.handle_webhook_get_config().await,
            "webhook.configure" => self.handle_webhook_configure(params).await,
            "webhook.test" => self.handle_webhook_test().await,

            _ => Err(anyhow::anyhow!("Unknown method: {}", method)),
        }
    }

    pub(super) async fn handle_echo(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        if let Some(p) = params {
            if let Some(msg) = p.get("message").and_then(|v| v.as_str()) {
                return Ok(serde_json::json!({ "message": msg }));
            }
        }
        Ok(serde_json::json!({ "message": "Hello from Archipelago!" }))
    }

    async fn handle_server_get_state(&self) -> Result<serde_json::Value> {
        let (data, rev) = self.state_manager.get_snapshot().await;
        Ok(serde_json::json!({ "data": data, "rev": rev }))
    }

    pub(super) async fn handle_health(&self) -> Result<serde_json::Value> {
        let recovery_complete = crate::crash_recovery::is_recovery_complete();
        let uptime = crate::crash_recovery::uptime_seconds();
        let status = if recovery_complete { "ok" } else { "degraded" };
        Ok(serde_json::json!({
            "status": status,
            "crash_recovery_complete": recovery_complete,
            "uptime_seconds": uptime,
            "version": format!("{}-{}", env!("CARGO_PKG_VERSION"), option_env!("GIT_HASH").unwrap_or("dev")),
        }))
    }
}
