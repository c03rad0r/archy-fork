export interface MockNostrEvent {
  id: string
  pubkey: string
  kind: number
  content: string
  created_at: number
  tags: string[][]
  sig: string
  authorName?: string
  authorNip05?: string
}

export const mockNostrEvents: MockNostrEvent[] = [
  { id: 'ne1', pubkey: 'npub1abc', kind: 1, content: 'Just set up my first Lightning node. The future of money is here! ⚡', created_at: 1709000000, tags: [['t', 'bitcoin'], ['t', 'lightning']], sig: 'sig1', authorName: 'satoshi_fan', authorNip05: 'satoshi@nostr.com' },
  { id: 'ne2', pubkey: 'npub1def', kind: 1, content: 'Running Nostr relays on a Raspberry Pi. Sovereign infrastructure for the win.', created_at: 1708990000, tags: [['t', 'nostr'], ['t', 'selfhosting']], sig: 'sig2', authorName: 'relay_runner', authorNip05: 'relay@nostr.com' },
  { id: 'ne3', pubkey: 'npub1ghi', kind: 30023, content: '# Why Bitcoin Matters for Freedom\n\nBitcoin is the first technology that enables truly censorship-resistant money...', created_at: 1708980000, tags: [['d', 'bitcoin-freedom'], ['title', 'Why Bitcoin Matters for Freedom']], sig: 'sig3', authorName: 'freedom_writer', authorNip05: 'writer@nostr.com' },
  { id: 'ne4', pubkey: 'npub1jkl', kind: 9735, content: '', created_at: 1708970000, tags: [['bolt11', 'lnbc100n1pjexample'], ['amount', '100000'], ['p', 'npub1abc']], sig: 'sig4', authorName: 'zapper', authorNip05: 'zap@nostr.com' },
  { id: 'ne5', pubkey: 'npub1mno', kind: 1, content: 'Just published my first article on Nostr! No platform can censor my words. #nostr #decentralized', created_at: 1708960000, tags: [['t', 'nostr'], ['t', 'decentralized']], sig: 'sig5', authorName: 'free_speech', authorNip05: 'free@nostr.com' },
  { id: 'ne6', pubkey: 'npub1pqr', kind: 1, content: 'Comparing the Bitcoin and Nostr protocols. Both use keypairs, both are censorship-resistant. The parallels are striking.', created_at: 1708950000, tags: [['t', 'bitcoin'], ['t', 'nostr']], sig: 'sig6', authorName: 'protocol_nerd' },
  { id: 'ne7', pubkey: 'npub1stu', kind: 30023, content: '# Getting Started with NIP-44 Encryption\n\nIn this guide we\'ll explore the new NIP-44 encryption standard...', created_at: 1708940000, tags: [['d', 'nip44-guide'], ['title', 'Getting Started with NIP-44 Encryption']], sig: 'sig7', authorName: 'crypto_teacher' },
  { id: 'ne8', pubkey: 'npub1vwx', kind: 1, content: 'Built a Nostr client that runs entirely in the terminal. Sometimes the simplest tools are the best.', created_at: 1708930000, tags: [['t', 'nostr'], ['t', 'cli']], sig: 'sig8', authorName: 'terminal_hacker' },
  { id: 'ne9', pubkey: 'npub1yz0', kind: 9735, content: '', created_at: 1708920000, tags: [['bolt11', 'lnbc500n1pjexample'], ['amount', '500000'], ['p', 'npub1mno']], sig: 'sig9', authorName: 'big_zapper' },
  { id: 'ne10', pubkey: 'npub1123', kind: 1, content: 'Remember: not your keys, not your coins. Not your keys, not your notes. Sovereignty matters everywhere.', created_at: 1708910000, tags: [['t', 'bitcoin'], ['t', 'nostr'], ['t', 'sovereignty']], sig: 'sig10', authorName: 'key_holder' },
  { id: 'ne11', pubkey: 'npub1456', kind: 1, content: 'Just integrated Cashu payments into my Nostr client. Instant, private payments for everyone!', created_at: 1708900000, tags: [['t', 'cashu'], ['t', 'nostr']], sig: 'sig11', authorName: 'ecash_dev' },
  { id: 'ne12', pubkey: 'npub1789', kind: 30023, content: '# Building Censorship-Resistant Applications\n\nThe architecture of freedom technology requires careful design...', created_at: 1708890000, tags: [['d', 'censorship-resistance'], ['title', 'Building Censorship-Resistant Applications']], sig: 'sig12', authorName: 'arch_freedom' },
  { id: 'ne13', pubkey: 'npub1abc', kind: 1, content: 'Lightning + Nostr + Cashu = the holy trinity of freedom tech. All open source, all interoperable.', created_at: 1708880000, tags: [['t', 'lightning'], ['t', 'nostr'], ['t', 'cashu']], sig: 'sig13', authorName: 'satoshi_fan' },
  { id: 'ne14', pubkey: 'npub1def', kind: 9735, content: '', created_at: 1708870000, tags: [['bolt11', 'lnbc1000n1pjexample'], ['amount', '1000000'], ['p', 'npub1ghi']], sig: 'sig14', authorName: 'relay_runner' },
  { id: 'ne15', pubkey: 'npub1ghi', kind: 1, content: 'New blog post: why I moved all my social media activity to Nostr. No algorithms, no ads, just people.', created_at: 1708860000, tags: [['t', 'nostr'], ['t', 'freedom']], sig: 'sig15', authorName: 'freedom_writer' },
  { id: 'ne16', pubkey: 'npub1jkl', kind: 1, content: 'The beauty of open protocols: anyone can build on them. No API keys, no rate limits, no permission needed.', created_at: 1708850000, tags: [['t', 'openprotocols']], sig: 'sig16', authorName: 'zapper' },
  { id: 'ne17', pubkey: 'npub1mno', kind: 30023, content: '# Nostr Relay Architecture Best Practices\n\nRunning a relay is an act of infrastructure sovereignty...', created_at: 1708840000, tags: [['d', 'relay-architecture'], ['title', 'Nostr Relay Architecture Best Practices']], sig: 'sig17', authorName: 'free_speech' },
  { id: 'ne18', pubkey: 'npub1pqr', kind: 1, content: 'Just discovered you can run a Nostr relay on a $5/month VPS. Decentralization doesn\'t have to be expensive.', created_at: 1708830000, tags: [['t', 'nostr'], ['t', 'selfhosting']], sig: 'sig18', authorName: 'protocol_nerd' },
  { id: 'ne19', pubkey: 'npub1stu', kind: 9735, content: '', created_at: 1708820000, tags: [['bolt11', 'lnbc2100n1pjexample'], ['amount', '2100000'], ['p', 'npub1pqr']], sig: 'sig19', authorName: 'crypto_teacher' },
  { id: 'ne20', pubkey: 'npub1vwx', kind: 1, content: 'Day 365 of posting exclusively on Nostr. Zero regrets. Complete ownership of my digital identity.', created_at: 1708810000, tags: [['t', 'nostr'], ['t', 'sovereignty']], sig: 'sig20', authorName: 'terminal_hacker' },
  { id: 'ne21', pubkey: 'npub1yz0', kind: 1, content: 'The best part about Nostr: your identity is a keypair. No email, no phone number, no government ID needed.', created_at: 1708800000, tags: [['t', 'nostr'], ['t', 'privacy']], sig: 'sig21', authorName: 'big_zapper' },
]

export class MockNostrRelay {
  private events: MockNostrEvent[] = [...mockNostrEvents]

  getEvents(filter?: { kinds?: number[]; limit?: number; authors?: string[] }): MockNostrEvent[] {
    let results = this.events
    if (filter?.kinds?.length) {
      results = results.filter((e) => filter.kinds!.includes(e.kind))
    }
    if (filter?.authors?.length) {
      results = results.filter((e) => filter.authors!.includes(e.pubkey))
    }
    if (filter?.limit) {
      results = results.slice(0, filter.limit)
    }
    return results
  }

  publish(event: MockNostrEvent): boolean {
    this.events.unshift(event)
    return true
  }
}
