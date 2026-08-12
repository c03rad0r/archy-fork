package com.archipelago.app.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Palette
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Fill
import androidx.compose.ui.input.pointer.changedToUp
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.archipelago.app.R
import com.archipelago.app.ui.theme.ControllerStyle
import com.archipelago.app.ui.theme.NES
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.abs

// ═══════════════════════════════════════════════════════════
// Palettes
// ═══════════════════════════════════════════════════════════

data class NESPalette(
    val body: Color, val face: Color, val ridge: Color,
    val label: Color, val labelMuted: Color,
    val dpad: Color, val dpadHi: Color,
    val btn: Color, val btnPress: Color,
    val capsule: Color, val capsulePress: Color,
    val inlayBg: Color, val inlayBorder: Color,
)

val ClassicPalette = NESPalette(
    body = NES.ClassicBody, face = NES.ClassicFace, ridge = NES.ClassicRidge,
    label = NES.ClassicLabel, labelMuted = NES.ClassicLabelMuted,
    dpad = Color(0xFF0C0C0C), dpadHi = Color(0xFF1A1A1A),
    btn = NES.ClassicButtonRed, btnPress = NES.ClassicButtonRedPress,
    capsule = Color(0xFF1C1C1C), capsulePress = Color(0xFF0E0E0E),
    inlayBg = Color(0xFF080808), inlayBorder = Color(0xFF999999),
)

// Glassmorphism-black (OS design): translucent dark surfaces so the backdrop
// shows through the controller, subtle white-alpha borders, translucent-white
// buttons. Accents come from each button's ring.
val DarkPalette = NESPalette(
    body = Color(0xA6121216), face = Color(0x8C0E0E12), ridge = Color(0x14FFFFFF),
    label = Color(0xFF9A9A9A), labelMuted = Color(0xFF777777),
    dpad = Color(0xFF202024), dpadHi = Color(0xFF33333A),
    btn = Color(0x14FFFFFF), btnPress = Color(0x0AFFFFFF),
    capsule = Color(0x12FFFFFF), capsulePress = Color(0x08FFFFFF),
    inlayBg = Color(0x990A0A0A), inlayBorder = Color(0x1FFFFFFF),
)

fun paletteFor(style: ControllerStyle) = if (style == ControllerStyle.CLASSIC) ClassicPalette else DarkPalette

// ═══════════════════════════════════════════════════════════
// Landscape NES Controller
// ═══════════════════════════════════════════════════════════

@Composable
fun NESController(
    style: ControllerStyle = ControllerStyle.CLASSIC,
    playerId: Int = 0,
    onKey: (String) -> Unit,
    onMenu: () -> Unit,
    onPlayerToggle: () -> Unit = {},
    onToggleStyle: (() -> Unit)? = null,
    modifier: Modifier = Modifier,
) {
    val c = paletteFor(style)
    val isClassic = style == ControllerStyle.CLASSIC

    Box(
        modifier = modifier
            .fillMaxSize()
            .threeFingerHold(onMenu)
            .padding(horizontal = 40.dp, vertical = 24.dp),
        contentAlignment = Alignment.Center,
    ) {
        // Controller body
        Box(
            Modifier
                .fillMaxWidth(0.86f)
                .aspectRatio(2.3f)
                .shadow(32.dp, RoundedCornerShape(16.dp), ambientColor = Color(0xFF000000), spotColor = Color(0xFF000000))
                .clip(RoundedCornerShape(16.dp))
                .background(
                    Brush.verticalGradient(listOf(c.body, c.body))
                )
                .border(1.dp, Color.White.copy(alpha = if (isClassic) 0.08f else 0.04f), RoundedCornerShape(16.dp)),
        ) {
            // Top highlight edge
            Box(
                Modifier.fillMaxWidth().height(1.dp).align(Alignment.TopCenter)
                    .background(Color.White.copy(alpha = if (isClassic) 0.12f else 0.05f))
            )

            // Face plate
            Box(
                Modifier
                    .fillMaxSize()
                    .padding(14.dp)
                    .clip(RoundedCornerShape(10.dp))
                    .background(c.face)
                    .border(0.5.dp, Color.White.copy(alpha = 0.03f), RoundedCornerShape(10.dp)),
            ) {
                // Ridges
                Ridges(c.ridge, Modifier.align(Alignment.CenterStart).width(7.dp).fillMaxHeight().padding(vertical = 12.dp))
                Ridges(c.ridge, Modifier.align(Alignment.CenterEnd).width(7.dp).fillMaxHeight().padding(vertical = 12.dp))

                // D-Pad in inlay (more left margin)
                Inlay(c, Modifier.align(Alignment.CenterStart).padding(start = 48.dp).size(140.dp)) {
                    OnePointDPad(c, 120.dp, onKey)
                }

                // Center: Logo + START/SELECT
                Column(
                    Modifier.align(Alignment.Center),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Image(
                        painter = painterResource(id = R.drawable.ic_logo_wide),
                        contentDescription = "Archipelago",
                        modifier = Modifier.width(180.dp),
                        colorFilter = ColorFilter.tint(if (isClassic) NES.ClassicLabel else c.label),
                    )
                    Spacer(Modifier.height(10.dp))
                    Inlay(c, Modifier.padding(horizontal = 4.dp)) {
                        Row(
                            Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                            horizontalArrangement = Arrangement.spacedBy(16.dp),
                        ) {
                            CapsuleBtn("SELECT", c, 64.dp, 28.dp) { onKey("Escape") }
                            CapsuleBtn("START", c, 64.dp, 28.dp) { onKey("Return") }
                        }
                    }
                }

                // A/B/C Buttons in inlay — triangle: C top, B+A bottom
                Inlay(c, Modifier.align(Alignment.CenterEnd).padding(end = 48.dp).size(140.dp)) {
                    Column(
                        Modifier.fillMaxSize(),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.Center,
                    ) {
                        // C on top
                        GlassFaceBtn("C", Color(0xFFBBBBBB), 44.dp) { onKey("c") }
                        Spacer(Modifier.height(6.dp))
                        // B + A on bottom row
                        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            GlassFaceBtn("B", Color(0xFF60A5FA), 44.dp) { onKey("b") }
                            GlassFaceBtn("A", Color(0xFFF7931A), 44.dp) { onKey("a") }
                        }
                    }
                }

                // Player toggle + settings (bottom center)
                Row(
                    Modifier.align(Alignment.BottomCenter).padding(bottom = 4.dp),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    PlayerPill(c, playerId, onPlayerToggle)
                    SettingsBtn(c, Modifier, onMenu)
                    onToggleStyle?.let { StyleBtn(c, Modifier, it) }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Shared sub-components
// ═══════════════════════════════════════════════════════════

/** Inlay well — dark recessed area with border */
@Composable
fun Inlay(c: NESPalette, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Box(
        modifier = modifier
            .clip(RoundedCornerShape(10.dp))
            .background(c.inlayBg)
            .border(3.dp, c.inlayBorder, RoundedCornerShape(10.dp))
            .padding(4.dp),
        contentAlignment = Alignment.Center,
    ) { content() }
}

/** One-piece D-pad — single cross shape, touch detects direction */
@Composable
fun OnePointDPad(c: NESPalette, size: Dp, onDir: (String) -> Unit) {
    val scope = rememberCoroutineScope()
    var job by remember { mutableStateOf<Job?>(null) }
    var activeDir by remember { mutableStateOf<String?>(null) }
    DisposableEffect(Unit) { onDispose { job?.cancel() } }

    Canvas(
        modifier = Modifier
            .size(size)
            .pointerInput(Unit) {
                detectTapGestures(
                    onPress = { offset ->
                        val cx = this@pointerInput.size.width / 2f
                        val cy = this@pointerInput.size.height / 2f
                        val dx = offset.x - cx
                        val dy = offset.y - cy
                        val dead = cx * 0.24f
                        if (abs(dx) < dead && abs(dy) < dead) {
                            tryAwaitRelease(); return@detectTapGestures
                        }
                        val dir = if (abs(dx) > abs(dy)) {
                            if (dx > 0) "Right" else "Left"
                        } else {
                            if (dy > 0) "Down" else "Up"
                        }
                        activeDir = dir; onDir(dir)
                        job?.cancel()
                        // 500ms initial delay so a normal tap sends one key, not
                        // two (a touch tap often exceeds 300ms → doubled nav sound).
                        job = scope.launch { delay(500); while (true) { onDir(dir); delay(90) } }
                        tryAwaitRelease()
                        job?.cancel(); activeDir = null
                    },
                )
            },
    ) {
        val w = size.toPx()
        val arm = w * 0.33f  // arm width = 1/3 of total
        val offset = (w - arm) / 2f

        // Cross shape
        val crossColor = c.dpad

        // Vertical bar
        drawRoundRect(
            color = crossColor,
            topLeft = Offset(offset, 0f),
            size = Size(arm, w),
            cornerRadius = CornerRadius(4.dp.toPx()),
        )
        // Horizontal bar
        drawRoundRect(
            color = crossColor,
            topLeft = Offset(0f, offset),
            size = Size(w, arm),
            cornerRadius = CornerRadius(4.dp.toPx()),
        )

        // Top-edge lighting
        drawRoundRect(
            brush = Brush.verticalGradient(listOf(Color.White.copy(alpha = 0.06f), Color.Transparent)),
            topLeft = Offset(offset, 0f),
            size = Size(arm, w * 0.15f),
            cornerRadius = CornerRadius(4.dp.toPx()),
        )
        drawRoundRect(
            brush = Brush.verticalGradient(listOf(Color.White.copy(alpha = 0.06f), Color.Transparent)),
            topLeft = Offset(0f, offset),
            size = Size(w, arm * 0.3f),
            cornerRadius = CornerRadius(4.dp.toPx()),
        )

        // Active direction highlight
        activeDir?.let { dir ->
            val hi = c.dpadHi
            when (dir) {
                "Up" -> drawRoundRect(hi, Offset(offset, 0f), Size(arm, arm), CornerRadius(4.dp.toPx()))
                "Down" -> drawRoundRect(hi, Offset(offset, w - arm), Size(arm, arm), CornerRadius(4.dp.toPx()))
                "Left" -> drawRoundRect(hi, Offset(0f, offset), Size(arm, arm), CornerRadius(4.dp.toPx()))
                "Right" -> drawRoundRect(hi, Offset(w - arm, offset), Size(arm, arm), CornerRadius(4.dp.toPx()))
            }
        }

        // Center circle
        drawCircle(c.dpadHi, radius = w * 0.06f, center = Offset(w / 2f, w / 2f))
    }
}

@Composable
fun Ridges(color: Color, modifier: Modifier) {
    Canvas(modifier = modifier) {
        val h = 1.5.dp.toPx(); val gap = 3.dp.toPx(); var y = 0f
        while (y < size.height) { drawRect(color, Offset(0f, y), Size(size.width, h)); y += h + gap }
    }
}

/** A/B round button with lighting */
@Composable
fun RoundBtn(c: NESPalette, sz: Dp = 52.dp, onClick: () -> Unit) {
    var p by remember { mutableStateOf(false) }
    Box(
        Modifier
            .size(sz)
            .shadow(if (p) 1.dp else 4.dp, CircleShape)
            .clip(CircleShape)
            .background(Brush.verticalGradient(
                if (p) listOf(c.btnPress, c.btn.copy(alpha = 0.85f))
                else listOf(c.btn, c.btn.copy(alpha = 0.8f))
            ))
            .pointerInput(Unit) { detectTapGestures(onPress = { p = true; onClick(); tryAwaitRelease(); p = false }) },
        contentAlignment = Alignment.Center,
    ) {
        if (!p) Box(Modifier.fillMaxSize().clip(CircleShape).background(
            Brush.verticalGradient(listOf(Color.White.copy(alpha = 0.18f), Color.Transparent))
        ))
    }
}

/** Colored round button — custom color instead of palette */
@Composable
fun ColorBtn(color: Color, pressColor: Color, sz: Dp = 48.dp, onClick: () -> Unit) {
    var p by remember { mutableStateOf(false) }
    Box(
        Modifier
            .size(sz)
            .shadow(if (p) 1.dp else 4.dp, CircleShape)
            .clip(CircleShape)
            .background(Brush.verticalGradient(
                if (p) listOf(pressColor, color.copy(alpha = 0.85f))
                else listOf(color, color.copy(alpha = 0.8f))
            ))
            .pointerInput(Unit) { detectTapGestures(onPress = { p = true; onClick(); tryAwaitRelease(); p = false }) },
        contentAlignment = Alignment.Center,
    ) {
        if (!p) Box(Modifier.fillMaxSize().clip(CircleShape).background(
            Brush.verticalGradient(listOf(Color.White.copy(alpha = 0.18f), Color.Transparent))
        ))
    }
}

/** Glass face button — dark translucent fill, colored ring + letter (OS style) */
@Composable
fun GlassFaceBtn(label: String, accent: Color, sz: Dp = 44.dp, onClick: () -> Unit) {
    var p by remember { mutableStateOf(false) }
    Box(
        Modifier
            .size(sz)
            .clip(CircleShape)
            .background(
                Brush.verticalGradient(
                    if (p) listOf(Color.White.copy(alpha = 0.05f), Color.White.copy(alpha = 0.02f))
                    else listOf(Color.White.copy(alpha = 0.10f), Color.White.copy(alpha = 0.03f))
                )
            )
            .border(1.5.dp, accent.copy(alpha = if (p) 0.95f else 0.55f), CircleShape)
            .pointerInput(Unit) { detectTapGestures(onPress = { p = true; onClick(); tryAwaitRelease(); p = false }) },
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = accent.copy(alpha = if (p) 1f else 0.85f), fontSize = 16.sp, fontWeight = FontWeight.Bold)
    }
}

/** START/SELECT capsule */
@Composable
fun CapsuleBtn(label: String, c: NESPalette, w: Dp = 64.dp, h: Dp = 28.dp, onClick: () -> Unit) {
    var p by remember { mutableStateOf(false) }
    Box(
        Modifier
            .width(w).height(h)
            .shadow(if (p) 0.dp else 2.dp, RoundedCornerShape(4.dp))
            .clip(RoundedCornerShape(4.dp))
            .background(Brush.verticalGradient(
                if (p) listOf(c.capsulePress, c.capsule)
                else listOf(c.capsule, c.capsule.copy(alpha = 0.85f))
            ))
            .pointerInput(Unit) { detectTapGestures(onPress = { p = true; onClick(); tryAwaitRelease(); p = false }) },
        contentAlignment = Alignment.Center,
    ) {
        if (!p) Box(Modifier.fillMaxSize().clip(RoundedCornerShape(4.dp)).background(
            Brush.verticalGradient(listOf(Color.White.copy(alpha = 0.05f), Color.Transparent))
        ))
        Text(label, color = c.labelMuted, fontSize = 8.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.sp)
    }
}

/** Settings gear button (48dp — large enough for easy tap on TV) */
@Composable
fun SettingsBtn(c: NESPalette, modifier: Modifier = Modifier, onClick: () -> Unit) {
    var p by remember { mutableStateOf(false) }
    Box(
        modifier = modifier
            .size(48.dp)
            .clip(CircleShape)
            .background(if (p) c.capsulePress else c.capsule)
            .pointerInput(Unit) { detectTapGestures(onPress = { p = true; onClick(); tryAwaitRelease(); p = false }) },
        contentAlignment = Alignment.Center,
    ) {
        Icon(Icons.Default.Settings, "Settings", Modifier.size(28.dp), tint = c.labelMuted)
    }
}

/** Dark/Classic style toggle — lives next to the settings gear (the menu hub
 *  no longer carries it). */
@Composable
fun StyleBtn(c: NESPalette, modifier: Modifier = Modifier, onClick: () -> Unit) {
    var p by remember { mutableStateOf(false) }
    Box(
        modifier = modifier
            .size(48.dp)
            .clip(CircleShape)
            .background(if (p) c.capsulePress else c.capsule)
            .pointerInput(Unit) { detectTapGestures(onPress = { p = true; onClick(); tryAwaitRelease(); p = false }) },
        contentAlignment = Alignment.Center,
    ) {
        Icon(Icons.Default.Palette, "Controller style", Modifier.size(26.dp), tint = c.labelMuted)
    }
}

/** Player ID toggle pill (P1/P2/ALL) */
@Composable
fun PlayerPill(c: NESPalette, playerId: Int, onToggle: () -> Unit) {
    val label = when (playerId) { 1 -> "P1"; 2 -> "P2"; else -> "ALL" }
    val accent = when (playerId) { 1 -> Color(0xFF00F0FF); 2 -> Color(0xFFFF0080); else -> c.labelMuted }
    var p by remember { mutableStateOf(false) }
    Box(
        modifier = Modifier
            .height(28.dp)
            .width(44.dp)
            .clip(RoundedCornerShape(6.dp))
            .background(if (p) c.capsulePress else c.capsule)
            .border(1.dp, accent.copy(alpha = 0.5f), RoundedCornerShape(6.dp))
            .pointerInput(Unit) { detectTapGestures(onPress = { p = true; onToggle(); tryAwaitRelease(); p = false }) },
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = accent, fontSize = 10.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.sp)
    }
}

/** Three-finger hold gesture modifier (two fingers stay free for scrolling) */
fun Modifier.threeFingerHold(onHold: () -> Unit) = this.pointerInput(Unit) {
    awaitEachGesture {
        awaitFirstDown(requireUnconsumed = false)
        var t = 0L; var fired = false
        do {
            val ev = awaitPointerEvent()
            val a = ev.changes.filter { !it.changedToUp() }
            if (a.size >= 3 && t == 0L) t = System.currentTimeMillis()
            if (a.size >= 3 && !fired && t > 0 && System.currentTimeMillis() - t > 500) { fired = true; onHold() }
            if (a.size < 3) t = 0L
        } while (ev.changes.any { it.pressed })
    }
}

