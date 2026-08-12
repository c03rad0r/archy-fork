package com.archipelago.app

import android.app.Application
import android.os.Looper
import android.webkit.WebView
import com.archipelago.app.data.ServerPreferences
import com.archipelago.app.fips.FipsNative
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

class ArchipelagoApp : Application() {

    private val warmupScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    override fun onCreate() {
        super.onCreate()

        // Warmups that otherwise land inside the first frame:
        // - FipsNative.available dlopens the 7 MB Rust core; referenced from
        //   composition (NESMenu, mesh auto-start), it blocked the UI thread.
        // - The first DataStore read gates the nav graph's start destination;
        //   parsing it here means the launch gate resolves in the first
        //   emission instead of waiting on cold disk IO.
        warmupScope.launch {
            FipsNative.available
            runCatching { ServerPreferences(this@ArchipelagoApp).launchState.first() }
        }

        // First WebView construction pays Chromium provider load (~150-400 ms
        // cold). Absorb it while the main thread is idle before the kiosk
        // needs it, instead of serially after the connection probe.
        Looper.getMainLooper().queue.addIdleHandler {
            runCatching { WebView(this).destroy() }
            false // one-shot
        }
    }
}
