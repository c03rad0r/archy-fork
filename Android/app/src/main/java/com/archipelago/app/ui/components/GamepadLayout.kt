package com.archipelago.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.input.pointer.changedToUp
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.archipelago.app.ui.theme.BitcoinOrange
import com.archipelago.app.ui.theme.Neo
import com.archipelago.app.ui.theme.neoInset
import com.archipelago.app.ui.theme.neoRaised

@Composable
fun GamepadLayout(
    onKey: (String) -> Unit,
    onThreeFingerHold: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val surface = Neo.surface()

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(surface)
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    var t = 0L; var fired = false
                    do {
                        val ev = awaitPointerEvent()
                        val a = ev.changes.filter { !it.changedToUp() }
                        if (a.size >= 3 && t == 0L) t = System.currentTimeMillis()
                        if (a.size >= 3 && !fired && t > 0 && System.currentTimeMillis() - t > 500) { fired = true; onThreeFingerHold() }
                        if (a.size < 3) t = 0L
                    } while (ev.changes.any { it.pressed })
                }
            }
            .padding(horizontal = 24.dp, vertical = 16.dp),
    ) {
        // D-pad — centered left
        DPad(
            onDirection = onKey,
            modifier = Modifier.align(Alignment.CenterStart).size(200.dp),
        )

        // Face buttons — centered right (diamond)
        Column(
            modifier = Modifier.align(Alignment.CenterEnd),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            FaceBtn("esc", 64.dp) { onKey("Escape") }
            Row(horizontalArrangement = Arrangement.spacedBy(28.dp)) {
                FaceBtn("tab", 64.dp) { onKey("Tab") }
                FaceBtn("enter", 64.dp, accent = true) { onKey("Return") }
            }
            FaceBtn("bksp", 64.dp) { onKey("BackSpace") }
        }

        // Bottom: L, SELECT, START, R
        Row(
            modifier = Modifier.align(Alignment.BottomCenter),
            horizontalArrangement = Arrangement.spacedBy(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            PillBtn("L", 56.dp) { onKey("Prior") }
            PillBtn("SELECT", 80.dp) { onKey("Escape") }
            PillBtn("START", 80.dp) { onKey("Return") }
            PillBtn("R", 56.dp) { onKey("Next") }
        }
    }
}

@Composable
private fun FaceBtn(label: String, size: Dp, accent: Boolean = false, onClick: () -> Unit) {
    var p by remember { mutableStateOf(false) }
    val l = Neo.shadowLight(); val d = Neo.shadowDark()
    val tc = if (accent) BitcoinOrange.copy(alpha = 0.7f) else Neo.textSecondary()

    Box(
        modifier = Modifier
            .size(size)
            .then(if (p) Modifier.neoInset(l, d, size / 2, 1.dp, 3.dp) else Modifier.neoRaised(l, d, size / 2, 2.dp, 4.dp))
            .clip(CircleShape)
            .background(Neo.surfaceRaised())
            .pointerInput(Unit) { detectTapGestures(onPress = { p = true; onClick(); tryAwaitRelease(); p = false }) },
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = if (p) tc.copy(alpha = 1f) else tc, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 0.5.sp)
    }
}

@Composable
private fun PillBtn(label: String, w: Dp, onClick: () -> Unit) {
    var p by remember { mutableStateOf(false) }
    val l = Neo.shadowLight(); val d = Neo.shadowDark()

    Box(
        modifier = Modifier
            .width(w).height(34.dp)
            .then(if (p) Modifier.neoInset(l, d, 8.dp, 1.dp, 2.dp) else Modifier.neoRaised(l, d, 8.dp, 2.dp, 4.dp))
            .clip(RoundedCornerShape(8.dp))
            .background(Neo.surfaceRaised())
            .pointerInput(Unit) { detectTapGestures(onPress = { p = true; onClick(); tryAwaitRelease(); p = false }) },
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = Neo.textMuted(), fontSize = 9.sp, fontWeight = FontWeight.Medium, letterSpacing = 1.sp)
    }
}
