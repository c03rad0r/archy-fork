package com.archipelago.app.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

object Neo {
    // ── Dark ───────────────────────────────────────────
    val DarkSurface = Color(0xFF0A0A0A)
    val DarkSurfaceRaised = Color(0xFF0F0F11)
    val DarkShadowLight = Color(0xFF151517)
    val DarkShadowDark = Color(0xFF000000)
    val DarkBorder = Color(0x0AFFFFFF)

    // ── Light ──────────────────────────────────────────
    val LightSurface = Color(0xFFE0E0E4)
    val LightSurfaceRaised = Color(0xFFE6E6EA)
    val LightShadowLight = Color(0xFFF2F2F6)
    val LightShadowDark = Color(0xFFB4B4BA)
    val LightBorder = Color(0x0A000000)

    val LightTextPrimary = Color(0xFF141414)
    val LightTextSecondary = Color(0xFF5A5A5A)
    val LightTextMuted = Color(0xFF9A9A9A)

    // ── Accessors ──────────────────────────────────────

    @Composable @ReadOnlyComposable
    fun surface() = if (isSystemInDarkTheme()) DarkSurface else LightSurface

    @Composable @ReadOnlyComposable
    fun surfaceRaised() = if (isSystemInDarkTheme()) DarkSurfaceRaised else LightSurfaceRaised

    @Composable @ReadOnlyComposable
    fun shadowLight() = if (isSystemInDarkTheme()) DarkShadowLight else LightShadowLight

    @Composable @ReadOnlyComposable
    fun shadowDark() = if (isSystemInDarkTheme()) DarkShadowDark else LightShadowDark

    @Composable @ReadOnlyComposable
    fun border() = if (isSystemInDarkTheme()) DarkBorder else LightBorder

    @Composable @ReadOnlyComposable
    fun textPrimary() = if (isSystemInDarkTheme()) Color(0xFFD0D0D0) else LightTextPrimary

    @Composable @ReadOnlyComposable
    fun textSecondary() = if (isSystemInDarkTheme()) Color(0xFF666666) else LightTextSecondary

    @Composable @ReadOnlyComposable
    fun textMuted() = if (isSystemInDarkTheme()) Color(0xFF333333) else LightTextMuted
}

/** Subtle neomorphic raised shadow */
fun Modifier.neoRaised(
    lightShadow: Color,
    darkShadow: Color,
    radius: Dp = 14.dp,
    shadowOffset: Dp = 2.dp,
    shadowBlur: Dp = 4.dp,
) = this.drawBehind {
    val r = radius.toPx()
    val off = shadowOffset.toPx()
    val blur = shadowBlur.toPx()
    drawIntoCanvas { canvas ->
        val path = Path().apply { addRoundRect(RoundRect(0f, 0f, size.width, size.height, CornerRadius(r))) }
        canvas.drawPath(path, Paint().also {
            it.asFrameworkPaint().apply { isAntiAlias = true; color = android.graphics.Color.TRANSPARENT; setShadowLayer(blur, off, off, darkShadow.toArgb()) }
        })
        canvas.drawPath(path, Paint().also {
            it.asFrameworkPaint().apply { isAntiAlias = true; color = android.graphics.Color.TRANSPARENT; setShadowLayer(blur, -off, -off, lightShadow.toArgb()) }
        })
    }
}

/** Subtle neomorphic inset shadow */
fun Modifier.neoInset(
    lightShadow: Color,
    darkShadow: Color,
    radius: Dp = 14.dp,
    shadowOffset: Dp = 1.dp,
    shadowBlur: Dp = 3.dp,
) = this.drawBehind {
    val r = radius.toPx()
    val off = shadowOffset.toPx()
    val blur = shadowBlur.toPx()
    drawIntoCanvas { canvas ->
        val path = Path().apply { addRoundRect(RoundRect(0f, 0f, size.width, size.height, CornerRadius(r))) }
        canvas.drawPath(path, Paint().also {
            it.asFrameworkPaint().apply { isAntiAlias = true; color = android.graphics.Color.TRANSPARENT; setShadowLayer(blur, -off, -off, darkShadow.toArgb()) }
        })
        canvas.drawPath(path, Paint().also {
            it.asFrameworkPaint().apply { isAntiAlias = true; color = android.graphics.Color.TRANSPARENT; setShadowLayer(blur, off, off, lightShadow.toArgb()) }
        })
    }
}
