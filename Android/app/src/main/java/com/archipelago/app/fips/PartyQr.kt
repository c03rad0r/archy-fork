package com.archipelago.app.fips

import android.net.Uri
import java.net.Inet4Address
import java.net.NetworkInterface

/**
 * Phone↔phone mesh pairing ("Mesh Party") QR contract:
 *
 *   archipelago://party?v=1&npub=<npub>&ula=<fd..>&name=<name>[&ip=<v4>&port=<udp>]
 *
 * npub + ula alone are enough to chat *through* the mesh (anchors route by
 * node address, no underlay info needed). ip/port are present only while the
 * showing phone has its inbound UDP listener up (party mode) — the scanner
 * then also gets a direct-dial link that works on a shared LAN/hotspot with
 * no internet at all. Same versioning stance as the node pairing QR
 * (docs/companion-pairing-qr.md): unknown params tolerated under v=1.
 */
data class PartyPeer(
    val npub: String,
    val ula: String,
    val name: String,
    /** Direct-dial underlay endpoint; empty when the peer wasn't listening. */
    val ip: String = "",
    val port: Int = 0,
)

object PartyQr {
    const val SCHEME_HOST = "party"
    private const val SUPPORTED_MAJOR = 1

    /** UDP port a party-mode phone listens on (matches the node's mesh port). */
    const val PARTY_UDP_PORT = 2121

    /** Application-layer chat/beam port, bound only on the mesh ULA. */
    const val FLARE_PORT = 5680

    fun build(npub: String, ula: String, name: String, ip: String?, port: Int): String {
        val b = Uri.Builder()
            .scheme("archipelago")
            .authority(SCHEME_HOST)
            .appendQueryParameter("v", "1")
            .appendQueryParameter("npub", npub)
            .appendQueryParameter("ula", ula)
            .appendQueryParameter("name", name)
        if (!ip.isNullOrBlank() && port > 0) {
            b.appendQueryParameter("ip", ip)
            b.appendQueryParameter("port", port.toString())
        }
        return b.build().toString()
    }

    /** Null when [raw] is not a valid party QR (foreign codes just keep scanning). */
    fun parse(raw: String): PartyPeer? {
        val uri = try {
            Uri.parse(raw.trim())
        } catch (_: Exception) {
            return null
        }
        if (!"archipelago".equals(uri.scheme, ignoreCase = true)) return null
        if (uri.isOpaque || !SCHEME_HOST.equals(uri.host, ignoreCase = true)) return null
        val major = uri.getQueryParameter("v")?.takeWhile { it.isDigit() }?.toIntOrNull() ?: return null
        if (major != SUPPORTED_MAJOR) return null

        val npub = uri.getQueryParameter("npub")?.trim().orEmpty()
        val ula = uri.getQueryParameter("ula")?.trim().orEmpty()
        if (!npub.startsWith("npub1") || !ula.startsWith("fd")) return null
        return PartyPeer(
            npub = npub,
            ula = ula,
            name = uri.getQueryParameter("name")?.trim().orEmpty().ifBlank { "Phone" },
            ip = uri.getQueryParameter("ip")?.trim().orEmpty(),
            port = uri.getQueryParameter("port")?.toIntOrNull() ?: 0,
        )
    }

    /**
     * This phone's private IPv4 on WiFi or its own hotspot, for the QR's
     * direct-dial hint. Hotspot interfaces (ap/swlan/softap) win over wlan so
     * the hotspot-host phone advertises the address its guests can reach.
     */
    fun localWifiIpv4(): String? {
        val candidates = mutableListOf<Pair<String, String>>() // ifname → addr
        try {
            for (nif in NetworkInterface.getNetworkInterfaces()) {
                if (!nif.isUp || nif.isLoopback) continue
                for (addr in nif.inetAddresses) {
                    if (addr is Inet4Address && addr.isSiteLocalAddress) {
                        candidates += nif.name to addr.hostAddress.orEmpty()
                    }
                }
            }
        } catch (_: Exception) {
            return null
        }
        val hotspot = candidates.firstOrNull {
            it.first.startsWith("ap") || it.first.startsWith("swlan") || it.first.startsWith("softap")
        }
        return (hotspot ?: candidates.firstOrNull { it.first.startsWith("wlan") } ?: candidates.firstOrNull())
            ?.second
    }
}
