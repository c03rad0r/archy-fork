package com.archipelago.app

import org.json.JSONObject

/**
 * JNI binding to the companion's non-mesh native surface (same
 * libarchy_fips_core.so as FipsNative — backup + nostr signer crypto, built
 * from Android/rust/archy-fips-core).
 *
 * Same contract as FipsNative: JSON over strings, failures come back as
 * {"error": "…"} rather than exceptions, and [available] is false on ABIs
 * the .so isn't built for so every caller can degrade gracefully.
 */
object NativeCore {
    val available: Boolean = try {
        System.loadLibrary("archy_fips_core")
        true
    } catch (_: Throwable) {
        false
    }

    // ── Backup (#128): the node's ADR-005 envelope ──────────────────────────

    /** Encrypt a JSON payload into an ADR-005 envelope (ChaCha20-Poly1305). */
    external fun backupEncrypt(payload: String, passphrase: String): String

    /** Decrypt an ADR-005 envelope back to its payload JSON. */
    external fun backupDecrypt(envelope: String, passphrase: String): String

    // ── NIP-46 remote signer (#139) ─────────────────────────────────────────

    /** Generate a fresh nostr key: {"secret","pubkey","npub","nsec"}. */
    external fun nostrGenerateSecret(): String

    /** Import a key from hex or nsec…: {"secret","pubkey","npub","nsec"}. */
    external fun nostrSecretFromAny(secret: String): String

    /** Parse nostrconnect://…: {"clientPubkey","relays":[…],"secret","perms","name","url","image"}. */
    external fun nostrParseConnectUri(uri: String): String

    /**
     * Sign `{kind, content, tags, created_at}` with the signer key: returns
     * the full signed event JSON. Approval happens BEFORE this call — the
     * native side never signs unasked.
     */
    external fun nostrSignEvent(secretHex: String, eventJson: String): String

    /** NIP-44 v2 encrypt/decrypt; result JSON: {"result": payload} or {"error": …}. */
    external fun nostrNip44Encrypt(secretHex: String, peerPub: String, plaintext: String): String
    external fun nostrNip44Decrypt(secretHex: String, peerPub: String, payload: String): String

    /** NIP-04 fallback (deprecated but still spoken by real clients). */
    external fun nostrNip04Encrypt(secretHex: String, peerPub: String, plaintext: String): String
    external fun nostrNip04Decrypt(secretHex: String, peerPub: String, payload: String): String

    /** True when a native reply is an error envelope. */
    fun isErr(json: String): Boolean = try {
        JSONObject(json).has("error")
    } catch (_: Exception) {
        true
    }

    /** Error text from a native reply, or a generic message if malformed. */
    fun errMsg(json: String): String = try {
        JSONObject(json).optString("error", "native call failed")
    } catch (_: Exception) {
        "native call failed"
    }
}
