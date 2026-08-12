package com.archipelago.app.ui.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Gamepad
import androidx.compose.material.icons.filled.Keyboard
import androidx.compose.material.icons.filled.RadioButtonChecked
import androidx.compose.material.icons.filled.RadioButtonUnchecked
import androidx.compose.material.icons.filled.Web
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import com.archipelago.app.data.ServerEntry
import com.archipelago.app.ui.theme.BitcoinOrange
import com.archipelago.app.ui.theme.Neo
import com.archipelago.app.ui.theme.TextMuted
import com.archipelago.app.ui.theme.TextPrimary
import com.archipelago.app.ui.theme.neoRaised

private val ROW_H = 48.dp
private val ROW_R = 12.dp

@Composable
fun ServerModal(
    visible: Boolean,
    servers: List<ServerEntry>,
    activeServer: ServerEntry?,
    isGamepadMode: Boolean,
    onDismiss: () -> Unit,
    onSelectServer: (ServerEntry) -> Unit,
    onAddServer: (ServerEntry) -> Unit,
    onRemoveServer: (ServerEntry) -> Unit,
    onToggleGamepadMode: () -> Unit,
    onBackToWebView: (() -> Unit)? = null,
) {
    AnimatedVisibility(visible = visible, enter = fadeIn(), exit = fadeOut()) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(Color.Black.copy(alpha = 0.55f))
                .clickable(
                    indication = null,
                    interactionSource = remember { MutableInteractionSource() },
                ) { onDismiss() },
            contentAlignment = Alignment.Center,
        ) {
            AnimatedVisibility(visible = visible, enter = fadeIn() + scaleIn(initialScale = 0.95f), exit = fadeOut() + scaleOut(targetScale = 0.95f)) {
                ModalBody(servers, activeServer, isGamepadMode, onDismiss, onSelectServer, onAddServer, onRemoveServer, onToggleGamepadMode, onBackToWebView)
            }
        }
    }
}

@Composable
private fun ModalBody(
    servers: List<ServerEntry>,
    activeServer: ServerEntry?,
    isGamepadMode: Boolean,
    onDismiss: () -> Unit,
    onSelectServer: (ServerEntry) -> Unit,
    onAddServer: (ServerEntry) -> Unit,
    onRemoveServer: (ServerEntry) -> Unit,
    onToggleGamepadMode: () -> Unit,
    onBackToWebView: (() -> Unit)?,
) {
    val surface = Neo.surfaceRaised()
    val light = Neo.shadowLight()
    val dark = Neo.shadowDark()
    var showAddForm by remember { mutableStateOf(false) }
    var newAddress by remember { mutableStateOf("") }
    var newPassword by remember { mutableStateOf("") }

    Column(
        modifier = Modifier
            .widthIn(max = 380.dp)
            .neoRaised(light, dark, 24.dp, 6.dp, 12.dp)
            .clip(RoundedCornerShape(24.dp))
            .background(surface)
            .clickable(indication = null, interactionSource = remember { MutableInteractionSource() }) {}
            .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        // Header
        Row(Modifier.fillMaxWidth(), Arrangement.SpaceBetween, Alignment.CenterVertically) {
            Text("Servers", style = MaterialTheme.typography.titleMedium, color = Neo.textPrimary())
            IconButton(onClick = onDismiss, modifier = Modifier.size(32.dp)) {
                Icon(Icons.Default.Close, "Close", Modifier.size(16.dp), tint = Neo.textMuted())
            }
        }

        // Server rows
        servers.forEach { server ->
            val isActive = server.serialize() == activeServer?.serialize()
            ModalRow(
                icon = if (isActive) Icons.Default.RadioButtonChecked else Icons.Default.RadioButtonUnchecked,
                iconTint = if (isActive) BitcoinOrange else Neo.textMuted(),
                label = server.address + if (server.port.isNotBlank()) ":${server.port}" else "",
                onClick = { onSelectServer(server) },
                trailing = {
                    IconButton(onClick = { onRemoveServer(server) }, modifier = Modifier.size(28.dp)) {
                        Icon(Icons.Default.Close, "Remove", Modifier.size(14.dp), tint = Neo.textMuted())
                    }
                },
            )
        }

        if (servers.isEmpty()) {
            Text("No servers", style = MaterialTheme.typography.bodyMedium, color = Neo.textMuted(), modifier = Modifier.padding(vertical = 4.dp))
        }

        // Add server
        if (showAddForm) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(ROW_R))
                    .background(Neo.surface())
                    .padding(12.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                OutlinedTextField(
                    value = newAddress, onValueChange = { newAddress = it.trim() },
                    placeholder = { Text("192.168.1.100") },
                    modifier = Modifier.fillMaxWidth(), singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Next),
                    colors = neoFieldColors(),
                    shape = RoundedCornerShape(10.dp),
                    textStyle = MaterialTheme.typography.bodyMedium,
                )
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    OutlinedTextField(
                        value = newPassword, onValueChange = { newPassword = it },
                        placeholder = { Text("Password") },
                        modifier = Modifier.weight(1f), singleLine = true,
                        visualTransformation = PasswordVisualTransformation(),
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Go),
                        keyboardActions = KeyboardActions(onGo = {
                            if (newAddress.isNotBlank()) {
                                onAddServer(ServerEntry(newAddress, false, password = newPassword))
                                newAddress = ""; newPassword = ""; showAddForm = false
                            }
                        }),
                        colors = neoFieldColors(),
                        shape = RoundedCornerShape(10.dp),
                        textStyle = MaterialTheme.typography.bodyMedium,
                    )
                    Box(
                        modifier = Modifier.size(36.dp).clip(CircleShape).background(BitcoinOrange.copy(alpha = 0.15f))
                            .clickable {
                                if (newAddress.isNotBlank()) {
                                    onAddServer(ServerEntry(newAddress, false, password = newPassword))
                                    newAddress = ""; newPassword = ""; showAddForm = false
                                }
                            },
                        contentAlignment = Alignment.Center,
                    ) { Icon(Icons.Default.Add, "Add", Modifier.size(16.dp), tint = BitcoinOrange) }
                }
            }
        } else {
            ModalRow(icon = Icons.Default.Add, iconTint = BitcoinOrange, label = "Add Server", labelColor = BitcoinOrange, onClick = { showAddForm = true })
        }

        HorizontalDivider(color = Neo.border(), modifier = Modifier.padding(vertical = 4.dp))

        // Gamepad toggle — label says what you switch TO
        ModalRow(
            icon = if (isGamepadMode) Icons.Default.Keyboard else Icons.Default.Gamepad,
            iconTint = Neo.textSecondary(),
            label = if (isGamepadMode) "Switch to Keyboard" else "Switch to Gamepad",
            onClick = onToggleGamepadMode,
        )

        // Back to dashboard
        if (onBackToWebView != null) {
            ModalRow(icon = Icons.Default.Web, iconTint = Neo.textSecondary(), label = "Back to Dashboard", onClick = onBackToWebView)
        }
    }
}

/** Uniform-height row used for all modal actions */
@Composable
private fun ModalRow(
    icon: ImageVector,
    iconTint: Color,
    label: String,
    onClick: () -> Unit,
    labelColor: Color = Neo.textPrimary(),
    trailing: (@Composable () -> Unit)? = null,
) {
    val bg = Neo.surface()
    val light = Neo.shadowLight()
    val dark = Neo.shadowDark()

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(ROW_H)
            .neoRaised(light, dark, ROW_R, 2.dp, 5.dp)
            .clip(RoundedCornerShape(ROW_R))
            .background(bg)
            .clickable { onClick() }
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, null, Modifier.size(18.dp), tint = iconTint)
        Spacer(Modifier.width(12.dp))
        Text(label, style = MaterialTheme.typography.bodyMedium, color = labelColor, modifier = Modifier.weight(1f))
        if (trailing != null) trailing()
    }
}

@Composable
private fun neoFieldColors() = OutlinedTextFieldDefaults.colors(
    focusedBorderColor = BitcoinOrange.copy(alpha = 0.4f),
    unfocusedBorderColor = Neo.border(),
    cursorColor = BitcoinOrange,
    focusedTextColor = Neo.textPrimary(),
    unfocusedTextColor = Neo.textPrimary(),
)
