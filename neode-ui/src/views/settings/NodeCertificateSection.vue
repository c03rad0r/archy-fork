<script setup lang="ts">
import { onMounted, ref } from 'vue'

// This node signs its own certificates with a CA that never leaves it. Install
// that CA once per device and every port on this node is trusted — which is what
// lets a gated app load inside the dashboard's frame at all: a cert warning
// cannot be clicked through inside an iframe, so an untrusted app port simply
// fails to render.

const fingerprint = ref('')
const fingerprintError = ref('')
const loading = ref(true)
const caAvailable = ref(false)

// SHA-256 over the DER bytes — the same number `openssl x509 -fingerprint
// -sha256` prints, so the two can be compared character for character.
async function computeFingerprint(pem: string): Promise<string> {
  const body = pem
    .replace(/-----BEGIN CERTIFICATE-----/, '')
    .replace(/-----END CERTIFICATE-----/, '')
    .replace(/\s+/g, '')
  const der = Uint8Array.from(atob(body), (c) => c.charCodeAt(0))
  const digest = await crypto.subtle.digest('SHA-256', der)
  return Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, '0').toUpperCase())
    .join(':')
}

onMounted(async () => {
  try {
    const res = await fetch('/ca.crt', { cache: 'no-store' })
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    const pem = await res.text()
    if (!pem.includes('BEGIN CERTIFICATE')) throw new Error('not a certificate')
    caAvailable.value = true

    // crypto.subtle only exists in a secure context. That is exactly the case
    // this feature is meant to fix, so an HTTP dashboard lands here — say so
    // and give the offline command rather than showing nothing.
    if (!window.crypto?.subtle) {
      fingerprintError.value =
        'The fingerprint cannot be computed over a plain HTTP connection. Verify it on the node instead: openssl x509 -in /etc/archipelago/ssl/ca.crt -noout -fingerprint -sha256'
    } else {
      fingerprint.value = await computeFingerprint(pem)
    }
  } catch {
    caAvailable.value = false
  } finally {
    loading.value = false
  }
})
</script>

<template>
  <!-- Node Certificate Section -->
  <div class="glass-card px-6 py-6 mb-6">
    <div class="mb-2">
      <h2 class="text-xl font-semibold text-white/96">Node certificate</h2>
    </div>
    <p class="text-sm text-white/60 mb-6">
      Install this node's certificate on a device and it stops warning you about
      this node — on every port, not just the dashboard. Apps that open inside
      the dashboard need this: a certificate warning cannot be accepted inside an
      embedded frame, so an untrusted app shows nothing at all.
    </p>

    <div v-if="loading" class="text-sm text-white/50">Checking…</div>

    <div
      v-else-if="!caAvailable"
      class="p-3 bg-white/5 border border-white/10 rounded-lg text-sm text-white/70"
    >
      This node has not generated a certificate authority yet. Run
      <code class="px-1 py-0.5 bg-black/30 rounded text-xs">scripts/setup-node-ca.sh</code>
      on the node, then reload this page.
    </div>

    <div v-else class="space-y-4">
      <div>
        <a
          href="/ca.crt"
          download="archipelago-node-ca.crt"
          class="inline-flex items-center gap-2 px-4 py-3 glass-button rounded-lg text-sm font-semibold"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
          </svg>
          Download this node's certificate
        </a>
      </div>

      <div>
        <p class="text-sm font-medium text-white/80 mb-1">Fingerprint (SHA-256)</p>
        <p v-if="fingerprint" class="font-mono text-xs text-white/70 break-all select-all">{{ fingerprint }}</p>
        <p v-else class="text-xs text-orange-300/80">{{ fingerprintError }}</p>
        <p class="text-xs text-white/50 mt-2">
          Check this matches the fingerprint the node itself prints before you trust
          it. If they differ, something is intercepting the connection — do not install it.
        </p>
      </div>

      <details class="group">
        <summary class="cursor-pointer text-sm font-medium text-white/80 py-2">
          How to install it
        </summary>
        <div class="mt-2 space-y-3 text-sm text-white/60">
          <p><strong class="text-white/80">macOS</strong> — open the file, add it to the
            <em>login</em> keychain, then find it in Keychain Access, open it, expand Trust
            and set “When using this certificate” to <em>Always Trust</em>.</p>
          <p><strong class="text-white/80">iOS / iPadOS</strong> — download it in Safari and
            allow the profile, then Settings → General → VPN &amp; Device Management to
            install it, and finally Settings → General → About → Certificate Trust Settings
            to switch it on. Both steps are required.</p>
          <p><strong class="text-white/80">Windows</strong> — right-click → Install
            Certificate → Local Machine → place it in <em>Trusted Root Certification
            Authorities</em>.</p>
          <p><strong class="text-white/80">Android</strong> — Settings → Security →
            Encryption &amp; credentials → Install a certificate → CA certificate.</p>
          <p><strong class="text-white/80">Linux</strong> — copy to
            <code class="px-1 py-0.5 bg-black/30 rounded text-xs">/usr/local/share/ca-certificates/</code>
            and run <code class="px-1 py-0.5 bg-black/30 rounded text-xs">sudo update-ca-certificates</code>.
            Firefox keeps its own store — add it under Settings → Privacy &amp; Security →
            View Certificates → Authorities.</p>
          <p class="text-white/50">
            You are trusting this node, not a company. The signing key stays on the node
            and only ever signs this node's own address. Anyone who takes the node also
            takes that key — remove the certificate from your devices if you retire it.
          </p>
        </div>
      </details>
    </div>
  </div>
</template>
