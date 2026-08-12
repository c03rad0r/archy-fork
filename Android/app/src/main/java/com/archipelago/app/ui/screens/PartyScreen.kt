package com.archipelago.app.ui.screens

import android.graphics.Bitmap
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
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
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.archipelago.app.fips.FipsManager
import com.archipelago.app.fips.FipsNative
import com.archipelago.app.fips.FipsPreferences
import com.archipelago.app.fips.FlareClient
import com.archipelago.app.fips.PartyPeer
import com.archipelago.app.fips.PartyQr
import com.archipelago.app.ui.components.CameraQrPreview
import com.archipelago.app.ui.theme.BitcoinOrange
import com.archipelago.app.ui.theme.SurfaceDark
import com.archipelago.app.ui.theme.TextMuted
import com.archipelago.app.ui.theme.TextPrimary
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private val CardBg = Color.White.copy(alpha = 0.05f)
private val CardBorder = Color.White.copy(alpha = 0.08f)

/** Public companion download (vps2 demo host — serves the same APK as the
 *  nodes' QR link). Rendered as the "Share this app" QR. */
private const val APP_DOWNLOAD_URL =
    "http://146.59.87.168:2100/packages/archipelago-companion.apk"

/**
 * Mesh Party — phone↔phone FIPS pairing. Show your QR, scan theirs, and the
 * two embedded mesh nodes link up: through anchors when there's internet,
 * directly over any shared WiFi/hotspot when there isn't.
 */
@Composable
fun PartyScreen(
    onBack: () -> Unit,
    onOpenChat: () -> Unit,
) {
    val context = LocalContext.current
    val prefs = remember { FipsPreferences(context) }
    val scope = rememberCoroutineScope()

    var identity by remember { mutableStateOf<FipsNative.Identity?>(null) }
    var name by remember { mutableStateOf("") }
    var localIp by remember { mutableStateOf<String?>(null) }
    var showScanner by remember { mutableStateOf(false) }
    var showShareQr by remember { mutableStateOf(false) }
    var scanHint by remember { mutableStateOf<String?>(null) }

    // Camera permission — fresh installs (and reinstalls: uninstall wipes
    // grants) land here with no CAMERA grant, and the raw preview just showed
    // black. Ask the moment the scanner opens.
    var hasCamera by remember {
        mutableStateOf(
            androidx.core.content.ContextCompat.checkSelfPermission(
                context, android.Manifest.permission.CAMERA,
            ) == android.content.pm.PackageManager.PERMISSION_GRANTED
        )
    }
    val cameraPermLauncher = androidx.activity.compose.rememberLauncherForActivityResult(
        androidx.activity.result.contract.ActivityResultContracts.RequestPermission()
    ) { hasCamera = it }
    LaunchedEffect(showScanner) {
        if (showScanner && !hasCamera) {
            cameraPermLauncher.launch(android.Manifest.permission.CAMERA)
        }
    }
    val listenOn by prefs.partyListenFlow.collectAsState(initial = false)
    val peers by prefs.partyPeersFlow.collectAsState(initial = emptyList())

    LaunchedEffect(Unit) {
        identity = FipsManager.ensureIdentity(prefs)
        name = prefs.partyName()
        // The hotspot/WiFi address can change while this screen is open
        // (e.g. the user flips the hotspot on mid-demo) — keep it fresh.
        // Tight only at first (the hotspot-flip window); interface walks
        // allocate, so back off once the screen has been open a while.
        var round = 0
        while (true) {
            localIp = withContext(Dispatchers.IO) { PartyQr.localWifiIpv4() }
            delay(if (round++ < 10) 3_000 else 30_000)
        }
    }

    val qrPayload = identity?.let { id ->
        PartyQr.build(
            npub = id.npub,
            ula = id.address,
            name = name.ifBlank { "Phone" },
            ip = if (listenOn) localIp else null,
            port = PartyQr.PARTY_UDP_PORT,
        )
    }
    // QR encode + bitmap fill off the composition: done in remember{} it ran
    // on the UI thread PER KEYSTROKE of the name field (the payload embeds the
    // name) — a ZXing encode plus a megabyte-plus allocation per character.
    // The 250 ms delay is a free debounce via coroutine cancellation.
    var qrBitmap by remember { mutableStateOf<android.graphics.Bitmap?>(null) }
    LaunchedEffect(qrPayload) {
        if (qrPayload == null) { qrBitmap = null; return@LaunchedEffect }
        if (qrBitmap != null) delay(250)
        qrBitmap = withContext(Dispatchers.Default) { renderQr(qrPayload) }
    }

    BackHandler {
        when {
            showScanner -> showScanner = false
            showShareQr -> showShareQr = false
            else -> onBack()
        }
    }

    Box(Modifier.fillMaxSize().background(SurfaceDark)) {
        Column(
            Modifier
                .fillMaxSize()
                .statusBarsPadding()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp, vertical = 16.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Row(
                Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text("‹ Back", color = TextMuted, fontSize = 15.sp, modifier = Modifier.clickable { onBack() }.padding(8.dp))
                Text("MESH PARTY", color = TextPrimary, fontSize = 17.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 3.sp)
                Spacer(Modifier.size(56.dp))
            }

            // My QR card
            Column(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(20.dp))
                    .background(CardBg)
                    .border(1.dp, CardBorder, RoundedCornerShape(20.dp))
                    .padding(18.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                qrBitmap?.let { bmp ->
                    Box(
                        Modifier
                            .clip(RoundedCornerShape(16.dp))
                            .background(Color.White)
                            .padding(12.dp),
                    ) {
                        Image(
                            bitmap = bmp.asImageBitmap(),
                            contentDescription = "My mesh party QR",
                            modifier = Modifier.size(220.dp),
                        )
                    }
                } ?: Text("Mesh identity unavailable on this device", color = TextMuted, fontSize = 14.sp)

                identity?.let {
                    Text(
                        it.npub.take(16) + "…" + it.npub.takeLast(6),
                        color = TextMuted,
                        fontSize = 12.sp,
                        textAlign = TextAlign.Center,
                    )
                }

                OutlinedTextField(
                    value = name,
                    onValueChange = {
                        name = it.take(24)
                        scope.launch { prefs.setPartyName(name) }
                    },
                    placeholder = { Text("Your name", color = TextMuted, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth()) },
                    modifier = Modifier.fillMaxWidth().height(56.dp),
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                    textStyle = TextStyle(color = TextPrimary, fontSize = 15.sp, textAlign = TextAlign.Center),
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedBorderColor = Color.White.copy(alpha = 0.3f),
                        unfocusedBorderColor = Color.White.copy(alpha = 0.12f),
                        cursorColor = BitcoinOrange,
                        focusedTextColor = TextPrimary,
                        unfocusedTextColor = TextPrimary,
                    ),
                    shape = RoundedCornerShape(12.dp),
                )

                // Direct-link toggle (hotspot mode)
                Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Column(Modifier.padding(end = 12.dp)) {
                        Text("Accept direct links", color = TextPrimary, fontSize = 15.sp, fontWeight = FontWeight.Medium)
                        Text(
                            when {
                                listenOn && localIp != null -> "Dialable at $localIp:${PartyQr.PARTY_UDP_PORT} — no internet needed"
                                listenOn -> "Waiting for a WiFi/hotspot address…"
                                else -> "Off — mesh routes via anchors only"
                            },
                            color = if (listenOn) BitcoinOrange else TextMuted,
                            fontSize = 12.sp,
                        )
                    }
                    Switch(
                        checked = listenOn,
                        onCheckedChange = { on ->
                            scope.launch {
                                prefs.setPartyListen(on)
                                FipsManager.requestMeshRestart(context)
                            }
                        },
                        colors = SwitchDefaults.colors(
                            checkedTrackColor = BitcoinOrange,
                            checkedThumbColor = Color.White,
                        ),
                    )
                }
            }

            GlassButton(
                text = "Scan a Phone",
                onClick = { showScanner = true },
                modifier = Modifier.fillMaxWidth().height(56.dp),
            )

            if (peers.isNotEmpty()) {
                Text("PARTY PEERS", color = TextMuted, fontSize = 12.sp, letterSpacing = 2.sp)
                peers.forEach { peer ->
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(14.dp))
                            .background(CardBg)
                            .border(1.dp, CardBorder, RoundedCornerShape(14.dp))
                            .clickable { onOpenChat() }
                            .padding(horizontal = 16.dp, vertical = 12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Column(Modifier.padding(end = 8.dp)) {
                            Text(peer.name, color = TextPrimary, fontSize = 15.sp, fontWeight = FontWeight.Medium)
                            Text(
                                peer.npub.take(14) + "…" + if (peer.ip.isNotBlank()) "  ·  direct ${peer.ip}" else "  ·  via mesh",
                                color = TextMuted,
                                fontSize = 11.sp,
                            )
                        }
                        Text(
                            "✕",
                            color = TextMuted,
                            fontSize = 16.sp,
                            modifier = Modifier.clickable {
                                scope.launch {
                                    prefs.removePartyPeer(peer.npub)
                                    FipsManager.requestMeshRestart(context)
                                }
                            }.padding(8.dp),
                        )
                    }
                }
                GlassButton(
                    text = "Open Flare Chat",
                    onClick = onOpenChat,
                    modifier = Modifier.fillMaxWidth().height(56.dp),
                )
            } else {
                Text(
                    "Scan another phone's party QR (or let them scan yours) to link your mesh nodes — works over 5G via anchors, or over any shared WiFi/hotspot with zero internet.",
                    color = TextMuted,
                    fontSize = 13.sp,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp),
                )
            }
            Spacer(Modifier.height(12.dp))
            // Hand the app itself to a friend: shares this install's own APK
            // through the system sheet (Quick Share/Bluetooth), so a nearby
            // phone gets the companion with zero internet — the whole party
            // premise.
            GlassButton(
                text = "Share this app",
                onClick = { showShareQr = true },
                modifier = Modifier.fillMaxWidth().height(48.dp),
            )
            Spacer(Modifier.height(12.dp))
        }

        // "Share this app" — a QR of the public download link, so the other
        // phone scans it with its normal camera and installs over any
        // internet. (The vps2 demo host serves the same APK the nodes do.)
        AnimatedVisibility(visible = showShareQr, enter = fadeIn(), exit = fadeOut()) {
            Box(
                Modifier
                    .fillMaxSize()
                    .background(Color.Black.copy(alpha = 0.94f))
                    .clickable { showShareQr = false },
                contentAlignment = Alignment.Center,
            ) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    // Encoded off-main; done in remember{} it dropped the
                    // overlay's first fade-in frame.
                    var dlQr by remember { mutableStateOf<android.graphics.Bitmap?>(null) }
                    LaunchedEffect(Unit) {
                        dlQr = withContext(Dispatchers.Default) { renderQr(APP_DOWNLOAD_URL) }
                    }
                    dlQr?.let { bmp ->
                        Box(
                            Modifier
                                .clip(RoundedCornerShape(16.dp))
                                .background(Color.White)
                                .padding(14.dp),
                        ) {
                            Image(
                                bitmap = bmp.asImageBitmap(),
                                contentDescription = "Companion download QR",
                                modifier = Modifier.size(240.dp),
                            )
                        }
                    }
                    Spacer(Modifier.height(18.dp))
                    Text(
                        "Scan with any camera to download\nthe Archipelago Companion",
                        color = TextPrimary,
                        fontSize = 15.sp,
                        textAlign = TextAlign.Center,
                    )
                    Spacer(Modifier.height(10.dp))
                    Text(
                        "…or send the APK file directly",
                        color = BitcoinOrange,
                        fontSize = 13.sp,
                        modifier = Modifier.clickable { scope.launch { shareCompanionApk(context) } }.padding(8.dp),
                    )
                    Spacer(Modifier.height(6.dp))
                    Text("Close", color = TextMuted, fontSize = 14.sp, modifier = Modifier.clickable { showShareQr = false }.padding(8.dp))
                }
            }
        }

        // Party QR scanner overlay
        AnimatedVisibility(visible = showScanner, enter = fadeIn(), exit = fadeOut()) {
            Box(Modifier.fillMaxSize().background(Color.Black)) {
                if (!hasCamera) {
                    Column(
                        Modifier.align(Alignment.Center).padding(horizontal = 32.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        Text(
                            "Camera access is needed to scan a party QR.",
                            color = TextPrimary,
                            fontSize = 15.sp,
                            textAlign = TextAlign.Center,
                        )
                        GlassButton(
                            text = "Grant camera access",
                            onClick = { cameraPermLauncher.launch(android.Manifest.permission.CAMERA) },
                            modifier = Modifier.fillMaxWidth().height(48.dp),
                        )
                    }
                } else CameraQrPreview(onDecoded = { text ->
                    val peer = PartyQr.parse(text)
                    when {
                        peer == null -> scanHint = "Not a mesh party QR"
                        peer.npub == identity?.npub -> scanHint = "That's your own QR"
                        else -> {
                            showScanner = false
                            scanHint = null
                            scope.launch {
                                prefs.upsertPartyPeer(peer)
                                // Pick up the direct-dial PeerConfig (and the
                                // listener, if ours is on) immediately.
                                FipsManager.requestMeshRestart(context)
                                // Pairing must be MUTUAL: announce ourselves so
                                // the scanned phone gets us as a peer + a chat
                                // entry without scanning back. Retried while
                                // the fresh link/session comes up.
                                val me = identity
                                if (me != null) {
                                    launch(Dispatchers.IO) {
                                        for (attempt in 0 until 6) {
                                            val ok = FlareClient.sendHello(
                                                peer = peer,
                                                myNpub = me.npub,
                                                myName = name.ifBlank { "Phone" },
                                                myUla = me.address,
                                                myIp = if (listenOn) localIp else null,
                                                myPort = PartyQr.PARTY_UDP_PORT,
                                            )
                                            if (ok) break
                                            delay(3_000)
                                        }
                                    }
                                }
                                onOpenChat()
                            }
                        }
                    }
                })
                Box(
                    Modifier
                        .align(Alignment.Center)
                        .size(260.dp)
                        .border(2.dp, BitcoinOrange.copy(alpha = 0.85f), RoundedCornerShape(20.dp)),
                )
                Text(
                    "Close",
                    color = TextPrimary,
                    fontSize = 16.sp,
                    modifier = Modifier
                        .align(Alignment.TopEnd)
                        .statusBarsPadding()
                        .clickable { showScanner = false }
                        .padding(20.dp),
                )
                scanHint?.let {
                    Text(
                        it,
                        color = BitcoinOrange,
                        fontSize = 14.sp,
                        textAlign = TextAlign.Center,
                        modifier = Modifier
                            .align(Alignment.BottomCenter)
                            .padding(bottom = 48.dp)
                            .fillMaxWidth(),
                    )
                }
            }
        }
    }

    // Scan hints fade so the camera feels live again.
    LaunchedEffect(scanHint) {
        if (scanHint != null) {
            delay(2500)
            scanHint = null
        }
    }
}

/** Render a QR payload as a bitmap (dark modules on white). 512 px covers the
 *  240.dp display size at any density; 640 was a third more pixels for nothing. */
private fun renderQr(payload: String, size: Int = 512): Bitmap? = try {
    val matrix = QRCodeWriter().encode(
        payload,
        BarcodeFormat.QR_CODE,
        size,
        size,
        mapOf(EncodeHintType.MARGIN to 1),
    )
    val pixels = IntArray(size * size)
    for (y in 0 until size) {
        for (x in 0 until size) {
            pixels[y * size + x] = if (matrix[x, y]) 0xFF0A0A0A.toInt() else 0xFFFFFFFF.toInt()
        }
    }
    Bitmap.createBitmap(pixels, size, size, Bitmap.Config.ARGB_8888)
} catch (_: Exception) {
    null
}

/** Share this install's own APK via the system share sheet — a nearby friend
 *  gets the companion with no internet at all (Quick Share / Bluetooth).
 *  The ~27 MB copy runs on IO — inline in the click handler it froze the UI
 *  for seconds (ANR territory on slow flash). Copied once per install; the
 *  cached file is reused while its size still matches the source. */
private suspend fun shareCompanionApk(context: android.content.Context) {
    try {
        val uri = withContext(Dispatchers.IO) {
            val src = java.io.File(context.applicationInfo.sourceDir)
            val dir = java.io.File(context.cacheDir, "share").apply { mkdirs() }
            val out = java.io.File(dir, "archipelago-companion.apk")
            if (!out.exists() || out.length() != src.length()) {
                src.copyTo(out, overwrite = true)
            }
            androidx.core.content.FileProvider.getUriForFile(
                context, "${context.packageName}.fileprovider", out,
            )
        }
        val send = android.content.Intent(android.content.Intent.ACTION_SEND).apply {
            type = "application/vnd.android.package-archive"
            putExtra(android.content.Intent.EXTRA_STREAM, uri)
            addFlags(android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        context.startActivity(
            android.content.Intent.createChooser(send, "Share Archipelago Companion")
                .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    } catch (_: Exception) {
        // No share targets / copy failed — nothing sensible to do here.
    }
}
