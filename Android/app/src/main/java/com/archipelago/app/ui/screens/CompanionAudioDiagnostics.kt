package com.archipelago.app.ui.screens

import android.app.NotificationManager
import android.content.Context
import android.os.Build
import android.os.SystemClock
import android.webkit.WebView

/** Local, memory-only allowlisted status. Never accepts URLs, titles, IDs, or exception text. */
internal object CompanionAudioDiagnostics {
    enum class Event {
        BRIDGE_ATTACHED, BRIDGE_UNSUPPORTED, NO_APPROVED_ORIGIN, ORIGIN_CHANGED,
        MESSAGE_RECEIVED, SENDER_REJECTED, MESSAGE_TOO_LARGE, INVALID_JSON, INVALID_STATE,
        RETIRED_SESSION, STALE_STATE, IDLE_STATE, STATE_ACCEPTED, SERVICE_REQUESTED,
        SERVICE_REQUEST_FAILED, SERVICE_PERMISSION_DENIED, SERVICE_BACKGROUND_START_DENIED, SERVICE_CREATED, SERVICE_STARTED, FOREGROUND_ACTIVE,
        SERVICE_FINISHED, SERVICE_DESTROYED, PAGE_RELEASED, SESSION_RELEASED, HEARTBEAT_EXPIRED,
    }
    private val counts = linkedMapOf<Event, Long>()
    private val recent = ArrayDeque<Pair<Long, Event>>()
    @Synchronized fun record(event: Event) {
        counts[event] = (counts[event] ?: 0) + 1
        // Position updates must not displace the useful startup/failure sequence.
        if (recent.lastOrNull()?.second != event && event !in setOf(Event.MESSAGE_RECEIVED, Event.STATE_ACCEPTED, Event.FOREGROUND_ACTIVE)) {
            recent.addLast(SystemClock.elapsedRealtime() to event)
            while (recent.size > 12) recent.removeFirst()
        }
    }
    @Synchronized internal fun events(): String = buildString {
        counts.forEach { (event, count) -> append("${event.name}: $count\n") }
        append("Recent transitions (seconds since boot):\n")
        recent.forEach { (at, event) -> append("${at / 1000}: ${event.name}\n") }
    }
    fun report(context: Context): String = buildString {
        val manager = context.getSystemService(NotificationManager::class.java)
        append("Companion playback diagnostics v1\n")
        val app = context.packageManager.getPackageInfo(context.packageName, 0)
        append("App: ${app.versionName}\n")
        append("Android API: ${Build.VERSION.SDK_INT}\n")
        append("WebView: ${WebView.getCurrentWebViewPackage()?.versionName ?: "unavailable"}\n")
        append("Notifications enabled: ${manager.areNotificationsEnabled()}\n")
        append("Audio channel importance: ${manager.getNotificationChannel("companion-audio")?.importance ?: "not created"}\n")
        append("Native session: ${CompanionAudioBridge.state != null}\n")
        append("Native playing: ${CompanionAudioBridge.state?.playing ?: false}\n")
        append("Service present: ${CompanionAudioService.instance != null}\n")
        append(events())
    }
}
