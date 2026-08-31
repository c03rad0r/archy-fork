package com.archipelago.app.ui.screens

import android.content.Intent
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
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
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.archipelago.app.NativeCore
import com.archipelago.app.R
import com.archipelago.app.nostr.BunkerManager
import com.archipelago.app.nostr.NostrSignerPreferences
import com.archipelago.app.ui.components.QrGlassModal
import com.archipelago.app.ui.theme.BitcoinOrange
import com.archipelago.app.ui.theme.SuccessGreen
import com.archipelago.app.ui.theme.SurfaceBlack
import com.archipelago.app.ui.theme.TextMuted
import com.archipelago.app.ui.theme.TextPrimary
import kotlinx.coroutines.launch
import org.json.JSONObject
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Remote Signer (#139) — the phone side of NIP-46.
 *
 * The phone holds a nostr key; the node's login page (or any NIP-46 client)
 * shows a `nostrconnect://` QR, this screen scans it, and every signature
 * request lands here as a legible approve/deny card — kind, content, tags —
 * before anything is signed. Nothing signs without a thumb on Approve.
 */
@Composable
fun SignerScreen(
    onBack: () -> Unit,
    initialPairUri: String? = null,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboardManager.current
    val prefs = remember { NostrSignerPreferences(context) }

    var keyInfo by remember { mutableStateOf<JSONObject?>(null) }
    var keyError by remember { mutableStateOf<String?>(null) }
    var importText by remember { mutableStateOf("") }
    var showNsec by remember { mutableStateOf(false) }
    var scanVisible by remember { mutableStateOf(false) }
    var status by remember { mutableStateOf<String?>(null) }
    var statusError by remember { mutableStateOf(false) }

    val bunkerState by BunkerManager.state.collectAsState()
    val pending by BunkerManager.pending.collectAsState()

    fun say(msg: String, error: Boolean) {
        status = msg
        statusError = error
    }

    suspend fun loadKey() {
        val secret = prefs.secret()
        keyInfo = secret?.let {
            val json = NativeCore.nostrSecretFromAny(it)
            if (NativeCore.isErr(json)) null else JSONObject(json)
        }
    }

    LaunchedEffect(Unit) {
        BunkerManager.refreshState(context)
        loadKey()
    }

    // Deep link (nostrconnect://…) may arrive with the route.
    LaunchedEffect(initialPairUri) {
        val uri = initialPairUri?.takeIf { it.isNotBlank() } ?: return@LaunchedEffect
        if (keyInfo == null) loadKey()
        val err = BunkerManager.pair(context, uri)
        if (err != null) say(err, true) else say("Pairing started…", false)
    }

    fun pairWith(uri: String) {
        scope.launch {
            val err = BunkerManager.pair(context, uri)
            if (err != null) say(err, true)
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(SurfaceBlack),
    ) {
        Image(
            painter = painterResource(id = R.drawable.bg_synthwave),
            contentDescription = null,
            modifier = Modifier.fillMaxSize(),
            contentScale = ContentScale.Crop,
        )
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(
                    Brush.verticalGradient(
                        colors = listOf(
                            Color.Black.copy(alpha = 0.65f),
                            Color.Black.copy(alpha = 0.5f),
                            Color.Black.copy(alpha = 0.85f),
                        ),
                    )
                ),
        )
        Column(
            modifier = Modifier
                .fillMaxSize()
                .windowInsetsPadding(WindowInsets.safeDrawing)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp)
                .padding(top = 16.dp, bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                IconButton(onClick = onBack) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back", tint = TextPrimary)
                }
                Text(
                    "Remote Signer",
                    style = androidx.compose.material3.MaterialTheme.typography.headlineSmall,
                    color = TextPrimary,
                )
            }
            Text(
                "Hold a nostr key on this phone and sign for it remotely — scan a pairing QR from your node's login page (or any NIP-46 client), then approve each signature request as it arrives. Nothing signs without you.",
                style = androidx.compose.material3.MaterialTheme.typography.bodyMedium,
                color = TextMuted,
            )

            if (bunkerState is BunkerManager.SignerState.Unavailable) {
                GlassCard {
                    Text(
                        "Signing is unavailable on this device (native core missing).",
                        color = Color(0xFFFF6B6B), fontSize = 13.sp,
                    )
                }
            }

            val info = keyInfo
            if (info == null) {
                // ── No key yet ─────────────────────────────────────────────
                GlassCard {
                    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        Text("Create your signer key", color = TextPrimary, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
                        Text(
                            "Generate a fresh key here, or import an existing nsec. The key never leaves this phone except inside an encrypted backup.",
                            color = TextMuted, fontSize = 11.sp,
                        )
                        keyError?.let { Text(it, color = Color(0xFFFF6B6B), fontSize = 12.sp) }
                        GlassButton(
                            text = "Generate key",
                            onClick = {
                                scope.launch {
                                    try {
                                        keyInfo = prefs.generateSecret()
                                        keyError = null
                                        BunkerManager.refreshState(context)
                                    } catch (e: Exception) {
                                        keyError = e.message ?: "could not generate a key"
                                    }
                                }
                            },
                            modifier = Modifier.fillMaxWidth().height(48.dp),
                        )
                        OutlinedTextField(
                            value = importText,
                            onValueChange = { importText = it },
                            placeholder = { Text("or import nsec…", color = TextMuted, fontSize = 14.sp) },
                            modifier = Modifier.fillMaxWidth(),
                            singleLine = true,
                            textStyle = TextStyle(color = TextPrimary, fontSize = 13.sp),
                            colors = OutlinedTextFieldDefaults.colors(
                                focusedBorderColor = BitcoinOrange,
                                unfocusedBorderColor = Color.White.copy(alpha = 0.15f),
                                cursorColor = BitcoinOrange,
                                focusedTextColor = TextPrimary,
                                unfocusedTextColor = TextPrimary,
                            ),
                            shape = RoundedCornerShape(12.dp),
                        )
                        GlassButton(
                            text = "Import",
                            onClick = {
                                if (importText.isBlank()) return@GlassButton
                                scope.launch {
                                    try {
                                        keyInfo = prefs.importSecret(importText)
                                        importText = ""
                                        keyError = null
                                        BunkerManager.refreshState(context)
                                    } catch (e: Exception) {
                                        keyError = e.message ?: "not a valid nsec"
                                    }
                                }
                            },
                            modifier = Modifier.fillMaxWidth().height(48.dp),
                        )
                    }
                }
            } else {
                // ── Key present: identity + session + pairing ──────────────
                GlassCard {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(10.dp),
                        ) {
                            Icon(Icons.Default.Key, contentDescription = null, tint = BitcoinOrange, modifier = Modifier.size(18.dp))
                            Text("Signer identity", color = TextPrimary, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                        }
                        MonoRow("npub", info.optString("npub")) {
                            clipboard.setText(AnnotatedString(info.optString("npub")))
                        }
                        if (showNsec) {
                            MonoRow("nsec", info.optString("nsec"), secret = true) {
                                clipboard.setText(AnnotatedString(info.optString("nsec")))
                            }
                            Text(
                                "Anyone with the nsec can sign as you — clear the clipboard after copying.",
                                color = Color(0xFFFF6B6B), fontSize = 10.sp,
                            )
                        } else {
                            Text(
                                "Show nsec",
                                color = TextMuted, fontSize = 12.sp,
                                modifier = Modifier
                                    .clip(RoundedCornerShape(8.dp))
                                    .clickable { showNsec = true }
                                    .padding(vertical = 4.dp, horizontal = 8.dp),
                            )
                        }
                    }
                }

                // Session status
                GlassCard {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        val label = when (val s = bunkerState) {
                            BunkerManager.SignerState.Unavailable -> "Unavailable on this device"
                            BunkerManager.SignerState.NoKey -> "No signer key yet"
                            BunkerManager.SignerState.Idle -> "Idle — scan a pairing QR to start"
                            is BunkerManager.SignerState.Connecting -> "Connecting to ${s.relay}…"
                            is BunkerManager.SignerState.AwaitingClient -> "Paired with \"${s.clientName}\" — waiting for it to finish the handshake"
                            is BunkerManager.SignerState.Ready -> "Ready for \"${s.clientName}\" via ${s.relay}"
                            is BunkerManager.SignerState.Failed -> s.reason
                        }
                        Text("Session", color = TextMuted, fontSize = 12.sp)
                        Text(
                            label,
                            color = if (bunkerState is BunkerManager.SignerState.Failed) Color(0xFFFF6B6B)
                            else if (bunkerState is BunkerManager.SignerState.Ready) SuccessGreen
                            else TextPrimary,
                            fontSize = 14.sp,
                        )
                        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            Box(
                                Modifier
                                    .weight(1f)
                                    .height(44.dp)
                                    .clip(RoundedCornerShape(12.dp))
                                    .background(Color.White.copy(alpha = 0.06f))
                                    .border(1.dp, Color.White.copy(alpha = 0.12f), RoundedCornerShape(12.dp))
                                    .clickable { scanVisible = true },
                                contentAlignment = Alignment.Center,
                            ) {
                                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                    Icon(Icons.Default.QrCodeScanner, contentDescription = null, tint = BitcoinOrange, modifier = Modifier.size(18.dp))
                                    Text("Scan pairing QR", color = BitcoinOrange, fontSize = 13.sp, fontWeight = FontWeight.Medium)
                                }
                            }
                        }
                        if (bunkerState is BunkerManager.SignerState.Ready ||
                            bunkerState is BunkerManager.SignerState.AwaitingClient ||
                            bunkerState is BunkerManager.SignerState.Connecting
                        ) {
                            Text(
                                "End session",
                                color = TextMuted, fontSize = 12.sp,
                                modifier = Modifier
                                    .clip(RoundedCornerShape(8.dp))
                                    .clickable { BunkerManager.unpair() }
                                    .padding(vertical = 4.dp, horizontal = 8.dp),
                            )
                        }
                    }
                }

                // Pending signature request — the whole point.
                pending?.let { req ->
                    GlassCard {
                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text(
                                "Signature request",
                                color = BitcoinOrange, fontSize = 15.sp, fontWeight = FontWeight.Bold,
                            )
                            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                                Text("Client", color = TextMuted, fontSize = 12.sp)
                                Text(req.clientName.ifBlank { req.clientPubkey.take(12) + "…" }, color = TextPrimary, fontSize = 12.sp)
                            }
                            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                                Text("Kind", color = TextMuted, fontSize = 12.sp)
                                Text(kindLabel(req.kind), color = TextPrimary, fontSize = 12.sp, fontWeight = FontWeight.Medium)
                            }
                            req.createdAt?.let {
                                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                                    Text("Time", color = TextMuted, fontSize = 12.sp)
                                    Text(
                                        SimpleDateFormat("HH:mm:ss", Locale.US).format(Date(it * 1000)),
                                        color = TextPrimary, fontSize = 12.sp,
                                    )
                                }
                            }
                            req.content?.takeIf { it.isNotBlank() }?.let { content ->
                                Text("Content", color = TextMuted, fontSize = 12.sp)
                                Text(
                                    content,
                                    color = TextPrimary,
                                    fontSize = 11.sp,
                                    lineHeight = 15.sp,
                                    fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace,
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .clip(RoundedCornerShape(10.dp))
                                        .background(Color.Black.copy(alpha = 0.4f))
                                        .padding(10.dp)
                                        .heightIn(max = 220.dp),
                                )
                            }
                            if (req.tags.isNotEmpty()) {
                                Text("Tags", color = TextMuted, fontSize = 12.sp)
                                Column(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .clip(RoundedCornerShape(10.dp))
                                        .background(Color.Black.copy(alpha = 0.4f))
                                        .padding(10.dp),
                                    verticalArrangement = Arrangement.spacedBy(3.dp),
                                ) {
                                    req.tags.take(6).forEach {
                                        Text(
                                            it,
                                            color = TextMuted, fontSize = 10.sp,
                                            fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace,
                                            maxLines = 1,
                                        )
                                    }
                                    if (req.tags.size > 6) {
                                        Text("+${req.tags.size - 6} more", color = TextMuted, fontSize = 10.sp)
                                    }
                                }
                            }
                            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                                Box(
                                    Modifier
                                        .weight(1f)
                                        .height(46.dp)
                                        .clip(RoundedCornerShape(12.dp))
                                        .background(Color(0xFFE5484D).copy(alpha = 0.18f))
                                        .border(1.dp, Color(0xFFE5484D).copy(alpha = 0.5f), RoundedCornerShape(12.dp))
                                        .clickable { BunkerManager.deny() },
                                    contentAlignment = Alignment.Center,
                                ) { Text("Deny", color = Color(0xFFFF8A8D), fontSize = 14.sp, fontWeight = FontWeight.Bold) }
                                Box(
                                    Modifier
                                        .weight(1f)
                                        .height(46.dp)
                                        .clip(RoundedCornerShape(12.dp))
                                        .background(BitcoinOrange.copy(alpha = 0.2f))
                                        .border(1.dp, BitcoinOrange.copy(alpha = 0.6f), RoundedCornerShape(12.dp))
                                        .clickable {
                                            scope.launch {
                                                val ok = BunkerManager.approve()
                                                say(if (ok) "Signed and sent." else "Could not send the signature.", !ok)
                                            }
                                        },
                                    contentAlignment = Alignment.Center,
                                ) { Text("Approve", color = BitcoinOrange, fontSize = 14.sp, fontWeight = FontWeight.Bold) }
                            }
                        }
                    }
                }
            }

            status?.takeIf { it.isNotBlank() }?.let { msg ->
                Text(
                    msg,
                    color = if (statusError) Color(0xFFFF6B6B) else SuccessGreen,
                    fontSize = 13.sp,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }

        // Pairing-QR scan — reuses the pairing scanner's glass modal, decodes
        // any text and treats nostrconnect:// (or a raw bunker:// error) here.
        QrGlassModal(
            visible = scanVisible,
            title = "Scan pairing QR",
            status = null,
            idleHint = "Point at the nostrconnect QR your node or client shows",
            permissionRationale = "Camera access is needed to scan the pairing code",
            onDismiss = { scanVisible = false },
            onDecoded = { text ->
                if (text.startsWith("nostrconnect://")) {
                    scanVisible = false
                    pairWith(text)
                }
                // Anything else keeps scanning with the hint showing.
            },
        )
    }
}

/** Kind number → legible label, so approve/deny cards read like sentences. */
private fun kindLabel(kind: Long?): String = when (kind) {
    0L -> "Metadata (kind 0)"
    1L -> "Text note (kind 1)"
    3L -> "Contact list (kind 3)"
    4L -> "Direct message (kind 4)"
    7L -> "Reaction (kind 7)"
    14L -> "Chat message (kind 14)"
    22242L -> "Client authentication (kind 22242)"
    30078L -> "App-stored data (kind 30078)"
    null -> "Unknown kind"
    else -> "Kind $kind"
}

/** Label + monospace value + copy affordance. */
@Composable
private fun MonoRow(label: String, value: String, secret: Boolean = false, onCopy: () -> Unit) {
    Column(Modifier.fillMaxWidth()) {
        Text(label, color = TextMuted, fontSize = 11.sp)
        Row(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(10.dp))
                .background(Color.Black.copy(alpha = 0.4f))
                .clickable { onCopy() }
                .padding(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                value,
                color = if (secret) Color(0xFFFFB86B) else TextPrimary,
                fontSize = 11.sp,
                fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace,
                modifier = Modifier.weight(1f),
                maxLines = 1,
                overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
            )
            Text("⧉", color = TextMuted, fontSize = 14.sp, modifier = Modifier.padding(start = 8.dp))
        }
    }
}
