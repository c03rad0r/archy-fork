package com.archipelago.app.ui.screens

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.Before
import org.robolectric.Shadows
import org.robolectric.RuntimeEnvironment
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE, sdk = [28, 35])
class CompanionAudioTest {
    @Before fun compatReceiverPermission() {
        val app = RuntimeEnvironment.getApplication()
        // The real merged manifest contributes this AndroidX permission.
        Shadows.shadowOf(app).grantPermissions(app.packageName + ".DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION")
    }
    @Test fun actualBridgeBindsOriginSessionAndSequenceAndReleasesStoppedPlayback() {
        val app = RuntimeEnvironment.getApplication()
        val web = android.webkit.WebView(app)
        web.loadUrl("https://node.test/cloud")
        val events = mutableListOf<JSONObject>()
        fun send(message: JSONObject, origin: String = "https://node.test", main: Boolean = true) {
            CompanionAudioBridge.receive(web, message.toString(), origin, main, setOf("https://node.test")) { events.add(JSONObject(it)) }
        }
        try {
            send(state(), main = false); assertNull(CompanionAudioBridge.state)
            send(state(), origin = "https://foreign.test"); assertNull(CompanionAudioBridge.state)
            send(state()); assertTrue(CompanionAudioBridge.retains(web))
            send(state().put("sequence", 0).put("playing", false)); assertTrue(CompanionAudioBridge.state!!.playing)
            CompanionAudioBridge.command("seek", 32.0)
            assertEquals("seek", events.last().getString("command")); assertEquals(32.0, events.last().getDouble("position"), 0.0)
            send(state().put("sequence", 2).put("playing", false)); assertFalse(CompanionAudioBridge.state!!.playing)
            CompanionAudioBridge.stop(); assertNull(CompanionAudioBridge.state)
            assertEquals("stop", events.last().getString("command"))
            send(state().put("sequence", 3)); assertNull(CompanionAudioBridge.state) // delayed state cannot revive a stopped session
        } finally { CompanionAudioBridge.release(web); web.destroy() }
    }
    @Test fun liveBridgeBuildsForegroundMediaNotificationForSameSession() {
        val app = RuntimeEnvironment.getApplication()
        val web = android.webkit.WebView(app); web.loadUrl("https://node.test/cloud")
        val payload = state().put("session", "22345678-1234-1234-1234-123456789abc")
        CompanionAudioBridge.receive(web, payload.toString(), "https://node.test", true, setOf("https://node.test")) {}
        val lifecycle = Robolectric.buildService(CompanionAudioService::class.java).create()
        try {
            lifecycle.get().onStartCommand(null, 0, 1)
            val notification = Shadows.shadowOf(lifecycle.get()).lastForegroundNotification
            assertNotNull(notification)
            assertEquals("Current song", notification.extras.getString(android.app.Notification.EXTRA_TITLE))
            assertEquals(5, notification.actions.size)
            assertNotNull(notification.extras.getParcelable<android.media.session.MediaSession.Token>(android.app.Notification.EXTRA_MEDIA_SESSION))
            lifecycle.get().onTaskRemoved(null)
            assertTrue(CompanionAudioBridge.retains(web))
        } finally { CompanionAudioBridge.release(web); lifecycle.destroy(); web.destroy() }
    }

    private fun state() = JSONObject("""{"version":1,"action":"state","session":"12345678-1234-1234-1234-123456789abc","sequence":1,"title":"Current song","playing":true,"position":10,"duration":120,"previous":true,"next":true,"shuffle":true,"shuffled":false}""")
    @Test fun malformedOrUnboundedMetadataCannotBecomeNativePlayback() {
        for ((key, value) in listOf("version" to 2, "session" to "foreign", "sequence" to -1,
                "position" to -1, "position" to 121, "duration" to 604801, "title" to "x".repeat(513),
                "artwork" to "https://node.test/protected?token=secret")) {
            assertTrue("Must reject $key", runCatching { CompanionAudioState.parse(state().put(key, value)) }.isFailure)
        }
    }
    @Test fun currentMetadataPreservesPauseSeekAndQueueCapabilities() {
        val parsed = CompanionAudioState.parse(state().put("playing", false).put("shuffled", true))
        assertFalse(parsed.playing); assertTrue(parsed.shuffled)
        assertTrue(parsed.previous && parsed.next && parsed.shuffle)
        assertEquals(10.0, parsed.position, 0.0)
        assertEquals(120.0, parsed.duration, 0.0)
        assertEquals("", parsed.artwork)
    }
    @Test fun artworkNeverFetchesProtectedUrlsAndRejectsInvalidBytes() {
        assertNull(decodeArtwork("https://node.test/protected"))
        assertNull(decodeArtwork("data:image/jpeg;base64,AAAA"))
        assertNull(decodeArtwork("data:image/jpeg;base64," + "A".repeat(90000)))
    }
    @Test fun serviceRestartWithoutLiveAuthorizedSessionDoesNotResumePlayback() {
        val lifecycle = Robolectric.buildService(CompanionAudioService::class.java).create()
        try {
            assertEquals(android.app.Service.START_NOT_STICKY, lifecycle.get().onStartCommand(null, 0, 1))
            assertNull(CompanionAudioBridge.state)
            assertNull(CompanionAudioService.instance)
        } finally { lifecycle.destroy() }
    }
}
