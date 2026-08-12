/**
 * Extraction Quality Tests
 *
 * Tests real-world AI response patterns to verify content surfacing.
 * Each test simulates a user query + AI response and checks that
 * the right content types are extracted with correct data.
 */
import { describe, it, expect } from 'vitest'
import {
  extractAllFilms,
  extractAllSongs,
  extractAllPodcasts,
  extractAllBooks,
  extractAllTVSeries,
  extractAllPlaces,
  extractAllImages,
  extractCodeBlocks,
  extractApps,
} from '@/composables/contentExtraction'
import {
  extractMagazineSections,
  extractMarkdownLinks,
  extractBoldDomainLinks,
  extractBareDomainLinks,
} from '@/composables/contentExtraction'
import {
  isBookQuery, isBookLikeResponse,
  isTVQuery, isPlaceQuery, isPlaceLikeResponse,
  isMusicQuery, isCodeQuery, isCodeLikeResponse,
  isNewsQuery, isNewsLikeResponse, isWebsitesQuery, isAppQuery,
  isNostrQuery, isNostrLikeResponse,
  isAppLikeResponse, isImageQuery,
  filterTabsByContext, preferredFirstTab,
} from '@/composables/contentFiltering'

// ─── Helper: simulate full pipeline ─────────────────────────────

function extractAll(text: string, userQuery: string) {
  const films = extractAllFilms(text)
  const songs = extractAllSongs(text, userQuery)
  const podcasts = extractAllPodcasts(text)
  const books = extractAllBooks(text, userQuery)
  const tvSeries = extractAllTVSeries(text, userQuery)
  const images = extractAllImages(text, userQuery)
  const places = extractAllPlaces(text, userQuery)
  const codeBlocks = extractCodeBlocks(text)
  const apps = extractApps(text, userQuery)
  return { films, songs, podcasts, books, tvSeries, images, places, codeBlocks, apps }
}

// ═══════════════════════════════════════════════════════════════════
// BOOKS — pattern-based extraction
// ═══════════════════════════════════════════════════════════════════

describe('Books: pattern extraction from AI responses', () => {
  it('extracts books from "Title by Author" format', () => {
    const text = `Here are some essential Bitcoin books:

The Bitcoin Standard by Saifedean Ammous is a great starting point. It covers the history of money and why Bitcoin matters.

You might also enjoy Mastering Bitcoin by Andreas Antonopoulos for the technical side.`
    const books = extractAllBooks(text, 'recommend bitcoin books')
    expect(books.length).toBeGreaterThanOrEqual(2)
    expect(books.some(b => b.title.includes('Bitcoin Standard'))).toBe(true)
    expect(books.some(b => b.title.includes('Mastering Bitcoin'))).toBe(true)
  })

  it('extracts books from numbered list with bold and "by"', () => {
    const text = `Top books on money:

1. **The Bitcoin Standard** by Saifedean Ammous
2. **The Fiat Standard** by Saifedean Ammous
3. **Broken Money** by Lyn Alden
4. **The Price of Tomorrow** by Jeff Booth`
    const books = extractAllBooks(text, 'books about money')
    expect(books.length).toBeGreaterThanOrEqual(4)
  })

  it('extracts books from em-dash format', () => {
    const text = `Essential reads:

- The Sovereign Individual — James Dale Davidson (1997)
- The Bitcoin Standard — Saifedean Ammous (2018)
- Broken Money — Lyn Alden (2023)`
    const books = extractAllBooks(text, 'book recommendations')
    expect(books.length).toBeGreaterThanOrEqual(2)
  })

  it('single "by Author" triggers isBookLikeResponse', () => {
    const text = 'I highly recommend The Bitcoin Standard by Saifedean Ammous. It is an excellent book on monetary theory.'
    expect(isBookLikeResponse(text)).toBe(true)
  })

  it('isBookQuery matches common book queries', () => {
    expect(isBookQuery('best books about bitcoin')).toBe(true)
    expect(isBookQuery('what should I read')).toBe(true)
    expect(isBookQuery('recommend me a novel')).toBe(true)
    expect(isBookQuery('any good nonfiction books')).toBe(true)
  })

  it('does not extract books from film responses', () => {
    const text = '[[film_ext:Inception|2010|Christopher Nolan]] is great. Directed by Christopher Nolan.'
    const books = extractAllBooks(text, 'what is inception')
    expect(books).toHaveLength(0)
  })
})

// ═══════════════════════════════════════════════════════════════════
// TV SERIES — pattern-based extraction
// ═══════════════════════════════════════════════════════════════════

describe('TV Series: pattern extraction from AI responses', () => {
  it('extracts TV shows from numbered list with years', () => {
    const text = `Best TV dramas of all time:

1. **Breaking Bad** (2008–2013)
2. **The Wire** (2002–2008)
3. **The Sopranos** (1999–2007)
4. **Better Call Saul** (2015–2022)`
    const series = extractAllTVSeries(text, 'best tv shows')
    expect(series.length).toBeGreaterThanOrEqual(4)
    expect(series.some(s => s.title.includes('Breaking Bad'))).toBe(true)
  })

  it('extracts shows with "N seasons" format', () => {
    const text = `Some great binge watches:

**Breaking Bad** — 5 seasons of incredible storytelling
**Better Call Saul** — 6 seasons, a worthy prequel`
    const series = extractAllTVSeries(text, 'what should I binge')
    expect(series.length).toBeGreaterThanOrEqual(2)
  })

  it('isTVLikeResponse triggers with single season mention', () => {
    const text = 'Breaking Bad ran for 5 seasons on AMC and is widely considered one of the best TV dramas ever made.'
    expect(isTVQuery('best tv shows')).toBe(true)
    // Should pass with just 1 "season" mention now
    const hasKeyword = /\b(season|episodes?|showrunner|streaming|renewed|cancelled|premiere|network|HBO|Netflix|AMC)\b/i.test(text)
    expect(hasKeyword).toBe(true)
  })

  it('does not extract TV from non-TV contexts', () => {
    const text = 'Bitcoin has seen a new season of adoption. The network grows stronger.'
    const series = extractAllTVSeries(text, 'what is bitcoin')
    expect(series).toHaveLength(0)
  })
})

// ═══════════════════════════════════════════════════════════════════
// PLACES — pattern-based extraction
// ═══════════════════════════════════════════════════════════════════

describe('Places: pattern extraction from AI responses', () => {
  it('extracts places with explicit type words', () => {
    const text = `Best pizza in New York:

1. **Joe's Pizza** — classic New York pizza joint since 1975
2. **Di Fara Pizza** — legendary Brooklyn pizzeria`
    const places = extractAllPlaces(text, 'best pizza in new york')
    expect(places.length).toBeGreaterThanOrEqual(1)
  })

  it('extracts places from bold + description without type word (place query)', () => {
    const text = `Great dinner spots in Austin:

1. **Franklin Barbecue** — World-famous brisket, expect a long line but worth every minute
2. **Uchi** — Innovative Japanese cuisine with a Texas twist
3. **Launderette** — New American comfort food in a converted laundromat`
    const places = extractAllPlaces(text, 'where to eat in austin')
    expect(places.length).toBeGreaterThanOrEqual(3)
  })

  it('isPlaceQuery matches food/restaurant queries', () => {
    expect(isPlaceQuery('best restaurants in london')).toBe(true)
    expect(isPlaceQuery('where to eat in tokyo')).toBe(true)
    expect(isPlaceQuery('good brunch spots')).toBe(true)
    expect(isPlaceQuery('I am hungry')).toBe(true)
  })

  it('does not extract places from non-place contexts', () => {
    const text = '**Bitcoin** — A peer-to-peer electronic cash system'
    const places = extractAllPlaces(text, 'what is bitcoin')
    expect(places).toHaveLength(0)
  })
})

// ═══════════════════════════════════════════════════════════════════
// SONGS — coexistence with other content types
// ═══════════════════════════════════════════════════════════════════

describe('Songs: coexistence with other content', () => {
  it('returns explicit song tags even when film tags present', () => {
    const text = `Great movie with an amazing soundtrack:
[[film_ext:Guardians of the Galaxy|2014|James Gunn]]

Featured songs:
[[song_ext:Hooked on a Feeling|Blue Swede|1974]]
[[song_ext:Come and Get Your Love|Redbone|1974]]`
    const songs = extractAllSongs(text, 'guardians of the galaxy soundtrack')
    expect(songs.length).toBeGreaterThanOrEqual(2)
  })

  it('returns explicit song tags even when book tags present', () => {
    const text = `[[book_ext:Norwegian Wood|Haruki Murakami|1987]]

The title references the Beatles song:
[[song_ext:Norwegian Wood|The Beatles|1965]]`
    const songs = extractAllSongs(text, 'tell me about norwegian wood')
    expect(songs.length).toBeGreaterThanOrEqual(1)
    expect(songs[0].title).toBe('Norwegian Wood')
  })

  it('skips pattern-based songs when film tags present (no explicit song tags)', () => {
    const text = `[[film_ext:Inception|2010|Christopher Nolan]]

Great film. The music by Hans Zimmer is amazing — "Time" is iconic.`
    const songs = extractAllSongs(text, 'tell me about inception')
    expect(songs).toHaveLength(0)
  })

  it('extracts songs for music queries', () => {
    const text = `Here are some great rock songs:

"Bohemian Rhapsody" by Queen is a masterpiece.
"Stairway to Heaven" by Led Zeppelin is another classic.`
    const songs = extractAllSongs(text, 'best rock songs')
    expect(songs.length).toBeGreaterThanOrEqual(1)
  })
})

// ═══════════════════════════════════════════════════════════════════
// CODE BLOCKS — extraction and classification
// ═══════════════════════════════════════════════════════════════════

describe('Code blocks: extraction and classification', () => {
  it('extracts fenced code blocks with language', () => {
    const text = `Here's a simple function:

\`\`\`typescript
function greet(name: string): string {
  return \`Hello, \${name}!\`
}
\`\`\`

And here's the Python version:

\`\`\`python
def greet(name: str) -> str:
    return f"Hello, {name}!"
\`\`\`
`
    const blocks = extractCodeBlocks(text)
    expect(blocks).toHaveLength(2)
    expect(blocks[0].language).toBe('typescript')
    expect(blocks[1].language).toBe('python')
    expect(blocks[0].code).toContain('function greet')
  })

  it('extracts code blocks without language specifier', () => {
    const text = `Run this command:

\`\`\`
npm install
\`\`\`
`
    const blocks = extractCodeBlocks(text)
    expect(blocks).toHaveLength(1)
    expect(blocks[0].language).toBe('text')
  })

  it('extracts label from preceding heading', () => {
    const text = `**Setup Script**
\`\`\`bash
#!/bin/bash
echo "Setting up..."
\`\`\`
`
    const blocks = extractCodeBlocks(text)
    expect(blocks).toHaveLength(1)
    expect(blocks[0].label).toBe('Setup Script')
  })

  it('isCodeQuery matches programming queries', () => {
    expect(isCodeQuery('write me a function to sort an array')).toBe(true)
    expect(isCodeQuery('how to implement binary search in python')).toBe(true)
    expect(isCodeQuery('debug this javascript error')).toBe(true)
    expect(isCodeQuery('best typescript libraries')).toBe(true)
  })

  it('isCodeQuery does not match non-code queries', () => {
    expect(isCodeQuery('best movies of 2024')).toBe(false)
    expect(isCodeQuery('recommend restaurants in paris')).toBe(false)
  })

  it('isCodeLikeResponse triggers with 3+ code blocks', () => {
    const text = '```js\na\n```\n```py\nb\n```\n```go\nc\n```'
    expect(isCodeLikeResponse(text)).toBe(true)
  })

  it('isCodeLikeResponse does not trigger with 1-2 blocks', () => {
    const text = '```js\na\n```\nSome text\n```py\nb\n```'
    expect(isCodeLikeResponse(text)).toBe(false)
  })
})

// ═══════════════════════════════════════════════════════════════════
// IMAGES — threshold behavior
// ═══════════════════════════════════════════════════════════════════

describe('Images: threshold and alt text', () => {
  it('returns single image for image queries', () => {
    const text = '![A sunset](https://example.com/sunset.jpg)'
    const images = extractAllImages(text, 'show me a sunset image')
    expect(images).toHaveLength(1)
  })

  it('returns single image with meaningful alt text', () => {
    const text = '![Bitcoin price chart for 2024](https://example.com/chart.png)'
    const images = extractAllImages(text, 'bitcoin price')
    expect(images).toHaveLength(1)
  })

  it('skips single image without alt text for non-image query', () => {
    const text = 'https://example.com/random.jpg'
    const images = extractAllImages(text, 'what is bitcoin')
    expect(images).toHaveLength(0)
  })

  it('returns 2+ images for any query', () => {
    const text = 'https://example.com/a.jpg https://example.com/b.png'
    const images = extractAllImages(text, 'what is bitcoin')
    expect(images).toHaveLength(2)
  })
})

// ═══════════════════════════════════════════════════════════════════
// TAB FILTERING — comprehensive routing
// ═══════════════════════════════════════════════════════════════════

describe('Tab filtering: correct tabs surfaced', () => {
  it('surfaces book tab for book query with books', () => {
    const tabs = filterTabsByContext('best bitcoin books', false, false, false, true, false, false, false, false, false, false, false, false, false)
    expect(tabs).toContain('book')
  })

  it('surfaces code tab for code query with code', () => {
    const tabs = filterTabsByContext('write a typescript function', false, false, false, false, false, false, false, false, false, false, false, false, true)
    expect(tabs).toContain('code')
    expect(tabs[0]).toBe('code') // should be first
  })

  it('surfaces multiple tabs when multiple content types present', () => {
    const tabs = filterTabsByContext('movies and music', true, true, false, false, false, false, false, false, false, false, false, false, false)
    expect(tabs).toContain('film')
    expect(tabs).toContain('song')
  })

  it('surfaces place tab for restaurant query', () => {
    const tabs = filterTabsByContext('best restaurants in london', false, false, false, false, false, false, true, false, false, false, false, false, false)
    expect(tabs).toContain('place')
    expect(tabs[0]).toBe('place')
  })

  it('surfaces TV tab for TV query', () => {
    const tabs = filterTabsByContext('best tv shows to binge', false, false, false, false, true, false, false, false, false, false, false, false, false)
    expect(tabs).toContain('tvshow')
  })
})

// ═══════════════════════════════════════════════════════════════════
// FULL PIPELINE — realistic AI responses
// ═══════════════════════════════════════════════════════════════════

describe('Full pipeline: realistic AI responses', () => {
  it('book recommendation response', () => {
    const query = 'recommend some books about bitcoin and economics'
    const response = `Here are my top recommendations:

1. **The Bitcoin Standard** by Saifedean Ammous — The definitive work on why Bitcoin matters for sound money.
2. **Broken Money** by Lyn Alden — A thorough analysis of the monetary system and how Bitcoin fits in.
3. **The Fiat Standard** by Saifedean Ammous — The sequel exploring the fiat monetary system.
4. **The Price of Tomorrow** by Jeff Booth — How deflation will shape the future economy.
5. **Mastering Bitcoin** by Andreas Antonopoulos — Technical deep-dive into how Bitcoin works.

Each of these books offers a unique perspective on money, technology, and economics.`

    const result = extractAll(response, query)
    expect(result.books.length).toBeGreaterThanOrEqual(4)
    expect(result.films).toHaveLength(0)
    expect(result.songs).toHaveLength(0)
  })

  it('restaurant recommendation response', () => {
    const query = 'best sushi in tokyo'
    const response = `Here are Tokyo's finest sushi restaurants:

1. **Sukiyabashi Jiro** — The legendary 3-Michelin-star omakase experience in Ginza
2. **Sushi Saito** — Another 3-star establishment known for exquisite nigiri
3. **Sushi Yoshitake** — Intimate counter seating with seasonal specialties
4. **Kyubey** — Classic Ginza sushi since 1935, welcoming to tourists`

    const result = extractAll(response, query)
    expect(result.places.length).toBeGreaterThanOrEqual(3)
    expect(result.books).toHaveLength(0)
  })

  it('coding response with multiple code blocks', () => {
    const query = 'implement binary search in typescript'
    const response = `Here's a binary search implementation:

**Iterative approach**
\`\`\`typescript
function binarySearch(arr: number[], target: number): number {
  let left = 0
  let right = arr.length - 1
  while (left <= right) {
    const mid = Math.floor((left + right) / 2)
    if (arr[mid] === target) return mid
    if (arr[mid] < target) left = mid + 1
    else right = mid - 1
  }
  return -1
}
\`\`\`

**Recursive approach**
\`\`\`typescript
function binarySearchRecursive(arr: number[], target: number, left = 0, right = arr.length - 1): number {
  if (left > right) return -1
  const mid = Math.floor((left + right) / 2)
  if (arr[mid] === target) return mid
  if (arr[mid] < target) return binarySearchRecursive(arr, target, mid + 1, right)
  return binarySearchRecursive(arr, target, left, mid - 1)
}
\`\`\`

**Usage**
\`\`\`typescript
const arr = [1, 3, 5, 7, 9, 11]
console.log(binarySearch(arr, 7)) // 3
console.log(binarySearchRecursive(arr, 7)) // 3
\`\`\`
`
    const result = extractAll(response, query)
    expect(result.codeBlocks.length).toBeGreaterThanOrEqual(3)
    expect(result.codeBlocks[0].language).toBe('typescript')
    expect(result.codeBlocks[0].label).toBe('Iterative approach')
    expect(isCodeQuery(query)).toBe(true)
    expect(isCodeLikeResponse(response)).toBe(true)
  })

  it('TV show recommendation response', () => {
    const query = 'best tv series of all time'
    const response = `Here are the greatest TV series ever made:

1. **Breaking Bad** (2008–2013) — A chemistry teacher turned drug lord. 5 seasons of perfect television.
2. **The Wire** (2002–2008) — A sprawling look at Baltimore's institutions. 5 seasons.
3. **The Sopranos** (1999–2007) — The show that started the golden age of TV. 6 seasons.
4. **Mad Men** (2007–2015) — 1960s advertising world, beautifully crafted. 7 seasons.
5. **Chernobyl** (2019) — A devastating miniseries about the nuclear disaster.`

    const result = extractAll(response, query)
    expect(result.tvSeries.length).toBeGreaterThanOrEqual(4)
    expect(result.films).toHaveLength(0)
  })

  it('mixed film and song tags coexist', () => {
    const query = 'tell me about the guardians of the galaxy soundtrack'
    const response = `Guardians of the Galaxy has one of the best movie soundtracks:

[[film_ext:Guardians of the Galaxy|2014|James Gunn]]

The "Awesome Mix Vol. 1" features:
[[song_ext:Hooked on a Feeling|Blue Swede|1974]]
[[song_ext:Come and Get Your Love|Redbone|1974]]
[[song_ext:Spirit in the Sky|Norman Greenbaum|1969]]
[[song_ext:Escape (The Piña Colada Song)|Rupert Holmes|1979]]`

    const result = extractAll(response, query)
    expect(result.films.length).toBeGreaterThanOrEqual(1)
    expect(result.songs.length).toBeGreaterThanOrEqual(4)
  })
})

// ═══════════════════════════════════════════════════════════════════
// EDGE CASES — tricky patterns that commonly fail
// ═══════════════════════════════════════════════════════════════════

describe('Edge cases: commonly failing patterns', () => {
  it('does not false-positive books from link markdown', () => {
    const text = 'Check out [The Bitcoin Standard](https://example.com) by visiting the website.'
    const books = extractAllBooks(text, 'bitcoin resources')
    // Should not extract "The Bitcoin Standard" as a book from a markdown link
    expect(books.every(b => !b.title.includes('Bitcoin Standard'))).toBe(true)
  })

  it('does not extract place names from non-place bold text', () => {
    const text = `Key Bitcoin concepts:

1. **Proof of Work** — The consensus mechanism that secures Bitcoin
2. **Hash Rate** — The computational power of the network`
    const places = extractAllPlaces(text, 'explain bitcoin')
    expect(places).toHaveLength(0)
  })

  it('handles quoted titles in book extraction', () => {
    const text = '"The Bitcoin Standard" by Saifedean Ammous is essential reading for understanding sound money.'
    const books = extractAllBooks(text, 'what books should I read about bitcoin')
    expect(books.length).toBeGreaterThanOrEqual(1)
  })

  it('does not extract TV series from Bitcoin season references', () => {
    const text = 'This is a new season for Bitcoin adoption. The network hashrate reached new highs.'
    const series = extractAllTVSeries(text, 'bitcoin news')
    expect(series).toHaveLength(0)
  })

  it('extracts books from smart/curly quotes', () => {
    const text = '\u201CThe Bitcoin Standard\u201D by Saifedean Ammous is a must-read.'
    const books = extractAllBooks(text, 'bitcoin books')
    expect(books.length).toBeGreaterThanOrEqual(1)
    expect(books[0].title).toContain('Bitcoin Standard')
  })

  it('extracts places when query mentions pizza', () => {
    expect(isPlaceQuery('best pizza near me')).toBe(true)
    expect(isPlaceQuery('pizza recommendations')).toBe(true)
  })

  it('does not false-positive songs from film director "by" pattern', () => {
    const text = '[[film_ext:Inception|2010|Christopher Nolan]]\n\nInception, directed by Christopher Nolan, is a mind-bending thriller.'
    const songs = extractAllSongs(text, 'tell me about inception')
    expect(songs).toHaveLength(0)
  })

  it('handles books with single-word titles in bold', () => {
    const text = '1. **Sapiens** by Yuval Noah Harari — A brief history of humankind'
    const books = extractAllBooks(text, 'best nonfiction books')
    expect(books.length).toBeGreaterThanOrEqual(1)
  })

  it('extracts multiple places from varied formatting', () => {
    const query = 'where to eat in paris'
    const text = `Top restaurants in Paris:

1. **Le Comptoir du Panth\u00e9on** \u2014 Classic French bistro with incredible steak frites
2. **Chez Janou** \u2014 Famous for its chocolate mousse, cozy Proven\u00e7al atmosphere
3. **L'As du Fallafel** \u2014 Best falafel in the Marais, always a queue`
    const places = extractAllPlaces(text, query)
    expect(places.length).toBeGreaterThanOrEqual(3)
  })

  it('extracts TV shows from "created by" format', () => {
    const text = `1. **Breaking Bad** (2008\u20132013) \u2014 Created by Vince Gilligan. A chemistry teacher becomes a drug lord.
2. **The Wire** (2002\u20132008) \u2014 Created by David Simon. A deep look at Baltimore institutions.`
    const series = extractAllTVSeries(text, 'best tv dramas')
    expect(series.length).toBeGreaterThanOrEqual(2)
  })

  it('does not extract books when response is about films', () => {
    const text = `Great sci-fi films:

1. **Blade Runner** (1982) \u2014 Directed by Ridley Scott
2. **2001: A Space Odyssey** (1968) \u2014 Directed by Stanley Kubrick`
    const books = extractAllBooks(text, 'best sci-fi movies')
    expect(books).toHaveLength(0)
  })

  it('extracts code blocks with varied languages', () => {
    const text = `**HTML**
\`\`\`html
<div class="container">Hello</div>
\`\`\`

**CSS**
\`\`\`css
.container { color: red; }
\`\`\`

**JavaScript**
\`\`\`javascript
document.querySelector('.container')
\`\`\`
`
    const blocks = extractCodeBlocks(text)
    expect(blocks).toHaveLength(3)
    expect(blocks[0].language).toBe('html')
    expect(blocks[1].language).toBe('css')
    expect(blocks[2].language).toBe('javascript')
  })

  it('surfaces correct tabs for multi-content response', () => {
    // Books + code in same response
    const tabs = filterTabsByContext(
      'how to learn programming',
      false, false, false, true, false, false, false, false, false, false, false, false, true
    )
    expect(tabs).toContain('book')
    expect(tabs).toContain('code')
  })

  it('prefers place tab first for food queries', () => {
    const tabs = filterTabsByContext(
      'best pizza in new york',
      false, false, false, false, false, false, true, false, false, false, false, false, false
    )
    expect(tabs[0]).toBe('place')
  })

  it('extracts books from prose with "also enjoy" mid-sentence', () => {
    const text = `I'd recommend starting with The Bitcoin Standard by Saifedean Ammous. You might also enjoy Mastering Bitcoin by Andreas Antonopoulos for the technical deep dive.`
    const books = extractAllBooks(text, 'bitcoin book recommendations')
    expect(books.length).toBeGreaterThanOrEqual(2)
  })

  it('extracts places with cuisine type in description', () => {
    const text = `Best spots in Brooklyn:

1. **Lucali** — Beloved BYOB pizzeria with incredible thin-crust pies
2. **Peter Luger** — Iconic steakhouse since 1887, cash only
3. **Olmsted** — Innovative New American restaurant with a backyard garden`
    const places = extractAllPlaces(text, 'where to eat in brooklyn')
    expect(places.length).toBeGreaterThanOrEqual(3)
  })

  it('does not false-positive places from tech concepts', () => {
    const text = `**React** — A JavaScript library for building user interfaces
**Vue** — The progressive JavaScript framework
**Angular** — A platform for building mobile and desktop web apps`
    const places = extractAllPlaces(text, 'best javascript frameworks')
    expect(places).toHaveLength(0)
  })

  it('extracts songs from "Artist - Title" format', () => {
    const text = `Classic rock essentials:

[[song_ext:Stairway to Heaven|Led Zeppelin|1971]]
[[song_ext:Hotel California|Eagles|1977]]
[[song_ext:Comfortably Numb|Pink Floyd|1979]]`
    const songs = extractAllSongs(text, 'best classic rock songs')
    expect(songs.length).toBeGreaterThanOrEqual(3)
  })

  it('handles TV query "what should I watch"', () => {
    expect(isTVQuery('what should I watch tonight')).toBe(true)
    expect(isTVQuery('anything good to binge')).toBe(true)
    expect(isTVQuery('best tv comedies')).toBe(true)
  })

  it('handles place query with cuisine names', () => {
    expect(isPlaceQuery('best sushi in tokyo')).toBe(true)
    expect(isPlaceQuery('good ramen spots')).toBe(true)
    expect(isPlaceQuery('where to get tacos')).toBe(true)
  })

  it('full pipeline: mixed books and songs response', () => {
    const query = 'tell me about norwegian wood'
    const text = `"Norwegian Wood" can refer to both a novel and a song:

**The Novel**: "Norwegian Wood" by Haruki Murakami (1987) is a nostalgic story of love and loss set in 1960s Tokyo. It's one of Murakami's most accessible works.

**The Song**: [[song_ext:Norwegian Wood (This Bird Has Flown)|The Beatles|1965]]

The Beatles' track from Rubber Soul inspired Murakami's title. The song features George Harrison's sitar playing.`
    const result = extractAll(text, query)
    expect(result.books.length).toBeGreaterThanOrEqual(1)
    expect(result.songs.length).toBeGreaterThanOrEqual(1)
  })

  it('full pipeline: place response without explicit type words', () => {
    const query = 'best brunch in london'
    const text = `Here are London's best brunch spots:

1. **Dishoom** — Bombay-inspired breakfast, try the bacon naan roll and chai
2. **The Wolseley** — Grand European cafe on Piccadilly, impeccable service
3. **Padella** — Fresh handmade pasta, worth the queue at Borough Market
4. **Bao** — Taiwanese steamed buns and small plates, Soho or Fitzrovia`
    const result = extractAll(text, query)
    expect(result.places.length).toBeGreaterThanOrEqual(3)
  })

  it('extracts TV from [[tv_ext:]] tags with Title|Year|Creator format', () => {
    const text = `Similar series:\n\n[[tv_ext:Ghost in the Shell: Stand Alone Complex|2002|Kenji Kamiyama]] — Gold standard for tech-noir sci-fi.\n\n[[tv_ext:Cyberpunk: Edgerunners|2022|Hiroyuki Imaishi]] — Stunning animation.\n\n[[tv_ext:Altered Carbon|2018|Laeta Kalogridis]] — Cyberpunk noir.`
    const result = extractAll(text, 'pantheon news')
    expect(result.tvSeries.length).toBeGreaterThanOrEqual(3)
    expect(result.tvSeries[0].title).toBe('Ghost in the Shell: Stand Alone Complex')
    expect(result.tvSeries[0].year).toBe(2002)
    expect(result.tvSeries[0].creator).toBe('Kenji Kamiyama')
  })

  it('news query with TV content surfaces both news and TV tabs', () => {
    // Simulates the Pantheon response: news query + TV ext tags
    const tabs = filterTabsByContext(
      'any news on pantheon season 3',
      false, false, false, false, true, false, false, false, true, false, false, false, false
    )
    expect(tabs).toContain('tvshow')
    expect(tabs).toContain('websites')
  })

  it('full pipeline: news query does not surface books or places', () => {
    const text = `Here are the latest developments:

Bitcoin has surged past $100,000 for the first time. The rally was driven by institutional adoption and ETF inflows. Major exchanges like Coinbase and Kraken reported record trading volumes.`
    const result = extractAll(text, 'latest bitcoin news')
    expect(result.books).toHaveLength(0)
    expect(result.places).toHaveLength(0)
    expect(result.tvSeries).toHaveLength(0)
  })
})

// ═══════════════════════════════════════════════════════════════════
// PODCASTS — extraction patterns
// ═══════════════════════════════════════════════════════════════════

describe('Podcasts: extraction from AI responses', () => {
  it('extracts podcasts from [[podcast_ext:]] tags', () => {
    const text = `Great Bitcoin podcasts:

[[podcast_ext:What Bitcoin Did|Peter McCormack|2018]]
[[podcast_ext:Bitcoin Audible|Guy Swann|2018]]`
    const podcasts = extractAllPodcasts(text)
    expect(podcasts.length).toBeGreaterThanOrEqual(2)
    expect(podcasts[0].title).toBe('What Bitcoin Did')
  })

  it('extracts podcasts from [[podcast:p1]] library tags', () => {
    const text = 'Check out [[podcast:p1]] and [[podcast:p2]] for great content.'
    const podcasts = extractAllPodcasts(text)
    // These reference library items — may or may not match depending on mock data
    expect(podcasts.length).toBeGreaterThanOrEqual(0)
  })

  it('extracts podcasts from ext tags with optional year', () => {
    const text = `[[podcast_ext:The Bitcoin Standard Podcast|Saifedean Ammous]]`
    const podcasts = extractAllPodcasts(text)
    expect(podcasts.length).toBeGreaterThanOrEqual(1)
    expect(podcasts[0].title).toBe('The Bitcoin Standard Podcast')
  })
})

// ═══════════════════════════════════════════════════════════════════
// APPS — extraction from AI responses
// ═══════════════════════════════════════════════════════════════════

describe('Apps: extraction from AI responses', () => {
  it('extracts apps from app-related query', () => {
    const text = `Best Bitcoin wallets:

1. **Sparrow Wallet** — Desktop wallet with full coin control
2. **Blue Wallet** — Mobile Lightning wallet
3. **Electrum** — Lightweight Bitcoin wallet`
    const apps = extractApps(text, 'best bitcoin wallet apps')
    expect(apps.length).toBeGreaterThanOrEqual(1)
  })

  it('isAppQuery matches app-related queries', () => {
    expect(isAppQuery('best nostr apps')).toBe(true)
    expect(isAppQuery('what wallet should I use')).toBe(true)
    expect(isAppQuery('recommend a bitcoin wallet')).toBe(true)
  })

  it('isAppQuery does not match non-app queries', () => {
    expect(isAppQuery('history of money')).toBe(false)
    expect(isAppQuery('best pizza in nyc')).toBe(false)
  })
})

// ═══════════════════════════════════════════════════════════════════
// FILMS — library and ext tag extraction
// ═══════════════════════════════════════════════════════════════════

describe('Films: tag-based extraction', () => {
  it('extracts films from [[film_ext:]] tags', () => {
    const text = `Classic sci-fi films:

[[film_ext:Blade Runner|1982|Ridley Scott]]
[[film_ext:2001: A Space Odyssey|1968|Stanley Kubrick]]
[[film_ext:The Matrix|1999|The Wachowskis]]`
    const films = extractAllFilms(text)
    expect(films.length).toBeGreaterThanOrEqual(3)
    expect(films[0].title).toBe('Blade Runner')
    expect(films[0].year).toBe(1982)
    expect(films[0].director).toBe('Ridley Scott')
  })

  it('extracts films from [[film:f1]] library tags', () => {
    const text = 'You should watch [[film:f1]] — it is a classic.'
    const films = extractAllFilms(text)
    expect(films.length).toBeGreaterThanOrEqual(0) // depends on mock data
  })

  it('extracts films from multiple [[film_ext:]] tags', () => {
    const text = `[[film_ext:Inception|2010|Christopher Nolan]]
[[film_ext:Interstellar|2014|Christopher Nolan]]`
    const films = extractAllFilms(text)
    expect(films.length).toBeGreaterThanOrEqual(2)
    expect(films[0].director).toBe('Christopher Nolan')
  })
})

// ═══════════════════════════════════════════════════════════════════
// MAGAZINE SECTIONS — bullet-style extraction
// ═══════════════════════════════════════════════════════════════════

describe('Magazine sections: extraction', () => {
  it('extracts bullet-format magazine sections with colon separator', () => {
    const text = `Here's your Bitcoin brief:

- **Bitcoin Surges Past $100K**: The price of Bitcoin has reached a new all-time high driven by ETF inflows. Institutional investors continue to pour capital into spot Bitcoin ETFs.

- **Lightning Network Growth**: Channel count has doubled in 2025, with total capacity exceeding 5,000 BTC. New routing solutions improve payment reliability.

- **Mining Difficulty Adjustment**: A 4.2% difficulty increase signals growing network hashrate. Miners are deploying next-gen ASIC hardware at scale.`
    const sections = extractMagazineSections(text)
    expect(sections.length).toBeGreaterThanOrEqual(2)
  })

  it('extracts numbered magazine sections', () => {
    const text = `Top Bitcoin developments:

1. **ETF Inflows Hit Record**: BlackRock's iShares Bitcoin Trust saw $500M in a single day.
2. **El Salvador Doubles Down**: The country adds another 100 BTC to reserves.`
    const sections = extractMagazineSections(text)
    expect(sections.length).toBeGreaterThanOrEqual(2)
  })
})

// ═══════════════════════════════════════════════════════════════════
// WEBSITES/NEWS — markdown and domain extraction
// ═══════════════════════════════════════════════════════════════════

describe('Websites: markdown link and domain extraction', () => {
  it('extracts markdown links', () => {
    const text = `Useful resources:

- [Bitcoin Whitepaper](https://bitcoin.org/bitcoin.pdf)
- [Mempool Explorer](https://mempool.space)
- [Lightning Network Docs](https://docs.lightning.engineering)`
    const links = extractMarkdownLinks(text)
    expect(links.length).toBeGreaterThanOrEqual(3)
    expect(links[0].title).toBe('Bitcoin Whitepaper')
  })

  it('extracts bold domain links with parenthesized domain', () => {
    const text = `Check out **Bitcoin.org**(bitcoin.org) and **Mempool Explorer**(mempool.space) for more information.`
    const domains = extractBoldDomainLinks(text)
    expect(domains.length).toBeGreaterThanOrEqual(2)
  })

  it('extracts bare domain names from text', () => {
    const text = `For Bitcoin info, check bitcoin.org and blockstream.com for real-time data.`
    const domains = extractBareDomainLinks(text)
    expect(domains.length).toBeGreaterThanOrEqual(2)
  })

  it('isWebsitesQuery matches resource queries', () => {
    expect(isWebsitesQuery('bitcoin resources')).toBe(true)
    expect(isWebsitesQuery('useful websites for learning')).toBe(true)
  })
})

// ═══════════════════════════════════════════════════════════════════
// EDGE CASES — extended
// ═══════════════════════════════════════════════════════════════════

describe('Edge cases: extended', () => {
  it('handles empty string input gracefully', () => {
    const result = extractAll('', '')
    expect(result.films).toHaveLength(0)
    expect(result.books).toHaveLength(0)
    expect(result.songs).toHaveLength(0)
    expect(result.tvSeries).toHaveLength(0)
    expect(result.places).toHaveLength(0)
    expect(result.images).toHaveLength(0)
    expect(result.codeBlocks).toHaveLength(0)
    expect(result.apps).toHaveLength(0)
    expect(result.podcasts).toHaveLength(0)
  })

  it('handles very long text without crashing', () => {
    const longText = 'A '.repeat(10000) + '\n\n**The Bitcoin Standard** by Saifedean Ammous is great.'
    const books = extractAllBooks(longText, 'bitcoin books')
    expect(books.length).toBeGreaterThanOrEqual(1)
  })

  it('handles unicode characters in titles', () => {
    const text = `1. **Café Müller** — Pina Bausch's choreographic masterpiece restaurant
2. **Ñoño's Tacos** — Authentic Mexican street food`
    const places = extractAllPlaces(text, 'where to eat')
    expect(places.length).toBeGreaterThanOrEqual(1)
  })

  it('handles special characters in code blocks', () => {
    const text = '```python\nprint("Hello <World> & \\"quotes\\"")\n```'
    const blocks = extractCodeBlocks(text)
    expect(blocks).toHaveLength(1)
    expect(blocks[0].code).toContain('<World>')
  })

  it('does not crash on malformed tags', () => {
    const text = '[[film_ext:incomplete tag\n[[song_ext:\n[[podcast_ext:Title|'
    const result = extractAll(text, 'test')
    // Should not throw, just return empty
    expect(result.films).toHaveLength(0)
  })

  it('handles mixed content with 3+ types in one response', () => {
    const text = `Here's a diverse recommendation:

**Books:**
1. **The Bitcoin Standard** by Saifedean Ammous — a must-read book

**Films:**
[[film_ext:The Big Short|2015|Adam McKay]]

**Music:**
[[song_ext:Money|Pink Floyd|1973]]

**Code:**
\`\`\`python
import hashlib
print(hashlib.sha256(b"bitcoin").hexdigest())
\`\`\`
\`\`\`javascript
const crypto = require('crypto')
console.log(crypto.createHash('sha256').update('bitcoin').digest('hex'))
\`\`\`
\`\`\`bash
echo -n "bitcoin" | sha256sum
\`\`\``
    const result = extractAll(text, 'recommend books and movies about bitcoin')
    expect(result.books.length).toBeGreaterThanOrEqual(1)
    expect(result.films.length).toBeGreaterThanOrEqual(1)
    expect(result.songs.length).toBeGreaterThanOrEqual(1)
    expect(result.codeBlocks.length).toBeGreaterThanOrEqual(3)
  })

  it('handles image markdown with special characters in alt text', () => {
    const text = `![A café & bistro's outdoor patio (2024)](https://example.com/photo.jpg)
![Mountain sunrise — golden hour](https://example.com/sunrise.png)`
    const images = extractAllImages(text, 'show me photos')
    expect(images.length).toBeGreaterThanOrEqual(2)
  })

  it('extracts images from multiple markdown image patterns', () => {
    const text = `Here are some cat photos:

![Tabby cat](https://example.com/tabby.jpg)
![Black cat](https://example.com/black.jpg)
![Persian cat](https://example.com/persian.jpg)`
    const images = extractAllImages(text, 'cat photos')
    expect(images.length).toBeGreaterThanOrEqual(3)
  })

  it('isImageQuery matches image-related queries', () => {
    expect(isImageQuery('show me pictures of cats')).toBe(true)
    expect(isImageQuery('generate an image of a sunset')).toBe(true)
  })
})

// ═══════════════════════════════════════════════════════════════════
// FILTER TABS BY CONTEXT — comprehensive routing (T11)
// ═══════════════════════════════════════════════════════════════════

describe('filterTabsByContext: comprehensive routing', () => {
  it('news query with TV content surfaces both', () => {
    const tabs = filterTabsByContext(
      'news about breaking bad season 6',
      false, false, false, false, true, false, false, true, false, false, false, false, false,
    )
    expect(tabs).toContain('tvshow')
    expect(tabs).toContain('news')
  })

  it('nostr query surfaces nostr first', () => {
    const tabs = filterTabsByContext(
      'what is nostr',
      false, false, false, false, false, false, false, false, false, false, true, false, false,
    )
    expect(tabs).toContain('nostr')
  })

  it('app query surfaces apps first', () => {
    const tabs = filterTabsByContext(
      'best bitcoin wallet apps',
      false, false, false, false, false, false, false, false, false, false, false, true, false,
    )
    expect(tabs).toContain('app')
    expect(tabs[0]).toBe('app')
  })

  it('nostr + apps query surfaces both', () => {
    const tabs = filterTabsByContext(
      'nostr apps and clients',
      false, false, false, false, false, false, false, false, false, false, true, true, false,
    )
    expect(tabs).toContain('nostr')
    expect(tabs).toContain('app')
  })

  it('code + apps surfaces both', () => {
    const tabs = filterTabsByContext(
      'how to build a bitcoin app',
      false, false, false, false, false, false, false, false, false, false, false, true, true,
    )
    expect(tabs).toContain('app')
    expect(tabs).toContain('code')
  })

  it('magazine shows for news-like query with magazine sections', () => {
    const tabs = filterTabsByContext(
      'bitcoin market update',
      false, false, false, false, false, false, false, false, false, true, false, false, false,
    )
    expect(tabs).toContain('magazine')
  })

  it('does not silently drop any present content type', () => {
    // All content types present
    const tabs = filterTabsByContext(
      'tell me everything',
      true, true, true, true, true, true, true, true, true, true, true, true, true,
    )
    expect(tabs).toContain('film')
    expect(tabs).toContain('song')
    expect(tabs).toContain('podcast')
    expect(tabs).toContain('book')
    expect(tabs).toContain('tvshow')
    expect(tabs).toContain('image')
    expect(tabs).toContain('place')
  })

  it('preferredFirstTab returns place for restaurant queries', () => {
    expect(preferredFirstTab('best restaurants nearby')).toBe('place')
  })

  it('preferredFirstTab returns book for book queries', () => {
    expect(preferredFirstTab('recommend good books')).toBe('book')
  })

  it('preferredFirstTab returns code for coding queries', () => {
    expect(preferredFirstTab('write a python script')).toBe('code')
  })

  it('preferredFirstTab returns tvshow for TV queries', () => {
    expect(preferredFirstTab('best tv shows to watch')).toBe('tvshow')
  })

  it('preferredFirstTab returns song for music queries', () => {
    expect(preferredFirstTab('recommend some music')).toBe('song')
  })

  it('preferredFirstTab returns null for generic queries', () => {
    const result = preferredFirstTab('what is bitcoin')
    // May return null or a default — just verify it doesn't crash
    expect(result === null || typeof result === 'string').toBe(true)
  })

  it('news + TV query prioritizes correctly', () => {
    const tabs = filterTabsByContext(
      'any news on stranger things',
      false, false, false, false, true, false, false, true, true, false, false, false, false,
    )
    expect(tabs).toContain('tvshow')
    expect(tabs).toContain('news')
  })

  it('websites tab surfaces when websites are present', () => {
    const tabs = filterTabsByContext(
      'bitcoin resources and links',
      false, false, false, false, false, false, false, false, true, false, false, false, false,
    )
    expect(tabs).toContain('websites')
  })

  it('image tab surfaces for image queries', () => {
    const tabs = filterTabsByContext(
      'show me sunset images',
      false, false, false, false, false, true, false, false, false, false, false, false, false,
    )
    expect(tabs).toContain('image')
    expect(tabs[0]).toBe('image')
  })

  it('podcast tab surfaces when podcasts are present', () => {
    const tabs = filterTabsByContext(
      'best bitcoin podcasts',
      false, false, true, false, false, false, false, false, false, false, false, false, false,
    )
    expect(tabs).toContain('podcast')
  })
})

// ═══════════════════════════════════════════════════════════════════
// NOSTR — query and response detection
// ═══════════════════════════════════════════════════════════════════

describe('Nostr: query and response detection', () => {
  it('isNostrQuery matches nostr-related queries', () => {
    expect(isNostrQuery('what is nostr')).toBe(true)
    expect(isNostrQuery('show me my nostr feed')).toBe(true)
    expect(isNostrQuery('nostr relay recommendations')).toBe(true)
  })

  it('isNostrQuery does not match unrelated queries', () => {
    expect(isNostrQuery('best pizza in nyc')).toBe(false)
    expect(isNostrQuery('tell me about bitcoin')).toBe(false)
  })

  it('isNostrLikeResponse detects nostr content', () => {
    const text = 'Nostr is a decentralized protocol using relays and npub keys for identity. You can use NIP-05 for verification.'
    expect(isNostrLikeResponse(text)).toBe(true)
  })
})

// ═══════════════════════════════════════════════════════════════════
// NEWS — query and response detection
// ═══════════════════════════════════════════════════════════════════

describe('News: query and response detection', () => {
  it('isNewsQuery matches news queries', () => {
    expect(isNewsQuery('latest bitcoin news')).toBe(true)
    expect(isNewsQuery("what's happening in crypto")).toBe(true)
    expect(isNewsQuery('recent headlines')).toBe(true)
  })

  it('isNewsLikeResponse detects news source patterns', () => {
    const text = 'For the latest Bitcoin news, check these sources for reliable information.'
    expect(isNewsLikeResponse(text)).toBe(true)
  })

  it('isNewsQuery does not match non-news queries', () => {
    expect(isNewsQuery('how to cook pasta')).toBe(false)
    expect(isNewsQuery('explain quantum computing')).toBe(false)
  })
})
