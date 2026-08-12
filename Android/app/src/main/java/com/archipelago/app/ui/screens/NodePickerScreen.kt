package com.archipelago.app.ui.screens

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
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Bolt
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.LockOpen
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.archipelago.app.R
import com.archipelago.app.data.ServerEntry
import com.archipelago.app.ui.theme.BitcoinOrange
import com.archipelago.app.ui.theme.SuccessGreen
import com.archipelago.app.ui.theme.SurfaceBlack
import com.archipelago.app.ui.theme.TextMuted
import com.archipelago.app.ui.theme.TextPrimary

/**
 * "Which node?" — shown at launch when more than one node is saved.
 *
 * The companion used to dive straight back into whichever node was last
 * active, which is wrong the moment a user keeps more than one: they arrive
 * somewhere they didn't choose, and (worse) the FIPS tunnel came up before
 * anyone said which network this session belongs to. Picking first makes the
 * choice explicit and lets the mesh stay down for nodes that aren't on it.
 *
 * [onPick] carries the entry; the caller decides what the mesh does about it.
 */
@Composable
fun NodePickerScreen(
    servers: List<ServerEntry>,
    lastActive: ServerEntry?,
    onPick: (ServerEntry) -> Unit,
    onAddNode: () -> Unit,
) {
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
                .padding(top = 48.dp, bottom = 32.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
        ) {
            Image(
                painter = painterResource(id = R.drawable.ic_logo),
                contentDescription = "Archipelago",
                modifier = Modifier.size(88.dp),
            )

            Spacer(Modifier.height(4.dp))

            Text(
                text = stringResource(R.string.pick_node_title),
                style = MaterialTheme.typography.headlineMedium,
                color = TextPrimary,
                textAlign = TextAlign.Center,
            )
            Text(
                text = stringResource(R.string.pick_node_hint),
                style = MaterialTheme.typography.bodyMedium,
                color = TextMuted,
                textAlign = TextAlign.Center,
            )

            Spacer(Modifier.height(8.dp))

            servers.forEach { server ->
                NodeCard(
                    server = server,
                    isLast = lastActive?.sameNode(server) == true,
                    onClick = { onPick(server) },
                )
            }

            Spacer(Modifier.height(8.dp))

            GlassButton(
                text = stringResource(R.string.pick_node_add),
                onClick = onAddNode,
                modifier = Modifier.fillMaxWidth().height(52.dp),
            )
        }
    }
}

@Composable
private fun NodeCard(
    server: ServerEntry,
    isLast: Boolean,
    onClick: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(Color.Black.copy(alpha = 0.6f))
            .background(
                Brush.verticalGradient(
                    colors = listOf(
                        Color.White.copy(alpha = 0.08f),
                        Color.White.copy(alpha = 0.02f),
                    ),
                )
            )
            .border(
                1.dp,
                if (isLast) BitcoinOrange.copy(alpha = 0.35f) else Color.White.copy(alpha = 0.1f),
                RoundedCornerShape(14.dp),
            )
            .clickable { onClick() }
            .padding(horizontal = 16.dp, vertical = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            imageVector = if (server.useHttps) Icons.Default.Lock else Icons.Default.LockOpen,
            contentDescription = null,
            modifier = Modifier.size(20.dp),
            tint = if (server.useHttps) SuccessGreen else BitcoinOrange,
        )
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(
                text = server.displayName(),
                style = MaterialTheme.typography.titleMedium,
                color = TextPrimary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            val secondary = buildString {
                if (server.name.isNotBlank()) append(server.address)
                if (server.port.isNotBlank()) {
                    if (isNotEmpty()) append(":${server.port}") else append("Port ${server.port}")
                }
            }
            if (secondary.isNotBlank()) {
                Text(
                    text = secondary,
                    style = MaterialTheme.typography.labelMedium,
                    color = TextMuted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        // The one thing that actually changes behaviour on this screen: a mesh
        // node brings the FIPS tunnel up, a plain one deliberately does not.
        if (server.isFipsNode()) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    imageVector = Icons.Default.Bolt,
                    contentDescription = null,
                    modifier = Modifier.size(14.dp),
                    tint = BitcoinOrange,
                )
                Spacer(Modifier.width(4.dp))
                Text(
                    text = "FIPS",
                    color = BitcoinOrange,
                    fontSize = 11.sp,
                    letterSpacing = 1.sp,
                    style = MaterialTheme.typography.labelMedium,
                )
            }
        }
    }
}
