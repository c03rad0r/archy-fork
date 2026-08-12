package com.archipelago.app.data

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.core.stringSetPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map

private val Context.dataStore: DataStore<Preferences> by preferencesDataStore(name = "server_prefs")

data class ServerEntry(
    val address: String,
    val useHttps: Boolean,
    val port: String = "",
    val password: String = "",
    val name: String = "",
    /** Node's FIPS mesh ULA (IPv6) — reachable from anywhere once meshed. */
    val meshIp: String = "",
    /** Node's FIPS npub — the durable identity. When present it, not the
     *  address, is what identifies the entry: FIPS peers on npubs, IPs are
     *  only dial hints (docs/companion-pairing-qr.md, npub-first contract). */
    val npub: String = "",
) {
    /** Label to show in lists — the user-given name, or the address if unnamed. */
    fun displayName(): String = name.ifBlank { address }

    /**
     * Is this node reachable over the Archipelago FIPS mesh?
     *
     * A node that advertised either identity (npub) or a mesh address (ULA)
     * came from a FIPS-capable pairing QR. Anything else — a hand-entered LAN
     * box, someone else's server behind their own VPN — is a plain HTTP
     * target, and the companion must NOT raise its own tunnel for it: Android
     * allows exactly one VPN at a time, so doing so would silently take the
     * tunnel away from whatever the user actually uses to reach that node.
     */
    fun isFipsNode(): Boolean = npub.isNotBlank() || meshIp.isNotBlank()

    /** Bracket bare IPv6 literals (the mesh ULA) so they form valid URLs. */
    private fun urlHost(host: String): String =
        if (host.contains(":") && !host.startsWith("[")) "[$host]" else host

    fun toUrl(): String {
        val scheme = if (useHttps) "https" else "http"
        val portSuffix = if (port.isNotBlank()) ":$port" else ""
        return "$scheme://${urlHost(address)}$portSuffix"
    }

    fun toWsUrl(): String {
        val scheme = if (useHttps) "wss" else "ws"
        val portSuffix = if (port.isNotBlank()) ":$port" else ""
        return "$scheme://${urlHost(address)}$portSuffix"
    }

    /** Mesh-address UI URL, or null when the node never advertised one. */
    fun toMeshUrl(): String? =
        meshIp.takeIf { it.isNotBlank() }?.let { "http://${urlHost(it)}" }

    // name/meshIp/npub are trailing fields so entries saved before they
    // existed (4/5/6 fields) still deserialize, defaulting to "".
    fun serialize(): String = "$address|$useHttps|$port|$password|$name|$meshIp|$npub"

    /** Same node as [other]? npub identity wins; address/port/scheme is the
     *  fallback for LAN-only entries that never advertised FIPS. */
    fun sameNode(other: ServerEntry): Boolean =
        (npub.isNotBlank() && npub == other.npub) ||
            (address == other.address && port == other.port && useHttps == other.useHttps)

    companion object {
        fun deserialize(raw: String): ServerEntry? {
            val parts = raw.split("|")
            if (parts.size < 2) return null
            return ServerEntry(
                address = parts[0],
                useHttps = parts[1].toBooleanStrictOrNull() ?: false,
                port = parts.getOrElse(2) { "" },
                password = parts.getOrElse(3) { "" },
                name = parts.getOrElse(4) { "" },
                meshIp = parts.getOrElse(5) { "" },
                npub = parts.getOrElse(6) { "" },
            )
        }
    }
}

class ServerPreferences(private val context: Context) {

    private val activeAddressKey = stringPreferencesKey("active_address")
    private val activeHttpsKey = booleanPreferencesKey("active_https")
    private val activePortKey = stringPreferencesKey("active_port")
    private val activePasswordKey = stringPreferencesKey("active_password")
    private val activeNameKey = stringPreferencesKey("active_name")
    private val activeMeshIpKey = stringPreferencesKey("active_mesh_ip")
    private val activeNpubKey = stringPreferencesKey("active_npub")
    private val savedServersKey = stringSetPreferencesKey("saved_servers")
    private val introSeenKey = booleanPreferencesKey("intro_seen")
    private val gestureHintSeenKey = booleanPreferencesKey("gesture_hint_seen")

    private fun activeServerFrom(prefs: Preferences): ServerEntry? {
        val address = prefs[activeAddressKey] ?: return null
        return ServerEntry(
            address = address,
            useHttps = prefs[activeHttpsKey] ?: false,
            port = prefs[activePortKey] ?: "",
            password = prefs[activePasswordKey] ?: "",
            name = prefs[activeNameKey] ?: "",
            meshIp = prefs[activeMeshIpKey] ?: "",
            npub = prefs[activeNpubKey] ?: "",
        )
    }

    // distinctUntilChanged on every flow: DataStore emits on EVERY write to the
    // file regardless of key, and each spurious emission recomposed whatever
    // screen collected it (the kiosk recomposed on gesture-hint writes).

    val activeServer: Flow<ServerEntry?> = context.dataStore.data
        .map { prefs -> activeServerFrom(prefs) }
        .distinctUntilChanged()

    val savedServers: Flow<List<ServerEntry>> = context.dataStore.data.map { prefs ->
        val raw = prefs[savedServersKey] ?: emptySet()
        // Sorted so set-iteration order can't produce a structurally different
        // list for the same servers (which defeats distinctUntilChanged).
        raw.mapNotNull { ServerEntry.deserialize(it) }.sortedBy { it.displayName() }
    }.distinctUntilChanged()

    val introSeen: Flow<Boolean> = context.dataStore.data.map { prefs ->
        prefs[introSeenKey] ?: false
    }.distinctUntilChanged()

    /** One-shot flag for the three-finger-hold teaching overlay. */
    val gestureHintSeen: Flow<Boolean> = context.dataStore.data.map { prefs ->
        prefs[gestureHintSeenKey] ?: false
    }.distinctUntilChanged()

    /** Everything the nav graph needs to pick a start destination, derived
     *  from ONE DataStore emission. Collecting introSeen and activeServer as
     *  two separate flows let them land in different frames — the intro flag
     *  could resolve first and flash the Connect screen at a paired user
     *  before the active server arrived. */
    data class LaunchState(
        val introSeen: Boolean,
        val activeServer: ServerEntry?,
        /** Every saved node — the launch gate needs the COUNT to decide
         *  whether to ask which one to connect to. */
        val savedServers: List<ServerEntry>,
    )

    val launchState: Flow<LaunchState> = context.dataStore.data.map { prefs ->
        LaunchState(
            introSeen = prefs[introSeenKey] ?: false,
            activeServer = activeServerFrom(prefs),
            savedServers = (prefs[savedServersKey] ?: emptySet())
                .mapNotNull { ServerEntry.deserialize(it) }
                .sortedBy { it.displayName() },
        )
    }.distinctUntilChanged()

    suspend fun setActiveServer(server: ServerEntry) {
        context.dataStore.edit { prefs ->
            prefs[activeAddressKey] = server.address
            prefs[activeHttpsKey] = server.useHttps
            prefs[activePortKey] = server.port
            prefs[activePasswordKey] = server.password
            prefs[activeNameKey] = server.name
            prefs[activeMeshIpKey] = server.meshIp
            prefs[activeNpubKey] = server.npub
        }
        addSavedServer(server)
    }

    suspend fun clearActiveServer() {
        context.dataStore.edit { prefs ->
            prefs.remove(activeAddressKey)
            prefs.remove(activeHttpsKey)
            prefs.remove(activePortKey)
            prefs.remove(activePasswordKey)
            prefs.remove(activeNameKey)
            prefs.remove(activeMeshIpKey)
            prefs.remove(activeNpubKey)
        }
    }

    suspend fun addSavedServer(server: ServerEntry) {
        context.dataStore.edit { prefs ->
            val current = prefs[savedServersKey] ?: emptySet()
            prefs[savedServersKey] = current + server.serialize()
        }
    }

    /**
     * Replace a saved server in place. Matches the existing entry by node
     * identity — npub first, address/port/scheme as the LAN-only fallback
     * (ServerEntry.sameNode) — so edits that change the name, password or even
     * every address still update the right record. An edit form that doesn't
     * carry the npub keeps the stored one. If the edited server is also the
     * active one, the active record is kept in sync.
     */
    suspend fun updateSavedServer(original: ServerEntry, updated: ServerEntry) {
        val toStore = updated.copy(npub = updated.npub.ifBlank { original.npub })
        context.dataStore.edit { prefs ->
            val current = prefs[savedServersKey] ?: emptySet()
            val filtered = current.filterNot { raw ->
                ServerEntry.deserialize(raw)?.sameNode(original) == true
            }.toSet()
            prefs[savedServersKey] = filtered + toStore.serialize()

            val activeNpub = prefs[activeNpubKey] ?: ""
            val isActive = (activeNpub.isNotBlank() && activeNpub == original.npub) ||
                (
                    prefs[activeAddressKey] == original.address &&
                        (prefs[activePortKey] ?: "") == original.port &&
                        (prefs[activeHttpsKey] ?: false) == original.useHttps
                    )
            if (isActive) {
                prefs[activeAddressKey] = toStore.address
                prefs[activeHttpsKey] = toStore.useHttps
                prefs[activePortKey] = toStore.port
                prefs[activePasswordKey] = toStore.password
                prefs[activeNameKey] = toStore.name
                prefs[activeMeshIpKey] = toStore.meshIp
                prefs[activeNpubKey] = toStore.npub
            }
        }
    }

    /**
     * Add a server, or update the entry for the same node — npub first,
     * address/port/scheme as the LAN-only fallback (ServerEntry.sameNode) —
     * used by QR pairing so re-scanning a node never duplicates it, even after
     * the LAN renumbered and every address changed (npub-first contract in
     * docs/companion-pairing-qr.md). A blank incoming password/name keeps the
     * stored value (a real node's QR never carries the password). Returns the
     * merged entry.
     */
    suspend fun upsertServer(server: ServerEntry): ServerEntry {
        var merged = server
        context.dataStore.edit { prefs ->
            val current = prefs[savedServersKey] ?: emptySet()
            val existing = current.mapNotNull { ServerEntry.deserialize(it) }
                .firstOrNull { it.sameNode(server) }
            if (existing != null) {
                merged = server.copy(
                    password = server.password.ifBlank { existing.password },
                    name = server.name.ifBlank { existing.name },
                    meshIp = server.meshIp.ifBlank { existing.meshIp },
                    npub = server.npub.ifBlank { existing.npub },
                )
            }
            val filtered = current.filterNot { raw ->
                ServerEntry.deserialize(raw)?.sameNode(merged) == true
            }.toSet()
            prefs[savedServersKey] = filtered + merged.serialize()
        }
        return merged
    }

    suspend fun removeSavedServer(server: ServerEntry) {
        context.dataStore.edit { prefs ->
            val current = prefs[savedServersKey] ?: emptySet()
            // Match by node identity (npub, else address/port/scheme) rather
            // than the exact serialized string, so a rename — or a legacy
            // short-format entry — still removes the right record.
            prefs[savedServersKey] = current.filterNot { raw ->
                ServerEntry.deserialize(raw)?.sameNode(server) == true
            }.toSet()
        }
    }

    suspend fun markIntroSeen() {
        context.dataStore.edit { prefs ->
            prefs[introSeenKey] = true
        }
    }

    suspend fun markGestureHintSeen() {
        context.dataStore.edit { prefs ->
            prefs[gestureHintSeenKey] = true
        }
    }
}
