package com.archipelago.app.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable

private val DarkColorScheme = darkColorScheme(
    primary = BitcoinOrange,
    onPrimary = SurfaceBlack,
    primaryContainer = BitcoinOrangeDark,
    onPrimaryContainer = TextPrimary,
    secondary = BitcoinOrangeLight,
    onSecondary = SurfaceBlack,
    background = SurfaceBlack,
    onBackground = TextPrimary,
    surface = SurfaceDark,
    onSurface = TextPrimary,
    surfaceVariant = SurfaceCard,
    onSurfaceVariant = TextSecondary,
    outline = BorderDefault,
    outlineVariant = BorderSubtle,
    error = ErrorRed,
    onError = TextPrimary,
)

private val LightColorScheme = lightColorScheme(
    primary = BitcoinOrange,
    onPrimary = SurfaceBlack,
    primaryContainer = BitcoinOrangeLight,
    onPrimaryContainer = SurfaceBlack,
    secondary = BitcoinOrangeDark,
    onSecondary = TextPrimary,
    background = Neo.LightSurface,
    onBackground = Neo.LightTextPrimary,
    surface = Neo.LightSurfaceRaised,
    onSurface = Neo.LightTextPrimary,
    surfaceVariant = Neo.LightSurface,
    onSurfaceVariant = Neo.LightTextSecondary,
    outline = Neo.LightBorder,
    outlineVariant = Neo.LightBorder,
    error = ErrorRed,
    onError = TextPrimary,
)

@Composable
fun ArchipelagoTheme(content: @Composable () -> Unit) {
    val colorScheme = if (isSystemInDarkTheme()) DarkColorScheme else LightColorScheme

    MaterialTheme(
        colorScheme = colorScheme,
        typography = Typography,
        content = content,
    )
}
