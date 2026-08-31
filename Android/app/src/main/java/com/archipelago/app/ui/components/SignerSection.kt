package com.archipelago.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material3.Icon
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.archipelago.app.NativeCore
import com.archipelago.app.nostr.BunkerManager
import com.archipelago.app.nostr.NostrSignerPreferences
import com.archipelago.app.ui.theme.BitcoinOrange
import com.archipelago.app.ui.theme.SuccessGreen
import com.archipelago.app.ui.theme.TextMuted
import com.archipelago.app.ui.theme.TextPrimary
import kotlinx.coroutines.launch
import org.json.JSONObject
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Remote Signer (#139) — the hub's SIGNER sub-page (same container as
 * Nodes/FIPS). The phone holds a nostr key; a NIP-46 client (the node's
 * login QR, any nostrconnect:// app) pairs via [pairUri] or the scanner
 * (hosted by NESMenu outside this panel), and every `sign_event` request
 * lands as a legible approve/deny card. See
 * docs/companion-nip46-remote-signer.md.
 */
@Composable
internal fun SignerSection(
    pairUri: String?,
    onScan: () -> Unit,
    onPairHandled: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboardManager.current
    val prefs = remember { NostrSignerPreferences(context) }

    var keyInfo by remember { mutableStateOf<JSONObject?>(null) }
    var keyError by remember { mutableStateOf<String?>(null) }
    var importText by remember { mutableStateOf("") }
    var showNsec by remember { mutableStateOf(false) }
    var notice by remember { mutableStateOf<String?>(null) }
    var noticeError by remember { mutableStateOf(false) }

    val bunkerState by BunkerManager.state.collectAsState()
    val pending by BunkerManager.pending.collectAsState()

    fun say(msg: String, error: Boolean) {
        notice = msg
        noticeError = error
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

    // Consume a pairing request (deep link or scanner) exactly once.
    LaunchedEffect(pairUri) {
        val uri = pairUri?.takeIf { it.isNotBlank() } ?: return@LaunchedEffect
        if (keyInfo == null) loadKey()
        val err = BunkerManager.pair(context, uri)
        if (err != null) say(err, true) else say("Pairing started…", false)
        onPairHandled()
    }

    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        SectionCopy(
            "Hold a nostr key on this phone and sign for it remotely — pair with your " +
                "node's login QR (or any NIP-46 client), then approve each signature " +
                "request as it arrives. Nothing signs without you."
        )

        if (bunkerState is BunkerManager.SignerState.Unavailable) {
            Text(
                "Signing is unavailable on this device (native core missing).",
                color = Color(0xFFFF6B6B), fontSize = 12.sp,
            )
        }

        val info = keyInfo
        if (info == null) {
            // ── No key yet: generate or import ──────────────────────────
            keyError?.let { Text(it, color = Color(0xFFFF6B6B), fontSize = 11.sp) }
            WideAction(text = "Generate signer key", onClick = {
                scope.launch {
                    try {
                        keyInfo = prefs.generateSecret()
                        keyError = null
                        BunkerManager.refreshState(context)
                    } catch (e: Exception) {
                        keyError = e.message ?: "could not generate a key"
                    }
                }
            })
            GlassField(
                value = importText,
                onValueChange = { importText = it },
                placeholder = "or import nsec…",
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onGo = {
                    if (importText.isNotBlank()) {
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
                    }
                }),
            )
            WideAction(text = "Import", onClick = {
                if (importText.isBlank()) return@WideAction
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
            })
        } else {
            // ── Identity ─────────────────────────────────────────────────
            SectionHeader(Icons.Default.Key, "Signer identity")
            MonoValue("npub", info.optString("npub")) {
                clipboard.setText(AnnotatedString(info.optString("npub")))
            }
            if (showNsec) {
                MonoValue("nsec", info.optString("nsec"), secret = true) {
                    clipboard.setText(AnnotatedString(info.optString("nsec")))
                }
                SectionHint("Anyone with the nsec can sign as you — clear the clipboard after copying.")
            } else {
                Text(
                    "Show nsec",
                    color = TextMuted, fontSize = 11.sp,
                    modifier = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .clickable { showNsec = true }
                        .padding(vertical = 2.dp, horizontal = 6.dp),
                )
            }

            // ── Session ──────────────────────────────────────────────────
            Spacer(Modifier.height(2.dp))
            val label = when (val s = bunkerState) {
                BunkerManager.SignerState.Unavailable -> "Unavailable on this device"
                BunkerManager.SignerState.NoKey -> "No signer key yet"
                BunkerManager.SignerState.Idle -> "Idle — pair to start"
                is BunkerManager.SignerState.Connecting -> "Connecting to ${s.relay}…"
                is BunkerManager.SignerState.AwaitingClient -> "Paired with \"${s.clientName}\" — waiting for the handshake to finish"
                is BunkerManager.SignerState.Ready -> "Ready for \"${s.clientName}\""
                is BunkerManager.SignerState.Failed -> s.reason
            }
            Text("Session", color = TextMuted, fontSize = 11.sp)
            Text(
                label,
                color = if (bunkerState is BunkerManager.SignerState.Failed) Color(0xFFFF6B6B)
                else if (bunkerState is BunkerManager.SignerState.Ready) SuccessGreen
                else TextPrimary,
                fontSize = 13.sp,
                lineHeight = 17.sp,
            )
            WideAction(
                text = "Scan pairing QR",
                onClick = {
                    if (bunkerState is BunkerManager.SignerState.NoKey) {
                        say("Generate or import a signer key first.", true)
                        return@WideAction
                    }
                    onScan()
                },
                icon = Icons.Default.QrCodeScanner,
            )
            if (bunkerState is BunkerManager.SignerState.Ready ||
                bunkerState is BunkerManager.SignerState.AwaitingClient ||
                bunkerState is BunkerManager.SignerState.Connecting
            ) {
                Text(
                    "End session",
                    color = TextMuted, fontSize = 11.sp,
                    modifier = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .clickable { BunkerManager.unpair() }
                        .padding(vertical = 2.dp, horizontal = 6.dp),
                )
            }

            // ── Pending signature request — the whole point ──────────────
            pending?.let { req ->
                Spacer(Modifier.height(2.dp))
                Column(
                    Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(14.dp))
                        .background(Color.White.copy(alpha = 0.04f))
                        .border(1.dp, BitcoinOrange.copy(alpha = 0.35f), RoundedCornerShape(14.dp))
                        .padding(12.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Text("Signature request", color = BitcoinOrange, fontSize = 13.sp, fontWeight = FontWeight.Bold)
                    SummaryRow("Client", req.clientName.ifBlank { req.clientPubkey.take(12) + "…" })
                    SummaryRow("Kind", kindLabel(req.kind))
                    req.createdAt?.let {
                        SummaryRow("Time", SimpleDateFormat("HH:mm:ss", Locale.US).format(Date(it * 1000)))
                    }
                    req.content?.takeIf { it.isNotBlank() }?.let { content ->
                        Text(
                            content,
                            color = TextPrimary, fontSize = 10.sp, lineHeight = 14.sp,
                            fontFamily = FontFamily.Monospace,
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(10.dp))
                                .background(Color.Black.copy(alpha = 0.45f))
                                .padding(8.dp)
                                .heightIn(max = 160.dp),
                        )
                    }
                    if (req.tags.isNotEmpty()) {
                        Column(
                            Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(10.dp))
                                .background(Color.Black.copy(alpha = 0.45f))
                                .padding(8.dp),
                            verticalArrangement = Arrangement.spacedBy(2.dp),
                        ) {
                            req.tags.take(6).forEach {
                                Text(
                                    it,
                                    color = TextMuted, fontSize = 9.sp,
                                    fontFamily = FontFamily.Monospace,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                            if (req.tags.size > 6) {
                                Text("+${req.tags.size - 6} more", color = TextMuted, fontSize = 9.sp)
                            }
                        }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Box(
                            Modifier
                                .weight(1f)
                                .height(40.dp)
                                .clip(RoundedCornerShape(12.dp))
                                .background(Color(0xFFE5484D).copy(alpha = 0.16f))
                                .border(1.dp, Color(0xFFE5484D).copy(alpha = 0.5f), RoundedCornerShape(12.dp))
                                .clickable { BunkerManager.deny() },
                            contentAlignment = Alignment.Center,
                        ) { Text("Deny", color = Color(0xFFFF8A8D), fontSize = 13.sp, fontWeight = FontWeight.Bold) }
                        Box(
                            Modifier
                                .weight(1f)
                                .height(40.dp)
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
                        ) { Text("Approve", color = BitcoinOrange, fontSize = 13.sp, fontWeight = FontWeight.Bold) }
                    }
                }
            }
        }

        notice?.takeIf { it.isNotBlank() }?.let { msg ->
            Text(
                msg,
                color = if (noticeError) Color(0xFFFF6B6B) else SuccessGreen,
                fontSize = 12.sp,
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

/** Kind number → legible label, so the approve/deny card reads like a sentence. */
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

/** Monospace value chip with a copy affordance (tap the row). */
@Composable
private fun MonoValue(label: String, value: String, secret: Boolean = false, onCopy: () -> Unit) {
    Column(Modifier.fillMaxWidth()) {
        Text(label, color = TextMuted, fontSize = 10.sp)
        Row(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(10.dp))
                .background(Color.Black.copy(alpha = 0.45f))
                .clickable { onCopy() }
                .padding(horizontal = 10.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                value,
                color = if (secret) Color(0xFFFFB86B) else TextPrimary,
                fontSize = 10.sp,
                fontFamily = FontFamily.Monospace,
                modifier = Modifier.weight(1f),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text("⧉", color = TextMuted, fontSize = 13.sp, modifier = Modifier.padding(start = 8.dp))
        }
    }
}
