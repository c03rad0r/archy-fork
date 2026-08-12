export interface MockNewsArticle {
  id: string
  title: string
  source: string
  url: string
  publishedAt: string
  summary: string
  category: string
  imageUrl?: string
}

export const mockNews: MockNewsArticle[] = [
  { id: 'n1', title: 'Bitcoin Surpasses $150,000 Mark as Institutional Adoption Accelerates', source: 'Bitcoin Magazine', url: 'https://bitcoinmagazine.com/markets/btc-150k', publishedAt: '2026-03-01', summary: 'Bitcoin reached a new all-time high as major pension funds added BTC to their portfolios.', category: 'Markets', imageUrl: 'https://images.unsplash.com/photo-1621761191319-c6fb62004040?w=600&h=400&fit=crop' },
  { id: 'n2', title: 'Lightning Network Capacity Reaches 100,000 BTC', source: 'The Block', url: 'https://theblock.co/lightning-100k', publishedAt: '2026-02-28', summary: 'The Lightning Network continues its exponential growth with total capacity surpassing 100K BTC.', category: 'Technology', imageUrl: 'https://images.unsplash.com/photo-1558494949-ef010cbdcc31?w=600&h=400&fit=crop' },
  { id: 'n3', title: 'Nostr Protocol Reaches 50 Million Users Worldwide', source: 'Nostr Report', url: 'https://nostr.report/50m-users', publishedAt: '2026-02-27', summary: 'The censorship-resistant social protocol has seen massive growth in decentralized communication.', category: 'Technology', imageUrl: 'https://images.unsplash.com/photo-1522071820081-009f0129c71c?w=600&h=400&fit=crop' },
  { id: 'n4', title: 'EU Digital Identity Regulation Raises Privacy Concerns', source: 'EFF', url: 'https://eff.org/eu-digital-id', publishedAt: '2026-02-26', summary: 'Digital rights groups warn about surveillance implications of new EU digital identity requirements.', category: 'Privacy', imageUrl: 'https://images.unsplash.com/photo-1557597774-9d273605dfa9?w=600&h=400&fit=crop' },
  { id: 'n5', title: 'Open Source AI Models Now Match Proprietary Performance', source: 'Ars Technica', url: 'https://arstechnica.com/open-ai-models', publishedAt: '2026-02-25', summary: 'New open-source language models demonstrate capabilities on par with commercial alternatives.', category: 'AI', imageUrl: 'https://images.unsplash.com/photo-1677442136019-21780ecad995?w=600&h=400&fit=crop' },
  { id: 'n6', title: 'Fedimint Launches Production-Ready Version 1.0', source: 'Bitcoin Magazine', url: 'https://bitcoinmagazine.com/fedimint-1', publishedAt: '2026-02-24', summary: 'Community custody protocol Fedimint releases its first production-ready version.', category: 'Bitcoin', imageUrl: 'https://images.unsplash.com/photo-1639762681057-408e52192e55?w=600&h=400&fit=crop' },
  { id: 'n7', title: 'WebAssembly 3.0 Specification Published', source: 'Mozilla Blog', url: 'https://blog.mozilla.org/wasm-3', publishedAt: '2026-02-23', summary: 'The new WASM spec brings garbage collection and improved interop with JavaScript.', category: 'Technology', imageUrl: 'https://images.unsplash.com/photo-1555066931-4365d14bab8c?w=600&h=400&fit=crop' },
  { id: 'n8', title: 'Signal Protocol Integrated Into Major Email Providers', source: 'Wired', url: 'https://wired.com/signal-email', publishedAt: '2026-02-22', summary: 'End-to-end encryption becomes the default for several major email services.', category: 'Privacy', imageUrl: 'https://images.unsplash.com/photo-1563013544-824ae1b704d3?w=600&h=400&fit=crop' },
  { id: 'n9', title: 'Bitcoin Mining Now 80% Renewable Energy', source: 'CoinDesk', url: 'https://coindesk.com/mining-renewables', publishedAt: '2026-02-21', summary: 'Industry report shows Bitcoin mining has become predominantly powered by renewable energy sources.', category: 'Mining', imageUrl: 'https://images.unsplash.com/photo-1509391366360-2e959784a276?w=600&h=400&fit=crop' },
  { id: 'n10', title: 'Cashu eCash Protocol Sees Rapid Adoption', source: 'Bitcoin Magazine', url: 'https://bitcoinmagazine.com/cashu-adoption', publishedAt: '2026-02-20', summary: 'The Chaumian eCash protocol built on Bitcoin sees growing usage for privacy-preserving payments.', category: 'Bitcoin', imageUrl: 'https://images.unsplash.com/photo-1620321023374-d1a68fbc720d?w=600&h=400&fit=crop' },
  { id: 'n11', title: 'Framework Laptop Launches ARM-Based Model', source: 'The Verge', url: 'https://theverge.com/framework-arm', publishedAt: '2026-02-19', summary: 'The repairable laptop company expands its lineup with an ARM processor option.', category: 'Hardware', imageUrl: 'https://images.unsplash.com/photo-1496181133206-80ce9b88a853?w=600&h=400&fit=crop' },
  { id: 'n12', title: 'Rust Becomes Top 5 Programming Language', source: 'TIOBE', url: 'https://tiobe.com/rust-top-5', publishedAt: '2026-02-18', summary: 'Memory-safe language Rust enters the top 5 in the TIOBE programming language index.', category: 'Technology', imageUrl: 'https://images.unsplash.com/photo-1461749280684-dccba630e2f6?w=600&h=400&fit=crop' },
  { id: 'n13', title: 'El Salvador Bitcoin Strategy Yields 300% Returns', source: 'Reuters', url: 'https://reuters.com/el-salvador-btc', publishedAt: '2026-02-17', summary: 'El Salvador\'s Bitcoin holdings have tripled in value since the country made it legal tender.', category: 'Markets', imageUrl: 'https://images.unsplash.com/photo-1526304640581-d334cdbbf45e?w=600&h=400&fit=crop' },
  { id: 'n14', title: 'IPFS Pinning Becomes Free for Open Source Projects', source: 'Protocol Labs Blog', url: 'https://blog.protocol.ai/free-ipfs', publishedAt: '2026-02-16', summary: 'A new initiative provides free IPFS pinning for open source and public good projects.', category: 'Technology', imageUrl: 'https://images.unsplash.com/photo-1544197150-b99a580bb7a8?w=600&h=400&fit=crop' },
  { id: 'n15', title: 'Privacy-Preserving AI Training Methods Breakthrough', source: 'Nature', url: 'https://nature.com/private-ai', publishedAt: '2026-02-15', summary: 'Researchers demonstrate AI training on encrypted data without performance loss.', category: 'AI', imageUrl: 'https://images.unsplash.com/photo-1507146153580-69a1fe6d8aa1?w=600&h=400&fit=crop' },
  { id: 'n16', title: 'Mesh Networking Protocol Goes Mainstream', source: 'TechCrunch', url: 'https://techcrunch.com/mesh-mainstream', publishedAt: '2026-02-14', summary: 'New mesh networking standard enables peer-to-peer connectivity without traditional ISPs.', category: 'Technology', imageUrl: 'https://images.unsplash.com/photo-1451187580459-43490279c0fa?w=600&h=400&fit=crop' },
  { id: 'n17', title: 'Bitcoin Multisig Wallets See Enterprise Adoption', source: 'Bitcoin Magazine', url: 'https://bitcoinmagazine.com/multisig-enterprise', publishedAt: '2026-02-13', summary: 'Major corporations adopt multisignature Bitcoin wallets for treasury management.', category: 'Bitcoin', imageUrl: 'https://images.unsplash.com/photo-1551288049-bebda4e38f71?w=600&h=400&fit=crop' },
  { id: 'n18', title: 'Right to Repair Legislation Passes in 30 US States', source: 'iFixit', url: 'https://ifixit.com/right-to-repair-30', publishedAt: '2026-02-12', summary: 'The right to repair movement achieves a major milestone with widespread legislative support.', category: 'Policy', imageUrl: 'https://images.unsplash.com/photo-1581092160562-40aa08e78837?w=600&h=400&fit=crop' },
  { id: 'n19', title: 'Sovereign Computing Movement Gains Momentum', source: 'Hacker News', url: 'https://news.ycombinator.com/sovereign', publishedAt: '2026-02-11', summary: 'Growing number of individuals running personal servers and self-hosting critical services.', category: 'Technology', imageUrl: 'https://images.unsplash.com/photo-1558494949-ef010cbdcc31?w=600&h=400&fit=crop' },
  { id: 'n20', title: 'Zero-Knowledge Proofs Enable Private Bitcoin Transactions', source: 'CoinDesk', url: 'https://coindesk.com/zkp-bitcoin', publishedAt: '2026-02-10', summary: 'New zero-knowledge proof implementations bring enhanced privacy to Bitcoin transactions.', category: 'Bitcoin', imageUrl: 'https://images.unsplash.com/photo-1526374965328-7f61d4dc18c5?w=600&h=400&fit=crop' },
  { id: 'n21', title: 'Decentralized VPN Networks Reach 10 Million Nodes', source: 'TorrentFreak', url: 'https://torrentfreak.com/dvpn-10m', publishedAt: '2026-02-09', summary: 'Decentralized VPN services powered by community nodes reach significant scale.', category: 'Privacy', imageUrl: 'https://images.unsplash.com/photo-1510511459019-5dda7724fd87?w=600&h=400&fit=crop' },
]

export function searchNews(query: string): MockNewsArticle[] {
  const q = query.toLowerCase()
  return mockNews.filter(
    (n) =>
      n.title.toLowerCase().includes(q) ||
      n.source.toLowerCase().includes(q) ||
      n.category.toLowerCase().includes(q) ||
      n.summary.toLowerCase().includes(q)
  )
}
