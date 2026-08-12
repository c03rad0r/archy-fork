package com.archipelago.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.archipelago.app.ui.theme.BitcoinOrange
import com.archipelago.app.ui.theme.Neo
import com.archipelago.app.ui.theme.neoInset
import com.archipelago.app.ui.theme.neoRaised
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private enum class Layer { ALPHA, NUM, SYM }
private val KEY_H = 46.dp
private val KEY_R = 10.dp
private val GAP = 5.dp

@Composable
fun VirtualKeyboard(onKey: (String) -> Unit, modifier: Modifier = Modifier) {
    var layer by remember { mutableStateOf(Layer.ALPHA) }
    var shifted by remember { mutableStateOf(false) }
    var capsLock by remember { mutableStateOf(false) }
    val up = shifted || capsLock

    fun emit(k: String) { onKey(k); if (shifted && !capsLock) shifted = false }
    fun ch(c: String) { emit(if (up && layer == Layer.ALPHA) "shift+$c" else c) }

    Column(
        modifier = modifier.background(Neo.surface()).padding(horizontal = 6.dp, vertical = 6.dp),
        verticalArrangement = Arrangement.spacedBy(GAP),
    ) {
        when (layer) {
            Layer.ALPHA -> {
                CRow("q w e r t y u i o p".split(" "), up, ::ch)
                CRow("a s d f g h j k l".split(" "), up, ::ch, inset = 18.dp)
                Row(Modifier.fillMaxWidth().height(KEY_H), Arrangement.spacedBy(GAP)) {
                    SKey(if (capsLock) "\u21EA" else "\u21E7", Modifier.weight(1.4f), active = up) {
                        if (capsLock) { capsLock = false; shifted = false } else if (shifted) capsLock = true else shifted = true
                    }
                    "z x c v b n m".split(" ").forEach { c -> CKey(if (up) c.uppercase() else c, Modifier.weight(1f)) { ch(c) } }
                    RKey("\u232B", Modifier.weight(1.4f)) { emit("BackSpace") }
                }
            }
            Layer.NUM -> {
                SRow("1 2 3 4 5 6 7 8 9 0".split(" "), ::emit)
                SRow("- / : ; ( ) \$ & @ \"".split(" "), ::emit)
                Row(Modifier.fillMaxWidth().height(KEY_H), Arrangement.spacedBy(GAP)) {
                    SKey("#+=", Modifier.weight(1.4f)) { layer = Layer.SYM }
                    ". , ? ! '".split(" ").forEach { c -> CKey(c, Modifier.weight(1f)) { emit(c) } }
                    RKey("\u232B", Modifier.weight(1.4f)) { emit("BackSpace") }
                }
            }
            Layer.SYM -> {
                SRow("[ ] { } # % ^ * + =".split(" "), ::emit)
                SRow("_ \\ | ~ < > ` @ !".split(" "), ::emit)
                Row(Modifier.fillMaxWidth().height(KEY_H), Arrangement.spacedBy(GAP)) {
                    SKey("123", Modifier.weight(1.4f)) { layer = Layer.NUM }
                    ". , ? ! '".split(" ").forEach { c -> CKey(c, Modifier.weight(1f)) { emit(c) } }
                    RKey("\u232B", Modifier.weight(1.4f)) { emit("BackSpace") }
                }
            }
        }
        Row(Modifier.fillMaxWidth().height(KEY_H), Arrangement.spacedBy(GAP)) {
            SKey(if (layer == Layer.ALPHA) "123" else "ABC", Modifier.weight(1.4f)) {
                layer = if (layer == Layer.ALPHA) Layer.NUM else Layer.ALPHA; shifted = false; capsLock = false
            }
            CKey(",", Modifier.weight(1f)) { emit("comma") }
            CKey("space", Modifier.weight(5f), fontSize = 13) { emit("space") }
            CKey(".", Modifier.weight(1f)) { emit("period") }
            AKey("\u23CE", Modifier.weight(1.4f)) { emit("Return") }
        }
    }
}

@Composable
private fun CRow(keys: List<String>, up: Boolean, onKey: (String) -> Unit, inset: Dp = 0.dp) {
    Row(Modifier.fillMaxWidth().height(KEY_H).padding(horizontal = inset), Arrangement.spacedBy(GAP)) {
        keys.forEach { c -> CKey(if (up) c.uppercase() else c, Modifier.weight(1f)) { onKey(c) } }
    }
}
@Composable
private fun SRow(keys: List<String>, onKey: (String) -> Unit) {
    Row(Modifier.fillMaxWidth().height(KEY_H), Arrangement.spacedBy(GAP)) {
        keys.forEach { c -> CKey(c, Modifier.weight(1f)) { onKey(c) } }
    }
}

/** Character key */
@Composable
private fun CKey(label: String, modifier: Modifier = Modifier, fontSize: Int = 19, onTap: () -> Unit) {
    var p by remember { mutableStateOf(false) }
    val bg = Neo.surfaceRaised(); val l = Neo.shadowLight(); val d = Neo.shadowDark(); val t = Neo.textPrimary()
    Box(
        modifier = modifier.height(KEY_H)
            .then(if (p) Modifier.neoInset(l, d, KEY_R) else Modifier.neoRaised(l, d, KEY_R))
            .clip(RoundedCornerShape(KEY_R)).background(bg)
            .pointerInput(label) { detectTapGestures(onPress = { p = true; onTap(); tryAwaitRelease(); p = false }) },
        contentAlignment = Alignment.Center,
    ) { Text(label, color = t.copy(alpha = if (p) 0.9f else 0.7f), fontSize = fontSize.sp, textAlign = TextAlign.Center, maxLines = 1) }
}

/** Special key */
@Composable
private fun SKey(label: String, modifier: Modifier = Modifier, active: Boolean = false, onTap: () -> Unit) {
    var p by remember { mutableStateOf(false) }
    val bg = Neo.surfaceRaised(); val l = Neo.shadowLight(); val d = Neo.shadowDark()
    val tc = if (active) BitcoinOrange.copy(alpha = 0.8f) else Neo.textSecondary()
    Box(
        modifier = modifier.height(KEY_H)
            .then(if (p) Modifier.neoInset(l, d, KEY_R) else Modifier.neoRaised(l, d, KEY_R))
            .clip(RoundedCornerShape(KEY_R)).background(bg)
            .pointerInput(label) { detectTapGestures(onPress = { p = true; onTap(); tryAwaitRelease(); p = false }) },
        contentAlignment = Alignment.Center,
    ) { Text(label, color = tc, fontSize = 14.sp, fontWeight = FontWeight.Medium, textAlign = TextAlign.Center) }
}

/** Accent key (return) */
@Composable
private fun AKey(label: String, modifier: Modifier = Modifier, onTap: () -> Unit) {
    var p by remember { mutableStateOf(false) }
    val l = Neo.shadowLight(); val d = Neo.shadowDark()
    Box(
        modifier = modifier.height(KEY_H)
            .then(if (p) Modifier.neoInset(l, d, KEY_R) else Modifier.neoRaised(l, d, KEY_R))
            .clip(RoundedCornerShape(KEY_R)).background(Neo.surfaceRaised())
            .pointerInput(Unit) { detectTapGestures(onPress = { p = true; onTap(); tryAwaitRelease(); p = false }) },
        contentAlignment = Alignment.Center,
    ) { Text(label, color = BitcoinOrange.copy(alpha = 0.7f), fontSize = 17.sp, fontWeight = FontWeight.Bold) }
}

/** Repeatable key (backspace) */
@Composable
private fun RKey(label: String, modifier: Modifier = Modifier, onTap: () -> Unit) {
    var p by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope(); var job by remember { mutableStateOf<Job?>(null) }
    val l = Neo.shadowLight(); val d = Neo.shadowDark()
    DisposableEffect(Unit) { onDispose { job?.cancel() } }
    Box(
        modifier = modifier.height(KEY_H)
            .then(if (p) Modifier.neoInset(l, d, KEY_R) else Modifier.neoRaised(l, d, KEY_R))
            .clip(RoundedCornerShape(KEY_R)).background(Neo.surfaceRaised())
            .pointerInput(Unit) { detectTapGestures(onPress = {
                p = true; onTap(); job = scope.launch { delay(400); while (true) { onTap(); delay(55) } }
                tryAwaitRelease(); job?.cancel(); p = false
            }) },
        contentAlignment = Alignment.Center,
    ) { Text(label, color = Neo.textSecondary(), fontSize = 17.sp) }
}
