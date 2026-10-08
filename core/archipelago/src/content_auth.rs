//! Short-lived, route- and recipient-bound proofs for peer content reads.
//! A claimed X-Federation-DID is never authentication. Sign with the existing
//! node Ed25519 identity; public shares remain readable without a peer proof.
use anyhow::{Context, Result};
use base64::Engine;
use ed25519_dalek::{Signature, Signer, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const HEADER: &str = "x-archipelago-content-auth";
const MAX_AGE: u64 = 60;

/// Purchase requests use a separate proof format bound to the exact body bytes.
/// Authentication does not prevent replay: purchase handlers must retain their
/// durable operation ID and return the original result for the same operation.
pub(crate) const REQUEST_HEADER: &str = "x-archipelago-content-request-auth";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestProof {
    did: String,
    audience: String,
    method: String,
    path: String,
    body_sha256: String,
    timestamp: i64,
    signature: String,
}

impl RequestProof {
    fn preimage(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&(
            "archipelago-content-request-auth-v2",
            &self.did,
            &self.audience,
            &self.method,
            &self.path,
            &self.body_sha256,
            self.timestamp,
        ))?)
    }
}

fn body_hash(body: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(body))
}

/// The caller must obtain the recipient DID from an authenticated peer binding,
/// and send precisely these bytes, method and path (including any query string).
pub(crate) fn sign_request(
    identity: &crate::identity::NodeIdentity,
    audience: &str,
    method: &hyper::Method,
    path: &str,
    body: &[u8],
    now: i64,
) -> Result<String> {
    crate::identity::pubkey_bytes_from_did_key(audience)?;
    anyhow::ensure!(
        path.starts_with('/') && !path.contains('#'),
        "Invalid request path"
    );
    let mut proof = RequestProof {
        did: identity.did_key()?,
        audience: audience.into(),
        method: method.as_str().into(),
        path: path.into(),
        body_sha256: body_hash(body),
        timestamp: now,
        signature: String::new(),
    };
    proof.signature = hex::encode(identity.signing_key().sign(&proof.preimage()?).to_bytes());
    let encoded = base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&proof)?);
    anyhow::ensure!(encoded.len() <= 4096, "Peer request proof is too large");
    Ok(encoded)
}

/// Verify a required purchase proof after enforcing the route's body-size limit.
/// A legacy GET proof or a claimed DID is never accepted as a purchase proof.
pub(crate) fn authenticate_request(
    headers: &hyper::HeaderMap,
    audience: &str,
    method: &hyper::Method,
    path: &str,
    body: &[u8],
    now: i64,
) -> Result<String> {
    let mut values = headers.get_all(REQUEST_HEADER).iter();
    let value = values.next().context("Missing peer request proof")?;
    anyhow::ensure!(values.next().is_none(), "Duplicate peer request proof");
    anyhow::ensure!(
        value.as_bytes().len() <= 4096,
        "Peer request proof is too large"
    );
    let raw = base64::engine::general_purpose::STANDARD
        .decode(value.as_bytes())
        .context("Invalid peer request proof encoding")?;
    let proof: RequestProof = serde_json::from_slice(&raw).context("Invalid peer request proof")?;
    anyhow::ensure!(
        proof.audience == audience
            && proof.method == method.as_str()
            && proof.path == path
            && proof.body_sha256 == body_hash(body),
        "Peer request proof scope mismatch"
    );
    anyhow::ensure!(
        proof.timestamp.abs_diff(now) <= MAX_AGE,
        "Peer request proof expired or clock differs"
    );
    let key = VerifyingKey::from_bytes(&crate::identity::pubkey_bytes_from_did_key(&proof.did)?)?;
    let signature = Signature::from_slice(
        &hex::decode(&proof.signature).context("Invalid peer request signature")?,
    )?;
    key.verify_strict(&proof.preimage()?, &signature)
        .context("Peer request signature rejected")?;
    Ok(proof.did)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    did: String,
    audience: String,
    path: String,
    range: String,
    timestamp: i64,
    signature: String,
}

impl Proof {
    fn preimage(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&(
            "archipelago-content-auth-v1",
            "GET",
            &self.did,
            &self.audience,
            &self.path,
            &self.range,
            self.timestamp,
        ))?)
    }
}

fn sign(
    identity: &crate::identity::NodeIdentity,
    audience: &str,
    path: &str,
    range: &str,
    now: i64,
) -> Result<String> {
    let mut proof = Proof {
        did: identity.did_key()?,
        audience: audience.into(),
        path: path.into(),
        range: range.into(),
        timestamp: now,
        signature: String::new(),
    };
    proof.signature = hex::encode(identity.signing_key().sign(&proof.preimage()?).to_bytes());
    Ok(base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&proof)?))
}

/// Only authenticated federation identity bindings are used as the audience.
/// No matching peer means an anonymous request, never a forged peer identity.
pub async fn outgoing(
    data_dir: &Path,
    onion: &str,
    path: &str,
    range: &str,
) -> Result<Option<String>> {
    let peers = crate::federation::load_nodes(data_dir).await?;
    let Some(peer) = peers.iter().find(|peer| peer.onion == onion) else {
        return Ok(None);
    };
    crate::identity::pubkey_bytes_from_did_key(&peer.did)?;
    let identity = crate::identity::NodeIdentity::load_existing(&data_dir.join("identity")).await?;
    Ok(Some(sign(
        &identity,
        &peer.did,
        path,
        range,
        chrono::Utc::now().timestamp(),
    )?))
}

pub fn incoming(
    headers: &hyper::HeaderMap,
    audience: &str,
    path: &str,
    now: i64,
) -> Result<Option<String>> {
    let Some(value) = headers.get(HEADER) else {
        return Ok(None);
    };
    anyhow::ensure!(value.as_bytes().len() <= 4096, "Peer proof is too large");
    let raw = base64::engine::general_purpose::STANDARD
        .decode(value.as_bytes())
        .context("Invalid peer proof encoding")?;
    let proof: Proof = serde_json::from_slice(&raw).context("Invalid peer proof")?;
    let range = headers
        .get("range")
        .map(|value| value.to_str())
        .transpose()?
        .unwrap_or("");
    anyhow::ensure!(
        proof.audience == audience && proof.path == path && proof.range == range,
        "Peer proof scope mismatch"
    );
    anyhow::ensure!(
        proof.timestamp.abs_diff(now) <= MAX_AGE,
        "Peer proof expired or clock differs"
    );
    let key = VerifyingKey::from_bytes(&crate::identity::pubkey_bytes_from_did_key(&proof.did)?)?;
    let bytes = hex::decode(&proof.signature).context("Invalid peer signature")?;
    let signature = Signature::from_slice(&bytes)?;
    key.verify_strict(&proof.preimage()?, &signature)
        .context("Peer signature rejected")?;
    Ok(Some(proof.did))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn purchase_proof_binds_exact_body_method_route_recipient_and_time() {
        let dir = tempfile::tempdir().unwrap();
        let identity = crate::identity::NodeIdentity::load_or_create(dir.path())
            .await
            .unwrap();
        let audience = crate::identity::did_key_from_pubkey_hex(&hex::encode([7; 32])).unwrap();
        let body = br#"{"id":"same-operation","token":"private"}"#;
        let path = "/content/purchase/settle";
        let mut headers = hyper::HeaderMap::new();
        let proof =
            sign_request(&identity, &audience, &hyper::Method::POST, path, body, 1000).unwrap();
        headers.insert(REQUEST_HEADER, proof.parse().unwrap());
        assert_eq!(
            authenticate_request(&headers, &audience, &hyper::Method::POST, path, body, 1000)
                .unwrap(),
            identity.did_key().unwrap()
        );
        for (recipient, method, route, bytes, now) in [
            (
                audience.as_str(),
                hyper::Method::GET,
                path,
                body.as_slice(),
                1000,
            ),
            (
                audience.as_str(),
                hyper::Method::POST,
                "/content/purchase/status",
                body.as_slice(),
                1000,
            ),
            (
                audience.as_str(),
                hyper::Method::POST,
                path,
                b"changed-token".as_slice(),
                1000,
            ),
            (
                "different-recipient",
                hyper::Method::POST,
                path,
                body.as_slice(),
                1000,
            ),
            (
                audience.as_str(),
                hyper::Method::POST,
                path,
                body.as_slice(),
                1061,
            ),
            (
                audience.as_str(),
                hyper::Method::POST,
                path,
                body.as_slice(),
                939,
            ),
        ] {
            assert!(authenticate_request(&headers, recipient, &method, route, bytes, now).is_err());
        }
        let mut decoded: RequestProof = serde_json::from_slice(
            &base64::engine::general_purpose::STANDARD
                .decode(&proof)
                .unwrap(),
        )
        .unwrap();
        decoded.did = audience.clone();
        headers.insert(
            REQUEST_HEADER,
            base64::engine::general_purpose::STANDARD
                .encode(serde_json::to_vec(&decoded).unwrap())
                .parse()
                .unwrap(),
        );
        assert!(
            authenticate_request(&headers, &audience, &hyper::Method::POST, path, body, 1000)
                .is_err()
        );
        headers.insert(REQUEST_HEADER, proof.parse().unwrap());
        headers.append(REQUEST_HEADER, proof.parse().unwrap());
        assert!(
            authenticate_request(&headers, &audience, &hyper::Method::POST, path, body, 1000)
                .is_err()
        );
        headers.remove(REQUEST_HEADER);
        headers.insert(
            HEADER,
            sign(&identity, &audience, path, "", 1000)
                .unwrap()
                .parse()
                .unwrap(),
        );
        headers.insert(
            "x-federation-did",
            identity.did_key().unwrap().parse().unwrap(),
        );
        assert!(
            authenticate_request(&headers, &audience, &hyper::Method::POST, path, body, 1000)
                .is_err()
        );
    }

    #[tokio::test]
    async fn proof_is_bound_to_signer_recipient_path_range_and_time() {
        let dir = tempfile::tempdir().unwrap();
        let identity = crate::identity::NodeIdentity::load_or_create(dir.path())
            .await
            .unwrap();
        let audience = crate::identity::did_key_from_pubkey_hex(&hex::encode([7; 32])).unwrap();
        let mut headers = hyper::HeaderMap::new();
        headers.insert(
            HEADER,
            sign(&identity, &audience, "/content/film", "bytes=0-9", 1000)
                .unwrap()
                .parse()
                .unwrap(),
        );
        headers.insert("range", "bytes=0-9".parse().unwrap());
        assert_eq!(
            incoming(&headers, &audience, "/content/film", 1000).unwrap(),
            Some(identity.did_key().unwrap())
        );
        for (recipient, path, time) in [
            (&audience[..], "/content/other", 1000),
            (&audience[..], "/content/film", 1061),
            (&audience[..], "/content/film", 939),
            ("other", "/content/film", 1000),
        ] {
            assert!(incoming(&headers, recipient, path, time).is_err());
        }
        headers.insert("range", "bytes=10-".parse().unwrap());
        assert!(incoming(&headers, &audience, "/content/film", 1000).is_err());
        headers.insert("range", "bytes=0-9".parse().unwrap());
        let mut proof: Proof = serde_json::from_slice(
            &base64::engine::general_purpose::STANDARD
                .decode(headers[HEADER].as_bytes())
                .unwrap(),
        )
        .unwrap();
        proof.did = audience.clone();
        headers.insert(
            HEADER,
            base64::engine::general_purpose::STANDARD
                .encode(serde_json::to_vec(&proof).unwrap())
                .parse()
                .unwrap(),
        );
        assert!(incoming(&headers, &audience, "/content/film", 1000).is_err());
    }

    #[tokio::test]
    async fn request_signing_never_creates_or_repairs_node_identity() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("identity");
        assert!(crate::identity::NodeIdentity::load_existing(&missing)
            .await
            .is_err());
        assert!(!missing.exists());
        tokio::fs::create_dir(&missing).await.unwrap();
        tokio::fs::write(missing.join("node_key"), b"damaged")
            .await
            .unwrap();
        assert!(crate::identity::NodeIdentity::load_existing(&missing)
            .await
            .is_err());
        assert_eq!(
            tokio::fs::read(missing.join("node_key")).await.unwrap(),
            b"damaged"
        );
    }

    #[test]
    fn catalog_invoice_and_bytes_visibility_share_the_same_rules() {
        use crate::content_server::{visible_to, AccessControl, Availability, ContentItem};
        let mut item = ContentItem {
            id: "id".into(),
            filename: "file".into(),
            mime_type: "video/mp4".into(),
            size_bytes: 1,
            description: String::new(),
            access: AccessControl::Paid {
                price_sats: 1,
                accepted: vec![],
            },
            availability: Availability::Specific {
                peers: vec!["verified-peer".into()],
            },
            added_at: String::new(),
        };
        assert!(!visible_to(&item, None, false, false));
        assert!(!visible_to(&item, Some("other-peer"), true, false));
        assert!(visible_to(&item, Some("verified-peer"), true, false));
        assert!(visible_to(&item, None, false, true));
        item.availability = Availability::AllPeers;
        item.access = AccessControl::PeersOnly;
        assert!(!visible_to(&item, None, false, false));
        assert!(!visible_to(&item, Some("not-connected"), false, false));
        assert!(visible_to(&item, Some("verified-peer"), true, false));
        item.access = AccessControl::Free;
        assert!(visible_to(&item, None, false, false));
        item.availability = Availability::Nobody;
        assert!(!visible_to(&item, None, false, true));
        assert!(!visible_to(&item, Some("verified-peer"), true, false));
    }

    #[tokio::test]
    async fn authentication_failure_is_a_delivery_error_before_any_network_attempt() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::create_dir(dir.path().join("federation"))
            .await
            .unwrap();
        tokio::fs::write(dir.path().join("federation/nodes.json"), b"invalid")
            .await
            .unwrap();
        let error = crate::fips::dial::PeerRequest::new(None, "peer.onion", "/content/file")
            .require_fips()
            .send_content_get(dir.path())
            .await
            .unwrap_err();
        // The corrupt identity store fails before the separate missing-FIPS
        // route error. It reaches the payment caller's existing refund branch.
        assert!(error.to_string().contains("Invalid federation nodes"));
        assert_eq!(
            tokio::fs::read(dir.path().join("federation/nodes.json"))
                .await
                .unwrap(),
            b"invalid"
        );
        assert!(!dir.path().join("identity").exists());
    }

    #[test]
    fn plain_claimed_did_never_becomes_an_authenticated_peer() {
        let mut headers = hyper::HeaderMap::new();
        headers.insert("x-federation-did", "did:key:claimed".parse().unwrap());
        assert!(incoming(&headers, "recipient", "/content/file", 1000)
            .unwrap()
            .is_none());
        for invalid in ["bad".to_string(), "A".repeat(4097)] {
            headers.insert(HEADER, invalid.parse().unwrap());
            assert!(incoming(&headers, "recipient", "/content/file", 1000).is_err());
        }
    }
}
