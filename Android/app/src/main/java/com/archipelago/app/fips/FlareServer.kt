package com.archipelago.app.fips

import android.content.Context
import android.util.Log
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject
import java.io.BufferedInputStream
import java.io.File
import java.io.InputStream
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.UUID
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/** One chat/photo message in a party conversation, keyed by the peer's npub. */
data class FlareMessage(
    val id: String,
    val peerNpub: String,
    val fromMe: Boolean,
    val name: String,
    val text: String = "",
    val photoPath: String = "",
    val ts: Long,
    val status: Status = Status.RECEIVED,
) {
    enum class Status { SENDING, SENT, FAILED, RECEIVED }
}

/** In-memory conversation store (demo scope — nothing persists across restarts). */
object FlareStore {
    private val _messages = MutableStateFlow<List<FlareMessage>>(emptyList())
    val messages: StateFlow<List<FlareMessage>> = _messages

    fun add(message: FlareMessage) {
        _messages.value = _messages.value + message
    }

    fun setStatus(id: String, status: FlareMessage.Status) {
        _messages.value = _messages.value.map { if (it.id == id) it.copy(status = status) else it }
    }
}

/**
 * Minimal HTTP listener bound ONLY on this phone's mesh ULA — plain HTTP is
 * fine there because FIPS is the encryption + peer-identity layer (same
 * stance as the node's ULA-only peer listener). This is what makes the phone
 * a *server* on the mesh: another phone (or `curl -6` from any mesh node)
 * reaches it by npub-derived address with no port forwarding, DNS, or CA.
 *
 * FIPS authenticates the node, not the request (project doctrine), so inputs
 * are still validated at this boundary: size caps, JSON shape, no
 * client-controlled paths.
 */
object FlareServer {
    private const val TAG = "FlareServer"
    private const val MAX_PHOTO_BYTES = 8 * 1024 * 1024
    private const val MAX_TEXT_CHARS = 4_000
    private const val MAX_HEADER_BYTES = 16 * 1024

    private var socket: ServerSocket? = null
    private var pool: ExecutorService? = null
    @Volatile private var identityName = "Phone"
    @Volatile private var identityNpub = ""
    @Volatile private var photoDir: File? = null

    /** Invoked when a peer announces itself (POST /hello) — pairing used to
     *  be one-way: only the SCANNING phone learned the other side, so the
     *  scanned phone had no peer, no chat entry, no way in. The VPN service
     *  wires this to upsert the peer + restart the mesh config. */
    @Volatile var onHello: ((PartyPeer) -> Unit)? = null

    @Synchronized
    fun start(context: Context, ula: String, myNpub: String, myName: String) {
        stop()
        identityNpub = myNpub
        identityName = myName
        photoDir = File(context.cacheDir, "flare").apply { mkdirs() }
        val pool = Executors.newCachedThreadPool().also { this.pool = it }
        pool.execute {
            try {
                val server = ServerSocket().apply {
                    reuseAddress = true
                    bind(InetSocketAddress(InetAddress.getByName(ula), PartyQr.FLARE_PORT))
                }
                socket = server
                Log.i(TAG, "flare listening on [$ula]:${PartyQr.FLARE_PORT}")
                while (!server.isClosed) {
                    val client = try {
                        server.accept()
                    } catch (_: Exception) {
                        break
                    }
                    pool.execute { handle(client) }
                }
            } catch (e: Exception) {
                Log.e(TAG, "flare server died: $e")
            }
        }
    }

    @Synchronized
    fun stop() {
        try {
            socket?.close()
        } catch (_: Exception) {
        }
        socket = null
        pool?.shutdownNow()
        pool = null
    }

    private fun handle(client: Socket) {
        client.use { sock ->
            sock.soTimeout = 30_000
            try {
                val input = BufferedInputStream(sock.getInputStream())
                val requestLine = readLine(input) ?: return
                val parts = requestLine.trim().split(" ")
                if (parts.size < 2) return respond(sock, 400, json("bad_request"))
                val (method, path) = parts[0] to parts[1]

                var contentLength = 0
                var from = ""
                var fromName = ""
                var headerBytes = requestLine.length
                while (true) {
                    val line = readLine(input) ?: return
                    if (line.isEmpty()) break
                    headerBytes += line.length
                    if (headerBytes > MAX_HEADER_BYTES) return respond(sock, 431, json("headers_too_large"))
                    val idx = line.indexOf(':')
                    if (idx <= 0) continue
                    val key = line.substring(0, idx).trim().lowercase()
                    val value = line.substring(idx + 1).trim()
                    when (key) {
                        "content-length" -> contentLength = value.toIntOrNull() ?: 0
                        "x-from" -> from = value.take(80)
                        "x-name" -> fromName = value.take(80)
                    }
                }

                when {
                    method == "GET" && (path == "/" || path.startsWith("/?")) ->
                        respondHtml(sock, profilePage())
                    method == "POST" && path == "/hello" -> {
                        if (contentLength !in 1..MAX_HEADER_BYTES) return respond(sock, 413, json("too_large"))
                        val body = readExactly(input, contentLength) ?: return
                        receiveHello(String(body, Charsets.UTF_8))
                        respond(sock, 200, """{"ok":true}""")
                    }
                    method == "POST" && path == "/flare" -> {
                        if (contentLength !in 1..MAX_HEADER_BYTES) return respond(sock, 413, json("too_large"))
                        val body = readExactly(input, contentLength) ?: return
                        receiveFlare(String(body, Charsets.UTF_8))
                        respond(sock, 200, """{"ok":true}""")
                    }
                    method == "POST" && path == "/photo" -> {
                        if (contentLength !in 1..MAX_PHOTO_BYTES) return respond(sock, 413, json("too_large"))
                        if (!from.startsWith("npub1")) return respond(sock, 400, json("bad_request"))
                        val body = readExactly(input, contentLength) ?: return
                        receivePhoto(from, fromName, body)
                        respond(sock, 200, """{"ok":true}""")
                    }
                    else -> respond(sock, 404, json("not_found"))
                }
            } catch (e: Exception) {
                Log.w(TAG, "request failed: $e")
            }
        }
    }

    /** A peer that scanned OUR QR announces itself — mutual pairing. */
    private fun receiveHello(body: String) {
        val o = try {
            JSONObject(body)
        } catch (_: Exception) {
            return
        }
        val from = o.optString("from")
        val ula = o.optString("ula")
        if (!from.startsWith("npub1") || !ula.startsWith("fd")) return
        if (from == identityNpub) return
        val peer = PartyPeer(
            npub = from.take(80),
            ula = ula.take(64),
            name = o.optString("name").take(24).ifBlank { "Phone" },
            ip = o.optString("ip").take(40),
            port = o.optInt("port", 0),
        )
        onHello?.invoke(peer)
        // Seed the conversation so the chat has a visible entry on this side.
        FlareStore.add(
            FlareMessage(
                id = UUID.randomUUID().toString(),
                peerNpub = peer.npub,
                fromMe = false,
                name = peer.name,
                text = "👋 ${peer.name} joined the party",
                ts = System.currentTimeMillis(),
            )
        )
    }

    private fun receiveFlare(body: String) {
        val o = try {
            JSONObject(body)
        } catch (_: Exception) {
            return
        }
        val from = o.optString("from")
        if (!from.startsWith("npub1")) return
        val text = o.optString("text").take(MAX_TEXT_CHARS)
        if (text.isBlank()) return
        FlareStore.add(
            FlareMessage(
                id = UUID.randomUUID().toString(),
                peerNpub = from,
                fromMe = false,
                name = o.optString("name").take(80).ifBlank { "Phone" },
                text = text,
                ts = System.currentTimeMillis(),
            )
        )
    }

    private fun receivePhoto(from: String, fromName: String, bytes: ByteArray) {
        // Server-generated filename — the sender never controls the path.
        val dir = photoDir ?: return
        val file = File(dir, "${UUID.randomUUID()}.jpg")
        file.writeBytes(bytes)
        FlareStore.add(
            FlareMessage(
                id = UUID.randomUUID().toString(),
                peerNpub = from,
                fromMe = false,
                name = fromName.ifBlank { "Phone" },
                photoPath = file.absolutePath,
                ts = System.currentTimeMillis(),
            )
        )
    }

    private fun profilePage(): String {
        val npub = identityNpub
        val name = identityName
        return """
            <!doctype html><html><head><meta charset="utf-8">
            <meta name="viewport" content="width=device-width,initial-scale=1">
            <title>$name — on the mesh</title>
            <style>
              body{background:#0a0a0a;color:#eee;font-family:monospace;
                   display:flex;min-height:100vh;align-items:center;justify-content:center;margin:0}
              .card{border:1px solid rgba(255,255,255,.12);border-radius:20px;padding:32px;
                    max-width:560px;background:rgba(255,255,255,.04)}
              h1{color:#f7931a;margin:0 0 8px;font-size:22px}
              .npub{word-break:break-all;color:#888;font-size:12px;margin:12px 0}
              p{line-height:1.5}
            </style></head><body><div class="card">
            <h1>⚡ $name</h1>
            <div class="npub">$npub</div>
            <p>This page is being served <b>by a phone</b>, addressed by its
            cryptographic identity over the FIPS mesh.</p>
            <p>No port forwarding. No DNS. No certificate authority. No cloud.
            The key <i>is</i> the address — and the transport underneath can be
            5G, WiFi, or a hotspot with no internet at all.</p>
            </div></body></html>
        """.trimIndent()
    }

    // ── tiny HTTP plumbing ──────────────────────────────────────────────────

    /** Read one CRLF-terminated header line as ISO-8859-1; null on EOF. */
    private fun readLine(input: InputStream): String? {
        val sb = StringBuilder()
        while (true) {
            val b = input.read()
            if (b == -1) return if (sb.isEmpty()) null else sb.toString()
            if (b == '\n'.code) return sb.toString().trimEnd('\r')
            sb.append(b.toChar())
            if (sb.length > MAX_HEADER_BYTES) return null
        }
    }

    private fun readExactly(input: InputStream, length: Int): ByteArray? {
        val buf = ByteArray(length)
        var off = 0
        while (off < length) {
            val n = input.read(buf, off, length - off)
            if (n == -1) return null
            off += n
        }
        return buf
    }

    private fun json(code: String) = """{"error":{"code":"$code","message":"request rejected"}}"""

    private fun respond(sock: Socket, status: Int, body: String) =
        writeResponse(sock, status, "application/json", body.toByteArray(Charsets.UTF_8))

    private fun respondHtml(sock: Socket, body: String) =
        writeResponse(sock, 200, "text/html; charset=utf-8", body.toByteArray(Charsets.UTF_8))

    private fun writeResponse(sock: Socket, status: Int, contentType: String, body: ByteArray) {
        val reason = when (status) {
            200 -> "OK"; 400 -> "Bad Request"; 404 -> "Not Found"
            413 -> "Payload Too Large"; 431 -> "Headers Too Large"
            else -> "Error"
        }
        val head = "HTTP/1.1 $status $reason\r\n" +
            "Content-Type: $contentType\r\n" +
            "Content-Length: ${body.size}\r\n" +
            "Connection: close\r\n\r\n"
        sock.getOutputStream().apply {
            write(head.toByteArray(Charsets.ISO_8859_1))
            write(body)
            flush()
        }
    }
}

/** Outbound flares: plain HTTP to the peer's ULA — FIPS encrypts underneath. */
object FlareClient {
    // Connect timeout must outlive cold mesh-session establishment (~15s via
    // the public tree); the attempt itself drives
    // session setup, same trick as the VPN service's session warmer.
    private val http = OkHttpClient.Builder()
        .connectTimeout(25, TimeUnit.SECONDS)
        .readTimeout(15, TimeUnit.SECONDS)
        .writeTimeout(60, TimeUnit.SECONDS)
        .build()

    private fun base(peer: PartyPeer) = "http://[${peer.ula}]:${PartyQr.FLARE_PORT}"

    /** Announce myself to a freshly scanned peer so pairing becomes MUTUAL —
     *  their phone gets me as a peer + a chat entry without scanning back.
     *  Blocking — call from Dispatchers.IO. */
    fun sendHello(
        peer: PartyPeer,
        myNpub: String,
        myName: String,
        myUla: String,
        myIp: String?,
        myPort: Int,
    ): Boolean = try {
        val body = JSONObject()
            .put("from", myNpub)
            .put("name", myName)
            .put("ula", myUla)
            .put("ip", myIp ?: "")
            .put("port", if (myIp != null) myPort else 0)
            .toString()
            .toRequestBody("application/json".toMediaType())
        http.newCall(
            Request.Builder().url("${base(peer)}/hello").post(body).build()
        ).execute().use { it.isSuccessful }
    } catch (_: Exception) {
        false
    }

    /** Blocking — call from Dispatchers.IO. */
    fun sendText(peer: PartyPeer, myNpub: String, myName: String, text: String): Boolean = try {
        val body = JSONObject()
            .put("from", myNpub)
            .put("name", myName)
            .put("text", text)
            .put("ts", System.currentTimeMillis())
            .toString()
            .toRequestBody("application/json".toMediaType())
        http.newCall(
            Request.Builder().url("${base(peer)}/flare").post(body).build()
        ).execute().use { it.isSuccessful }
    } catch (_: Exception) {
        false
    }

    /** Blocking — call from Dispatchers.IO. */
    fun sendPhoto(peer: PartyPeer, myNpub: String, myName: String, jpeg: ByteArray): Boolean = try {
        http.newCall(
            Request.Builder()
                .url("${base(peer)}/photo")
                .header("X-From", myNpub)
                .header("X-Name", myName)
                .post(jpeg.toRequestBody("image/jpeg".toMediaType()))
                .build()
        ).execute().use { it.isSuccessful }
    } catch (_: Exception) {
        false
    }
}
