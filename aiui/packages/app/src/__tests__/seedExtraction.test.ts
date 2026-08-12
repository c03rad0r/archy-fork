/**
 * Seed Extraction Tests
 *
 * Validates that every seed prompt in the prompt index extracts the expected
 * content types and counts. These are the gold-standard test cases — if any
 * fail, the content surfacing pipeline has regressed.
 *
 * Run overnight to harden extraction patterns against real-world AI responses.
 */
import { describe, it, expect } from 'vitest'
import { seedPrompts } from './fixtures/seedPrompts'
import {
  extractAllFilms,
  extractAllSongs,
  extractAllPodcasts,
  extractAllBooks,
  extractAllTVSeries,
  extractAllPlaces,
  extractAllImages,
  extractCodeBlocks,
  extractRecipes,
  extractEvents,
  extractMagazineSections,
  stripContentTags,
  stripRecipeTags,
  stripEventTags,
} from '@/composables/contentExtraction'

// ─── Extraction count validation ─────────────────────────────

describe('Seed prompt extraction', () => {
  for (const seed of seedPrompts) {
    describe(`[${seed.id}] "${seed.userQuery}"`, () => {
      const text = seed.assistantResponse
      const query = seed.userQuery

      if (seed.expected.films !== undefined) {
        it(`extracts ${seed.expected.films} films`, () => {
          const films = extractAllFilms(text)
          expect(films.length).toBe(seed.expected.films)
          for (const f of films) {
            expect(f.title).toBeTruthy()
          }
        })
      }

      if (seed.expected.songs !== undefined) {
        it(`extracts ${seed.expected.songs} songs`, () => {
          const songs = extractAllSongs(text, query)
          expect(songs.length).toBe(seed.expected.songs)
          for (const s of songs) {
            expect(s.title).toBeTruthy()
            expect(s.artist).toBeTruthy()
          }
        })
      }

      if (seed.expected.books !== undefined) {
        it(`extracts ${seed.expected.books} books`, () => {
          const books = extractAllBooks(text, query)
          expect(books.length).toBe(seed.expected.books)
          for (const b of books) {
            expect(b.title).toBeTruthy()
          }
        })
      }

      if (seed.expected.tvSeries !== undefined) {
        it(`extracts ${seed.expected.tvSeries} TV series`, () => {
          const tv = extractAllTVSeries(text, query)
          expect(tv.length).toBe(seed.expected.tvSeries)
          for (const t of tv) {
            expect(t.title).toBeTruthy()
          }
        })
      }

      if (seed.expected.places !== undefined) {
        it(`extracts ${seed.expected.places} places`, () => {
          const places = extractAllPlaces(text, query)
          expect(places.length).toBe(seed.expected.places)
          for (const p of places) {
            expect(p.name).toBeTruthy()
          }
        })
      }

      if (seed.expected.podcasts !== undefined) {
        it(`extracts ${seed.expected.podcasts} podcasts`, () => {
          const podcasts = extractAllPodcasts(text)
          expect(podcasts.length).toBe(seed.expected.podcasts)
          for (const p of podcasts) {
            expect(p.title).toBeTruthy()
          }
        })
      }

      if (seed.expected.images !== undefined) {
        it(`extracts ${seed.expected.images} images`, () => {
          const images = extractAllImages(text, query)
          expect(images.length).toBe(seed.expected.images)
        })
      }

      if (seed.expected.codeBlocks !== undefined) {
        it(`extracts ${seed.expected.codeBlocks} code blocks`, () => {
          const code = extractCodeBlocks(text)
          expect(code.length).toBe(seed.expected.codeBlocks)
          for (const c of code) {
            expect(c.code.trim()).toBeTruthy()
          }
        })
      }

      if (seed.expected.recipes !== undefined) {
        it(`extracts ${seed.expected.recipes} recipes`, () => {
          const recipes = extractRecipes(text)
          expect(recipes.length).toBe(seed.expected.recipes)
          for (const r of recipes) {
            expect(r.title).toBeTruthy()
            expect(r.ingredients.length).toBeGreaterThan(0)
            expect(r.steps.length).toBeGreaterThan(0)
          }
        })
      }

      if (seed.expected.events !== undefined) {
        it(`extracts ${seed.expected.events} events`, () => {
          const events = extractEvents(text)
          expect(events.length).toBe(seed.expected.events)
          for (const e of events) {
            expect(e.title).toBeTruthy()
          }
        })
      }

      if (seed.expected.magazineSections !== undefined) {
        it(`extracts ${seed.expected.magazineSections} magazine sections`, () => {
          const sections = extractMagazineSections(text)
          expect(sections.length).toBeGreaterThanOrEqual(seed.expected.magazineSections!)
        })
      }
    })
  }
})

// ─── Tag stripping — no tags leak into displayed content ──────

describe('Tag stripping completeness', () => {
  for (const seed of seedPrompts) {
    it(`[${seed.id}] stripContentTags removes all bracket tags`, () => {
      const cleaned = stripContentTags(seed.assistantResponse)
      // No [[...]] bracket tags should remain
      const bracketMatches = cleaned.match(/\[\[[^\]]+\]\]/g)
      expect(bracketMatches).toBeNull()
    })

    it(`[${seed.id}] strip functions remove all XML tags`, () => {
      let cleaned = stripRecipeTags(stripEventTags(seed.assistantResponse))
      cleaned = stripContentTags(cleaned)
      // No <..._ext> XML tags should remain
      const xmlMatches = cleaned.match(/<\/?(?:recipe|event)_ext[^>]*>/g)
      expect(xmlMatches).toBeNull()
    })
  }
})

// ─── Data integrity — extracted content has required fields ───

describe('Extraction data integrity', () => {
  const filmSeed = seedPrompts.find(s => s.id === 'seed-films')!
  it('films have title, year, and director', () => {
    const films = extractAllFilms(filmSeed.assistantResponse)
    for (const f of films) {
      expect(f.title).toBeTruthy()
      expect(f.year).toBeGreaterThan(1900)
      expect(f.director).toBeTruthy()
    }
  })

  const songSeed = seedPrompts.find(s => s.id === 'seed-songs')!
  it('songs have title, artist, and year', () => {
    const songs = extractAllSongs(songSeed.assistantResponse, songSeed.userQuery)
    for (const s of songs) {
      expect(s.title).toBeTruthy()
      expect(s.artist).toBeTruthy()
    }
  })

  const bookSeed = seedPrompts.find(s => s.id === 'seed-books')!
  it('books have title and author', () => {
    const books = extractAllBooks(bookSeed.assistantResponse, bookSeed.userQuery)
    for (const b of books) {
      expect(b.title).toBeTruthy()
      expect(b.author).toBeTruthy()
    }
  })

  const tvSeed = seedPrompts.find(s => s.id === 'seed-tv')!
  it('TV series have title and creator', () => {
    const tv = extractAllTVSeries(tvSeed.assistantResponse, tvSeed.userQuery)
    for (const t of tv) {
      expect(t.title).toBeTruthy()
      expect(t.creator).toBeTruthy()
    }
  })

  const placeSeed = seedPrompts.find(s => s.id === 'seed-places')!
  it('places have name, cuisine, and city', () => {
    const places = extractAllPlaces(placeSeed.assistantResponse, placeSeed.userQuery)
    for (const p of places) {
      expect(p.name).toBeTruthy()
      expect(p.cuisine).toBeTruthy()
      expect(p.city).toBeTruthy()
    }
  })

  const recipeSeed = seedPrompts.find(s => s.id === 'seed-recipes')!
  it('recipes have complete data', () => {
    const recipes = extractRecipes(recipeSeed.assistantResponse)
    expect(recipes.length).toBe(1)
    const r = recipes[0]
    expect(r.title).toBe('Spaghetti alla Carbonara')
    expect(r.servings).toBe('4')
    expect(r.time).toBe('25 min')
    expect(r.ingredients.length).toBeGreaterThanOrEqual(4)
    expect(r.steps.length).toBeGreaterThanOrEqual(5)
  })

  const eventSeed = seedPrompts.find(s => s.id === 'seed-events')!
  it('events have title, date, and location', () => {
    const events = extractEvents(eventSeed.assistantResponse)
    for (const e of events) {
      expect(e.title).toBeTruthy()
      expect(e.date).toBeTruthy()
      expect(e.location).toBeTruthy()
    }
  })
})
