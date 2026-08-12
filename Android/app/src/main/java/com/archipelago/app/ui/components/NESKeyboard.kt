package com.archipelago.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
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
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.archipelago.app.ui.theme.ControllerStyle
import com.archipelago.app.ui.theme.NES
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private enum class NKLayer { ALPHA, NUM, SYM }
private val KEY_H = 42.dp
private val GAP = 4.dp

@Composable
fun NESKeyboard(
    style: ControllerStyle = ControllerStyle.CLASSIC,
    onKey: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = paletteFor(style)
    val isClassic = style == ControllerStyle.CLASSIC
    val keyBg = c.dpad
    val keyBgP = c.dpadHi
    val keyTxt = c.labelMuted
    val accent = if (isClassic) NES.ClassicLabel else c.labelMuted

    var layer by remember { mutableStateOf(NKLayer.ALPHA) }
    var shifted by remember { mutableStateOf(false) }
    var capsLock by remember { mutableStateOf(false) }
    var ctrlHeld by remember { mutableStateOf(false) }
    val up = shifted || capsLock

    fun emit(k: String) {
        val key = if (ctrlHeld) "ctrl+$k" else k
        onKey(key)
        if (shifted && !capsLock) shifted = false
        if (ctrlHeld) ctrlHeld = false
    }
    fun ch(cc: String) { emit(if (up && layer == NKLayer.ALPHA) "shift+$cc" else cc) }

    // NES body wrapping keyboard
    Column(
        modifier = modifier
            .clip(RoundedCornerShape(14.dp))
            .background(c.body)
            .padding(8.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(c.face)
            .padding(horizontal = 8.dp, vertical = 6.dp),
        verticalArrangement = Arrangement.spacedBy(GAP),
    ) {
        when (layer) {
            NKLayer.ALPHA -> {
                KeyRow("q w e r t y u i o p".split(" "), up, keyBg, keyBgP, keyTxt, ::ch)
                KeyRow("a s d f g h j k l".split(" "), up, keyBg, keyBgP, keyTxt, ::ch, inset = 16.dp)
                Row(Modifier.fillMaxWidth().height(KEY_H), Arrangement.spacedBy(GAP)) {
                    NKey(if (capsLock) "\u21EA" else "\u21E7", Modifier.weight(1.4f), keyBg, keyBgP, if (up) accent else keyTxt) {
                        if (capsLock) { capsLock = false; shifted = false } else if (shifted) capsLock = true else shifted = true
                    }
                    "z x c v b n m".split(" ").forEach { k ->
                        NKey(if (up) k.uppercase() else k, Modifier.weight(1f), keyBg, keyBgP, keyTxt, 17) { ch(k) }
                    }
                    NRepKey("\u232B", Modifier.weight(1.4f), keyBg, keyBgP, keyTxt) { emit("BackSpace") }
                }
            }
            NKLayer.NUM -> {
                KeyRow("1 2 3 4 5 6 7 8 9 0".split(" "), false, keyBg, keyBgP, keyTxt, ::emit)
                KeyRow("- / : ; ( ) \$ & @ \"".split(" "), false, keyBg, keyBgP, keyTxt, ::emit)
                Row(Modifier.fillMaxWidth().height(KEY_H), Arrangement.spacedBy(GAP)) {
                    NKey("#+=", Modifier.weight(1.4f), keyBg, keyBgP, keyTxt) { layer = NKLayer.SYM }
                    ". , ? ! '".split(" ").forEach { k ->
                        NKey(k, Modifier.weight(1f), keyBg, keyBgP, keyTxt) { emit(k) }
                    }
                    NRepKey("\u232B", Modifier.weight(1.4f), keyBg, keyBgP, keyTxt) { emit("BackSpace") }
                }
            }
            NKLayer.SYM -> {
                KeyRow("[ ] { } # % ^ * + =".split(" "), false, keyBg, keyBgP, keyTxt, ::emit)
                KeyRow("_ \\ | ~ < > ` @ !".split(" "), false, keyBg, keyBgP, keyTxt, ::emit)
                Row(Modifier.fillMaxWidth().height(KEY_H), Arrangement.spacedBy(GAP)) {
                    NKey("123", Modifier.weight(1.4f), keyBg, keyBgP, keyTxt) { layer = NKLayer.NUM }
                    ". , ? ! '".split(" ").forEach { k ->
                        NKey(k, Modifier.weight(1f), keyBg, keyBgP, keyTxt) { emit(k) }
                    }
                    NRepKey("\u232B", Modifier.weight(1.4f), keyBg, keyBgP, keyTxt) { emit("BackSpace") }
                }
            }
        }
        // Bottom row
        Row(Modifier.fillMaxWidth().height(KEY_H), Arrangement.spacedBy(GAP)) {
            NKey(if (layer == NKLayer.ALPHA) "123" else "ABC", Modifier.weight(1.4f), keyBg, keyBgP, keyTxt) {
                layer = if (layer == NKLayer.ALPHA) NKLayer.NUM else NKLayer.ALPHA; shifted = false; capsLock = false
            }
            NKey("Ctrl", Modifier.weight(1.2f), keyBg, keyBgP, if (ctrlHeld) accent else keyTxt, 11) {
                ctrlHeld = !ctrlHeld
            }
            NKey(",", Modifier.weight(0.8f), keyBg, keyBgP, keyTxt) { emit("comma") }
            NKey("space", Modifier.weight(4f), keyBg, keyBgP, keyTxt, 12) { emit("space") }
            NKey(".", Modifier.weight(0.8f), keyBg, keyBgP, keyTxt) { emit("period") }
            NKey("\u23CE", Modifier.weight(1.4f), keyBg, keyBgP, accent, 15) { emit("Return") }
        }
    }
}

/** Key row — each key gets equal weight */
@Composable
private fun KeyRow(
    keys: List<String>, up: Boolean,
    bg: Color, bgP: Color, txt: Color,
    onKey: (String) -> Unit, inset: Dp = 0.dp,
) {
    Row(
        Modifier.fillMaxWidth().height(KEY_H).padding(horizontal = inset),
        Arrangement.spacedBy(GAP),
    ) {
        keys.forEach { k ->
            NKey(
                label = if (up) k.uppercase() else k,
                modifier = Modifier.weight(1f),
                bg = bg, bgP = bgP, txt = txt,
                fontSize = 17,
                onTap = { onKey(k) },
            )
        }
    }
}

/** Single NES key — D-pad style flat dark button */
@Composable
private fun NKey(
    label: String, modifier: Modifier = Modifier,
    bg: Color, bgP: Color, txt: Color,
    fontSize: Int = 13, onTap: () -> Unit,
) {
    var p by remember { mutableStateOf(false) }
    Box(
        modifier = modifier
            .height(KEY_H)
            .clip(RoundedCornerShape(4.dp))
            .background(Brush.verticalGradient(if (p) listOf(bgP, bg) else listOf(bg, bg.copy(alpha = 0.9f))))
            .then(
                if (!p) Modifier.border(0.5.dp,
                    Brush.verticalGradient(listOf(Color.White.copy(alpha = 0.06f), Color.Transparent)),
                    RoundedCornerShape(4.dp))
                else Modifier
            )
            .pointerInput(label) {
                detectTapGestures(onPress = { p = true; onTap(); tryAwaitRelease(); p = false })
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = txt, fontSize = fontSize.sp, textAlign = TextAlign.Center, maxLines = 1)
    }
}

/** Repeatable NES key (backspace) */
@Composable
private fun NRepKey(
    label: String, modifier: Modifier,
    bg: Color, bgP: Color, txt: Color, onTap: () -> Unit,
) {
    var p by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    var job by remember { mutableStateOf<Job?>(null) }
    DisposableEffect(Unit) { onDispose { job?.cancel() } }

    Box(
        modifier = modifier
            .height(KEY_H)
            .clip(RoundedCornerShape(4.dp))
            .background(Brush.verticalGradient(if (p) listOf(bgP, bg) else listOf(bg, bg.copy(alpha = 0.9f))))
            .pointerInput(Unit) {
                detectTapGestures(onPress = {
                    p = true; onTap()
                    job = scope.launch { delay(400); while (true) { onTap(); delay(55) } }
                    tryAwaitRelease(); job?.cancel(); p = false
                })
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = txt, fontSize = 16.sp)
    }
}
