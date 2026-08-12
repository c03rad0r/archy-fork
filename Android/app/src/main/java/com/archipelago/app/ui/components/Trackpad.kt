package com.archipelago.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.changedToUp
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.unit.dp
import com.archipelago.app.ui.theme.Neo
import com.archipelago.app.ui.theme.neoInset

private const val TAP_THRESHOLD = 12f
private const val TAP_TIMEOUT = 250L

@Composable
fun Trackpad(
    onMove: (dx: Int, dy: Int) -> Unit,
    onClick: (button: Int) -> Unit,
    onScroll: (dy: Int) -> Unit,
    onThreeFingerHold: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var fingers by remember { mutableIntStateOf(0) }
    val surface = Neo.surface()
    val light = Neo.shadowLight()
    val dark = Neo.shadowDark()
    val muted = Neo.textMuted()

    Box(
        modifier = modifier
            .neoInset(light, dark, 20.dp, 3.dp, 6.dp)
            .clip(RoundedCornerShape(20.dp))
            .background(surface)
            .pointerInput(Unit) {
                awaitEachGesture {
                    val first = awaitFirstDown(requireUnconsumed = false)
                    var total = Offset.Zero
                    val t0 = System.currentTimeMillis()
                    var maxPtrs = 1
                    var holdFired = false
                    var threeStart = 0L
                    var scrollAcc = 0f
                    fingers = 1

                    do {
                        val ev = awaitPointerEvent()
                        val active = ev.changes.filter { !it.changedToUp() }
                        maxPtrs = maxOf(maxPtrs, active.size)
                        fingers = active.size

                        when {
                            // Three fingers = hold for menu; two = scroll. Kept
                            // on separate counts so a long two-finger scroll can
                            // never fire the menu mid-gesture.
                            active.size >= 3 -> {
                                if (threeStart == 0L) threeStart = System.currentTimeMillis()
                                if (!holdFired && System.currentTimeMillis() - threeStart > 500) {
                                    holdFired = true
                                    onThreeFingerHold()
                                }
                                ev.changes.forEach { it.consume() }
                            }
                            active.size == 2 -> {
                                threeStart = 0L
                                val dy = active.map { it.positionChange().y }.average().toFloat()
                                scrollAcc += dy
                                if (kotlin.math.abs(scrollAcc) > 12f) {
                                    onScroll(if (scrollAcc > 0) 1 else -1)
                                    scrollAcc = 0f
                                }
                                ev.changes.forEach { it.consume() }
                            }
                            active.size == 1 && maxPtrs == 1 -> {
                                val d = active.first().positionChange()
                                total += d
                                if (d != Offset.Zero) onMove(d.x.toInt(), d.y.toInt())
                                active.first().consume()
                            }
                        }
                    } while (ev.changes.any { it.pressed })

                    fingers = 0
                    val elapsed = System.currentTimeMillis() - t0
                    if (maxPtrs == 1 && elapsed < TAP_TIMEOUT && total.getDistance() < TAP_THRESHOLD) {
                        onClick(1)
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = when {
                fingers >= 3 -> "hold for menu"
                fingers == 2 -> "scroll"
                else -> ""
            },
            style = MaterialTheme.typography.labelSmall,
            color = muted.copy(alpha = 0.4f),
        )
    }
}
