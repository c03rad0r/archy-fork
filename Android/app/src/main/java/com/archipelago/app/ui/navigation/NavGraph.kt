package com.archipelago.app.ui.navigation

import android.net.VpnService
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument
import com.archipelago.app.data.PairResult
import com.archipelago.app.data.ServerEntry
import com.archipelago.app.data.ServerPreferences
import com.archipelago.app.data.ServerQrParser
import com.archipelago.app.fips.FipsManager
import com.archipelago.app.ui.components.SignerLaunch
import com.archipelago.app.ui.screens.FlareScreen
import com.archipelago.app.ui.screens.IntroScreen
import com.archipelago.app.ui.screens.NodePickerScreen
import com.archipelago.app.ui.screens.PartyScreen
import com.archipelago.app.ui.screens.RemoteInputScreen
import com.archipelago.app.ui.screens.ServerConnectScreen
import com.archipelago.app.ui.screens.WebViewScreen
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

object Routes {
    const val INTRO = "intro"
    const val NODE_PICKER = "node_picker"
    const val SERVER_CONNECT = "server_connect"
    const val WEB_VIEW = "web_view"
    const val REMOTE_INPUT = "remote_input"
    const val MESH_PARTY = "mesh_party"
    const val FLARE = "flare"
}

/**
 * Process-scoped "have we already asked which node?" flag.
 *
 * The picker is a COLD-START question: opening the app fresh (or after the
 * mesh service and its process were killed) is exactly when the user may want
 * a different node than last time. An Activity recreation inside a live
 * process — rotation, theme change — must not re-ask, and neither must a
 * simple return from the background, so the flag lives with the process
 * rather than in saved state.
 */
private object LaunchGate {
    @Volatile
    var nodeChoiceMade: Boolean = false
}

@Composable
fun AppNavHost(
    pairUri: String? = null,
    onPairUriConsumed: () -> Unit = {},
    onReady: () -> Unit = {},
) {
    val context = LocalContext.current
    val prefs = remember { ServerPreferences(context) }
    val navController = rememberNavController()
    val scope = rememberCoroutineScope()

    // One combined emission — introSeen and activeServer resolving in separate
    // frames used to flash the Connect screen at paired users on launch.
    val launchState by prefs.launchState.collectAsState(initial = null)
    val introSeen = launchState?.introSeen
    val activeServer = launchState?.activeServer
    val savedServers = launchState?.savedServers ?: emptyList()

    // Pairing entry from a deep link that carried no password — prefills the
    // connect form so the user lands on the password prompt for that server.
    var pairPrefill by remember { mutableStateOf<ServerEntry?>(null) }

    // Mesh tunnel: Android's VPN consent dialog is the single unavoidable
    // interaction — it can only be launched from an Activity, so pairing
    // paths raise FipsManager.consentNeeded and it is handled here, once.
    val consentNeeded by FipsManager.consentNeeded.collectAsState()
    val vpnConsentLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.StartActivityForResult()
    ) { result ->
        FipsManager.consentHandled()
        if (result.resultCode == android.app.Activity.RESULT_OK) {
            FipsManager.startService(context)
        }
    }
    LaunchedEffect(consentNeeded) {
        if (!consentNeeded) return@LaunchedEffect
        val consentIntent = VpnService.prepare(context)
        if (consentIntent == null) {
            FipsManager.consentHandled()
            FipsManager.startService(context)
        } else {
            vpnConsentLauncher.launch(consentIntent)
        }
    }

    if (introSeen == null) return

    // Ask which node when the user keeps more than one and this is a cold
    // start. Anything else (single node, mid-process Activity recreation,
    // a pairing deep link) goes straight through as before.
    val needsNodeChoice = introSeen == true &&
        !LaunchGate.nodeChoiceMade &&
        savedServers.size > 1

    // Paired + previously consented → the mesh comes back silently on launch,
    // but ONLY once the session's node is known to be a FIPS node. Bringing
    // the tunnel up before that took Android's single VPN slot away from
    // whatever the user uses to reach a non-mesh node. Off the main
    // dispatcher: this path dlopens the 7 MB fips core and does a binder
    // round-trip (VpnService.prepare).
    LaunchedEffect(needsNodeChoice, activeServer?.npub, activeServer?.meshIp) {
        if (needsNodeChoice) return@LaunchedEffect
        if (activeServer?.isFipsNode() != true) return@LaunchedEffect
        withContext(Dispatchers.IO) { FipsManager.autoStartIfReady(context) }
    }

    // Launch state resolved — MainActivity holds the system splash until now,
    // so the first visible frame is the real UI, never a black gap.
    LaunchedEffect(Unit) { onReady() }

    // Declared after the introSeen gate so it can't fire before the NavHost
    // below has set the nav graph; pairUri stays pending until consumed here.
    LaunchedEffect(pairUri) {
        val raw = pairUri ?: return@LaunchedEffect
        onPairUriConsumed()
        when {
            // Remote-signer pairing deep link (NIP-46): nostrconnect://…
            // from the node's login QR — any QR scanner app can hand it over.
            // The signer UI lives inside the hub menu: drop the URI where
            // WebViewScreen picks it up and route to the session, which opens
            // the hub on its signer sub-page.
            raw.startsWith("nostrconnect://") -> {
                prefs.markIntroSeen()
                SignerLaunch.pendingUri.value = raw
                navController.navigate(Routes.WEB_VIEW) {
                    popUpTo(0) { inclusive = true }
                }
            }
            else -> when (val result = ServerQrParser.parse(raw)) {
                is PairResult.Success -> {
                    // Pairing implies the app is installed and in use — skip the intro.
                    prefs.markIntroSeen()
                    val merged = prefs.upsertServer(result.server)
                    FipsManager.registerNode(context, result.fips, merged.displayName())
                    if (merged.password.isNotBlank()) {
                        // Demo flow: password came with the link — connect in one step.
                        prefs.setActiveServer(merged)
                        navController.navigate(Routes.WEB_VIEW) {
                            popUpTo(0) { inclusive = true }
                        }
                    } else {
                        pairPrefill = merged
                        navController.navigate(Routes.SERVER_CONNECT) {
                            popUpTo(0) { inclusive = true }
                        }
                    }
                }
                else -> {
                    // Invalid or too-new pairing link — ignore; normal startup continues.
                }
            }
        }
    }

    val startDestination = when {
        introSeen == false -> Routes.INTRO
        needsNodeChoice -> Routes.NODE_PICKER
        activeServer != null -> Routes.WEB_VIEW
        else -> Routes.SERVER_CONNECT
    }

    NavHost(
        navController = navController,
        startDestination = startDestination,
    ) {
        composable(Routes.NODE_PICKER) {
            NodePickerScreen(
                servers = savedServers,
                lastActive = activeServer,
                onPick = { server ->
                    LaunchGate.nodeChoiceMade = true
                    scope.launch {
                        prefs.setActiveServer(server)
                        // The mesh follows the choice, and ONLY the choice.
                        // A non-mesh node gets the tunnel taken down: Android
                        // hands out one VPN slot, and holding it hostage is
                        // what broke reaching nodes behind a different VPN.
                        withContext(Dispatchers.IO) {
                            if (server.isFipsNode()) {
                                FipsManager.autoStartIfReady(context)
                            } else {
                                FipsManager.stopService(context)
                            }
                        }
                        navController.navigate(Routes.WEB_VIEW) {
                            popUpTo(0) { inclusive = true }
                        }
                    }
                },
                onAddNode = {
                    LaunchGate.nodeChoiceMade = true
                    navController.navigate(Routes.SERVER_CONNECT)
                },
            )
        }

        composable(Routes.INTRO) {
            IntroScreen(
                onMeshParty = {
                    navController.navigate(Routes.MESH_PARTY)
                },
                onContinue = {
                    scope.launch {
                        prefs.markIntroSeen()
                        navController.navigate(Routes.SERVER_CONNECT) {
                            popUpTo(Routes.INTRO) { inclusive = true }
                        }
                    }
                },
            )
        }

        composable(Routes.SERVER_CONNECT) {
            ServerConnectScreen(
                onConnected = { _ ->
                    navController.navigate(Routes.WEB_VIEW) {
                        popUpTo(Routes.SERVER_CONNECT) { inclusive = true }
                    }
                },
                initialServer = pairPrefill,
            )
        }

        composable(Routes.WEB_VIEW) {
            val server = activeServer
            if (server == null) {
                ServerConnectScreen(
                    onConnected = { _ ->
                        navController.navigate(Routes.WEB_VIEW) {
                            popUpTo(0) { inclusive = true }
                        }
                    },
                )
            } else {
                WebViewScreen(
                    serverUrl = server.toUrl(),
                    serverPassword = server.password,
                    meshFallbackUrl = server.toMeshUrl(),
                    onDisconnect = {
                        scope.launch {
                            prefs.clearActiveServer()
                            navController.navigate(Routes.SERVER_CONNECT) {
                                popUpTo(0) { inclusive = true }
                            }
                        }
                    },
                    onRemoteInput = {
                        navController.navigate(Routes.REMOTE_INPUT)
                    },
                    onRemoteKeyboard = {
                        navController.navigate("${Routes.REMOTE_INPUT}?keyboard=true")
                    },
                    onMeshParty = {
                        navController.navigate(Routes.MESH_PARTY)
                    },
                )
            }
        }

        composable(
            "${Routes.REMOTE_INPUT}?keyboard={keyboard}",
            arguments = listOf(
                navArgument("keyboard") {
                    type = NavType.BoolType
                    defaultValue = false
                },
            ),
        ) { entry ->
            RemoteInputScreen(
                onBack = {
                    navController.popBackStack()
                },
                onMeshParty = {
                    navController.navigate(Routes.MESH_PARTY)
                },
                startInKeyboard = entry.arguments?.getBoolean("keyboard") == true,
            )
        }

        composable(Routes.MESH_PARTY) {
            PartyScreen(
                onBack = { navController.popBackStack() },
                onOpenChat = { navController.navigate(Routes.FLARE) },
            )
        }

        composable(Routes.FLARE) {
            FlareScreen(
                onBack = { navController.popBackStack() },
            )
        }
    }
}
