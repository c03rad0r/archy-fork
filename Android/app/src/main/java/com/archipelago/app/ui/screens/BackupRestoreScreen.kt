package com.archipelago.app.ui.screens

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.Restore
import androidx.compose.material.icons.filled.Save
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.archipelago.app.R
import com.archipelago.app.data.BackupManager
import com.archipelago.app.ui.theme.BitcoinOrange
import com.archipelago.app.ui.theme.SuccessGreen
import com.archipelago.app.ui.theme.SurfaceBlack
import com.archipelago.app.ui.theme.TextMuted
import com.archipelago.app.ui.theme.TextPrimary
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Backup & Restore (#128) — the phone side of "losing your phone, or wiping
 * it to cross a border".
 *
 * Export seals everything the companion holds (servers, FIPS identity and
 * peers, the remote-signer key) into the node's ADR-005 envelope and hands it
 * to the system file picker; import decrypts, previews and merges it. On
 * GrapheneOS there is no cloud anything — the .json goes wherever the user
 * saves it (USB drive, computer, a folder synced their way), and the
 * passphrase is the only way back in.
 */
@Composable
fun BackupRestoreScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val manager = remember { BackupManager(context) }

    var passphrase by remember { mutableStateOf("") }
    var confirm by remember { mutableStateOf("") }
    var status by remember { mutableStateOf<String?>(null) }
    var statusError by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }

    // Decrypted preview awaiting the user's go-ahead (restore flow).
    var restorePreview by remember { mutableStateOf<Pair<BackupManager.PayloadSummary, org.json.JSONObject>?>(null) }

    fun say(msg: String, error: Boolean) {
        status = msg
        statusError = error
    }

    val exportLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument("application/json")
    ) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            busy = true
            try {
                val envelope = manager.createBackup(passphrase)
                withContext(Dispatchers.IO) {
                    context.contentResolver.openOutputStream(uri)?.use { out ->
                        out.write(envelope.toByteArray())
                    } ?: throw BackupManager.BackupException("could not open the destination file")
                }
                say("Backup saved — keep the file and the passphrase somewhere safe.", false)
            } catch (e: Exception) {
                say(e.message ?: "backup failed", true)
            } finally {
                busy = false
            }
        }
    }

    val importLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument()
    ) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            busy = true
            try {
                val envelope = withContext(Dispatchers.IO) {
                    context.contentResolver.openInputStream(uri)?.use { it.readBytes().decodeToString() }
                        ?: throw BackupManager.BackupException("could not read the selected file")
                }
                val (summary, payload) = manager.readBackup(envelope, passphrase)
                restorePreview = summary to payload
                say("", false)
            } catch (e: Exception) {
                say(e.message ?: "restore failed", true)
            } finally {
                busy = false
            }
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(SurfaceBlack),
    ) {
        Image(
            painter = painterResource(id = R.drawable.bg_synthwave),
            contentDescription = null,
            modifier = Modifier.fillMaxSize(),
            contentScale = ContentScale.Crop,
        )
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(
                    Brush.verticalGradient(
                        colors = listOf(
                            Color.Black.copy(alpha = 0.65f),
                            Color.Black.copy(alpha = 0.5f),
                            Color.Black.copy(alpha = 0.85f),
                        ),
                    )
                ),
        )
        Column(
            modifier = Modifier
                .fillMaxSize()
                .windowInsetsPadding(WindowInsets.safeDrawing)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp)
                .padding(top = 16.dp, bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            // Header
            Row(verticalAlignment = Alignment.CenterVertically) {
                IconButton(onClick = onBack) {
                    Icon(
                        Icons.AutoMirrored.Filled.ArrowBack,
                        contentDescription = "Back",
                        tint = TextPrimary,
                    )
                }
                Text(
                    "Backup & Restore",
                    style = androidx.compose.material3.MaterialTheme.typography.headlineSmall,
                    color = TextPrimary,
                )
            }
            Text(
                "An encrypted copy of everything this phone holds — nodes and their passwords, your mesh identity, the remote-signer key. Same envelope your node uses (ADR-005), one passphrase, no cloud.",
                style = androidx.compose.material3.MaterialTheme.typography.bodyMedium,
                color = TextMuted,
            )

            GlassCard {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        Icon(Icons.Default.Save, contentDescription = null, tint = BitcoinOrange, modifier = Modifier.size(20.dp))
                        Text("Create a backup", color = TextPrimary, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
                    }
                    PassphraseField(
                        value = passphrase,
                        onValueChange = { passphrase = it },
                        placeholder = "Passphrase",
                    )
                    PassphraseField(
                        value = confirm,
                        onValueChange = { confirm = it },
                        placeholder = "Repeat passphrase",
                    )
                    Text(
                        "The passphrase cannot be recovered — a backup nobody can open is a paperweight.",
                        color = TextMuted, fontSize = 11.sp,
                    )
                    GlassButton(
                        text = if (busy) "Working…" else "Save backup file",
                        onClick = {
                            if (busy) return@GlassButton
                            if (passphrase.length < 8) {
                                say("Use at least 8 characters — this passphrase guards every secret in the app.", true)
                                return@GlassButton
                            }
                            if (passphrase != confirm) {
                                say("The two passphrases don't match.", true)
                                return@GlassButton
                            }
                            val stamp = SimpleDateFormat("yyyyMMdd-HHmm", Locale.US).format(Date())
                            exportLauncher.launch("archy-companion-backup-$stamp.json")
                        },
                        modifier = Modifier.fillMaxWidth().height(48.dp),
                    )
                }
            }

            GlassCard {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        Icon(Icons.Default.Restore, contentDescription = null, tint = BitcoinOrange, modifier = Modifier.size(20.dp))
                        Text("Restore a backup", color = TextPrimary, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
                    }
                    Text(
                        "Pick a backup file and enter its passphrase. Nothing is overwritten: nodes merge by identity, and the mesh identity and signer key only restore when this phone has none.",
                        color = TextMuted, fontSize = 11.sp,
                    )
                    PassphraseField(
                        value = passphrase,
                        onValueChange = { passphrase = it },
                        placeholder = "Backup passphrase",
                    )
                    GlassButton(
                        text = if (busy) "Working…" else "Choose backup file",
                        onClick = {
                            if (busy) return@GlassButton
                            if (passphrase.isEmpty()) {
                                say("Enter the backup's passphrase first.", true)
                                return@GlassButton
                            }
                            importLauncher.launch(arrayOf("application/json"))
                        },
                        modifier = Modifier.fillMaxWidth().height(48.dp),
                    )
                }
            }

            restorePreview?.let { (summary, payload) ->
                GlassCard {
                    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        Text(
                            "Backup verified${if (summary.appVersion.isNotBlank()) " (made by v${summary.appVersion})" else ""}",
                            color = SuccessGreen, fontSize = 15.sp, fontWeight = FontWeight.SemiBold,
                        )
                        SummaryRow("Nodes", summary.serverCount.toString())
                        if (summary.hasFipsIdentity) SummaryRow("Mesh identity", "included")
                        if (summary.hasSignerKey) SummaryRow("Remote-signer key", "included")
                        GlassButton(
                            text = if (busy) "Restoring…" else "Restore onto this phone",
                            onClick = {
                                if (busy) return@GlassButton
                                scope.launch {
                                    busy = true
                                    try {
                                        val result = manager.restoreBackup(payload)
                                        restorePreview = null
                                        passphrase = ""
                                        confirm = ""
                                        say(
                                            "Restored ${result.serversRestored} node(s)" +
                                                (if (result.activeSet) ", set active" else "") +
                                                (if (result.fipsIdentityRestored) ", mesh identity" else "") +
                                                (if (result.signerKeyRestored) ", signer key" else "") +
                                                ". Restart the app to reconnect.",
                                            false,
                                        )
                                    } catch (e: Exception) {
                                        say(e.message ?: "restore failed", true)
                                    } finally {
                                        busy = false
                                    }
                                }
                            },
                            modifier = Modifier.fillMaxWidth().height(48.dp),
                        )
                    }
                }
            }

            status?.takeIf { it.isNotBlank() }?.let { msg ->
                Text(
                    msg,
                    color = if (statusError) Color(0xFFFF6B6B) else SuccessGreen,
                    fontSize = 13.sp,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
    }
}

@Composable
private fun PassphraseField(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        placeholder = { Text(placeholder, color = TextMuted, fontSize = 14.sp) },
        modifier = Modifier.fillMaxWidth(),
        singleLine = true,
        visualTransformation = PasswordVisualTransformation(),
        keyboardOptions = KeyboardOptions(
            keyboardType = KeyboardType.Password,
            imeAction = ImeAction.Done,
        ),
        keyboardActions = KeyboardActions(),
        textStyle = TextStyle(color = TextPrimary, fontSize = 15.sp),
        colors = OutlinedTextFieldDefaults.colors(
            focusedBorderColor = BitcoinOrange,
            unfocusedBorderColor = Color.White.copy(alpha = 0.15f),
            cursorColor = BitcoinOrange,
            focusedTextColor = TextPrimary,
            unfocusedTextColor = TextPrimary,
        ),
        shape = RoundedCornerShape(12.dp),
    )
}

@Composable
private fun SummaryRow(label: String, value: String) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(label, color = TextMuted, fontSize = 13.sp)
        Text(value, color = TextPrimary, fontSize = 13.sp, fontWeight = FontWeight.Medium)
    }
}

/** House glass card: translucent panel, hairline border. */
@Composable
fun GlassCard(content: @Composable () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(16.dp))
            .background(Color.White.copy(alpha = 0.06f))
            .padding(16.dp),
    ) {
        content()
    }
}
