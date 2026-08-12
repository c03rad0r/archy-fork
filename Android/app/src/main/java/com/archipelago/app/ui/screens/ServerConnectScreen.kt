package com.archipelago.app.ui.screens

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.filled.Visibility
import androidx.compose.material.icons.filled.VisibilityOff
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.LockOpen
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.archipelago.app.R
import com.archipelago.app.data.PairResult
import com.archipelago.app.data.ServerEntry
import com.archipelago.app.data.ServerPreferences
import com.archipelago.app.fips.FipsManager
import com.archipelago.app.ui.components.MeshLoadingScreen
import com.archipelago.app.ui.components.SlidingLoader
import com.archipelago.app.ui.components.QrScannerOverlay
import com.archipelago.app.ui.theme.BitcoinOrange
import com.archipelago.app.ui.theme.ErrorRed
import com.archipelago.app.ui.theme.SurfaceBlack
import com.archipelago.app.ui.theme.SurfaceCard
import com.archipelago.app.ui.theme.SuccessGreen
import com.archipelago.app.ui.theme.TextMuted
import com.archipelago.app.ui.theme.TextPrimary
import com.archipelago.app.ui.theme.TextSecondary
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.net.HttpURLConnection
import java.net.URL
import javax.net.ssl.HttpsURLConnection
import javax.net.ssl.SSLContext
import javax.net.ssl.X509TrustManager

@Composable
fun ServerConnectScreen(
    onConnected: (String) -> Unit,
    onRemoteInput: () -> Unit = {},
    // Prefill from a pairing deep link (archipelago://pair) that carried no
    // password — opens the manual form on the password prompt for that server.
    initialServer: ServerEntry? = null,
) {
    val context = LocalContext.current
    val prefs = remember { ServerPreferences(context) }
    val scope = rememberCoroutineScope()
    val keyboard = LocalSoftwareKeyboardController.current

    val savedServers by prefs.savedServers.collectAsState(initial = emptyList())

    // Warm the mesh tunnel the moment the screen appears — starting it only
    // after the LAN probe failed put full tunnel bring-up + session discovery
    // inside the user's wait. By connect-tap time it's usually already up.
    //
    // Only when there is actually a mesh node to warm for, though: raising the
    // tunnel on a phone whose saved nodes are all plain HTTP boxes takes
    // Android's single VPN slot for nothing.
    LaunchedEffect(savedServers.any { it.isFipsNode() }) {
        if (savedServers.none { it.isFipsNode() }) return@LaunchedEffect
        withContext(Dispatchers.IO) { FipsManager.autoStartIfReady(context) }
    }

    var name by remember { mutableStateOf("") }
    var address by remember { mutableStateOf("") }
    var port by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var passwordVisible by remember { mutableStateOf(false) }
    var useHttps by remember { mutableStateOf(false) }
    var isConnecting by remember { mutableStateOf(false) }
    var errorMessage by remember { mutableStateOf<String?>(null) }
    // The saved server currently being edited, or null when adding/connecting.
    var editingServer by remember { mutableStateOf<ServerEntry?>(null) }
    // Landing shows Scan/Manual choice; the form appears in manual mode or while editing.
    var manualMode by remember { mutableStateOf(false) }
    var showScanner by remember { mutableStateOf(false) }
    // Is the connect currently running aimed at a mesh node? Drives whether
    // the loader wears the FIPS brand — see MeshLoadingScreen.
    var connectingOverMesh by remember { mutableStateOf(false) }
    var connectingName by remember { mutableStateOf("") }
    // Brief green landing on the loader before the kiosk takes over, matching
    // the platform's install overlay.
    var connectSucceeded by remember { mutableStateOf(false) }

    fun clearForm() {
        name = ""
        address = ""
        port = ""
        password = ""
        useHttps = false
        passwordVisible = false
        errorMessage = null
    }

    fun startEdit(server: ServerEntry) {
        editingServer = server
        name = server.name
        address = server.address
        port = server.port
        password = server.password
        useHttps = server.useHttps
        passwordVisible = false
        errorMessage = null
    }

    fun cancelEdit() {
        editingServer = null
        clearForm()
    }

    fun saveEdit() {
        val original = editingServer ?: return
        if (address.isBlank()) {
            errorMessage = "Enter a server address"
            return
        }
        val updated = ServerEntry(address, useHttps, port, password, name)
        scope.launch {
            prefs.updateSavedServer(original, updated)
            cancelEdit()
        }
    }

    fun connect(server: ServerEntry) {
        if (isConnecting) return
        if (server.address.isBlank()) {
            errorMessage = "Enter a server address"
            return
        }
        isConnecting = true
        errorMessage = null
        connectingOverMesh = server.isFipsNode()
        connectingName = server.displayName()
        connectSucceeded = false

        scope.launch {
            // LAN and mesh race IN PARALLEL — the serial LAN-then-mesh chain
            // burned a guaranteed-dead 5 s LAN probe before the mesh path even
            // started (the off-LAN QR-pairing case, exactly where speed shows).
            // The scanned IP was only ever a dial hint; the node's real
            // identity is its npub and its ULA is reachable from anywhere over
            // the mesh. Mesh discovery + first session can take 15s+ through
            // the public tree (per node diagnosis), and on a
            // first-ever pairing the VPN consent dialog is on screen at the
            // same time — so the mesh side keeps probing inside its budget
            // while the tunnel (already started at screen entry, and kicked
            // again here) warms up underneath.
            val meshServer = server.meshIp.takeIf { it.isNotBlank() }?.let {
                FipsManager.autoStartIfReady(context)
                server.copy(address = it, useHttps = false, port = "")
            }
            val reachable = kotlinx.coroutines.coroutineScope {
                val lan = async { testConnection(server, timeoutMs = 4_000) }
                val mesh = async {
                    if (meshServer == null) return@async false
                    val deadline = System.currentTimeMillis() + 45_000
                    var ok = false
                    while (!ok && System.currentTimeMillis() < deadline) {
                        ok = testConnection(meshServer, timeoutMs = 8_000)
                        if (!ok) delay(2000)
                    }
                    ok
                }
                val first = kotlinx.coroutines.selects.select<Boolean> {
                    lan.onAwait { it }
                    mesh.onAwait { it }
                }
                if (first) {
                    lan.cancel(); mesh.cancel()
                    true
                } else {
                    // One side gave up — the verdict is whatever the other says.
                    if (lan.isCompleted) mesh.await() else lan.await()
                }
            }
            if (reachable) {
                // Land the loader green before handing over, so the last thing
                // seen is "done", not a bar cut mid-sweep.
                connectSucceeded = true
                prefs.setActiveServer(server)
                delay(320)
                isConnecting = false
                onConnected(server.toUrl())
            } else {
                isConnecting = false
                errorMessage = context.getString(R.string.connection_failed)
            }
        }
    }

    fun prefill(server: ServerEntry) {
        name = server.name
        address = server.address
        port = server.port
        password = server.password
        useHttps = server.useHttps
    }

    // Pairing QR scanned: dedupe against saved servers, then either auto-connect
    // (payload carried a credential — demo password or a real node's device
    // token) or land on the password prompt with everything else filled in.
    // Mesh info (when present) is registered so the FIPS tunnel comes up too.
    fun onQrScanned(scan: PairResult.Success) {
        showScanner = false
        scope.launch {
            val merged = prefs.upsertServer(scan.server)
            FipsManager.registerNode(context, scan.fips, merged.displayName())
            prefill(merged)
            if (merged.password.isNotBlank()) {
                connect(merged)
            } else {
                manualMode = true
            }
        }
    }

    LaunchedEffect(initialServer) {
        if (initialServer != null) {
            prefill(prefs.upsertServer(initialServer))
            manualMode = true
        }
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
        // Dark scrim so the form stays legible over the art
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(
                    Brush.verticalGradient(
                        colors = listOf(
                            Color.Black.copy(alpha = 0.6f),
                            Color.Black.copy(alpha = 0.45f),
                            Color.Black.copy(alpha = 0.8f),
                        ),
                    )
                ),
        )
        Column(
            modifier = Modifier
                .fillMaxSize()
                .windowInsetsPadding(WindowInsets.safeDrawing)
                .verticalScroll(state = rememberScrollState())
                .drawWithContent { drawContent() }
                .padding(horizontal = 24.dp)
                .padding(top = 48.dp, bottom = 32.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            // Center the content vertically — the landing (logo + two buttons) is
            // short and looks stranded at the top otherwise. Taller content (the
            // manual form, saved servers) still scrolls from the top as normal.
            verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
        ) {
            // Circular badge logo
            Image(
                painter = painterResource(id = R.drawable.ic_logo),
                contentDescription = "Archipelago",
                modifier = Modifier.size(96.dp),
            )

            Spacer(modifier = Modifier.height(4.dp))

            Text(
                text = if (editingServer != null) stringResource(R.string.edit_server_title) else stringResource(R.string.connect_to_node),
                style = MaterialTheme.typography.headlineMedium,
                color = TextPrimary,
                textAlign = TextAlign.Center,
            )

            val showForm = manualMode || editingServer != null

            Text(
                text = if (showForm) stringResource(R.string.server_address_hint) else stringResource(R.string.connect_landing_hint),
                style = MaterialTheme.typography.bodyMedium,
                color = TextMuted,
                textAlign = TextAlign.Center,
            )

            Spacer(modifier = Modifier.height(4.dp))

            if (!showForm) {
                // Landing: scan the pairing QR, or fall back to manual entry
                GlassButton(
                    text = stringResource(R.string.scan_node_qr),
                    onClick = { showScanner = true },
                    modifier = Modifier.fillMaxWidth().height(56.dp),
                )
                GlassButton(
                    text = stringResource(R.string.enter_manually),
                    onClick = {
                        errorMessage = null
                        manualMode = true
                    },
                    modifier = Modifier.fillMaxWidth().height(56.dp),
                )
            }

            // Glass card with form
            if (showForm) Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(16.dp))
                    .background(Color.Black.copy(alpha = 0.6f))
                    .background(
                        Brush.verticalGradient(
                            colors = listOf(
                                Color.White.copy(alpha = 0.06f),
                                Color.White.copy(alpha = 0.02f),
                            ),
                        )
                    )
                    .border(1.dp, Color.White.copy(alpha = 0.1f), RoundedCornerShape(16.dp))
                    .padding(20.dp),
            ) {
                Column {
                    OutlinedTextField(
                        value = name,
                        onValueChange = {
                            name = it
                            errorMessage = null
                        },
                        label = { Text(stringResource(R.string.server_name_label)) },
                        placeholder = { Text(stringResource(R.string.server_name_placeholder)) },
                        modifier = Modifier.fillMaxWidth(),
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(
                            keyboardType = KeyboardType.Text,
                            imeAction = ImeAction.Next,
                        ),
                        colors = OutlinedTextFieldDefaults.colors(
                            focusedBorderColor = Color.White.copy(alpha = 0.3f),
                            unfocusedBorderColor = Color.White.copy(alpha = 0.12f),
                            cursorColor = Color.White,
                            focusedLabelColor = Color.White.copy(alpha = 0.7f),
                            unfocusedLabelColor = TextMuted,
                            focusedTextColor = TextPrimary,
                            unfocusedTextColor = TextPrimary,
                        ),
                        shape = RoundedCornerShape(12.dp),
                    )

                    Spacer(modifier = Modifier.height(12.dp))

                    OutlinedTextField(
                        value = address,
                        onValueChange = {
                            address = sanitizeAddress(it)
                            errorMessage = null
                        },
                        label = { Text(stringResource(R.string.server_address_label)) },
                        placeholder = { Text(stringResource(R.string.server_address_placeholder)) },
                        modifier = Modifier.fillMaxWidth(),
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(
                            keyboardType = KeyboardType.Uri,
                            imeAction = ImeAction.Next,
                        ),
                        colors = OutlinedTextFieldDefaults.colors(
                            focusedBorderColor = Color.White.copy(alpha = 0.3f),
                            unfocusedBorderColor = Color.White.copy(alpha = 0.12f),
                            cursorColor = Color.White,
                            focusedLabelColor = Color.White.copy(alpha = 0.7f),
                            unfocusedLabelColor = TextMuted,
                            focusedTextColor = TextPrimary,
                            unfocusedTextColor = TextPrimary,
                        ),
                        shape = RoundedCornerShape(12.dp),
                    )

                    Spacer(modifier = Modifier.height(12.dp))

                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        OutlinedTextField(
                            value = port,
                            onValueChange = {
                                port = it.filter { c -> c.isDigit() }.take(5)
                                errorMessage = null
                            },
                            label = { Text(stringResource(R.string.port_label)) },
                            placeholder = { Text("80") },
                            modifier = Modifier.weight(1f),
                            singleLine = true,
                            keyboardOptions = KeyboardOptions(
                                keyboardType = KeyboardType.Number,
                                imeAction = ImeAction.Next,
                            ),
                            colors = OutlinedTextFieldDefaults.colors(
                                focusedBorderColor = Color.White.copy(alpha = 0.3f),
                                unfocusedBorderColor = Color.White.copy(alpha = 0.12f),
                                cursorColor = Color.White,
                                focusedLabelColor = Color.White.copy(alpha = 0.7f),
                                unfocusedLabelColor = TextMuted,
                                focusedTextColor = TextPrimary,
                                unfocusedTextColor = TextPrimary,
                            ),
                            shape = RoundedCornerShape(12.dp),
                        )

                        OutlinedTextField(
                            value = password,
                            onValueChange = {
                                password = it
                                errorMessage = null
                            },
                            label = { Text("Password") },
                            modifier = Modifier.weight(2f),
                            singleLine = true,
                            visualTransformation = if (passwordVisible) VisualTransformation.None else PasswordVisualTransformation(),
                            trailingIcon = {
                                IconButton(onClick = { passwordVisible = !passwordVisible }) {
                                    Icon(
                                        imageVector = if (passwordVisible) Icons.Default.VisibilityOff else Icons.Default.Visibility,
                                        contentDescription = if (passwordVisible) "Hide password" else "Show password",
                                        tint = TextMuted,
                                        modifier = Modifier.size(20.dp),
                                    )
                                }
                            },
                            keyboardOptions = KeyboardOptions(
                                keyboardType = KeyboardType.Password,
                                imeAction = ImeAction.Go,
                            ),
                            keyboardActions = KeyboardActions(
                                onGo = {
                                    keyboard?.hide()
                                    if (editingServer != null) {
                                        saveEdit()
                                    } else {
                                        connect(ServerEntry(address, useHttps, port, password, name))
                                    }
                                },
                            ),
                            colors = OutlinedTextFieldDefaults.colors(
                                focusedBorderColor = Color.White.copy(alpha = 0.3f),
                                unfocusedBorderColor = Color.White.copy(alpha = 0.12f),
                                cursorColor = Color.White,
                                focusedLabelColor = Color.White.copy(alpha = 0.7f),
                                unfocusedLabelColor = TextMuted,
                                focusedTextColor = TextPrimary,
                                unfocusedTextColor = TextPrimary,
                            ),
                            shape = RoundedCornerShape(12.dp),
                        )
                    }

                    Spacer(modifier = Modifier.height(16.dp))

                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Icon(
                                imageVector = if (useHttps) Icons.Default.Lock else Icons.Default.LockOpen,
                                contentDescription = null,
                                modifier = Modifier.size(20.dp),
                                tint = if (useHttps) SuccessGreen else TextMuted,
                            )
                            Spacer(modifier = Modifier.width(8.dp))
                            Text(
                                text = stringResource(R.string.use_https),
                                style = MaterialTheme.typography.bodyMedium,
                                color = TextSecondary,
                            )
                        }
                        Switch(
                            checked = useHttps,
                            onCheckedChange = { useHttps = it },
                            colors = SwitchDefaults.colors(
                                checkedThumbColor = SurfaceBlack,
                                checkedTrackColor = BitcoinOrange,
                                uncheckedThumbColor = TextMuted,
                                uncheckedTrackColor = SurfaceCard,
                            ),
                        )
                    }
                }
            }

            // Error
            AnimatedVisibility(visible = errorMessage != null, enter = fadeIn(), exit = fadeOut()) {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(12.dp))
                        .background(ErrorRed.copy(alpha = 0.12f))
                        .border(1.dp, ErrorRed.copy(alpha = 0.25f), RoundedCornerShape(12.dp))
                        .padding(12.dp),
                ) {
                    Text(text = errorMessage ?: "", color = ErrorRed, style = MaterialTheme.typography.bodyMedium)
                }
            }

            if (editingServer != null) {
                // Save / Cancel while editing an existing saved server
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    GlassButton(
                        text = stringResource(R.string.cancel),
                        onClick = {
                            keyboard?.hide()
                            cancelEdit()
                        },
                        modifier = Modifier.weight(1f).height(56.dp),
                    )
                    GlassButton(
                        text = stringResource(R.string.save_changes),
                        onClick = {
                            keyboard?.hide()
                            saveEdit()
                        },
                        modifier = Modifier.weight(1f).height(56.dp),
                    )
                }
            } else if (manualMode) {
                // Back to the Scan/Manual landing + Connect
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    GlassButton(
                        text = stringResource(R.string.back),
                        onClick = {
                            keyboard?.hide()
                            manualMode = false
                            clearForm()
                        },
                        modifier = Modifier.weight(1f).height(56.dp),
                    )
                    GlassButton(
                        text = if (isConnecting) stringResource(R.string.connecting) else stringResource(R.string.connect),
                        onClick = {
                            keyboard?.hide()
                            connect(ServerEntry(address, useHttps, port, password, name))
                        },
                        modifier = Modifier.weight(2f).height(56.dp),
                    )
                }
            }

            if (isConnecting) {
                SlidingLoader(
                    modifier = Modifier.fillMaxWidth(),
                    done = connectSucceeded,
                )
            }

            // Saved servers (hidden while editing one to keep focus on the form)
            if (editingServer == null && savedServers.isNotEmpty()) {
                Spacer(modifier = Modifier.height(8.dp))
                Text(
                    text = stringResource(R.string.saved_servers),
                    style = MaterialTheme.typography.labelMedium,
                    color = TextMuted,
                    letterSpacing = 1.sp,
                    modifier = Modifier.fillMaxWidth(),
                )

                savedServers.forEach { server ->
                    SavedServerItem(
                        server = server,
                        onConnect = { connect(it) },
                        onEdit = { startEdit(it) },
                        onRemove = { scope.launch { prefs.removeSavedServer(it) } },
                    )
                }
            }
        }

        QrScannerOverlay(
            visible = showScanner,
            onDismiss = { showScanner = false },
            onServerScanned = { onQrScanned(it) },
        )

        // Full-screen branded loader while the first connect runs — most
        // visibly right after a pairing-QR scan, when the mesh may still be
        // establishing (LAN probe → tunnel up → ULA probe can take a while).
        // The small inline spinner stays for context; this owns the screen.
        if (isConnecting) {
            MeshLoadingScreen(
                mesh = connectingOverMesh,
                nodeName = connectingName,
                done = connectSucceeded,
            )
        }
    }
}

@Composable
private fun SavedServerItem(
    server: ServerEntry,
    onConnect: (ServerEntry) -> Unit,
    onEdit: (ServerEntry) -> Unit,
    onRemove: (ServerEntry) -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(Color.Black.copy(alpha = 0.6f))
            .background(
                Brush.verticalGradient(
                    colors = listOf(
                        Color.White.copy(alpha = 0.06f),
                        Color.White.copy(alpha = 0.02f),
                    ),
                )
            )
            .border(1.dp, Color.White.copy(alpha = 0.1f), RoundedCornerShape(12.dp))
            .clickable { onConnect(server) }
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.weight(1f)) {
            Icon(
                imageVector = if (server.useHttps) Icons.Default.Lock else Icons.Default.LockOpen,
                contentDescription = null,
                modifier = Modifier.size(20.dp),
                tint = if (server.useHttps) SuccessGreen else BitcoinOrange,
            )
            Spacer(modifier = Modifier.width(12.dp))
            Column {
                Text(text = server.displayName(), style = MaterialTheme.typography.bodyMedium, color = TextPrimary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                val secondary = buildString {
                    if (server.name.isNotBlank()) append(server.address)
                    if (server.port.isNotBlank()) {
                        if (isNotEmpty()) append(":${server.port}") else append("Port ${server.port}")
                    }
                }
                if (secondary.isNotBlank()) {
                    Text(text = secondary, style = MaterialTheme.typography.labelMedium, color = TextMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
        IconButton(onClick = { onEdit(server) }) {
            Icon(imageVector = Icons.Default.Edit, contentDescription = stringResource(R.string.edit_server), modifier = Modifier.size(18.dp), tint = TextMuted)
        }
        IconButton(onClick = { onRemove(server) }) {
            Icon(imageVector = Icons.Default.Close, contentDescription = stringResource(R.string.remove_server), modifier = Modifier.size(18.dp), tint = TextMuted)
        }
    }
}

/** Strip protocol prefixes and trailing slashes from address input. */
private fun sanitizeAddress(input: String): String {
    return input.trim()
        .removePrefix("https://")
        .removePrefix("http://")
        .trimEnd('/')
}

// Built once — the connect loop probed up to 20 times, and each attempt was
// paying a fresh SSLContext + SecureRandom init.
private val trustAllSslFactory: javax.net.ssl.SSLSocketFactory by lazy {
    val trustAll = arrayOf<javax.net.ssl.TrustManager>(object : X509TrustManager {
        override fun checkClientTrusted(chain: Array<java.security.cert.X509Certificate>?, authType: String?) {}
        override fun checkServerTrusted(chain: Array<java.security.cert.X509Certificate>?, authType: String?) {}
        override fun getAcceptedIssuers(): Array<java.security.cert.X509Certificate> = arrayOf()
    })
    SSLContext.getInstance("TLS").apply { init(null, trustAll, java.security.SecureRandom()) }.socketFactory
}

/** Test RPC connectivity. Accepts self-signed certs for local LAN servers.
 *  [timeoutMs] is per-phase (connect / read) — mesh probes need far more
 *  patience than LAN ones (first session through the tree can take 15s+). */
private suspend fun testConnection(server: ServerEntry, timeoutMs: Int = 5000): Boolean {
    return withContext(Dispatchers.IO) {
        try {
            val url = URL("${server.toUrl()}/rpc/v1")
            val connection = url.openConnection() as HttpURLConnection

            // Trust self-signed certs for local HTTPS (Archipelago nodes rarely have CA certs)
            if (connection is HttpsURLConnection) {
                connection.sslSocketFactory = trustAllSslFactory
                connection.hostnameVerifier = javax.net.ssl.HostnameVerifier { _, _ -> true }
            }

            connection.requestMethod = "POST"
            connection.connectTimeout = timeoutMs
            connection.readTimeout = timeoutMs
            connection.setRequestProperty("Content-Type", "application/json")
            connection.doOutput = true
            val body = """{"method":"server.echo","params":{"message":"ping"}}"""
            connection.outputStream.use { it.write(body.toByteArray()) }
            val code = connection.responseCode
            connection.disconnect()
            code in 200..499
        } catch (_: Exception) {
            false
        }
    }
}
