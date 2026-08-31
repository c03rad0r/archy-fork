package com.archipelago.app.nostr

import android.content.Context
import com.archipelago.app.NativeCore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.json.JSONArray
import org.json.JSONObject
import java.security.SecureRandom
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

/**
 * NIP-46 remote-signer session (#139) — the phone side, wire-faithful to
 * rust-nostr's reference bunker (`signer/nostr-connect/src/signer.rs`),
 * which the node's login flow will interoperate with:
 *
 * 1. Client (the node's login page) shows a `nostrconnect://` QR.
 * 2. We scan it, connect to its relay, subscribe to kind-24133 events
 *    p-tagged to our signer key, and send a `connect` request carrying the
 *    secret (the client validates it and answers "ack").
 * 3. Requests arrive as NIP-44-encrypted kind-24133 events; we respond over
 *    the same channel. `sign_event` is the one method that never runs
 *    without a human tapping Approve on this phone.
 *
 * The session lives while the app is around (the login handshake takes
 * seconds); there is no background service in v1 and no remembered-session
 * auto-reconnect (research doc flow C — deferred deliberately).
 */
object BunkerManager {

    sealed class SignerState {
        /** Native core unavailable (e.g. x86 emulator) — signing impossible. */
        object Unavailable : SignerState()
        /** Key exists, no session. */
        object Idle : SignerState()
        /** No signer key generated/imported yet. */
        object NoKey : SignerState()
        data class Connecting(val relay: String) : SignerState()
        /** Connect request sent; waiting for the client to ack. */
        data class AwaitingClient(val relay: String, val clientName: String) : SignerState()
        /** Handshake complete — this is the state where requests are answered. */
        data class Ready(val relay: String, val clientName: String) : SignerState()
        data class Failed(val reason: String) : SignerState()
    }

    /** One signature request awaiting a human decision. */
    data class PendingRequest(
        val id: String,
        val method: String,
        val clientPubkey: String,
        val clientName: String,
        val kind: Long?,
        val content: String?,
        /** Formatted tag lines for the approval card. */
        val tags: List<String>,
        val createdAt: Long?,
        /** The full unsigned event JSON handed to the native signer on approve. */
        val unsignedEventJson: String,
    )

    private val _state = MutableStateFlow<SignerState>(SignerState.Idle)
    val state: StateFlow<SignerState> = _state.asStateFlow()

    private val _pending = MutableStateFlow<PendingRequest?>(null)
    val pending: StateFlow<PendingRequest?> = _pending.asStateFlow()

    private val client = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS)
        .pingInterval(25, TimeUnit.SECONDS) // relay keepalive
        .build()

    private data class Session(
        val socket: WebSocket,
        val relay: String,
        /** The client's pubkey (hex) from the nostrconnect URI. */
        val clientPubkey: String,
        val clientName: String,
        /** The pairing secret — echoed back during handshake, then kept for
         *  validating an incoming `connect` from the same client. */
        val secret: String,
        /** Our connect request id, to match the client's ack response. */
        val connectRequestId: String,
        /** Our signer secret (hex). */
        val signerSecretHex: String,
        /** Our signer pubkey (hex). */
        val signerPubkeyHex: String,
        /** Event ids already handled (relays may redeliver). */
        val seen: MutableSet<String> = java.util.concurrent.ConcurrentHashMap.newKeySet(),
    )

    private val session = AtomicReference<Session?>(null)

    /** Refresh Idle/NoKey state (suspend; call from a coroutine — DataStore reads hit disk). */
    suspend fun refreshState(context: Context) {
        if (!NativeCore.available) {
            _state.value = SignerState.Unavailable
            return
        }
        if (session.get() != null) return
        val prefs = NostrSignerPreferences(context.applicationContext)
        _state.value =
            if (prefs.secret() == null) SignerState.NoKey else SignerState.Idle
    }

    /**
     * Pair from a scanned or deep-linked `nostrconnect://…` URI. Returns a
     * user-presentable error on failure, or null on success (state moves to
     * Connecting → AwaitingClient).
     */
    suspend fun pair(context: Context, uri: String): String? {
        if (!NativeCore.available) return "Signing is unavailable on this device"
        val appContext = context.applicationContext
        return withContext(Dispatchers.IO) {
            val parsed = JSONObject(NativeCore.nostrParseConnectUri(uri.trim()))
            if (parsed.has("error")) return@withContext parsed.getString("error")

            val prefs = NostrSignerPreferences(appContext)
            val secret = prefs.secret()
                ?: return@withContext "No signer key yet — generate or import one first"
            val info = JSONObject(NativeCore.nostrSecretFromAny(secret))
            if (info.has("error")) return@withContext info.getString("error")

            val clientPubkey = parsed.getString("clientPubkey")
            val relays = mutableListOf<String>()
            parsed.optJSONArray("relays")?.let { arr -> for (i in 0 until arr.length()) relays.add(arr.optString(i)) }
            val clientName = parsed.optString("name").ifBlank { "client" }
            val pairSecret = parsed.getString("secret")

            if (relays.isEmpty()) return@withContext "The pairing code carries no relay to reach the client on"

            teardown()

            var lastError = "no relay could be reached"
            for (relay in relays) {
                _state.value = SignerState.Connecting(relay)
                val opened = openSession(
                    relay, clientPubkey, clientName, pairSecret, secret, info,
                )
                if (opened != null) {
                    session.set(opened)
                    prefs.savePairing(
                        NostrSignerPreferences.Pairing(clientPubkey, relay, clientName)
                    )
                    _state.value = SignerState.AwaitingClient(relay, clientName)
                    return@withContext null
                }
                lastError = "relay $relay did not answer"
            }
            _state.value = SignerState.Failed(lastError)
            lastError
        }
    }

    /** Re-establish the last saved pairing without a fresh QR. */
    suspend fun resume(context: Context): String? {
        if (!NativeCore.available) return "Signing is unavailable on this device"
        val appContext = context.applicationContext
        return withContext(Dispatchers.IO) {
            val prefs = NostrSignerPreferences(appContext)
            val pairing = prefs.lastPairing()
                ?: return@withContext "Nothing to resume — no saved pairing"
            val secret = prefs.secret()
                ?: return@withContext "No signer key"
            val info = JSONObject(NativeCore.nostrSecretFromAny(secret))
            if (info.has("error")) return@withContext info.getString("error")
            teardown()
            _state.value = SignerState.Connecting(pairing.relay)
            val opened = openSession(
                pairing.relay, pairing.clientPubkey, pairing.name,
                secret = "", signerSecretHex = secret, info = info,
            )
            if (opened == null) {
                _state.value = SignerState.Failed("relay ${pairing.relay} did not answer")
                return@withContext "Could not reach ${pairing.relay}"
            }
            session.set(opened)
            _state.value = SignerState.AwaitingClient(pairing.relay, pairing.name)
            null
        }
    }

    fun unpair() {
        teardown()
        _state.value = SignerState.Idle
    }

    private fun teardown() {
        session.getAndSet(null)?.socket?.close(1000, "unpaired")
        _pending.value = null
    }

    private fun randomId(): String {
        val bytes = ByteArray(8)
        SecureRandom().nextBytes(bytes)
        return bytes.joinToString("") { "%02x".format(it) }
    }

    private fun openSession(
        relay: String,
        clientPubkey: String,
        clientName: String,
        secret: String,
        signerSecretHex: String,
        info: JSONObject,
    ): Session? {
        val signerPubkeyHex = info.getString("pubkey")
        val connectRequestId = randomId()
        // The listener needs the Session, the Session needs the WebSocket:
        // bind through a holder set right after newWebSocket returns (OkHttp
        // invokes onOpen on its own dispatcher after the network round-trip,
        // i.e. always after the bind below).
        val holder = AtomicReference<Session?>()

        val request = Request.Builder().url(relay).build()
        val socket = client.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                val s = holder.get() ?: return
                // Subscribe to requests addressed to us (p-tag filter), from
                // now — no history replay of stale login attempts.
                webSocket.send(
                    """["REQ","${s.connectRequestId}sub",{"kinds":[24133],"#p":["${s.signerPubkeyHex}"],"since":${epochSecs() - 120}}]"""
                )
                // Handshake: the signer sends `connect` carrying the secret
                // (rust-nostr's NostrConnectRemoteSigner.send_connect_ack —
                // the exact frame the node's client waits for).
                val content = JSONObject().apply {
                    put("id", s.connectRequestId)
                    put("method", "connect")
                    put("params", JSONArray().put(s.signerPubkeyHex).put(s.secret))
                }.toString()
                if (!sendEncrypted(s, content)) {
                    _state.value = SignerState.Failed("Could not encrypt the connect message")
                }
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                val s = session.get() ?: return
                handleRelayMessage(s, text)
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                if (session.get()?.socket === webSocket) {
                    _state.value = SignerState.Failed(t.message ?: "relay connection failed")
                    session.getAndSet(null)
                }
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                if (session.get()?.socket === webSocket) {
                    _state.value = SignerState.Idle
                    session.getAndSet(null)
                }
            }
        })

        val s = Session(
            socket = socket,
            relay = relay,
            clientPubkey = clientPubkey,
            clientName = clientName,
            secret = secret,
            connectRequestId = connectRequestId,
            signerSecretHex = signerSecretHex,
            signerPubkeyHex = signerPubkeyHex,
        )
        holder.set(s)
        return s
    }

    private fun epochSecs(): Long = System.currentTimeMillis() / 1000

    /** Encrypt a JSON-RPC frame to the peer and publish it as kind 24133. */
    private fun sendEncrypted(s: Session, json: String): Boolean {
        val enc = NativeCore.nostrNip44Encrypt(s.signerSecretHex, s.clientPubkey, json)
        if (NativeCore.isErr(enc)) return false
        val payload = JSONObject(enc).getString("result")
        val event = JSONObject().apply {
            put("kind", 24133)
            put("content", payload)
            put("tags", JSONArray().put(JSONArray().put("p").put(s.clientPubkey)))
            put("created_at", epochSecs())
        }.toString()
        val signed = NativeCore.nostrSignEvent(s.signerSecretHex, event)
        if (NativeCore.isErr(signed)) return false
        return s.socket.send("""["EVENT",$signed]""")
    }

    private fun handleRelayMessage(s: Session, text: String) {
        val arr = try {
            JSONArray(text)
        } catch (_: Exception) {
            return
        }
        if (arr.length() == 0) return
        when (arr.optString(0)) {
            "EVENT" -> {
                val event = arr.optJSONObject(2) ?: return
                if (event.optLong("kind") != 24133L) return
                val id = event.optString("id")
                if (id.isNotEmpty() && !s.seen.add(id)) return
                val author = event.optString("pubkey")
                if (author != s.clientPubkey) return // not our client
                handleClientEvent(s, author, event.optString("content"))
            }
            // OK / CLOSED / NOTICE: nothing actionable for the bunker in v1.
        }
    }

    private fun handleClientEvent(s: Session, author: String, content: String) {
        // NIP-44 is the mandated transport; NIP-04 stays as receive fallback
        // for clients that still speak the deprecated scheme.
        val plain = run {
            val nip44 = NativeCore.nostrNip44Decrypt(s.signerSecretHex, author, content)
            if (!NativeCore.isErr(nip44)) JSONObject(nip44).getString("result") else {
                val nip04 = NativeCore.nostrNip04Decrypt(s.signerSecretHex, author, content)
                if (!NativeCore.isErr(nip04)) JSONObject(nip04).getString("result") else return
            }
        }

        val msg = try {
            JSONObject(plain)
        } catch (_: Exception) {
            return
        }

        val id = msg.optString("id")
        val method = msg.optString("method", "")
        if (method.isNotEmpty()) {
            when (method) {
                "connect" -> {
                    val params = msg.optJSONArray("params") ?: return
                    // Param 0 must be OUR pubkey (client is connecting to us,
                    // not some other bunker through this session).
                    val target = params.optString(0)
                    val givenSecret = params.optString(1)
                    val authorized = target == s.signerPubkeyHex &&
                        (s.secret.isBlank() || givenSecret == s.secret || givenSecret.isBlank())
                    if (authorized) {
                        respond(s, id, result = "ack")
                        _state.value = SignerState.Ready(s.relay, s.clientName)
                    } else {
                        respond(s, id, error = "unauthorized")
                    }
                }
                "get_public_key" -> respond(s, id, result = s.signerPubkeyHex)
                "describe" -> respond(s, id, result = "connect get_public_key sign_event ping")
                "ping" -> respond(s, id, result = "pong")
                "sign_event" -> {
                    val params = msg.optJSONArray("params") ?: return
                    val eventJson = params.optString(0)
                    val ev = try {
                        JSONObject(eventJson)
                    } catch (_: Exception) {
                        respond(s, id, error = "malformed event")
                        return
                    }
                    // Never overwrite a pending request silently — a second
                    // tap on the node would otherwise cancel the visible one.
                    if (_pending.value == null) {
                        _pending.value = PendingRequest(
                            id = id,
                            method = method,
                            clientPubkey = author,
                            clientName = s.clientName,
                            kind = if (ev.has("kind") && !ev.isNull("kind")) ev.optLong("kind") else null,
                            content = if (ev.has("content") && !ev.isNull("content")) ev.optString("content") else null,
                            tags = formatTags(ev.optJSONArray("tags")),
                            createdAt = if (ev.has("created_at") && !ev.isNull("created_at")) ev.optLong("created_at") else null,
                            unsignedEventJson = eventJson,
                        )
                    } else {
                        respond(s, id, error = "busy")
                    }
                }
                else -> respond(s, id, error = "not authorized")
            }
        } else if (msg.has("result") || msg.has("error")) {
            // A response to OUR connect request (the client's ack).
            if (id == s.connectRequestId) {
                if (msg.has("error")) {
                    _state.value = SignerState.Failed("Client rejected the connection: ${msg.optString("error")}")
                } else if (msg.optString("result") == "ack") {
                    _state.value = SignerState.Ready(s.relay, s.clientName)
                }
            }
        }
    }

    /** Approve the pending request: sign and send the result. */
    suspend fun approve(): Boolean {
        val s = session.get() ?: return false
        val req = _pending.value ?: return false
        val ok = withContext(Dispatchers.IO) {
            val signed = NativeCore.nostrSignEvent(s.signerSecretHex, req.unsignedEventJson)
            if (NativeCore.isErr(signed)) {
                respond(s, req.id, error = "signing failed")
                false
            } else {
                // Result is the signed event, JSON-stringified per the spec.
                respond(s, req.id, result = signed)
            }
        }
        _pending.value = null
        return ok
    }

    /** Deny the pending request with an explicit error. */
    fun deny() {
        val s = session.get() ?: return
        val req = _pending.value ?: return
        respond(s, req.id, error = "denied")
        _pending.value = null
    }

    /** Send a JSON-RPC response frame to the client. True when the WS send worked. */
    private fun respond(s: Session, id: String, result: String? = null, error: String? = null): Boolean {
        val frame = JSONObject().apply {
            put("id", id)
            if (error != null) put("error", error)
            if (result != null) put("result", result)
        }.toString()
        return sendEncrypted(s, frame)
    }

    private fun formatTags(tags: JSONArray?): List<String> {
        tags ?: return emptyList()
        val out = mutableListOf<String>()
        for (i in 0 until tags.length()) {
            val tag = tags.optJSONArray(i) ?: continue
            val parts = mutableListOf<String>()
            for (j in 0 until tag.length()) parts.add(tag.optString(j))
            out.add(parts.joinToString(" "))
        }
        return out
    }
}
