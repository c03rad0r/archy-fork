//! Throwaway, hand-run probe for Routstr's live wire contract (RESEARCH Open Question 3 /
//! COVERAGE.md's three `INTEGRATE — UNCONFIRMED` rows).
//!
//! Run by hand: `cd core && cargo run --example routstr_probe [-- --endpoint <url>]`
//!
//! This is an `examples/` target: it links no daemon code path, is not built or run by
//! anything else in this repo, and is deletable at any time.
//!
//! READ-ONLY OBSERVATION ONLY:
//!   - subscribes to public Nostr relays and prints kind-38421 provider-announcement events
//!     verbatim (no parsing into a typed struct — the point is to see what is actually
//!     published, not what a struct expects)
//!   - issues at most three unauthenticated `GET`s against a discovered (or `--endpoint`
//!     overridden) provider
//!   - NEVER builds or sends a Cashu token, NEVER sends an `Authorization`/payment header,
//!     NEVER publishes a Nostr event, NEVER sends a node-identifying header (T-13-16/17/18)
//!
//! Tor-proxy-aware client construction reproduces the SHAPE of
//! `crate::nostr_discovery::build_nostr_client` (see that file). It is not imported: this
//! package has no `[lib]` target (only `[[bin]] archipelago`), so an `examples/` binary
//! cannot reach the daemon's internal modules regardless of their visibility — reproducing the
//! pattern here is the correct way to avoid a second, divergent, un-Tor-aware client, per the
//! plan's read_first note.

use anyhow::Result;
use nostr_sdk::prelude::*;
use serde_json::Value;
use std::time::Duration;

/// Routstr provider-announcement event kind, per docs.routstr.com (CITED, MEDIUM confidence —
/// this probe exists to confirm or correct it against a live relay).
const ROUTSTR_KIND: u16 = 38421;

/// The `d`-tag value docs.routstr.com cites for a provider-announcement event. Used as a
/// second, kind-unrestricted discovery filter in case the kind number in the docs has drifted.
const ROUTSTR_D_TAG: &str = "routstr-provider";

/// Default relays cited in docs.routstr.com.
const DEFAULT_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://relay.nostr.band",
    "wss://nos.lol",
];

/// Same SOCKS proxy address as `crate::constants::TOR_SOCKS_PROXY` — duplicated as a literal
/// because this example has no access to the daemon's private crate internals (see module doc
/// comment). Only used for `.onion` endpoints or when `ARCHIPELAGO_NOSTR_TOR_PROXY` is set.
const TOR_SOCKS_PROXY: &str = "socks5h://127.0.0.1:9050";

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(30);
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

/// Everything this probe observed, accumulated across discovery + capability checks and
/// printed as the final five-question summary block.
#[derive(Debug, Default)]
struct Findings {
    kind_38421_observed: bool,
    d_tag_fallback_observed: bool,
    tag_names: Vec<String>,
    content_keys: Vec<String>,
    model_field: Option<String>,
    price_field: Option<String>,
    payment_header_body: Option<String>,
    models_endpoint_ok: Option<bool>,
    models_endpoint_openai_shape: Option<bool>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let endpoint_override = args
        .iter()
        .position(|a| a == "--endpoint")
        .and_then(|i| args.get(i + 1))
        .cloned();

    println!("=== routstr_probe: live observation, read-only, spends nothing ===");
    println!("relays: {DEFAULT_RELAYS:?}");
    if let Some(e) = &endpoint_override {
        println!("--endpoint override given: {e}");
    }

    let mut findings = Findings::default();

    // Ephemeral key for this subscription only — sends no node-identifying header (T-13-18).
    let keys = Keys::generate();
    let tor_proxy = std::env::var("ARCHIPELAGO_NOSTR_TOR_PROXY").ok();
    let client = build_probe_nostr_client(keys, tor_proxy.as_deref())?;
    for relay in DEFAULT_RELAYS {
        let _ = client.add_relay(*relay).await;
    }
    if tokio::time::timeout(Duration::from_secs(10), client.connect())
        .await
        .is_err()
    {
        println!("WARNING: relay connect timed out after 10s, continuing anyway");
    }

    let discovered_endpoint = discover_providers(&client, &mut findings).await;
    client.disconnect().await;

    let endpoint = endpoint_override.or(discovered_endpoint);

    match &endpoint {
        Some(e) => {
            println!("\n--- probing capabilities at {e} ---");
            probe_capabilities(e, &mut findings).await;
        }
        None => {
            println!("\n(no endpoint discovered and no --endpoint override given — skipping capability probe)");
        }
    }

    let had_any_signal =
        findings.kind_38421_observed || findings.d_tag_fallback_observed || endpoint.is_some();
    print_summary(&findings, had_any_signal);

    Ok(())
}

/// Reproduces `nostr_discovery.rs::build_nostr_client`'s Tor-proxy-aware client-construction
/// shape (see module doc comment for why this is duplicated rather than imported).
fn build_probe_nostr_client(keys: Keys, tor_proxy: Option<&str>) -> Result<Client> {
    let client = if let Some(proxy_str) = tor_proxy {
        let addr: std::net::SocketAddr = proxy_str
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid Nostr Tor proxy: {proxy_str}"))?;
        let connection = Connection::new().proxy(addr).target(ConnectionTarget::All);
        let opts = ClientOptions::new().connection(connection);
        Client::builder().signer(keys).opts(opts).build()
    } else {
        Client::new(keys)
    };
    Ok(client)
}

/// Subscribes to two filters over the configured relays: (1) the documented kind 38421, and
/// (2) no kind restriction but a `#d` tag of "routstr-provider", in case the kind number in
/// the docs has drifted. Prints every matching event verbatim: full tag list, full content,
/// pubkey, created_at. Does NOT parse into a typed struct — the whole point is to see what is
/// actually published rather than what a struct expects. Returns the first HTTP(S) endpoint
/// URL found in any matched event's content, if any.
async fn discover_providers(client: &Client, findings: &mut Findings) -> Option<String> {
    let mut first_endpoint: Option<String> = None;

    println!("\n--- subscription 1: kind {ROUTSTR_KIND} ---");
    let kind_filter = Filter::new().kind(Kind::Custom(ROUTSTR_KIND)).limit(50);
    let kind_events = client
        .fetch_events(kind_filter, DISCOVERY_TIMEOUT)
        .await
        .map(|e| e.to_vec())
        .unwrap_or_default();
    if kind_events.is_empty() {
        println!("(no events)");
    }
    for ev in &kind_events {
        findings.kind_38421_observed = true;
        print_event(ev);
        record_event_shape(ev, findings);
        if first_endpoint.is_none() {
            first_endpoint = extract_endpoint(&ev.content);
        }
    }

    println!(
        "\n--- subscription 2: no kind filter, #d = \"{ROUTSTR_D_TAG}\" (fallback, in case the kind number drifted) ---"
    );
    let d_filter = Filter::new().identifier(ROUTSTR_D_TAG).limit(50);
    let d_events = client
        .fetch_events(d_filter, DISCOVERY_TIMEOUT)
        .await
        .map(|e| e.to_vec())
        .unwrap_or_default();
    if d_events.is_empty() {
        println!("(no events)");
    }
    for ev in &d_events {
        // Skip re-printing an event already shown by subscription 1's kind filter.
        if kind_events.iter().any(|k| k.id == ev.id) {
            continue;
        }
        findings.d_tag_fallback_observed = true;
        print_event(ev);
        record_event_shape(ev, findings);
        if first_endpoint.is_none() {
            first_endpoint = extract_endpoint(&ev.content);
        }
    }

    first_endpoint
}

fn print_event(ev: &Event) {
    println!("event id:   {}", ev.id);
    println!("pubkey:     {}", ev.pubkey);
    println!("kind:       {}", ev.kind.as_u16());
    println!("created_at: {}", ev.created_at.as_secs());
    println!("tags:");
    for tag in ev.tags.iter() {
        println!("  {:?}", tag.as_slice());
    }
    println!("content:    {}", ev.content);
    println!();
}

/// Records tag names and top-level content JSON keys seen across every matched event, plus a
/// best-effort guess at which content key carries the model list vs. the price — used for the
/// summary block's questions 2/3. Never overwrites a field already found.
fn record_event_shape(ev: &Event, findings: &mut Findings) {
    for tag in ev.tags.iter() {
        if let Some(name) = tag.as_slice().first() {
            if !findings.tag_names.contains(name) {
                findings.tag_names.push(name.clone());
            }
        }
    }
    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&ev.content) {
        for key in map.keys() {
            if !findings.content_keys.contains(key) {
                findings.content_keys.push(key.clone());
            }
        }
        for candidate in ["models", "model", "model_list"] {
            if map.contains_key(candidate) && findings.model_field.is_none() {
                findings.model_field = Some(candidate.to_string());
            }
        }
        for candidate in ["pricing", "price", "prices", "sats_per_request", "cost"] {
            if map.contains_key(candidate) && findings.price_field.is_none() {
                findings.price_field = Some(candidate.to_string());
            }
        }
    }
}

/// Best-effort extraction of an http(s) endpoint URL from a provider-announcement event's
/// content: prefers the docs-cited `endpoints` field, falls back to a generic scan of every
/// string value in the object.
fn extract_endpoint(content: &str) -> Option<String> {
    let value: Value = serde_json::from_str(content).ok()?;
    if let Some(endpoints) = value.get("endpoints") {
        if let Some(s) = first_http_string(endpoints) {
            return Some(s);
        }
    }
    first_http_string(&value)
}

fn first_http_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if s.starts_with("http://") || s.starts_with("https://") => {
            Some(s.trim_end_matches('/').to_string())
        }
        Value::Array(arr) => arr.iter().find_map(first_http_string),
        Value::Object(map) => map.values().find_map(first_http_string),
        _ => None,
    }
}

/// Issues unauthenticated `GET`s against `endpoint`: `/v1/models` and `/`. Sends NO Cashu
/// token, NO Authorization header, NO node-identifying header — this probe spends no money and
/// authenticates as nothing. A 401/402 body is printed verbatim and recorded: per the plan, a
/// provider naming its own expected payment header in that body is the single most valuable
/// artifact this probe can capture.
async fn probe_capabilities(endpoint: &str, findings: &mut Findings) {
    let client = match build_probe_http_client(endpoint) {
        Ok(c) => c,
        Err(e) => {
            println!("ERROR building HTTP client for {endpoint}: {e}");
            return;
        }
    };

    for path in ["/v1/models", "/"] {
        let url = format!("{}{}", endpoint.trim_end_matches('/'), path);
        probe_one(&client, &url, path, findings).await;
    }
}

async fn probe_one(client: &reqwest::Client, url: &str, label: &str, findings: &mut Findings) {
    println!("\nGET {url}");
    match client.get(url).send().await {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let trimmed: String = body.chars().take(2000).collect();
            println!("status: {status}");
            println!("body:   {trimmed}");
            if label == "/v1/models" {
                findings.models_endpoint_ok = Some(status.is_success());
                findings.models_endpoint_openai_shape = Some(
                    status.is_success() && body.contains("\"data\"") && body.contains("\"object\""),
                );
            }
            if (status.as_u16() == 401 || status.as_u16() == 402)
                && findings.payment_header_body.is_none()
            {
                findings.payment_header_body = Some(trimmed);
            }
        }
        Err(e) => {
            println!("request failed: {e}");
        }
    }
}

/// Builds a plain `reqwest::Client`, routed through the Tor SOCKS proxy only when the target
/// is a `.onion` address (mandatory — plain HTTP cannot reach one) or when
/// `ARCHIPELAGO_NOSTR_TOR_PROXY` is set (opt-in, mirrors the Nostr client's own Tor gate).
fn build_probe_http_client(endpoint: &str) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder().timeout(HTTP_TIMEOUT);
    let needs_tor =
        endpoint.contains(".onion") || std::env::var("ARCHIPELAGO_NOSTR_TOR_PROXY").is_ok();
    if needs_tor {
        let proxy = reqwest::Proxy::all(TOR_SOCKS_PROXY)?;
        builder = builder.proxy(proxy);
    }
    Ok(builder.build()?)
}

/// Prints the final summary block. If nothing was observed at all (no kind-38421 event, no
/// #d=routstr-provider fallback event, no discovered/overridden endpoint), prints exactly
/// `NO LIVE PROVIDER OBSERVED` as a first-class, non-error outcome instead of the five-question
/// breakdown — an ecosystem being quiet is a valid result, not something this probe should
/// panic or error over.
fn print_summary(findings: &Findings, had_any_signal: bool) {
    if !had_any_signal {
        println!("\nNO LIVE PROVIDER OBSERVED");
        return;
    }

    println!("\n=== SUMMARY ===");
    println!(
        "1. Was a live kind-{ROUTSTR_KIND} event observed? {}",
        if findings.kind_38421_observed {
            "YES".to_string()
        } else if findings.d_tag_fallback_observed {
            format!(
                "NO (kind {ROUTSTR_KIND} was empty, but a #d=\"{ROUTSTR_D_TAG}\" event WAS found under a different kind — see tags/content above)"
            )
        } else {
            "NO".to_string()
        }
    );
    println!(
        "2. Tag names observed: {:?}   Content JSON keys observed: {:?}",
        findings.tag_names, findings.content_keys
    );
    println!(
        "3. Model-list field: {:?}   Price field: {:?}",
        findings.model_field, findings.price_field
    );
    println!(
        "4. Payment header named in a 401/402 body: {:?}",
        findings.payment_header_body
    );
    println!(
        "5. Does /v1/models respond, and does its shape match OpenAI's (has \"data\"/\"object\")? responds={:?} openai_shaped={:?}",
        findings.models_endpoint_ok, findings.models_endpoint_openai_shape
    );
}
