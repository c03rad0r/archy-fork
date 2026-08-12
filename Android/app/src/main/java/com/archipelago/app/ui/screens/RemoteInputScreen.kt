package com.archipelago.app.ui.screens

import android.content.res.Configuration
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import com.archipelago.app.R
import com.archipelago.app.data.ServerPreferences
import com.archipelago.app.fips.FipsManager
import com.archipelago.app.network.ConnectionState
import com.archipelago.app.network.InputWebSocket
import com.archipelago.app.ui.components.NESController
import com.archipelago.app.ui.components.NESKeyboard
import com.archipelago.app.ui.components.NESMenu
import com.archipelago.app.ui.components.NESPortraitController
import com.archipelago.app.ui.components.QrScannerOverlay
import com.archipelago.app.ui.components.Trackpad
import com.archipelago.app.ui.theme.BitcoinOrange
import com.archipelago.app.ui.theme.ControllerStyle
import com.archipelago.app.ui.theme.ErrorRed
import com.archipelago.app.ui.theme.SuccessGreen
import com.archipelago.app.ui.theme.TextMuted
import kotlinx.coroutines.launch

@Composable
fun RemoteInputScreen(
    onBack: () -> Unit,
    onMeshParty: (() -> Unit)? = null,
    // Land on the keyboard instead of the gamepad (hub menu's Keyboard card).
    startInKeyboard: Boolean = false,
) {
    val context = LocalContext.current
    val prefs = remember { ServerPreferences(context) }
    val scope = rememberCoroutineScope()
    val isLandscape = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE

    val savedServers by prefs.savedServers.collectAsState(initial = emptyList())
    val activeServer by prefs.activeServer.collectAsState(initial = null)

    var isGamepadMode by remember { mutableStateOf(!startInKeyboard) }
    var showModal by remember { mutableStateOf(false) }
    var showQrScanner by remember { mutableStateOf(false) }
    var controllerStyle by remember { mutableStateOf(ControllerStyle.DARK) }
    var playerId by remember { mutableStateOf(0) }  // 0 = broadcast, 1 = P1, 2 = P2

    val ws = remember { InputWebSocket(scope) }

    // When the kiosk forwards an "open in external browser" app, launch it in
    // the phone's default browser.
    DisposableEffect(ws) {
        ws.onExternalOpen = { url ->
            try {
                val intent = android.content.Intent(
                    android.content.Intent.ACTION_VIEW,
                    android.net.Uri.parse(url),
                ).apply { addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK) }
                context.startActivity(intent)
            } catch (_: Exception) {}
        }
        onDispose { ws.onExternalOpen = null }
    }

    fun togglePlayer() {
        playerId = when (playerId) { 0 -> 1; 1 -> 2; else -> 0 }
        ws.playerId = playerId
    }
    fun toggleStyle() {
        controllerStyle = if (controllerStyle == ControllerStyle.CLASSIC) ControllerStyle.DARK else ControllerStyle.CLASSIC
    }
    val connectionState by ws.state.collectAsState()
    val lifecycleOwner = LocalLifecycleOwner.current

    BackHandler { onBack() }

    // Connect on server change + reconnect when app resumes from background
    DisposableEffect(lifecycleOwner, activeServer) {
        val server = activeServer
        if (server != null) {
            ws.connect(server.toUrl(), server.password)
        }

        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_RESUME && server != null) {
                val state = ws.state.value
                if (state != ConnectionState.CONNECTED && state != ConnectionState.CONNECTING) {
                    ws.connect(server.toUrl(), server.password)
                }
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)

        onDispose {
            lifecycleOwner.lifecycle.removeObserver(observer)
            ws.disconnect()
        }
    }

    Box(
        Modifier
            .fillMaxSize()
            .background(Color(0xFF0C0C0C)),
    ) {
        // Reddish synthwave backdrop behind the controller
        Image(
            painter = painterResource(id = R.drawable.bg_synthwave),
            contentDescription = null,
            modifier = Modifier.fillMaxSize(),
            contentScale = ContentScale.Crop,
        )
        // Light scrim — the controller body provides its own contrast, so keep
        // this subtle and let the backdrop show through around it.
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(
                    Brush.verticalGradient(
                        colors = listOf(
                            Color.Black.copy(alpha = 0.4f),
                            Color.Black.copy(alpha = 0.25f),
                            Color.Black.copy(alpha = 0.45f),
                        ),
                    )
                ),
        )
        Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
        when {
            isGamepadMode && isLandscape -> NESController(
                style = controllerStyle,
                playerId = playerId,
                onKey = { ws.sendKey(it) },
                onMenu = { showModal = true },
                onPlayerToggle = ::togglePlayer,
                onToggleStyle = ::toggleStyle,
            )
            isGamepadMode && !isLandscape -> NESPortraitController(
                style = controllerStyle,
                playerId = playerId,
                onKey = { ws.sendKey(it) },
                onMouseMove = { dx, dy -> ws.sendMouseMove(dx, dy) },
                onMouseClick = { ws.sendClick(it) },
                onMouseScroll = { ws.sendScroll(it) },
                onMenu = { showModal = true },
                onPlayerToggle = ::togglePlayer,
                onToggleStyle = ::toggleStyle,
            )
            else -> {
                // Keyboard mode: trackpad fills top, keyboard pinned bottom
                Box(Modifier.fillMaxSize()) {
                    Column(Modifier.fillMaxSize()) {
                        Trackpad(
                            onMove = { dx, dy -> ws.sendMouseMove(dx, dy) },
                            onClick = { ws.sendClick(it) },
                            onScroll = { ws.sendScroll(it) },
                            onThreeFingerHold = { showModal = true },
                            modifier = Modifier.fillMaxWidth().weight(1f)
                                .padding(horizontal = 16.dp, vertical = 8.dp),
                        )
                        NESKeyboard(
                            style = controllerStyle,
                            onKey = { ws.sendKey(it) },
                            modifier = Modifier.fillMaxWidth(),
                        )
                    }
                    // Settings + style icons top-right in keyboard mode
                    Row(
                        Modifier.align(Alignment.TopEnd).padding(8.dp),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        com.archipelago.app.ui.components.SettingsBtn(
                            c = com.archipelago.app.ui.components.paletteFor(controllerStyle),
                            onClick = { showModal = true },
                        )
                        com.archipelago.app.ui.components.StyleBtn(
                            c = com.archipelago.app.ui.components.paletteFor(controllerStyle),
                            onClick = ::toggleStyle,
                        )
                    }
                }
            }
        }

        // Connection dot
        Box(
            Modifier.align(Alignment.TopStart).padding(6.dp).size(8.dp)
                .clip(CircleShape).background(
                    when (connectionState) {
                        ConnectionState.CONNECTED -> SuccessGreen
                        ConnectionState.CONNECTING -> BitcoinOrange
                        ConnectionState.ERROR, ConnectionState.AUTH_FAILED -> ErrorRed
                        ConnectionState.DISCONNECTED -> TextMuted
                    }
                ),
        )
        }

        NESMenu(
            visible = showModal,
            servers = savedServers,
            activeServer = activeServer,
            onDismiss = { showModal = false },
            onSelectServer = { server ->
                scope.launch { ws.disconnect(); prefs.setActiveServer(server) }; showModal = false
            },
            onAddServer = { server ->
                scope.launch { prefs.addSavedServer(server); if (activeServer == null) prefs.setActiveServer(server) }
            },
            onScanQr = { showQrScanner = true },
            onEditServer = { original, updated ->
                scope.launch {
                    prefs.updateSavedServer(original, updated)
                    // If the edited server is the live one, reconnect with the new
                    // address/credentials so the change takes effect immediately.
                    if (original.serialize() == activeServer?.serialize()) {
                        ws.disconnect()
                        prefs.setActiveServer(updated)
                    }
                }
            },
            onRemoveServer = { server ->
                scope.launch {
                    prefs.removeSavedServer(server)
                    // Deleting the last server leaves nothing to control — drop the
                    // active server and return to the Connect screen.
                    val remaining = savedServers.count { it.serialize() != server.serialize() }
                    if (remaining == 0) {
                        ws.disconnect()
                        prefs.clearActiveServer()
                        showModal = false
                        onBack()
                    }
                }
            },
            onRemote = { isGamepadMode = true; showModal = false },
            onKeyboard = { isGamepadMode = false; showModal = false },
            onBackToWebView = { showModal = false; onBack() },
            onMeshParty = onMeshParty?.let { open -> { showModal = false; open() } },
        )

        // Pairing-QR scan launched from the menu's Add Server row. The menu stays
        // open behind the scanner so the new entry appears as soon as it closes.
        QrScannerOverlay(
            visible = showQrScanner,
            onDismiss = { showQrScanner = false },
            onServerScanned = { scan ->
                showQrScanner = false
                scope.launch {
                    val merged = prefs.upsertServer(scan.server)
                    FipsManager.registerNode(context, scan.fips, merged.displayName())
                    if (activeServer == null) prefs.setActiveServer(merged)
                }
            },
        )
    }
}
