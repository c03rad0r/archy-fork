import { describe, it, expect } from 'vitest'
import {
  extractAllFilms,
  extractAllSongs,
  extractAllPodcasts,
  extractAllBooks,
  extractAllTVSeries,
  extractAllPlaces,
  extractAllImages,
  extractMagazineSections,
  extractMagazineHeroImage,
  extractBoldDomainLinks,
  extractMarkdownLinks,
  mergeNewsResults,
  stripContentTags,
  stripFilmTags,
  stripSongTags,
  stripPodcastTags,
  stripBookTags,
  stripTVTags,
  stripPlaceTags,
  extractFilmIds,
  extractSongIds,
  extractPodcastIds,
} from '@/composables/contentExtraction'

// ─── Films ────────────────────────────────────────────────────────

describe('extractAllFilms', () => {
  it('extracts film_ext tags with title, year, director', () => {
    const text = 'Check out [[film_ext:Inception|2010|Christopher Nolan]]'
    const films = extractAllFilms(text)
    expect(films).toHaveLength(1)
    expect(films[0].title).toBe('Inception')
    expect(films[0].year).toBe(2010)
    expect(films[0].director).toBe('Christopher Nolan')
  })

  it('extracts film:id tags and looks up from library', () => {
    const text = 'You should watch [[film:f1]]'
    const films = extractAllFilms(text)
    expect(films).toHaveLength(1)
    expect(films[0].id).toBe('f1')
  })

  it('returns empty array for text with no film tags', () => {
    const text = 'Just some text about movies without any tags'
    const films = extractAllFilms(text)
    expect(films).toHaveLength(0)
  })

  it('handles multiple films in one message', () => {
    const text = '[[film_ext:Inception|2010|Christopher Nolan]] and [[film_ext:Interstellar|2014|Christopher Nolan]]'
    const films = extractAllFilms(text)
    expect(films).toHaveLength(2)
    expect(films[0].title).toBe('Inception')
    expect(films[1].title).toBe('Interstellar')
  })

  it('handles malformed tags gracefully', () => {
    const text = '[[film_ext:]] [[film_ext:Incomplete]] [[film:]]'
    const films = extractAllFilms(text)
    expect(films).toHaveLength(0)
  })

  it('deduplicates films by title and year', () => {
    const text = '[[film_ext:Inception|2010|Nolan]] and again [[film_ext:Inception|2010|Nolan]]'
    const films = extractAllFilms(text)
    expect(films).toHaveLength(1)
  })

  it('normalizes film IDs with or without f prefix', () => {
    const ids1 = extractFilmIds('[[film:1]] [[film:f2]]')
    expect(ids1).toEqual(['f1', 'f2'])
  })
})

// ─── Songs ────────────────────────────────────────────────────────

describe('extractAllSongs', () => {
  it('extracts song_ext tags with title, artist, year', () => {
    const text = '[[song_ext:Bohemian Rhapsody|Queen|1975]]'
    const songs = extractAllSongs(text)
    expect(songs).toHaveLength(1)
    expect(songs[0].title).toBe('Bohemian Rhapsody')
    expect(songs[0].artist).toBe('Queen')
    expect(songs[0].year).toBe(1975)
  })

  it('extracts song_ext without year', () => {
    const text = '[[song_ext:Paranoid Android|Radiohead]]'
    const songs = extractAllSongs(text)
    expect(songs).toHaveLength(1)
    expect(songs[0].title).toBe('Paranoid Android')
    expect(songs[0].artist).toBe('Radiohead')
    expect(songs[0].year).toBeUndefined()
  })

  it('extracts song:id tags from library', () => {
    const text = 'Listen to [[song:s1]]'
    const songs = extractAllSongs(text)
    expect(songs).toHaveLength(1)
    expect(songs[0].id).toBe('s1')
  })

  it('deduplicates songs by title+artist', () => {
    const text = '[[song_ext:Creep|Radiohead]] again [[song_ext:Creep|Radiohead]]'
    const songs = extractAllSongs(text)
    expect(songs).toHaveLength(1)
  })

  it('returns empty for text with film tags (not music)', () => {
    const text = '[[film_ext:Inception|2010|Nolan]] great movie'
    const songs = extractAllSongs(text)
    expect(songs).toHaveLength(0)
  })

  it('filters out non-song content using looksLikeSong', () => {
    const text = '[[song_ext:Latest News|Web Search]]'
    const songs = extractAllSongs(text)
    expect(songs).toHaveLength(0)
  })

  it('normalizes song IDs with or without s prefix', () => {
    const ids = extractSongIds('[[song:1]] [[song:s2]]')
    expect(ids).toEqual(['s1', 's2'])
  })
})

// ─── Podcasts ─────────────────────────────────────────────────────

describe('extractAllPodcasts', () => {
  it('extracts podcast_ext tags', () => {
    const text = '[[podcast_ext:Bitcoin Audible|Guy Swann|2018]]'
    const podcasts = extractAllPodcasts(text)
    expect(podcasts).toHaveLength(1)
    expect(podcasts[0].title).toBe('Bitcoin Audible')
    expect(podcasts[0].host).toBe('Guy Swann')
    expect(podcasts[0].year).toBe(2018)
  })

  it('extracts podcast:id from library', () => {
    const text = '[[podcast:p1]]'
    const podcasts = extractAllPodcasts(text)
    expect(podcasts).toHaveLength(1)
    expect(podcasts[0].id).toBe('p1')
  })

  it('returns empty for no podcast tags', () => {
    const text = 'No podcasts here'
    const podcasts = extractAllPodcasts(text)
    expect(podcasts).toHaveLength(0)
  })

  it('filters out non-podcast titles', () => {
    const text = '[[podcast_ext:Bitcoin Mailing List|GitHub]]'
    const podcasts = extractAllPodcasts(text)
    expect(podcasts).toHaveLength(0)
  })

  it('normalizes podcast IDs', () => {
    const ids = extractPodcastIds('[[podcast:1]] [[podcast:p2]]')
    expect(ids).toEqual(['p1', 'p2'])
  })
})

// ─── Books ────────────────────────────────────────────────────────

describe('extractAllBooks', () => {
  it('extracts book_ext tags with title, author, year', () => {
    const text = '[[book_ext:The Bitcoin Standard|Saifedean Ammous|2018]]'
    const books = extractAllBooks(text, 'best bitcoin books')
    expect(books).toHaveLength(1)
    expect(books[0].title).toBe('The Bitcoin Standard')
    expect(books[0].author).toBe('Saifedean Ammous')
    expect(books[0].year).toBe(2018)
  })

  it('handles optional year field', () => {
    const text = '[[book_ext:Mastering Bitcoin|Andreas Antonopoulos]]'
    const books = extractAllBooks(text, 'bitcoin books')
    expect(books).toHaveLength(1)
    expect(books[0].year).toBeUndefined()
  })

  it('returns empty when no book tags and not book query', () => {
    const text = 'Just talking about tech'
    const books = extractAllBooks(text, 'what is javascript')
    expect(books).toHaveLength(0)
  })

  it('extracts multiple books', () => {
    const text = '[[book_ext:Book One|Author A|2020]] and [[book_ext:Book Two|Author B|2021]]'
    const books = extractAllBooks(text, 'books')
    expect(books).toHaveLength(2)
  })
})

// ─── TV Series ────────────────────────────────────────────────────

describe('extractAllTVSeries', () => {
  it('extracts tv_ext tags', () => {
    const text = '[[tv_ext:Breaking Bad|Vince Gilligan|2008]]'
    const series = extractAllTVSeries(text, 'best tv shows')
    expect(series).toHaveLength(1)
    expect(series[0].title).toBe('Breaking Bad')
    expect(series[0].creator).toBe('Vince Gilligan')
    expect(series[0].year).toBe(2008)
  })

  it('parses creator field', () => {
    const text = '[[tv_ext:The Wire|David Simon|2002]]'
    const series = extractAllTVSeries(text, 'tv shows')
    expect(series).toHaveLength(1)
    expect(series[0].creator).toBe('David Simon')
  })

  it('returns empty when no TV tags and not TV query', () => {
    const text = 'Some random text'
    const series = extractAllTVSeries(text, 'cooking recipe')
    expect(series).toHaveLength(0)
  })

  it('handles tv_ext without year', () => {
    const text = '[[tv_ext:The Sopranos|David Chase]]'
    const series = extractAllTVSeries(text, 'tv')
    expect(series).toHaveLength(1)
    expect(series[0].year).toBeUndefined()
  })
})

// ─── Places ───────────────────────────────────────────────────────

describe('extractAllPlaces', () => {
  it('extracts place_ext tags with all fields', () => {
    const text = '[[place_ext:Joe\'s Pizza|Pizza|New York|4.5|2|123 Main St]]'
    const places = extractAllPlaces(text, 'pizza near me')
    expect(places).toHaveLength(1)
    expect(places[0].name).toBe("Joe's Pizza")
    expect(places[0].cuisine).toBe('Pizza')
    expect(places[0].city).toBe('New York')
    expect(places[0].rating).toBe(4.5)
    expect(places[0].priceLevel).toBe(2)
    expect(places[0].address).toBe('123 Main St')
  })

  it('handles missing optional fields (rating, price)', () => {
    const text = '[[place_ext:The Diner|American]]'
    const places = extractAllPlaces(text, 'restaurants')
    expect(places).toHaveLength(1)
    expect(places[0].name).toBe('The Diner')
    expect(places[0].cuisine).toBe('American')
    expect(places[0].rating).toBeUndefined()
    expect(places[0].priceLevel).toBeUndefined()
  })

  it('returns empty for non-place queries without tags', () => {
    const text = 'Nothing about restaurants here'
    const places = extractAllPlaces(text, 'what is bitcoin')
    expect(places).toHaveLength(0)
  })
})

// ─── Images ───────────────────────────────────────────────────────

describe('extractAllImages', () => {
  it('extracts markdown image syntax', () => {
    const text = '![alt text](https://example.com/image.jpg)'
    const images = extractAllImages(text, 'show me images')
    expect(images).toHaveLength(1)
    expect(images[0].url).toBe('https://example.com/image.jpg')
    expect(images[0].alt).toBe('alt text')
  })

  it('extracts bare image URLs', () => {
    const text = 'Check this: https://example.com/photo.png and https://example.com/pic.webp'
    const images = extractAllImages(text, 'images')
    expect(images).toHaveLength(2)
  })

  it('returns empty when not an image query and only one image', () => {
    const text = 'https://example.com/photo.jpg'
    const images = extractAllImages(text, 'what is bitcoin')
    expect(images).toHaveLength(0)
  })
})

// ─── Magazine Sections ────────────────────────────────────────────

describe('extractMagazineSections', () => {
  it('extracts sections from markdown headings', () => {
    const text = `## First Section
This is the content of the first section with enough text to pass the minimum length filter.

## Second Section
This is the content of the second section, also with enough detail to be meaningful.`

    const sections = extractMagazineSections(text)
    expect(sections.length).toBeGreaterThanOrEqual(2)
    const titles = sections.map(s => s.title)
    expect(titles).toContain('First Section')
    expect(titles).toContain('Second Section')
  })

  it('captures content between headings', () => {
    const text = `## Market Update
Bitcoin surged to new highs today as institutional demand increased significantly and retail sentiment improved.

## Analysis
Analysts believe the trend will continue through the end of the quarter as macro conditions stabilize.`

    const sections = extractMagazineSections(text)
    const marketSection = sections.find(s => s.title === 'Market Update')
    expect(marketSection).toBeDefined()
    expect(marketSection!.content).toContain('Bitcoin surged')
  })

  it('extracts hero images', () => {
    const text = 'Some text with ![hero](https://example.com/hero.jpg) embedded'
    const hero = extractMagazineHeroImage(text)
    expect(hero).toBe('https://example.com/hero.jpg')
  })

  it('returns undefined for no images', () => {
    const hero = extractMagazineHeroImage('No images here')
    expect(hero).toBeUndefined()
  })

  it('extracts sections from numbered lists with bold titles', () => {
    const text = `Here are the key developments:

1. **Strong price recovery** — Bitcoin climbed back above $60,000 as market confidence returned.
2. **Institutional adoption grows** — Major banks announced new crypto custody services for their clients.
3. **Regulatory clarity emerges** — New framework provides guidelines for digital asset companies.`

    const sections = extractMagazineSections(text)
    expect(sections.length).toBeGreaterThanOrEqual(3)
    const titles = sections.map(s => s.title)
    expect(titles).toContain('Strong price recovery')
  })

  it('handles empty or short text', () => {
    const sections = extractMagazineSections('')
    expect(sections).toHaveLength(0)
  })
})

// ─── Tag Stripping ────────────────────────────────────────────────

describe('stripContentTags', () => {
  it('removes all tag types from text', () => {
    const text = 'Watch [[film:f1]] and listen to [[song:s1]] and read [[book_ext:Title|Author|2020]]'
    const stripped = stripContentTags(text)
    expect(stripped).not.toContain('[[film:')
    expect(stripped).not.toContain('[[song:')
    expect(stripped).not.toContain('[[book_ext:')
  })

  it('preserves non-tag content', () => {
    const text = 'Watch this great movie [[film:f1]] and enjoy'
    const stripped = stripContentTags(text)
    expect(stripped).toContain('Watch this great movie')
    expect(stripped).toContain('and enjoy')
  })

  it('handles adjacent tags', () => {
    const text = '[[film:f1]][[song:s1]][[podcast:p1]]'
    const stripped = stripContentTags(text)
    expect(stripped).toBe('')
  })

  it('strips all specific tag types individually', () => {
    expect(stripFilmTags('[[film:f1]] [[film_ext:Title|2020|Dir]]')).toBe('')
    expect(stripSongTags('[[song:s1]] [[song_ext:Title|Artist]]')).toBe('')
    expect(stripPodcastTags('[[podcast:p1]] [[podcast_ext:Title|Host]]')).toBe('')
    expect(stripBookTags('[[book_ext:Title|Author|2020]]')).toBe('')
    expect(stripTVTags('[[tv_ext:Title|Creator|2020]]')).toBe('')
    expect(stripPlaceTags('[[place_ext:Name|Cuisine]]')).toBe('')
  })
})

// ─── Links Extraction ─────────────────────────────────────────────

describe('extractBoldDomainLinks', () => {
  it('extracts **domain.com** patterns with URLs', () => {
    const text = '**CoinDesk** (coindesk.com) — the best source'
    const links = extractBoldDomainLinks(text)
    expect(links).toHaveLength(1)
    expect(links[0].title).toBe('CoinDesk')
    expect(links[0].url).toBe('https://coindesk.com')
  })

  it('deduplicates URLs', () => {
    const text = '**CoinDesk** (coindesk.com) and **CoinDesk News** (coindesk.com)'
    const links = extractBoldDomainLinks(text)
    expect(links).toHaveLength(1)
  })
})

describe('extractMarkdownLinks', () => {
  it('extracts markdown links', () => {
    const text = 'Check out [Bitcoin](https://bitcoin.org) for more'
    const links = extractMarkdownLinks(text)
    expect(links).toHaveLength(1)
    expect(links[0].title).toBe('Bitcoin')
    expect(links[0].url).toBe('https://bitcoin.org')
  })

  it('handles multiple links', () => {
    const text = '[Link 1](https://example.com) and [Link 2](https://example.org/page)'
    const links = extractMarkdownLinks(text)
    expect(links).toHaveLength(2)
  })

  it('skips invalid URLs', () => {
    const text = '[Bad Link](not-a-url)'
    const links = extractMarkdownLinks(text)
    expect(links).toHaveLength(0)
  })
})

describe('mergeNewsResults', () => {
  it('merges web results with text-extracted results', () => {
    const web = [{ title: 'Web A', url: 'https://a.com', content: 'content a' }]
    const fromText = [
      { title: 'Text B', url: 'https://b.com', content: undefined },
      { title: 'Text A Dup', url: 'https://a.com', content: undefined },
    ]
    const merged = mergeNewsResults(web, fromText)
    expect(merged).toHaveLength(2)
    // Web result takes priority for same URL
    const aResult = merged.find(r => r.url.includes('a.com'))
    expect(aResult!.title).toBe('Web A')
  })
})
