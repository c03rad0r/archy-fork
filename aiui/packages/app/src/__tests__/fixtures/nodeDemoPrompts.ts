/**
 * Node Demo Prompts — pre-populated conversations demonstrating
 * AIUI's local node search and management capabilities.
 */

export interface NodeDemoPrompt {
  id: string
  userQuery: string
  assistantResponse: string
}

export const nodeDemoPrompts: NodeDemoPrompt[] = [
  {
    id: 'node-apps',
    userQuery: 'What apps do I have installed on my node?',
    assistantResponse: `You have the following apps installed on your Archipelago node:

- **Bitcoin Core** (running) — Your Bitcoin full node, fully synced to the blockchain
- **LND** (running) — Lightning Network daemon for fast, cheap payments
- **Mempool** (running) — Visual Bitcoin blockchain explorer and mempool monitor
- **Nextcloud** (running) — Your private cloud storage for files, photos, and documents
- **BTCPay Server** (running) — Self-hosted payment processor for accepting Bitcoin
- **Tor** (running) — Anonymous network routing for privacy

All 6 apps are running normally. Would you like me to check the logs for any of them, or open one up?`,
  },
  {
    id: 'node-bitcoin',
    userQuery: "How's my Bitcoin node doing?",
    assistantResponse: `Your Bitcoin Core node is looking healthy:

**Bitcoin Node Status**
- **Block Height:** 886,421
- **Sync Progress:** 100.00% (fully synced)
- **Chain:** mainnet
- **Mempool:** 42,318 transactions (68.2 MB)

Your node is fully caught up with the Bitcoin network. The mempool is moderately full — transaction fees are around 12-25 sat/vB for next-block confirmation. This is a typical level of activity.

Would you like me to check your Lightning node status as well?`,
  },
  {
    id: 'node-lightning',
    userQuery: "Yes, what's my Lightning balance?",
    assistantResponse: `Here's your Lightning Network status:

**Lightning (LND)**
- **Node Alias:** ArchyNode-228
- **Channels:** 5 active channels
- **Peers:** 8 connected peers
- **Channel Balance:** 3,150,000 sats (in channels)
- **On-chain Balance:** 1,245,000 sats
- **Pending Open:** 0 sats
- **Sync:** Fully synced to chain

Your total Lightning capacity is about 3.15M sats across 5 channels, plus 1.24M sats on-chain. Everything looks healthy — all channels are active and your node is fully synced.

Your total Bitcoin holdings visible to the node: **4,395,000 sats** (~$4,395 at current rates).`,
  },
  {
    id: 'node-files',
    userQuery: 'What files do I have stored on my node?',
    assistantResponse: `You have **47 files** in your Nextcloud storage. Here are the most recent:

- bitcoin.conf
- lnd.conf
- notes.md
- budget-2026.csv
- family-photos/ (folder)
- Documents/ (folder)
- backup-keys.txt
- todo.txt
- project-ideas.md
- recipes/ (folder)

Would you like me to read any of these files? I can open text files like .txt, .md, .json, .csv, .conf, .yaml, and more.`,
  },
  {
    id: 'node-read-file',
    userQuery: 'Read my todo.txt',
    assistantResponse: `Here's the contents of **todo.txt** (342 bytes):

\`\`\`
Shopping List
- Groceries for the week
- New HDMI cable for the node monitor
- Backup hard drive (4TB minimum)

Node Tasks
- Open more Lightning channels
- Set up Ride The Lightning (RTL)
- Configure automatic channel backups
- Test BTCPay Server checkout flow

Personal
- Call dentist for appointment
- Renew domain registration
- Plan weekend hike
\`\`\`

Looks like you've got some node tasks on your list! I can help you with a few of those — I can open the app marketplace to install Ride The Lightning, or check your current channel backup configuration. Want me to help with any of these?`,
  },
  {
    id: 'node-read-config',
    userQuery: 'Show me my bitcoin.conf',
    assistantResponse: `Here's your **bitcoin.conf** (512 bytes):

\`\`\`conf
# Bitcoin Core Configuration
server=1
daemon=1
txindex=1

# RPC Settings
rpcuser=archipelago
rpcpassword=EXAMPLE-ONLY-not-a-real-password
rpcallowip=127.0.0.1
rpcbind=127.0.0.1
rpcport=8332

# Network
listen=1
maxconnections=40
maxuploadtarget=5000

# Mempool
maxmempool=300
mempoolexpiry=72

# Performance
dbcache=450
par=2

# ZMQ (for LND)
zmqpubrawblock=tcp://127.0.0.1:28332
zmqpubrawtx=tcp://127.0.0.1:28333
\`\`\`

Your config looks solid. A few observations:
- **txindex=1** is enabled, which is needed for Electrum Server and block explorers
- **dbcache=450** MB — reasonable for your hardware
- **ZMQ** is configured for LND to receive real-time block/tx notifications
- **maxuploadtarget=5000** MB/day limits upload bandwidth

Everything is properly set up for running LND and Mempool alongside Bitcoin Core.`,
  },
  {
    id: 'node-app-logs',
    userQuery: "Mempool seems slow, can you check what's going on?",
    assistantResponse: `Let me check the Mempool logs for you.

Here are the last 50 lines of **Mempool** logs:

\`\`\`
[2026-03-05 14:23:01] Updating mempool stats...
[2026-03-05 14:23:02] Fetched 42318 transactions from Bitcoin Core
[2026-03-05 14:23:03] Block 886421 indexed successfully
[2026-03-05 14:23:05] WebSocket: 12 clients connected
[2026-03-05 14:23:10] Mining pool stats updated
[2026-03-05 14:23:15] Mempool size: 68.2 MB (42318 txs)
[2026-03-05 14:23:20] Fee estimation updated: 12-25 sat/vB
[2026-03-05 14:23:25] WARN: Electrs sync delay detected (2.3s)
[2026-03-05 14:23:30] Updating difficulty adjustment...
[2026-03-05 14:23:35] Hashrate calculation complete
\`\`\`

I see one warning: **Electrs sync delay of 2.3 seconds**. This is the Electrum Server indexer that Mempool uses for address lookups. A small delay like this is usually temporary and happens when:

1. A new block was just found (extra indexing work)
2. The mempool is particularly full (more data to process)

The rest of the logs look normal. Mempool is serving 12 active WebSocket connections. If the slowness persists, you might want to restart the Mempool container. Want me to do that?`,
  },
  {
    id: 'node-open-app',
    userQuery: 'Open Mempool so I can check it myself',
    assistantResponse: `Opening **Mempool** for you now. The app should appear in a new view — you'll see the real-time mempool visualization, fee estimates, and recent blocks.

If it's still feeling slow, try refreshing the page. The Electrs sync delay I noticed earlier should resolve itself within a few minutes.`,
  },
  {
    id: 'node-install',
    userQuery: 'What other apps can I install?',
    assistantResponse: `Here are some apps available in the Archipelago marketplace that you haven't installed yet:

**Recommended for your setup:**
- **Ride The Lightning (RTL)** — Visual Lightning node manager with channel management, payments, and routing fee settings. Great companion to LND.
- **ThunderHub** — Another Lightning dashboard with a modern UI. More advanced routing and rebalancing tools.
- **BOS (Balance of Satoshis)** — CLI tool for advanced Lightning channel management and rebalancing.

**Privacy & Communication:**
- **Nostr Relay** — Run your own Nostr relay for censorship-resistant social media
- **SimpleX Chat Server** — Private messaging server

**Other:**
- **Home Assistant** — Smart home automation
- **Gitea** — Self-hosted Git repository (like a private GitHub)
- **Vaultwarden** — Self-hosted password manager (Bitwarden compatible)

Since you mentioned wanting to set up RTL in your todo list, would you like me to install **Ride The Lightning** for you?`,
  },
  {
    id: 'node-summary',
    userQuery: 'Give me a full status summary of my node',
    assistantResponse: `Here's your complete **Archipelago Node Status**:

---

**System**
- Archipelago v0.1.0
- Network: Connected
- Uptime: 14 days, 6 hours

**Bitcoin Core** — Running
- Block: 886,421 (100% synced)
- Mempool: 42,318 txs (68.2 MB)
- Connections: 38 peers

**Lightning (LND)** — Running
- Alias: ArchyNode-228
- Channels: 5 active, 0 pending
- Peers: 8 connected
- Channel balance: 3,150,000 sats
- On-chain balance: 1,245,000 sats
- Total: 4,395,000 sats

**Apps** — 6/6 running
- Bitcoin Core, LND, Mempool, Nextcloud, BTCPay Server, Tor

**Storage**
- Nextcloud: 47 files
- Blockchain: ~600 GB

---

Everything is running smoothly. No errors or warnings in the last 24 hours. Your node is fully synced, all channels are active, and all apps are healthy.`,
  },
]

/**
 * Build a single conversation with all node demo prompts as sequential messages.
 */
export function nodeDemoToConversation(): {
  id: string
  title: string
  messages: { id: string; role: string; content: string; timestamp: number }[]
  createdAt: number
  updatedAt: number
} {
  const baseTime = 1772492400000 // Slightly after seed prompts
  const messages: { id: string; role: string; content: string; timestamp: number }[] = []

  for (let i = 0; i < nodeDemoPrompts.length; i++) {
    const prompt = nodeDemoPrompts[i]
    const ts = baseTime + i * 120000 // 2 min between each exchange
    messages.push(
      { id: `${prompt.id}-q`, role: 'user', content: prompt.userQuery, timestamp: ts },
      { id: `${prompt.id}-a`, role: 'assistant', content: prompt.assistantResponse, timestamp: ts + 5000 },
    )
  }

  return {
    id: 'node-demo',
    title: 'Exploring My Node',
    messages,
    createdAt: baseTime,
    updatedAt: baseTime + nodeDemoPrompts.length * 120000,
  }
}
