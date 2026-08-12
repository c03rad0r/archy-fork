package com.archipelago.app.fips

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.VpnService
import android.os.Build
import android.util.Log
import com.archipelago.app.MainActivity
import com.archipelago.app.R
import com.archipelago.app.data.ServerPreferences
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * VpnService hosting the embedded FIPS mesh node.
 *
 * Split tunnel: only fd00::/8 (the FIPS ULA space) routes into the TUN, so
 * normal phone traffic is untouched — this is mesh reachability, not a
 * default-route VPN. The established fd is detached and handed to the Rust
 * node (Node::start_with_tun_fd); the node owns it until stop.
 */
class ArchyVpnService : VpnService() {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var warmerJob: Job? = null
    private var handoffKickJob: Job? = null

    // Seamless transport handoff (Wi-Fi ⇄ 5G ⇄ future BLE). Without this the
    // tunnel's underlying network stays pinned to the interface that was
    // default when the VPN came up; when the phone leaves Wi-Fi for 5G the
    // mesh sockets ride a dead network and sessions never recover until the
    // app is restarted (user-reported 2026-07-27). The callback (a) re-pins
    // the tunnel to the new default network via setUnderlyingNetworks and
    // (b) forces an immediate mesh re-home so discovery + sessions rebuild
    // on the new path within seconds instead of waiting out dead-link
    // timeouts.
    private var connectivityManager: ConnectivityManager? = null
    private var networkCallback: ConnectivityManager.NetworkCallback? = null
    @Volatile
    private var currentUnderlying: Network? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            shutdown()
            return START_NOT_STICKY
        }
        startForeground(NOTIFICATION_ID, buildNotification())
        scope.launch { startMesh() }
        return START_STICKY
    }

    private suspend fun startMesh() {
        if (!FipsNative.available) {
            shutdown()
            return
        }
        val prefs = FipsPreferences(this)
        val identity = FipsManager.ensureIdentity(prefs)
        val peersJson = prefs.combinedPeersJson()
        if (identity == null || peersJson == "[]") {
            Log.w(TAG, "mesh not configured — stopping")
            shutdown()
            return
        }
        // Party mode: fixed inbound UDP bind so a nearby phone can dial us
        // directly over a shared LAN/hotspot (no internet required).
        val listenPort = if (prefs.partyListen()) PartyQr.PARTY_UDP_PORT else 0

        // A fresh app open re-triggers the service. Tearing a HEALTHY mesh
        // down to rebuild it costs ~8s of anchor+session bring-up on every
        // launch (observed live: stop 00:34:38 → session back 00:34:49) and
        // is what made "freshly loading the app" slow. Keep a running node;
        // restart only when it's dead or a pairing changed the peer set.
        if (FipsNative.isRunning() && !FipsManager.peersDirty) {
            Log.i(TAG, "mesh already running — keeping warm sessions")
            startSessionWarmer()
            return
        }
        FipsManager.peersDirty = false
        // Re-establishing while running would strand the old fd; restart clean.
        if (FipsNative.isRunning()) FipsNative.stop()

        val pfd = try {
            Builder()
                .setSession("Archipelago Mesh")
                .setMtu(1280)
                .addAddress(identity.address, 128)
                .addRoute("fd00::", 8)
                // The TUN is IPv6-only. Android blocks every address family
                // the VPN has no address for — without this, bringing the
                // mesh up cut ALL of the phone's IPv4 internet.
                .allowFamily(android.system.OsConstants.AF_INET)
                // And let apps that bind their own network skip the TUN
                // entirely — this is mesh reachability, not a privacy VPN.
                .allowBypass()
                .apply {
                    // Android 10+ treats VPN networks as METERED by default,
                    // which flips the whole phone into data-saver behaviour
                    // (background sync off, "metered" warnings) while the
                    // mesh is up. It inherits the underlying network's real
                    // metered state instead.
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) setMetered(false)
                }
                .establish()
        } catch (e: Exception) {
            Log.e(TAG, "VPN establish failed", e)
            null
        }
        if (pfd == null) {
            shutdown()
            return
        }

        val fd = pfd.detachFd()
        val result = FipsNative.start(identity.secret, peersJson, fd, listenPort)
        Log.i(TAG, "mesh start: $result (listen=$listenPort)")
        if (result.contains("\"error\"")) {
            shutdown()
        } else {
            startSessionWarmer()
            registerNetworkHandoff()
            // Phone-to-phone chat/beam + the phone's own mesh-served page.
            FlareServer.start(this, identity.address, identity.npub, prefs.partyName())
            // Mutual pairing: when a phone that scanned OUR QR announces
            // itself, store it as a party peer and re-run the mesh config so
            // this side gets the peer + chat entry without scanning back.
            FlareServer.onHello = { peer ->
                scope.launch {
                    prefs.upsertPartyPeer(peer)
                    FipsManager.requestMeshRestart(this@ArchyVpnService)
                }
            }
        }
    }

    /**
     * Pre-warm + keep-warm mesh sessions to every known node ULA.
     *
     * Discovery + first session through the public tree can take 15s+
     * (from node diagnosis) — paying that cost here, the
     * moment the tunnel is up, means the connect probe and WebView hit an
     * established session instead of timing out on a cold one. The periodic
     * touch afterwards keeps the session from idling out. Failed connects
     * are expected and cheap; the attempt itself is what drives discovery.
     */
    private fun startSessionWarmer() {
        warmerJob?.cancel()
        warmerJob = scope.launch {
            val prefs = ServerPreferences(this@ArchyVpnService)
            val fipsPrefs = FipsPreferences(this@ArchyVpnService)
            var round = 0
            while (isActive && FipsNative.isRunning()) {
                val targets = try {
                    prefs.savedServers.first()
                        .mapNotNull { it.meshIp.ifBlank { null } }
                        .map { it to 80 } +
                        // Party phones answer on the flare port, not :80.
                        fipsPrefs.partyPeers().map { it.ula to PartyQr.FLARE_PORT }
                } catch (_: Exception) {
                    emptyList()
                }.distinct()
                if (round == 0) Log.i(TAG, "session warmer: ${targets.map { it.first }}")
                // Probe all targets CONCURRENTLY with a short timeout — the
                // old sequential 20s-per-target loop let one cold node starve
                // every other target for the whole aggressive window.
                targets.map { (ula, port) ->
                    launch {
                        try {
                            java.net.Socket().use { s ->
                                s.connect(
                                    java.net.InetSocketAddress(
                                        java.net.InetAddress.getByName(ula),
                                        port,
                                    ),
                                    5_000,
                                )
                            }
                        } catch (_: Exception) {
                            // Cold path / node away — the attempt still drove
                            // session establishment; try again next round.
                        }
                    }
                }.forEach { it.join() }
                round++
                // Aggressive for the first ~minute (session bring-up), then a
                // slow keep-warm tick that costs nearly nothing.
                delay(if (round < 12) 5_000 else 60_000)
            }
        }
    }

    /**
     * Track the phone's default network and hand the mesh over to it as the
     * phone roams (Wi-Fi ⇄ 5G). Two actions per change:
     *  1. setUnderlyingNetworks(new) — the tunnel's packets follow the live
     *     network instead of dying on the one it launched with.
     *  2. re-home the mesh — kick the session warmer so discovery + sessions
     *     rebuild on the new path; the node's own fast-reconnect (1s) redials
     *     peers over the new route.
     * onAvailable also fires for the FIRST network, which is how the initial
     * underlying network gets set.
     *
     * requestNetwork, NOT registerDefaultNetworkCallback: this app is routed
     * through its own TUN, so its "default network" IS the VPN — a default
     * callback fires once with our own tunnel and never again on Wi-Fi ⇄ 5G.
     * A NetworkRequest's default capabilities include NOT_VPN, so requestNetwork
     * tracks the best real transport underneath instead.
     */
    private fun registerNetworkHandoff() {
        if (networkCallback != null) return
        val cm = getSystemService(ConnectivityManager::class.java) ?: return
        connectivityManager = cm
        val request = NetworkRequest.Builder()
            .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            .build()
        val cb = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                handoffTo(network)
            }

            override fun onLost(network: Network) {
                // The lost network was our underlying one — clear the pin so
                // the system falls back to whatever default remains; the next
                // onAvailable re-pins explicitly.
                if (network == currentUnderlying) {
                    currentUnderlying = null
                    runCatching { setUnderlyingNetworks(null) }
                }
            }
        }
        networkCallback = cb
        runCatching { cm.requestNetwork(request, cb) }
    }

    private fun handoffTo(network: Network) {
        val changed = network != currentUnderlying
        currentUnderlying = network
        // Always re-assert; cheap and covers capability changes on the same
        // Network object.
        runCatching { setUnderlyingNetworks(arrayOf(network)) }
        if (changed && FipsNative.isRunning()) {
            Log.i(TAG, "network handoff → re-homing mesh on new default network")
            // Coalesced, not immediate: marginal Wi-Fi flaps the default
            // Wi-Fi ⇄ cell in bursts, and an aggressive warmer pass per flip
            // meant near-constant session churn — the "reconnects a lot"
            // report. The re-pin above still happens on every change; only
            // the rediscovery kick waits for the network to hold still.
            handoffKickJob?.cancel()
            handoffKickJob = scope.launch {
                delay(2_000)
                if (FipsNative.isRunning()) startSessionWarmer()
            }
        }
    }

    private fun unregisterNetworkHandoff() {
        handoffKickJob?.cancel()
        handoffKickJob = null
        val cm = connectivityManager
        val cb = networkCallback
        if (cm != null && cb != null) {
            runCatching { cm.unregisterNetworkCallback(cb) }
        }
        networkCallback = null
        connectivityManager = null
        currentUnderlying = null
    }

    private fun shutdown() {
        warmerJob?.cancel()
        unregisterNetworkHandoff()
        FlareServer.stop()
        FipsNative.stop()
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    override fun onDestroy() {
        unregisterNetworkHandoff()
        FipsNative.stop()
        scope.cancel()
        super.onDestroy()
    }

    override fun onRevoke() {
        // User pulled VPN permission from system settings.
        shutdown()
    }

    private fun buildNotification(): Notification {
        val manager = getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            manager.createNotificationChannel(
                NotificationChannel(
                    CHANNEL_ID,
                    "Mesh connection",
                    NotificationManager.IMPORTANCE_MIN,
                ).apply { description = "Keeps the node reachable from anywhere" }
            )
        }
        val tapIntent = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE,
        )
        return Notification.Builder(this, CHANNEL_ID)
            .setContentTitle("Connected to your Archipelago")
            .setContentText("Secure mesh link active")
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentIntent(tapIntent)
            .setOngoing(true)
            .build()
    }

    companion object {
        const val ACTION_STOP = "com.archipelago.app.fips.STOP"
        private const val CHANNEL_ID = "archy_mesh"
        private const val NOTIFICATION_ID = 4841
        private const val TAG = "ArchyVpnService"
    }
}
