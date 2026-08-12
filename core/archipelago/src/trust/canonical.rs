//! Canonical JSON for signing — a pragmatic subset of RFC 8785 (JCS).
//!
//! Signatures are computed over a *byte-exact* serialization so that a verifier
//! reproduces the same preimage the signer hashed. We guarantee:
//!
//!   * object keys recursively sorted (lexicographic by Rust `str` ordering,
//!     i.e. Unicode scalar value — matches JCS for the ASCII keys we use),
//!   * no insignificant whitespace,
//!   * arrays preserved in order.
//!
//! We do NOT implement JCS number canonicalization (ECMAScript shortest-form).
//! Archipelago manifests/catalogs carry only integers, strings, bools, arrays
//! and objects, for which `serde_json`'s output is already unambiguous. If a
//! float ever enters a signed document this must be hardened (or rejected).
//! `contains_float()` lets callers enforce that invariant.

use serde_json::Value;

/// Serialize `value` to canonical JSON bytes (sorted keys, compact).
///
/// Rebuilds every object through a `BTreeMap` so the result is independent of
/// the `serde_json/preserve_order` feature being toggled on anywhere in the
/// dependency graph.
pub fn to_canonical_bytes(value: &Value) -> Vec<u8> {
    let canonical = canonicalize(value);
    // serde_json never fails to serialize a Value it produced.
    serde_json::to_vec(&canonical).expect("canonical JSON serialization")
}

/// Reject documents that contain a float anywhere — they are not safely
/// canonicalizable under this implementation.
pub fn contains_float(value: &Value) -> bool {
    match value {
        Value::Number(n) => n.as_i64().is_none() && n.as_u64().is_none(),
        Value::Array(items) => items.iter().any(contains_float),
        Value::Object(map) => map.values().any(contains_float),
        _ => false,
    }
}

fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            // BTreeMap gives deterministic key ordering on serialize.
            let sorted: std::collections::BTreeMap<String, Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), canonicalize(v)))
                .collect();
            serde_json::to_value(sorted).expect("canonical object")
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn key_order_does_not_change_bytes() {
        let a = json!({"b": 1, "a": 2, "c": {"z": 1, "y": 2}});
        let b = json!({"c": {"y": 2, "z": 1}, "a": 2, "b": 1});
        assert_eq!(to_canonical_bytes(&a), to_canonical_bytes(&b));
    }

    #[test]
    fn output_is_sorted_and_compact() {
        let v = json!({"b": 1, "a": [3, 2, 1]});
        assert_eq!(to_canonical_bytes(&v), br#"{"a":[3,2,1],"b":1}"#.to_vec());
    }

    #[test]
    fn array_order_is_preserved() {
        let a = json!([1, 2, 3]);
        let b = json!([3, 2, 1]);
        assert_ne!(to_canonical_bytes(&a), to_canonical_bytes(&b));
    }

    #[test]
    fn detects_floats() {
        assert!(contains_float(&json!({"x": 1.5})));
        assert!(contains_float(&json!([1, 2, 0.1])));
        assert!(!contains_float(&json!({"x": 12345, "y": "s", "z": [1, 2]})));
    }
}
