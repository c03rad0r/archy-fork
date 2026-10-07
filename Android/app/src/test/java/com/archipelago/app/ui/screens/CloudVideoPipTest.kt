package com.archipelago.app.ui.screens

import org.junit.Assert.*
import org.junit.Test

class CloudVideoPipTest {
    @Test fun nativeChannelRejectsSiblingFrameAndStaleOrForeignPage() {
        val allowed = setOf("https://node.test", "http://[fd00::1]")
        assertTrue(cloudVideoSenderAllowed("https://node.test/cloud", "https://node.test", allowed, true))
        assertFalse(cloudVideoSenderAllowed("https://node.test/cloud", "https://node.test", allowed, false))
        assertFalse(cloudVideoSenderAllowed("https://other.test", "https://node.test", allowed, true))
        assertFalse(cloudVideoSenderAllowed("https://node.test:7778", "https://node.test:7778", allowed, true))
        assertFalse(cloudVideoSenderAllowed("https://node.test", "http://node.test", allowed, true))
    }

    @Test fun exactOriginIncludesSchemeAndNonDefaultPort() {
        assertEquals("https://node.test", cloudVideoOrigin("https://NODE.test:443/cloud"))
        assertEquals("http://node.test:8080", cloudVideoOrigin("http://node.test:8080/cloud?file=video"))
        assertEquals("http://[fd00::1]", cloudVideoOrigin("http://[fd00::1]/cloud"))
        assertNotEquals(cloudVideoOrigin("https://node.test"), cloudVideoOrigin("http://node.test"))
        assertNotEquals(cloudVideoOrigin("https://node.test"), cloudVideoOrigin("https://node.test:7778"))
    }
    @Test fun unsupportedAndCredentialOriginsCannotReceiveBridge() {
        for (url in listOf("javascript:alert(1)", "file:///video", "data:text/plain,video", "https://user:password@node.test/video", "not-a-url")) assertNull(cloudVideoOrigin(url))
    }
}
