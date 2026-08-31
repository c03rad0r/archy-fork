package com.archipelago.app.data

import android.content.Context
import com.archipelago.app.NativeCore
import com.archipelago.app.fips.FipsPreferences
import com.archipelago.app.nostr.NostrSignerPreferences
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

/**
 * Companion backup & restore (#128) — the phone side of "losing your phone,
 * or wiping it to cross a border".
 *
 * The payload (servers + FIPS identity/peers + signer key + flags) is
 * serialized to JSON and sealed into the node's ADR-005 envelope (Argon2id +
 * ChaCha20-Poly1305) by the native core — the SAME envelope the node uses,
 * not a second format. The passphrase never leaves the encrypt call.
 *
 * Transport is deliberately boring: a plain .json file the user saves via
 * the system file picker (SAF) — on GrapheneOS there is no cloud backup and
 * there should be none here either; the file goes wherever the user puts it
 * (USB drive, computer, a folder synced their way).
 */
class BackupManager(private val context: Context) {

    private val servers = ServerPreferences(context)
    private val fips = FipsPreferences(context)
    private val signer = NostrSignerPreferences(context)

    /** Everything the backup captures, for the restore preview UI. */
    data class PayloadSummary(
        val serverCount: Int,
        val hasFipsIdentity: Boolean,
        val hasSignerKey: Boolean,
        val appVersion: String,
    )

    /** What a restore actually did, for the result UI. */
    data class RestoreResult(
        val serversRestored: Int,
        val activeSet: Boolean,
        val fipsIdentityRestored: Boolean,
        val signerKeyRestored: Boolean,
    )

    private fun appVersion(): String = try {
        context.packageManager.getPackageInfo(context.packageName, 0).versionName ?: ""
    } catch (_: Exception) {
        ""
    }

    /**
     * Assemble the encrypted backup envelope. Runs on IO: DataStore reads
     * plus the Argon2id KDF (tens of ms) + AEAD.
     */
    suspend fun createBackup(passphrase: String): String = withContext(Dispatchers.IO) {
        require(passphrase.isNotEmpty()) { "passphrase required" }

        val active = servers.activeServer.first()
        val saved = servers.savedServers.first()
        val fipsId = fips.identity()
        val peers = fips.peersJson()
        val partyPeers = fips.partyPeers()
        val partyName = fips.partyName()
        val partyListen = fips.partyListen()
        val signerSecret = signer.secret()

        val payload = JSONObject().apply {
            put("app", "archipelago-companion")
            put("payloadVersion", 1)
            put("appVersion", appVersion())
            put("createdAt", System.currentTimeMillis() / 1000)
            put("servers", JSONArray(saved.map { it.serialize() }))
            put("active", active?.serialize() ?: JSONObject.NULL)
            if (fipsId != null) {
                put("fips", JSONObject().apply {
                    put("secret", fipsId.secret)
                    put("npub", fipsId.npub)
                    put("address", fipsId.address)
                    put("peers", JSONArray(peers))
                    put("partyPeers", JSONArray().apply { partyPeers.forEach { put(JSONObject().apply {
                        put("npub", it.npub); put("ula", it.ula); put("name", it.name)
                        put("ip", it.ip); put("port", it.port)
                    }) } })
                    put("partyName", partyName)
                    put("partyListen", partyListen)
                })
            }
            if (signerSecret != null) {
                put("signer", JSONObject().apply { put("secret", signerSecret) })
            }
            put("flags", JSONObject().apply {
                put("introSeen", servers.introSeen.first())
            })
        }

        val envelope = NativeCore.backupEncrypt(payload.toString(), passphrase)
        if (NativeCore.isErr(envelope)) throw BackupException(NativeCore.errMsg(envelope))
        envelope
    }

    /**
     * Peek at a decrypted backup (passphrase already checked) to preview what
     * a restore would do. Does NOT touch any stored state.
     */
    suspend fun readBackup(envelope: String, passphrase: String): Pair<PayloadSummary, JSONObject> =
        withContext(Dispatchers.IO) {
            val payload = NativeCore.backupDecrypt(envelope, passphrase)
            if (NativeCore.isErr(payload)) throw BackupException(NativeCore.errMsg(payload))
            val obj = JSONObject(payload)
            if (obj.optString("app") != "archipelago-companion") {
                throw BackupException("Not a companion backup (this may be a node backup — restore it on the node)")
            }
            val summary = PayloadSummary(
                serverCount = obj.optJSONArray("servers")?.length() ?: 0,
                hasFipsIdentity = obj.has("fips"),
                hasSignerKey = obj.has("signer"),
                appVersion = obj.optString("appVersion", ""),
            )
            summary to obj
        }

    /**
     * Apply a decrypted backup to this install. Merge semantics — a restore
     * never silently destroys what's already here:
     *
     * - Servers upsert (npub-first, [ServerPreferences.upsertServer]) — same
     *   identity merges, never duplicates.
     * - The backup's active server is set active only when none is.
     * - FIPS identity/peers restore only when this phone has none (a phone
     *   that already paired has a live identity the node peers with; swapping
     *   it from a backup would strand the current pairing). Peers merge by
     *   npub otherwise.
     * - Signer key restores only when none exists locally.
     */
    suspend fun restoreBackup(payload: JSONObject): RestoreResult = withContext(Dispatchers.IO) {
        val serverArray = payload.optJSONArray("servers") ?: JSONArray()
        var restored = 0
        for (i in 0 until serverArray.length()) {
            val raw = serverArray.optString(i)
            val entry = ServerEntry.deserialize(raw) ?: continue
            servers.upsertServer(entry)
            restored++
        }

        var activeSet = false
        val activeStr = if (payload.isNull("active")) null else payload.optString("active", "")
        val activeEntry = activeStr?.takeIf { it.isNotBlank() }?.let { ServerEntry.deserialize(it) }
        if (activeEntry != null && servers.activeServer.first() == null) {
            servers.setActiveServer(activeEntry)
            activeSet = true
        }

        // FIPS identity: only adopt when this phone has none.
        var fipsRestored = false
        val fipsObj = payload.optJSONObject("fips")
        if (fipsObj != null && fips.identity() == null) {
            val secret = fipsObj.optString("secret")
            if (secret.isNotBlank()) {
                fips.saveIdentity(
                    com.archipelago.app.fips.FipsNative.Identity(
                        secret = secret,
                        npub = fipsObj.optString("npub"),
                        address = fipsObj.optString("address"),
                    )
                )
                fipsRestored = true
            }
            // Peers: union by npub with whatever is already here (an empty
            // store takes the backup's list wholesale).
            val backupPeers = fipsObj.optJSONArray("peers")?.let { arr ->
                (0 until arr.length()).joinToString(",", "[", "]") { arr.optString(it) }
            } ?: "[]"
            fips.mergePeersJson(backupPeers)

            val partyArr = fipsObj.optJSONArray("partyPeers")
            if (partyArr != null) {
                for (i in 0 until partyArr.length()) {
                    val p = partyArr.optJSONObject(i) ?: continue
                    val npub = p.optString("npub")
                    val ula = p.optString("ula")
                    if (npub.isNotBlank() && ula.isNotBlank()) {
                        fips.upsertPartyPeer(
                            com.archipelago.app.fips.PartyPeer(
                                npub = npub, ula = ula,
                                name = p.optString("name").ifBlank { "Phone" },
                                ip = p.optString("ip"), port = p.optInt("port"),
                            )
                        )
                    }
                }
            }
            if (fipsObj.optString("partyName").isNotBlank()) {
                fips.setPartyName(fipsObj.optString("partyName"))
            }
            fips.setPartyListen(fipsObj.optBoolean("partyListen", false))
        }

        // Signer key: only adopt when none exists locally.
        var signerRestored = false
        val signerObj = payload.optJSONObject("signer")
        if (signerObj != null && signer.secret() == null) {
            val secret = signerObj.optString("secret")
            if (secret.isNotBlank()) {
                signer.saveSecret(secret)
                signerRestored = true
            }
        }

        // Flags: a user who completed the intro on the old phone shouldn't
        // see it again on the new one.
        val flags = payload.optJSONObject("flags")
        if (flags?.optBoolean("introSeen", false) == true) {
            servers.markIntroSeen()
        }

        RestoreResult(
            serversRestored = restored,
            activeSet = activeSet,
            fipsIdentityRestored = fipsRestored,
            signerKeyRestored = signerRestored,
        )
    }

    class BackupException(message: String) : Exception(message)
}
