package com.archipelago.app.data

import android.net.Uri
import com.archipelago.app.fips.AnchorPeer
import com.archipelago.app.fips.FipsPairInfo

/**
 * Result of parsing a pairing QR / deep link.
 *
 * UnsupportedVersion means the payload is structurally a pairing URI but its
 * major version is newer than this app understands — the UI should tell the
 * user to update the app rather than call the code invalid.
 */
sealed class PairResult {
    data class Success(
        val server: ServerEntry,
        /** Mesh info when the node advertises FIPS (fnpub present). */
        val fips: FipsPairInfo? = null,
    ) : PairResult()
    object UnsupportedVersion : PairResult()
    object Invalid : PairResult()
}

/**
 * Parser for the companion pairing QR / OS deep link. Contract:
 * docs/companion-pairing-qr.md (repo root).
 *
 *   archipelago://pair?v=1&url=<origin>&name=…[&tok=…][&pw=…][&fnpub=…&fip=…&fhost=…&fudp=…&ftcp=…]
 *
 * - `url` is a full origin including scheme (http for LAN/mDNS nodes, https
 *   for the public demo); trailing slashes are normalized away.
 * - `tok` is a device token minted by the node; it goes through the password
 *   field on purpose — the whole password auto-login path (WebSocket auth +
 *   WebView form injection) then works unchanged, and the backend accepts
 *   device tokens wherever it accepts the password. `pw` (demo only) wins if
 *   both are ever present.
 * - `fnpub`/`fip`/`fhost`/`fudp`/`ftcp` describe the node's FIPS mesh; their
 *   absence just means LAN-only pairing (older node, or FIPS not provisioned).
 * - Unknown extra query params are tolerated (forward compat under v=1).
 */
object ServerQrParser {
    private const val SUPPORTED_MAJOR = 1

    fun parse(raw: String): PairResult {
        val uri = try {
            Uri.parse(raw.trim())
        } catch (_: Exception) {
            return PairResult.Invalid
        }
        if (!"archipelago".equals(uri.scheme, ignoreCase = true)) return PairResult.Invalid
        if (uri.isOpaque || !"pair".equals(uri.host, ignoreCase = true)) return PairResult.Invalid

        val major = uri.getQueryParameter("v")
            ?.trim()
            ?.takeWhile { it.isDigit() }
            ?.toIntOrNull()
            ?: return PairResult.Invalid
        if (major != SUPPORTED_MAJOR) return PairResult.UnsupportedVersion

        val serverUrl = uri.getQueryParameter("url")?.trim()?.trimEnd('/')
        if (serverUrl.isNullOrBlank()) return PairResult.Invalid
        val server = Uri.parse(serverUrl)
        val scheme = server.scheme?.lowercase()
        if (scheme != "http" && scheme != "https") return PairResult.Invalid
        val host = server.host
        if (host.isNullOrBlank()) return PairResult.Invalid

        val fips = parseFips(uri)
        val credential = uri.getQueryParameter("pw")?.takeIf { it.isNotBlank() }
            ?: uri.getQueryParameter("tok")
            ?: ""

        return PairResult.Success(
            server = ServerEntry(
                address = host,
                useHttps = scheme == "https",
                port = if (server.port != -1) server.port.toString() else "",
                password = credential,
                name = uri.getQueryParameter("name") ?: "",
                meshIp = fips?.ula ?: "",
                // npub is the durable identity — saved-server upserts match on
                // it, so re-scanning after a LAN renumber updates in place.
                npub = fips?.npub ?: "",
            ),
            fips = fips,
        )
    }

    private fun parseFips(uri: Uri): FipsPairInfo? {
        val npub = uri.getQueryParameter("fnpub")?.trim()
        var host = uri.getQueryParameter("fhost")?.trim()
        if (npub.isNullOrBlank() || host.isNullOrBlank()) return null
        // A .fips fhost is a dead dial hint: Android's system DNS can't
        // resolve .fips, so every handshake send fails and first connect
        // falls back to slow anchor discovery. If the QR's `url` host is a
        // real address, dial that instead (QRs minted while the node UI was
        // browsed over the mesh carry npub….fips here).
        if (host.endsWith(".fips")) {
            val urlHost = uri.getQueryParameter("url")
                ?.let { runCatching { Uri.parse(it).host }.getOrNull() }
            if (!urlHost.isNullOrBlank() && !urlHost.endsWith(".fips")) host = urlHost
        }
        return FipsPairInfo(
            npub = npub,
            ula = uri.getQueryParameter("fip")?.trim().orEmpty(),
            host = host,
            udpPort = uri.getQueryParameter("fudp")?.toIntOrNull() ?: 2121,
            tcpPort = uri.getQueryParameter("ftcp")?.toIntOrNull() ?: 8443,
            anchors = parseAnchors(uri.getQueryParameter("fanchors")),
        )
    }

    /** `fanchors` = comma-joined `npub@host:port/transport`; bad items skipped. */
    private fun parseAnchors(raw: String?): List<AnchorPeer> {
        if (raw.isNullOrBlank()) return emptyList()
        return raw.split(",").mapNotNull { item ->
            val at = item.indexOf('@')
            val slash = item.lastIndexOf('/')
            if (at <= 0 || slash <= at) return@mapNotNull null
            val npub = item.substring(0, at).trim()
            val addr = item.substring(at + 1, slash).trim()
            val transport = item.substring(slash + 1).trim().lowercase()
            if (npub.isBlank() || !addr.contains(":")) return@mapNotNull null
            if (transport != "udp" && transport != "tcp") return@mapNotNull null
            AnchorPeer(npub = npub, addr = addr, transport = transport)
        }
    }
}
