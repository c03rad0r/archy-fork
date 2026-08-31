package com.archipelago.app.ui.components

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Restore
import androidx.compose.material.icons.filled.Save
import androidx.compose.material3.Icon
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.archipelago.app.data.BackupManager
import com.archipelago.app.ui.theme.BitcoinOrange
import com.archipelago.app.ui.theme.SuccessGreen
import com.archipelago.app.ui.theme.TextMuted
import com.archipelago.app.ui.theme.TextPrimary
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Backup & Restore (#128) — the hub's BACKUP sub-page (same container as
 * Nodes/FIPS), the phone side of losing your phone or wiping it to cross a
 * border. See docs/companion-backup-restore.md for the envelope and merge
 * semantics; this composable is the flow only.
 */
@Composable
internal fun BackupSection() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val manager = remember { BackupManager(context) }

    var passphrase by remember { mutableStateOf("") }
    var confirm by remember { mutableStateOf("") }
    var status by remember { mutableStateOf<String?>(null) }
    var statusError by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }

    // Decrypted backup awaiting the user's go-ahead (restore flow).
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
                say("Saved — keep the file and the passphrase somewhere safe.", false)
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
            } catch (e: Exception) {
                say(e.message ?: "restore failed", true)
            } finally {
                busy = false
            }
        }
    }

    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        SectionCopy(
            "An encrypted copy of everything this phone holds — nodes and their passwords, " +
                "your mesh identity, the remote-signer key. Same envelope your node uses (ADR-005), " +
                "one passphrase, no cloud."
        )

        // ── Create a backup ──────────────────────────────────────────────
        SectionHeader(Icons.Default.Save, "Create a backup")
        GlassField(
            value = passphrase,
            onValueChange = { passphrase = it },
            placeholder = "Passphrase",
            visualTransformation = androidx.compose.ui.text.input.PasswordVisualTransformation(),
        )
        GlassField(
            value = confirm,
            onValueChange = { confirm = it },
            placeholder = "Repeat passphrase",
            visualTransformation = androidx.compose.ui.text.input.PasswordVisualTransformation(),
        )
        SectionHint("The passphrase cannot be recovered — a backup nobody can open is a paperweight.")
        WideAction(
            text = if (busy) "Working…" else "Save backup file",
            onClick = {
                if (busy) return@WideAction
                if (passphrase.length < 8) {
                    say("Use at least 8 characters — this passphrase guards every secret in the app.", true)
                    return@WideAction
                }
                if (passphrase != confirm) {
                    say("The two passphrases don't match.", true)
                    return@WideAction
                }
                val stamp = SimpleDateFormat("yyyyMMdd-HHmm", Locale.US).format(Date())
                exportLauncher.launch("archy-companion-backup-$stamp.json")
            },
        )

        Spacer(Modifier.height(2.dp))

        // ── Restore a backup ─────────────────────────────────────────────
        SectionHeader(Icons.Default.Restore, "Restore a backup")
        SectionHint(
            "Nothing is overwritten: nodes merge by identity, and the mesh identity and " +
                "signer key only restore when this phone has none."
        )
        WideAction(
            text = if (busy) "Working…" else "Choose backup file",
            onClick = {
                if (busy) return@WideAction
                if (passphrase.isEmpty()) {
                    say("Enter the backup's passphrase first.", true)
                    return@WideAction
                }
                importLauncher.launch(arrayOf("application/json"))
            },
        )

        restorePreview?.let { (summary, payload) ->
            Spacer(Modifier.height(2.dp))
            Column(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(14.dp))
                    .background(Color.White.copy(alpha = 0.04f))
                    .border(1.dp, Color.White.copy(alpha = 0.08f), RoundedCornerShape(14.dp))
                    .padding(12.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Text(
                    "Backup verified${if (summary.appVersion.isNotBlank()) " (made by v${summary.appVersion})" else ""}",
                    color = SuccessGreen, fontSize = 13.sp, fontWeight = FontWeight.SemiBold,
                )
                SummaryRow("Nodes", summary.serverCount.toString())
                if (summary.hasFipsIdentity) SummaryRow("Mesh identity", "included")
                if (summary.hasSignerKey) SummaryRow("Remote-signer key", "included")
                WideAction(
                    text = if (busy) "Restoring…" else "Restore onto this phone",
                    onClick = {
                        if (busy) return@WideAction
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
                )
            }
        }

        status?.takeIf { it.isNotBlank() }?.let { msg ->
            Text(
                msg,
                color = if (statusError) Color(0xFFFF6B6B) else SuccessGreen,
                fontSize = 12.sp,
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

@Composable
internal fun SectionHeader(icon: androidx.compose.ui.graphics.vector.ImageVector, title: String) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(icon, contentDescription = null, tint = BitcoinOrange, modifier = Modifier.size(18.dp))
        Text(title, color = TextPrimary, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
    }
}

@Composable
internal fun SectionCopy(text: String) {
    Text(text, color = TextMuted, fontSize = 12.sp, lineHeight = 16.sp)
}

@Composable
internal fun SectionHint(text: String) {
    Text(text, color = TextMuted.copy(alpha = 0.8f), fontSize = 10.sp, lineHeight = 13.sp)
}

/** Wide orange-outline action button in the menu's visual language. */
@Composable
internal fun WideAction(
    text: String,
    onClick: () -> Unit,
    icon: androidx.compose.ui.graphics.vector.ImageVector? = null,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .height(44.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(BitcoinOrange.copy(alpha = 0.15f))
            .border(1.dp, BitcoinOrange.copy(alpha = 0.4f), RoundedCornerShape(12.dp))
            .clickable { onClick() },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = null, tint = BitcoinOrange, modifier = Modifier.size(16.dp))
            Spacer(Modifier.size(8.dp))
        }
        Text(text, color = BitcoinOrange, fontSize = 13.sp, fontWeight = FontWeight.Bold)
    }
}

@Composable
internal fun SummaryRow(label: String, value: String) {
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(label, color = TextMuted, fontSize = 12.sp)
        Text(value, color = TextPrimary, fontSize = 12.sp, fontWeight = FontWeight.Medium)
    }
}
