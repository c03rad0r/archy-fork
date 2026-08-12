package com.archipelago.app.ui.components

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.archipelago.app.R
import com.archipelago.app.ui.theme.BitcoinOrange
import com.archipelago.app.ui.theme.SurfaceBlack
import com.archipelago.app.ui.theme.TextMuted
import com.archipelago.app.ui.theme.TextPrimary

/**
 * Full-screen loader shown while the app is dialing a node.
 *
 * Two faces, because they are two different promises:
 *  - [mesh] `true` — a FIPS node: the branded "F*CK IPs" screen, because what
 *    is loading really is a connection to a cryptographic identity, not an IP.
 *  - [mesh] `false` — a plain node reached over the network like anything
 *    else. No mesh branding at all: claiming the mesh is carrying a connection
 *    it isn't is worse than an anonymous spinner.
 */
@Composable
fun MeshLoadingScreen(
    mesh: Boolean = true,
    nodeName: String = "",
    done: Boolean = false,
) {
    Box(
        Modifier
            .fillMaxSize()
            .background(SurfaceBlack),
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            // The app's own badge — the same ringed mark as the launcher icon
            // and the system splash, so launch → splash → this screen is one
            // continuous identity.
            Image(
                painter = painterResource(id = R.drawable.ic_logo),
                contentDescription = null,
                modifier = Modifier.size(112.dp),
            )
            Spacer(Modifier.height(24.dp))
            Text(
                text = if (mesh) "F*CK IPS MESH" else "CONNECTING",
                color = BitcoinOrange,
                fontSize = 16.sp,
                fontWeight = FontWeight.Bold,
                letterSpacing = 4.sp,
            )
            Spacer(Modifier.height(10.dp))
            Text(
                text = when {
                    mesh -> "Dialing your node by its key — no IPs harmed"
                    nodeName.isNotBlank() -> "Reaching $nodeName"
                    else -> "Reaching your node"
                },
                color = if (done) TextPrimary else TextMuted,
                fontSize = 13.sp,
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(horizontal = 32.dp),
            )
            Spacer(Modifier.height(28.dp))
            SlidingLoader(
                modifier = Modifier.width(220.dp),
                done = done,
            )
        }
    }
}
