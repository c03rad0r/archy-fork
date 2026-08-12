//! TPM 2.0 hardware attestation module.
//!
//! Nodes with TPM chips can cryptographically prove their hardware identity,
//! adding a trust layer to federation. The TPM attestation key is included
//! in the node's DID Document as an additional verification method.
//!
//! Requires: tss-esapi crate (TPM2 Software Stack) and physical TPM 2.0 chip.

use serde::{Deserialize, Serialize};

/// TPM attestation status for a node.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TpmStatus {
    /// Whether a TPM 2.0 chip was detected
    pub available: bool,
    /// TPM manufacturer info
    pub manufacturer: Option<String>,
    /// Firmware version
    pub firmware_version: Option<String>,
    /// Whether an attestation key has been generated
    pub attestation_key_created: bool,
    /// Public part of the attestation key (hex)
    pub attestation_pubkey: Option<String>,
}

/// TPM attestation for inclusion in DID Documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpmAttestation {
    /// Attestation type (e.g., "TpmAttestationKey2023")
    pub attestation_type: String,
    /// TPM public key (hex-encoded)
    pub public_key: String,
    /// Platform Certificate (if available)
    pub platform_cert: Option<String>,
    /// Quote signature over node's DID (proves TPM controls this identity)
    pub quote_signature: Option<String>,
}

/// Check if TPM 2.0 is available on this system.
pub fn detect_tpm() -> TpmStatus {
    // Check /dev/tpm0 or /dev/tpmrm0
    let tpm_device = std::path::Path::new("/dev/tpmrm0").exists()
        || std::path::Path::new("/dev/tpm0").exists();

    TpmStatus {
        available: tpm_device,
        manufacturer: None,
        firmware_version: None,
        attestation_key_created: false,
        attestation_pubkey: None,
    }
}
