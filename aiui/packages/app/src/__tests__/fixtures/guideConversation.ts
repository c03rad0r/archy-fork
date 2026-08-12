/**
 * AIUI Guide — pre-loaded as a chat conversation so users can
 * read it right in the chat window.
 */

export function guideToConversation(): {
  id: string
  title: string
  messages: { id: string; role: string; content: string; timestamp: number }[]
  createdAt: number
  updatedAt: number
} {
  const baseTime = 1772496000000

  const guideContent = `# AIUI Guide — Your Node Assistant

AIUI is your AI assistant running directly on your Archipelago node. It can see your installed apps, read your files, check Bitcoin and Lightning status, and help you manage everything — all privately, with no data leaving your node.

---

## Node Awareness

AIUI automatically knows about your node setup. Just ask naturally:

- *"What apps do I have installed?"*
- *"Is my node connected to the network?"*
- *"What version of Archipelago am I running?"*

---

## File Browsing & Reading

AIUI can browse and read text files stored in your Nextcloud. Supported formats: \`.txt\`, \`.md\`, \`.json\`, \`.csv\`, \`.log\`, \`.yaml\`, \`.conf\`, \`.toml\`, \`.xml\`, \`.html\`, \`.css\`, \`.js\`, \`.ts\`, \`.py\`, \`.sh\`, and more.

- *"What files do I have?"*
- *"Read my config.yaml file"*
- *"Show me the contents of notes.md"*
- *"Summarize my todo.txt"*

> Files are read up to 100KB. Larger files are truncated. Binary files (images, videos) cannot be read as text.

---

## Bitcoin Node Status

If you have Bitcoin Core running, AIUI can check sync status, block height, and mempool info in real-time.

- *"How's my Bitcoin node doing?"*
- *"What block height am I on?"*
- *"Is my node fully synced?"*
- *"How many transactions are in the mempool?"*

---

## Lightning Network (LND)

AIUI can query your LND node for channels, peers, balances, and sync status. Private keys and macaroons are never exposed.

- *"What's my Lightning balance?"*
- *"How many channels do I have open?"*
- *"How many peers is my node connected to?"*
- *"Is my Lightning node synced?"*

---

## App Logs

When an app isn't working right, AIUI can pull recent log output to help diagnose issues.

- *"Why is Mempool not working?"*
- *"Show me the Bitcoin Core logs"*
- *"What errors is Nextcloud showing?"*
- *"Show me the last 100 lines of LND logs"*

---

## App Management

AIUI can help you navigate your node, open apps, and install new ones.

- *"Open Mempool"*
- *"Install BTCPay Server"*
- *"Take me to the Settings page"*
- *"What apps are available to install?"*

---

## Chat Features

- **Conversation History** — All chats saved locally. Use the history panel to switch between them.
- **Edit Messages** — Click any sent message to edit and re-send.
- **Branch Conversations** — Fork at any point to explore a different direction.
- **Web Search** — When enabled, AIUI searches the web for current info.
- **Image Support** — Attach images for visual questions.

---

## Privacy & Permissions

AIUI only accesses what you allow. Node data categories (apps, files, wallet, bitcoin, network, system) are permission-gated through the Archy permissions panel. All processing goes through your node's Claude proxy — your conversations and data never touch third-party servers beyond the AI API. Private keys, seeds, and macaroons are never exposed.

---

## Tips

- Be specific — *"Read my bitcoin.conf"* works better than *"show me config files"*
- AIUI remembers context within a conversation, so ask follow-ups
- If something seems wrong with an app, ask AIUI to check the logs first
- You can ask AIUI to explain what a config file does after reading it
- Use the history panel to return to previous conversations at any time

---

Try asking me something! For example: *"What apps do I have installed?"* or *"How's my Bitcoin node?"*`

  return {
    id: 'aiui-guide',
    title: 'AIUI Guide',
    messages: [
      {
        id: 'guide-q',
        role: 'user',
        content: 'How do I use AIUI?',
        timestamp: baseTime,
      },
      {
        id: 'guide-a',
        role: 'assistant',
        content: guideContent,
        timestamp: baseTime + 2000,
      },
    ],
    createdAt: baseTime,
    updatedAt: baseTime + 2000,
  }
}
