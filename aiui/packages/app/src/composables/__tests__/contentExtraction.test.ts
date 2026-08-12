import { describe, it, expect } from 'vitest'
import {
  extractAllFilms,
  extractAllSongs,
  extractAllBooks,
  extractAllTVSeries,
  extractAllPlaces,
  extractAllImages,
  extractMagazineSections,
  stripContentTags,
} from '../contentExtraction'

// ─── Edge cases ──────────────────────────────────────────────────

describe('contentExtraction edge cases', () => {
  it('handles empty string', () => {
    expect(extractAllFilms('')).toEqual([])
    expect(extractAllSongs('', '')).toEqual([])
    expect(extractAllBooks('', '')).toEqual([])
    expect(extractAllTVSeries('', '')).toEqual([])
    expect(extractAllPlaces('', '')).toEqual([])
    expect(extractAllImages('', '')).toEqual([])
    expect(extractMagazineSections('')).toEqual([])
  })

  it('handles text with no tags', () => {
    const text = 'Just a regular response about movies and music.'
    expect(extractAllFilms(text)).toEqual([])
    expect(extractAllSongs(text, '')).toEqual([])
  })

  it('handles interleaved tags of different types', () => {
    const text = `
Here are some recommendations:
[[film_ext:Inception|2010|Christopher Nolan]]
Then a great song:
[[song_ext:Bohemian Rhapsody|Queen|1975]]
And a book:
[[book_ext:Dune|Frank Herbert|1965]]
`
    const films = extractAllFilms(text)
    const songs = extractAllSongs(text, '')
    const books = extractAllBooks(text, '')
    expect(films).toHaveLength(1)
    expect(films[0].title).toBe('Inception')
    expect(songs).toHaveLength(1)
    expect(songs[0].title).toBe('Bohemian Rhapsody')
    expect(books).toHaveLength(1)
    expect(books[0].title).toBe('Dune')
  })

  it('handles malformed tags (missing fields)', () => {
    // film_ext requires title|year|director — missing any means no match
    expect(extractAllFilms('[[film_ext:Inception]]')).toHaveLength(0)
    expect(extractAllFilms('[[film_ext:Inception|2010]]')).toHaveLength(0)

    // All 3 fields present — matches
    const films = extractAllFilms('[[film_ext:Inception|2010|Christopher Nolan]]')
    expect(films).toHaveLength(1)
    expect(films[0].title).toBe('Inception')
  })

  it('handles unicode content in tags', () => {
    const text = '[[film_ext:千と千尋の神隠し|2001|宮崎駿]]'
    const films = extractAllFilms(text)
    expect(films).toHaveLength(1)
    expect(films[0].title).toBe('千と千尋の神隠し')
  })

  it('handles tags with extra whitespace in pipe-separated values', () => {
    // Regex captures include leading/trailing spaces in groups
    // but the tag format requires no spaces around brackets
    const text = '[[film_ext:Inception|2010|Christopher Nolan]]'
    const films = extractAllFilms(text)
    expect(films).toHaveLength(1)
    expect(films[0].title).toBe('Inception')
    expect(films[0].director).toBe('Christopher Nolan')
  })

  it('handles duplicate tags (same title)', () => {
    const text = `
[[film_ext:Inception|2010|Christopher Nolan]]
[[film_ext:Inception|2010|Christopher Nolan]]
`
    const films = extractAllFilms(text)
    // Should deduplicate
    expect(films).toHaveLength(1)
  })

  it('handles place_ext with all fields', () => {
    const text = '[[place_ext:Sushi Nakazawa|40.7258|-74.0030|Japanese|$$$$|New York]]'
    const places = extractAllPlaces(text, '')
    expect(places).toHaveLength(1)
    expect(places[0].name).toBe('Sushi Nakazawa')
  })

  it('handles tv_ext tags', () => {
    const text = '[[tv_ext:Breaking Bad|2008|Vince Gilligan]]'
    const series = extractAllTVSeries(text, '')
    expect(series).toHaveLength(1)
    expect(series[0].title).toBe('Breaking Bad')
  })

  it('stripContentTags removes all tag types', () => {
    const text = 'Watch [[film_ext:Inception|2010|Nolan]] and listen to [[song_ext:Song|Artist|2020]]'
    const stripped = stripContentTags(text)
    expect(stripped).not.toContain('[[')
    expect(stripped).not.toContain(']]')
    expect(stripped).toContain('Watch')
    expect(stripped).toContain('and listen to')
  })

  it('handles magazine sections with bold markers', () => {
    const text = `
- **Bitcoin rallies**: Price surges 5% as ETF inflows hit record.
- **Lightning Network growth**: Capacity doubles in Q1 2026.
- **Mining difficulty**: New all-time high reached.
`
    const sections = extractMagazineSections(text)
    expect(sections.length).toBeGreaterThanOrEqual(1)
    // First section is a summary grouping the items
    const titles = sections.map(s => s.title)
    expect(titles.length).toBeGreaterThan(0)
  })
})

describe('description pairing (operator-reported: cards carried the WRONG description)', () => {
  // Verbatim shape of the Bitcoin-films answer that produced mis-paired cards.
  // The model's prose was correct; the parser paired each title with the
  // PREVIOUS line because `(?:^|\n)`-anchored patterns report m.index at the
  // newline, shifting the line window back by one line.
  const TRANSCRIPT = [
    'Here are some films about Bitcoin:',
    '',
    'Documentaries:',
    '',
    '- The Rise and Rise of Bitcoin (2014) – Early documentary following Bitcoin\'s emergence and community.',
    '- Banking on Bitcoin (2016) – Explores Bitcoin\'s origins and its potential to disrupt finance.',
    '- Cryptopia (2020) – Examines both the promise and pitfalls of crypto.',
  ].join('\n')

  it('does not caption a title with the previous item\'s description', () => {
    const series = extractAllTVSeries(TRANSCRIPT, 'recommend me some bitcoin series')
    const banking = series.find(s => /Banking on Bitcoin/i.test(s.title))
    if (banking?.synopsis) {
      expect(banking.synopsis).not.toMatch(/Early documentary/i)
      expect(banking.synopsis).toMatch(/Explores Bitcoin/i)
    }
  })

  it('does not bleed a section header into the first card of a group', () => {
    const series = extractAllTVSeries(TRANSCRIPT, 'recommend me some bitcoin series')
    for (const s of series) {
      expect(s.synopsis ?? '').not.toMatch(/Documentaries:/i)
    }
  })

  it('keeps each item paired with its OWN description', () => {
    const series = extractAllTVSeries(TRANSCRIPT, 'recommend me some bitcoin series')
    const rise = series.find(s => /Rise and Rise/i.test(s.title))
    if (rise?.synopsis) {
      expect(rise.synopsis).toMatch(/Early documentary/i)
      expect(rise.synopsis).not.toMatch(/Explores Bitcoin/i)
    }
  })
})
