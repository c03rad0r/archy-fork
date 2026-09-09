<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { installCertificateInCompanion } from '@/utils/openExternal'

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

const generating = ref(false)
const generateError = ref('')

// WebUI rule: never point a user at a terminal — the backend runs the
// (idempotent) CA setup script for us.
async function generateCa() {
  generating.value = true
  generateError.value = ''
  try {
    const { rpcClient } = await import('@/api/rpc-client')
    await rpcClient.call({ method: 'system.node-ca.generate', timeout: 60000 })
    loading.value = true
    await probe()
    if (!caAvailable.value) {
      generateError.value = 'Generation reported success but the certificate is not being served yet — try reloading in a few seconds.'
    }
  } catch (e: unknown) {
    generateError.value = e instanceof Error ? e.message : 'Certificate generation failed'
  } finally {
    generating.value = false
  }
}

async function probe() {
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
}

function downloadCertificate(event: MouseEvent) {
  // Android WebView does not implement HTML downloads by itself. Ask the
  // companion to fetch this connected node's CA and open the system credential
  // installer; normal browsers keep the Content-Disposition download.
  if (installCertificateInCompanion()) {
    event.preventDefault()
  }
}

onMounted(probe)
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
      <p class="mb-3">This node has not generated its certificate yet.</p>
      <button
        :disabled="generating"
        @click="generateCa"
        class="px-4 py-2 glass-button rounded-lg text-sm font-semibold disabled:opacity-60"
      >{{ generating ? 'Generating…' : 'Generate certificate' }}</button>
      <p v-if="generateError" class="mt-2 text-xs text-orange-300/90">{{ generateError }}</p>
    </div>

    <div v-else class="space-y-4">
      <div>
        <a
          href="/ca.crt"
          download="archipelago-node-ca.crt"
          @click="downloadCertificate"
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
        <div class="mt-2 space-y-5 text-sm text-white/60">
          <p class="text-white/70">
            You are trusting this node, not a company. The signing key stays on the node
            and only ever signs this node's own address. Anyone who takes the node also
            takes that key — remove the certificate from your devices if you retire it.
          </p>

          <section class="space-y-2">
            <h4 class="font-semibold text-white/80">macOS</h4>
            <ol class="list-decimal pl-5 space-y-1">
              <li>Double-click the file to add it to your <em>login keychain</em>.</li>
              <li>Open Keychain Access and find it under Certificates.</li>
              <li>Open it, expand Trust, set “When using this certificate” to <em>Always Trust</em>, then close the window and enter your password.</li>
            </ol>
            <p>Quit and reopen your browser after changing the trust setting.</p>
          </section>

          <section class="space-y-2">
            <h4 class="font-semibold text-white/80">iOS / iPadOS</h4>
            <ol class="list-decimal pl-5 space-y-1">
              <li>Open the file in Safari and tap Allow to download the profile.</li>
              <li>Settings → Profile Downloaded, or General → VPN &amp; Device Management → Install.</li>
              <li>Settings → General → About → Certificate Trust Settings → switch the certificate on.</li>
            </ol>
            <p class="text-orange-200/80">The final Certificate Trust Settings step is required.</p>
          </section>

          <section class="space-y-2">
            <h4 class="font-semibold text-white/80">Windows</h4>
            <ol class="list-decimal pl-5 space-y-1">
              <li>Right-click the file and choose Install Certificate.</li>
              <li>Select Local Machine.</li>
              <li>Choose “Place all certificates in the following store” → Trusted Root Certification Authorities → Finish.</li>
            </ol>
          </section>

          <section class="space-y-2">
            <h4 class="font-semibold text-white/80">Android</h4>
            <p>Settings → Security → Encryption &amp; credentials → Install a certificate → CA certificate, then choose the file.</p>
            <p>Browsers using the system certificate store will trust it after restart. Apps that pin their own certificates may still refuse it.</p>
          </section>

          <section class="space-y-2">
            <h4 class="font-semibold text-white/80">Linux</h4>
            <pre class="overflow-x-auto rounded-lg bg-black/30 p-3 text-xs text-white/75"><code>sudo install -m644 /path/to/node-ca.crt /usr/local/share/ca-certificates/node-ca.crt &amp;&amp; sudo update-ca-certificates</code></pre>
            <p><strong class="text-white/70">Firefox:</strong> Settings → Privacy &amp; Security → View Certificates → Authorities → Import, then enable “Trust this CA to identify websites”.</p>
            <p><strong class="text-white/70">Arch / Manjaro:</strong></p>
            <pre class="overflow-x-auto rounded-lg bg-black/30 p-3 text-xs text-white/75"><code>sudo cp node-ca.crt /etc/ca-certificates/trust-source/anchors/ &amp;&amp; sudo update-ca-trust extract</code></pre>
          </section>

          <section class="space-y-2">
            <h4 class="font-semibold text-white/80">Restart the browser first</h4>
            <p>Chrome, Brave, Firefox, and Safari cache certificate decisions. Fully quit and reopen the browser before troubleshooting a certificate that still appears untrusted.</p>
            <p>For a one-visit sanity check on a machine you own, Chrome and Brave accept the keyboard shortcut <code class="px-1 py-0.5 bg-black/30 rounded text-xs">thisisunsafe</code> on the certificate error page. Use this only for testing.</p>
          </section>

          <section class="space-y-2">
            <h4 class="font-semibold text-white/80">If the node name does not resolve</h4>
            <p>Certificate trust and DNS are separate. If <code class="px-1 py-0.5 bg-black/30 rounded text-xs">node.local</code> does not resolve, prefer the node's Tailscale MagicDNS name when available.</p>
            <p>To keep using a local name on Linux or macOS, add the node address to <code class="px-1 py-0.5 bg-black/30 rounded text-xs">/etc/hosts</code>:</p>
            <pre class="overflow-x-auto rounded-lg bg-black/30 p-3 text-xs text-white/75"><code>echo '192.168.x.y  mynode.local' | sudo tee -a /etc/hosts</code></pre>
            <p>On Linux, if that still fails, inspect <code class="px-1 py-0.5 bg-black/30 rounded text-xs">grep '^hosts:' /etc/nsswitch.conf</code>. Put <code class="px-1 py-0.5 bg-black/30 rounded text-xs">files</code> before <code class="px-1 py-0.5 bg-black/30 rounded text-xs">mdns_minimal [NOTFOUND=return]</code> so an mDNS miss cannot block <code class="px-1 py-0.5 bg-black/30 rounded text-xs">/etc/hosts</code>.</p>
          </section>

          <section class="space-y-2">
            <h4 class="font-semibold text-white/80">Symptoms</h4>
            <div class="overflow-x-auto rounded-lg border border-white/10">
              <table class="w-full text-left text-xs">
                <thead class="bg-white/5 text-white/75">
                  <tr><th class="p-2">What you see</th><th class="p-2">Likely cause</th></tr>
                </thead>
                <tbody class="divide-y divide-white/10">
                  <tr><td class="p-2">Not trusted / ERR_CERT_AUTHORITY_INVALID</td><td class="p-2">The certificate is not installed, or the browser was not restarted.</td></tr>
                  <tr><td class="p-2">This site can't be reached / DNS error</td><td class="p-2">Name resolution, not TLS. Check the DNS guidance above.</td></tr>
                  <tr><td class="p-2">curl works, browser does not</td><td class="p-2">A separate browser certificate store or a stale browser process.</td></tr>
                </tbody>
              </table>
            </div>
          </section>
        </div>
      </details>
    </div>
  </div>
</template>
