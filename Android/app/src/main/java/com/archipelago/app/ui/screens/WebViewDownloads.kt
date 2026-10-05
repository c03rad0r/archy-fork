package com.archipelago.app.ui.screens

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.webkit.CookieManager
import android.webkit.DownloadListener
import android.webkit.URLUtil
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalContext
import kotlinx.coroutines.*
import okhttp3.Call
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.IOException
import java.io.OutputStream
import java.util.concurrent.TimeUnit

internal data class WebDownload(val url: String, val userAgent: String, val cookies: String, val name: String, val mime: String)

/** Only the starting origin receives its WebView cookies, even across redirects. */
internal fun streamWebDownload(
    download: WebDownload,
    output: OutputStream,
    client: OkHttpClient,
    onCall: (Call) -> Unit = {},
    checkCancelled: () -> Unit = {},
    onProgress: (Long, Long) -> Unit = { _, _ -> },
): Long {
    val transport = client.newBuilder().followRedirects(false).followSslRedirects(false).build()
    val original = download.url.toHttpUrlOrNull() ?: throw IOException("Unsupported download link")
    var url = original
    var redirects = 0
    while (true) {
        checkCancelled()
        if (url.username.isNotEmpty() || url.password.isNotEmpty()) throw IOException("Unsupported download link")
        val request = Request.Builder().url(url).header("User-Agent", download.userAgent)
        if (url.scheme == original.scheme && url.host == original.host && url.port == original.port && download.cookies.isNotBlank()) {
            request.header("Cookie", download.cookies)
        }
        val call = transport.newCall(request.build())
        onCall(call)
        call.execute().use { response ->
            if (response.code in listOf(301, 302, 303, 307, 308)) {
                if (++redirects > 5) throw IOException("Too many download redirects")
                val next = response.header("Location")?.let { url.resolve(it) } ?: throw IOException("Invalid download redirect")
                if (url.isHttps && !next.isHttps) throw IOException("Insecure download redirect blocked")
                url = next
            } else {
                if (response.code == 401 || response.code == 403) throw IOException("Sign in to the node again, then retry the download")
                if (!response.isSuccessful) throw IOException("Download failed (HTTP ${response.code})")
                if (response.header("Content-Type")?.substringBefore(';')?.trim()?.lowercase() == "text/html" &&
                    download.mime != "text/html" && !download.name.endsWith(".html", true) && !download.name.endsWith(".htm", true)) {
                    throw IOException("Sign in to the node again, then retry the download")
                }
                val body = response.body ?: throw IOException("The download was empty")
                val total = body.contentLength()
                var written = 0L
                body.byteStream().use { input ->
                    val buffer = ByteArray(64 * 1024)
                    var lastUpdate = 0L
                    while (true) {
                        checkCancelled()
                        val count = input.read(buffer)
                        if (count == -1) break
                        output.write(buffer, 0, count)
                        written += count
                        val now = System.nanoTime()
                        if (now - lastUpdate > 100_000_000L) { onProgress(written, total); lastUpdate = now }
                    }
                }
                if (total >= 0 && written != total) throw IOException("Download interrupted; please retry")
                onProgress(written, total)
                return written
            }
        }
    }
}

/** Uses the system Save dialog: no broad storage permission and no external browser login. */
@Composable
internal fun rememberWebViewDownloads(): DownloadListener {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var pending by remember { mutableStateOf<WebDownload?>(null) }
    var active by remember { mutableStateOf<WebDownload?>(null) }
    var progress by remember { mutableStateOf<Pair<Long, Long>>(0L to -1L) }
    var failure by remember { mutableStateOf<String?>(null) }
    var job by remember { mutableStateOf<Job?>(null) }
    val currentCall = remember { java.util.concurrent.atomic.AtomicReference<Call?>(null) }
    val client = remember {
        OkHttpClient.Builder().followRedirects(false).followSslRedirects(false)
            .connectTimeout(20, TimeUnit.SECONDS).readTimeout(60, TimeUnit.SECONDS).build()
    }
    fun cancel() { job?.cancel(); currentCall.getAndSet(null)?.cancel() }
    DisposableEffect(Unit) { onDispose { currentCall.getAndSet(null)?.cancel() } }
    val save = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        val download = pending
        pending = null
        val uri = result.data?.data
        if (result.resultCode != Activity.RESULT_OK || uri == null || download == null) return@rememberLauncherForActivityResult
        job = scope.launch {
            active = download
            progress = 0L to -1L
            var complete = false
            try {
                withContext(Dispatchers.IO) {
                    val task = currentCoroutineContext()
                    context.contentResolver.openOutputStream(uri, "w")?.use { output ->
                        streamWebDownload(download, output, client,
                            onCall = { call -> currentCall.set(call); if (!task.isActive) call.cancel() },
                            checkCancelled = { task.ensureActive() },
                            onProgress = { done, total -> scope.launch { progress = done to total } })
                    } ?: throw IOException("Unable to open the selected destination")
                }
                complete = true
                Toast.makeText(context, "Download complete: ${download.name}", Toast.LENGTH_LONG).show()
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                if (currentCoroutineContext().isActive) {
                    // Do not expose authenticated URLs or request headers in UI/logs.
                    failure = when {
                        error is javax.net.ssl.SSLException -> "The server certificate could not be verified."
                        error is IOException && error.message?.startsWith("Sign in") == true -> error.message
                        else -> "Download failed. Check your connection and available storage, then try again."
                    }
                }
            } finally {
                currentCall.getAndSet(null)?.cancel()
                if (!complete) withContext(NonCancellable + Dispatchers.IO) {
                    // This URI was newly created by ACTION_CREATE_DOCUMENT; never remove an existing user file.
                    runCatching { DocumentsContract.deleteDocument(context.contentResolver, uri) }
                }
                active = null
                job = null
            }
        }
    }
    if (active != null) {
        AlertDialog(onDismissRequest = {}, title = { Text("Downloading") }, text = {
            Column {
                Text(active!!.name)
                if (progress.second > 0) LinearProgressIndicator(progress = (progress.first.toFloat() / progress.second).coerceIn(0f, 1f))
                else LinearProgressIndicator()
            }
        }, confirmButton = {}, dismissButton = { TextButton(onClick = { cancel() }) { Text("Cancel") } })
    }
    failure?.let { message ->
        AlertDialog(onDismissRequest = { failure = null }, title = { Text("Download unavailable") },
            text = { Text(message) }, confirmButton = { TextButton(onClick = { failure = null }) { Text("OK") } })
    }
    return DownloadListener { url, userAgent, disposition, mimeType, _ ->
        if (active != null || pending != null) {
            Toast.makeText(context, "Finish or cancel the current download first", Toast.LENGTH_SHORT).show()
        } else if (url.toHttpUrlOrNull() == null) {
            failure = "This download link is not supported. Open the file from Cloud and try again."
        } else {
            val mime = mimeType?.substringBefore(';')?.takeIf { it.contains('/') } ?: "application/octet-stream"
            val name = URLUtil.guessFileName(url, disposition, mime).replace(Regex("[\\\\/\\p{Cntrl}]"), "_").take(180).ifBlank { "download" }
            pending = WebDownload(url, userAgent ?: "Archipelago Companion", CookieManager.getInstance().getCookie(url).orEmpty(), name, mime)
            try {
                save.launch(Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                    addCategory(Intent.CATEGORY_OPENABLE); type = mime; putExtra(Intent.EXTRA_TITLE, name)
                })
            } catch (_: Exception) { pending = null; failure = "No file-saving app is available on this device." }
        }
    }
}
