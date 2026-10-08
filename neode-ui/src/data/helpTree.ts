export interface HelpSection {
  id: string
  label: string
  items: HelpItem[]
}

export interface HelpItem {
  id: string
  label: string
  path?: string
  content?: string
  relatedPath?: string
}

export interface SearchableItem {
  id: string
  label: string
  path?: string
  type: 'navigate' | 'learn' | 'action' | 'goal'
  section: string
  content?: string
  relatedPath?: string
}

export const helpTree: HelpSection[] = [
  {
    id: 'navigate',
    label: 'Navigate',
    items: [
      { id: 'home', label: 'Home', path: '/dashboard' },
      { id: 'apps', label: 'My Apps', path: '/dashboard/apps' },
      { id: 'marketplace', label: 'App Store', path: '/dashboard/marketplace' },
      { id: 'cloud', label: 'Cloud', path: '/dashboard/cloud' },
      { id: 'server', label: 'Network', path: '/dashboard/server' },
      { id: 'web5', label: 'Web5', path: '/dashboard/web5' },
      { id: 'settings', label: 'Settings', path: '/dashboard/settings' },
    ],
  },
  {
    id: 'learn',
    label: 'Learn',
    items: [
      {
        id: 'getting-started',
        label: 'Getting Started',
        content: 'Welcome to Archipelago! To get started: 1) Flash the Archipelago ISO to a USB drive using Balena Etcher or dd. 2) Boot your hardware from the USB. 3) The installer partitions your disk, installs the OS, and reboots automatically. 4) On first boot, open a browser and navigate to your node\'s IP address. 5) Complete the onboarding wizard — set a password, create your first identity (DID), and choose your setup path. Your node is now ready to install apps, connect to Bitcoin, and join the sovereign web.',
        relatedPath: '/dashboard',
      },
      {
        id: 'bitcoin-basics',
        label: 'Bitcoin Basics',
        content: 'Bitcoin is a decentralized digital currency. Your node validates transactions and maintains the blockchain locally. Install Bitcoin Knots from the App Store to run a full node. Initial sync takes 2-3 days depending on hardware and internet speed. Once synced, your node independently verifies every transaction without trusting any third party.',
        relatedPath: '/dashboard/server',
      },
      {
        id: 'lightning-network',
        label: 'Lightning Network',
        content: 'Lightning enables instant, low-fee payments. Install LND from the App Store (requires Bitcoin Knots). After installation, open channels with other nodes to send and receive payments. Use the Lightning Channels view to manage channels, check inbound/outbound liquidity, and monitor your routing fees. Channels require an on-chain transaction to open and close.',
        relatedPath: '/dashboard/apps',
      },
      {
        id: 'app-store-guide',
        label: 'Installing & Managing Apps',
        content: 'Open the App Store (marketplace icon) to browse available apps. Click Install to download and start an app. Some apps have dependencies — Electrs requires Bitcoin, BTCPay requires LND, Mempool requires both Bitcoin and Electrs. The system handles these automatically. After installation, apps appear in My Apps. Click an app to open it in an overlay or new tab. Use the app detail page to start, stop, restart, or uninstall apps.',
        relatedPath: '/dashboard/marketplace',
      },
      {
        id: 'identity-guide',
        label: 'Your Digital Identity (DIDs)',
        content: 'Archipelago creates a sovereign digital identity (DID) during onboarding. DIDs are cryptographic keypairs that prove your identity without any company in the middle. You can create multiple identities for different purposes — Personal, Business, or Anonymous. Each identity can sign messages, issue verifiable credentials, and authenticate with services like Indeehub. Manage identities in the Web5 view.',
        relatedPath: '/dashboard/web5',
      },
      {
        id: 'networking-guide',
        label: 'Connecting with Peers',
        content: 'Archipelago nodes can discover and connect with each other over Tor. In Web5, set your node visibility to "Discoverable" to let other nodes find you via Nostr. Accept connection requests from peers you trust. Once connected, you can message peers, share content, and exchange ecash payments — all over encrypted Tor connections. Your .onion address is shown in Settings.',
        relatedPath: '/dashboard/web5',
      },
      {
        id: 'content-sharing',
        label: 'Sharing Content',
        content: 'Share files and media with connected peers through the Content section in Web5. Add content from your Cloud storage, set it as free or paid (ecash-gated), and connected peers can browse and access your catalog. For paid content, peers pay with ecash micropayments — the sats appear in your wallet instantly.',
        relatedPath: '/dashboard/web5',
      },
      {
        id: 'external-access-guide',
        label: 'Allowing External Connections',
        content: 'Open Setup → Allow external connections. Choose how visitors will connect: FIPS, an HTTPS domain through your own gateway, or Tor. You can select more than one. Save your choices, then choose a supported app and create an expiring guest token. Share that token privately. Guests cannot use it to sign in to your dashboard. Check the app address from the visitor’s device; saved settings alone do not confirm a working connection.',
        relatedPath: '/dashboard/setup/external-access',
      },
      {
        id: 'website-guide',
        label: 'Publishing a Website',
        content: 'Open Setup → Publish a website. The guide reuses your saved connections and skips Blossom installation when it is already installed. Create a page with AIUI, use Continue to website setup on its HTML preview, then review and save the draft. You can keep a signed copy in local Blossom. Choose an address and explicitly publish each connection. For Nostr, review the exact page, profile identity, storage server and relays before sharing: public copies may remain even after a removal request. FIPS and Tor do not require a purchased domain.',
        relatedPath: '/dashboard/setup/website',
      },
      {
        id: 'self-hosting',
        label: 'Self-Hosting',
        content: 'Archipelago runs your services locally. Your data stays on your hardware, giving you full control and privacy. No cloud subscriptions, no data harvesting, no service shutdowns. You own your node, your data, and your identity. Back up your node regularly using the backup feature in Settings.',
        relatedPath: '/dashboard',
      },
      {
        id: 'troubleshooting',
        label: 'Troubleshooting FAQ',
        content: 'Common issues: 1) App won\'t start — check disk space in Settings > Server. 2) Bitcoin not syncing — ensure port 8333 is reachable; check network diagnostics. 3) Can\'t connect to peers — verify Tor is running (Settings > Network). 4) UI is slow — some views load data from multiple sources; check server resources. 5) Lost password — use the backup recovery key created during onboarding. 6) Container errors — try stopping and restarting the app, or uninstall and reinstall.',
        relatedPath: '/dashboard/settings',
      },
    ],
  },
  {
    id: 'actions',
    label: 'Actions',
    items: [
      { id: 'open-cli', label: 'Open CLI', path: '__cli__' },
      { id: 'install-app', label: 'Install an App', path: '/dashboard/marketplace' },
      { id: 'manage-apps', label: 'Manage My Apps', path: '/dashboard/apps' },
      { id: 'network-settings', label: 'Network Settings', path: '/dashboard/server' },
      { id: 'backup', label: 'Backup & Recovery', path: '/dashboard/settings' },
    ],
  },
  {
    id: 'goals',
    label: 'Quick Start Goals',
    items: [
      { id: 'goal-shop', label: 'Open a Shop', path: '/dashboard/goals/open-a-shop' },
      { id: 'goal-payments', label: 'Accept Payments', path: '/dashboard/goals/accept-payments' },
      { id: 'goal-photos', label: 'Store My Photos', path: '/dashboard/goals/store-photos' },
      { id: 'goal-files', label: 'Store My Files', path: '/dashboard/goals/store-files' },
      { id: 'goal-lightning', label: 'Run a Lightning Node', path: '/dashboard/goals/run-lightning-node' },
      { id: 'goal-identity', label: 'Create My Identity', path: '/dashboard/goals/create-identity' },
      { id: 'goal-backup', label: 'Back Up Everything', path: '/dashboard/goals/back-up-everything' },
    ],
  },
]

export function flattenForSearch(): SearchableItem[] {
  const result: SearchableItem[] = []
  for (const section of helpTree) {
    const type =
      section.id === 'navigate'
        ? 'navigate'
        : section.id === 'learn'
          ? 'learn'
          : section.id === 'goals'
            ? 'goal'
            : 'action'
    for (const item of section.items) {
      result.push({
        id: item.id,
        label: item.label,
        path: item.path,
        type,
        section: section.label,
        content: item.content,
        relatedPath: item.relatedPath,
      })
    }
  }
  return result
}
