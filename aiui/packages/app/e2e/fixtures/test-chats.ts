import type { Conversation } from '@aiui/core/types/message'

const now = Date.now()

/** Films: user asks for films, assistant responds with [[film:f1]] etc */
export const filmsConversation: Conversation = {
  id: 'e2e-films',
  title: 'Film recommendations',
  messages: [
    {
      id: 'm1',
      role: 'user',
      content: 'Recommend some sci-fi films',
      timestamp: now - 60000,
    },
    {
      id: 'm2',
      role: 'assistant',
      content: `Here are some great sci-fi films:\n\n- [[film:f1]] - Blade Runner 2049\n- [[film:f2]] - Arrival\n- [[film:f3]] - Dune\n\nAll from Denis Villeneuve.`,
      timestamp: now - 30000,
    },
  ],
  createdAt: now - 120000,
  updatedAt: now,
}

/** Magazine: news-like query + bullet sections (BIP/debate context) */
export const magazineConversation: Conversation = {
  id: 'e2e-magazine',
  title: 'BIP 110 brief',
  messages: [
    {
      id: 'm1',
      role: 'user',
      content: "What's the latest on BIP 110? What are people saying?",
      timestamp: now - 60000,
    },
    {
      id: 'm2',
      role: 'assistant',
      content: `## Summary\n\nBIP 110 is being debated. Macro sentiment is bearish. BTC holding.\n\n- **Pro camp** — Technical improvement, faster.\n- **Anti camp** — Too risky, prefer status quo.\n\n**Henrik Zeberg** (analyst) says this could be bullish long-term.\n\nFor deeper analysis: check **Bitcoin Mailing List** (gnusha.org).`,
      timestamp: now - 30000,
    },
  ],
  createdAt: now - 120000,
  updatedAt: now,
}

/** Websites: user asks for resources, assistant gives markdown links */
export const websitesConversation: Conversation = {
  id: 'e2e-websites',
  title: 'Bitcoin resources',
  messages: [
    {
      id: 'm1',
      role: 'user',
      content: 'Best websites to check for Bitcoin news?',
      timestamp: now - 60000,
    },
    {
      id: 'm2',
      role: 'assistant',
      content: `Here are the best places to check:\n\n- [Bitcoin Magazine](https://bitcoinmagazine.com)\n- [Bitcoin.org](https://bitcoin.org)\n- [Mempool.space](https://mempool.space)`,
      timestamp: now - 30000,
    },
  ],
  createdAt: now - 120000,
  updatedAt: now,
}

/** News: web search results + news-like response */
export const newsConversation: Conversation = {
  id: 'e2e-news',
  title: 'Latest Bitcoin news',
  messages: [
    {
      id: 'm1',
      role: 'user',
      content: "What's the latest Bitcoin news?",
      timestamp: now - 60000,
    },
    {
      id: 'm2',
      role: 'assistant',
      content: `Here's what's happening. For the latest news check these sources:\n\n- [Bitcoin hits new high](https://example.com/btc-high)\n- [ETF inflows surge](https://example.com/etf-inflows)`,
      timestamp: now - 30000,
      webResults: [
        { title: 'Bitcoin hits new high', url: 'https://example.com/btc-high', content: 'BTC reached...' },
        { title: 'ETF inflows surge', url: 'https://example.com/etf-inflows', content: 'Spot ETF...' },
      ],
    },
  ],
  createdAt: now - 120000,
  updatedAt: now,
}

/** Songs: user asks for music, assistant responds with [[song:s1]] */
export const songsConversation: Conversation = {
  id: 'e2e-songs',
  title: 'Music recommendations',
  messages: [
    {
      id: 'm1',
      role: 'user',
      content: 'Recommend some math rock',
      timestamp: now - 60000,
    },
    {
      id: 'm2',
      role: 'assistant',
      content: `Here are great math rock tracks:\n\n- [[song:s1]] Never Meant by American Football\n- [[song:s2]] The Kill by Toe`,
      timestamp: now - 30000,
    },
  ],
  createdAt: now - 120000,
  updatedAt: now,
}

/** Podcasts */
export const podcastsConversation: Conversation = {
  id: 'e2e-podcasts',
  title: 'Bitcoin podcasts',
  messages: [
    {
      id: 'm1',
      role: 'user',
      content: 'Best Bitcoin podcasts?',
      timestamp: now - 60000,
    },
    {
      id: 'm2',
      role: 'assistant',
      content: `Check these:\n\n- [[podcast:p1]] What Bitcoin Did\n- [[podcast:p2]] The Audacity to Podcast`,
      timestamp: now - 30000,
    },
  ],
  createdAt: now - 120000,
  updatedAt: now,
}

export const allTestConversations = {
  [filmsConversation.id]: filmsConversation,
  [magazineConversation.id]: magazineConversation,
  [websitesConversation.id]: websitesConversation,
  [newsConversation.id]: newsConversation,
  [songsConversation.id]: songsConversation,
  [podcastsConversation.id]: podcastsConversation,
}
