package com.archipelago.app.ui.screens

import android.content.Context
import android.content.ContextWrapper
import android.graphics.Color
import android.view.View
import android.view.ViewGroup
import android.webkit.WebChromeClient
import android.widget.FrameLayout
import androidx.activity.ComponentActivity
import androidx.activity.OnBackPressedCallback
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

private fun Context.fullscreenActivity(): ComponentActivity? = when (this) {
    is ComponentActivity -> this
    is ContextWrapper -> baseContext.takeIf { it !== this }?.fullscreenActivity()
    else -> null
}

/** Hosts Chromium's custom fullscreen view without replacing or reloading its WebView. */
internal class WebViewFullscreen(private val activity: ComponentActivity?) {
    private var overlay: FrameLayout? = null
    private var callback: WebChromeClient.CustomViewCallback? = null
    private var back: OnBackPressedCallback? = null
    private var visibleBars = 0
    private var originalBehavior = 0

    fun show(view: View?, onHidden: WebChromeClient.CustomViewCallback?) {
        val owner = activity
        // A second enter must not detach the active video or strand its callback.
        if (owner == null || owner.isFinishing || owner.isDestroyed || view == null ||
            view.parent != null || overlay != null
        ) {
            onHidden?.onCustomViewHidden()
            return
        }
        val decor = owner.window.decorView as? ViewGroup
        if (decor == null) { onHidden?.onCustomViewHidden(); return }
        val controller = WindowCompat.getInsetsController(owner.window, decor)
        val insets = ViewCompat.getRootWindowInsets(decor)
        visibleBars = 0
        if (insets?.isVisible(WindowInsetsCompat.Type.statusBars()) != false) {
            visibleBars = visibleBars or WindowInsetsCompat.Type.statusBars()
        }
        if (insets?.isVisible(WindowInsetsCompat.Type.navigationBars()) != false) {
            visibleBars = visibleBars or WindowInsetsCompat.Type.navigationBars()
        }
        originalBehavior = controller.systemBarsBehavior
        val host = FrameLayout(owner).apply {
            setBackgroundColor(Color.BLACK)
            keepScreenOn = true
            addView(view, FrameLayout.LayoutParams(-1, -1))
        }
        overlay = host
        callback = onHidden
        decor.addView(host, ViewGroup.LayoutParams(-1, -1))
        controller.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        controller.hide(WindowInsetsCompat.Type.systemBars())
        back = object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() = hide()
        }.also { owner.onBackPressedDispatcher.addCallback(it) }
        view.requestFocus()
    }

    fun hide() {
        val host = overlay ?: return
        // Clear first: Chromium may synchronously call onHideCustomView again.
        overlay = null
        val notify = callback
        callback = null
        back?.remove()
        back = null
        host.keepScreenOn = false
        host.removeAllViews()
        (host.parent as? ViewGroup)?.removeView(host)
        activity?.let { owner ->
            val controller = WindowCompat.getInsetsController(owner.window, owner.window.decorView)
            controller.systemBarsBehavior = originalBehavior
            controller.hide(WindowInsetsCompat.Type.systemBars())
            if (visibleBars != 0) controller.show(visibleBars)
        }
        notify?.onCustomViewHidden()
    }
}

@Composable
internal fun rememberWebViewFullscreen(): WebViewFullscreen {
    val context = LocalContext.current
    val fullscreen = remember(context) { WebViewFullscreen(context.fullscreenActivity()) }
    DisposableEffect(fullscreen) { onDispose { fullscreen.hide() } }
    return fullscreen
}
