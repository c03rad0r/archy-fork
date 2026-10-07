package com.archipelago.app

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import com.archipelago.app.ui.navigation.AppNavHost
import com.archipelago.app.ui.screens.releaseKioskWebView
import com.archipelago.app.ui.theme.ArchipelagoTheme
import kotlinx.coroutines.flow.MutableStateFlow

class MainActivity : ComponentActivity() {
    internal var cloudVideoPip: com.archipelago.app.ui.screens.CloudVideoPip? = null
    override fun onPictureInPictureModeChanged(active: Boolean, config: android.content.res.Configuration) {
        super.onPictureInPictureModeChanged(active, config)
        cloudVideoPip?.modeChanged(active)
    }
    override fun onStop() { cloudVideoPip?.stopped(); super.onStop() }


    // Pairing deep link (archipelago://pair?...) from the launch intent or a
    // later one (launchMode=singleTask). Consumed by AppNavHost.
    private val pendingPairUri = MutableStateFlow<String?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        // Hold the branded system splash until the nav graph has its launch
        // state — without this the splash dropped at the first composed frame,
        // which was EMPTY (the DataStore read hadn't landed): splash → black
        // flash → UI on every launch.
        var navReady = false
        val splash = installSplashScreen()
        splash.setKeepOnScreenCondition { !navReady }
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        pendingPairUri.value = intent?.dataString
        setContent {
            ArchipelagoTheme {
                val pairUri by pendingPairUri.collectAsState()
                AppNavHost(
                    pairUri = pairUri,
                    onPairUriConsumed = { pendingPairUri.value = null },
                    onReady = { navReady = true },
                )
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        pendingPairUri.value = intent.dataString
    }

    override fun onDestroy() {
        super.onDestroy()
        // Swiped out of recents (or otherwise finished) — let go of the
        // retained kiosk WebView so the next launch starts clean. Without
        // this the FIPS service keeps the process (and the static WebView)
        // alive, and "close the app" no longer restarted it. isFinishing
        // keeps config changes (rotation) on the fast reattach path.
        if (isFinishing) releaseKioskWebView()
    }
}
