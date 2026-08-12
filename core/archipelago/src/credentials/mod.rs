//! Verifiable Credentials (VC) management following W3C VC Data Model 2.0.
//! Implements JSON-LD @context, Ed25519Signature2020 proof format.
//! See: https://www.w3.org/TR/vc-data-model-2.0/

mod operations;
mod presentation;
mod store;
mod types;

pub use operations::{
    is_revoked, issue_credential, list_credentials, revoke_credential, verify_credential,
};
pub use presentation::{create_presentation, verify_presentation, VerifiablePresentation};
pub use store::load_credentials;
