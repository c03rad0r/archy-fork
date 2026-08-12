package com.archipelago.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
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
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.archipelago.app.ui.theme.BitcoinOrange
import com.archipelago.app.ui.theme.Neo
import com.archipelago.app.ui.theme.neoInset
import com.archipelago.app.ui.theme.neoRaised

private val R = 14.dp

@Composable
fun ActionButtons(
    onEscape: () -> Unit,
    onEnter: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(10.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        NeoBtn("ESC", Neo.textSecondary(), Modifier.fillMaxWidth().weight(1f), onEscape)
        NeoBtn("ENTER", BitcoinOrange.copy(alpha = 0.7f), Modifier.fillMaxWidth().weight(1f), onEnter)
    }
}

@Composable
private fun NeoBtn(label: String, color: androidx.compose.ui.graphics.Color, modifier: Modifier, onClick: () -> Unit) {
    var p by remember { mutableStateOf(false) }
    val l = Neo.shadowLight(); val d = Neo.shadowDark()
    Box(
        modifier = modifier
            .then(if (p) Modifier.neoInset(l, d, R, 1.dp, 2.dp) else Modifier.neoRaised(l, d, R, 2.dp, 4.dp))
            .clip(RoundedCornerShape(R))
            .background(Neo.surfaceRaised())
            .pointerInput(Unit) { detectTapGestures(onPress = { p = true; onClick(); tryAwaitRelease(); p = false }) },
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = if (p) color else color.copy(alpha = 0.7f), fontSize = 12.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.5.sp)
    }
}
