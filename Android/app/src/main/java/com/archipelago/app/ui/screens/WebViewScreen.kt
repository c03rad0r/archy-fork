package com.archipelago.app.ui.screens

import android.Manifest
import android.annotation.SuppressLint
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.view.ViewGroup
import android.webkit.CookieManager
import android.webkit.PermissionRequest
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.border
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsBottomHeight
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.windowInsetsTopHeight
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.ArrowForward
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.CloudOff
import androidx.compose.material.icons.filled.OpenInBrowser
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import android.webkit.ValueCallback
import com.archipelago.app.R
import com.archipelago.app.data.ServerPreferences
import com.archipelago.app.fips.FipsManager
import com.archipelago.app.ui.components.GestureHintOverlay
import com.archipelago.app.ui.components.MeshLoadingScreen
import com.archipelago.app.ui.components.NESMenu
import com.archipelago.app.ui.components.QrScannerOverlay
import com.archipelago.app.ui.components.SlidingLoader
import com.archipelago.app.ui.components.WalletQrScannerModal
import com.archipelago.app.ui.components.prewarmQrScanner
import com.archipelago.app.ui.theme.BitcoinOrange
import com.archipelago.app.ui.theme.ErrorRed
import com.archipelago.app.ui.theme.SurfaceBlack
import com.archipelago.app.ui.theme.TextMuted
import com.archipelago.app.ui.theme.TextPrimary
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject

/** Open a URL in the phone's default browser (genuinely external links). */
private fun openExternalUrl(context: android.content.Context, url: String) {
    try {
        val intent = android.content.Intent(
            android.content.Intent.ACTION_VIEW,
            android.net.Uri.parse(url),
        ).apply {
            // Required when launching from a non-Activity/binder thread
            // (the JS bridge below can run off the UI thread).
            addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        context.startActivity(intent)
    } catch (_: Exception) {}
}

/** Drop the retained kiosk WebView (see [KioskWebView]).
 *
 *  Retaining it is what keeps the dashboard alive across remote ⇄ dashboard —
 *  but it's a process-scoped static, and the FIPS mesh service keeps this
 *  process alive after the task is swiped out of recents. So a swipe-away
 *  followed by a relaunch used to reattach the OLD page instead of starting
 *  clean, i.e. "closing the app" stopped restarting it. MainActivity calls
 *  this when the task is genuinely finishing. */
fun releaseKioskWebView() = KioskWebView.drop()

/** Restart the app in place: throw away the retained page and relaunch the
 *  task from scratch. The mesh/VPN service is deliberately left running — this
 *  is the "give me a clean app" button (hub menu), not a process kill. */
fun restartCompanionApp(context: android.content.Context) {
    // Drop BEFORE relaunching: with singleTask, the new instance can compose
    // before the old one is destroyed and would otherwise reattach the very
    // WebView we're trying to discard.
    KioskWebView.drop()
    val intent = context.packageManager
        .getLaunchIntentForPackage(context.packageName)
        ?.apply {
            addFlags(
                android.content.Intent.FLAG_ACTIVITY_NEW_TASK or
                    android.content.Intent.FLAG_ACTIVITY_CLEAR_TASK,
            )
        } ?: return
    context.startActivity(intent)
}

/** Copy/paste for pages running inside the app.
 *
 *  The node UI is served over plain HTTP on the LAN/mesh, so it is NOT a secure
 *  context and the browser withholds `navigator.clipboard` entirely. The web
 *  UI's own fallback can only fake a WRITE (hidden textarea + execCommand) and
 *  its readText() resolves to an empty string — which is why every "Paste"
 *  button in the wallet was dead inside the app: it fired, got "", and silently
 *  did nothing. This bridge hands both directions to the Android clipboard;
 *  [CLIPBOARD_SHIM_JS] then points `navigator.clipboard` at it, so pages get
 *  working copy AND paste with no change on the web side.
 *
 *  JS calls `ArchipelagoClipboard.copy(text)` / `.paste()`; a paste comes back
 *  through `window.__archyClipboardResult(text)` (same shape as the QR bridge).
 */
private fun WebView.addClipboardBridge() {
    val appContext = context.applicationContext
    val view = this
    fun clipboard() =
        appContext.getSystemService(android.content.Context.CLIPBOARD_SERVICE)
            as? android.content.ClipboardManager

    addJavascriptInterface(
        object {
            @android.webkit.JavascriptInterface
            fun copy(text: String) {
                // ClipboardManager is main-thread-only; bridge calls land on
                // the JS thread.
                view.post {
                    runCatching {
                        clipboard()?.setPrimaryClip(
                            android.content.ClipData.newPlainText("Archipelago", text),
                        )
                    }
                }
            }

            @android.webkit.JavascriptInterface
            fun paste() {
                view.post {
                    val text = runCatching {
                        val clip = clipboard()?.primaryClip
                        if (clip == null || clip.itemCount == 0) {
                            ""
                        } else {
                            clip.getItemAt(0).coerceToText(appContext)?.toString() ?: ""
                        }
                    }.getOrDefault("")
                    view.evaluateJavascript(
                        "window.__archyClipboardResult && " +
                            "window.__archyClipboardResult(${JSONObject.quote(text)})",
                        null,
                    )
                }
            }
        },
        "ArchipelagoClipboard",
    )
}

/** Point `navigator.clipboard` at the native bridge. Deliberately tolerant:
 *  the web UI defines its own `navigator.clipboard` stand-in at boot with
 *  Object.defineProperty (non-configurable), so redefining the property can
 *  throw — patching the two methods on whatever object is there works for the
 *  stand-in, for a real Clipboard instance, and for our own placeholder. */
private const val CLIPBOARD_SHIM_JS = """
(function() {
    var bridge = window.ArchipelagoClipboard;
    if (!bridge || window.__archyClipboardPatched) return;
    window.__archyClipboardPatched = true;

    var waiting = [];
    window.__archyClipboardResult = function(text) {
        var queued = waiting;
        waiting = [];
        for (var i = 0; i < queued.length; i++) queued[i](text || '');
    };

    function readText() {
        return new Promise(function(resolve) {
            var settled = false;
            function finish(t) { if (!settled) { settled = true; resolve(t) } }
            waiting.push(finish);
            // The bridge always answers; the timeout is only so a page can
            // never hang on a clipboard read.
            setTimeout(function() { finish('') }, 1500);
            try { bridge.paste() } catch (e) { finish('') }
        });
    }

    function writeText(text) {
        try { bridge.copy(String(text)); return Promise.resolve() }
        catch (e) { return Promise.reject(e) }
    }

    try {
        if (!navigator.clipboard) {
            Object.defineProperty(navigator, 'clipboard', { value: {}, configurable: true });
        }
    } catch (e) { /* something already owns it — patch its methods below */ }
    try {
        navigator.clipboard.readText = readText;
        navigator.clipboard.writeText = writeText;
    } catch (e) { /* frozen: the page keeps its own (write-only) fallback */ }
})();
"""

private fun injectClipboardShim(view: WebView) {
    view.evaluateJavascript(CLIPBOARD_SHIM_JS, null)
}

/** True when [url] points at the same host as the connected Archipelago node
 *  (ignoring port). Such URLs are node apps — e.g. one that can't be iframed —
 *  and should stay inside the app rather than bouncing out to the browser. */
private fun isSameHost(url: String, base: String): Boolean {
    return try {
        val a = android.net.Uri.parse(url).host ?: return false
        val b = android.net.Uri.parse(base).host ?: return false
        a.equals(b, ignoreCase = true)
    } catch (_: Exception) {
        false
    }
}

/** Kiosk WebView retained across navigation (remote ⇄ dashboard) so leaving
 *  the kiosk and coming back reattaches the LIVE page — no reload, no
 *  re-login, no reconnect. Dropped on retry/disconnect/server change. */
/** What the in-app overlay opens: the URL plus (when the web UI provides
 *  them) the app's catalog icon and display name for the branded loader. */
data class InAppLaunch(val url: String, val icon: String? = null, val name: String? = null)

private object KioskWebView {
    var instance: WebView? = null
    var url: String? = null

    // Live-composition delegates for the JS bridges (see the factory) — the
    // registered interface objects call through these, so reattaching the
    // retained view re-points them instead of leaving stale closures.
    var onRouteOutbound: (String) -> Unit = {}
    var onOpenInApp: (InAppLaunch) -> Unit = {}
    var onQrOpen: () -> Unit = {}
    var onQrStatus: (String, Boolean) -> Unit = { _, _ -> }
    var onQrClose: () -> Unit = {}

    fun drop() {
        instance?.let {
            (it.parent as? ViewGroup)?.removeView(it)
            it.destroy()
        }
        instance = null
        url = null
    }
}

/** Inject the safe-area CSS vars from the CURRENT window insets. Android
 *  WebView doesn't populate env(safe-area-inset-*); worse, on a cold start
 *  onPageFinished can run before the view is attached — rootWindowInsets is
 *  null then, and injecting 0px collapsed the UI's top/bottom margins (and
 *  put the tab bar inside the gesture zone, killing its taps). Called from
 *  onPageFinished, from the window-insets listener (fires when real insets
 *  arrive), and on reattach. */
private fun injectSafeAreaVars(view: WebView) {
    val insets = view.rootWindowInsets ?: return // listener re-fires when real
    val density = view.resources.displayMetrics.density
    val sat = (insets.getInsets(android.view.WindowInsets.Type.statusBars()).top / density).toInt()
    val sab = (insets.getInsets(android.view.WindowInsets.Type.navigationBars()).bottom / density).toInt()
    // The insets listener fires on every pass (every IME show/hide); skip the
    // JS round-trip — and the Vue event it dispatches — when nothing changed.
    val stamp = "sa:$sat,$sab"
    if (view.tag == stamp) return
    view.tag = stamp
    view.evaluateJavascript(
        """
        (function() {
            var style = document.getElementById('archipelago-android-insets');
            if (!style) {
                style = document.createElement('style');
                style.id = 'archipelago-android-insets';
                document.head.appendChild(style);
            }
            style.textContent = ':root { --safe-area-top: ${sat}px; --safe-area-bottom: ${sab}px; }';
            // Vue components sample the var into reactive state; tell them it
            // changed (an authenticated session can mount before we run).
            window.dispatchEvent(new CustomEvent('archy-insets', { detail: { top: ${sat}, bottom: ${sab} } }));
        })();
        """.trimIndent(),
        null,
    )
}

/** Status-bar treatment for in-app pages, the contract being: the page's
 *  BACKGROUND (colour or imagery) extends up into the status-bar area, while
 *  page CONTENT always starts below it. The WebView draws edge-to-edge; this
 *  injection makes any page honour that contract:
 *
 *  - `body{padding-top}` moves normal-flow content down; body background
 *    (colour + background-image) keeps painting across the padded strip.
 *  - The strip must show the page's EFFECTIVE background, not body's declared
 *    one — LND declares a white body under a dark full-height root, which
 *    painted a white bar. We sample the rendered background at the top of the
 *    viewport (elementFromPoint, walking up to the first opaque colour) and
 *    repaint body with it, only when body's own background is transparent or
 *    disagrees with what's actually rendered.
 *  - fixed/sticky bars anchored at the top (IndeeHub's floating header) don't
 *    move with body padding — each is pushed down by the inset, once.
 *  - full-height fixed overlays (modals) must NOT move down (that exposed the
 *    page behind them in a strip) — they get padding-top instead, so their own
 *    background covers the bar.
 *  - a throttled MutationObserver re-applies the fixed/sticky treatment to
 *    elements the SPA mounts after load.
 *  Idempotent; runs on start (early, so content never flashes under the bar)
 *  and finish (after the app rewrites head; layout exists for sampling). */
private fun injectTopInset(view: WebView) {
    val insets = view.rootWindowInsets ?: return
    val density = view.resources.displayMetrics.density
    val sat = (insets.getInsets(android.view.WindowInsets.Type.statusBars()).top / density).toInt()
    if (sat <= 0) return
    view.evaluateJavascript(
        """
        (function() {
            var SAT = $sat;
            var CLEAR = 'rgba(0, 0, 0, 0)';
            function styleEl() {
                var s = document.getElementById('archy-top-inset');
                if (!s) {
                    s = document.createElement('style');
                    s.id = 'archy-top-inset';
                    (document.head || document.documentElement).appendChild(s);
                }
                return s;
            }
            // Effective rendered background at the top of the viewport: from
            // the topmost element at a few probe points, walk up to the first
            // ancestor that actually paints a colour.
            function effectiveTopBg() {
                if (!document.body || !document.elementFromPoint) return '';
                var w = window.innerWidth;
                var probes = [[w >> 1, SAT + 2], [8, SAT + 2], [w - 8, SAT + 2]];
                for (var i = 0; i < probes.length; i++) {
                    var el = document.elementFromPoint(probes[i][0], probes[i][1]);
                    while (el && el !== document.documentElement) {
                        var c = getComputedStyle(el).backgroundColor;
                        if (c && c !== CLEAR && c !== 'transparent') return c;
                        el = el.parentElement;
                    }
                }
                return '';
            }
            function apply() {
                var bodyBg = document.body
                    ? getComputedStyle(document.body).backgroundColor : '';
                var eff = effectiveTopBg();
                // Repaint body only when its declared background is not what
                // the page actually renders at the top (or paints nothing).
                var fix = eff && eff !== bodyBg
                    ? 'background-color:' + eff + ' !important;' : '';
                styleEl().textContent =
                    'body{padding-top:' + SAT + 'px !important;' +
                    'box-sizing:border-box !important;' + fix + '}';
            }
            function push(el) {
                if (el.dataset.archyInset) return;
                var cs = getComputedStyle(el);
                if (cs.position !== 'fixed' && cs.position !== 'sticky') return;
                var top = parseFloat(cs.top); // 'auto' -> NaN skips bottom bars
                if (isNaN(top) || top >= SAT) return;
                // offsetHeight ignores enter-animation transforms.
                if (el.offsetHeight >= window.innerHeight - SAT - 1) {
                    var pt = parseFloat(cs.paddingTop) || 0;
                    el.style.setProperty('padding-top', (pt + SAT) + 'px', 'important');
                    el.style.setProperty('box-sizing', 'border-box', 'important');
                } else {
                    el.style.setProperty('top', (top + SAT) + 'px', 'important');
                }
                el.dataset.archyInset = '1';
            }
            function sweep() {
                if (!document.body) return;
                // Fixed/sticky bars live shallow (portals mount on body); the
                // depth cap keeps the computed-style pass off big lists.
                var els = document.body.querySelectorAll(
                    'body > *, body > * > *, body > * > * > *, body > * > * > * > *');
                for (var i = 0; i < els.length; i++) push(els[i]);
            }
            apply();
            sweep();
            if (!window.__archyInsetObserver) {
                var queued = false, last = 0, idleSweeps = 0;
                var obs = new MutationObserver(function() {
                    if (queued) return;
                    queued = true;
                    var wait = Math.max(0, 250 - (Date.now() - last));
                    setTimeout(function() {
                        queued = false;
                        last = Date.now();
                        var before = document.querySelectorAll('[data-archy-inset]').length;
                        apply();
                        sweep();
                        var after = document.querySelectorAll('[data-archy-inset]').length;
                        // Page has settled — stop paying the computed-style
                        // tax on every SPA mutation. onPageStarted/Finished
                        // re-run the injection, which reinstalls us.
                        idleSweeps = (after === before) ? idleSweeps + 1 : 0;
                        if (idleSweeps >= 12) {
                            obs.disconnect();
                            window.__archyInsetObserver = null;
                        }
                    }, wait);
                });
                window.__archyInsetObserver = obs;
                obs.observe(document.documentElement,
                    { childList: true, subtree: true });
            }
        })();
        """.trimIndent(),
        null,
    )
}

/** True when a TCP listener answers at [base]'s host:port within [timeoutMs]. */
private fun tcpAnswers(base: String, timeoutMs: Int): Boolean = try {
    val u = android.net.Uri.parse(base)
    val port = if (u.port != -1) u.port else if (u.scheme == "https") 443 else 80
    java.net.Socket().use {
        it.connect(java.net.InetSocketAddress(u.host, port), timeoutMs)
        true
    }
} catch (_: Exception) {
    false
}

/** Last origin that actually answered, per LAN|mesh pair — so a relaunch
 *  starts loading in milliseconds instead of re-running discovery probes.
 *  Persisted: the in-memory map alone only survived the process, which made
 *  exactly the launch that mattered (cold start next morning) the slow one. */
private object StartUrlCache {
    private const val STORE = "start_url_cache"
    private val lastGood = java.util.concurrent.ConcurrentHashMap<String, String>()

    fun get(context: android.content.Context, key: String): String? =
        lastGood[key] ?: context.getSharedPreferences(STORE, 0).getString(key, null)
            ?.also { lastGood[key] = it }

    fun put(context: android.content.Context, key: String, origin: String) {
        lastGood[key] = origin
        context.getSharedPreferences(STORE, 0).edit().putString(key, origin).apply()
    }

    fun invalidate(context: android.content.Context, key: String) {
        lastGood.remove(key)
        context.getSharedPreferences(STORE, 0).edit().remove(key).apply()
    }

    fun key(lanUrl: String, meshUrl: String?) = "$lanUrl|${meshUrl ?: ""}"
}

/** Fastest answering origin. THREE probes race in parallel — the persisted
 *  last-good origin (short timeout: it usually answers in ~10-50 ms on LAN or
 *  a warm mesh session), the LAN URL, and the mesh ULA (patient — a cold
 *  session may still be establishing). Racing the cached probe instead of
 *  checking it first means a stale cache entry costs nothing: the other
 *  racers are already running. Serial probing was the "lightning sometimes,
 *  garbage other times" launch variance — 2.5 s of dead LAN wait before the
 *  mesh probe even started.
 *  If nothing answers, fall back to the mesh URL when we have one — off-LAN
 *  the LAN IP is unreachable, and loading it just produced a confusing
 *  "can't reach 192.168.x.x" error page (user-reported 2026-07-27). */
private suspend fun pickStartUrl(
    context: android.content.Context,
    lanUrl: String,
    meshUrl: String?,
): String =
    withContext(Dispatchers.IO) {
        val key = StartUrlCache.key(lanUrl, meshUrl)
        val cached = StartUrlCache.get(context, key)
        val winner = kotlinx.coroutines.coroutineScope {
            val racers = buildList {
                cached?.let { c -> add(async { if (tcpAnswers(c, 1200)) c else null }) }
                add(async { if (tcpAnswers(lanUrl, 2500)) lanUrl else null })
                meshUrl?.let { m -> add(async { if (tcpAnswers(m, 8_000)) m else null }) }
            }
            var pending = racers.toMutableList()
            var result: String? = null
            while (result == null && pending.isNotEmpty()) {
                val (value, done) = kotlinx.coroutines.selects.select<Pair<String?, kotlinx.coroutines.Deferred<String?>>> {
                    pending.forEach { d -> d.onAwait { it to d } }
                }
                pending.remove(done)
                result = value
            }
            racers.forEach { it.cancel() }
            result
        }
        winner?.also { StartUrlCache.put(context, key, it) } ?: meshUrl ?: lanUrl
    }

/** Apply the WebView settings shared by the kiosk view and the in-app browser.
 *  These are tuned for SPA performance and parity with the mobile browser;
 *  none of them alter how a page renders visually. */
@SuppressLint("SetJavaScriptEnabled")
private fun WebView.applyArchipelagoSettings() {
    // Pre-rasterize just outside the viewport so flinging the kiosk/app doesn't
    // show blank checkerboarding — the single biggest scroll-smoothness win and
    // a major part of the "feels slower than the browser" gap. (API 23+)
    settings.setOffscreenPreRaster(true)

    settings.apply {
        javaScriptEnabled = true
        domStorageEnabled = true
        databaseEnabled = true
        mediaPlaybackRequiresUserGesture = false
        mixedContentMode = WebSettings.MIXED_CONTENT_COMPATIBILITY_MODE
        useWideViewPort = true
        loadWithOverviewMode = true
        setSupportZoom(false)
        builtInZoomControls = false
        cacheMode = WebSettings.LOAD_DEFAULT
        allowContentAccess = true
        allowFileAccess = false
    }

    // chrome://inspect profiling on debuggable builds only — lets us measure the
    // real in-page bottleneck rather than guess. No effect on release builds.
    val debuggable = 0 != (context.applicationInfo.flags and
        android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE)
    if (debuggable) WebView.setWebContentsDebuggingEnabled(true)
}

@SuppressLint("SetJavaScriptEnabled", "ClickableViewAccessibility")
@Composable
fun WebViewScreen(
    serverUrl: String,
    onDisconnect: () -> Unit,
    onRemoteInput: () -> Unit = {},
    // Like onRemoteInput but landing on the keyboard (the hub menu's Keyboard card).
    onRemoteKeyboard: () -> Unit = {},
    // Opens the phone-to-phone Mesh Party screen; null hides its hub card.
    onMeshParty: (() -> Unit)? = null,
    // Stored password for this server (from QR pairing or manual entry). When
    // non-blank, the login page is auto-filled and submitted — the one-step
    // demo flow from docs/companion-pairing-qr.md.
    serverPassword: String = "",
    // Node's FIPS mesh URL (http://[fd…]). When the primary address fails on
    // a main-frame load — typically the phone left the LAN — retry there
    // before surfacing the error page: the mesh tunnel works from anywhere.
    meshFallbackUrl: String? = null,
) {
    var isLoading by remember { mutableStateOf(true) }
    // First kiosk load (often over the FIPS mesh) gets the full branded
    // loader; later navigations keep just the slim top progress bar.
    var firstLoadDone by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        snapshotFlow { isLoading }.first { !it }
        firstLoadDone = true
    }
    var loadProgress by remember { mutableIntStateOf(0) }
    var triedMeshFallback by remember { mutableStateOf(false) }
    var hasError by remember { mutableStateOf(false) }
    var webView by remember { mutableStateOf<WebView?>(null) }

    // Race LAN vs mesh BEFORE the WebView exists: Chromium pointed at an
    // unreachable LAN IP burns a minute+ in connect retries before
    // onReceivedError fires the mesh fallback — the "stuck connecting"
    // stall. A raw TCP probe answers in milliseconds at home and fails in
    // ~2.5s off-LAN, so startup lands on the right origin in seconds.
    var startUrl by remember(serverUrl) { mutableStateOf<String?>(null) }
    var raceNonce by remember { mutableIntStateOf(0) }
    var kioskCanGoBack by remember { mutableStateOf(false) }
    val appContext = LocalContext.current.applicationContext
    LaunchedEffect(serverUrl, meshFallbackUrl, raceNonce) {
        // A retained live session exists — reattach instantly: no race, no
        // reload, no re-login (remote ⇄ dashboard round trip).
        if (KioskWebView.instance != null && KioskWebView.url == serverUrl) {
            isLoading = false
            startUrl = serverUrl
            return@LaunchedEffect
        }
        val picked = pickStartUrl(appContext, serverUrl, meshFallbackUrl)
        // Starting on the mesh: don't bounce back to it on error (it IS it).
        if (picked != serverUrl) triedMeshFallback = true
        startUrl = picked
    }

    // Web-page camera access (wallet QR scanner). The WebView's default
    // WebChromeClient silently denies getUserMedia, so grant video capture —
    // asking for the app-level CAMERA permission first when needed.
    val webViewContext = LocalContext.current
    var pendingWebPermission by remember { mutableStateOf<PermissionRequest?>(null) }
    val webCameraPermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        pendingWebPermission?.let { req ->
            if (granted) req.grant(arrayOf(PermissionRequest.RESOURCE_VIDEO_CAPTURE)) else req.deny()
        }
        pendingWebPermission = null
    }

    // A node app that refused iframing, opened in a local WebView overlay.
    // null = no overlay. The kiosk WebView underneath stays alive (and warm)
    // while this is shown, so closing it returns instantly with no reload.
    var inAppLaunch by remember { mutableStateOf<InAppLaunch?>(null) }

    // Same node = EITHER of its addresses. Over the mesh the kiosk's host is
    // the ULA while app links may carry the LAN IP (and vice versa) —
    // comparing against one host bounced same-node apps (Pine, Home
    // Assistant, BTCPay) out to the phone's external browser.
    fun isSameNode(url: String): Boolean =
        isSameHost(url, serverUrl) ||
            (meshFallbackUrl != null && isSameHost(url, meshFallbackUrl))

    // Native wallet QR scanner, opened by the web UI via the ArchipelagoQr
    // bridge; status lines stream back from the page while it's up.
    var walletScannerVisible by remember { mutableStateOf(false) }
    var walletScannerStatus by remember { mutableStateOf<Pair<String, Boolean>?>(null) }

    // One-time three-finger-hold teaching overlay (initial=true: never flash
    // it while DataStore is still loading).
    val prefs = remember { ServerPreferences(webViewContext) }

    // Hub menu overlay state — the three-finger hold opens the menu right here
    // over the dashboard (it used to jump to the remote screen).
    val savedServers by prefs.savedServers.collectAsState(initial = emptyList())
    val activeServer by prefs.activeServer.collectAsState(initial = null)
    var showHubMenu by remember { mutableStateOf(false) }
    var showPairScanner by remember { mutableStateOf(false) }

    val gestureHintSeen by prefs.gestureHintSeen.collectAsState(initial = true)
    var gestureHintDismissed by remember { mutableStateOf(false) }
    // Don't teach the gesture on top of the login/splash — arm the overlay
    // ~2 minutes after the kiosk first finishes loading, once the user has
    // settled in.
    var gestureHintReady by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        snapshotFlow { isLoading }.first { !it }
        delay(120_000)
        gestureHintReady = true
    }
    val scope = rememberCoroutineScope()

    // <input type="file"> support — without a chooser implementation the
    // WebView silently ignores file inputs (broke the wallet's upload path).
    var pendingFileChooser by remember { mutableStateOf<ValueCallback<Array<android.net.Uri>>?>(null) }
    val fileChooserLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { result ->
        pendingFileChooser?.onReceiveValue(
            WebChromeClient.FileChooserParams.parseResult(result.resultCode, result.data),
        )
        pendingFileChooser = null
    }

    // canGoBack() is a live Chromium call, not snapshot state — read from
    // composition it was stale (navigation doesn't recompose). Tracked via
    // doUpdateVisitedHistory instead, like the in-app overlay already does.
    BackHandler(enabled = inAppLaunch == null && kioskCanGoBack) {
        webView?.goBack()
    }

    // Roaming re-pick (Wi-Fi ⇄ 5G). The LAN-vs-mesh choice above is made once
    // at connect time, but the network it was made on can vanish mid-session:
    // walk off Wi-Fi and the page sits on a dead 192.168.x.x origin. The SPA's
    // failing XHRs/WebSockets never fire onReceivedError (main-frame only), so
    // nothing recovered without a manual Retry. Watch real transports coming
    // and going instead: on any change, probe the origin we're on and hop to
    // the other one if it's dead — a ~2-3s automatic blip instead of a dead
    // page. A live origin is left alone (no forced reload mid-task).
    // (registerNetworkCallback with a request, not a default-network callback:
    // with the mesh up this app's default network is the TUN itself, which
    // never changes across Wi-Fi ⇄ 5G — NOT_VPN is a default request
    // capability, so this sees the underlying transports.)
    DisposableEffect(serverUrl, meshFallbackUrl) {
        val cm = appContext.getSystemService(android.net.ConnectivityManager::class.java)
        var reprobeJob: kotlinx.coroutines.Job? = null
        fun onNetworksChanged() {
            reprobeJob?.cancel()
            reprobeJob = scope.launch {
                // Let the switch settle; marginal Wi-Fi flaps Wi-Fi ⇄ cell
                // in bursts and each event cancels the previous pass.
                delay(1_500)
                if (hasError) {
                    // The error pane is up — a new network is exactly when
                    // a retry can succeed. Same steps as the Retry button.
                    StartUrlCache.invalidate(
                        appContext,
                        StartUrlCache.key(serverUrl, meshFallbackUrl),
                    )
                    KioskWebView.drop()
                    webView = null
                    hasError = false
                    isLoading = true
                    triedMeshFallback = false
                    startUrl = null
                    raceNonce++
                    return@launch
                }
                val mesh = meshFallbackUrl ?: return@launch
                val current = webView?.url ?: startUrl ?: return@launch
                val onMesh = current.startsWith(mesh)
                val currentOrigin = if (onMesh) mesh else serverUrl
                val alive = withContext(Dispatchers.IO) { tcpAnswers(currentOrigin, 2_000) }
                if (alive) return@launch
                // Mesh probes get patience: the tunnel is re-homing onto
                // this same new network and a session rebuild takes seconds.
                val other = if (onMesh) serverUrl else mesh
                val otherAlive = withContext(Dispatchers.IO) {
                    tcpAnswers(other, if (onMesh) 2_500 else 10_000)
                }
                if (!otherAlive) return@launch
                StartUrlCache.put(
                    appContext,
                    StartUrlCache.key(serverUrl, meshFallbackUrl),
                    other,
                )
                triedMeshFallback = other == mesh
                isLoading = true
                // Keep the user's place — origins differ only in host, so the
                // path and hash carry over to the live one.
                val target = if (current.startsWith(currentOrigin)) {
                    other + current.removePrefix(currentOrigin)
                } else {
                    other
                }
                webView?.loadUrl(target)
            }
        }
        val cb = object : android.net.ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: android.net.Network) = onNetworksChanged()
            override fun onLost(network: android.net.Network) = onNetworksChanged()
        }
        if (cm != null) {
            val request = android.net.NetworkRequest.Builder()
                .addCapability(android.net.NetworkCapabilities.NET_CAPABILITY_INTERNET)
                .build()
            runCatching { cm.registerNetworkCallback(request, cb) }
        }
        onDispose {
            reprobeJob?.cancel()
            if (cm != null) runCatching { cm.unregisterNetworkCallback(cb) }
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(SurfaceBlack),
    ) {
        if (hasError) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .windowInsetsPadding(WindowInsets.safeDrawing)
                    .padding(32.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                Icon(
                    imageVector = Icons.Default.CloudOff,
                    contentDescription = null,
                    modifier = Modifier.size(64.dp),
                    tint = TextMuted,
                )

                Spacer(modifier = Modifier.height(24.dp))

                Text(
                    text = stringResource(R.string.server_unreachable),
                    style = MaterialTheme.typography.headlineMedium,
                    color = TextPrimary,
                    textAlign = TextAlign.Center,
                )

                Spacer(modifier = Modifier.height(12.dp))

                Text(
                    text = stringResource(R.string.connection_failed),
                    style = MaterialTheme.typography.bodyMedium,
                    color = TextMuted,
                    textAlign = TextAlign.Center,
                )

                Spacer(modifier = Modifier.height(32.dp))

                GlassButton(
                    text = stringResource(R.string.retry),
                    onClick = {
                        // Re-race LAN vs mesh — the network we're on may have
                        // changed since the last pick, so the remembered
                        // origin is suspect too. Drop the retained view:
                        // an errored session must genuinely reload.
                        StartUrlCache.invalidate(
                            webViewContext,
                            StartUrlCache.key(serverUrl, meshFallbackUrl),
                        )
                        KioskWebView.drop()
                        webView = null
                        hasError = false
                        isLoading = true
                        triedMeshFallback = false
                        startUrl = null
                        raceNonce++
                    },
                    modifier = Modifier.fillMaxWidth().height(56.dp),
                )

                Spacer(modifier = Modifier.height(12.dp))

                GlassButton(
                    text = stringResource(R.string.disconnect),
                    onClick = {
                        KioskWebView.drop()
                        onDisconnect()
                    },
                    modifier = Modifier.fillMaxWidth().height(48.dp),
                )
            }
        } else if (startUrl == null) {
            // Racing LAN vs mesh (≤2.5s at home, a few seconds off-LAN) —
            // far cheaper than letting Chromium retry a dead LAN IP.
            // No mesh URL = not a FIPS node, so no mesh branding on the wait.
            MeshLoadingScreen(mesh = meshFallbackUrl != null)
        } else {
            // Edge-to-edge WebView — background bleeds behind status bar.
            // Safe area values injected as CSS env() polyfill on each page load.
            val initialUrl = startUrl ?: serverUrl
            AndroidView(
                modifier = Modifier.fillMaxSize(),
                factory = { context ->
                    // Reattach the retained kiosk WebView (remote ⇄ dashboard
                    // must not reload the node UI). Everything configured
                    // below is idempotent, and re-running it rebinds clients,
                    // bridges and listeners to THIS composition's state —
                    // stale closures from the previous visit are replaced.
                    if (KioskWebView.url != serverUrl) KioskWebView.drop()
                    val reused = KioskWebView.instance
                    (reused ?: WebView(context)).apply {
                        (parent as? ViewGroup)?.removeView(this)
                        layoutParams = ViewGroup.LayoutParams(
                            ViewGroup.LayoutParams.MATCH_PARENT,
                            ViewGroup.LayoutParams.MATCH_PARENT,
                        )

                        isVerticalScrollBarEnabled = false
                        isHorizontalScrollBarEnabled = false

                        val cookieManager = CookieManager.getInstance()
                        cookieManager.setAcceptCookie(true)
                        cookieManager.setAcceptThirdPartyCookies(this, true)

                        applyArchipelagoSettings()
                        settings.apply {
                            setSupportMultipleWindows(true)  // enables onCreateWindow for window.open
                            // Let JS open windows without a synchronous user-gesture
                            // chain; without this, window.open() from a Vue click
                            // handler silently no-ops and "Open in new tab" dies.
                            javaScriptCanOpenWindowsAutomatically = true
                        }

                        val webViewRef = this

                        // Re-inject the safe-area vars whenever REAL insets
                        // arrive — on cold start onPageFinished often beats
                        // window attachment and would otherwise bake in 0px.
                        setOnApplyWindowInsetsListener { v, insets ->
                            (v as? WebView)?.let { injectSafeAreaVars(it) }
                            v.onApplyWindowInsets(insets)
                        }

                        // Decide where an outbound URL goes:
                        //  - same host as the node  → in-app WebView overlay
                        //    (this is the "open in browser" target for apps the
                        //    kiosk couldn't iframe — keep the user inside the app)
                        //  - different host         → the phone's real browser
                        fun routeOutbound(url: String) {
                            if (isSameNode(url)) {
                                inAppLaunch = InAppLaunch(url)
                            } else {
                                openExternalUrl(context, url)
                            }
                        }

                        // Bridge callbacks are DELEGATED through the holder:
                        // the interface objects registered on a retained
                        // WebView survive recomposition, and re-registering
                        // them doesn't reliably swap the JS-visible object
                        // without a reload — with direct closures, a
                        // remote ⇄ dashboard round trip left the bridges
                        // writing to a dead composition's state ("apps don't
                        // launch"). These assignments re-point the live
                        // interface objects at THIS composition every attach.
                        KioskWebView.onRouteOutbound = { url -> routeOutbound(url) }
                        KioskWebView.onOpenInApp = { launch -> inAppLaunch = launch }
                        KioskWebView.onQrOpen = {
                            walletScannerStatus = null
                            walletScannerVisible = true
                        }
                        KioskWebView.onQrStatus = { msg, err -> walletScannerStatus = msg to err }
                        KioskWebView.onQrClose = { walletScannerVisible = false }

                        // JS bridge. The web UI calls:
                        //   window.ArchipelagoNative.openExternal(url) — host-routed
                        //   window.ArchipelagoNative.openInApp(url)    — force in-app
                        // Falls back to window.open in a plain mobile browser.
                        if (reused == null) addJavascriptInterface(
                            object {
                                @android.webkit.JavascriptInterface
                                fun openExternal(url: String) {
                                    webViewRef.post { KioskWebView.onRouteOutbound(url) }
                                }

                                @android.webkit.JavascriptInterface
                                fun openInApp(url: String) {
                                    webViewRef.post { KioskWebView.onOpenInApp(InAppLaunch(url)) }
                                }

                                // Richer launch: the web UI passes the app's
                                // catalog icon + display name for the branded
                                // pulsating loader (never a site favicon).
                                @android.webkit.JavascriptInterface
                                fun openInAppEx(url: String, iconUrl: String, name: String) {
                                    webViewRef.post {
                                        KioskWebView.onOpenInApp(
                                            InAppLaunch(
                                                url,
                                                iconUrl.takeUnless { it.isBlank() },
                                                name.takeUnless { it.isBlank() },
                                            ),
                                        )
                                    }
                                }
                            },
                            "ArchipelagoNative",
                        )

                        // Clipboard bridge — the node UI is plain HTTP, so the
                        // page's own navigator.clipboard can't read (see
                        // addClipboardBridge).
                        if (reused == null) addClipboardBridge()

                        // Wallet QR bridge. The web scan modal calls:
                        //   window.ArchipelagoQr.open()             — show the native scanner
                        //   window.ArchipelagoQr.setStatus(msg, e)  — mirror status/progress lines
                        //   window.ArchipelagoQr.close()            — code accepted, tear down
                        //   window.ArchipelagoQr.prewarm()          — scan is likely; warm the camera
                        // Decodes flow back through window.__archyQrResult(text);
                        // a user cancel calls window.__archyQrCancelled().
                        if (reused == null) addJavascriptInterface(
                            object {
                                @android.webkit.JavascriptInterface
                                fun open() {
                                    webViewRef.post { KioskWebView.onQrOpen() }
                                }

                                /** Optional hint from the page (e.g. the wallet
                                 *  modal opening) that a scan is coming: pays
                                 *  CameraX/decoder init now instead of on the
                                 *  user's first frame. Safe to call repeatedly —
                                 *  it runs once per process. */
                                @android.webkit.JavascriptInterface
                                fun prewarm() {
                                    prewarmQrScanner(appContext)
                                }

                                @android.webkit.JavascriptInterface
                                fun setStatus(message: String, isError: Boolean) {
                                    webViewRef.post { KioskWebView.onQrStatus(message, isError) }
                                }

                                @android.webkit.JavascriptInterface
                                fun close() {
                                    webViewRef.post { KioskWebView.onQrClose() }
                                }
                            },
                            "ArchipelagoQr",
                        )

                        webViewClient = object : WebViewClient() {
                            override fun onPageStarted(view: WebView?, url: String?, favicon: Bitmap?) {
                                isLoading = true
                                hasError = false
                                // New document — the injected safe-area style is
                                // gone; reset the dedup stamp so it re-injects.
                                view?.tag = null
                                // Before the app boots, so its own clipboard
                                // stand-in never takes the slot.
                                view?.let { injectClipboardShim(it) }
                            }

                            override fun doUpdateVisitedHistory(view: WebView?, url: String?, isReload: Boolean) {
                                kioskCanGoBack = view?.canGoBack() == true
                            }

                            override fun onPageFinished(view: WebView?, url: String?) {
                                isLoading = false
                                kioskCanGoBack = view?.canGoBack() == true
                                if (view == null) return

                                injectSafeAreaVars(view)
                                // Idempotent — covers a document that swapped
                                // navigator.clipboard after onPageStarted.
                                injectClipboardShim(view)
                                // The wallet is one tap away on any node page;
                                // warm the scanner while the user reads it.
                                prewarmQrScanner(view.context.applicationContext)

                                // Auto-login with the stored password (QR pairing /
                                // saved server) — only on our own server's pages
                                // (LAN origin or the mesh address).
                                val onOwnServer = url != null && (
                                    url.startsWith(serverUrl) ||
                                        (meshFallbackUrl != null && url.startsWith(meshFallbackUrl))
                                    )
                                if (serverPassword.isNotBlank() && onOwnServer) {
                                    view.evaluateJavascript(buildAutoLoginScript(serverPassword), null)
                                }
                            }

                            override fun onReceivedError(
                                view: WebView?,
                                request: WebResourceRequest?,
                                error: WebResourceError?,
                            ) {
                                if (request?.isForMainFrame == true) {
                                    val mesh = meshFallbackUrl
                                    if (mesh != null && !triedMeshFallback &&
                                        request.url?.toString()?.startsWith(mesh) != true
                                    ) {
                                        triedMeshFallback = true
                                        isLoading = true
                                        view?.loadUrl(mesh)
                                        return
                                    }
                                    hasError = true
                                    isLoading = false
                                }
                            }

                            // Node apps (e.g. NetBird) terminate TLS with a
                            // self-signed cert — the dashboard needs a secure
                            // context for OIDC/window.crypto.subtle (#15). The
                            // WebView default is to CANCEL untrusted certs, so
                            // those apps render blank. The user explicitly trusts
                            // their own node, so proceed for same-host certs only;
                            // reject anything else (don't blanket-trust the web).
                            override fun onReceivedSslError(
                                view: WebView?,
                                handler: android.webkit.SslErrorHandler?,
                                error: android.net.http.SslError?,
                            ) {
                                val u = error?.url
                                if (u != null && isSameNode(u)) {
                                    handler?.proceed()
                                } else {
                                    handler?.cancel()
                                }
                            }

                            override fun shouldOverrideUrlLoading(
                                view: WebView?,
                                request: WebResourceRequest?,
                            ): Boolean {
                                val url = request?.url?.toString() ?: return false
                                // Keep kiosk navigation (same origin incl. port) in place
                                if (url.startsWith(serverUrl)) return false
                                // Mesh address is the same server too
                                if (meshFallbackUrl != null && url.startsWith(meshFallbackUrl)) return false
                                // Same node (other port) → in-app; external → browser
                                routeOutbound(url)
                                return true
                            }
                        }

                        webChromeClient = object : WebChromeClient() {
                            override fun onProgressChanged(view: WebView?, newProgress: Int) {
                                loadProgress = newProgress
                            }

                            override fun onShowFileChooser(
                                view: WebView?,
                                filePathCallback: ValueCallback<Array<android.net.Uri>>?,
                                fileChooserParams: FileChooserParams?,
                            ): Boolean {
                                pendingFileChooser?.onReceiveValue(null)
                                pendingFileChooser = filePathCallback
                                val intent = fileChooserParams?.createIntent()
                                if (intent == null) {
                                    pendingFileChooser = null
                                    return false
                                }
                                return try {
                                    fileChooserLauncher.launch(intent)
                                    true
                                } catch (_: Exception) {
                                    pendingFileChooser = null
                                    false
                                }
                            }

                            // Wallet QR scanner: grant the page camera access.
                            // Only video capture is granted — anything else the
                            // page asks for is denied as before.
                            override fun onPermissionRequest(request: PermissionRequest) {
                                if (PermissionRequest.RESOURCE_VIDEO_CAPTURE !in request.resources) {
                                    request.deny()
                                    return
                                }
                                val hasCamera = ContextCompat.checkSelfPermission(
                                    webViewContext, Manifest.permission.CAMERA,
                                ) == PackageManager.PERMISSION_GRANTED
                                if (hasCamera) {
                                    request.grant(arrayOf(PermissionRequest.RESOURCE_VIDEO_CAPTURE))
                                } else {
                                    pendingWebPermission = request
                                    webCameraPermissionLauncher.launch(Manifest.permission.CAMERA)
                                }
                            }

                            // window.open() — e.g. the kiosk's "Open in new tab"
                            // for an app that can't be iframed. Capture the target
                            // URL via a throwaway WebView and route it ourselves.
                            override fun onCreateWindow(
                                view: WebView?,
                                isDialog: Boolean,
                                isUserGesture: Boolean,
                                resultMsg: android.os.Message?,
                            ): Boolean {
                                val transport = resultMsg?.obj as? WebView.WebViewTransport
                                    ?: return false

                                val popup = WebView(context).apply {
                                    settings.javaScriptEnabled = true
                                    webViewClient = object : WebViewClient() {
                                        override fun shouldOverrideUrlLoading(
                                            view: WebView?,
                                            request: WebResourceRequest?,
                                        ): Boolean {
                                            val url = request?.url?.toString() ?: return true
                                            routeOutbound(url)
                                            return true
                                        }

                                        override fun onPageStarted(view: WebView?, url: String?, favicon: Bitmap?) {
                                            if (url != null) routeOutbound(url)
                                            view?.stopLoading()
                                        }
                                    }
                                }
                                transport.webView = popup
                                resultMsg.sendToTarget()
                                return true
                            }
                        }

                        // Three-finger hold (500ms) → open the hub menu overlay
                        // in place (Remote/Keyboard cards do the navigating).
                        // Three fingers, not two: two-finger scroll/pinch on the
                        // page collided with the old two-finger hold.
                        var threeFingerStart = 0L
                        var threeFingerFired = false
                        setOnTouchListener { _, event ->
                            val pointerCount = event.pointerCount
                            when (event.actionMasked) {
                                android.view.MotionEvent.ACTION_POINTER_DOWN -> {
                                    if (pointerCount >= 3) {
                                        threeFingerStart = System.currentTimeMillis()
                                        threeFingerFired = false
                                    }
                                }
                                android.view.MotionEvent.ACTION_MOVE -> {
                                    if (pointerCount >= 3 && !threeFingerFired && threeFingerStart > 0) {
                                        if (System.currentTimeMillis() - threeFingerStart > 500) {
                                            threeFingerFired = true
                                            showHubMenu = true
                                        }
                                    }
                                }
                                android.view.MotionEvent.ACTION_UP,
                                android.view.MotionEvent.ACTION_POINTER_UP,
                                android.view.MotionEvent.ACTION_CANCEL -> {
                                    if (event.pointerCount <= 3) {
                                        threeFingerStart = 0L
                                    }
                                }
                            }
                            false // don't consume — let WebView handle normally
                        }

                        webView = this
                        if (reused == null) {
                            KioskWebView.instance = this
                            KioskWebView.url = serverUrl
                            loadUrl(initialUrl)
                        } else {
                            // Reattached views keep stale measurements until an
                            // input event — that was the top/bottom UI being
                            // wrong until a tap. Force a fresh pass, and re-sync
                            // the page's safe-area vars while we're at it.
                            post {
                                requestLayout()
                                invalidate()
                                injectSafeAreaVars(this)
                            }
                        }
                    }
                },
            )

            // Loading bar at top edge
            AnimatedVisibility(
                visible = isLoading,
                enter = fadeIn(),
                exit = fadeOut(),
            ) {
                LinearProgressIndicator(
                    progress = { loadProgress / 100f },
                    modifier = Modifier.fillMaxWidth(),
                    color = BitcoinOrange,
                    trackColor = SurfaceBlack,
                )
            }

            // Branded first-load screen while the mesh session comes up.
            AnimatedVisibility(
                visible = isLoading && !firstLoadDone,
                enter = fadeIn(),
                exit = fadeOut(),
            ) {
                Column(
                    Modifier.fillMaxSize().background(SurfaceBlack),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center,
                ) {
                    // The mesh boast is only honest when the mesh is actually
                    // carrying this session. A plain node gets the badge.
                    if (meshFallbackUrl != null) {
                        Text(
                            buildAnnotatedString {
                                withStyle(SpanStyle(color = ErrorRed)) { append("F*CK") }
                                withStyle(SpanStyle(color = TextPrimary)) { append(" IPS") }
                            },
                            fontSize = 40.sp,
                            fontWeight = FontWeight.Black,
                            letterSpacing = 4.sp,
                            textAlign = TextAlign.Center,
                        )
                    } else {
                        Image(
                            painter = painterResource(id = R.drawable.ic_logo),
                            contentDescription = null,
                            modifier = Modifier.size(96.dp),
                        )
                    }
                    Spacer(Modifier.height(10.dp))
                    Text(
                        "connecting to your archipelago",
                        color = TextMuted,
                        fontSize = 13.sp,
                        letterSpacing = 1.sp,
                    )
                    Spacer(Modifier.height(28.dp))
                    SlidingLoader(modifier = Modifier.width(220.dp))
                }
            }

            // In-app browser overlay for non-iframeable node apps. Rendered last
            // so it sits above the kiosk WebView, which stays alive underneath.
            inAppLaunch?.let { target ->
                InAppBrowser(
                    url = target.url,
                    serverUrl = serverUrl,
                    meshUrl = meshFallbackUrl,
                    appIcon = target.icon,
                    appName = target.name,
                    onClose = { inAppLaunch = null },
                )
            }

            // Native wallet QR scanner, opened by the page via ArchipelagoQr.
            WalletQrScannerModal(
                visible = walletScannerVisible,
                status = walletScannerStatus,
                onDecoded = { text ->
                    webView?.evaluateJavascript(
                        "window.__archyQrResult && window.__archyQrResult(${JSONObject.quote(text)})",
                        null,
                    )
                },
                onDismiss = {
                    walletScannerVisible = false
                    webView?.evaluateJavascript(
                        "window.__archyQrCancelled && window.__archyQrCancelled()",
                        null,
                    )
                },
            )

            // First-launch teaching overlay for the three-finger hold — armed
            // ~2 minutes after login so it never fights the splash/first look.
            if (gestureHintReady && !gestureHintSeen && !gestureHintDismissed &&
                !isLoading && inAppLaunch == null
            ) {
                GestureHintOverlay(
                    onDismiss = {
                        gestureHintDismissed = true
                        scope.launch { prefs.markGestureHintSeen() }
                    },
                )
            }
        }

        // Hub menu overlay — opened by the three-finger hold, drawn above
        // everything (also reachable from the error screen, where switching
        // servers is exactly what's needed).
        NESMenu(
            visible = showHubMenu,
            servers = savedServers,
            activeServer = activeServer,
            onDismiss = { showHubMenu = false },
            onSelectServer = { server ->
                showHubMenu = false
                scope.launch {
                    prefs.setActiveServer(server)
                    // Same policy as the launch picker: the tunnel follows the
                    // node. Switching to a plain node hands Android's single
                    // VPN slot back so the user's own VPN can reach it.
                    withContext(Dispatchers.IO) {
                        if (server.isFipsNode()) {
                            FipsManager.autoStartIfReady(webViewContext)
                        } else {
                            FipsManager.stopService(webViewContext)
                        }
                    }
                }
            },
            onAddServer = { server ->
                scope.launch {
                    prefs.addSavedServer(server)
                    if (activeServer == null) prefs.setActiveServer(server)
                }
            },
            onScanQr = { showPairScanner = true },
            onEditServer = { original, updated ->
                scope.launch {
                    prefs.updateSavedServer(original, updated)
                    // Editing the live server reloads the kiosk with the new
                    // address/credentials via the activeServer recomposition.
                    if (original.serialize() == activeServer?.serialize()) {
                        prefs.setActiveServer(updated)
                    }
                }
            },
            onRemoveServer = { server ->
                scope.launch {
                    prefs.removeSavedServer(server)
                    // Nothing left to show — back to the Connect screen.
                    val remaining = savedServers.count { it.serialize() != server.serialize() }
                    if (remaining == 0) {
                        prefs.clearActiveServer()
                        showHubMenu = false
                        onDisconnect()
                    }
                }
            },
            onRemote = { showHubMenu = false; onRemoteInput() },
            onKeyboard = { showHubMenu = false; onRemoteKeyboard() },
            onBackToWebView = { showHubMenu = false },
            onMeshParty = onMeshParty?.let { open -> { showHubMenu = false; open() } },
        )

        // Pairing-QR scan launched from the menu's Nodes page; the menu stays
        // open behind it so the new entry appears as soon as it closes.
        QrScannerOverlay(
            visible = showPairScanner,
            onDismiss = { showPairScanner = false },
            onServerScanned = { scan ->
                showPairScanner = false
                scope.launch {
                    val merged = prefs.upsertServer(scan.server)
                    FipsManager.registerNode(webViewContext, scan.fips, merged.displayName())
                    if (activeServer == null) prefs.setActiveServer(merged)
                }
            },
        )
    }
}

/** Catalog icons re-fetched over the mesh on every app open made the loader
 *  show the fallback logo first — cache the handful of icons a node has. */
private val iconCache = androidx.collection.LruCache<String, Bitmap>(24)

/** Best-effort fetch of an image URL (the app's catalog icon) for the branded
 *  loading screen. Blocking — call on IO. */
private fun fetchBitmap(imageUrl: String): Bitmap? {
    iconCache.get(imageUrl)?.let { return it }
    return try {
        val conn = (java.net.URL(imageUrl).openConnection()
            as java.net.HttpURLConnection).apply {
            connectTimeout = 4000
            readTimeout = 4000
            instanceFollowRedirects = true
        }
        conn.inputStream.use { BitmapFactory.decodeStream(it) }
            ?.also { iconCache.put(imageUrl, it) }
    } catch (_: Exception) {
        null
    }
}

/**
 * Lightweight in-app browser used when the kiosk hands off an app that can't be
 * shown in an iframe. Loads the app in a local WebView with a centered loading
 * screen (app favicon + progress bar) and a BOTTOM control bar mirroring the
 * web mobile-iframe footer (back / forward / reload / open-in-browser / close).
 * Same-host navigation stays here; any genuinely external link escapes to the
 * phone's browser.
 */
@SuppressLint("SetJavaScriptEnabled")
@Composable
private fun InAppBrowser(
    url: String,
    serverUrl: String,
    meshUrl: String? = null,
    appIcon: String? = null,
    appName: String? = null,
    onClose: () -> Unit,
) {
    val context = LocalContext.current
    // Same-node check across BOTH node addresses (LAN + mesh ULA) — see the
    // kiosk's isSameNode; a mismatch here bounced app links to the browser.
    fun isSameNode(u: String): Boolean =
        isSameHost(u, serverUrl) || (meshUrl != null && isSameHost(u, meshUrl))
    var browser by remember { mutableStateOf<WebView?>(null) }
    // Loader title: never show a raw IP host — a mesh ULA like
    // [fd79:1aa:…] is technically the host but reads as garbage on the
    // loading screen. Show a neutral name until the page reports its
    // real <title> (onReceivedTitle upgrades it).
    var title by remember {
        mutableStateOf(
            appName
                ?: android.net.Uri.parse(url).host
                    ?.takeUnless { it.contains(':') || it.matches(Regex("^\\d+(\\.\\d+){3}$")) }
                ?: "Archipelago",
        )
    }
    // Loader icon: the app's CATALOG icon passed by the web UI — never the
    // site favicon (generic favicons made the loader look broken). Falls back
    // to the Archipelago pixel logo while absent.
    var loaderIcon by remember { mutableStateOf<Bitmap?>(null) }
    var progress by remember { mutableIntStateOf(0) }
    var loading by remember { mutableStateOf(true) }
    var canGoBack by remember { mutableStateOf(false) }
    var canGoForward by remember { mutableStateOf(false) }
    // Main-frame load failure — the branded offline screen renders instead of
    // Chromium's stock "Webpage not available" page (never show that).
    var loadError by remember { mutableStateOf(false) }

    // Same camera bridge as the main WebView — node apps opened in the overlay
    // (e.g. anything with a QR scanner) get getUserMedia too.
    var pendingWebPermission by remember { mutableStateOf<PermissionRequest?>(null) }
    val webCameraPermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        pendingWebPermission?.let { req ->
            if (granted) req.grant(arrayOf(PermissionRequest.RESOURCE_VIDEO_CAPTURE)) else req.deny()
        }
        pendingWebPermission = null
    }

    // Fetch the app's catalog icon for the loader (absolute URL from the web
    // UI). No favicon fallback — the pixel logo is the brand fallback.
    LaunchedEffect(appIcon) {
        if (appIcon != null) {
            loaderIcon = withContext(Dispatchers.IO) { fetchBitmap(appIcon) }
        }
    }

    // Back: walk the in-app history first, then close the overlay.
    BackHandler {
        val b = browser
        if (b != null && b.canGoBack()) b.goBack() else onClose()
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(SurfaceBlack)
            // Whole-overlay touch shield: every touch not handled by a child
            // (control-bar gaps, inset strips) dies here instead of falling
            // through to the kiosk's tab bar behind (a near-miss on Close
            // was opening the AIUI tab underneath).
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = {},
            )
            // Bottom inset handled by the touch-shield strip below the bar.
            .windowInsetsPadding(
                WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal)
            ),
    ) {
        // No native top strip: the WebView draws edge-to-edge so page
        // BACKGROUNDS extend into the status-bar area; injectTopInset keeps
        // page CONTENT below it (the design contract for every in-app page).
        // WebView + loading overlay fill the area above the bottom control bar.
        Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
            AndroidView(
                modifier = Modifier.fillMaxSize(),
                factory = { ctx ->
                    WebView(ctx).apply {
                        layoutParams = ViewGroup.LayoutParams(
                            ViewGroup.LayoutParams.MATCH_PARENT,
                            ViewGroup.LayoutParams.MATCH_PARENT,
                        )
                        isVerticalScrollBarEnabled = false
                        isHorizontalScrollBarEnabled = false

                        CookieManager.getInstance().setAcceptThirdPartyCookies(this, true)
                        applyArchipelagoSettings()
                        // Node apps (BTCPay invoices, LND, Portainer tokens) are
                        // served over plain HTTP too — same dead-clipboard trap.
                        addClipboardBridge()

                        webChromeClient = object : WebChromeClient() {
                            override fun onProgressChanged(view: WebView?, newProgress: Int) {
                                progress = newProgress
                            }

                            override fun onReceivedTitle(view: WebView?, t: String?) {
                                if (!t.isNullOrBlank()) title = t
                            }

                            override fun onPermissionRequest(request: PermissionRequest) {
                                if (PermissionRequest.RESOURCE_VIDEO_CAPTURE !in request.resources) {
                                    request.deny()
                                    return
                                }
                                val hasCamera = ContextCompat.checkSelfPermission(
                                    context, Manifest.permission.CAMERA,
                                ) == PackageManager.PERMISSION_GRANTED
                                if (hasCamera) {
                                    request.grant(arrayOf(PermissionRequest.RESOURCE_VIDEO_CAPTURE))
                                } else {
                                    pendingWebPermission = request
                                    webCameraPermissionLauncher.launch(Manifest.permission.CAMERA)
                                }
                            }
                        }

                        webViewClient = object : WebViewClient() {
                            override fun onPageStarted(view: WebView?, u: String?, favicon: Bitmap?) {
                                loading = true
                                loadError = false
                                view?.let {
                                    injectTopInset(it)
                                    injectClipboardShim(it)
                                }
                            }

                            override fun onPageFinished(view: WebView?, u: String?) {
                                loading = false
                                canGoBack = view?.canGoBack() == true
                                canGoForward = view?.canGoForward() == true
                                view?.let {
                                    injectTopInset(it)
                                    injectClipboardShim(it)
                                }
                            }

                            override fun onReceivedError(
                                view: WebView?,
                                request: WebResourceRequest?,
                                error: WebResourceError?,
                            ) {
                                // Sub-resource failures are the page's problem;
                                // only a dead MAIN FRAME gets the offline screen.
                                if (request?.isForMainFrame == true) {
                                    loadError = true
                                    loading = false
                                }
                            }

                            override fun doUpdateVisitedHistory(view: WebView?, u: String?, isReload: Boolean) {
                                canGoBack = view?.canGoBack() == true
                                canGoForward = view?.canGoForward() == true
                            }

                            // Self-signed TLS on the node's apps (e.g. NetBird on
                            // :8087) would otherwise be cancelled by the WebView
                            // and render blank. Proceed for the user's own node
                            // (same host); reject any other untrusted cert.
                            override fun onReceivedSslError(
                                view: WebView?,
                                handler: android.webkit.SslErrorHandler?,
                                error: android.net.http.SslError?,
                            ) {
                                val u = error?.url
                                if (u != null && isSameNode(u)) {
                                    handler?.proceed()
                                } else {
                                    handler?.cancel()
                                }
                            }

                            override fun shouldOverrideUrlLoading(
                                view: WebView?,
                                request: WebResourceRequest?,
                            ): Boolean {
                                val u = request?.url?.toString() ?: return false
                                // Stay in the overlay for same-node navigation;
                                // hand genuinely external links to the real browser.
                                if (isSameNode(u)) return false
                                openExternalUrl(ctx, u)
                                return true
                            }
                        }

                        browser = this
                        loadUrl(url)
                    }
                },
            )

            // Centered loading screen — app favicon (or spinner) + title + bar.
            if (loadError) {
                // Branded offline screen — Chromium's stock error page is
                // rendering underneath, and must never be visible.
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .background(SurfaceBlack)
                        .padding(32.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center,
                ) {
                    Icon(
                        imageVector = Icons.Default.CloudOff,
                        contentDescription = null,
                        modifier = Modifier.size(56.dp),
                        tint = TextMuted,
                    )
                    Spacer(modifier = Modifier.height(20.dp))
                    Text(
                        text = title,
                        style = MaterialTheme.typography.headlineSmall,
                        color = TextPrimary,
                        textAlign = TextAlign.Center,
                    )
                    Spacer(modifier = Modifier.height(10.dp))
                    Text(
                        text = stringResource(R.string.connection_failed),
                        style = MaterialTheme.typography.bodyMedium,
                        color = TextMuted,
                        textAlign = TextAlign.Center,
                    )
                    Spacer(modifier = Modifier.height(28.dp))
                    GlassButton(
                        text = stringResource(R.string.retry),
                        onClick = {
                            loadError = false
                            loading = true
                            browser?.reload()
                        },
                        modifier = Modifier.fillMaxWidth().height(52.dp),
                    )
                }
            } else if (loading) {
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .background(SurfaceBlack),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center,
                ) {
                    // Pulsating app-icon card — mirrors the web UI's
                    // AppLoadingScreen (84dp, r20, 1.8s scale/fade pulse).
                    val pulse = rememberInfiniteTransition(label = "loaderPulse")
                    val scale by pulse.animateFloat(
                        initialValue = 1f,
                        targetValue = 1.05f,
                        animationSpec = infiniteRepeatable(
                            animation = tween(900, easing = FastOutSlowInEasing),
                            repeatMode = RepeatMode.Reverse,
                        ),
                        label = "loaderScale",
                    )
                    val fade by pulse.animateFloat(
                        initialValue = 1f,
                        targetValue = 0.85f,
                        animationSpec = infiniteRepeatable(
                            animation = tween(900, easing = FastOutSlowInEasing),
                            repeatMode = RepeatMode.Reverse,
                        ),
                        label = "loaderFade",
                    )
                    Box(
                        modifier = Modifier
                            .size(84.dp)
                            .graphicsLayer {
                                scaleX = scale
                                scaleY = scale
                                alpha = fade
                            }
                            .clip(RoundedCornerShape(20.dp))
                            .background(Color.White.copy(alpha = 0.05f))
                            .border(
                                1.dp,
                                Color.White.copy(alpha = 0.08f),
                                RoundedCornerShape(20.dp),
                            ),
                        contentAlignment = Alignment.Center,
                    ) {
                        val icon = loaderIcon
                        if (icon != null) {
                            Image(
                                bitmap = icon.asImageBitmap(),
                                contentDescription = title,
                                modifier = Modifier.fillMaxSize(),
                            )
                        } else {
                            PixelArtLogo(Modifier.size(48.dp))
                        }
                    }
                    Spacer(modifier = Modifier.height(18.dp))
                    Text(
                        text = title,
                        style = MaterialTheme.typography.bodyLarge,
                        color = TextPrimary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Spacer(modifier = Modifier.height(16.dp))
                    LinearProgressIndicator(
                        progress = { progress / 100f },
                        modifier = Modifier.width(220.dp),
                        color = BitcoinOrange,
                        trackColor = TextMuted.copy(alpha = 0.2f),
                    )
                }
            }
        }

        // Bottom control bar — mirrors the web mobile-iframe footer.
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(56.dp)
                .background(SurfaceBlack)
                .padding(horizontal = 8.dp),
            horizontalArrangement = Arrangement.SpaceAround,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = { browser?.goBack() }, enabled = canGoBack) {
                Icon(
                    imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                    contentDescription = "Back",
                    tint = if (canGoBack) TextPrimary else TextMuted.copy(alpha = 0.4f),
                )
            }
            IconButton(onClick = { browser?.goForward() }, enabled = canGoForward) {
                Icon(
                    imageVector = Icons.AutoMirrored.Filled.ArrowForward,
                    contentDescription = "Forward",
                    tint = if (canGoForward) TextPrimary else TextMuted.copy(alpha = 0.4f),
                )
            }
            IconButton(onClick = { browser?.reload() }) {
                Icon(
                    imageVector = Icons.Default.Refresh,
                    contentDescription = "Reload",
                    tint = TextPrimary,
                )
            }
            IconButton(onClick = { openExternalUrl(context, browser?.url ?: url) }) {
                Icon(
                    imageVector = Icons.Default.OpenInBrowser,
                    contentDescription = stringResource(R.string.open_in_browser),
                    tint = TextPrimary,
                )
            }
            IconButton(onClick = onClose) {
                Icon(
                    imageVector = Icons.Default.Close,
                    contentDescription = stringResource(R.string.close),
                    tint = TextPrimary,
                )
            }
        }

        // Touch-shield over the gesture-nav strip: solid black AND consumes
        // taps — stray touches below the control bar landed on the kiosk's
        // tab bar behind this overlay (opening the AIUI chat by accident).
        Box(
            Modifier
                .fillMaxWidth()
                .windowInsetsBottomHeight(WindowInsets.navigationBars)
                .background(Color.Black)
                .clickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                    onClick = {},
                ),
        )
    }
}

/**
 * JS that fills the web UI login form (Login.vue's #login-password) with the
 * stored password and submits it once the form is interactive — the one-step
 * pairing flow. No-ops when the login step never appears (already
 * authenticated, first-boot setup, TOTP). At most two attempts per page load,
 * then it stops for good so a wrong stored password can't spam the node.
 */
private fun buildAutoLoginScript(password: String): String {
    val quoted = org.json.JSONObject.quote(password)
    return """
    (function () {
        if (window.__archyAutoLogin) return;
        window.__archyAutoLogin = true;
        var pw = $quoted;
        var attempts = 0;
        var started = Date.now();
        var timer = setInterval(function () {
            if (Date.now() - started > 45000) { clearInterval(timer); return; }
            var el = document.getElementById('login-password');
            if (!el) {
                // Field gone after we submitted = success or step change - stop.
                if (attempts > 0) clearInterval(timer);
                return;
            }
            if (el.disabled) return; // form waits on serverReady
            if (attempts >= 2) { clearInterval(timer); return; }
            attempts++;
            var setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
            setter.call(el, pw);
            el.dispatchEvent(new Event('input', { bubbles: true }));
            // Let Vue re-render before submitting: a synchronous Enter arrives
            // while the login button is still disabled, and the web UI's
            // controller-nav "Enter in input clicks the next enabled button"
            // pattern then hits Replay Intro instead — restarting the intro
            // cinematic on every connect (two frames = value flush + render).
            requestAnimationFrame(function () {
                requestAnimationFrame(function () {
                    el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
                });
            });
        }, 1500);
    })();
    """.trimIndent()
}
