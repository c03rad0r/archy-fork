package com.archipelago.app.ui.components

import android.content.Context
import android.graphics.BitmapFactory
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.archipelago.app.R
import com.archipelago.app.ui.screens.GlassButton
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.NotFoundException
import com.google.zxing.RGBLuminanceSource
import com.google.zxing.common.HybridBinarizer

/**
 * Native replacement for the web wallet's scan pane — the shared [QrGlassModal]
 * shell (same visual design as neode-ui's WalletScanModal) with the camera and
 * decoding running natively, so the preview doesn't lag the way getUserMedia
 * does inside a WebView.
 *
 * Decoded text is handed back to the page ([onDecoded]) which does all the
 * detection/spend logic; the page in turn streams status lines (animated-QR
 * progress, "not recognised" errors) back in via [status] and closes the
 * modal through the JS bridge once it accepts a code.
 */
@Composable
fun WalletQrScannerModal(
    visible: Boolean,
    status: Pair<String, Boolean>?, // message from the web page + isError
    onDecoded: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val haptics = LocalHapticFeedback.current

    // Local error from a failed image upload; a fresh web status replaces it.
    var uploadError by remember { mutableStateOf<String?>(null) }
    val noQrMessage = stringResource(R.string.no_qr_in_image)
    val imagePicker = rememberLauncherForActivityResult(
        ActivityResultContracts.GetContent()
    ) { uri ->
        if (uri != null) {
            val decoded = decodeQrFromUri(context, uri)
            if (decoded != null) {
                uploadError = null
                onDecoded(decoded)
            } else {
                uploadError = noQrMessage
            }
        }
    }

    LaunchedEffect(visible) { if (visible) uploadError = null }
    LaunchedEffect(status) { if (status != null) uploadError = null }

    // Throttle repeat frames: a static QR decodes many times a second but the
    // page only needs one; animated QRs still stream because each frame's
    // text differs.
    var lastText by remember { mutableStateOf("") }
    var lastSentAt by remember { mutableStateOf(0L) }
    LaunchedEffect(visible) {
        if (visible) {
            lastText = ""
            lastSentAt = 0L
        }
    }

    QrGlassModal(
        visible = visible,
        title = stringResource(R.string.scan_to_send),
        status = uploadError?.let { it to true } ?: status,
        idleHint = stringResource(R.string.scan_wallet_hint),
        permissionRationale = stringResource(R.string.camera_permission_needed),
        onDismiss = onDismiss,
        onDecoded = { text ->
            val now = System.currentTimeMillis()
            if (text != lastText || now - lastSentAt > 250) {
                // Buzz on the FIRST hit only: an animated QR streams a new
                // frame every few ms, and one buzz each would be a drill in
                // the hand.
                if (lastText.isEmpty()) {
                    haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                }
                lastText = text
                lastSentAt = now
                onDecoded(text)
            }
        },
        footer = {
            GlassButton(
                text = stringResource(R.string.upload_qr_image),
                onClick = { imagePicker.launch("image/*") },
                modifier = Modifier.fillMaxWidth().height(48.dp),
            )
        },
    )
}

/** Decode a QR from a picked image, downsampled so huge photos stay cheap. */
private fun decodeQrFromUri(context: Context, uri: Uri): String? {
    return try {
        val resolver = context.contentResolver
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        resolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, bounds) }
        var sample = 1
        val maxDim = maxOf(bounds.outWidth, bounds.outHeight)
        while (maxDim / (sample * 2) >= 1600) sample *= 2
        val opts = BitmapFactory.Options().apply { inSampleSize = sample }
        val bmp = resolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, opts) }
            ?: return null
        val pixels = IntArray(bmp.width * bmp.height)
        bmp.getPixels(pixels, 0, bmp.width, 0, 0, bmp.width, bmp.height)
        val source = RGBLuminanceSource(bmp.width, bmp.height, pixels)
        val reader = MultiFormatReader().apply {
            setHints(
                mapOf(
                    DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE),
                    DecodeHintType.TRY_HARDER to true,
                )
            )
        }
        try {
            reader.decodeWithState(BinaryBitmap(HybridBinarizer(source))).text
        } catch (_: NotFoundException) {
            // Light-on-dark QRs (dark-themed wallets) decode inverted.
            reader.reset()
            reader.decodeWithState(BinaryBitmap(HybridBinarizer(source.invert()))).text
        }
    } catch (_: Exception) {
        null
    }
}
