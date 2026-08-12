package com.archipelago.app.network

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import java.security.cert.X509Certificate
import java.util.concurrent.TimeUnit
import javax.net.ssl.SSLContext
import javax.net.ssl.X509TrustManager

enum class ConnectionState { DISCONNECTED, CONNECTING, CONNECTED, AUTH_FAILED, ERROR }

class InputWebSocket(
    private val scope: CoroutineScope,
) {
    private var ws: WebSocket? = null
    private var reconnectJob: Job? = null
    private var reconnectAttempt = 0
    private var serverUrl: String = ""
    private var password: String = ""
    private var sessionCookie: String? = null

    /** Player ID for arcade mode (0 = broadcast, 1 = P1, 2 = P2) */
    var playerId: Int = 0

    /**
     * Invoked when the kiosk asks us to open a URL in the phone's default
     * browser ({"t":"o","url":"…"}). "Open in external browser" apps can't be
     * usefully opened on the kiosk, so the kiosk forwards them here.
     */
    var onExternalOpen: ((String) -> Unit)? = null

    private val _state = MutableStateFlow(ConnectionState.DISCONNECTED)
    val state: StateFlow<ConnectionState> = _state

    private val trustManager = object : X509TrustManager {
        override fun checkClientTrusted(chain: Array<X509Certificate>?, authType: String?) {}
        override fun checkServerTrusted(chain: Array<X509Certificate>?, authType: String?) {}
        override fun getAcceptedIssuers(): Array<X509Certificate> = arrayOf()
    }

    private val client: OkHttpClient by lazy {
        val sc = SSLContext.getInstance("TLS")
        sc.init(null, arrayOf(trustManager), java.security.SecureRandom())

        OkHttpClient.Builder()
            .sslSocketFactory(sc.socketFactory, trustManager)
            .hostnameVerifier { _, _ -> true }
            .pingInterval(30, TimeUnit.SECONDS)
            .readTimeout(0, TimeUnit.MILLISECONDS)
            .connectTimeout(10, TimeUnit.SECONDS)
            .build()
    }

    fun connect(httpUrl: String, pwd: String = "") {
        disconnect()
        serverUrl = httpUrl
        password = pwd
        sessionCookie = null
        reconnectAttempt = 0
        scope.launch(Dispatchers.IO) { doAuth() }
    }

    private suspend fun doAuth() {
        _state.value = ConnectionState.CONNECTING

        if (password.isBlank()) {
            doConnect()
            return
        }

        try {
            val body = """{"method":"auth.login","params":{"password":"$password"}}"""
                .toRequestBody("application/json".toMediaType())
            val req = Request.Builder()
                .url("$serverUrl/rpc/v1")
                .post(body)
                .build()

            val response = withContext(Dispatchers.IO) { client.newCall(req).execute() }

            if (response.isSuccessful) {
                sessionCookie = response.headers("Set-Cookie")
                    .mapNotNull { cookie ->
                        cookie.split(";")
                            .firstOrNull()
                            ?.trim()
                            ?.takeIf { it.startsWith("session=") }
                            ?.removePrefix("session=")
                    }
                    .firstOrNull()
                response.close()

                if (sessionCookie != null) {
                    doConnect()
                } else {
                    _state.value = ConnectionState.AUTH_FAILED
                }
            } else {
                response.close()
                _state.value = ConnectionState.AUTH_FAILED
            }
        } catch (_: Exception) {
            _state.value = ConnectionState.ERROR
            scheduleReconnect()
        }
    }

    private fun doConnect() {
        val basePath = "/ws/remote-input" + if (playerId > 0) "?p=$playerId" else ""
        val wsUrl = serverUrl
            .replace("https://", "wss://")
            .replace("http://", "ws://")
            .trimEnd('/') + basePath

        val reqBuilder = Request.Builder().url(wsUrl)
        sessionCookie?.let { reqBuilder.header("Cookie", "session=$it") }

        ws = client.newWebSocket(reqBuilder.build(), object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                _state.value = ConnectionState.CONNECTED
                reconnectAttempt = 0
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                // The only inbound message we act on is an external-open request
                // forwarded from the kiosk: {"t":"o","url":"https://…"}.
                try {
                    val obj = org.json.JSONObject(text)
                    if (obj.optString("t") == "o") {
                        val url = obj.optString("url")
                        if (url.startsWith("http://") || url.startsWith("https://")) {
                            onExternalOpen?.invoke(url)
                        }
                    }
                } catch (_: Exception) {}
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                _state.value = ConnectionState.ERROR
                scheduleReconnect()
            }

            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                webSocket.close(1000, null)
                _state.value = ConnectionState.DISCONNECTED
                if (code != 1000) scheduleReconnect()
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                _state.value = ConnectionState.DISCONNECTED
            }
        })
    }

    private fun scheduleReconnect() {
        reconnectJob?.cancel()
        reconnectJob = scope.launch(Dispatchers.IO) {
            val delayMs = minOf(1000L * (1 shl minOf(reconnectAttempt, 5)), 30_000L)
            reconnectAttempt++
            delay(delayMs)
            doAuth()
        }
    }

    fun disconnect() {
        reconnectJob?.cancel()
        ws?.close(1000, "bye")
        ws = null
        _state.value = ConnectionState.DISCONNECTED
    }

    // ─── Input senders ──────────────────────────────────────────

    fun sendKey(key: String) {
        val pField = if (playerId > 0) ""","p":$playerId""" else ""
        ws?.send("""{"t":"k","k":"$key"$pField}""")
    }

    fun sendMouseMove(dx: Int, dy: Int) {
        ws?.send("""{"t":"m","x":$dx,"y":$dy}""")
    }

    fun sendClick(button: Int = 1) {
        ws?.send("""{"t":"c","b":$button}""")
    }

    fun sendScroll(dy: Int) {
        ws?.send("""{"t":"s","y":$dy}""")
    }
}
