package com.archipelago.app.ui.components

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.hardware.camera2.CameraCharacteristics
import android.hardware.camera2.CameraManager
import android.hardware.camera2.CameraMetadata
import android.hardware.camera2.CaptureRequest
import android.os.Process
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.camera2.interop.Camera2Interop
import androidx.camera.camera2.interop.ExperimentalCamera2Interop
import androidx.camera.core.CameraSelector
import androidx.camera.core.FocusMeteringAction
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.core.SurfaceOrientedMeteringPointFactory
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.FlashOff
import androidx.compose.material.icons.filled.FlashOn
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import com.archipelago.app.R
import com.archipelago.app.data.PairResult
import com.archipelago.app.data.ServerQrParser
import com.archipelago.app.ui.screens.GlassButton
import com.archipelago.app.ui.theme.BitcoinOrange
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.GlobalHistogramBinarizer
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader
import kotlinx.coroutines.delay
import java.util.concurrent.Executors

/**
 * Scans the node pairing QR (docs/companion-pairing-qr.md) and reports the
 * decoded server entry. Handles the camera permission itself; foreign/invalid
 * codes show a hint in the status strip and scanning continues.
 *
 * Visually this is the SAME glass modal the web wallet uses (neode-ui's
 * WalletScanModal) — scrim, glass card, square preview, orange viewfinder,
 * status strip — so pairing from the app and scanning from the web UI look
 * like one product rather than two different scanners.
 */
@Composable
fun QrScannerOverlay(
    visible: Boolean,
    onDismiss: () -> Unit,
    onServerScanned: (PairResult.Success) -> Unit,
) {
    val haptics = LocalHapticFeedback.current
    var hintRes by remember { mutableStateOf<Int?>(null) }
    var handled by remember { mutableStateOf(false) }

    LaunchedEffect(visible) {
        if (visible) {
            handled = false
            hintRes = null
        }
    }

    // Foreign-code hint fades after a moment so scanning feels live again.
    LaunchedEffect(hintRes) {
        if (hintRes != null) {
            delay(2500)
            hintRes = null
        }
    }

    QrGlassModal(
        visible = visible,
        title = stringResource(R.string.scan_node_qr),
        status = hintRes?.let { stringResource(it) to true },
        idleHint = stringResource(R.string.scan_qr_hint),
        permissionRationale = stringResource(R.string.camera_permission_needed),
        onDismiss = onDismiss,
        onDecoded = { text ->
            if (!handled) {
                when (val result = ServerQrParser.parse(text)) {
                    is PairResult.Success -> {
                        handled = true
                        // Confirm the hit in the hand — the eye is still on the
                        // code, not on the screen.
                        haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                        onServerScanned(result)
                    }
                    is PairResult.UnsupportedVersion -> hintRes = R.string.update_app_for_qr
                    is PairResult.Invalid -> hintRes = R.string.invalid_pairing_qr
                }
            }
        },
    )
}

/**
 * The shared native scanner shell — one visual contract for every camera the
 * app opens (pairing, wallet), mirroring neode-ui's WalletScanModal so the
 * native and web scanners are indistinguishable:
 *  - black/60 scrim, dismiss on tap-outside
 *  - glass card (rounded 24, white/10 hairline) capped at 420dp
 *  - square preview with the 62% orange viewfinder and a darkened surround
 *  - a status strip that carries hints and errors
 *  - an optional footer (the wallet's "Upload image")
 */
@Composable
internal fun QrGlassModal(
    visible: Boolean,
    title: String,
    // message + isError; null falls back to [idleHint].
    status: Pair<String, Boolean>?,
    idleHint: String,
    permissionRationale: String,
    onDismiss: () -> Unit,
    onDecoded: (String) -> Unit,
    footer: @Composable (() -> Unit)? = null,
) {
    val context = LocalContext.current
    var hasPermission by remember {
        mutableStateOf(
            ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
                PackageManager.PERMISSION_GRANTED
        )
    }
    var torchOn by remember { mutableStateOf(false) }
    var hasTorch by remember { mutableStateOf(false) }

    val permissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { granted -> hasPermission = granted }

    LaunchedEffect(visible) {
        if (visible) {
            val granted = ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
                PackageManager.PERMISSION_GRANTED
            hasPermission = granted
            if (!granted) permissionLauncher.launch(Manifest.permission.CAMERA)
        } else {
            torchOn = false
        }
    }

    AnimatedVisibility(visible = visible, enter = fadeIn(), exit = fadeOut()) {
        BackHandler { onDismiss() }
        Box(
            Modifier
                .fillMaxSize()
                .background(Color.Black.copy(alpha = 0.6f))
                .clickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                    onClick = onDismiss,
                ),
            contentAlignment = Alignment.Center,
        ) {
            Column(
                Modifier
                    .padding(16.dp)
                    .widthIn(max = 420.dp)
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(24.dp))
                    .background(Color(0xF212151C))
                    .border(1.dp, Color.White.copy(alpha = 0.10f), RoundedCornerShape(24.dp))
                    .clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                        onClick = {}, // swallow — only the scrim dismisses
                    )
                    .padding(24.dp),
            ) {
                Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Text(
                        text = title,
                        style = MaterialTheme.typography.titleLarge,
                        fontWeight = FontWeight.SemiBold,
                        color = Color.White,
                    )
                    IconButton(onClick = onDismiss) {
                        Icon(
                            Icons.Default.Close,
                            stringResource(R.string.close),
                            tint = Color.White.copy(alpha = 0.7f),
                        )
                    }
                }

                Spacer(Modifier.height(8.dp))

                Box(
                    Modifier
                        .fillMaxWidth()
                        .aspectRatio(1f)
                        .clip(RoundedCornerShape(12.dp))
                        .background(Color.Black.copy(alpha = 0.4f))
                        .border(1.dp, Color.White.copy(alpha = 0.10f), RoundedCornerShape(12.dp)),
                    contentAlignment = Alignment.Center,
                ) {
                    if (hasPermission) {
                        CameraQrPreview(
                            onDecoded = onDecoded,
                            torchOn = torchOn,
                            onTorchAvailable = { hasTorch = it },
                        )
                        // Viewfinder — 62% of the preview, matching the web
                        // modal's .scan-viewfinder, and matching the ROI the
                        // decoder actually reads (QR_ROI_FRACTION).
                        Box(
                            Modifier
                                .fillMaxSize(QR_ROI_FRACTION)
                                .border(
                                    2.dp,
                                    BitcoinOrange.copy(alpha = 0.85f),
                                    RoundedCornerShape(16.dp),
                                ),
                        )
                        if (hasTorch) {
                            IconButton(
                                onClick = { torchOn = !torchOn },
                                modifier = Modifier
                                    .align(Alignment.TopEnd)
                                    .padding(6.dp)
                                    .clip(RoundedCornerShape(50))
                                    .background(Color.Black.copy(alpha = 0.45f)),
                            ) {
                                Icon(
                                    if (torchOn) Icons.Default.FlashOn else Icons.Default.FlashOff,
                                    stringResource(
                                        if (torchOn) R.string.torch_off else R.string.torch_on,
                                    ),
                                    tint = if (torchOn) BitcoinOrange else Color.White.copy(alpha = 0.85f),
                                )
                            }
                        }
                    } else {
                        Column(
                            Modifier.padding(horizontal = 24.dp),
                            horizontalAlignment = Alignment.CenterHorizontally,
                            verticalArrangement = Arrangement.spacedBy(12.dp),
                        ) {
                            Text(
                                text = permissionRationale,
                                color = Color.White.copy(alpha = 0.7f),
                                style = MaterialTheme.typography.bodyMedium,
                                textAlign = TextAlign.Center,
                            )
                            GlassButton(
                                text = stringResource(R.string.grant_camera_access),
                                onClick = { permissionLauncher.launch(Manifest.permission.CAMERA) },
                                modifier = Modifier.fillMaxWidth().height(48.dp),
                            )
                        }
                    }
                }

                Spacer(Modifier.height(16.dp))

                Box(
                    Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(8.dp))
                        .background(Color.White.copy(alpha = 0.05f))
                        .padding(12.dp)
                        .defaultMinSize(minHeight = 24.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(
                        text = status?.first?.takeIf { it.isNotBlank() } ?: idleHint,
                        style = MaterialTheme.typography.bodySmall,
                        color = if (status?.second == true) {
                            Color(0xFFF87171)
                        } else {
                            Color.White.copy(alpha = 0.6f)
                        },
                        textAlign = TextAlign.Center,
                    )
                }

                if (footer != null) {
                    Spacer(Modifier.height(16.dp))
                    footer()
                }
            }
        }
    }
}

/**
 * Warm the CameraX provider and the ZXing decode path before the user ever
 * asks for a scan, so opening the scanner doesn't pay provider init + class
 * loading on the critical path. Does NOT open the camera: no permission is
 * needed, no LED lights up, nothing is recorded — [ProcessCameraProvider]
 * init is process-wide and cached, and the synthetic decode below just walks
 * a blank 32x32 frame to class-load the binarizer/detector.
 *
 * Called once per process from the kiosk WebView (first page load) and by the
 * page via `ArchipelagoQr.prewarm()`.
 */
internal fun prewarmQrScanner(context: Context) {
    if (!qrPrewarmed.compareAndSet(false, true)) return
    val app = context.applicationContext
    runCatching { ProcessCameraProvider.getInstance(app) }
    // Off the UI thread: the first decode attempt loads a dozen ZXing classes.
    Executors.newSingleThreadExecutor().let { exec ->
        exec.execute {
            runCatching {
                val blank = ByteArray(32 * 32)
                val reader = MultiFormatReader().apply {
                    setHints(mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE)))
                }
                val source = PlanarYUVLuminanceSource(blank, 32, 32, 0, 0, 32, 32, false)
                reader.decodeWithState(BinaryBitmap(HybridBinarizer(source)))
            }
        }
        exec.shutdown()
    }
}

private val qrPrewarmed = java.util.concurrent.atomic.AtomicBoolean(false)

/**
 * Fraction of the preview's shorter edge that both the on-screen viewfinder
 * and the decoder's region of interest use. Keeping them identical is the
 * point: the user aims at the box, and the box is exactly what gets decoded.
 */
internal const val QR_ROI_FRACTION = 0.62f

/**
 * Shared by the pairing scanner and the wallet scan modal.
 *
 * [torchOn] drives the flash; [onTorchAvailable] reports whether this camera
 * has one at all (the caller only draws its toggle when it does).
 *
 * ## Why this looks the way it does
 *
 * The previous version hunted: a scheduled tick alternated the optical zoom
 * between 1x and 1.5x and re-fired `startFocusAndMetering(...disableAutoCancel())`
 * every 2 seconds. Both are camera-hostile:
 *
 *  - Every zoom step restarts AE/AF convergence, so the sensor spends the
 *    seconds right after it delivering soft frames — precisely the frames the
 *    decoder needs to be sharp. The visible symptom is the "zooms in and out
 *    and takes ages" report.
 *  - `disableAutoCancel()` leaves AF **locked** at whatever it converged on
 *    instead of handing the lens back to continuous AF, so a re-aim never
 *    refocused on its own; the next timer tick then kicked off another full
 *    sweep from a locked position — a lens that hunts forever.
 *
 * A stock camera app does neither. It leaves CameraX's continuous AF alone,
 * refocuses on tap, and never touches zoom. This does the same, with one
 * concession to the "hand-held QR is a static scene" case: if nothing has
 * decoded for a few seconds, ONE auto-cancelling focus nudge is issued (and
 * then not again for a while), which re-arms continuous AF instead of
 * fighting it.
 */
@Composable
internal fun CameraQrPreview(
    onDecoded: (String) -> Unit,
    torchOn: Boolean = false,
    onTorchAvailable: (Boolean) -> Unit = {},
) {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val currentOnDecoded by rememberUpdatedState(onDecoded)
    val currentOnTorchAvailable by rememberUpdatedState(onTorchAvailable)
    var camera by remember { mutableStateOf<androidx.camera.core.Camera?>(null) }
    val previewView = remember {
        PreviewView(context).apply {
            scaleType = PreviewView.ScaleType.FILL_CENTER
            // TextureView, not the SurfaceView default: SurfaceView punches a
            // hole in the window, which black-flashes inside Compose fades and
            // ignores rounded-corner clipping (the glass modal).
            implementationMode = PreviewView.ImplementationMode.COMPATIBLE
        }
    }
    // Set by the analyzer on every decode; the focus nudge below reads it to
    // tell "nothing in view" from "reading fine, leave the camera alone".
    val lastDecodeAt = remember { java.util.concurrent.atomic.AtomicLong(0L) }
    // A tap-to-focus wins over the periodic centre AF for a few seconds.
    val lastTapFocusAt = remember { java.util.concurrent.atomic.AtomicLong(0L) }

    DisposableEffect(Unit) {
        // Analysis runs at display priority: the decode thread competes with
        // the FIPS mesh service's native workers in this same process, and a
        // background-priority analyzer is exactly how a sharp, well-framed
        // code still takes seconds to land.
        val analysisExecutor = Executors.newSingleThreadExecutor { r ->
            Thread {
                Process.setThreadPriority(Process.THREAD_PRIORITY_DISPLAY)
                r.run()
            }.apply { name = "qr-analyzer" }
        }
        val mainExecutor = ContextCompat.getMainExecutor(context)
        val providerFuture = ProcessCameraProvider.getInstance(context)
        var provider: ProcessCameraProvider? = null
        val focusScheduler = Executors.newSingleThreadScheduledExecutor()

        providerFuture.addListener({
            val p = providerFuture.get()
            provider = p
            val previewBuilder = Preview.Builder()
            tuneForBarcodes(previewBuilder, context)
            val preview = previewBuilder.build().also {
                it.setSurfaceProvider(previewView.surfaceProvider)
            }
            // Dense Lightning-invoice QRs need BOTH enough pixels per module and
            // sharp focus. 1280x720 left dense invoices undecodable while sparse
            // address QRs still read — the "scanner doesn't pick up invoices"
            // report. 1920x1080 roughly doubles module resolution. The analyzer
            // never binarizes the full 2 MP: it reads the centre ROI at this
            // resolution (for dense codes) and the whole frame at half of it
            // (for coverage), so the big frame costs little.
            @Suppress("DEPRECATION")
            val analysis = ImageAnalysis.Builder()
                .setTargetResolution(android.util.Size(1920, 1080))
                .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                .build()
                .also {
                    it.setAnalyzer(
                        analysisExecutor,
                        QrCodeAnalyzer { text ->
                            lastDecodeAt.set(System.currentTimeMillis())
                            mainExecutor.execute { currentOnDecoded(text) }
                        },
                    )
                }
            try {
                p.unbindAll()
                val cam = p.bindToLifecycle(lifecycleOwner, CameraSelector.DEFAULT_BACK_CAMERA, preview, analysis)
                camera = cam
                currentOnTorchAvailable(cam.cameraInfo.hasFlashUnit())
                // Start the clock at bind time so the nudge below waits for the
                // user to actually aim before it does anything.
                lastDecodeAt.set(System.currentTimeMillis())
                // Centre point, normalized — valid before the view is measured.
                val point = SurfaceOrientedMeteringPointFactory(1f, 1f).createPoint(0.5f, 0.5f)
                // A one-shot AF action puts the lens in AUTO — i.e. LOCKED —
                // until it auto-cancels. The default 5s lock is far too long
                // here: it spans exactly the window where the user is swinging
                // the phone towards the code, and a locked lens cannot follow
                // them. Hand control back after 1s so CONTINUOUS_PICTURE (set
                // explicitly in tuneForBarcodes) does the real work, which is
                // what actually tracks a moving aim.
                val focusAction = FocusMeteringAction.Builder(point, FocusMeteringAction.FLAG_AF)
                    .setAutoCancelDuration(1, java.util.concurrent.TimeUnit.SECONDS)
                    .build()
                var lastNudgeAt = 0L
                focusScheduler.scheduleWithFixedDelay({
                    val now = System.currentTimeMillis()
                    // The nudge only exists for the one case continuous AF
                    // genuinely misses: the phone held perfectly still on a
                    // code while the lens sits at its resting focus, with no
                    // scene change to trigger a sweep.
                    if (now - lastDecodeAt.get() > 2_000 &&
                        now - lastNudgeAt > 3_000 &&
                        now - lastTapFocusAt.get() > 3_000
                    ) {
                        lastNudgeAt = now
                        runCatching { cam.cameraControl.startFocusAndMetering(focusAction) }
                    }
                }, 1, 1, java.util.concurrent.TimeUnit.SECONDS)
            } catch (_: Exception) {
                // Camera unavailable — the user can dismiss and enter details manually.
            }
        }, mainExecutor)

        onDispose {
            focusScheduler.shutdownNow()
            runCatching { camera?.cameraControl?.enableTorch(false) }
            camera = null
            provider?.unbindAll()
            analysisExecutor.shutdown()
        }
    }

    // Torch follows the caller's state (and switches off when the view goes).
    LaunchedEffect(camera, torchOn) {
        runCatching { camera?.cameraControl?.enableTorch(torchOn) }
    }

    AndroidView(
        factory = { previewView },
        modifier = Modifier
            .fillMaxSize()
            // Tap-to-focus: the ROI assumes the code is centred; a tap lets the
            // user point at one that isn't, or re-trigger AF the instant
            // they've framed it.
            .pointerInput(camera) {
                detectTapGestures { offset ->
                    val cam = camera ?: return@detectTapGestures
                    val factory = previewView.meteringPointFactory
                    val action = FocusMeteringAction.Builder(
                        factory.createPoint(offset.x, offset.y),
                        FocusMeteringAction.FLAG_AF or FocusMeteringAction.FLAG_AE,
                    ).build()
                    lastTapFocusAt.set(System.currentTimeMillis())
                    runCatching { cam.cameraControl.startFocusAndMetering(action) }
                }
            },
    )
}

/**
 * Configure the capture session the way a dedicated barcode scanner does,
 * rather than the way a photo app does.
 *
 * The single most valuable knob is **CONTROL_AE_TARGET_FPS_RANGE**. Left
 * alone, auto-exposure indoors happily drops the sensor to 10–15 fps and
 * takes 60–100 ms exposures — every hand-held frame is then motion-blurred,
 * and a blurred QR is not a slow decode, it is *no* decode. The user waves
 * the phone about waiting for a lock that cannot happen. Pinning the lower
 * bound of the AE range as high as the device allows caps exposure time
 * (~33 ms at 30 fps), so frames come out sharp; AE compensates with gain
 * instead, and ZXing tolerates noise far better than it tolerates blur.
 * (Dark rooms get grainier as a result — that is what the torch button is
 * for, and grainy-but-sharp still decodes where smooth-but-smeared never
 * does.)
 *
 * CONTINUOUS_PICTURE is set explicitly so that when a tap-to-focus action
 * expires, CameraX restores continuous AF rather than whatever the device
 * defaults to; FAST noise/edge processing shaves ISP latency per frame.
 *
 * All of it is best-effort — an OEM that rejects a key just keeps its default.
 */
@androidx.annotation.OptIn(ExperimentalCamera2Interop::class)
private fun tuneForBarcodes(builder: Preview.Builder, context: Context) {
    runCatching {
        val ext = Camera2Interop.Extender(builder)
        ext.setCaptureRequestOption(
            CaptureRequest.CONTROL_AF_MODE,
            CameraMetadata.CONTROL_AF_MODE_CONTINUOUS_PICTURE,
        )
        ext.setCaptureRequestOption(
            CaptureRequest.NOISE_REDUCTION_MODE,
            CameraMetadata.NOISE_REDUCTION_MODE_FAST,
        )
        ext.setCaptureRequestOption(
            CaptureRequest.EDGE_MODE,
            CameraMetadata.EDGE_MODE_FAST,
        )
        highestSteadyFpsRange(context)?.let {
            ext.setCaptureRequestOption(CaptureRequest.CONTROL_AE_TARGET_FPS_RANGE, it)
        }
    }
}

/**
 * The back camera's AE range with the highest floor, ignoring anything that
 * runs past 30 fps (those are the high-speed/slow-motion modes, which cost
 * light for frames we do not need).
 */
private fun highestSteadyFpsRange(context: Context): android.util.Range<Int>? = runCatching {
    val manager = context.getSystemService(CameraManager::class.java) ?: return@runCatching null
    val backId = manager.cameraIdList.firstOrNull { id ->
        manager.getCameraCharacteristics(id)
            .get(CameraCharacteristics.LENS_FACING) == CameraCharacteristics.LENS_FACING_BACK
    } ?: return@runCatching null
    manager.getCameraCharacteristics(backId)
        .get(CameraCharacteristics.CONTROL_AE_AVAILABLE_TARGET_FPS_RANGES)
        ?.filter { it.upper <= 30 }
        ?.maxWithOrNull(compareBy({ it.lower }, { it.upper }))
}.getOrNull()

/**
 * ZXing decoder over the camera's Y (luminance) plane.
 *
 * ## The rule this class exists to obey
 *
 * **Every frame costs the same, and every frame sees the whole scene.**
 *
 * That sounds obvious; the previous version violated both halves and produced
 * a scanner with a very specific failure: it locked on instantly if the code
 * was already in view when the camera opened, but crawled if you opened it
 * and then moved to the code. The cause was an escalation ladder — each frame
 * that failed to decode unlocked progressively more expensive searches, up to
 * a TRY_HARDER pass over the full 2 MP frame plus an inverted retry, easily
 * 150–300 ms of work.
 *
 * So the moment the user began hunting for the code, the analyzer dropped from
 * ~30 attempts per second to ~4, each one on a motion-blurred frame. By the
 * time they framed the code and held still, the pipeline was busy grinding
 * through an exhaustive search of an old, blurry frame. Escalating on failure
 * is exactly backwards: failure means the user is still aiming, which is when
 * the scanner must be at its *fastest*, not its most thorough.
 *
 * ## What runs now, on every single frame
 *
 *  1. **Centre ROI at full resolution** ([QR_ROI_FRACTION], ~0.45 MP). Full
 *     sensor detail, so dense Lightning invoices keep their pixels-per-module.
 *  2. **The whole frame at half resolution** (~0.5 MP). This is what fixes the
 *     "move to the code" case: coverage is no longer limited to the viewfinder
 *     box on the fast path, so a code that is merely *near* the middle decodes
 *     immediately instead of waiting for a slow tier to come around. A code
 *     big enough to be off-centre is big enough to survive the 2x downscale.
 *  3. **One alternating second binarizer** — GlobalHistogram over the ROI on
 *     even frames, over the half-frame on odd ones. Hybrid is tuned for
 *     shadowed paper; most codes this app scans are on a *screen* (the node's
 *     pairing popup, another phone's wallet) where a global threshold is both
 *     cheaper and more reliable. Alternating keeps the per-frame budget flat.
 *
 * Two rare extras, both bounded so they can never dent the loop above: an
 * inverted ROI pass every 8th frame (light-on-dark codes), and one TRY_HARDER
 * pass over the half-frame at most once a second (skewed/damaged codes).
 *
 * Steady-state that is ~35 ms per frame — around 27 attempts per second, and
 * it does not degrade the longer the user hunts.
 *
 * Buffers are allocated once and reused: the original path allocated a fresh
 * ~2 MB array per frame, 60 MB/s of garbage at 30 fps, with GC pauses landing
 * mid-decode.
 */
private class QrCodeAnalyzer(private val onDecoded: (String) -> Unit) : ImageAnalysis.Analyzer {
    // QRCodeReader directly rather than MultiFormatReader: with a single
    // format in play the dispatch and per-call state reset are pure overhead.
    private val reader = QRCodeReader()
    private val plainHints = mapOf<DecodeHintType, Any>(
        DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE),
    )
    private val hardHints = mapOf<DecodeHintType, Any>(
        DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE),
        DecodeHintType.TRY_HARDER to true,
    )

    private var roiBuffer = ByteArray(0)
    private var halfBuffer = ByteArray(0)
    private var frame = 0L
    private var lastHardAt = 0L

    private fun read(
        source: PlanarYUVLuminanceSource,
        global: Boolean = false,
        hard: Boolean = false,
        inverted: Boolean = false,
    ): String? {
        val src = if (inverted) source.invert() else source
        val bitmap = BinaryBitmap(
            if (global) GlobalHistogramBinarizer(src) else HybridBinarizer(src),
        )
        return runCatching {
            reader.decode(bitmap, if (hard) hardHints else plainHints).text
        }.getOrNull().also { reader.reset() }
    }

    override fun analyze(image: ImageProxy) {
        try {
            val plane = image.planes[0]
            val buffer = plane.buffer
            val stride = plane.rowStride
            // YUV_420_888 permits an interleaved Y plane. Rare, but a device
            // that does it would otherwise hand the decoder pure noise.
            val pixelStride = plane.pixelStride
            val width = image.width
            val height = image.height
            frame++

            buffer.rewind()
            val available = buffer.remaining()

            // ── 1. Centre ROI, full resolution ──────────────────────────────
            val side = (minOf(width, height) * QR_ROI_FRACTION).toInt().coerceAtLeast(1)
            val left = (width - side) / 2
            val top = (height - side) / 2
            if (roiBuffer.size != side * side) roiBuffer = ByteArray(side * side)
            for (row in 0 until side) {
                val srcPos = (top + row) * stride + left * pixelStride
                if (srcPos + side * pixelStride > available) break
                if (pixelStride == 1) {
                    buffer.position(srcPos)
                    buffer.get(roiBuffer, row * side, side)
                } else {
                    val dst = row * side
                    for (col in 0 until side) {
                        roiBuffer[dst + col] = buffer.get(srcPos + col * pixelStride)
                    }
                }
            }
            val roi = PlanarYUVLuminanceSource(roiBuffer, side, side, 0, 0, side, side, false)
            read(roi)?.let { onDecoded(it); return }

            // ── 2. Whole frame, half resolution ─────────────────────────────
            val hw = width / 2
            val hh = height / 2
            if (halfBuffer.size != hw * hh) halfBuffer = ByteArray(hw * hh)
            var truncated = false
            for (row in 0 until hh) {
                val srcRow = row * 2 * stride
                val dst = row * hw
                for (col in 0 until hw) {
                    val srcPos = srcRow + col * 2 * pixelStride
                    if (srcPos >= available) { truncated = true; break }
                    halfBuffer[dst + col] = buffer.get(srcPos)
                }
                if (truncated) break
            }
            val half = PlanarYUVLuminanceSource(halfBuffer, hw, hh, 0, 0, hw, hh, false)
            read(half)?.let { onDecoded(it); return }

            // ── 3. Alternating second binarizer ─────────────────────────────
            val second = if (frame % 2 == 0L) roi else half
            read(second, global = true)?.let { onDecoded(it); return }

            // ── Bounded extras ──────────────────────────────────────────────
            if (frame % 8 == 0L) {
                read(roi, inverted = true)?.let { onDecoded(it); return }
            }
            val now = System.currentTimeMillis()
            if (now - lastHardAt >= 1_000) {
                lastHardAt = now
                read(half, hard = true)?.let { onDecoded(it); return }
            }
        } catch (_: Exception) {
            // Malformed frame; skip it.
        } finally {
            image.close()
        }
    }
}
