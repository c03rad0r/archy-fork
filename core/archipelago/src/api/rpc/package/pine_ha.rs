//! Home Assistant <-> Pine voice-stack provisioning.
//!
//! The Pine app ships the Wyoming engines (pine-whisper :10300, pine-piper
//! :10200, pine-openwakeword :10400) as plain containers. Home Assistant only
//! uses them once a `wyoming` config entry exists for each, and once an
//! Assist pipeline points at the resulting entities. Out of the box that
//! meant clicking through HA's integration UI and building a pipeline by
//! hand — so seed all of it directly into HA's `.storage` when both apps are
//! present:
//!
//! - a `wyoming` config entry per engine,
//! - an Assist pipeline wired to whisper + piper,
//! - "ask Archy" voice intents backed by REST sensors on the node's
//!   `/api/pine/status` endpoint (block height, peers, sync, balances),
//! - a Claude conversation agent (HA's `anthropic` integration) using the
//!   node's shared `secrets/claude-api-key`, set as the pipeline's brain with
//!   `prefer_local_intents: true` so node questions stay local/free,
//! - an automation announcing new mesh messages on any Assist satellite.
//!
//! Everything here is best-effort (warn, never fail an install): HA migrates
//! the minimal store shapes we write to its current schema on boot, and skips
//! entries that already exist, so re-running is idempotent.

use serde_json::{json, Value};
use tracing::{info, warn};

/// Timestamp in the exact format HA writes to `.storage`
/// (`2026-07-22T08:29:35.123456+00:00` — micros, `+00:00` offset), safe for
/// Python's `datetime.fromisoformat`.
fn ha_now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
}

const HA_CONFIG_DIR: &str = "/var/lib/archipelago/home-assistant";
const HA_STORAGE_DIR: &str = "/var/lib/archipelago/home-assistant/.storage";
const NODE_SECRETS_DIR: &str = "/var/lib/archipelago/secrets";

/// The Wyoming engines the Pine stack publishes on the host.
/// HA runs under pasta, so the host is reachable as host.containers.internal.
const PINE_ENGINES: [(&str, &str, u16); 3] = [
    ("faster-whisper", "host.containers.internal", 10300),
    ("piper", "host.containers.internal", 10200),
    ("openwakeword", "host.containers.internal", 10400),
];

/// Entity id the Anthropic conversation subentry produces (device name
/// "Claude conversation" -> slug). Deterministic on a fresh registry.
const CLAUDE_CONVERSATION_ENTITY: &str = "conversation.claude_conversation";

/// Seed Home Assistant with the Pine voice defaults. Called after the Pine
/// stack installs and after Home Assistant installs (each side no-ops when
/// the other is missing). HA picks the new entries up on its next start; the
/// caller restarts the container when it is already running.
///
/// Returns true when anything was written (caller should restart HA).
pub(super) async fn seed_home_assistant_pine_defaults() -> bool {
    let storage = std::path::Path::new(HA_STORAGE_DIR);
    if let Err(e) = tokio::fs::create_dir_all(storage).await {
        warn!("pine/HA seed: cannot create {}: {}", HA_STORAGE_DIR, e);
        return false;
    }

    let entries_changed = seed_wyoming_config_entries(storage).await;
    let claude = seed_claude_conversation(storage).await;
    let pipeline_changed = seed_assist_pipeline(
        storage,
        claude.available.then_some(CLAUDE_CONVERSATION_ENTITY),
    )
    .await;
    let intents_changed = seed_voice_intents().await;
    let automation_changed = seed_mesh_announce_automation().await;
    entries_changed || claude.changed || pipeline_changed || intents_changed || automation_changed
}

// ---------------------------------------------------------------------------
// Voice intents + REST sensors (configuration.yaml + custom_sentences)
// ---------------------------------------------------------------------------

/// Markers bounding the seeded block in configuration.yaml. The begin marker
/// doubles as the legacy marker (early seeds had no end marker and always sat
/// at EOF, so legacy upgrade = replace from begin marker to end of file).
const VOICE_MARKER: &str = "# --- archipelago pine voice (seeded) ---";
const VOICE_END_MARKER: &str = "# --- archipelago pine voice (end) ---";

/// Read the pine status token (minting it on first use, 0600). The same
/// token gates the sensitive tier of `/api/pine/status` (see
/// `api::rpc::pine_status`), so seeding it into HA's REST sensor config is
/// what lets HA — and only HA — read balances and mesh text.
async fn ensure_status_token() -> Option<String> {
    let dir = std::path::Path::new(NODE_SECRETS_DIR);
    let path = dir.join(crate::api::rpc::pine_status::PINE_STATUS_TOKEN_FILE);
    if let Ok(existing) = tokio::fs::read_to_string(&path).await {
        let existing = existing.trim().to_string();
        if !existing.is_empty() {
            return Some(existing);
        }
    }
    if let Err(e) = tokio::fs::create_dir_all(dir).await {
        warn!("pine/HA seed: cannot create {}: {}", NODE_SECRETS_DIR, e);
        return None;
    }
    // KEY-05: a bearer status token written 0600 under NODE_SECRETS_DIR — source
    // named, 32-byte draw guarded.
    let mut raw = [0u8; 32];
    if let Err(e) = crate::entropy::draw_key_bytes(&mut rand::rngs::OsRng, &mut raw) {
        warn!("pine/HA seed: refusing to mint a status token from degenerate entropy: {e}");
        return None;
    }
    let token = hex::encode(raw);
    if let Err(e) = tokio::fs::write(&path, &token).await {
        warn!("pine/HA seed: writing status token failed: {e}");
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    info!("pine/HA seed: minted pine status token");
    Some(token)
}

/// The seeded configuration.yaml block: REST sensors polling the node's
/// status endpoint (through nginx on :80) + intent_script answers.
fn voice_config_block(token: &str) -> String {
    format!(
        r#"{VOICE_MARKER}
rest:
  - resource: http://host.containers.internal/api/pine/status
    headers:
      Authorization: "Bearer {token}"
    # 15s: the mesh-message sensor only carries the LATEST message id, so two
    # messages inside one poll window coalesce and only the newest announces
    # (observed on a test node 2026-07-22). Halving the window halves both
    # the announce lag and the coalescing odds; the endpoint is local + cheap.
    scan_interval: 15
    sensor:
      - name: "Archy Block Height"
        unique_id: archy_block_height
        value_template: "{{{{ value_json.bitcoin.height }}}}"
      - name: "Archy Bitcoin Sync"
        unique_id: archy_bitcoin_sync
        unit_of_measurement: "%"
        value_template: "{{{{ value_json.bitcoin.sync_percent }}}}"
        availability: "{{{{ value_json.bitcoin.sync_percent is not none }}}}"
      - name: "Archy Bitcoin Peers"
        unique_id: archy_bitcoin_peers
        value_template: "{{{{ value_json.bitcoin.peers }}}}"
      - name: "Archy Mesh Peers"
        unique_id: archy_mesh_peers
        value_template: "{{{{ value_json.mesh.peers }}}}"
      - name: "Archy Lightning Balance"
        unique_id: archy_lightning_balance
        unit_of_measurement: "sats"
        value_template: "{{{{ (value_json.lightning or {{}}).get('channel_balance_sats') }}}}"
        availability: "{{{{ (value_json.lightning or {{}}).get('channel_balance_sats') is not none }}}}"
      - name: "Archy Onchain Balance"
        unique_id: archy_onchain_balance
        unit_of_measurement: "sats"
        value_template: "{{{{ (value_json.lightning or {{}}).get('balance_sats') }}}}"
        availability: "{{{{ (value_json.lightning or {{}}).get('balance_sats') is not none }}}}"
      - name: "Archy Mesh Message"
        unique_id: archy_mesh_message
        value_template: "{{{{ (value_json.mesh_message or {{}}).get('id') }}}}"
        json_attributes_path: "$.mesh_message"
        json_attributes:
          - from
          - text
          - timestamp
intent_script:
  ArchyBlockHeight:
    description: >-
      Get the current Bitcoin block height of this node. Use for any question
      about the block height, chain tip, or number of blocks.
    speech:
      text: >-
        {{% if states('sensor.archy_block_height') not in ['unknown', 'unavailable', 'None'] %}}
          The block height is {{{{ states('sensor.archy_block_height') }}}}.
        {{% else %}}
          I can't read the block height right now.
        {{% endif %}}
  ArchyPeers:
    description: >-
      Get how many peers this node is connected to (bitcoin peers and mesh
      radio peers). Use for any question about peer or connection counts.
    speech:
      text: >-
        {{% set btc = states('sensor.archy_bitcoin_peers') %}}
        {{% set mesh = states('sensor.archy_mesh_peers') %}}
        {{% if btc not in ['unknown', 'unavailable', 'None'] %}}
          The node has {{{{ btc }}}} bitcoin peers{{% if mesh not in ['unknown', 'unavailable', 'None'] and mesh | int(0) > 0 %}} and {{{{ mesh }}}} mesh peers{{% endif %}}.
        {{% else %}}
          I can't read the peer count right now.
        {{% endif %}}
  ArchySyncStatus:
    description: >-
      Get this node's Bitcoin sync status / progress percentage. Use for any
      question about whether the node or bitcoin is synced, syncing, or up to
      date.
    speech:
      text: >-
        {{% set pct = states('sensor.archy_bitcoin_sync') %}}
        {{% if pct in ['unknown', 'unavailable', 'None'] %}}
          I can't read the sync status right now.
        {{% elif pct | float(0) >= 99.99 %}}
          The node is fully synced at block {{{{ states('sensor.archy_block_height') }}}}.
        {{% else %}}
          Bitcoin is {{{{ pct }}}} percent synced.
        {{% endif %}}
  ArchyLightningBalance:
    description: >-
      Get the Lightning wallet balance of this node in sats. Use for any
      question about the lightning balance, wallet balance, or how many sats
      are available.
    speech:
      text: >-
        {{% set ln = states('sensor.archy_lightning_balance') %}}
        {{% if ln not in ['unknown', 'unavailable', 'None'] %}}
          Your Lightning balance is {{{{ ln }}}} sats.
        {{% else %}}
          I can't read the Lightning balance right now. Is Lightning set up on this node?
        {{% endif %}}
{VOICE_END_MARKER}
"#
    )
}

const VOICE_SENTENCES: &str = r#"language: "en"
intents:
  ArchyBlockHeight:
    data:
      - sentences:
          - "what's the [current] block height"
          - "what is the [current] block height"
          - "[current] block height"
          - "how many blocks [are there]"
  ArchyPeers:
    data:
      - sentences:
          - "how many peers [do I have]"
          - "how many peers (is|are) [the node] connected to"
          - "[node] peer count"
  ArchySyncStatus:
    data:
      - sentences:
          - "is (the node|bitcoin) [fully] synced"
          - "[bitcoin] sync (status|progress|percent|percentage)"
          - "how synced is (the node|bitcoin)"
  ArchyLightningBalance:
    data:
      - sentences:
          - "what's my lightning balance"
          - "what is my lightning balance"
          - "lightning balance"
          - "how many sats (do I have|are in my wallet)"
"#;

/// Seed "ask Archy" node-info voice intents: custom sentences + a bounded
/// intent_script/rest block in configuration.yaml. Upgrades earlier seeded
/// blocks in place (including the pre-endpoint legacy block that ended at
/// EOF); skips cleanly when the user defines intent_script/rest themselves
/// outside our markers.
async fn seed_voice_intents() -> bool {
    let config_dir = std::path::Path::new(HA_CONFIG_DIR);
    let config_yaml = config_dir.join("configuration.yaml");

    let Some(token) = ensure_status_token().await else {
        return false;
    };
    let desired_block = voice_config_block(&token);

    // Sentences file (its own dir, no key-collision risk) — keep in sync.
    let sentences_dir = config_dir.join("custom_sentences/en");
    if let Err(e) = tokio::fs::create_dir_all(&sentences_dir).await {
        warn!(
            "pine/HA seed: cannot create {}: {}",
            sentences_dir.display(),
            e
        );
        return false;
    }
    let sentences_path = sentences_dir.join("archy.yaml");
    let sentences_changed = tokio::fs::read_to_string(&sentences_path)
        .await
        .map(|cur| cur != VOICE_SENTENCES)
        .unwrap_or(true);
    if sentences_changed {
        if let Err(e) = tokio::fs::write(&sentences_path, VOICE_SENTENCES).await {
            warn!("pine/HA seed: writing custom sentences failed: {e}");
            return false;
        }
    }

    let existing = tokio::fs::read_to_string(&config_yaml)
        .await
        .unwrap_or_default();

    let merged = if let Some(begin) = existing.find(VOICE_MARKER) {
        if let Some(end) = existing.find(VOICE_END_MARKER) {
            // Bounded block — replace when stale.
            let after = end + VOICE_END_MARKER.len();
            // Consume the trailing newline of the old block, if present.
            let after = after + existing[after..].starts_with('\n') as usize;
            let current = &existing[begin..after];
            if current == desired_block {
                return sentences_changed;
            }
            format!(
                "{}{}{}",
                &existing[..begin],
                desired_block,
                &existing[after..]
            )
        } else {
            // Legacy block (no end marker) — it was always appended at EOF,
            // possibly hand-edited on the node (e.g. the interim bitcoind
            // socat sensor). Replace marker..EOF wholesale.
            format!("{}{}", &existing[..begin], desired_block)
        }
    } else {
        if existing.contains("intent_script:") || existing.contains("\nrest:") {
            warn!("pine/HA seed: configuration.yaml already defines intent_script/rest — skipping voice intents");
            return sentences_changed;
        }
        format!("{existing}\n{desired_block}")
    };

    let tmp = config_yaml.with_extension("yaml.tmp-seed");
    if let Err(e) = tokio::fs::write(&tmp, &merged).await {
        warn!("pine/HA seed: writing configuration.yaml failed: {e}");
        return false;
    }
    if let Err(e) = tokio::fs::rename(&tmp, &config_yaml).await {
        warn!("pine/HA seed: replacing configuration.yaml failed: {e}");
        return false;
    }
    info!("pine/HA seed: voice intents + node-status sensors seeded");
    true
}

// ---------------------------------------------------------------------------
// Mesh-message announcements (automations.yaml)
// ---------------------------------------------------------------------------

const MESH_ANNOUNCE_AUTOMATION_ID: &str = "archy_mesh_announce";

/// Announce new mesh messages on every Assist satellite. Only seeded into an
/// empty/missing automations.yaml — merging into user-authored YAML risks
/// mangling it, and anyone with existing automations can paste this one from
/// the docs. Trigger conditions skip HA-restart transitions (unknown -> id)
/// but deliberately allow `None`/`unavailable` -> id: `None` is the sensor's
/// state before the first-ever received message (and again whenever the mesh
/// message store restarts empty), and `unavailable` follows any endpoint
/// blip — blocking those swallowed the first DM a fresh node ever received
/// (a test node, 2026-07-22). HA restarts still can't re-announce an old
/// message: rest sensors don't restore state, so the post-restart transition
/// is always from `unknown`.
const MESH_ANNOUNCE_AUTOMATION: &str = r#"- id: archy_mesh_announce
  alias: Announce mesh messages on Pine
  description: Speak new mesh messages on the Pine speaker (seeded by Archipelago)
  triggers:
    - trigger: state
      entity_id: sensor.archy_mesh_message
  conditions:
    - condition: template
      value_template: >-
        {{ trigger.from_state is not none
           and trigger.from_state.state != 'unknown'
           and trigger.to_state.state not in ['unknown', 'unavailable', 'None']
           and trigger.from_state.state != trigger.to_state.state }}
  actions:
    - variables:
        satellites: "{{ states.assist_satellite | map(attribute='entity_id') | list }}"
    - condition: template
      value_template: "{{ satellites | count > 0 }}"
    - action: assist_satellite.announce
      target:
        entity_id: "{{ satellites }}"
      data:
        message: >-
          New mesh message from {{ state_attr('sensor.archy_mesh_message', 'from') or 'unknown' }}:
          {{ state_attr('sensor.archy_mesh_message', 'text') or '' }}
  mode: queued
  max: 5
"#;

async fn seed_mesh_announce_automation() -> bool {
    let path = std::path::Path::new(HA_CONFIG_DIR).join("automations.yaml");
    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    if existing.contains(MESH_ANNOUNCE_AUTOMATION_ID) {
        // v1 of the seeded condition also blocked `None`/`unavailable` -> id
        // transitions, swallowing the first-ever received message. Upgrade by
        // replacing exactly that clause; anything else in the file (including
        // user-added automations) is left untouched.
        const V1_CLAUSE: &str =
            "trigger.from_state.state not in ['unknown', 'unavailable', 'None']";
        const V2_CLAUSE: &str = "trigger.from_state.state != 'unknown'";
        if existing.contains(V1_CLAUSE) {
            let upgraded = existing.replace(V1_CLAUSE, V2_CLAUSE);
            if let Err(e) = tokio::fs::write(&path, upgraded).await {
                warn!("pine/HA seed: upgrading automations.yaml failed: {e}");
                return false;
            }
            info!("pine/HA seed: mesh announce automation upgraded (first message announces)");
            return true;
        }
        return false;
    }
    let trimmed = existing.trim();
    if !(trimmed.is_empty() || trimmed == "[]") {
        warn!("pine/HA seed: automations.yaml has user content — skipping mesh announce seed");
        return false;
    }
    if let Err(e) = tokio::fs::write(&path, MESH_ANNOUNCE_AUTOMATION).await {
        warn!("pine/HA seed: writing automations.yaml failed: {e}");
        return false;
    }
    info!("pine/HA seed: mesh-message announce automation seeded");
    true
}

// ---------------------------------------------------------------------------
// Claude conversation agent (anthropic config entry)
// ---------------------------------------------------------------------------

struct ClaudeSeed {
    /// An anthropic entry exists (seeded now or already there) — the
    /// pipeline may point at the Claude conversation entity.
    available: bool,
    /// This call wrote the store.
    changed: bool,
}

/// Seed HA's `anthropic` integration from the node's shared Claude API key
/// (`secrets/claude-api-key`, the same key the mesh `!ai` assistant uses).
/// Skips when the key is absent or an anthropic entry already exists.
async fn seed_claude_conversation(storage: &std::path::Path) -> ClaudeSeed {
    let none = ClaudeSeed {
        available: false,
        changed: false,
    };
    let key = match tokio::fs::read_to_string(
        std::path::Path::new(NODE_SECRETS_DIR).join("claude-api-key"),
    )
    .await
    {
        Ok(k) if !k.trim().is_empty() => k.trim().to_string(),
        _ => {
            info!("pine/HA seed: no claude-api-key on node — skipping Claude conversation agent");
            return none;
        }
    };

    let path = storage.join("core.config_entries");
    let mut store = if tokio::fs::metadata(&path).await.is_ok() {
        match read_store(&path).await {
            Some(v) => v,
            None => return none,
        }
    } else {
        json!({
            "version": 1,
            "minor_version": 1,
            "key": "core.config_entries",
            "data": { "entries": [] }
        })
    };

    let Some(entries) = store
        .get_mut("data")
        .and_then(|d| d.get_mut("entries"))
        .and_then(|e| e.as_array_mut())
    else {
        warn!("pine/HA seed: core.config_entries has unexpected shape, leaving untouched");
        return none;
    };

    if entries
        .iter()
        .any(|e| e.get("domain").and_then(Value::as_str) == Some("anthropic"))
    {
        return ClaudeSeed {
            available: true,
            changed: false,
        };
    }

    // KEY-05: source named. Home Assistant config-entry / subentry ids are
    // identifiers HA needs only for uniqueness — not credentials and not key
    // material — so they are drawn unguarded per the classification table in
    // docs/security/KEY-05-ENTROPY-ENFORCEMENT.md.
    let id = || {
        let mut raw = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut raw);
        hex::encode(raw)
    };
    // Shape mirrors what HA 2026.7's anthropic config flow creates (entry
    // version 2.4 with conversation + ai_task subentries). Bookkeeping
    // fields (created_at/modified_at/discovery_keys) must be written here:
    // the store already carries HA's current schema minor_version, so HA
    // never migrates appended entries — a missing created_at is a
    // KeyError that crash-loops HA at boot.
    entries.push(json!({
        "entry_id": id(),
        "version": 2,
        "minor_version": 4,
        "domain": "anthropic",
        "title": "Claude",
        "data": { "api_key": key },
        "options": {},
        "pref_disable_new_entities": false,
        "pref_disable_polling": false,
        "source": "user",
        "unique_id": null,
        "disabled_by": null,
        "created_at": ha_now(),
        "modified_at": ha_now(),
        "discovery_keys": {},
        "subentries": [
            {
                "subentry_id": id(),
                "subentry_type": "conversation",
                "title": "Claude conversation",
                "unique_id": null,
                "data": {
                    "recommended": true,
                    "llm_hass_api": ["assist"],
                    // Steers fuzzy phrasings onto the local Archy* intent
                    // tools (cheap + exact) instead of free-form answers,
                    // and keeps replies speaker-length.
                    "prompt": "You are Archy, the voice of this Archipelago Bitcoin node, speaking through a smart speaker. Answers are spoken aloud: keep them to one or two short sentences, no markdown, no lists. When the user asks about the node — block height, sync status, peers, balances — call the matching Archy tool rather than answering from memory, even if the phrasing is loose. Only answer directly when no tool fits."
                }
            },
            {
                "subentry_id": id(),
                "subentry_type": "ai_task_data",
                "title": "Claude AI Task",
                "unique_id": null,
                "data": { "recommended": true }
            }
        ]
    }));

    if !write_store(&path, &store).await {
        return none;
    }
    info!("pine/HA seed: added Claude conversation agent (anthropic config entry)");
    ClaudeSeed {
        available: true,
        changed: true,
    }
}

// ---------------------------------------------------------------------------
// Wyoming config entries
// ---------------------------------------------------------------------------

/// Ensure `core.config_entries` has a `wyoming` entry per Pine engine.
async fn seed_wyoming_config_entries(storage: &std::path::Path) -> bool {
    let path = storage.join("core.config_entries");
    let mut store = if tokio::fs::metadata(&path).await.is_ok() {
        match read_store(&path).await {
            Some(v) => v,
            // Present but unreadable/corrupt: never overwrite it with a
            // fresh store — that would throw away every integration.
            None => return false,
        }
    } else {
        json!({
            "version": 1,
            "minor_version": 1,
            "key": "core.config_entries",
            "data": { "entries": [] }
        })
    };

    let Some(entries) = store
        .get_mut("data")
        .and_then(|d| d.get_mut("entries"))
        .and_then(|e| e.as_array_mut())
    else {
        warn!("pine/HA seed: core.config_entries has unexpected shape, leaving untouched");
        return false;
    };

    let mut added = false;
    for (title, host, port) in PINE_ENGINES {
        let exists = entries.iter().any(|e| {
            e.get("domain").and_then(Value::as_str) == Some("wyoming")
                && e.get("data")
                    .and_then(|d| d.get("host"))
                    .and_then(Value::as_str)
                    == Some(host)
                && e.get("data")
                    .and_then(|d| d.get("port"))
                    .and_then(Value::as_u64)
                    == Some(port as u64)
        });
        if exists {
            continue;
        }
        // KEY-05: source named; an HA entry identifier, unguarded (see above).
        let mut entry_id = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut entry_id);
        entries.push(json!({
            "entry_id": hex::encode(entry_id),
            "version": 1,
            "minor_version": 1,
            "domain": "wyoming",
            "title": title,
            "data": { "host": host, "port": port },
            "options": {},
            "pref_disable_new_entities": false,
            "pref_disable_polling": false,
            "source": "user",
            "unique_id": null,
            "disabled_by": null,
            "created_at": ha_now(),
            "modified_at": ha_now(),
            "discovery_keys": {},
            "subentries": []
        }));
        info!("pine/HA seed: added wyoming config entry {title} ({host}:{port})");
        added = true;
    }

    if added && !write_store(&path, &store).await {
        return false;
    }
    added
}

// ---------------------------------------------------------------------------
// Assist pipeline
// ---------------------------------------------------------------------------

/// Ensure an Assist pipeline uses the Pine engines. Creates the store when
/// missing; when it exists, repairs the pre-2024.6 conversation-agent id
/// (`homeassistant` -> `conversation.home_assistant`) and — when a Claude
/// agent is available — upgrades pipelines still on the default local agent
/// to Claude with `prefer_local_intents: true` (node intents stay local/free,
/// everything else goes to Claude).
async fn seed_assist_pipeline(storage: &std::path::Path, claude_entity: Option<&str>) -> bool {
    let path = storage.join("assist_pipeline.pipelines");

    if let Some(mut store) = read_store(&path).await {
        let Some(items) = store
            .get_mut("data")
            .and_then(|d| d.get_mut("items"))
            .and_then(|i| i.as_array_mut())
        else {
            return false;
        };
        let mut changed = false;
        for item in items.iter_mut() {
            if item.get("conversation_engine").and_then(Value::as_str) == Some("homeassistant") {
                item["conversation_engine"] = json!("conversation.home_assistant");
                info!("pine/HA seed: repaired legacy conversation agent id in Assist pipeline");
                changed = true;
            }
            if let Some(engine) = claude_entity {
                if item.get("conversation_engine").and_then(Value::as_str)
                    == Some("conversation.home_assistant")
                {
                    item["conversation_engine"] = json!(engine);
                    item["prefer_local_intents"] = json!(true);
                    info!("pine/HA seed: pipeline upgraded to Claude (local intents preferred)");
                    changed = true;
                }
            }
        }
        if changed {
            return write_store(&path, &store).await;
        }
        return false;
    }

    // ULID-shaped id (26 chars, Crockford base32) — HA only needs uniqueness.
    let id: String = {
        const ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";
        // KEY-05: source named; an HA pipeline identifier, unguarded (see above).
        // The `% 32` reduction is unchanged and unbiased — 32 divides 256 exactly.
        let mut raw = [0u8; 26];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut raw);
        raw.iter()
            .map(|b| ALPHABET[(*b % 32) as usize] as char)
            .collect()
    };
    let store = json!({
        "version": 1,
        "minor_version": 1,
        "key": "assist_pipeline.pipelines",
        "data": {
            "items": [{
                "conversation_engine": claude_entity.unwrap_or("conversation.home_assistant"),
                "conversation_language": "en",
                "id": id,
                "language": "en",
                "name": "Pine (local)",
                "prefer_local_intents": claude_entity.is_some(),
                "stt_engine": "stt.faster_whisper",
                "stt_language": "en",
                "tts_engine": "tts.piper",
                "tts_language": "en_GB",
                "tts_voice": null,
                "wake_word_entity": null,
                "wake_word_id": null
            }],
            "preferred_item": id
        }
    });
    if write_store(&path, &store).await {
        info!("pine/HA seed: created default Assist pipeline (whisper + piper)");
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Wyoming satellite keeper
// ---------------------------------------------------------------------------

/// Keep IP-pinned Wyoming satellite entries (voice speakers) reachable.
///
/// HA's zeroconf discovery stores a satellite as a fixed LAN IP. DHCP
/// renumbering — or the whole node moving to a different network — strands
/// the entry and the speaker silently drops (a test node 2026-07-23: entry
/// pinned to 192.0.2.16 while the LAN had become 192.0.2.0/24). HA
/// never re-resolves on its own. This keeper probes each satellite entry and,
/// when one stops answering, sweeps the node's local /24s for the same
/// Wyoming port and rewrites the entry to the address that answers.
pub(crate) async fn wyoming_satellite_keeper() {
    loop {
        if home_assistant_installed().await {
            heal_wyoming_satellites().await;
        }
        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
    }
}

async fn tcp_alive(host: &str, port: u16, ms: u64) -> bool {
    matches!(
        tokio::time::timeout(
            std::time::Duration::from_millis(ms),
            tokio::net::TcpStream::connect((host, port)),
        )
        .await,
        Ok(Ok(_))
    )
}

/// The node's own global IPv4 addresses. Loopback/CGNAT (tailscale) ranges are
/// excluded — satellites live on real LANs.
async fn local_ipv4_addresses() -> Vec<std::net::Ipv4Addr> {
    let Ok(out) = tokio::process::Command::new("ip")
        .args(["-4", "-o", "addr", "show", "scope", "global"])
        .output()
        .await
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let cidr = l.split_whitespace().nth(3)?;
            let ip: std::net::Ipv4Addr = cidr.split('/').next()?.parse().ok()?;
            let o = ip.octets();
            // 100.64.0.0/10 — tailscale/CGNAT, never a speaker LAN.
            if o[0] == 100 && (64..128).contains(&o[1]) {
                return None;
            }
            Some(ip)
        })
        .collect()
}

/// Sweep the /24 of every local interface for something answering `port`.
/// First responder wins; the node's own addresses are skipped.
async fn find_satellite(port: u16) -> Option<String> {
    let self_ips = local_ipv4_addresses().await;
    for base in self_ips.iter().map(|ip| ip.octets()) {
        let mut set = tokio::task::JoinSet::new();
        for i in 1..255u8 {
            let ip = std::net::Ipv4Addr::new(base[0], base[1], base[2], i);
            if self_ips.contains(&ip) {
                continue;
            }
            set.spawn(async move {
                tcp_alive(&ip.to_string(), port, 500)
                    .await
                    .then(|| ip.to_string())
            });
        }
        while let Some(res) = set.join_next().await {
            if let Ok(Some(ip)) = res {
                return Some(ip);
            }
        }
    }
    None
}

/// One keeper pass: re-point dead IP-pinned wyoming entries at wherever their
/// port now answers. Stops HA before editing the store (HA flushes its own
/// in-memory copy on shutdown, which would clobber a live edit) and starts it
/// again after.
async fn heal_wyoming_satellites() {
    let path = std::path::Path::new(HA_STORAGE_DIR).join("core.config_entries");
    let Some(store) = read_store(&path).await else {
        return;
    };
    let Some(entries) = store
        .get("data")
        .and_then(|d| d.get("entries"))
        .and_then(|e| e.as_array())
    else {
        return;
    };

    // (host, port, new_host) for every stranded satellite we can re-resolve.
    let mut moves: Vec<(String, u16, String)> = Vec::new();
    for e in entries {
        if e.get("domain").and_then(Value::as_str) != Some("wyoming") {
            continue;
        }
        let Some(host) = e
            .get("data")
            .and_then(|d| d.get("host"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        // Engines use host.containers.internal — only IP-pinned entries drift.
        if host.parse::<std::net::Ipv4Addr>().is_err() {
            continue;
        }
        let port = e
            .get("data")
            .and_then(|d| d.get("port"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as u16;
        if port == 0 || tcp_alive(host, port, 1500).await {
            continue;
        }
        let Some(new_host) = find_satellite(port).await else {
            info!("pine/HA keeper: satellite {host}:{port} unreachable and not found on any local /24 yet");
            continue;
        };
        if new_host != host {
            moves.push((host.to_string(), port, new_host));
        }
    }
    if moves.is_empty() {
        return;
    }

    // Stop HA, re-read + rewrite the store, start HA.
    let _ = tokio::process::Command::new("podman")
        .args(["stop", "homeassistant"])
        .output()
        .await;
    if let Some(mut store) = read_store(&path).await {
        let mut changed = false;
        if let Some(entries) = store
            .get_mut("data")
            .and_then(|d| d.get_mut("entries"))
            .and_then(|e| e.as_array_mut())
        {
            for e in entries.iter_mut() {
                if e.get("domain").and_then(Value::as_str) != Some("wyoming") {
                    continue;
                }
                let host = e
                    .get("data")
                    .and_then(|d| d.get("host"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let port = e
                    .get("data")
                    .and_then(|d| d.get("port"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as u16;
                if let Some((_, _, new_host)) =
                    moves.iter().find(|(h, p, _)| *h == host && *p == port)
                {
                    if let Some(data) = e.get_mut("data") {
                        data["host"] = json!(new_host);
                    }
                    e["modified_at"] = json!(ha_now());
                    info!("pine/HA keeper: satellite moved {host}:{port} -> {new_host}:{port}");
                    changed = true;
                }
            }
        }
        if changed {
            write_store(&path, &store).await;
        }
    }
    let _ = tokio::process::Command::new("podman")
        .args(["start", "homeassistant"])
        .output()
        .await;
}

// ---------------------------------------------------------------------------
// Presence probes + HA restart
// ---------------------------------------------------------------------------

/// True when the Pine Wyoming engines are installed on this node (their shared
/// data dir is created by the stack install).
pub(super) async fn pine_engines_installed() -> bool {
    tokio::fs::metadata("/var/lib/archipelago/pine")
        .await
        .is_ok()
}

/// True when Home Assistant has a config dir on this node (installed at some
/// point; .storage may not exist until first boot, which seeding handles).
pub(super) async fn home_assistant_installed() -> bool {
    tokio::fs::metadata(HA_CONFIG_DIR).await.is_ok()
}

/// Restart the HA container so it loads the seeded storage. Best-effort: when
/// the container doesn't exist (not yet created) the next start picks the
/// seeds up anyway.
pub(super) async fn restart_home_assistant_if_running() {
    let running = tokio::process::Command::new("podman")
        .args([
            "ps",
            "--format",
            "{{.Names}}",
            "--filter",
            "name=homeassistant",
        ])
        .output()
        .await
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l.trim() == "homeassistant")
        })
        .unwrap_or(false);
    if !running {
        return;
    }
    match tokio::process::Command::new("podman")
        .args(["restart", "homeassistant"])
        .output()
        .await
    {
        Ok(o) if o.status.success() => info!("pine/HA seed: restarted homeassistant"),
        Ok(o) => warn!(
            "pine/HA seed: podman restart homeassistant failed: {}",
            String::from_utf8_lossy(&o.stderr)
        ),
        Err(e) => warn!("pine/HA seed: podman restart homeassistant failed: {e}"),
    }
}

async fn read_store(path: &std::path::Path) -> Option<Value> {
    let raw = tokio::fs::read_to_string(path).await.ok()?;
    match serde_json::from_str(&raw) {
        Ok(v) => Some(v),
        Err(e) => {
            warn!(
                "pine/HA seed: {} is not valid JSON ({}), leaving untouched",
                path.display(),
                e
            );
            None
        }
    }
}

async fn write_store(path: &std::path::Path, store: &Value) -> bool {
    let pretty = match serde_json::to_string_pretty(store) {
        Ok(s) => s,
        Err(e) => {
            warn!("pine/HA seed: serialize {} failed: {}", path.display(), e);
            return false;
        }
    };
    // Write via temp + rename so HA never reads a half-written store.
    let tmp = path.with_extension("tmp-seed");
    if let Err(e) = tokio::fs::write(&tmp, pretty).await {
        warn!("pine/HA seed: write {} failed: {}", tmp.display(), e);
        return false;
    }
    if let Err(e) = tokio::fs::rename(&tmp, path).await {
        warn!("pine/HA seed: rename into {} failed: {}", path.display(), e);
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_block_is_bounded_and_carries_token() {
        let block = voice_config_block("deadbeef");
        assert!(block.starts_with(VOICE_MARKER));
        assert!(block.trim_end().ends_with(VOICE_END_MARKER));
        assert!(block.contains("Bearer deadbeef"));
        // Every intent the sentences file declares has a script answer.
        for intent in [
            "ArchyBlockHeight",
            "ArchyPeers",
            "ArchySyncStatus",
            "ArchyLightningBalance",
        ] {
            assert!(block.contains(intent), "missing intent {intent}");
            assert!(
                VOICE_SENTENCES.contains(intent),
                "missing sentences {intent}"
            );
        }
    }

    #[test]
    fn legacy_block_replacement_drops_old_content() {
        // Simulates the pre-endpoint node state: legacy marker + hand-edited
        // socat/bitcoind sensor block at EOF.
        let existing = format!(
            "default_config:\n\n{VOICE_MARKER}\nrest:\n  - resource: http://host.containers.internal:18332/\n    username: archipelago\n"
        );
        let begin = existing.find(VOICE_MARKER).unwrap();
        let desired = voice_config_block("tok");
        let merged = format!("{}{}", &existing[..begin], desired);
        assert!(!merged.contains("18332"));
        assert!(merged.contains("Bearer tok"));
        assert!(merged.starts_with("default_config:"));
    }
}
