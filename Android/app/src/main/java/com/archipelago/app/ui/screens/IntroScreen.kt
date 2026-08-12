package com.archipelago.app.ui.screens

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.slideInVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.archipelago.app.R
import com.archipelago.app.ui.theme.SurfaceBlack
import com.archipelago.app.ui.theme.TextMuted
import com.archipelago.app.ui.theme.TextPrimary
import kotlinx.coroutines.delay

@Composable
fun IntroScreen(
    onContinue: () -> Unit,
    // Mesh Party works with no node at all (phone↔phone) — offered right on
    // the first screen so a friend who just got the app can join a party.
    onMeshParty: () -> Unit = {},
) {
    val logoAlpha = remember { Animatable(0f) }
    var showContent by remember { mutableStateOf(false) }

    LaunchedEffect(Unit) {
        // Content fades in WITH the logo, not after it — the serial
        // 800ms + 300ms sequence held "Get Started" off-screen for 1.1s.
        showContent = true
        logoAlpha.animateTo(1f, animationSpec = tween(450))
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(SurfaceBlack),
    ) {
        // Reddish synthwave backdrop
        Image(
            painter = painterResource(id = R.drawable.bg_synthwave),
            contentDescription = null,
            modifier = Modifier.fillMaxSize(),
            contentScale = ContentScale.Crop,
        )
        // Dark scrim so the title/buttons stay legible over the art
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(
                    Brush.verticalGradient(
                        colors = listOf(
                            Color.Black.copy(alpha = 0.55f),
                            Color.Black.copy(alpha = 0.35f),
                            Color.Black.copy(alpha = 0.75f),
                        ),
                    )
                ),
        )
        Column(
            modifier = Modifier
                .align(Alignment.Center)
                .fillMaxWidth()
                .windowInsetsPadding(WindowInsets.safeDrawing)
                .padding(horizontal = 32.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            // Circular badge logo
            Image(
                painter = painterResource(id = R.drawable.ic_logo),
                contentDescription = "Archipelago",
                modifier = Modifier
                    .size(160.dp)
                    // graphicsLayer defers the alpha read to the draw phase —
                    // .alpha(value) recomposed the whole screen per frame.
                    .graphicsLayer { alpha = logoAlpha.value },
            )

            Spacer(modifier = Modifier.height(48.dp))

            AnimatedVisibility(
                visible = showContent,
                enter = fadeIn(tween(600)) + slideInVertically(
                    initialOffsetY = { it / 4 },
                    animationSpec = tween(600),
                ),
            ) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Text(
                        text = stringResource(R.string.welcome_title),
                        style = MaterialTheme.typography.headlineLarge,
                        color = Color(0xFFFAFAFA),
                        textAlign = TextAlign.Center,
                    )

                    Spacer(modifier = Modifier.height(16.dp))

                    Text(
                        text = stringResource(R.string.welcome_subtitle),
                        style = MaterialTheme.typography.bodyLarge,
                        color = Color(0xFFFAFAFA),
                        textAlign = TextAlign.Center,
                        lineHeight = 26.sp,
                    )

                    Spacer(modifier = Modifier.height(48.dp))

                    GlassButton(
                        text = stringResource(R.string.get_started),
                        onClick = onContinue,
                        modifier = Modifier.fillMaxWidth().height(56.dp),
                    )

                    Spacer(modifier = Modifier.height(12.dp))

                    GlassButton(
                        text = stringResource(R.string.mesh_party),
                        onClick = onMeshParty,
                        modifier = Modifier.fillMaxWidth().height(48.dp),
                    )
                }
            }
        }
    }
}

/** The pixel-art "A" from AnimatedLogo.vue — 20 white squares */
@Composable
fun PixelArtLogo(modifier: Modifier = Modifier) {
    Canvas(modifier = modifier) {
        val s = size.width / 1024f
        val rects = listOf(
            floatArrayOf(357.614f, 318f, 71.007f, 70.936f),
            floatArrayOf(436.152f, 318f, 72.082f, 70.936f),
            floatArrayOf(515.766f, 318f, 72.082f, 70.936f),
            floatArrayOf(595.379f, 318f, 71.007f, 70.936f),
            floatArrayOf(595.379f, 396.46f, 71.007f, 72.011f),
            floatArrayOf(673.917f, 396.46f, 72.083f, 72.011f),
            floatArrayOf(278f, 475.994f, 72.083f, 72.012f),
            floatArrayOf(357.614f, 475.994f, 71.007f, 72.012f),
            floatArrayOf(436.152f, 475.994f, 72.082f, 72.012f),
            floatArrayOf(515.766f, 475.994f, 72.082f, 72.012f),
            floatArrayOf(595.379f, 475.994f, 71.007f, 72.012f),
            floatArrayOf(673.917f, 475.994f, 72.083f, 72.012f),
            floatArrayOf(278f, 555.529f, 72.083f, 70.936f),
            floatArrayOf(357.614f, 555.529f, 71.007f, 70.936f),
            floatArrayOf(595.379f, 555.529f, 71.007f, 70.936f),
            floatArrayOf(673.917f, 555.529f, 72.083f, 70.936f),
            floatArrayOf(357.614f, 633.989f, 71.007f, 72.011f),
            floatArrayOf(436.152f, 633.989f, 72.082f, 72.011f),
            floatArrayOf(515.766f, 633.989f, 72.082f, 72.011f),
            floatArrayOf(595.379f, 633.989f, 71.007f, 72.011f),
        )
        for (r in rects) {
            drawRect(
                color = Color.White,
                topLeft = Offset(r[0] * s, r[1] * s),
                size = Size(r[2] * s, r[3] * s),
            )
        }
    }
}

/**
 * Glass-style button matching Archipelago's .glass-button.
 * Custom press state (subtle brighten) instead of Material ripple.
 */
@Composable
fun GlassButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val interactionSource = remember { MutableInteractionSource() }
    val isPressed by interactionSource.collectIsPressedAsState()
    val pressAlpha by animateFloatAsState(
        targetValue = if (isPressed) 1f else 0f,
        animationSpec = tween(if (isPressed) 0 else 150),
        label = "press",
    )

    // Lerp between rest and pressed states
    val bgTop = 0.12f + pressAlpha * 0.08f      // 0.12 → 0.20
    val bgBottom = 0.04f + pressAlpha * 0.06f    // 0.04 → 0.10
    val borderA = 0.15f + pressAlpha * 0.10f     // 0.15 → 0.25
    val textAlpha = 1f - pressAlpha * 0.2f       // 1.0  → 0.8

    Box(
        modifier = modifier
            .clip(RoundedCornerShape(12.dp))
            .background(
                Brush.verticalGradient(
                    colors = listOf(
                        Color.White.copy(alpha = bgTop),
                        Color.White.copy(alpha = bgBottom),
                    ),
                )
            )
            .border(
                width = 1.dp,
                color = Color.White.copy(alpha = borderA),
                shape = RoundedCornerShape(12.dp),
            )
            .clickable(
                interactionSource = interactionSource,
                indication = null,
                onClick = onClick,
            ),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = text,
            color = Color.White.copy(alpha = textAlpha),
            style = MaterialTheme.typography.labelLarge,
            fontSize = 16.sp,
        )
    }
}
