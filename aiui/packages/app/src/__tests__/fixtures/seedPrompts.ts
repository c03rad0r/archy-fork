/**
 * Seed Prompt Index — realistic AI prompt/response pairs covering every content type.
 * Used by: .dev/chats.json (seeded conversations), extraction tests, e2e tests.
 *
 * Each entry represents a user query + AI response that exercises a specific
 * content surface (films, songs, books, TV, places, podcasts, images, code,
 * recipes, events, news/magazine, mixed content).
 */

export interface SeedPrompt {
  id: string
  /** Content types this prompt exercises */
  types: string[]
  userQuery: string
  assistantResponse: string
  /** Expected extraction counts for validation */
  expected: {
    films?: number
    songs?: number
    books?: number
    tvSeries?: number
    places?: number
    podcasts?: number
    images?: number
    codeBlocks?: number
    recipes?: number
    events?: number
    magazineSections?: number
    apps?: number
  }
}

export const seedPrompts: SeedPrompt[] = [
  // ─── Films ──────────────────────────────────────────────────
  {
    id: 'seed-films',
    types: ['films'],
    userQuery: 'What are the best Christopher Nolan films?',
    assistantResponse: `Here are Christopher Nolan's finest works:

[[film_ext:Inception|2010|Christopher Nolan]] — A mind-bending heist through layers of dreams. DiCaprio leads an ensemble cast through architecture-defying action sequences and emotionally resonant storytelling.

[[film_ext:The Dark Knight|2008|Christopher Nolan]] — Heath Ledger's Joker elevates this beyond superhero fare into genuine crime thriller territory. The interrogation scene alone is worth the watch.

[[film_ext:Interstellar|2014|Christopher Nolan]] — A father-daughter love story wrapped in hard science fiction. The docking scene and "years of messages" sequence are devastating.

[[film_ext:Memento|2000|Christopher Nolan]] — The reverse-chronology thriller that put Nolan on the map. Guy Pearce hunts his wife's killer while unable to form new memories.

[[film_ext:Oppenheimer|2023|Christopher Nolan]] — Cillian Murphy embodies the father of the atomic bomb in Nolan's most dialogue-heavy and politically charged film.

All feature practical effects, IMAX cinematography, and Hans Zimmer scores (except Memento and Oppenheimer's Göransson score).`,
    expected: { films: 5 },
  },

  // ─── Songs / Music ──────────────────────────────────────────
  {
    id: 'seed-songs',
    types: ['songs'],
    userQuery: 'Recommend some ambient electronic music for deep work',
    assistantResponse: `Here are excellent ambient electronic tracks for focused work:

[[song_ext:Weightless|Marconi Union|2011]] — Scientifically designed to reduce anxiety. The 8-minute track uses sustained harmonies and a gradually slowing tempo (60 BPM down to 50).

[[song_ext:An Ending (Ascent)|Brian Eno|1983]] — From the Apollo album. Pure synthesizer beauty — ascending chords that feel like floating above Earth. 4 minutes of transcendence.

[[song_ext:Intro|The xx|2009]] — Minimalist guitar loops over electronic beats. Not strictly ambient, but the repetitive structure aids concentration.

[[song_ext:Avril 14th|Aphex Twin|2001]] — A rare gentle piano piece from Richard D. James. Delicate and hypnotic — perfect for reading or writing.

[[song_ext:Music for Airports 1/1|Brian Eno|1978]] — The track that coined "ambient music." Tape loops of piano and voices create an ever-evolving soundscape.

[[song_ext:Treefingers|Radiohead|2000]] — From Kid A. Processed guitar creating ethereal textures. Works beautifully on loop.`,
    // Six, not ten. The assistantResponse above lists exactly six
    // `song_ext` entries and ends coherently on Treefingers — it was never
    // truncated. The extractor was returning the right answer and this
    // number was the wrong one, which is why "extracts 10 songs" had been
    // failing. Counted from the fixture, not from intent.
    expected: { songs: 6 },
  },

  // ─── Books ──────────────────────────────────────────────────
  {
    id: 'seed-books',
    types: ['books'],
    userQuery: 'Best books on Bitcoin and monetary history',
    assistantResponse: `Essential reading on Bitcoin and the history of money:

[[book_ext:The Bitcoin Standard|Saifedean Ammous|2018]] — The foundational text connecting Austrian economics to Bitcoin. Traces money from primitive shells through gold to fiat, making the case for Bitcoin as the hardest money ever created.

[[book_ext:The Fiat Standard|Saifedean Ammous|2021]] — The sequel examining how fiat currency actually works — debt creation, cantillon effects, and why the system incentivizes time preference distortion.

[[book_ext:Mastering Bitcoin|Andreas Antonopoulos|2017]] — The technical reference. Covers cryptography, transactions, mining, and the protocol in depth. Essential for developers.

[[book_ext:The Price of Tomorrow|Jeff Booth|2020]] — Argues that technology is naturally deflationary, but central banks fight this with money printing. Bitcoin aligns with technology's deflationary nature.

[[book_ext:Broken Money|Lyn Alden|2023]] — A comprehensive monetary history from ancient to modern times, examining why monetary systems break and how Bitcoin fits into the broader evolution of money technology.

**Reading order:** Start with *The Bitcoin Standard* for philosophy, then *Broken Money* for history, then *Mastering Bitcoin* for technical depth.`,
    expected: { books: 5 },
  },

  // ─── TV Series ──────────────────────────────────────────────
  {
    id: 'seed-tv',
    types: ['tvSeries'],
    userQuery: 'Best TV shows of the last decade',
    assistantResponse: `Outstanding television from the 2010s and 2020s:

[[tv_ext:Breaking Bad|Vince Gilligan|2008–2013]] — Walter White's transformation from chemistry teacher to drug lord. The gold standard of character-driven TV drama with a flawless final season.

[[tv_ext:Severance|Dan Erickson|2022–present]] — A workplace thriller where employees surgically separate work and personal memories. Ben Stiller directs a Kafkaesque masterpiece.

[[tv_ext:Chernobyl|Craig Mazin|2019]] — Five episodes covering the 1986 nuclear disaster. Haunting, meticulously researched, and terrifyingly relevant to institutional failure.

[[tv_ext:The Bear|Christopher Storer|2022–present]] — A fine-dining chef returns to run his family's Chicago sandwich shop. Captures kitchen intensity with a season 2 that rivals prestige drama.

[[tv_ext:Better Call Saul|Peter Gould|2015–2022]] — The Breaking Bad prequel that arguably surpasses it. Bob Odenkirk delivers a career-defining performance across 6 seasons.

[[tv_ext:True Detective|Nic Pizzolatto|2014–present]] — Season 1 with McConaughey and Harrelson is peak television. Philosophical detective fiction in the Louisiana bayou.

All available on major streaming platforms. Start with whichever genre appeals most — there's no wrong entry point.`,
    expected: { tvSeries: 6 },
  },

  // ─── Places / Restaurants ───────────────────────────────────
  {
    id: 'seed-places',
    types: ['places'],
    userQuery: 'Best restaurants in Austin Texas',
    assistantResponse: `Here are Austin's standout restaurants:

[[place_ext:Franklin Barbecue|BBQ|Austin TX|4.8|2|1900 E 11th St]] — The most famous BBQ in Texas. Expect 3-4 hour lines for brisket that's genuinely worth the wait. Get there by 8am on weekends.

[[place_ext:Uchi|Japanese|Austin TX|4.7|3|801 S Lamar Blvd]] — Tyson Cole's Japanese farmhouse dining. The tasting menu is exceptional — the maguro sashimi with goat cheese and cracked pepper is iconic Austin dining.

[[place_ext:Ramen Tatsu-Ya|Ramen|Austin TX|4.6|2|8557 Research Blvd]] — Austin's best ramen. The Underbelly (pork broth, woodear mushroom, nori) has cult status. Cash only, expect a line.

[[place_ext:Suerte|Mexican|Austin TX|4.5|2|1800 E 6th St]] — Modern Mexican from the Uchi team. Smoked beef suadero tacos and duck carnitas are standouts. Great mezcal program.

[[place_ext:Odd Duck|American|Austin TX|4.4|2|1201 S Lamar Blvd]] — Farm-to-table small plates in a converted trailer park. The menu changes daily based on what's fresh. Excellent craft cocktails.

**Pro tip:** Reservations are essential for Uchi and Suerte. Franklin is walk-in only. Visit Odd Duck for late-night bites after 9pm.`,
    expected: { places: 5 },
  },

  // ─── Podcasts ───────────────────────────────────────────────
  {
    id: 'seed-podcasts',
    types: ['podcasts'],
    userQuery: 'Best Bitcoin and technology podcasts',
    assistantResponse: `Top podcasts for Bitcoin and tech:

[[podcast_ext:What Bitcoin Did|Peter McCormack|2018]] — Twice-weekly interviews covering Bitcoin from beginner to advanced. Peter's honest, accessible style makes complex topics approachable. Essential listening.

[[podcast_ext:Lex Fridman Podcast|Lex Fridman|2018]] — Long-form conversations with scientists, engineers, and thinkers. The Bitcoin episodes with Saylor, Dorsey, and Antonopoulos are must-listens.

[[podcast_ext:Bitcoin Audible|Guy Swann|2016]] — Guy reads and discusses the best Bitcoin articles and papers. Perfect for commutes — turns written content into audio education.

[[podcast_ext:TFTC - Tales from the Crypt|Marty Bent|2018]] — No-nonsense Bitcoin podcast. Marty's "Rabbit Hole Recap" Friday episodes are a great weekly Bitcoin news summary.

[[podcast_ext:Stephan Livera Podcast|Stephan Livera|2018]] — Technical and economic deep dives. Stephan interviews protocol developers, economists, and Lightning Network builders.

Start with **What Bitcoin Did** for breadth, **Stephan Livera** for depth, and **Lex Fridman** for cross-disciplinary thinking.`,
    expected: { podcasts: 5 },
  },

  // ─── Code ───────────────────────────────────────────────────
  {
    id: 'seed-code',
    types: ['code'],
    userQuery: 'Show me how to implement a debounce function in TypeScript',
    assistantResponse: `Here's a proper TypeScript debounce with generic types and cancel support:

\`\`\`typescript
function debounce<T extends (...args: any[]) => any>(
  fn: T,
  delay: number,
): { (...args: Parameters<T>): void; cancel: () => void } {
  let timer: ReturnType<typeof setTimeout> | null = null

  const debounced = (...args: Parameters<T>) => {
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => {
      fn(...args)
      timer = null
    }, delay)
  }

  debounced.cancel = () => {
    if (timer) {
      clearTimeout(timer)
      timer = null
    }
  }

  return debounced
}
\`\`\`

Usage example:

\`\`\`typescript
const debouncedSearch = debounce((query: string) => {
  console.log('Searching for:', query)
  fetch(\`/api/search?q=\${encodeURIComponent(query)}\`)
}, 300)

// In an input handler:
input.addEventListener('input', (e) => {
  debouncedSearch((e.target as HTMLInputElement).value)
})

// Cleanup:
debouncedSearch.cancel()
\`\`\`

Key design decisions:
- **Generic types** preserve the original function's parameter types
- **Cancel method** prevents stale calls on component unmount
- **ReturnType<typeof setTimeout>** works in both Node and browser
- The return type explicitly shows the augmented function signature`,
    expected: { codeBlocks: 2 },
  },

  // ─── Images ─────────────────────────────────────────────────
  {
    id: 'seed-images',
    types: ['images'],
    userQuery: 'Show me some beautiful landscape photography',
    assistantResponse: `Here are stunning landscape photographs:

![Patagonia's Torres del Paine at sunrise](https://images.unsplash.com/photo-1531804055935-76f44d7c3621?w=1200)

The granite towers catching first light create one of Earth's most dramatic mountain scenes.

![Iceland's Kirkjufell mountain with aurora borealis](https://images.unsplash.com/photo-1504829857797-ddff29c27927?w=1200)

The iconic arrowhead-shaped mountain with northern lights dancing above.

![Japanese autumn — Kiyomizu-dera temple in Kyoto](https://images.unsplash.com/photo-1493976040374-85c8e12f0c0e?w=1200)

Crimson maples frame the famous wooden stage during peak koyo season.

These represent three different approaches to landscape photography: dramatic alpine, astrophotography, and cultural landscape. Each uses natural light and careful composition to tell a story about place.`,
    expected: { images: 3 },
  },

  // ─── Mixed: Films + Songs ───────────────────────────────────
  {
    id: 'seed-mixed-films-songs',
    types: ['films', 'songs'],
    userQuery: 'What are the best movie soundtracks?',
    assistantResponse: `Great question — here are films with legendary soundtracks:

[[film_ext:Drive|2011|Nicolas Winding Refn]] — The synthwave soundtrack defined a genre. Cliff Martinez's score plus curated tracks create an unforgettable atmosphere.

Key tracks:
[[song_ext:Nightcall|Kavinsky|2010]] — The opening drive through LA at night. Pure 80s-inspired synth perfection.
[[song_ext:A Real Hero|College & Electric Youth|2010]] — The emotional core of the film. Dreamy, melancholic electronica.

[[film_ext:Blade Runner 2049|2017|Denis Villeneuve]] — Hans Zimmer and Benjamin Wallfisch created a soundscape that honors Vangelis while pushing into new territory.

[[film_ext:Interstellar|2014|Christopher Nolan]] — Zimmer's organ-driven score is overwhelming in IMAX. "No Time for Caution" during the docking scene is peak cinema.

[[song_ext:Time|Hans Zimmer|2010]] — From Inception. The building layers of strings and brass have become the template for modern trailer music.

[[song_ext:Where Is My Mind?|Pixies|1988]] — The Fight Club ending. Sometimes the perfect needle drop is worth more than any original score.

The best soundtracks don't just accompany the film — they become inseparable from the experience.`,
    expected: { films: 3, songs: 2 },
  },

  // ─── Mixed: Books + Podcasts ────────────────────────────────
  {
    id: 'seed-mixed-books-podcasts',
    types: ['books', 'podcasts'],
    userQuery: 'I want to learn about Austrian economics, what should I read and listen to?',
    assistantResponse: `Here's a complete Austrian economics learning path:

## Essential Reading

[[book_ext:Economics in One Lesson|Henry Hazlitt|1946]] — The best starting point. Hazlitt distills Bastiat's "seen and unseen" into a masterclass on economic thinking. Short, clear, devastating to interventionist arguments.

[[book_ext:Human Action|Ludwig von Mises|1949]] — The magnum opus of Austrian economics. Dense but comprehensive — covers praxeology, money, banking, and the business cycle. Read after Hazlitt.

[[book_ext:The Road to Serfdom|Friedrich Hayek|1944]] — Hayek's warning about central planning. Written during WWII, prophetic about the dangers of collectivism. Surprisingly readable.

[[book_ext:Man, Economy, and State|Murray Rothbard|1962]] — Rothbard's systematic treatise building from first principles. More accessible than Mises, more rigorous than Hazlitt.

## Podcasts for Ongoing Learning

[[podcast_ext:The Tom Woods Show|Tom Woods|2013]] — Daily episodes on economics, liberty, and history. Woods makes Austrian concepts accessible with humor and clarity.

[[podcast_ext:Human Action Podcast|Jeff Deist|2018]] — Chapter-by-chapter walkthrough of Mises' Human Action. Perfect companion to the book.

[[podcast_ext:EconTalk|Russ Roberts|2006]] — Not strictly Austrian, but Roberts' classical liberal perspective and interview style make this essential economics listening.

**Path:** Hazlitt → Tom Woods podcast → Road to Serfdom → Human Action book + podcast → Rothbard for depth.`,
    expected: { books: 4, podcasts: 3 },
  },

  // ─── Magazine / News ────────────────────────────────────────
  {
    id: 'seed-magazine',
    types: ['magazine'],
    userQuery: "What's happening in the Bitcoin ecosystem this week?",
    assistantResponse: `## Bitcoin Weekly Roundup

### Lightning Network Milestone
The Lightning Network reached 6,000 BTC in public capacity this week, a new all-time high. Channel count grew 12% month-over-month, with major growth in developing markets.

**amboss.space** — Track Lightning network statistics and node rankings.

---

### Ordinals Debate Heats Up
Bitcoin Core developers are split on whether to filter Ordinals inscriptions at the mempool level. Luke Dashjr's patch was rejected, but the debate continues about block space allocation.

**delvingbitcoin.org** — Developer discussion forum where the technical arguments play out.

---

### Regulatory Update: EU MiCA
The EU's Markets in Crypto-Assets regulation takes effect next month. Self-custody remains legal, but exchanges face new reporting requirements. Most Bitcoin-only companies are prepared.

---

### Mining Difficulty Adjustment
Difficulty increased 4.7% — the third consecutive positive adjustment. Hash rate continues climbing post-halving as miners optimize operations.

**mempool.space** — Real-time mempool and mining statistics.

---

*Sources: Bitcoin Magazine, Nostr, Lightning Labs blog*`,
    expected: { magazineSections: 4 },
  },

  // ─── Recipes ────────────────────────────────────────────────
  {
    id: 'seed-recipes',
    types: ['recipes'],
    userQuery: 'How do I make a proper Italian carbonara?',
    assistantResponse: `Here's an authentic Roman carbonara — no cream, no shortcuts:

<recipe_ext title="Spaghetti alla Carbonara" servings="4" time="25 min" calories="550">
- 400g spaghetti or rigatoni
- 200g guanciale (cured pork cheek), cut into strips
- 4 large egg yolks + 2 whole eggs
- 100g Pecorino Romano, finely grated
- Freshly ground black pepper
1. Bring a large pot of well-salted water to boil. Cook pasta 1 minute short of al dente.
2. Cut guanciale into 1cm strips. Cook in a cold dry pan over medium heat until fat renders and edges crisp (8-10 min). Remove from heat.
3. Whisk egg yolks, whole eggs, and most of the Pecorino together. Season generously with black pepper.
4. When pasta is ready, reserve 200ml pasta water. Drain and add pasta to the guanciale pan (off heat).
5. Wait 30 seconds for the pan to cool slightly, then pour the egg mixture over the pasta. Toss vigorously — the residual heat cooks the eggs into a creamy sauce. Add pasta water a splash at a time if needed.
6. Serve immediately with remaining Pecorino and more black pepper.
</recipe_ext>

**Keys to success:**
- Guanciale, not pancetta or bacon — the flavor is completely different
- Pan OFF heat before adding eggs — scrambled eggs ≠ carbonara
- Pecorino Romano, not Parmesan — sharper, saltier, authentic
- Save that pasta water — it's liquid gold for emulsifying the sauce`,
    expected: { recipes: 1 },
  },

  // ─── Events ─────────────────────────────────────────────────
  {
    id: 'seed-events',
    types: ['events'],
    userQuery: 'What Bitcoin conferences are coming up?',
    assistantResponse: `Here are the major upcoming Bitcoin events:

<event_ext title="Bitcoin 2026" date="2026-07-25" location="Nashville, TN" url="https://b.tc/conference">
The largest annual Bitcoin conference. 35,000+ attendees, major announcements, and networking. Past speakers include Saylor, Dorsey, and Bukele.
</event_ext>

<event_ext title="Baltic Honeybadger" date="2026-09-05" location="Riga, Latvia" url="https://baltichoneybadger.com">
The OG Bitcoin-only conference. Technical focus, cypherpunk ethos, and intimate atmosphere. 1,000 attendees who actually build things.
</event_ext>

<event_ext title="Adopting Bitcoin" date="2026-11-15" location="San Salvador, El Salvador" url="https://adoptingbitcoin.org">
Focused on real-world Bitcoin adoption. Lightning Network workshops, merchant onboarding, and stories from the ground in El Salvador.
</event_ext>

<event_ext title="TABConf" date="2026-10-10" location="Atlanta, GA" url="https://tabconf.com">
Developer-focused unconference. Hands-on workshops for building on Bitcoin and Lightning. Small, technical, high signal.
</event_ext>

**Tip:** Baltic Honeybadger sells out fast. TABConf is the best value for developers. Bitcoin 2026 is the big tent event.`,
    expected: { events: 4 },
  },

  // ─── TV + Places combined ───────────────────────────────────
  {
    id: 'seed-mixed-tv-places',
    types: ['tvSeries', 'places'],
    userQuery: 'Best food shows and where to eat what they feature',
    assistantResponse: `Food TV worth watching, with real-world destinations:

[[tv_ext:The Bear|Christopher Storer|2022–present]] — Set in a Chicago Italian beef shop. The kitchen chaos is real — watch it for the cooking AND the anxiety.

Eat the real thing at:
[[place_ext:Mr. Beef|Italian Beef|Chicago IL|4.2|1|666 N Orleans St]] — The actual shop that inspired The Bear. Cash only. Get the Italian beef dipped with hot giardiniera.

[[tv_ext:Chef's Table|David Gelb|2015–present]] — Netflix's cinematic exploration of the world's best chefs. Each episode is a visual feast.

Featured restaurant still taking reservations:
[[place_ext:Gaggan Anand|Indian|Bangkok|4.9|4|68/1 Soi Langsuan]] — The molecular Indian cuisine from Chef's Table Season 2. 25-course tasting menu.

[[tv_ext:Anthony Bourdain: Parts Unknown|Zero Point Zero|2013–2018]] — Bourdain's masterpiece. Food, culture, and humanity. The Japan and Iran episodes are peak television.

Bourdain's favorite:
[[place_ext:Bún Bò Huế Bà Phượng|Vietnamese|Huế Vietnam|4.5|1|Huế City]] — The noodle soup spot from the Vietnam episode. Simple, perfect, $2 a bowl.`,
    expected: { tvSeries: 3, places: 3 },
  },

  // ─── Tag stripping test — all tags in one response ──────────
  {
    id: 'seed-strip-tags',
    types: ['films', 'songs', 'books', 'tvSeries', 'places', 'podcasts'],
    userQuery: 'Give me your top pick in every category',
    assistantResponse: `Here's one masterpiece per category:

**Film:** [[film_ext:There Will Be Blood|2007|Paul Thomas Anderson]] — Daniel Day-Lewis as an oil prospector consumed by greed. The milkshake scene. The bowling alley. Perfect.

**Song:** [[song_ext:Bohemian Rhapsody|Queen|1975]] — Six minutes that redefined what a pop single could be. Opera section? Guitar solo? Headbanging? Yes to all.

**Book:** [[book_ext:Blood Meridian|Cormac McCarthy|1985]] — The darkest, most beautiful novel in American literature. The Judge is literature's greatest villain.

**TV Show:** [[tv_ext:The Wire|David Simon|2002–2008]] — Every institution fails. Every character is compromised. Baltimore becomes a lens for all of America.

**Restaurant:** [[place_ext:Jiro Sushi|Sushi|Tokyo|4.9|4|Ginza]] — 20 pieces of sushi. No menu. The greatest craftsman alive serves fish that transcends food.

**Podcast:** [[podcast_ext:Hardcore History|Dan Carlin|2006]] — Multi-hour epics on history's most dramatic moments. "Blueprint for Armageddon" (WWI) is the greatest podcast ever made.

One of each is all you need to start.`,
    expected: { films: 1, songs: 1, books: 1, tvSeries: 1, places: 1, podcasts: 1 },
  },
]

/**
 * Convert seed prompts to the .dev/chats.json conversation format.
 */
/** Build a single conversation with all seed prompts as sequential messages. */
export function seedPromptsToConversation(): {
  id: string
  title: string
  messages: { id: string; role: string; content: string; timestamp: number }[]
  createdAt: number
  updatedAt: number
} {
  const baseTime = 1772488800000
  const messages: { id: string; role: string; content: string; timestamp: number }[] = []

  for (let i = 0; i < seedPrompts.length; i++) {
    const seed = seedPrompts[i]
    const ts = baseTime + i * 60000
    messages.push(
      { id: `${seed.id}-q`, role: 'user', content: seed.userQuery, timestamp: ts },
      { id: `${seed.id}-a`, role: 'assistant', content: seed.assistantResponse, timestamp: ts + 3000 },
    )
  }

  return {
    id: 'seed-all',
    title: 'Content Showcase',
    messages,
    createdAt: baseTime,
    updatedAt: baseTime + seedPrompts.length * 60000,
  }
}
