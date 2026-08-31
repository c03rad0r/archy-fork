package com.archipelago.app.ui.components

import kotlinx.coroutines.flow.MutableStateFlow

/**
 * Cross-layer handoff for remote-signer pairing (#139): NavGraph's
 * `nostrconnect://` deep link drops the URI here and routes to the session;
 * WebViewScreen collects it, opens the hub menu, and NESMenu opens the
 * signer sub-page with the request. Cleared once the signer section has
 * consumed it (via NESMenu's onSignerPairHandled).
 */
object SignerLaunch {
    val pendingUri = MutableStateFlow<String?>(null)
}
