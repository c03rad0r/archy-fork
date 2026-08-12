package com.archipelago.app.ui.screens

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Image
import androidx.compose.material3.Icon
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.archipelago.app.fips.FipsManager
import com.archipelago.app.fips.FipsNative
import com.archipelago.app.fips.FipsPreferences
import com.archipelago.app.fips.FlareClient
import com.archipelago.app.fips.FlareMessage
import com.archipelago.app.fips.FlareStore
import com.archipelago.app.fips.PartyPeer
import com.archipelago.app.ui.theme.BitcoinOrange
import com.archipelago.app.ui.theme.SurfaceDark
import com.archipelago.app.ui.theme.TextMuted
import com.archipelago.app.ui.theme.TextPrimary
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.UUID

private val BubbleTheirs = Color.White.copy(alpha = 0.07f)
private val BubbleBorder = Color.White.copy(alpha = 0.08f)

/**
 * Flare — phone↔phone chat and photo beam over the FIPS mesh. Every byte is
 * E2E encrypted by the mesh layer and addressed by npub; whether it travels
 * via a public anchor (5G) or a direct hotspot link is invisible up here —
 * which is the entire point.
 */
@Composable
fun FlareScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    val prefs = remember { FipsPreferences(context) }
    val scope = rememberCoroutineScope()

    var identity by remember { mutableStateOf<FipsNative.Identity?>(null) }
    var myName by remember { mutableStateOf("Phone") }
    val peers by prefs.partyPeersFlow.collectAsState(initial = emptyList())
    var selectedNpub by remember { mutableStateOf<String?>(null) }
    val allMessages by FlareStore.messages.collectAsState()
    var draft by remember { mutableStateOf("") }

    LaunchedEffect(Unit) {
        identity = FipsManager.ensureIdentity(prefs)
        myName = prefs.partyName()
    }
    LaunchedEffect(peers) {
        if (selectedNpub == null || peers.none { it.npub == selectedNpub }) {
            selectedNpub = peers.firstOrNull()?.npub
        }
    }

    val peer = peers.firstOrNull { it.npub == selectedNpub }
    // derivedStateOf: filtering inline re-ran over the whole store on every
    // recomposition — including one per keystroke in the composer.
    val messages by remember(selectedNpub) {
        androidx.compose.runtime.derivedStateOf { allMessages.filter { it.peerNpub == selectedNpub } }
    }
    val listState = rememberLazyListState()
    LaunchedEffect(messages.size) {
        if (messages.isNotEmpty()) listState.animateScrollToItem(messages.size - 1)
    }

    fun sendText() {
        val target = peer ?: return
        val me = identity ?: return
        val text = draft.trim()
        if (text.isEmpty()) return
        draft = ""
        val msg = FlareMessage(
            id = UUID.randomUUID().toString(),
            peerNpub = target.npub,
            fromMe = true,
            name = myName,
            text = text,
            ts = System.currentTimeMillis(),
            status = FlareMessage.Status.SENDING,
        )
        FlareStore.add(msg)
        scope.launch(Dispatchers.IO) {
            val ok = FlareClient.sendText(target, me.npub, myName, text)
            FlareStore.setStatus(msg.id, if (ok) FlareMessage.Status.SENT else FlareMessage.Status.FAILED)
        }
    }

    fun sendPhoto(uri: Uri) {
        val target = peer ?: return
        val me = identity ?: return
        scope.launch(Dispatchers.IO) {
            val jpeg = compressPhoto(context, uri) ?: return@launch
            // Local copy so our own bubble renders the sent photo.
            val dir = File(context.cacheDir, "flare").apply { mkdirs() }
            val local = File(dir, "${UUID.randomUUID()}.jpg").apply { writeBytes(jpeg) }
            val msg = FlareMessage(
                id = UUID.randomUUID().toString(),
                peerNpub = target.npub,
                fromMe = true,
                name = myName,
                photoPath = local.absolutePath,
                ts = System.currentTimeMillis(),
                status = FlareMessage.Status.SENDING,
            )
            FlareStore.add(msg)
            val ok = FlareClient.sendPhoto(target, me.npub, myName, jpeg)
            FlareStore.setStatus(msg.id, if (ok) FlareMessage.Status.SENT else FlareMessage.Status.FAILED)
        }
    }

    val photoPicker = rememberLauncherForActivityResult(
        ActivityResultContracts.GetContent()
    ) { uri -> uri?.let { sendPhoto(it) } }

    BackHandler { onBack() }

    Column(
        Modifier
            .fillMaxSize()
            .background(SurfaceDark)
            .statusBarsPadding()
            .navigationBarsPadding()
            .imePadding(),
    ) {
        // Header
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text("‹ Back", color = TextMuted, fontSize = 15.sp, modifier = Modifier.clickable { onBack() }.padding(6.dp))
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Text("FLARE", color = TextPrimary, fontSize = 16.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 3.sp)
                peer?.let {
                    Text(
                        it.name + if (it.ip.isNotBlank()) "  ·  direct+mesh" else "  ·  mesh",
                        color = BitcoinOrange,
                        fontSize = 11.sp,
                    )
                }
            }
            Spacer(Modifier.size(48.dp))
        }

        // Peer tabs when chatting with more than one phone
        if (peers.size > 1) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                peers.forEach { p ->
                    val active = p.npub == selectedNpub
                    Text(
                        p.name,
                        color = if (active) BitcoinOrange else TextMuted,
                        fontSize = 13.sp,
                        modifier = Modifier
                            .clip(RoundedCornerShape(10.dp))
                            .background(if (active) BitcoinOrange.copy(alpha = 0.12f) else Color.Transparent)
                            .border(
                                1.dp,
                                if (active) BitcoinOrange.copy(alpha = 0.4f) else BubbleBorder,
                                RoundedCornerShape(10.dp),
                            )
                            .clickable { selectedNpub = p.npub }
                            .padding(horizontal = 12.dp, vertical = 6.dp),
                    )
                }
            }
        }

        if (peer == null) {
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                Text("No party peers yet — scan a phone first", color = TextMuted, fontSize = 14.sp)
            }
        } else {
            LazyColumn(
                state = listState,
                modifier = Modifier.weight(1f).fillMaxWidth(),
                contentPadding = androidx.compose.foundation.layout.PaddingValues(16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                items(messages, key = { it.id }) { msg ->
                    MessageBubble(msg)
                }
            }

            // Composer
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Box(
                    Modifier
                        .size(48.dp)
                        .clip(RoundedCornerShape(12.dp))
                        .background(BubbleTheirs)
                        .border(1.dp, BubbleBorder, RoundedCornerShape(12.dp))
                        .clickable { photoPicker.launch("image/*") },
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(Icons.Default.Image, "Beam a photo", tint = BitcoinOrange, modifier = Modifier.size(22.dp))
                }
                OutlinedTextField(
                    value = draft,
                    onValueChange = { draft = it },
                    placeholder = { Text("Send a flare…", color = TextMuted, fontSize = 14.sp) },
                    modifier = Modifier.weight(1f),
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                    keyboardActions = KeyboardActions(onSend = { sendText() }),
                    textStyle = TextStyle(color = TextPrimary, fontSize = 15.sp),
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedBorderColor = Color.White.copy(alpha = 0.3f),
                        unfocusedBorderColor = Color.White.copy(alpha = 0.12f),
                        cursorColor = BitcoinOrange,
                        focusedTextColor = TextPrimary,
                        unfocusedTextColor = TextPrimary,
                    ),
                    shape = RoundedCornerShape(12.dp),
                )
                Box(
                    Modifier
                        .size(48.dp)
                        .clip(RoundedCornerShape(12.dp))
                        .background(BitcoinOrange.copy(alpha = 0.15f))
                        .border(1.dp, BitcoinOrange.copy(alpha = 0.4f), RoundedCornerShape(12.dp))
                        .clickable { sendText() },
                    contentAlignment = Alignment.Center,
                ) {
                    Text("➤", color = BitcoinOrange, fontSize = 18.sp)
                }
            }
        }
    }
}

@Composable
private fun MessageBubble(msg: FlareMessage) {
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = if (msg.fromMe) Arrangement.End else Arrangement.Start,
    ) {
        Column(
            Modifier
                .widthIn(max = 300.dp)
                .clip(RoundedCornerShape(16.dp))
                .background(if (msg.fromMe) BitcoinOrange.copy(alpha = 0.14f) else BubbleTheirs)
                .border(
                    1.dp,
                    if (msg.fromMe) BitcoinOrange.copy(alpha = 0.35f) else BubbleBorder,
                    RoundedCornerShape(16.dp),
                )
                .padding(horizontal = 12.dp, vertical = 8.dp),
        ) {
            if (msg.photoPath.isNotBlank()) {
                // Decoded off-main and downsampled to the bubble width —
                // full-size decode in remember{} ran on the UI thread mid-
                // scroll and held ~8 MB per visible photo (OOM territory).
                var bmp by remember(msg.photoPath) { mutableStateOf<android.graphics.Bitmap?>(null) }
                LaunchedEffect(msg.photoPath) {
                    bmp = withContext(Dispatchers.IO) { decodeSampledPhoto(msg.photoPath, 600) }
                }
                bmp?.let {
                    Image(
                        bitmap = it.asImageBitmap(),
                        contentDescription = "Beamed photo",
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(10.dp)),
                        contentScale = ContentScale.FillWidth,
                    )
                }
            }
            if (msg.text.isNotBlank()) {
                Text(msg.text, color = TextPrimary, fontSize = 15.sp)
            }
            Text(
                when (msg.status) {
                    FlareMessage.Status.SENDING -> "sending…"
                    FlareMessage.Status.SENT -> "sent · E2E via mesh"
                    FlareMessage.Status.FAILED -> "failed — tap to retry later"
                    FlareMessage.Status.RECEIVED -> msg.name
                },
                color = if (msg.status == FlareMessage.Status.FAILED) BitcoinOrange else TextMuted,
                fontSize = 10.sp,
                modifier = Modifier.padding(top = 2.dp),
            )
        }
    }
}

/** Decode, downscale (≤1600px) and JPEG-compress a picked photo off-main. */
/** Decode a stored beamed photo at roughly [maxPx] on the long edge — the
 *  bubble renders at ~300 dp, so the stored 1600 px original is 25× the
 *  pixels needed. Blocking — call on IO. */
private fun decodeSampledPhoto(path: String, maxPx: Int): android.graphics.Bitmap? = try {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(path, bounds)
    var sample = 1
    while (maxOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= maxPx) sample *= 2
    BitmapFactory.decodeFile(path, BitmapFactory.Options().apply { inSampleSize = sample })
} catch (_: Exception) {
    null
}

private suspend fun compressPhoto(context: android.content.Context, uri: Uri): ByteArray? =
    withContext(Dispatchers.IO) {
        try {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            context.contentResolver.openInputStream(uri)?.use {
                BitmapFactory.decodeStream(it, null, bounds)
            }
            var sample = 1
            while (maxOf(bounds.outWidth, bounds.outHeight) / sample > 1600) sample *= 2
            val opts = BitmapFactory.Options().apply { inSampleSize = sample }
            val bitmap = context.contentResolver.openInputStream(uri)?.use {
                BitmapFactory.decodeStream(it, null, opts)
            } ?: return@withContext null
            val out = ByteArrayOutputStream()
            bitmap.compress(Bitmap.CompressFormat.JPEG, 80, out)
            bitmap.recycle()
            out.toByteArray()
        } catch (_: Exception) {
            null
        }
    }
