package com.archipelago.app.ui.theme

import androidx.compose.ui.graphics.Color

/** NES/8BitDo controller palettes */
object NES {
    // ── Classic (light body, red buttons) ──────────────
    val ClassicBody = Color(0xFFD4D0C8)        // warm light gray plastic
    val ClassicFace = Color(0xFF1C1C1C)        // dark face plate
    val ClassicAccent = Color(0xFF8A8A8A)       // mid gray trim
    val ClassicRidge = Color(0xFFBBB8B0)        // grip lines
    val ClassicButtonRed = Color(0xFFC1121C)    // A/B red
    val ClassicButtonRedPress = Color(0xFF8A0D14)
    val ClassicButtonGray = Color(0xFF5A5A5A)   // turbo buttons
    val ClassicButtonGrayPress = Color(0xFF3A3A3A)
    val ClassicDPad = Color(0xFF1A1A1A)
    val ClassicDPadPress = Color(0xFF2A2A2A)
    val ClassicLabel = Color(0xFFC1121C)        // red text labels
    val ClassicLabelMuted = Color(0xFF6A6A6A)
    val ClassicSelect = Color(0xFF2A2A2A)       // START/SELECT

    // ── Transparent Dark ───────────────────────────────
    val DarkBody = Color(0xFF2A2A2E)           // smoky translucent dark
    val DarkFace = Color(0xFF151518)           // darker face
    val DarkAccent = Color(0xFF3A3A3E)         // trim
    val DarkRidge = Color(0xFF222226)          // grip lines
    val DarkButtonMain = Color(0xFF3A3A3E)     // all buttons dark
    val DarkButtonMainPress = Color(0xFF222226)
    val DarkDPad = Color(0xFF0E0E10)
    val DarkDPadPress = Color(0xFF1A1A1E)
    val DarkLabel = Color(0xFF5A5A60)          // muted labels
    val DarkLabelMuted = Color(0xFF3A3A3E)
    val DarkSelect = Color(0xFF1A1A1E)

    // ── Menu UI (NES-style) ────────────────────────────
    val MenuBg = Color(0xFF000000)
    val MenuPanel = Color(0xFF0B1B4A)          // dark navy
    val MenuBorder = Color(0xFFFFFFFF)
    val MenuText = Color(0xFFFFFFFF)
    val MenuSelected = Color(0xFFC1121C)
    val MenuMuted = Color(0xFF7A7A7A)
}

enum class ControllerStyle { CLASSIC, DARK }
