package com.archipelago.app.ui.screens

import android.view.View
import android.widget.FrameLayout
import androidx.activity.ComponentActivity
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(manifest = Config.NONE, sdk = [28, 35])
class WebViewFullscreenTest {
    @Test fun closingVideoInPipKeepsOpaqueCoverUntilActivityReturns() {
        val lifecycle = Robolectric.buildActivity(ComponentActivity::class.java).setup()
        try {
            val activity = lifecycle.get()
            val fullscreen = WebViewFullscreen(activity)
            val video = View(activity)
            var hidden = 0
            fullscreen.show(video) { hidden++ }
            assertTrue(activity.enterPictureInPictureMode(android.app.PictureInPictureParams.Builder().build()))
            assertTrue(activity.isInPictureInPictureMode)
            fullscreen.hide()
            assertNull(video.parent)
            assertTrue(fullscreen.isActive)
            assertEquals(1, hidden)
            fullscreen.hide()
            assertEquals(1, hidden)
        } finally { lifecycle.pause().stop().destroy() }
    }

    @Test fun backExitsFullscreenWithoutFinishingActivityAndNotifiesOnce() {
        val lifecycle = Robolectric.buildActivity(ComponentActivity::class.java).setup()
        try {
            val activity = lifecycle.get()
            val fullscreen = WebViewFullscreen(activity)
            val video = View(activity)
            var hidden = 0
            fullscreen.show(video) { hidden++ }
            assertNotNull(video.parent)
            activity.onBackPressedDispatcher.onBackPressed()
            assertNull(video.parent)
            assertFalse(activity.isFinishing)
            assertEquals(1, hidden)
            fullscreen.hide()
            assertEquals(1, hidden)
        } finally { lifecycle.pause().stop().destroy() }
    }

    @Test fun duplicateRequestPreservesActiveViewAndCanReenterAfterExit() {
        val lifecycle = Robolectric.buildActivity(ComponentActivity::class.java).setup()
        try {
            val activity = lifecycle.get()
            val fullscreen = WebViewFullscreen(activity)
            val first = View(activity)
            val second = View(activity)
            var firstHidden = 0
            var secondHidden = 0
            fullscreen.show(first) { firstHidden++ }
            fullscreen.show(second) { secondHidden++ }
            assertNotNull(first.parent)
            assertNull(second.parent)
            assertEquals(0, firstHidden)
            assertEquals(1, secondHidden)
            fullscreen.hide()
            fullscreen.show(second) { secondHidden++ }
            assertNotNull(second.parent)
            fullscreen.hide()
            assertEquals(1, firstHidden)
            assertEquals(2, secondHidden)
        } finally { lifecycle.pause().stop().destroy() }
    }

    @Test fun rejectsOwnedViewWithoutReparentingAndHandlesUnavailableActivity() {
        val lifecycle = Robolectric.buildActivity(ComponentActivity::class.java).setup()
        try {
            val activity = lifecycle.get()
            val video = View(activity)
            val owner = FrameLayout(activity).apply { addView(video) }
            var hidden = 0
            WebViewFullscreen(activity).show(video) { hidden++ }
            assertSame(owner, video.parent)
            WebViewFullscreen(null).show(null) { hidden++ }
            assertEquals(2, hidden)
        } finally { lifecycle.pause().stop().destroy() }
    }
}
