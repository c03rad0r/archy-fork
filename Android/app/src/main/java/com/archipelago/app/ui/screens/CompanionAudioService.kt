package com.archipelago.app.ui.screens

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.media.AudioManager
import android.media.MediaMetadata
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.Bundle
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.SystemClock
import android.util.Base64
import androidx.core.content.ContextCompat
import com.archipelago.app.MainActivity
import com.archipelago.app.ui.screens.CompanionAudioDiagnostics.Event

/** Foreground ownership of the existing authenticated WebView player. No stream
 * URL, auth token or cookie is copied into native playback or notifications. */
class CompanionAudioService : Service() {
    companion object {
        internal var instance: CompanionAudioService? = null
        private const val CHANNEL = "companion-audio"
        private const val NOTIFICATION = 4056
    }
    private lateinit var media: MediaSession
    private val handler = Handler(Looper.getMainLooper())
    private var lastArtwork = ""
    private var bitmap: Bitmap? = null
    private var finishing = false
    private val noisy = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            if (intent?.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY) CompanionAudioBridge.command("pause")
        }
    }
    private val watchdog = object : Runnable {
        override fun run() {
            val state = CompanionAudioBridge.state ?: return
            val age = SystemClock.elapsedRealtime() - CompanionAudioBridge.updatedAt
            if (age > 90000) { CompanionAudioDiagnostics.record(Event.HEARTBEAT_EXPIRED); CompanionAudioBridge.stop() }
            else {
                if (age > 15000) CompanionAudioBridge.command("sync")
                handler.postDelayed(this, 5000)
            }
        }
    }
    override fun onCreate() {
        super.onCreate(); instance = this
        CompanionAudioDiagnostics.record(Event.SERVICE_CREATED)
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(CHANNEL, "Audio playback", NotificationManager.IMPORTANCE_LOW))
        media = MediaSession(this, "Archipelago audio")
        media.setCallback(object : MediaSession.Callback() {
            override fun onPlay() = CompanionAudioBridge.command("play")
            override fun onPause() = CompanionAudioBridge.command("pause")
            override fun onStop() = CompanionAudioBridge.stop()
            override fun onSkipToNext() { if (CompanionAudioBridge.state?.next == true) CompanionAudioBridge.command("next") }
            override fun onSkipToPrevious() { if (CompanionAudioBridge.state?.previous == true) CompanionAudioBridge.command("previous") }
            override fun onSeekTo(pos: Long) {
                val duration = CompanionAudioBridge.state?.duration ?: return
                CompanionAudioBridge.command("seek", (pos / 1000.0).coerceIn(0.0, duration))
            }
            override fun onCustomAction(action: String, extras: Bundle?) {
                if (action == "shuffle" && CompanionAudioBridge.state?.shuffle == true) CompanionAudioBridge.command("shuffle")
            }
        }, handler)
        media.setFlags(MediaSession.FLAG_HANDLES_MEDIA_BUTTONS or MediaSession.FLAG_HANDLES_TRANSPORT_CONTROLS)
        media.setSessionActivity(openPlayer())
        media.isActive = true
        ContextCompat.registerReceiver(this, noisy, IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY), ContextCompat.RECEIVER_NOT_EXPORTED)
        handler.postDelayed(watchdog, 5000)
    }
    override fun onBind(intent: Intent?): IBinder? = null
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        CompanionAudioDiagnostics.record(Event.SERVICE_STARTED)
        val state = CompanionAudioBridge.state
        if (state == null) { finishPlayback(); return START_NOT_STICKY }
        finishing = false; instance = this
        if (intent?.action != null && intent.getStringExtra("session") == state.session) {
            when (intent.action) {
                "stop" -> CompanionAudioBridge.stop()
                "play", "pause" -> CompanionAudioBridge.command(intent.action!!)
                "next" -> if (state.next) CompanionAudioBridge.command("next")
                "previous" -> if (state.previous) CompanionAudioBridge.command("previous")
                "shuffle" -> if (state.shuffle) CompanionAudioBridge.command("shuffle")
            }
        }
        if (!finishing) refresh()
        return START_NOT_STICKY // Never reconstruct an authorized stream after process death.
    }
    private fun openPlayer() = PendingIntent.getActivity(this, 0,
        Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    private fun action(name: String, label: String, icon: Int, session: String): Notification.Action {
        val intent = Intent(this, CompanionAudioService::class.java).setAction(name).putExtra("session", session)
        val pending = PendingIntent.getService(this, name.hashCode(), intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        return Notification.Action.Builder(icon, label, pending).build()
    }
    internal fun refresh() {
        if (finishing) return
        val state = CompanionAudioBridge.state ?: return
        if (state.artwork != lastArtwork) {
            lastArtwork = state.artwork
            bitmap = decodeArtwork(state.artwork)
        }
        val metadata = MediaMetadata.Builder().putString(MediaMetadata.METADATA_KEY_TITLE, state.title)
            .putString(MediaMetadata.METADATA_KEY_ARTIST, "Archipelago")
            .putLong(MediaMetadata.METADATA_KEY_DURATION, (state.duration * 1000).toLong())
        bitmap?.let { metadata.putBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART, it) }
        media.setMetadata(metadata.build())
        var actions = PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or PlaybackState.ACTION_PLAY_PAUSE or PlaybackState.ACTION_STOP
        if (state.duration > 0) actions = actions or PlaybackState.ACTION_SEEK_TO
        if (state.previous) actions = actions or PlaybackState.ACTION_SKIP_TO_PREVIOUS
        if (state.next) actions = actions or PlaybackState.ACTION_SKIP_TO_NEXT
        val playback = PlaybackState.Builder().setActions(actions)
            .setState(if (state.playing) PlaybackState.STATE_PLAYING else PlaybackState.STATE_PAUSED,
                (state.position * 1000).toLong(), if (state.playing) 1f else 0f, SystemClock.elapsedRealtime())
        if (state.shuffle) playback.addCustomAction("shuffle", if (state.shuffled) "Shuffle on" else "Shuffle off", android.R.drawable.ic_menu_rotate)
        media.setPlaybackState(playback.build())
        val controls = mutableListOf<Notification.Action>()
        if (state.previous) controls.add(action("previous", "Previous", android.R.drawable.ic_media_previous, state.session))
        controls.add(action(if (state.playing) "pause" else "play", if (state.playing) "Pause" else "Play",
            if (state.playing) android.R.drawable.ic_media_pause else android.R.drawable.ic_media_play, state.session))
        if (state.next) controls.add(action("next", "Next", android.R.drawable.ic_media_next, state.session))
        val compact = controls.indices.toList().toIntArray()
        if (state.shuffle) controls.add(action("shuffle", if (state.shuffled) "Shuffle on" else "Shuffle off", android.R.drawable.ic_menu_rotate, state.session))
        controls.add(action("stop", "Stop", android.R.drawable.ic_menu_close_clear_cancel, state.session))
        val notification = Notification.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.ic_media_play).setContentTitle(state.title).setContentText("Archipelago")
            .setContentIntent(openPlayer()).setOnlyAlertOnce(true).setOngoing(state.playing)
            .setVisibility(Notification.VISIBILITY_PUBLIC).setCategory(Notification.CATEGORY_TRANSPORT)
            .setStyle(Notification.MediaStyle().setMediaSession(media.sessionToken).setShowActionsInCompactView(*compact))
            .setActions(*controls.toTypedArray())
        bitmap?.let { notification.setLargeIcon(it) }
        startForeground(NOTIFICATION, notification.build())
        CompanionAudioDiagnostics.record(Event.FOREGROUND_ACTIVE)
    }
    override fun onTaskRemoved(rootIntent: Intent?) {
        if (CompanionAudioBridge.state?.playing != true) CompanionAudioBridge.stop()
        super.onTaskRemoved(rootIntent)
    }
    internal fun finishPlayback() {
        if (finishing) return
        finishing = true
        CompanionAudioDiagnostics.record(Event.SERVICE_FINISHED)
        if (instance === this) instance = null
        stopForeground(STOP_FOREGROUND_REMOVE); stopSelf()
    }
    override fun onDestroy() {
        CompanionAudioDiagnostics.record(Event.SERVICE_DESTROYED)
        handler.removeCallbacksAndMessages(null)
        runCatching { unregisterReceiver(noisy) }
        media.isActive = false; media.release(); bitmap = null
        if (instance === this) { instance = null; CompanionAudioBridge.stop() }
        super.onDestroy()
    }
}

internal fun decodeArtwork(data: String): Bitmap? = runCatching {
    if (!data.startsWith("data:image/jpeg;base64,") || data.length > 90000) return null
    val bytes = Base64.decode(data.substringAfter(','), Base64.NO_WRAP)
    if (bytes.size < 4 || bytes[0] != 0xff.toByte() || bytes[1] != 0xd8.toByte() || bytes[2] != 0xff.toByte()) return null
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
    if (bounds.outWidth !in 1..512 || bounds.outHeight !in 1..512) return null
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
}.getOrNull()
