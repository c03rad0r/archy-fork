import type { Podcast } from '@aiui/core/types/content'

export const mockPodcasts: Podcast[] = [
  {
    id: 'p1',
    title: 'What Bitcoin Did',
    host: 'Peter McCormack',
    description: 'Interview-based podcast exploring Bitcoin, freedom, and the future of money.',
    coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts221/v4/fb/ff/bb/fbffbba3-1a5b-0759-fa35-cafc42f214c9/mza_4444412895211887709.jpg/600x600bb.jpg',
    year: 2018,
    episodeCount: 400,
    genres: ['Bitcoin', 'Finance', 'Tech'],
    sources: [
      { type: 'fountain', name: 'Fountain', url: 'https://fountain.fm/show/whatbitcoindid', icon: '⚡' },
      { type: 'podcastindex', name: 'Podcast Index', url: 'https://podcastindex.org/podcast/123456', icon: '📻' },
      { type: 'youtube', name: 'YouTube', url: 'https://youtube.com/@WhatBitcoinDid', icon: '▶️' },
      { type: 'rss', name: 'RSS', url: 'https://www.whatbitcoindid.com/podcast?format=rss', icon: '📡' },
    ],
  },
  {
    id: 'p2',
    title: 'The Audacity to Podcast',
    host: 'Daniel J. Lewis',
    description: 'Podcasting tips, strategies, and news. Podcasting 2.0 focused.',
    coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts123/v4/7a/b6/84/7ab6843a-3f8a-f3b6-772e-74904aeb1fa1/mza_5013277301801181914.png/600x600bb.jpg',
    year: 2010,
    episodeCount: 500,
    genres: ['Podcasting', 'Tech', 'How-To'],
    sources: [
      { type: 'fountain', name: 'Fountain', url: 'https://fountain.fm/show/1I7TKhOPXJz9MDhG5gqh', icon: '⚡' },
      { type: 'youtube', name: 'YouTube', url: 'https://youtube.com/@audacitytopodcast', icon: '▶️' },
      { type: 'castopod', name: 'Castopod', url: 'https://castopod.example/audacity', icon: '🦣' },
    ],
  },
  {
    id: 'p3',
    title: 'Tales from the Crypt',
    host: 'Marty Bent',
    description: 'Bitcoin, Austrian economics, and the importance of sound money.',
    coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts221/v4/84/cd/73/84cd73e4-eddb-d7f6-a777-875efb65b924/mza_15569173368189135395.jpg/600x600bb.jpg',
    year: 2017,
    episodeCount: 300,
    genres: ['Bitcoin', 'Economics', 'Finance'],
    sources: [
      { type: 'fountain', name: 'Fountain', url: 'https://fountain.fm/show/talesfromthecrypt', icon: '⚡' },
      { type: 'odysee', name: 'Odysee', url: 'https://odysee.com/@tftc', icon: '🔗' },
      { type: 'rumble', name: 'Rumble', url: 'https://rumble.com/c/tftc', icon: '📺' },
      { type: 'youtube', name: 'YouTube', url: 'https://youtube.com/@TalesFromTheCrypt', icon: '▶️' },
    ],
  },
  {
    id: 'p4',
    title: 'Stephan Livera Podcast',
    host: 'Stephan Livera',
    description: 'Bitcoin, Lightning Network, and sovereign individual topics.',
    coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts113/v4/ed/fb/80/edfb8088-08fb-adfc-3a0e-89a216d0cb0a/mza_546663195369372323.jpg/600x600bb.jpg',
    year: 2018,
    episodeCount: 450,
    genres: ['Bitcoin', 'Lightning', 'Tech'],
    sources: [
      { type: 'fountain', name: 'Fountain', url: 'https://fountain.fm/show/stephanlivera', icon: '⚡' },
      { type: 'podcastindex', name: 'Podcast Index', url: 'https://podcastindex.org/podcast/789012', icon: '📻' },
      { type: 'podverse', name: 'Podverse', url: 'https://podverse.fm/podcast/stephan-livera', icon: '🎧' },
      { type: 'rss', name: 'RSS', url: 'https://stephanlivera.com/feed/', icon: '📡' },
    ],
  },
  {
    id: 'p5',
    title: 'Hell Money',
    host: 'Brittany Kaiser & Patrick Wood',
    description: 'Investigative podcast on financial crime, surveillance, and control systems.',
    coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts112/v4/dd/00/4a/dd004ad9-a2b5-d905-59f9-761309118e47/mza_8872129051801444337.jpg/600x600bb.jpg',
    year: 2022,
    episodeCount: 80,
    genres: ['True Crime', 'Finance', 'Investigative'],
    sources: [
      { type: 'rumble', name: 'Rumble', url: 'https://rumble.com/c/hellmoney', icon: '📺' },
      { type: 'youtube', name: 'YouTube', url: 'https://youtube.com/@HellMoney', icon: '▶️' },
      { type: 'odysee', name: 'Odysee', url: 'https://odysee.com/@HellMoney', icon: '🔗' },
    ],
  },
  {
    id: 'p6',
    title: 'Cypherpunk Bitstream',
    host: 'Brandon Zemp',
    description: 'Decentralization, privacy tech, and cypherpunk philosophy.',
    coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts125/v4/82/70/44/827044a2-cabb-f681-00e7-64d2e02fff13/mza_18243182780759307932.jpg/600x600bb.jpg',
    year: 2020,
    episodeCount: 120,
    genres: ['Privacy', 'Bitcoin', 'Decentralization'],
    sources: [
      { type: 'fountain', name: 'Fountain', url: 'https://fountain.fm/show/cypherpunkbitstream', icon: '⚡' },
      { type: 'castopod', name: 'Castopod', url: 'https://castopod.example/cypherpunk', icon: '🦣' },
      { type: 'ipfs', name: 'IPFS', url: 'ipfs://QmCypherpunkBitstream...', icon: '🌐' },
      { type: 'rss', name: 'RSS', url: 'https://cypherpunkbitstream.com/feed', icon: '📡' },
    ],
  },
  { id: 'p7', title: 'Bitcoin Audible', host: 'Guy Swann', description: 'Reading the best in Bitcoin content, one article at a time.', coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts126/v4/a8/1c/b2/a81cb23a-7aa5-c123-60a1-9e5d0335496b/mza_12623622548899082019.jpg/600x600bb.jpg', year: 2017, episodeCount: 600, genres: ['Bitcoin', 'Education'], sources: [{ type: 'fountain', name: 'Fountain', url: 'https://fountain.fm/show/bitcoinaudible', icon: '⚡' }] },
  { id: 'p8', title: 'The Bitcoin Standard Podcast', host: 'Saifedean Ammous', description: 'Economics, sound money, and the case for Bitcoin.', coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts221/v4/fc/a5/38/fca5380d-362d-d44c-04f5-df14044e88ac/mza_4951446082004808256.jpg/600x600bb.jpg', year: 2019, episodeCount: 200, genres: ['Bitcoin', 'Economics'], sources: [{ type: 'fountain', name: 'Fountain', url: 'https://fountain.fm/show/tbs', icon: '⚡' }] },
  { id: 'p9', title: 'Bitcoin Explained', host: 'Aaron van Wirdum & Sjors Provoost', description: 'Technical explanations of Bitcoin protocol developments.', coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts116/v4/1a/7b/d2/1a7bd297-e8c8-bf51-860e-fd36a788242a/mza_14220953861479257188.jpg/600x600bb.jpg', year: 2020, episodeCount: 150, genres: ['Bitcoin', 'Technology'], sources: [{ type: 'rss', name: 'RSS', url: 'https://bitcoinexplained.com/feed', icon: '📡' }] },
  { id: 'p10', title: 'Nostr Talks', host: 'The Nostr Community', description: 'Discussions about the Nostr protocol, clients, and ecosystem.', coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts211/v4/aa/3e/7b/aa3e7b40-7546-aa17-9695-6cdc506cdc10/mza_15889741860915228164.jpeg/600x600bb.jpg', year: 2023, episodeCount: 60, genres: ['Nostr', 'Decentralization', 'Tech'], sources: [{ type: 'fountain', name: 'Fountain', url: 'https://fountain.fm/show/nostrtalks', icon: '⚡' }] },
  { id: 'p11', title: 'Lex Fridman Podcast', host: 'Lex Fridman', description: 'Conversations about the nature of intelligence, consciousness, love, and power.', coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts115/v4/3e/e3/9c/3ee39c89-de08-47a6-7f3d-3849cef6d255/mza_16657851278549137484.png/600x600bb.jpg', year: 2018, episodeCount: 420, genres: ['Science', 'Technology', 'Philosophy'], sources: [{ type: 'youtube', name: 'YouTube', url: 'https://youtube.com/@lexfridman', icon: '▶️' }] },
  { id: 'p12', title: 'Darknet Diaries', host: 'Jack Rhysider', description: 'True stories from the dark side of the Internet.', coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts122/v4/3a/80/a7/3a80a7db-5620-f77b-9935-016e61cc2fbc/mza_9399859904175514567.jpg/600x600bb.jpg', year: 2017, episodeCount: 160, genres: ['Cybersecurity', 'True Crime', 'Technology'], sources: [{ type: 'fountain', name: 'Fountain', url: 'https://fountain.fm/show/darknetdiaries', icon: '⚡' }] },
  { id: 'p13', title: 'The Investors Podcast', host: 'Preston Pysh & Stig Brodersen', description: 'Value investing, Bitcoin, and financial analysis.', coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts211/v4/e5/1d/ec/e51decd3-c299-33d3-15dc-cbf732db40ba/mza_7000864012270734067.jpg/600x600bb.jpg', year: 2014, episodeCount: 700, genres: ['Bitcoin', 'Investing', 'Finance'], sources: [{ type: 'fountain', name: 'Fountain', url: 'https://fountain.fm/show/tip', icon: '⚡' }] },
  { id: 'p14', title: 'Rabbit Hole Recap', host: 'Matt Odell & Marty Bent', description: 'Weekly Bitcoin news and analysis.', coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts211/v4/f2/e2/17/f2e2177f-46df-8eba-a909-485d182cbde2/mza_8161838611335848412.jpg/600x600bb.jpg', year: 2019, episodeCount: 250, genres: ['Bitcoin', 'News'], sources: [{ type: 'fountain', name: 'Fountain', url: 'https://fountain.fm/show/rhr', icon: '⚡' }] },
  { id: 'p15', title: 'Citadel Dispatch', host: 'Matt Odell', description: 'Interactive Bitcoin discussion with audience participation.', coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts221/v4/01/bc/59/01bc59a8-83df-55dc-3f7e-b03ad6180d21/mza_16016917205334564904.jpeg/600x600bb.jpg', year: 2021, episodeCount: 130, genres: ['Bitcoin', 'Privacy', 'Open Source'], sources: [{ type: 'fountain', name: 'Fountain', url: 'https://fountain.fm/show/citadeldispatch', icon: '⚡' }, { type: 'youtube', name: 'YouTube', url: 'https://youtube.com/@citadeldispatch', icon: '▶️' }] },
  { id: 'p16', title: 'Hardcore History', host: 'Dan Carlin', description: 'Deep dives into the hardest core history topics.', coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts115/v4/49/b7/eb/49b7eb32-8f08-6fac-aadb-2f002131fe5f/mza_15196161972010256532.jpg/600x600bb.jpg', year: 2006, episodeCount: 70, genres: ['History', 'Education'], sources: [{ type: 'rss', name: 'RSS', url: 'https://dchhaddendum.libsyn.com/rss', icon: '📡' }] },
  { id: 'p17', title: 'The Changelog', host: 'Adam Stacoviak & Jerod Santo', description: 'Conversations with the hackers, leaders, and innovators of open source.', coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts123/v4/b5/b1/43/b5b14333-7cbe-123d-c444-0204e5d08102/mza_311421542997449775.png/600x600bb.jpg', year: 2009, episodeCount: 600, genres: ['Open Source', 'Technology', 'Programming'], sources: [{ type: 'rss', name: 'RSS', url: 'https://changelog.com/podcast/feed', icon: '📡' }] },
  { id: 'p18', title: 'Once BITten!', host: 'Daniel Prince', description: 'Bitcoin, philosophy, and the orange pill journey.', coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts211/v4/a8/2f/36/a82f3675-c69d-b076-9fbe-eb6e7d69c5a8/mza_2824796342334185417.jpg/600x600bb.jpg', year: 2019, episodeCount: 350, genres: ['Bitcoin', 'Philosophy'], sources: [{ type: 'fountain', name: 'Fountain', url: 'https://fountain.fm/show/oncebitten', icon: '⚡' }] },
  { id: 'p19', title: 'Bitcoin Fundamentals', host: 'Preston Pysh', description: 'Understanding Bitcoin from first principles.', coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts211/v4/e5/1d/ec/e51decd3-c299-33d3-15dc-cbf732db40ba/mza_7000864012270734067.jpg/600x600bb.jpg', year: 2020, episodeCount: 180, genres: ['Bitcoin', 'Education', 'Finance'], sources: [{ type: 'fountain', name: 'Fountain', url: 'https://fountain.fm/show/btcfundamentals', icon: '⚡' }] },
  { id: 'p20', title: 'Opt Out Podcast', host: 'Seth For Privacy', description: 'Privacy tools, techniques, and philosophy for everyone.', coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts221/v4/92/9a/27/929a27d3-bc84-fad6-052e-8e112e3e439d/mza_947152063232700697.jpg/600x600bb.jpg', year: 2021, episodeCount: 90, genres: ['Privacy', 'Technology', 'Security'], sources: [{ type: 'fountain', name: 'Fountain', url: 'https://fountain.fm/show/optout', icon: '⚡' }, { type: 'rss', name: 'RSS', url: 'https://optoutpod.com/feed', icon: '📡' }] },
  { id: 'p21', title: 'Bitcoin Review', host: 'NVK & community', description: 'Technical Bitcoin development review and discussion.', coverUrl: 'https://is1-ssl.mzstatic.com/image/thumb/Podcasts221/v4/9d/d9/60/9dd96097-725b-be30-4541-90f2e85c6f55/mza_11769037187183507672.jpg/600x600bb.jpg', year: 2022, episodeCount: 75, genres: ['Bitcoin', 'Development', 'Open Source'], sources: [{ type: 'fountain', name: 'Fountain', url: 'https://fountain.fm/show/bitcoinreview', icon: '⚡' }] },
]

export function searchPodcasts(query: string): Podcast[] {
  const q = query.toLowerCase()
  return mockPodcasts.filter(
    (p) =>
      p.title.toLowerCase().includes(q) ||
      (p.host ?? '').toLowerCase().includes(q) ||
      (p.genres ?? []).some((g: string) => g.toLowerCase().includes(q))
  )
}
