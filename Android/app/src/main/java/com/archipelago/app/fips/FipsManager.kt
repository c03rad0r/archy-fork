package com.archipelago.app.fips

import android.content.Context
import android.content.Intent
import android.net.VpnService
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.withContext

/**
 * Glue between pairing and the mesh: persists the node peer from a scanned
 * QR and asks the UI to bring the tunnel up. There is deliberately no
 * settings surface — scanning a node's QR is the entire configuration.
 *
 * The one unavoidable interaction is Android's VPN consent dialog
 * (VpnService.prepare), which only an Activity can launch; [consentNeeded]
 * signals AppNavHost to run it, once, on first pairing.
 */
object FipsManager {

    /** Set when a pairing registered mesh info and the VPN needs starting. */
    private val _consentNeeded = MutableStateFlow(false)
    val consentNeeded: StateFlow<Boolean> = _consentNeeded

    /** True after a pairing changed the peer set while the node was running —
     *  tells the service a restart is genuinely needed (the ONLY case; a
     *  routine app open must keep the warm mesh, not rebuild it). */
    @Volatile
    var peersDirty: Boolean = false

    fun consentHandled() {
        _consentNeeded.value = false
    }

    /**
     * Persist mesh info from a pairing scan and request tunnel start.
     * No-op on devices without the native lib (non-arm64).
     */
    suspend fun registerNode(context: Context, info: FipsPairInfo?, alias: String) {
        if (info == null) return
        // Every caller reaches this from a Compose scope — i.e. the MAIN
        // thread — the instant a pairing QR decodes. Everything below is
        // main-hostile: touching FipsNative dlopens the 7 MB mesh core,
        // ensureIdentity runs native ed25519 keygen, and VpnService.prepare
        // is a binder round-trip. Left on the UI thread it froze the frame
        // right after the camera got the code, which reads as "the scanner
        // is slow" when the scan itself already succeeded.
        val consent = withContext(Dispatchers.IO) {
            if (!FipsNative.available) return@withContext null
            val prefs = FipsPreferences(context)
            ensureIdentity(prefs)
            prefs.upsertNodePeer(info, alias)
            peersDirty = true
            // Restart the mesh with the new peer RIGHT NOW when consent already
            // exists — relying on the consentNeeded collector left a running
            // mesh on the OLD peer list whenever the collector wasn't active
            // (fresh pairings looked dead until a full app restart).
            VpnService.prepare(context) == null
        } ?: return
        if (consent) startService(context) else _consentNeeded.value = true
    }

    /** Generate-once mesh identity. Returns null only if the RNG/native fails. */
    suspend fun ensureIdentity(prefs: FipsPreferences): FipsNative.Identity? {
        prefs.identity()?.let { return it }
        val generated = FipsNative.parseIdentity(FipsNative.generateIdentity()) ?: return null
        prefs.saveIdentity(generated)
        return generated
    }

    /**
     * Start the mesh service if this device is paired and the user has already
     * consented to the VPN (prepare() == null). Called on app start so the
     * tunnel comes back without any interaction; first-time consent goes
     * through AppNavHost instead.
     */
    suspend fun autoStartIfReady(context: Context) {
        // Self-dispatching for the same reason as registerNode: callers reach
        // this from Compose scopes, and dlopen + binder must not ride the UI
        // thread (the connect path calls it while the scanner is still up).
        val ready = withContext(Dispatchers.IO) {
            if (!FipsNative.available) return@withContext false
            val prefs = FipsPreferences(context)
            if (prefs.identity() == null || !prefs.hasPeers()) return@withContext false
            // consent missing — don't prompt here
            VpnService.prepare(context) == null
        }
        if (ready) startService(context)
    }

    fun startService(context: Context) {
        val intent = Intent(context, ArchyVpnService::class.java)
        context.startForegroundService(intent)
    }

    /**
     * Re-run the mesh with current prefs (party listen toggled, peer added).
     * Marks the peer set dirty so startMesh genuinely restarts the node —
     * otherwise the keep-warm fast path would skip the new config.
     * First-timers go through the consent flow.
     */
    fun requestMeshRestart(context: Context) {
        if (!FipsNative.available) return
        peersDirty = true
        if (VpnService.prepare(context) == null) {
            startService(context)
        } else {
            _consentNeeded.value = true
        }
    }

    fun stopService(context: Context) {
        val intent = Intent(context, ArchyVpnService::class.java)
            .setAction(ArchyVpnService.ACTION_STOP)
        context.startService(intent)
    }
}
