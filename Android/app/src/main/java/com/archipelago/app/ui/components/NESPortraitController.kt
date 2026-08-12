package com.archipelago.app.ui.components

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import com.archipelago.app.R
import com.archipelago.app.ui.theme.ControllerStyle
import com.archipelago.app.ui.theme.NES

/**
 * Portrait gamepad — vertical remote shape like Apple TV but NES-styled.
 * Large trackpad top, D-pad middle, A/B + START/SELECT bottom.
 */
@Composable
fun NESPortraitController(
    style: ControllerStyle = ControllerStyle.CLASSIC,
    playerId: Int = 0,
    onKey: (String) -> Unit,
    onMouseMove: (Int, Int) -> Unit = { _, _ -> },
    onMouseClick: (Int) -> Unit = { _ -> },
    onMouseScroll: (Int) -> Unit = { _ -> },
    onMenu: () -> Unit,
    onPlayerToggle: () -> Unit = {},
    onToggleStyle: (() -> Unit)? = null,
) {
    val c = paletteFor(style)
    val isClassic = style == ControllerStyle.CLASSIC

    Box(
        Modifier
            .fillMaxSize()
            .threeFingerHold(onMenu)
            .padding(horizontal = 40.dp, vertical = 24.dp),
        contentAlignment = Alignment.Center,
    ) {
        // Remote body — tall vertical shape
        Box(
            Modifier
                .fillMaxWidth(0.75f)
                .fillMaxSize()
                .shadow(28.dp, RoundedCornerShape(20.dp), ambientColor = Color.Black, spotColor = Color.Black)
                .clip(RoundedCornerShape(20.dp))
                .background(Brush.verticalGradient(listOf(c.body, c.body)))
                .border(1.dp, Color.White.copy(alpha = if (isClassic) 0.08f else 0.04f), RoundedCornerShape(20.dp)),
        ) {
            // Top highlight
            Box(
                Modifier.fillMaxWidth().height(1.dp).align(Alignment.TopCenter)
                    .background(Color.White.copy(alpha = if (isClassic) 0.12f else 0.05f))
            )

            // Face plate
            Column(
                Modifier
                    .fillMaxSize()
                    .padding(14.dp)
                    .clip(RoundedCornerShape(14.dp))
                    .background(c.face)
                    .border(0.5.dp, Color.White.copy(alpha = 0.03f), RoundedCornerShape(14.dp))
                    .padding(16.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.SpaceBetween,
            ) {
                // Trackpad area (touch surface for mouse)
                Trackpad(
                    onMove = { dx, dy -> onMouseMove(dx, dy) },
                    onClick = { onMouseClick(it) },
                    onScroll = { dy -> onMouseScroll(dy) },
                    onThreeFingerHold = onMenu,
                    modifier = Modifier
                        .fillMaxWidth()
                        .weight(1f),
                )

                Spacer(Modifier.height(12.dp))

                // D-Pad
                Inlay(c, Modifier.size(150.dp)) {
                    OnePointDPad(c, 130.dp, onKey)
                }

                Spacer(Modifier.height(12.dp))

                // Logo
                Image(
                    painter = painterResource(id = R.drawable.ic_logo_wide),
                    contentDescription = "Archipelago",
                    modifier = Modifier.width(140.dp),
                    colorFilter = ColorFilter.tint(if (isClassic) NES.ClassicLabel else c.label),
                )

                Spacer(Modifier.height(12.dp))

                // A/B/C Buttons — triangle: C top, B+A bottom
                Inlay(c, Modifier.fillMaxWidth()) {
                    Column(
                        Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        GlassFaceBtn("C", Color(0xFFBBBBBB), 46.dp) { onKey("c") }
                        Spacer(Modifier.height(6.dp))
                        Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                            GlassFaceBtn("B", Color(0xFF60A5FA), 46.dp) { onKey("b") }
                            GlassFaceBtn("A", Color(0xFFF7931A), 46.dp) { onKey("a") }
                        }
                    }
                }

                Spacer(Modifier.height(10.dp))

                // START / SELECT
                Inlay(c, Modifier) {
                    Row(
                        Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                        horizontalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        CapsuleBtn("SELECT", c, 64.dp, 28.dp) { onKey("Escape") }
                        CapsuleBtn("START", c, 64.dp, 28.dp) { onKey("Return") }
                    }
                }

                Spacer(Modifier.height(6.dp))

                // Player toggle + Settings
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.Center,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    PlayerPill(c, playerId, onPlayerToggle)
                    Spacer(Modifier.width(10.dp))
                    SettingsBtn(c, Modifier, onMenu)
                    onToggleStyle?.let {
                        Spacer(Modifier.width(10.dp))
                        StyleBtn(c, Modifier, it)
                    }
                }
            }
        }
    }
}
