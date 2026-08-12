package com.archipelago.app.ui.components

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.archipelago.app.R
import com.archipelago.app.ui.theme.BitcoinOrange
import kotlinx.coroutines.delay

/**
 * First-launch teaching overlay for the three-finger hold gesture. Three
 * fingertip dots pulse in a "press" rhythm with an expanding ring while a
 * short caption explains what the gesture opens. Dismissed by tapping
 * anywhere (or automatically after a few seconds) — shown once, ever.
 */
@Composable
fun GestureHintOverlay(onDismiss: () -> Unit) {
    // Auto-dismiss so a user who taps nothing is never stuck behind the scrim.
    LaunchedEffect(Unit) {
        delay(6500)
        onDismiss()
    }

    val transition = rememberInfiniteTransition(label = "gesture-hint")
    // Fingertips press down together…
    val press by transition.animateFloat(
        initialValue = 1f,
        targetValue = 0.86f,
        animationSpec = infiniteRepeatable(tween(650), RepeatMode.Reverse),
        label = "press",
    )
    // …while a ring ripples outward on each press cycle.
    val ripple by transition.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(tween(1300, easing = LinearEasing)),
        label = "ripple",
    )

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.72f))
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onDismiss,
            ),
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            // Hand: three fingertip dots in a natural arc + ripple ring.
            Box(Modifier.size(160.dp), contentAlignment = Alignment.Center) {
                Box(
                    Modifier
                        .size(150.dp)
                        .scale(0.4f + ripple * 0.6f)
                        .border(
                            2.dp,
                            BitcoinOrange.copy(alpha = (1f - ripple) * 0.8f),
                            CircleShape,
                        ),
                )
                FingerDot(x = (-44).dp, y = 14.dp, scale = press)
                FingerDot(x = 0.dp, y = (-12).dp, scale = press)
                FingerDot(x = 44.dp, y = 8.dp, scale = press)
            }

            Spacer(Modifier.height(28.dp))

            Text(
                text = stringResource(R.string.gesture_hint_title),
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.Bold,
                color = Color.White,
                textAlign = TextAlign.Center,
            )
            Spacer(Modifier.height(10.dp))
            Text(
                text = stringResource(R.string.gesture_hint_body),
                style = MaterialTheme.typography.bodyMedium,
                color = Color.White.copy(alpha = 0.7f),
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(horizontal = 48.dp),
            )

            Spacer(Modifier.height(32.dp))

            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(12.dp))
                    .background(Color.White.copy(alpha = 0.12f))
                    .clickable(onClick = onDismiss)
                    .padding(horizontal = 28.dp, vertical = 12.dp),
            ) {
                Text(
                    text = stringResource(R.string.gesture_hint_got_it),
                    style = MaterialTheme.typography.labelLarge,
                    color = Color.White,
                )
            }
        }
    }
}

@Composable
private fun FingerDot(x: androidx.compose.ui.unit.Dp, y: androidx.compose.ui.unit.Dp, scale: Float) {
    Box(
        Modifier
            .offset(x = x, y = y)
            .size(26.dp)
            .scale(scale)
            .background(Color.White.copy(alpha = 0.92f), CircleShape),
    )
}
