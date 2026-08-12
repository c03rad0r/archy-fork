package com.archipelago.app.fips

import org.json.JSONObject

/**
 * JNI binding to the embedded FIPS mesh node (Android/rust/archy-fips-core,
 * built into libarchy_fips_core.so by the buildRustArm64 gradle task).
 *
 * All calls return JSON strings; failures come back as {"error": "…"} rather
 * than exceptions. [available] is false on ABIs the .so isn't built for
 * (anything but arm64) — every caller must gate on it so the app still runs
 * as a plain companion there.
 */
object FipsNative {
    val available: Boolean = try {
        System.loadLibrary("archy_fips_core")
        true
    } catch (_: Throwable) {
        false
    }

    external fun generateIdentity(): String
    external fun deriveIdentity(secret: String): String

    /**
     * [listenPort] 0 = outbound-only (default posture). Non-zero binds UDP on
     * that port so a nearby phone can dial us directly (party mode); the node
     * stays leaf-only either way.
     */
    external fun start(secret: String, peersJson: String, tunFd: Int, listenPort: Int): String
    external fun stop()
    external fun isRunning(): Boolean
    external fun statusJson(): String

    data class Identity(val secret: String, val npub: String, val address: String)

    /** Parse an identity JSON reply; null on {"error": …} or malformed. */
    fun parseIdentity(json: String): Identity? = try {
        val obj = JSONObject(json)
        if (obj.has("error")) null
        else Identity(
            secret = obj.getString("secret"),
            npub = obj.getString("npub"),
            address = obj.getString("address"),
        )
    } catch (_: Exception) {
        null
    }
}
