package com.archipelago.app.ui.screens

import android.app.PendingIntent
import android.app.PictureInPictureParams
import android.app.RemoteAction
import android.content.BroadcastReceiver
import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.graphics.Rect
import android.graphics.drawable.Icon
import android.net.Uri
import android.util.Rational
import android.webkit.WebView
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.ContextCompat
import androidx.webkit.JavaScriptReplyProxy
import androidx.webkit.WebMessageCompat
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import com.archipelago.app.MainActivity
import org.json.JSONObject
import java.net.URI
import java.util.UUID

internal fun cloudVideoOrigin(value: String?): String? = runCatching {
    val uri = URI(value ?: return null)
    val scheme = uri.scheme?.lowercase() ?: return null
    if (scheme !in setOf("http", "https") || uri.userInfo != null) return null
    val host = uri.host?.lowercase() ?: return null
    val port = uri.port.takeUnless { it == -1 || it == if (scheme == "https") 443 else 80 }
    "$scheme://$host${port?.let { ":$it" } ?: ""}"
}.getOrNull()

internal fun cloudVideoSenderAllowed(currentUrl: String?, source: String, allowed: Set<String>, mainFrame: Boolean): Boolean {
    val origin = cloudVideoOrigin(source)
    return mainFrame && origin != null && origin in allowed && cloudVideoOrigin(currentUrl) == origin
}

/** Only the dashboard's origin-restricted main-frame channel can arm Cloud PiP.
 * No URL, cookies, bearer token or second media player enters native storage. */
internal class CloudVideoPip(private val activity: MainActivity?, private val fullscreen: WebViewFullscreen) {
    private class Binding(val origins: Set<String>, var owner: java.lang.ref.WeakReference<CloudVideoPip>)
    companion object {
        private val bindings = java.util.WeakHashMap<WebView, Binding>()
    }
    private var webView: WebView? = null
    private var session: String? = null
    private var reply: JavaScriptReplyProxy? = null
    private var playing = false
    private var ratio = Rational(16, 9)
    private var entered = false
    private var registered = false
    private val action = "com.archipelago.app.CLOUD_VIDEO_PIP.${UUID.randomUUID()}"
    private val receiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            if (!entered || intent?.action != action || intent.getStringExtra("session") != session) return
            event("command", if (playing) "pause" else "play")
        }
    }
    private fun supported() = activity?.packageManager?.hasSystemFeature(PackageManager.FEATURE_PICTURE_IN_PICTURE) == true
    private fun event(state: String, command: String? = null) {
        val message = JSONObject().put("type", "event").put("session", session).put("state", state)
        if (command != null) message.put("command", command)
        runCatching { reply?.postMessage(message.toString()) }
    }
    private fun params(): PictureInPictureParams {
        val owner = requireNotNull(activity)
        val intent = Intent(action).setPackage(owner.packageName).putExtra("session", session)
        val pending = PendingIntent.getBroadcast(owner, 0, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val control = RemoteAction(Icon.createWithResource(owner, if (playing) android.R.drawable.ic_media_pause else android.R.drawable.ic_media_play),
            if (playing) "Pause" else "Play", if (playing) "Pause video" else "Play video", pending)
        val bounds = Rect()
        val builder = PictureInPictureParams.Builder().setAspectRatio(ratio).setActions(listOf(control))
        if (fullscreen.bounds(bounds)) builder.setSourceRectHint(bounds)
        return builder.build()
    }
    fun attach(view: WebView, allowedUrls: List<String>) {
        if (!WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) return
        val origins = allowedUrls.mapNotNull(::cloudVideoOrigin).toSet()
        if (origins.isEmpty()) return
        webView = view
        val existing = bindings[view]
        if (existing != null) {
            // Rebind the retained document; removing/re-adding a listener would
            // require a reload and strand the page's existing JS bridge.
            if (existing.origins != origins) {
                existing.owner.clear(); webView = null
                return // The page receives a bounded unavailable response; reconnect reloads policy.
            }
            existing.owner = java.lang.ref.WeakReference(this)
            return
        }
        val binding = Binding(origins, java.lang.ref.WeakReference(this))
        bindings[view] = binding
        WebViewCompat.addWebMessageListener(view, "ArchipelagoCloudVideo", origins,
            object : WebViewCompat.WebMessageListener {
                override fun onPostMessage(web: WebView, message: WebMessageCompat, sourceOrigin: Uri, isMainFrame: Boolean, proxy: JavaScriptReplyProxy) {
                    binding.owner.get()?.receive(web, message, sourceOrigin, isMainFrame, proxy, binding.origins)
                }
            })
    }
    private fun receive(web: WebView, message: WebMessageCompat, sourceOrigin: Uri, isMainFrame: Boolean, proxy: JavaScriptReplyProxy, origins: Set<String>) {
        if (web !== webView || !cloudVideoSenderAllowed(web.url, sourceOrigin.toString(), origins, isMainFrame)) return
        val raw = runCatching { message.data }.getOrNull() ?: return
        if (raw.length > 2048) return
        val request = runCatching { JSONObject(raw) }.getOrNull() ?: return
        val id = request.optString("id")
        if (!id.matches(Regex("[0-9a-f-]{36}"))) return
        val response = JSONObject().put("id", id)
        runCatching {
            when (request.optString("action")) {
                "capabilities" -> response.put("supported", supported()).put("version", 1)
                "arm" -> {
                    check(supported()) { "Picture-in-picture is unavailable on this device." }
                    check(!entered) { "A video is already in picture-in-picture." }
                    val width = request.optInt("width", 0); val height = request.optInt("height", 0)
                    check(width in 1..16384 && height in 1..16384) { "Video dimensions are not ready." }
                    ratio = Rational(((width.toDouble() / height).coerceIn(1.0 / 2.39, 2.39) * 10000).toInt(), 10000)
                    session = id; reply = proxy; playing = request.optBoolean("playing", false)
                    response.put("session", id)
                }
                "enter" -> {
                    check(request.optString("session") == session && session != null && fullscreen.isActive) { "Open the selected Cloud video fullscreen first." }
                    val owner = requireNotNull(activity)
                    if (!registered) {
                        ContextCompat.registerReceiver(owner, receiver, IntentFilter(action), ContextCompat.RECEIVER_NOT_EXPORTED)
                        registered = true
                    }
                    check(owner.enterPictureInPictureMode(params())) { "Picture-in-picture is disabled or unavailable. Check this app's system setting." }
                    entered = true
                    response.put("active", true)
                }
                "state" -> {
                    check(request.optString("session") == session && session != null) { "Video session changed." }
                    playing = request.optBoolean("playing", false)
                    if (entered) activity?.setPictureInPictureParams(params())
                }
                "release" -> {
                    check(request.optString("session") == session && session != null) { "Video session changed." }
                    reset()
                }
                else -> error("Unsupported Cloud video action.")
            }
            Unit
        }.onFailure { response.put("error", it.message ?: "Picture-in-picture is unavailable.") }
        proxy.postMessage(response.toString())
    }
    fun modeChanged(active: Boolean) {
        if (active) { entered = true; event("entered") }
        else if (entered) {
            entered = false
            event("restored")
            // Restoring the viewer is not a stop request. Retire the native
            // session before Chromium's hide callback can recursively reset it.
            session = null; reply = null
            fullscreen.hide()
        }
    }
    fun stopped() { if (entered) { event("command", "pause"); event("closed") } }
    fun reset() {
        event("command", "pause")
        event("closed")
        session = null; reply = null
        fullscreen.hide()
    }
    fun dispose() {
        reset()
        if (registered) runCatching { activity?.unregisterReceiver(receiver) }
        registered = false
        webView?.let { view -> bindings[view]?.takeIf { it.owner.get() === this }?.owner?.clear() }
        webView = null
    }
}
private fun Context.pipActivity(): MainActivity? = when(this) {
    is MainActivity -> this
    is ContextWrapper -> baseContext.takeIf { it !== this }?.pipActivity()
    else -> null
}
@Composable
internal fun rememberCloudVideoPip(fullscreen: WebViewFullscreen): CloudVideoPip {
    val owner = LocalContext.current.pipActivity()
    val pip = remember(owner, fullscreen) { CloudVideoPip(owner, fullscreen) }
    DisposableEffect(pip) {
        owner?.cloudVideoPip = pip
        onDispose { if (owner != null && owner.cloudVideoPip === pip) owner.cloudVideoPip = null; pip.dispose() }
    }
    return pip
}
