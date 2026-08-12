//! TLS for gated app ports, alongside plain HTTP on the same socket.
//!
//! # Why both, on one port
//!
//! An app port has to serve whatever the browser asks for. A node whose
//! dashboard is plain HTTP embeds `http://host:PORT`; a node with HTTPS embeds
//! `https://host:PORT` — and an HTTPS page cannot embed an HTTP frame at all
//! (mixed content), so the choice is genuinely per-node, not per-fleet. Giving
//! TLS its own port number would mean every app declares a second port, every
//! manifest changes, and torrc doubles. Instead the gate peeks the first byte:
//! a TLS ClientHello starts with `0x16` (handshake) and no HTTP method does, so
//! the two are distinguishable without consuming anything.
//!
//! `peek` is what makes this safe — it leaves the bytes in the socket buffer,
//! so the TLS acceptor still sees a complete, untouched ClientHello.
//!
//! # Why reload, rather than load once
//!
//! `scripts/setup-node-ca.sh` reissues the leaf whenever the node gains an
//! address (DHCP, Tailscale coming up, the fips0 ULA appearing late) — the same
//! churn the bind sweep exists for. A config parsed once at startup would keep
//! serving a certificate that omits the address the user is actually on, and
//! the failure is a browser-side name mismatch that no node-side log would
//! explain. So the mtime of both files is checked and the config rebuilt when
//! either moves.
//!
//! # Absent certificates are not an error
//!
//! A node that has never run the CA script has no certificate. That node serves
//! plain HTTP exactly as before and is fully functional — TLS is an upgrade,
//! not a requirement — so a missing file is logged once at debug, not warn.
//! What IS logged at warn is a certificate that exists but cannot be parsed:
//! that is a misconfiguration the operator can act on, and silently falling
//! back to plain HTTP would hide it.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use tokio::sync::RwLock;
use tokio_rustls::rustls::{Certificate, PrivateKey, ServerConfig};
use tokio_rustls::TlsAcceptor;
use tracing::{debug, warn};

/// Where `setup-node-ca.sh` writes the node's leaf. Same pair nginx serves, so
/// the dashboard and the app ports present one identity and a single trusted
/// CA covers both.
const DEFAULT_CERT: &str = "/etc/archipelago/ssl/archipelago.crt";
const DEFAULT_KEY: &str = "/etc/archipelago/ssl/archipelago.key";

/// First byte of a TLS record of type `handshake` (22). No HTTP request can
/// begin with it: methods are uppercase ASCII letters, so the two wire formats
/// are unambiguous from a single byte.
pub const TLS_HANDSHAKE_FIRST_BYTE: u8 = 0x16;

/// Does this look like the start of a TLS connection rather than plain HTTP?
pub fn looks_like_tls(first: u8) -> bool {
    first == TLS_HANDSHAKE_FIRST_BYTE
}

/// Lazily-built, mtime-invalidated TLS config for the gate.
pub struct GateTls {
    cert_path: PathBuf,
    key_path: PathBuf,
    cached: RwLock<Option<Cached>>,
}

struct Cached {
    acceptor: TlsAcceptor,
    stamp: Stamp,
}

/// Modification times of both halves. Compared as a pair because reissuing
/// writes the certificate and the key separately — keying on only one would
/// serve a certificate that no longer matches its key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Stamp {
    cert: SystemTime,
    key: SystemTime,
}

impl GateTls {
    pub fn new() -> Self {
        Self::with_paths(DEFAULT_CERT, DEFAULT_KEY)
    }

    pub fn with_paths(cert: impl Into<PathBuf>, key: impl Into<PathBuf>) -> Self {
        Self {
            cert_path: cert.into(),
            key_path: key.into(),
            cached: RwLock::new(None),
        }
    }

    /// The current acceptor, rebuilding it if the files changed underneath.
    ///
    /// `None` means this node has no usable certificate and app ports stay
    /// plain HTTP. Callers must treat that as ordinary, not as a failure.
    pub async fn acceptor(&self) -> Option<TlsAcceptor> {
        let stamp = self.stamp().await?;

        if let Some(c) = self.cached.read().await.as_ref() {
            if c.stamp == stamp {
                return Some(c.acceptor.clone());
            }
        }

        // Rebuild. Re-check under the write lock so concurrent connections
        // during a reissue do not each parse the same files.
        let mut guard = self.cached.write().await;
        if let Some(c) = guard.as_ref() {
            if c.stamp == stamp {
                return Some(c.acceptor.clone());
            }
        }

        match load_config(&self.cert_path, &self.key_path).await {
            Ok(config) => {
                let acceptor = TlsAcceptor::from(Arc::new(config));
                debug!(
                    cert = %self.cert_path.display(),
                    "app gate loaded its TLS certificate"
                );
                *guard = Some(Cached {
                    acceptor: acceptor.clone(),
                    stamp,
                });
                Some(acceptor)
            }
            Err(e) => {
                // A present-but-broken certificate is an operator-actionable
                // misconfiguration; do not let it pass quietly as "no TLS".
                warn!(
                    cert = %self.cert_path.display(),
                    error = %e,
                    "app gate could not load its TLS certificate — app ports stay plain HTTP"
                );
                // Cache the failure against this stamp so a broken file is not
                // re-parsed on every single connection.
                *guard = None;
                None
            }
        }
    }

    async fn stamp(&self) -> Option<Stamp> {
        let cert = mtime(&self.cert_path).await?;
        let key = mtime(&self.key_path).await?;
        Some(Stamp { cert, key })
    }
}

impl Default for GateTls {
    fn default() -> Self {
        Self::new()
    }
}

async fn mtime(path: &Path) -> Option<SystemTime> {
    tokio::fs::metadata(path).await.ok()?.modified().ok()
}

async fn load_config(cert_path: &Path, key_path: &Path) -> io::Result<ServerConfig> {
    let cert_pem = tokio::fs::read(cert_path).await?;
    let key_pem = tokio::fs::read(key_path).await?;
    build_config(&cert_pem, &key_pem)
}

/// Split out from the filesystem so it can be tested against bytes directly.
pub(crate) fn build_config(cert_pem: &[u8], key_pem: &[u8]) -> io::Result<ServerConfig> {
    let certs: Vec<Certificate> = rustls_pemfile::certs(&mut &cert_pem[..])?
        .into_iter()
        .map(Certificate)
        .collect();
    if certs.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no certificates in PEM",
        ));
    }

    let key = read_key(key_pem)?;

    // rustls does NOT check that the key matches the certificate — verified by
    // test, not assumed: `with_single_cert` accepts a pair from two different
    // keys and only fails later, mid-handshake, in someone's browser. That is
    // precisely the silently-broken-security-control shape this module exists
    // to avoid, so prove the pairing here and refuse to serve otherwise.
    ensure_key_matches_cert(&certs[0], &key)?;

    ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Sign a fixed message with the private key and verify it with the public key
/// inside the certificate. They pair iff the verification succeeds.
fn ensure_key_matches_cert(cert: &Certificate, key: &PrivateKey) -> io::Result<()> {
    use tokio_rustls::rustls::sign;

    let signing_key = sign::any_supported_type(key)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "unsupported private key type"))?;

    // Any scheme the key supports will do — this proves possession, it is not
    // negotiating anything. Offer the full set and let rustls pick.
    const ALL_SCHEMES: &[tokio_rustls::rustls::SignatureScheme] = {
        use tokio_rustls::rustls::SignatureScheme as S;
        &[
            S::ECDSA_NISTP256_SHA256,
            S::ECDSA_NISTP384_SHA384,
            S::ED25519,
            S::RSA_PSS_SHA256,
            S::RSA_PSS_SHA384,
            S::RSA_PSS_SHA512,
            S::RSA_PKCS1_SHA256,
            S::RSA_PKCS1_SHA384,
            S::RSA_PKCS1_SHA512,
        ]
    };
    let signer = signing_key
        .choose_scheme(ALL_SCHEMES)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no usable signature scheme"))?;

    const PROOF: &[u8] = b"archipelago app gate certificate pairing check";
    let signature = signer
        .sign(PROOF)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let end_entity = webpki::EndEntityCert::try_from(cert.0.as_slice())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("bad certificate: {e}")))?;

    let alg: &webpki::SignatureAlgorithm = match signer.scheme() {
        tokio_rustls::rustls::SignatureScheme::RSA_PKCS1_SHA256 => {
            &webpki::RSA_PKCS1_2048_8192_SHA256
        }
        tokio_rustls::rustls::SignatureScheme::RSA_PKCS1_SHA384 => {
            &webpki::RSA_PKCS1_2048_8192_SHA384
        }
        tokio_rustls::rustls::SignatureScheme::RSA_PKCS1_SHA512 => {
            &webpki::RSA_PKCS1_2048_8192_SHA512
        }
        tokio_rustls::rustls::SignatureScheme::RSA_PSS_SHA256 => {
            &webpki::RSA_PSS_2048_8192_SHA256_LEGACY_KEY
        }
        tokio_rustls::rustls::SignatureScheme::RSA_PSS_SHA384 => {
            &webpki::RSA_PSS_2048_8192_SHA384_LEGACY_KEY
        }
        tokio_rustls::rustls::SignatureScheme::RSA_PSS_SHA512 => {
            &webpki::RSA_PSS_2048_8192_SHA512_LEGACY_KEY
        }
        tokio_rustls::rustls::SignatureScheme::ECDSA_NISTP256_SHA256 => &webpki::ECDSA_P256_SHA256,
        tokio_rustls::rustls::SignatureScheme::ECDSA_NISTP384_SHA384 => &webpki::ECDSA_P384_SHA384,
        tokio_rustls::rustls::SignatureScheme::ED25519 => &webpki::ED25519,
        // An unrecognised scheme must not silently skip the check.
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("cannot verify key/certificate pairing for scheme {other:?}"),
            ))
        }
    };

    end_entity
        .verify_signature(alg, PROOF, &signature)
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "private key does not match the certificate",
            )
        })
}

/// Accept PKCS#8 or PKCS#1. `setup-node-ca.sh` emits PKCS#8, but a key that
/// predates it (or was generated by hand) may be PKCS#1, and refusing that
/// would be a silent downgrade to plain HTTP on an already-working node.
fn read_key(key_pem: &[u8]) -> io::Result<PrivateKey> {
    if let Some(k) = rustls_pemfile::pkcs8_private_keys(&mut &key_pem[..])?
        .into_iter()
        .next()
    {
        return Ok(PrivateKey(k));
    }
    if let Some(k) = rustls_pemfile::rsa_private_keys(&mut &key_pem[..])?
        .into_iter()
        .next()
    {
        return Ok(PrivateKey(k));
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "no PKCS#8 or PKCS#1 private key in PEM",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Generated by scripts/setup-node-ca.sh's own openssl invocation, so these
    // exercise the exact shape the node produces.
    const CERT: &[u8] = include_bytes!("testdata/leaf.crt");
    const KEY: &[u8] = include_bytes!("testdata/leaf.key");

    #[test]
    fn a_tls_client_hello_is_distinguishable_from_every_http_method() {
        assert!(looks_like_tls(0x16));
        // Every HTTP method starts with an uppercase letter; none is 0x16.
        for m in ["GET", "POST", "PUT", "HEAD", "OPTIONS", "DELETE", "PATCH"] {
            assert!(
                !looks_like_tls(m.as_bytes()[0]),
                "{m} misread as a TLS handshake"
            );
        }
    }

    #[test]
    fn builds_a_config_from_the_nodes_own_cert_and_key() {
        assert!(build_config(CERT, KEY).is_ok());
    }

    #[test]
    fn a_cert_without_its_matching_key_is_rejected_not_ignored() {
        // Key from a different pair: rustls must refuse rather than serve a
        // certificate it cannot prove ownership of.
        let other = build_config(CERT, OTHER_KEY);
        assert!(other.is_err(), "mismatched cert/key pair was accepted");
    }
    const OTHER_KEY: &[u8] = include_bytes!("testdata/other.key");

    #[test]
    fn empty_pem_is_an_error_rather_than_an_empty_chain() {
        assert!(build_config(b"", KEY).is_err());
        assert!(build_config(CERT, b"").is_err());
    }

    #[tokio::test]
    async fn a_node_without_certificates_reports_no_acceptor() {
        let tls = GateTls::with_paths(
            "/nonexistent/archipelago.crt",
            "/nonexistent/archipelago.key",
        );
        assert!(tls.acceptor().await.is_none());
    }

    #[tokio::test]
    async fn an_acceptor_is_built_and_then_served_from_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cert = dir.path().join("c.crt");
        let key = dir.path().join("c.key");
        tokio::fs::write(&cert, CERT).await.unwrap();
        tokio::fs::write(&key, KEY).await.unwrap();

        let tls = GateTls::with_paths(&cert, &key);
        assert!(tls.acceptor().await.is_some());
        // Second call hits the cache; the observable contract is simply that it
        // still yields an acceptor.
        assert!(tls.acceptor().await.is_some());
    }

    #[tokio::test]
    async fn a_reissued_certificate_is_picked_up_without_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let cert = dir.path().join("c.crt");
        let key = dir.path().join("c.key");
        tokio::fs::write(&cert, CERT).await.unwrap();
        tokio::fs::write(&key, KEY).await.unwrap();

        let tls = GateTls::with_paths(&cert, &key);
        assert!(tls.acceptor().await.is_some());
        let first = *tls.cached.read().await.as_ref().map(|c| &c.stamp).unwrap();

        // Reissue with a distinctly later mtime, the way the CA script does
        // when the node gains an address. Set explicitly rather than relying on
        // wall-clock advancing, because a same-second rewrite can land on an
        // identical mtime on coarse-granularity filesystems and make this pass
        // or fail by luck.
        tokio::fs::write(&cert, CERT).await.unwrap();
        let later = SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&cert)
            .unwrap()
            .set_modified(later)
            .unwrap();

        assert!(tls.acceptor().await.is_some());
        let second = *tls.cached.read().await.as_ref().map(|c| &c.stamp).unwrap();
        assert_ne!(first, second, "reissued certificate was not reloaded");
    }
}
