<template>
  <div class="guide-page">
    <header class="guide-header">
      <button class="back-btn" @click="goBack">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M19 12H5M12 19l-7-7 7-7" />
        </svg>
      </button>
      <h1 class="guide-title">AIUI Guide</h1>
      <span class="guide-version">v1.0</span>
    </header>

    <main class="guide-content">
      <!-- Intro -->
      <section class="guide-section">
        <p class="guide-intro">
          AIUI is your AI assistant running directly on your Archipelago node. It can see your installed apps,
          read your files, check Bitcoin and Lightning status, and help you manage everything — all privately,
          with no data leaving your node.
        </p>
      </section>

      <!-- Node Context -->
      <section class="guide-section">
        <h2 class="section-title">
          <span class="section-icon">&#x1F4E1;</span>
          Node Awareness
        </h2>
        <p class="section-desc">
          When running on your Archipelago node, AIUI automatically knows about your setup. It sees which apps
          are installed, your system status, network connectivity, and more. Just ask naturally:
        </p>
        <div class="example-box">
          <div class="example-prompt">"What apps do I have installed?"</div>
          <div class="example-prompt">"Is my node connected to the network?"</div>
          <div class="example-prompt">"What version of Archipelago am I running?"</div>
        </div>
      </section>

      <!-- File Access -->
      <section class="guide-section">
        <h2 class="section-title">
          <span class="section-icon">&#x1F4C1;</span>
          File Browsing & Reading
        </h2>
        <p class="section-desc">
          AIUI can browse and read text files stored in your Nextcloud instance. It supports common text formats
          like <code>.txt</code>, <code>.md</code>, <code>.json</code>, <code>.csv</code>, <code>.log</code>,
          <code>.yaml</code>, <code>.conf</code>, and many more.
        </p>
        <div class="example-box">
          <div class="example-prompt">"What files do I have?"</div>
          <div class="example-prompt">"Read my config.yaml file"</div>
          <div class="example-prompt">"Show me the contents of notes.md"</div>
          <div class="example-prompt">"Summarize my todo.txt"</div>
        </div>
        <div class="info-note">
          Files are read up to 100KB. Larger files are truncated with a note. Binary files (images, videos, etc.)
          cannot be read as text.
        </div>
      </section>

      <!-- Bitcoin -->
      <section class="guide-section">
        <h2 class="section-title">
          <span class="section-icon">&#x20BF;</span>
          Bitcoin Node Status
        </h2>
        <p class="section-desc">
          If you have Bitcoin Core running on your node, AIUI can check the blockchain sync status,
          current block height, and mempool information in real-time.
        </p>
        <div class="example-box">
          <div class="example-prompt">"How's my Bitcoin node doing?"</div>
          <div class="example-prompt">"What block height am I on?"</div>
          <div class="example-prompt">"Is my node fully synced?"</div>
          <div class="example-prompt">"How many transactions are in the mempool?"</div>
        </div>
      </section>

      <!-- Lightning -->
      <section class="guide-section">
        <h2 class="section-title">
          <span class="section-icon">&#x26A1;</span>
          Lightning Network (LND)
        </h2>
        <p class="section-desc">
          AIUI can query your LND node for channel information, peer count, on-chain and channel balances,
          and sync status. Your private keys and macaroons are never exposed.
        </p>
        <div class="example-box">
          <div class="example-prompt">"What's my Lightning balance?"</div>
          <div class="example-prompt">"How many channels do I have open?"</div>
          <div class="example-prompt">"How many peers is my node connected to?"</div>
          <div class="example-prompt">"Is my Lightning node synced?"</div>
        </div>
      </section>

      <!-- App Logs -->
      <section class="guide-section">
        <h2 class="section-title">
          <span class="section-icon">&#x1F4CB;</span>
          App Logs
        </h2>
        <p class="section-desc">
          When an app isn't working right, AIUI can pull recent log output to help diagnose issues.
          It reads the last 50 lines by default (up to 200).
        </p>
        <div class="example-box">
          <div class="example-prompt">"Why is Mempool not working?"</div>
          <div class="example-prompt">"Show me the Bitcoin Core logs"</div>
          <div class="example-prompt">"What errors is Nextcloud showing?"</div>
          <div class="example-prompt">"Show me the last 100 lines of LND logs"</div>
        </div>
      </section>

      <!-- App Management -->
      <section class="guide-section">
        <h2 class="section-title">
          <span class="section-icon">&#x1F4E6;</span>
          App Management
        </h2>
        <p class="section-desc">
          AIUI can help you navigate your node, open installed apps, and even install new ones from the
          marketplace. It checks what's already installed before recommending anything.
        </p>
        <div class="example-box">
          <div class="example-prompt">"Open Mempool"</div>
          <div class="example-prompt">"Install BTCPay Server"</div>
          <div class="example-prompt">"Take me to the Settings page"</div>
          <div class="example-prompt">"What apps are available to install?"</div>
        </div>
      </section>

      <!-- Chat Features -->
      <section class="guide-section">
        <h2 class="section-title">
          <span class="section-icon">&#x1F4AC;</span>
          Chat Features
        </h2>
        <div class="feature-grid">
          <div class="feature-card">
            <h3>Conversation History</h3>
            <p>All chats are saved locally on your node. Switch between conversations using the history panel.</p>
          </div>
          <div class="feature-card">
            <h3>Edit Messages</h3>
            <p>Click any of your sent messages to edit and re-send them. The AI will regenerate its response.</p>
          </div>
          <div class="feature-card">
            <h3>Branch Conversations</h3>
            <p>Fork a conversation at any point to explore a different direction without losing the original thread.</p>
          </div>
          <div class="feature-card">
            <h3>Web Search</h3>
            <p>When enabled, AIUI can search the web to find current information and include sources in its responses.</p>
          </div>
          <div class="feature-card">
            <h3>Image Support</h3>
            <p>Attach images to your messages for visual questions. AIUI can analyze screenshots, photos, and diagrams.</p>
          </div>
          <div class="feature-card">
            <h3>Dark / Light Theme</h3>
            <p>AIUI adapts to your preferred theme. Toggle between dark and light mode in the settings.</p>
          </div>
        </div>
      </section>

      <!-- Permissions -->
      <section class="guide-section">
        <h2 class="section-title">
          <span class="section-icon">&#x1F512;</span>
          Privacy & Permissions
        </h2>
        <p class="section-desc">
          AIUI only accesses what you allow. Your node data categories (apps, files, wallet, bitcoin, network, system)
          are permission-gated. You control exactly what context AIUI can see through the Archy permissions panel.
        </p>
        <div class="info-note">
          All processing happens through your node's Claude proxy. Your conversations and node data never touch
          third-party servers beyond the AI model API itself. Private keys, seeds, and macaroons are never exposed.
        </div>
      </section>

      <!-- Tips -->
      <section class="guide-section">
        <h2 class="section-title">
          <span class="section-icon">&#x1F4A1;</span>
          Tips
        </h2>
        <ul class="tips-list">
          <li>Be specific — "Read my bitcoin.conf" works better than "show me config files"</li>
          <li>AIUI remembers context within a conversation, so you can ask follow-up questions</li>
          <li>If something seems wrong with an app, ask AIUI to check the logs first</li>
          <li>You can ask AIUI to explain what a config file does after reading it</li>
          <li>Use the history panel to return to previous conversations at any time</li>
        </ul>
      </section>

      <!-- Try It — demo builds only (S7): the demo conversation is
           fabricated example node state, demo-site/dev only -->
      <section v-if="DEMO_CONTENT_ENABLED" class="guide-section">
        <h2 class="section-title">
          <span class="section-icon">&#x1F680;</span>
          Try It Out
        </h2>
        <p class="section-desc">
          Load a demo conversation that showcases all the node search capabilities described above.
          You'll see example exchanges for checking Bitcoin status, reading files, viewing logs, and more.
        </p>
        <button class="demo-button" :disabled="demoLoading" @click="loadDemo">
          <span v-if="demoLoading" class="demo-spinner" />
          <span v-else>{{ demoLoaded ? 'View Demo Chat' : 'Load Demo Conversation' }}</span>
        </button>
      </section>
    </main>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { useChatStore } from '@/stores/chat'
import { DEMO_CONTENT_ENABLED } from '@/utils/demoContent'

const router = useRouter()
const chatStore = useChatStore()
const demoLoading = ref(false)
const demoLoaded = ref(false)

function goBack() {
  router.push('/')
}

async function loadDemo() {
  demoLoading.value = true
  try {
    await chatStore.loadNodeDemoChat()
    demoLoaded.value = true
    // Navigate to chat page to see the demo
    router.push('/')
  } finally {
    demoLoading.value = false
  }
}
</script>

<style scoped>
.guide-page {
  min-height: 100vh;
  background: #0a0a0a;
  color: rgba(255, 255, 255, 0.9);
  font-family: Inter, -apple-system, BlinkMacSystemFont, sans-serif;
}

.guide-header {
  position: sticky;
  top: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 16px 20px;
  background: rgba(0, 0, 0, 0.6);
  backdrop-filter: blur(24px);
  -webkit-backdrop-filter: blur(24px);
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
}

.back-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: 10px;
  border: 1px solid rgba(255, 255, 255, 0.12);
  background: rgba(255, 255, 255, 0.06);
  color: rgba(255, 255, 255, 0.7);
  cursor: pointer;
  transition: all 0.2s ease;
}

.back-btn:hover {
  background: rgba(255, 255, 255, 0.1);
  color: white;
}

.guide-title {
  font-size: 18px;
  font-weight: 600;
  flex: 1;
}

.guide-version {
  font-size: 12px;
  color: rgba(255, 255, 255, 0.4);
  padding: 2px 8px;
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.06);
}

.guide-content {
  max-width: 680px;
  margin: 0 auto;
  padding: 24px 20px 80px;
}

.guide-intro {
  font-size: 15px;
  line-height: 1.7;
  color: rgba(255, 255, 255, 0.7);
}

.guide-section {
  margin-bottom: 36px;
}

.section-title {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 17px;
  font-weight: 600;
  margin-bottom: 12px;
  color: rgba(255, 255, 255, 0.95);
}

.section-icon {
  font-size: 20px;
}

.section-desc {
  font-size: 14px;
  line-height: 1.7;
  color: rgba(255, 255, 255, 0.6);
  margin-bottom: 16px;
}

.section-desc code {
  background: rgba(255, 255, 255, 0.08);
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 13px;
  font-family: Menlo, monospace;
  color: rgba(255, 255, 255, 0.8);
}

.example-box {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-bottom: 12px;
}

.example-prompt {
  padding: 10px 14px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 10px;
  font-size: 13px;
  color: rgba(255, 255, 255, 0.75);
  font-style: italic;
}

.info-note {
  padding: 12px 16px;
  background: rgba(247, 147, 26, 0.08);
  border: 1px solid rgba(247, 147, 26, 0.2);
  border-radius: 10px;
  font-size: 13px;
  line-height: 1.6;
  color: rgba(255, 255, 255, 0.6);
}

.feature-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 12px;
}

.feature-card {
  padding: 16px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 12px;
}

.feature-card h3 {
  font-size: 14px;
  font-weight: 600;
  margin-bottom: 6px;
  color: rgba(255, 255, 255, 0.9);
}

.feature-card p {
  font-size: 13px;
  line-height: 1.5;
  color: rgba(255, 255, 255, 0.5);
}

.tips-list {
  list-style: none;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.tips-list li {
  padding: 10px 14px;
  padding-left: 28px;
  position: relative;
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 10px;
  font-size: 13px;
  line-height: 1.5;
  color: rgba(255, 255, 255, 0.6);
}

.tips-list li::before {
  content: '\2022';
  position: absolute;
  left: 14px;
  color: #F7931A;
}
.demo-button {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  width: 100%;
  padding: 14px 20px;
  border-radius: 12px;
  border: 1px solid rgba(247, 147, 26, 0.3);
  background: rgba(247, 147, 26, 0.1);
  color: #F7931A;
  font-size: 15px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s ease;
}

.demo-button:hover:not(:disabled) {
  background: rgba(247, 147, 26, 0.18);
  border-color: rgba(247, 147, 26, 0.5);
  transform: translateY(-1px);
}

.demo-button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.demo-spinner {
  width: 18px;
  height: 18px;
  border: 2px solid rgba(247, 147, 26, 0.3);
  border-top-color: #F7931A;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
