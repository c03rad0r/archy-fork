package com.archipelago.app.fips

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import org.json.JSONArray
import org.json.JSONObject
import androidx.datastore.preferences.preferencesDataStore

private val Context.fipsDataStore: DataStore<Preferences> by preferencesDataStore(name = "fips_prefs")

// Archipelago-operated public anchor (vps2). Baked in so EVERY pairing yields
// both paths — direct LAN p2p to the node AND a public rendezvous for
// away-from-home — even when the scanned node is old enough that its QR
// carries no fanchors. Keep in lockstep with
// core/archipelago/src/fips/anchors.rs (ARCHY_ANCHOR_*).
internal const val ARCHY_ANCHOR_NPUB =
    "npub1dptaktwxv0mm245g2lqjykwm5ll0jpc6m3r4242ydfa9z7qe6urs3jvrak"
internal const val ARCHY_ANCHOR_ADDR = "146.59.87.168:8444"
internal const val ARCHY_ANCHOR_TRANSPORT = "tcp"

/**
 * Public FIPS network anchors (join.fips.network — the dual-transport TCP
 * pair; keep in lockstep with core/archipelago/src/fips/anchors.rs
 * fips_network_anchors()). Baked into every pairing at trailing priority so
 * a degraded/unreachable vps2 anchor can never strand the phone: the mesh
 * still joins the public tree and routes to the node through it.
 */
internal val PUBLIC_FIPS_ANCHORS = listOf(
    Triple(
        "npub10yffd020a4ag8zcy75f9pruq3rnghvvhd5hphl9s62zgp35s560qrksp9u",
        "23.182.128.74:443",
        "tcp",
    ),
    Triple(
        "npub1qmc3cvfz0yu2hx96nq3gp55zdan2qclealn7xshgr448d3nh6lks7zel98",
        "217.77.8.91:443",
        "tcp",
    ),
)

/**
 * Mesh identity + known node peers. Follows the same plaintext-DataStore
 * storage model as ServerPreferences (the server password lives there the
 * same way); the mesh secret only grants mesh membership, not node login.
 */
class FipsPreferences(private val context: Context) {

    private val secretKey = stringPreferencesKey("fips_secret")
    private val npubKey = stringPreferencesKey("fips_npub")
    private val addressKey = stringPreferencesKey("fips_address")
    /** JSON array of node peers in fips PeerConfig shape (see NodePeer). */
    private val peersKey = stringPreferencesKey("fips_node_peers")
    /** JSON array of phone party peers (PartyPeer shape, NOT PeerConfig). */
    private val partyPeersKey = stringPreferencesKey("fips_party_peers")
    /** Party mode: accept a direct inbound mesh link (UDP 2121). */
    private val partyListenKey = booleanPreferencesKey("fips_party_listen")
    /** Name shown in this phone's party QR and outgoing flares. */
    private val partyNameKey = stringPreferencesKey("fips_party_name")

    suspend fun identity(): FipsNative.Identity? {
        val prefs = context.fipsDataStore.data.first()
        val secret = prefs[secretKey] ?: return null
        return FipsNative.Identity(
            secret = secret,
            npub = prefs[npubKey] ?: "",
            address = prefs[addressKey] ?: "",
        )
    }

    suspend fun saveIdentity(identity: FipsNative.Identity) {
        context.fipsDataStore.edit { prefs ->
            prefs[secretKey] = identity.secret
            prefs[npubKey] = identity.npub
            prefs[addressKey] = identity.address
        }
    }

    suspend fun peersJson(): String {
        val prefs = context.fipsDataStore.data.first()
        return prefs[peersKey] ?: "[]"
    }

    suspend fun hasPeers(): Boolean = JSONArray(peersJson()).length() > 0

    /**
     * Union the stored node peers with a backup's peer list, matched by
     * npub — the backup's copy wins for the same npub (its addresses are what
     * the restored identity pairs against). Used by companion restore (#128)
     * after [saveIdentity] adopted the backup's mesh identity.
     */
    suspend fun mergePeersJson(incomingJson: String) {
        context.fipsDataStore.edit { prefs ->
            val current = JSONArray(prefs[peersKey] ?: "[]")
            val incoming = try {
                JSONArray(incomingJson)
            } catch (_: Exception) {
                JSONArray()
            }
            val incomingNpubs = mutableSetOf<String>()
            val merged = JSONArray()
            for (i in 0 until incoming.length()) {
                val peer = incoming.optJSONObject(i) ?: continue
                val npub = peer.optString("npub")
                if (npub.isNotBlank()) {
                    incomingNpubs.add(npub)
                    merged.put(peer)
                }
            }
            for (i in 0 until current.length()) {
                val peer = current.optJSONObject(i) ?: continue
                if (peer.optString("npub") !in incomingNpubs) merged.put(peer)
            }
            prefs[peersKey] = merged.toString()
        }
    }

    // ── Mesh Party (phone↔phone) ────────────────────────────────────────────

    suspend fun partyListen(): Boolean =
        context.fipsDataStore.data.first()[partyListenKey] ?: false

    val partyListenFlow: Flow<Boolean>
        get() = context.fipsDataStore.data.map { it[partyListenKey] ?: false }

    suspend fun setPartyListen(enabled: Boolean) {
        context.fipsDataStore.edit { it[partyListenKey] = enabled }
    }

    suspend fun partyName(): String =
        context.fipsDataStore.data.first()[partyNameKey]
            ?: android.os.Build.MODEL.orEmpty().ifBlank { "Phone" }

    suspend fun setPartyName(name: String) {
        context.fipsDataStore.edit { it[partyNameKey] = name.trim() }
    }

    val partyPeersFlow: Flow<List<PartyPeer>>
        get() = context.fipsDataStore.data.map { parsePartyPeers(it[partyPeersKey] ?: "[]") }

    suspend fun partyPeers(): List<PartyPeer> =
        parsePartyPeers(context.fipsDataStore.data.first()[partyPeersKey] ?: "[]")

    /** Matched by npub, so re-scanning updates the direct-dial address in place. */
    suspend fun upsertPartyPeer(peer: PartyPeer) {
        context.fipsDataStore.edit { prefs ->
            val kept = parsePartyPeers(prefs[partyPeersKey] ?: "[]").filter { it.npub != peer.npub }
            prefs[partyPeersKey] = toJson(kept + peer)
        }
    }

    suspend fun removePartyPeer(npub: String) {
        context.fipsDataStore.edit { prefs ->
            val kept = parsePartyPeers(prefs[partyPeersKey] ?: "[]").filter { it.npub != npub }
            prefs[partyPeersKey] = toJson(kept)
        }
    }

    /**
     * Node peers + direct-dial party peers, in the fips PeerConfig JSON the
     * Rust node deserializes. Party peers get the best priority: on a shared
     * LAN/hotspot the direct link beats every anchor path, and while off-LAN
     * the failed dial is cheap (auto-reconnect keeps retrying, which is
     * exactly what makes the link snap up the moment both phones share WiFi).
     * Party peers without an underlay address are mesh-routed and need no
     * entry here at all.
     */
    suspend fun combinedPeersJson(): String {
        val merged = JSONArray(peersJson())
        val party = partyPeers()
        for (peer in party) {
            if (peer.ip.isBlank() || peer.port <= 0) continue
            merged.put(JSONObject().apply {
                put("npub", peer.npub)
                put("alias", hostSafeAlias(peer.name.ifBlank { "party-phone" }))
                put("addresses", JSONArray().put(JSONObject().apply {
                    put("transport", "udp")
                    put("addr", "${peer.ip}:${peer.port}")
                    put("priority", 5)
                }))
            })
        }
        // A party-only phone (never paired with a node) still needs a public
        // rendezvous to reach its peers ACROSS the internet — without it, two
        // bare phones would be hotspot/LAN-only. Node pairing normally bakes
        // this anchor in; do the same when there are party peers.
        if (party.isNotEmpty() &&
            (0 until merged.length()).none {
                merged.optJSONObject(it)?.optString("npub") == ARCHY_ANCHOR_NPUB
            }
        ) {
            merged.put(JSONObject().apply {
                put("npub", ARCHY_ANCHOR_NPUB)
                put("alias", "archipelago-anchor")
                put("addresses", JSONArray().put(JSONObject().apply {
                    put("transport", ARCHY_ANCHOR_TRANSPORT)
                    put("addr", ARCHY_ANCHOR_ADDR)
                    put("priority", 40)
                }))
            })
        }
        // Backfill anchor peers for entries paired before newer QR/app
        // releases added them. `upsertNodePeer` persists these on re-scan, but
        // startup must also self-heal old DataStore state so updating the APK is
        // enough to get off-LAN redundancy.
        if (merged.length() > 0) {
            addAnchorIfMissing(merged, ARCHY_ANCHOR_NPUB, "archipelago-anchor", ARCHY_ANCHOR_ADDR, ARCHY_ANCHOR_TRANSPORT, 40)
            for ((i, anchor) in PUBLIC_FIPS_ANCHORS.withIndex()) {
                val (npub, addr, transport) = anchor
                addAnchorIfMissing(merged, npub, "fips-network-anchor-${i + 1}", addr, transport, 50 + i * 10)
            }
        }
        return merged.toString()
    }

    private fun addAnchorIfMissing(
        peers: JSONArray,
        npub: String,
        alias: String,
        addr: String,
        transport: String,
        priority: Int,
    ) {
        if ((0 until peers.length()).any { peers.optJSONObject(it)?.optString("npub") == npub }) return
        peers.put(JSONObject().apply {
            put("npub", npub)
            put("alias", alias)
            put("addresses", JSONArray().put(JSONObject().apply {
                put("transport", transport)
                put("addr", addr)
                put("priority", priority)
            }))
        })
    }

    private fun parsePartyPeers(json: String): List<PartyPeer> = try {
        val arr = JSONArray(json)
        (0 until arr.length()).mapNotNull { i ->
            val o = arr.optJSONObject(i) ?: return@mapNotNull null
            val npub = o.optString("npub")
            val ula = o.optString("ula")
            if (npub.isBlank() || ula.isBlank()) return@mapNotNull null
            PartyPeer(
                npub = npub,
                ula = ula,
                name = o.optString("name").ifBlank { "Phone" },
                ip = o.optString("ip"),
                port = o.optInt("port"),
            )
        }
    } catch (_: Exception) {
        emptyList()
    }

    private fun toJson(peers: List<PartyPeer>): String {
        val arr = JSONArray()
        for (p in peers) {
            arr.put(JSONObject().apply {
                put("npub", p.npub)
                put("ula", p.ula)
                put("name", p.name)
                put("ip", p.ip)
                put("port", p.port)
            })
        }
        return arr.toString()
    }

    /**
     * Add or update the node peer plus its rendezvous anchors (each matched
     * by npub, so re-pairing updates addresses instead of duplicating).
     * Stored directly in the fips PeerConfig JSON shape the Rust side
     * deserializes. The node's direct addresses get the best priorities;
     * anchors trail so the mesh prefers the direct path when it works.
     */
    suspend fun upsertNodePeer(info: FipsPairInfo, alias: String) {
        val incoming = mutableListOf<JSONObject>()
        incoming += JSONObject().apply {
            put("npub", info.npub)
            put("alias", hostSafeAlias(alias.ifBlank { "Archipelago" }))
            val addresses = JSONArray()
            // .fips hosts are unresolvable on Android (no system .fips DNS):
            // storing one gives the mesh a dial target that fails every
            // handshake and stalls first connect on anchor discovery.
            if (info.udpPort > 0 && !info.host.endsWith(".fips")) {
                addresses.put(JSONObject().apply {
                    put("transport", "udp")
                    put("addr", "${info.host}:${info.udpPort}")
                    put("priority", 10)
                })
            }
            if (info.tcpPort > 0 && !info.host.endsWith(".fips")) {
                addresses.put(JSONObject().apply {
                    put("transport", "tcp")
                    put("addr", "${info.host}:${info.tcpPort}")
                    put("priority", 20)
                })
            }
            put("addresses", addresses)
        }
        for (anchor in info.anchors) {
            if (anchor.npub == info.npub) continue
            if (anchor.addr.substringBeforeLast(":").endsWith(".fips")) continue
            incoming += JSONObject().apply {
                put("npub", anchor.npub)
                put("alias", "mesh-anchor")
                put("addresses", JSONArray().put(JSONObject().apply {
                    put("transport", anchor.transport)
                    put("addr", anchor.addr)
                    put("priority", 30)
                }))
            }
        }
        // Guarantee the public anchor: without it, a QR from an older node
        // leaves the phone LAN-only and pairing/connecting dies off-LAN.
        if (info.npub != ARCHY_ANCHOR_NPUB &&
            incoming.none { it.optString("npub") == ARCHY_ANCHOR_NPUB }
        ) {
            incoming += JSONObject().apply {
                put("npub", ARCHY_ANCHOR_NPUB)
                put("alias", "archipelago-anchor")
                put("addresses", JSONArray().put(JSONObject().apply {
                    put("transport", ARCHY_ANCHOR_TRANSPORT)
                    put("addr", ARCHY_ANCHOR_ADDR)
                    put("priority", 40)
                }))
            }
        }
        // And the public FIPS network anchors at trailing priority, so one
        // degraded rendezvous (vps2, 2026-07-24) can never strand the phone.
        for ((i, anchor) in PUBLIC_FIPS_ANCHORS.withIndex()) {
            val (npub, addr, transport) = anchor
            if (info.npub == npub || incoming.any { it.optString("npub") == npub }) continue
            incoming += JSONObject().apply {
                put("npub", npub)
                put("alias", "fips-network-anchor-${i + 1}")
                put("addresses", JSONArray().put(JSONObject().apply {
                    put("transport", transport)
                    put("addr", addr)
                    put("priority", 50 + i * 10)
                }))
            }
        }
        val incomingNpubs = incoming.map { it.optString("npub") }.toSet()
        context.fipsDataStore.edit { prefs ->
            val current = JSONArray(prefs[peersKey] ?: "[]")
            val merged = JSONArray()
            for (i in 0 until current.length()) {
                val existing = current.optJSONObject(i) ?: continue
                if (existing.optString("npub") !in incomingNpubs) merged.put(existing)
            }
            incoming.forEach { merged.put(it) }
            prefs[peersKey] = merged.toString()
        }
    }
}

/**
 * Peer aliases feed the fips host map as `<alias>.fips` hostnames; anything
 * that isn't a valid DNS label ("Test Node" — the space) gets rejected and
 * silently drops the peer from name resolution. Slug it instead of losing it.
 */
internal fun hostSafeAlias(alias: String): String =
    alias.lowercase()
        .replace(Regex("[^a-z0-9.-]+"), "-")
        .trim('-', '.')
        .ifBlank { "archipelago" }
