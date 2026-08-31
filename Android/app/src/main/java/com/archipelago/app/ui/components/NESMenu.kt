package com.archipelago.app.ui.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Bolt
import androidx.compose.material.icons.filled.Dashboard
import androidx.compose.material.icons.filled.Dns
import androidx.compose.material.icons.filled.Groups
import androidx.compose.material.icons.filled.Keyboard
import androidx.compose.material.icons.filled.RestartAlt
import androidx.compose.material.icons.filled.SettingsBackupRestore
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.SportsEsports
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import com.archipelago.app.fips.FipsManager
import com.archipelago.app.fips.FipsNative
import com.archipelago.app.fips.FipsPreferences
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.archipelago.app.R
import com.archipelago.app.data.ServerEntry
import com.archipelago.app.ui.screens.restartCompanionApp
import com.archipelago.app.ui.theme.BitcoinOrange
import com.archipelago.app.ui.theme.SurfaceDark
import com.archipelago.app.ui.theme.TextMuted
import com.archipelago.app.ui.theme.TextPrimary

// Glassmorphism palette (OS design): near-black surfaces, subtle white borders,
// Bitcoin-orange accent.
private val PanelBg = SurfaceDark                       // #0A0A0A
private val PanelBorder = Color.White.copy(alpha = 0.12f)
private val RowBg = Color.White.copy(alpha = 0.05f)
private val RowBorder = Color.White.copy(alpha = 0.08f)
private val FieldBg = Color.White.copy(alpha = 0.04f)

private val PANEL_R = 20.dp
private val ROW_R = 14.dp
private val ROW_H = 54.dp
private val FIELD_H = 58.dp

/** Glassmorphism modal menu — #0A0A0A surface, subtle white borders. */
@Composable
fun NESMenu(
    visible: Boolean,
    servers: List<ServerEntry>,
    activeServer: ServerEntry?,
    onDismiss: () -> Unit,
    onSelectServer: (ServerEntry) -> Unit,
    onAddServer: (ServerEntry) -> Unit,
    onScanQr: (() -> Unit)? = null,
    onEditServer: (ServerEntry, ServerEntry) -> Unit,
    onRemoveServer: (ServerEntry) -> Unit,
    onRemote: () -> Unit,
    onKeyboard: () -> Unit,
    onBackToWebView: (() -> Unit)? = null,
    onMeshParty: (() -> Unit)? = null,
    // Remote-signer pairing request (nostrconnect://… deep link, or a scan):
    // non-null opens the hub on the signer sub-page and pairs. Consumed once
    // the signer section hands it back via [onSignerPairHandled].
    signerPairRequest: String? = null,
    onSignerPairHandled: () -> Unit = {},
) {
    // Pairing state is latched here (not passed straight through) so the
    // source can clear itself while the request stays alive until consumed.
    var pendingSignerPair by remember { mutableStateOf<String?>(null) }
    var signerScan by remember { mutableStateOf(false) }
    LaunchedEffect(signerPairRequest) {
        if (signerPairRequest != null) pendingSignerPair = signerPairRequest
    }

    AnimatedVisibility(visible = visible, enter = fadeIn(), exit = fadeOut()) {
        // Contained hub overlay: a centred glass panel (not full-screen) that
        // holds the card page and its sub-pages (Nodes, FIPS, Backup, Signer)
        // and scrolls inside its own bounds when content is tall. Tapping the
        // dimmed backdrop dismisses.
        Box(
            Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.7f))
                .clickable(indication = null, interactionSource = remember { MutableInteractionSource() }) { onDismiss() },
            contentAlignment = Alignment.Center,
        ) {
            AnimatedVisibility(visible = visible, enter = fadeIn() + scaleIn(initialScale = 0.95f), exit = fadeOut() + scaleOut(targetScale = 0.95f)) {
                MenuPanel(
                    servers, activeServer, onDismiss, onSelectServer, onAddServer, onScanQr,
                    onEditServer, onRemoveServer, onRemote, onKeyboard, onBackToWebView, onMeshParty,
                    signerPairUri = pendingSignerPair,
                    onSignerScan = { signerScan = true },
                    onSignerPairHandled = {
                        pendingSignerPair = null
                        onSignerPairHandled()
                    },
                )
            }
        }
    }

    // Pairing-QR scanner for the signer sub-page — a full-screen glass
    // modal hosted OUTSIDE the hub panel so it isn't clipped to the panel's
    // bounds (same layering the pairing scanner gets from WebViewScreen).
    QrGlassModal(
        visible = signerScan && visible,
        title = "Scan pairing QR",
        status = null,
        idleHint = "Point at the nostrconnect QR the node or client shows",
        permissionRationale = "Camera access is needed to scan the pairing code",
        onDismiss = { signerScan = false },
        onDecoded = { text ->
            if (text.startsWith("nostrconnect://")) {
                signerScan = false
                pendingSignerPair = text
            }
        },
    )
}

@Composable
private fun MenuPanel(
    servers: List<ServerEntry>,
    activeServer: ServerEntry?,
    onDismiss: () -> Unit,
    onSelectServer: (ServerEntry) -> Unit,
    onAddServer: (ServerEntry) -> Unit,
    onScanQr: (() -> Unit)?,
    onEditServer: (ServerEntry, ServerEntry) -> Unit,
    onRemoveServer: (ServerEntry) -> Unit,
    onRemote: () -> Unit,
    onKeyboard: () -> Unit,
    onBackToWebView: (() -> Unit)?,
    onMeshParty: (() -> Unit)?,
    signerPairUri: String?,
    onSignerScan: () -> Unit,
    onSignerPairHandled: () -> Unit,
) {
    var showAdd by remember { mutableStateOf(false) }
    // The saved server being edited, or null when adding a new one.
    var editing by remember { mutableStateOf<ServerEntry?>(null) }
    var nm by remember { mutableStateOf("") }
    var addr by remember { mutableStateOf("") }
    var pwd by remember { mutableStateOf("") }
    var https by remember { mutableStateOf(false) }

    fun resetForm() {
        nm = ""; addr = ""; pwd = ""; https = false; showAdd = false; editing = null
    }

    fun startEdit(server: ServerEntry) {
        editing = server
        nm = server.name; addr = server.address; pwd = server.password; https = server.useHttps
        showAdd = false
    }

    fun submit() {
        if (addr.isBlank()) return
        val orig = editing
        if (orig != null) {
            // Preserve port (compact form doesn't expose it); scheme is now editable.
            onEditServer(orig, orig.copy(address = addr, useHttps = https, password = pwd, name = nm))
        } else {
            onAddServer(ServerEntry(addr, https, password = pwd, name = nm))
        }
        resetForm()
    }

    var page by remember { mutableStateOf(HubPage.HUB) }

    Column(
        modifier = Modifier
            .widthIn(max = 420.dp)
            .fillMaxWidth()
            .padding(horizontal = 20.dp)
            // Cap height at 70% of the screen — a ~15% breathing margin top
            // and bottom — the panel wraps short content and scrolls inside
            // its own bounds when a sub-page outgrows this.
            .heightIn(max = (LocalConfiguration.current.screenHeightDp * 0.70f).dp)
            .clip(RoundedCornerShape(PANEL_R))
            .background(PanelBg.copy(alpha = 0.86f))
            .border(1.dp, PanelBorder, RoundedCornerShape(PANEL_R))
            .clickable(indication = null, interactionSource = remember { MutableInteractionSource() }) {}
            .verticalScroll(rememberScrollState())
            .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        // Header: back (on sub-pages) or title, and a close on the hub.
        Row(
            Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            if (page == HubPage.HUB) {
                Text("Menu", color = TextPrimary, fontSize = 20.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 2.sp)
                IconRound(Icons.Default.Close, "Close") { onDismiss() }
            } else {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    IconRound(Icons.AutoMirrored.Filled.ArrowBack, "Back") { resetForm(); page = HubPage.HUB }
                    Spacer(Modifier.width(12.dp))
                    Text(
                        when (page) {
                            HubPage.NODES -> "Nodes"
                            HubPage.FIPS -> "FIPS Mesh"
                            HubPage.BACKUP -> "Backup & Restore"
                            HubPage.SIGNER -> "Remote Signer"
                            HubPage.HUB -> "Menu"
                        },
                        color = TextPrimary, fontSize = 20.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 1.sp,
                    )
                }
                IconRound(Icons.Default.Close, "Close") { onDismiss() }
            }
        }
        Spacer(Modifier.height(4.dp))

        when (page) {
            HubPage.HUB -> {
                // Card page — one card per destination. Dashboard first: it's a
                // peer of the others so three-finger → hub → Dashboard returns
                // to the node UI, same shape as every other option.
                if (onBackToWebView != null) {
                    HubCard(Icons.Default.Dashboard, "Dashboard", "The node's web interface") { onBackToWebView() }
                }
                HubCard(Icons.Default.SportsEsports, "Remote", "Game controller for the node") { onRemote() }
                HubCard(Icons.Default.Keyboard, "Keyboard", "Type into the node") { onKeyboard() }
                HubCard(Icons.Default.Dns, "Nodes", activeServer?.displayName() ?: "Add or switch servers") {
                    page = HubPage.NODES
                }
                // Mesh oversight only when this session is actually on the
                // mesh. Offering "FIPS Mesh" while connected to a plain node
                // (whose traffic is going nowhere near the tunnel) advertises
                // a connection the user doesn't have.
                if (FipsNative.available && activeServer?.isFipsNode() == true) {
                    HubCard(Icons.Default.Bolt, "FIPS Mesh", "Mesh identity & status") { page = HubPage.FIPS }
                }
                if (onMeshParty != null) {
                    HubCard(Icons.Default.Groups, "Mesh Party", "Phone-to-phone chat & beam") { onMeshParty() }
                }
                // Backup & Restore (#128): the phone side of losing your phone
                // or wiping it to cross a border — encrypted export file, no cloud.
                HubCard(Icons.Default.SettingsBackupRestore, "Backup & Restore", "Encrypted export for a wiped phone") { page = HubPage.BACKUP }
                // Remote Signer (#139): hold a nostr key on the phone and
                // approve/deny remote signature requests (NIP-46).
                HubCard(Icons.Default.Key, "Remote Signer", "Approve signatures for your node") { page = HubPage.SIGNER }
                // Dark/Classic style lives on the remote/keyboard screen next to
                // the settings button — not here.

                // Small version chip at the hub's foot — the one place a
                // connected user can always check what build they're on.
                val hubContext = LocalContext.current

                // Restart: the dashboard WebView is retained across
                // remote ⇄ dashboard (that's the point), which also means a
                // wedged page can't be cleared by leaving the screen. This
                // throws the page away and relaunches the app clean — the mesh
                // service keeps running.
                HubCard(Icons.Default.RestartAlt, "Restart", "Reload the app from scratch") {
                    onDismiss()
                    restartCompanionApp(hubContext)
                }
                val versionLabel = remember {
                    runCatching {
                        hubContext.packageManager
                            .getPackageInfo(hubContext.packageName, 0).versionName
                    }.getOrNull()?.let { "Companion v$it" } ?: ""
                }
                if (versionLabel.isNotEmpty()) {
                    Text(
                        versionLabel,
                        color = TextMuted.copy(alpha = 0.6f),
                        fontSize = 11.sp,
                        letterSpacing = 1.sp,
                        textAlign = TextAlign.Center,
                        modifier = Modifier.fillMaxWidth().padding(top = 6.dp),
                    )
                }
            }

            HubPage.NODES -> {
                servers.forEach { server ->
                    val active = server.serialize() == activeServer?.serialize()
                    MenuItem(
                        label = server.displayName(),
                        // FIPS nodes carry their mesh ULA — the address Termux
                        // (or any other app) can reach over the split-tunnel,
                        // from anywhere. Tap to copy; the node's npub stays
                        // visible in the FIPS Mesh page.
                        subtitle = server.meshIp.takeIf { it.isNotBlank() },
                        selected = active,
                        onClick = { onSelectServer(server) },
                        onEdit = { startEdit(server) },
                        onRemove = { onRemoveServer(server) },
                    )
                }
                if (servers.isEmpty()) {
                    Text("No servers", color = TextMuted, fontSize = 14.sp, modifier = Modifier.padding(vertical = 4.dp))
                }

                if (showAdd || editing != null) {
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(ROW_R))
                            .background(FieldBg)
                            .border(1.dp, RowBorder, RoundedCornerShape(ROW_R))
                            .padding(12.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Row(
                            Modifier.fillMaxWidth(),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.SpaceBetween,
                        ) {
                            Text(
                                if (editing != null) "Edit Server" else "Add Server",
                                color = TextMuted, fontSize = 13.sp, letterSpacing = 1.sp, fontWeight = FontWeight.Medium,
                            )
                            Text(
                                "Cancel", color = TextMuted, fontSize = 13.sp,
                                modifier = Modifier.clickable { resetForm() }.padding(start = 8.dp),
                            )
                        }
                        GlassField(
                            value = nm, onValueChange = { nm = it },
                            placeholder = "Name (optional)",
                            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Text, imeAction = ImeAction.Next),
                        )
                        GlassField(
                            value = addr, onValueChange = { addr = it.trim() },
                            placeholder = "192.168.1.100",
                            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Next),
                        )
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                            GlassField(
                                value = pwd, onValueChange = { pwd = it },
                                placeholder = "Password",
                                modifier = Modifier.weight(1f),
                                visualTransformation = PasswordVisualTransformation(),
                                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Go),
                                keyboardActions = KeyboardActions(onGo = { submit() }),
                            )
                            Box(
                                Modifier.size(FIELD_H).clip(RoundedCornerShape(12.dp)).background(BitcoinOrange.copy(alpha = 0.15f))
                                    .border(1.dp, BitcoinOrange.copy(alpha = 0.4f), RoundedCornerShape(12.dp))
                                    .clickable { submit() },
                                contentAlignment = Alignment.Center,
                            ) { Text("OK", color = BitcoinOrange, fontSize = 14.sp, fontWeight = FontWeight.Bold) }
                        }
                        // HTTPS scheme toggle (available on both add and edit).
                        Row(
                            Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(ROW_R))
                                .clickable { https = !https }
                                .padding(vertical = 2.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.SpaceBetween,
                        ) {
                            Text("Use HTTPS", color = TextMuted, fontSize = 13.sp)
                            Box(
                                Modifier
                                    .width(46.dp).height(26.dp)
                                    .clip(RoundedCornerShape(13.dp))
                                    .background(if (https) BitcoinOrange.copy(alpha = 0.9f) else RowBg)
                                    .border(1.dp, if (https) BitcoinOrange else RowBorder, RoundedCornerShape(13.dp)),
                                contentAlignment = if (https) Alignment.CenterEnd else Alignment.CenterStart,
                            ) {
                                Box(
                                    Modifier
                                        .padding(horizontal = 3.dp)
                                        .size(20.dp)
                                        .clip(RoundedCornerShape(10.dp))
                                        .background(if (https) Color.White else TextMuted),
                                )
                            }
                        }
                    }
                } else {
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Box(Modifier.weight(1f)) {
                            MenuItem(label = "Add Server", labelColor = BitcoinOrange, onClick = { showAdd = true })
                        }
                        if (onScanQr != null) {
                            Box(
                                Modifier
                                    .size(ROW_H)
                                    .clip(RoundedCornerShape(ROW_R))
                                    .background(RowBg)
                                    .border(1.dp, RowBorder, RoundedCornerShape(ROW_R))
                                    .clickable { onScanQr() },
                                contentAlignment = Alignment.Center,
                            ) {
                                Icon(
                                    Icons.Default.QrCodeScanner,
                                    contentDescription = stringResource(R.string.add_server_qr),
                                    tint = BitcoinOrange,
                                    modifier = Modifier.size(24.dp),
                                )
                            }
                        }
                    }
                }
            }

            HubPage.FIPS -> {
                FipsSection(embedded = true)
            }

            HubPage.BACKUP -> {
                BackupSection()
            }

            HubPage.SIGNER -> {
                SignerSection(
                    pairUri = signerPairUri,
                    onScan = onSignerScan,
                    onPairHandled = onSignerPairHandled,
                )
            }
        }
    }
}

private enum class HubPage { HUB, NODES, FIPS, BACKUP, SIGNER }

/** Big tappable destination card for the hub page: icon + title + subtitle. */
@Composable
private fun HubCard(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    title: String,
    subtitle: String,
    onClick: () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ROW_R))
            .background(RowBg)
            .border(1.dp, RowBorder, RoundedCornerShape(ROW_R))
            .clickable { onClick() }
            .padding(16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Box(
            Modifier.size(40.dp).clip(RoundedCornerShape(12.dp)).background(BitcoinOrange.copy(alpha = 0.14f)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(icon, contentDescription = title, tint = BitcoinOrange, modifier = Modifier.size(22.dp))
        }
        Column(Modifier.weight(1f)) {
            Text(title, color = TextPrimary, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
            Text(subtitle, color = TextMuted, fontSize = 12.sp, maxLines = 1)
        }
    }
}

/** Small circular icon button used in the hub header. */
@Composable
private fun IconRound(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    desc: String,
    onClick: () -> Unit,
) {
    Box(
        Modifier
            .size(40.dp)
            .clip(RoundedCornerShape(20.dp))
            .background(RowBg)
            .border(1.dp, RowBorder, RoundedCornerShape(20.dp))
            .clickable { onClick() },
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = desc, tint = TextPrimary, modifier = Modifier.size(20.dp))
    }
}

/** Snapshot of the phone's mesh identity + state for the FIPS menu section. */
private data class FipsInfo(
    val available: Boolean,
    val running: Boolean,
    val npub: String,
    val meshAddress: String,
    val peerCount: Int,
)

/**
 * FIPS mesh oversight: shows what the phone's embedded mesh node is doing —
 * running state, its mesh identity (npub), its mesh address (fd…ULA), how
 * many peers/anchors it's configured with — and a one-tap Reconnect that
 * re-homes the mesh (also the manual fix if a network handoff ever misses).
 * Collapsed by default so the menu stays compact.
 */
@Composable
private fun FipsSection(embedded: Boolean = false) {
    if (!FipsNative.available) return
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    var expanded by remember { mutableStateOf(embedded) }
    var info by remember { mutableStateOf<FipsInfo?>(null) }

    // Load identity/state when the section opens (cheap DataStore + JSON read).
    LaunchedEffect(expanded) {
        if (expanded && info == null) {
            val prefs = FipsPreferences(context)
            val id = prefs.identity()
            val peers = runCatching {
                org.json.JSONArray(prefs.peersJson()).length()
            }.getOrDefault(0)
            info = FipsInfo(
                available = true,
                running = FipsNative.isRunning(),
                npub = id?.npub.orEmpty(),
                meshAddress = id?.address.orEmpty(),
                peerCount = peers,
            )
        }
    }

    Column(Modifier.fillMaxWidth()) {
        // Embedded in the hub's FIPS sub-page the header row would duplicate
        // the page title, so only the standalone (collapsible) form shows it.
        if (!embedded) MenuItem(
            label = "FIPS Mesh",
            labelColor = BitcoinOrange,
            onClick = { expanded = !expanded },
        )
        if (expanded) {
            val i = info
            Column(
                Modifier
                    .fillMaxWidth()
                    .padding(top = 6.dp)
                    .clip(RoundedCornerShape(ROW_R))
                    .background(FieldBg)
                    .border(1.dp, RowBorder, RoundedCornerShape(ROW_R))
                    .padding(14.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                if (i == null) {
                    Text("Loading…", color = TextMuted, fontSize = 13.sp)
                } else {
                    FipsRow("Status", if (i.running) "Connected" else "Stopped",
                        valueColor = if (i.running) BitcoinOrange else TextMuted)
                    if (i.meshAddress.isNotBlank()) {
                        FipsRow("Mesh address", i.meshAddress, mono = true,
                            onCopy = { clipboard.setText(AnnotatedString(i.meshAddress)) })
                    }
                    if (i.npub.isNotBlank()) {
                        FipsRow("Identity (npub)", i.npub, mono = true,
                            onCopy = { clipboard.setText(AnnotatedString(i.npub)) })
                    }
                    FipsRow("Peers & anchors", i.peerCount.toString())
                    Text(
                        "Your node reaches this phone over the mesh by its npub — no ports opened to the internet.",
                        color = TextMuted, fontSize = 11.sp,
                    )
                    MenuItem(
                        label = "Reconnect mesh",
                        labelColor = BitcoinOrange,
                        onClick = {
                            FipsManager.requestMeshRestart(context)
                            info = null
                            if (!embedded) expanded = false
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun FipsRow(
    label: String,
    value: String,
    valueColor: Color = TextPrimary,
    mono: Boolean = false,
    onCopy: (() -> Unit)? = null,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .then(if (onCopy != null) Modifier.clickable { onCopy() } else Modifier),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(label, color = TextMuted, fontSize = 12.sp, modifier = Modifier.width(120.dp))
        Text(
            value,
            color = valueColor,
            fontSize = if (mono) 11.sp else 13.sp,
            fontWeight = FontWeight.Medium,
            modifier = Modifier.weight(1f),
            textAlign = TextAlign.End,
        )
        if (onCopy != null) {
            Text("⧉", color = TextMuted, fontSize = 13.sp, modifier = Modifier.padding(start = 8.dp))
        }
    }
}

@Composable
private fun MenuItem(
    label: String,
    selected: Boolean = false,
    labelColor: Color = TextPrimary,
    onClick: () -> Unit,
    onEdit: (() -> Unit)? = null,
    onRemove: (() -> Unit)? = null,
    /** Optional second line (the node's mesh ULA); tapping it copies. */
    subtitle: String? = null,
) {
    val clipboard = LocalClipboardManager.current
    Row(
        Modifier
            .fillMaxWidth()
            // Rows with a second line grow to fit it.
            .then(if (subtitle == null) Modifier.height(ROW_H) else Modifier.heightIn(min = ROW_H))
            .clip(RoundedCornerShape(ROW_R))
            .background(if (selected) BitcoinOrange.copy(alpha = 0.12f) else RowBg)
            .border(1.dp, if (selected) BitcoinOrange.copy(alpha = 0.4f) else RowBorder, RoundedCornerShape(ROW_R))
            .clickable { onClick() }
            .padding(horizontal = 16.dp)
            .then(if (subtitle == null) Modifier else Modifier.padding(vertical = 8.dp)),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                label,
                color = if (selected) BitcoinOrange else labelColor,
                fontSize = 16.sp,
                fontWeight = FontWeight.Medium,
            )
            if (subtitle != null) {
                Row(
                    Modifier
                        .padding(top = 2.dp)
                        .clip(RoundedCornerShape(6.dp))
                        .clickable { clipboard.setText(AnnotatedString(subtitle)) }
                        .padding(horizontal = 4.dp, vertical = 2.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        subtitle,
                        color = TextMuted,
                        fontSize = 10.sp,
                        fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace,
                        maxLines = 1,
                        overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
                    )
                    Text("⧉", color = TextMuted.copy(alpha = 0.7f), fontSize = 11.sp, modifier = Modifier.padding(start = 6.dp))
                }
            }
        }
        if (onEdit != null) {
            Text(
                "✎",
                color = TextMuted,
                fontSize = 16.sp,
                modifier = Modifier.clickable { onEdit() }.padding(horizontal = 8.dp),
            )
        }
        if (onRemove != null) {
            Text(
                "✕",
                color = TextMuted,
                fontSize = 16.sp,
                modifier = Modifier.clickable { onRemove() }.padding(horizontal = 8.dp),
            )
        }
    }
}

/** Glass text field with centered input text. */
@Composable
internal fun GlassField(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    modifier: Modifier = Modifier,
    visualTransformation: androidx.compose.ui.text.input.VisualTransformation = androidx.compose.ui.text.input.VisualTransformation.None,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        placeholder = {
            Text(placeholder, color = TextMuted, fontSize = 15.sp, modifier = Modifier.fillMaxWidth(), textAlign = TextAlign.Center)
        },
        modifier = modifier.fillMaxWidth().height(FIELD_H),
        singleLine = true,
        visualTransformation = visualTransformation,
        keyboardOptions = keyboardOptions,
        keyboardActions = keyboardActions,
        textStyle = TextStyle(color = TextPrimary, fontSize = 16.sp, textAlign = TextAlign.Center),
        colors = OutlinedTextFieldDefaults.colors(
            focusedBorderColor = Color.White.copy(alpha = 0.3f),
            unfocusedBorderColor = Color.White.copy(alpha = 0.12f),
            cursorColor = BitcoinOrange,
            focusedTextColor = TextPrimary,
            unfocusedTextColor = TextPrimary,
        ),
        shape = RoundedCornerShape(12.dp),
    )
}
