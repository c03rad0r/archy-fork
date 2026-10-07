package com.archipelago.app.ui.screens

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.SystemClock
import android.webkit.WebView
import androidx.core.content.ContextCompat
import androidx.webkit.JavaScriptReplyProxy
import androidx.webkit.WebMessageCompat
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import org.json.JSONObject
import com.archipelago.app.ui.screens.CompanionAudioDiagnostics.Event

/** Metadata/control only: the authorized WebView owns the stream and queue. */
internal data class CompanionAudioState(
    val session: String, val sequence: Long, val title: String,
    val playing: Boolean, val position: Double, val duration: Double,
    val previous: Boolean, val next: Boolean, val shuffle: Boolean,
    val shuffled: Boolean, val artwork: String,
) {
    companion object {
        fun parse(value: JSONObject): CompanionAudioState {
            require(value.optInt("version") == 1 && value.getString("action") == "state")
            val session = value.getString("session")
            require(session.matches(Regex("[0-9a-f-]{36}")))
            val sequence = value.getLong("sequence")
            require(sequence >= 0 && sequence <= 9007199254740991L)
            val position = value.getDouble("position"); val duration = value.getDouble("duration")
            require(position.isFinite() && duration.isFinite() && duration in 0.0..604800.0 && position in 0.0..duration)
            val title = value.getString("title"); require(title.length <= 512)
            val artwork = value.optString("artwork", "")
            require(artwork.length <= 90000 && (artwork.isEmpty() || artwork.startsWith("data:image/jpeg;base64,")))
            return CompanionAudioState(session, sequence, title, value.getBoolean("playing"), position, duration,
                value.optBoolean("previous"), value.optBoolean("next"), value.optBoolean("shuffle"),
                value.optBoolean("shuffled"), artwork)
        }
    }
}

internal object CompanionAudioBridge {
    private val bindings = java.util.WeakHashMap<WebView, Set<String>>()
    private val retired = linkedSetOf<String>()
    private var view: WebView? = null
    private var origin: String? = null
    private var reply: ((String) -> Unit)? = null
    var state: CompanionAudioState? = null
        private set
    var updatedAt: Long = 0
        private set
    fun retains(web: WebView?) = web != null && view === web && state != null
    fun attach(web: WebView, urls: List<String>) {
        if (!WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) { CompanionAudioDiagnostics.record(Event.BRIDGE_UNSUPPORTED); return }
        val origins = urls.mapNotNull(::cloudVideoOrigin).toSet()
        if (origins.isEmpty()) { CompanionAudioDiagnostics.record(Event.NO_APPROVED_ORIGIN); return }
        val existing = bindings[web]
        if (existing != null) {
            if (existing != origins) { CompanionAudioDiagnostics.record(Event.ORIGIN_CHANGED); bindings[web] = emptySet(); release(web) }
            return
        }
        bindings[web] = origins
        WebViewCompat.addWebMessageListener(web, "ArchipelagoAudio", origins,
            object : WebViewCompat.WebMessageListener {
                override fun onPostMessage(web: WebView, message: WebMessageCompat, source: Uri, main: Boolean, proxy: JavaScriptReplyProxy) {
                    if (bindings[web] != origins) return
                    val raw = runCatching { message.data }.getOrNull() ?: return
                    receive(web, raw, source.toString(), main, origins) { proxy.postMessage(it) }
                }
            })
        CompanionAudioDiagnostics.record(Event.BRIDGE_ATTACHED)
    }
    internal fun receive(web: WebView, raw: String, source: String, main: Boolean,
                         origins: Set<String>, proxy: (String) -> Unit) {
        CompanionAudioDiagnostics.record(Event.MESSAGE_RECEIVED)
        if (!cloudVideoSenderAllowed(web.url, source, origins, main)) { CompanionAudioDiagnostics.record(Event.SENDER_REJECTED); return }
        if (raw.length > 96000) { CompanionAudioDiagnostics.record(Event.MESSAGE_TOO_LARGE); return }
        val data = runCatching { JSONObject(raw) }.getOrNull() ?: run { CompanionAudioDiagnostics.record(Event.INVALID_JSON); return }
        val session = data.optString("session")
        if (data.optString("action") == "release") {
            if (web === view && session == state?.session) { CompanionAudioDiagnostics.record(Event.SESSION_RELEASED); terminate() }
            return
        }
        val incoming = runCatching { CompanionAudioState.parse(data) }.getOrNull() ?: run { CompanionAudioDiagnostics.record(Event.INVALID_STATE); return }
        if (session in retired) { CompanionAudioDiagnostics.record(Event.RETIRED_SESSION); return }
        val old = state
        if (old != null && old.session == session) {
            if (view !== web || incoming.sequence <= old.sequence) { CompanionAudioDiagnostics.record(Event.STALE_STATE); return }
        } else {
            if (!incoming.playing) { CompanionAudioDiagnostics.record(Event.IDLE_STATE); return } // Do not start a service for idle metadata.
            if (old != null) { command("pause"); retire(old.session) }
        }
        CompanionAudioDiagnostics.record(Event.STATE_ACCEPTED)
        view = web; origin = cloudVideoOrigin(source); reply = proxy
        state = if (old != null && old.session == session && !data.has("artwork")) incoming.copy(artwork = old.artwork) else incoming; updatedAt = SystemClock.elapsedRealtime()
        runCatching {
            val service = CompanionAudioService.instance
            if (service != null) service.refresh()
            else {
                CompanionAudioDiagnostics.record(Event.SERVICE_REQUESTED)
                ContextCompat.startForegroundService(web.context.applicationContext,
                    Intent(web.context.applicationContext, CompanionAudioService::class.java))
            }
        }.onFailure {
            CompanionAudioDiagnostics.record(when {
                it is SecurityException -> Event.SERVICE_PERMISSION_DENIED
                it.javaClass.simpleName == "ForegroundServiceStartNotAllowedException" -> Event.SERVICE_BACKGROUND_START_DENIED
                else -> Event.SERVICE_REQUEST_FAILED
            })
            event("error", "Background playback could not start. Reopen the companion and press Play.")
            command("pause"); terminate()
        }
    }
    private fun retire(session: String) {
        retired.add(session)
        while (retired.size > 64) retired.remove(retired.first())
    }
    private fun event(type: String, value: String? = null, position: Double? = null) {
        val current = state ?: return
        if (cloudVideoOrigin(view?.url) != origin) { terminate(); return }
        val message = JSONObject().put("version", 1).put("session", current.session).put("type", type)
        if (value != null) message.put(if (type == "error") "error" else "command", value)
        if (position != null) message.put("position", position)
        runCatching { reply?.invoke(message.toString()) }
    }
    fun command(name: String, position: Double? = null) = event("command", name, position)
    fun release(web: WebView) { if (view === web) { CompanionAudioDiagnostics.record(Event.PAGE_RELEASED); command("stop"); terminate() } }
    fun terminate() {
        state?.session?.let(::retire)
        state = null; view = null; origin = null; reply = null
        CompanionAudioService.instance?.finishPlayback()
        releaseDetachedKioskWebView()
    }
    fun stop() { command("stop"); terminate() }
}
