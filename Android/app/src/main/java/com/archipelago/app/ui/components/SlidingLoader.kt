package com.archipelago.app.ui.components

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.keyframes
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.archipelago.app.ui.theme.BitcoinOrange

/** green-400 — the same "done" colour the web install overlay lands on. */
private val DoneGreen = Color(0xFF4ADE80)

/**
 * The Archipelago loading bar: a stripe that runs side to side inside a dim
 * track and lands as a solid green bar when the work completes.
 *
 * This is a direct port of the platform's install-progress overlay
 * (neode-ui SystemUpdate.vue `.install-overlay-bar-anim`): a third-width
 * orange stripe on a white/10 track, 1.8s ease-in-out, going full green on
 * success. Using the same loader natively is what makes the companion feel
 * like the same product as the node UI rather than a stock Android app.
 *
 * @param done      finished successfully — the bar fills solid green.
 * @param stalled   waiting on the user / something external — the bar parks
 *                  half-full in a dimmed orange instead of animating, so it
 *                  reads as "this needs you", not "still working".
 */
@Composable
fun SlidingLoader(
    modifier: Modifier = Modifier,
    done: Boolean = false,
    stalled: Boolean = false,
    height: Dp = 8.dp,
) {
    val doneProgress by animateFloatAsState(
        targetValue = if (done) 1f else 0f,
        animationSpec = tween(320),
        label = "loaderDone",
    )

    BoxWithConstraints(
        modifier
            .fillMaxWidth()
            .height(height)
            .clip(RoundedCornerShape(percent = 50))
            .background(Color.White.copy(alpha = 0.10f)),
    ) {
        val trackWidth = maxWidth
        val stripeWidth = trackWidth / 3
        val stripePx = with(LocalDensity.current) { stripeWidth.toPx() }

        if (doneProgress < 1f) {
            if (stalled) {
                Box(
                    Modifier
                        .fillMaxWidth(0.5f)
                        .fillMaxHeight()
                        .clip(RoundedCornerShape(percent = 50))
                        .background(BitcoinOrange.copy(alpha = 0.6f)),
                )
            } else {
                // Keyframes copied from the web overlay: -100% → 120% → 300%
                // of the STRIPE's own width, which is what gives the bar its
                // fast sweep out and lazy re-entry.
                val transition = rememberInfiniteTransition(label = "loaderSlide")
                val offset by transition.animateFloat(
                    initialValue = -1f,
                    targetValue = 3f,
                    animationSpec = infiniteRepeatable(
                        animation = keyframes {
                            durationMillis = 1800
                            (-1f) at 0
                            1.2f at 900
                            3f at 1800
                        },
                        repeatMode = RepeatMode.Restart,
                    ),
                    label = "loaderOffset",
                )
                Box(
                    Modifier
                        .fillMaxWidth(1f / 3f)
                        .fillMaxHeight()
                        .graphicsLayer { translationX = offset * stripePx }
                        .clip(RoundedCornerShape(percent = 50))
                        .background(BitcoinOrange),
                )
            }
        }

        if (doneProgress > 0f) {
            Box(
                Modifier
                    .fillMaxWidth()
                    .fillMaxHeight()
                    .graphicsLayer { alpha = doneProgress }
                    .background(DoneGreen),
            )
        }
    }
}
