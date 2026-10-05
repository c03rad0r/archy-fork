package com.archipelago.app.ui.screens

import okhttp3.OkHttpClient
import okhttp3.ResponseBody.Companion.toResponseBody
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okio.Buffer
import org.junit.Assert.*
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.util.concurrent.CancellationException

class WebViewDownloadsTest {
    private fun spec(url: String) = WebDownload(url, "test-agent", "session=test-only", "file.bin", "application/octet-stream")
    @Test fun authenticatedDownloadWritesExactBytesAndReportsCompletion() {
        MockWebServer().use { server ->
            val bytes = ByteArray(256 * 1024 + 13) { (it % 251).toByte() }
            server.enqueue(MockResponse().setBody(Buffer().write(bytes)))
            val out = ByteArrayOutputStream()
            var progress = 0L to 0L
            assertEquals(bytes.size.toLong(), streamWebDownload(spec(server.url("/file").toString()), out, OkHttpClient(), onProgress = { done, total -> progress = done to total }))
            assertArrayEquals(bytes, out.toByteArray())
            assertEquals(bytes.size.toLong() to bytes.size.toLong(), progress)
            assertEquals("session=test-only", server.takeRequest().getHeader("Cookie"))
        }
    }
    @Test fun sameOriginRedirectKeepsSessionButCrossOriginNeverReceivesIt() {
        MockWebServer().use { first -> MockWebServer().use { second ->
            second.enqueue(MockResponse().setBody("final"))
            first.enqueue(MockResponse().setResponseCode(302).addHeader("Location", "/relative"))
            first.enqueue(MockResponse().setResponseCode(307).addHeader("Location", second.url("/target")))
            val out = ByteArrayOutputStream()
            streamWebDownload(spec(first.url("/start").toString()), out, OkHttpClient())
            assertEquals("final", out.toString())
            assertEquals("session=test-only", first.takeRequest().getHeader("Cookie"))
            assertEquals("session=test-only", first.takeRequest().getHeader("Cookie"))
            assertNull(second.takeRequest().getHeader("Cookie"))
        } }
    }
    @Test fun authenticationFailureDoesNotSaveErrorBody() {
        MockWebServer().use { server ->
            server.enqueue(MockResponse().setResponseCode(401).setBody("login required"))
            val out = ByteArrayOutputStream()
            val error = assertThrows(IOException::class.java) { streamWebDownload(spec(server.url("/").toString()), out, OkHttpClient()) }
            assertTrue(error.message!!.startsWith("Sign in"))
            assertEquals(0, out.size())
        }
    }
    @Test fun redirectsAreBoundedAndUnsafeSchemesAreRejected() {
        MockWebServer().use { server ->
            repeat(6) { server.enqueue(MockResponse().setResponseCode(302).addHeader("Location", "/loop")) }
            assertThrows(IOException::class.java) { streamWebDownload(spec(server.url("/loop").toString()), ByteArrayOutputStream(), OkHttpClient()) }
            assertEquals(6, server.requestCount)
        }
        for (url in listOf("file:///etc/passwd", "data:text/plain,test", "blob:test")) {
            assertThrows(IOException::class.java) { streamWebDownload(spec(url), ByteArrayOutputStream(), OkHttpClient()) }
        }
    }
    @Test fun cancellationAndDestinationFailureAreNotReportedAsComplete() {
        MockWebServer().use { server ->
            server.enqueue(MockResponse().setBody("bytes"))
            assertThrows(CancellationException::class.java) { streamWebDownload(spec(server.url("/").toString()), ByteArrayOutputStream(), OkHttpClient(), checkCancelled = { throw CancellationException() }) }
            assertEquals(0, server.requestCount)
            var progressCalled = false
            val out = object : java.io.OutputStream() { override fun write(b: Int) { throw IOException("disk full") } }
            assertThrows(IOException::class.java) { streamWebDownload(spec(server.url("/").toString()), out, OkHttpClient(), onProgress = { _, _ -> progressCalled = true }) }
            assertFalse(progressCalled)
        }
    }
    @Test fun tlsDowngradeAndLoginHtmlAreRejected() {
        var requests = 0
        val client = OkHttpClient.Builder().addInterceptor { chain ->
            requests++
            okhttp3.Response.Builder().request(chain.request()).protocol(okhttp3.Protocol.HTTP_1_1)
                .code(302).message("redirect").header("Location", "http://example.test/file").body("".toResponseBody(null)).build()
        }.build()
        assertThrows(IOException::class.java) { streamWebDownload(spec("https://example.test/file"), ByteArrayOutputStream(), client) }
        assertEquals(1, requests)
        MockWebServer().use { server ->
            server.enqueue(MockResponse().addHeader("Content-Type", "Text/HTML; charset=utf-8").setBody("<html>Sign in</html>"))
            val out = ByteArrayOutputStream()
            assertThrows(IOException::class.java) { streamWebDownload(spec(server.url("/file").toString()), out, OkHttpClient()) }
            assertEquals(0, out.size())
        }
    }

}
