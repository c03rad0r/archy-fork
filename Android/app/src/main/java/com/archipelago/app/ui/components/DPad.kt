package com.archipelago.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material3.Icon
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
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import com.archipelago.app.ui.theme.BitcoinOrange
import com.archipelago.app.ui.theme.Neo
import com.archipelago.app.ui.theme.neoInset
import com.archipelago.app.ui.theme.neoRaised
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private val BTN = 50.dp
private val BTN_R = 12.dp
private val GAP = 8.dp
private val NOB = 24.dp

@Composable
fun DPad(
    onDirection: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val surface = Neo.surface()
    val raised = Neo.surfaceRaised()
    val l = Neo.shadowLight()
    val d = Neo.shadowDark()

    // Recessed well
    Box(
        modifier = modifier
            .neoInset(l, d, 20.dp, 2.dp, 4.dp)
            .clip(RoundedCornerShape(20.dp))
            .background(surface)
            .padding(14.dp),
        contentAlignment = Alignment.Center,
    ) {
        // Cross layout with explicit spacing
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Btn(Icons.Default.KeyboardArrowUp, "Up", onDirection)
            Box(modifier = Modifier.size(height = GAP, width = BTN)) // spacer
            Row(verticalAlignment = Alignment.CenterVertically) {
                Btn(Icons.AutoMirrored.Filled.KeyboardArrowLeft, "Left", onDirection)
                Box(modifier = Modifier.size(width = GAP, height = BTN)) // spacer
                // Center nob
                Box(
                    modifier = Modifier
                        .size(NOB)
                        .neoRaised(l, d, NOB / 2, 1.dp, 2.dp)
                        .clip(CircleShape)
                        .background(raised),
                    contentAlignment = Alignment.Center,
                ) {
                    Box(Modifier.size(8.dp).clip(CircleShape).background(BitcoinOrange.copy(alpha = 0.15f)))
                }
                Box(modifier = Modifier.size(width = GAP, height = BTN)) // spacer
                Btn(Icons.AutoMirrored.Filled.KeyboardArrowRight, "Right", onDirection)
            }
            Box(modifier = Modifier.size(height = GAP, width = BTN)) // spacer
            Btn(Icons.Default.KeyboardArrowDown, "Down", onDirection)
        }
    }
}

@Composable
private fun Btn(icon: ImageVector, key: String, onDir: (String) -> Unit) {
    val scope = rememberCoroutineScope()
    var job by remember { mutableStateOf<Job?>(null) }
    var p by remember { mutableStateOf(false) }
    val bg = Neo.surfaceRaised()
    val l = Neo.shadowLight()
    val d = Neo.shadowDark()
    val tint = Neo.textPrimary()
    DisposableEffect(Unit) { onDispose { job?.cancel() } }

    Box(
        modifier = Modifier
            .size(BTN)
            .then(if (p) Modifier.neoInset(l, d, BTN_R, 1.dp, 2.dp) else Modifier.neoRaised(l, d, BTN_R, 2.dp, 4.dp))
            .clip(RoundedCornerShape(BTN_R))
            .background(bg)
            .pointerInput(key) {
                detectTapGestures(onPress = {
                    p = true; onDir(key)
                    // 500ms initial delay so a normal tap sends one key, not two
                    // (a touch tap often exceeds 350ms → doubled nav sound).
                    job = scope.launch { delay(500); while (true) { onDir(key); delay(100) } }
                    tryAwaitRelease(); p = false; job?.cancel()
                })
            },
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, key, Modifier.fillMaxSize(0.48f), tint = if (p) tint.copy(alpha = 0.9f) else tint.copy(alpha = 0.5f))
    }
}
