/**
 * Seed Conversation Regression Tests
 *
 * Ensures each seed conversation produces at least the expected content types
 * when run through the content extraction pipeline. This is a quick smoke test
 * that verifies the extraction pipeline hasn't regressed.
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
  stripContentTags,
} from '@/composables/contentExtraction'

describe('Seed conversation regression', () => {
  for (const seed of seedPrompts) {
    it(`[${seed.id}] produces expected content types for "${seed.userQuery.slice(0, 50)}"`, () => {
      const text = seed.assistantResponse
      const query = seed.userQuery

      // Run all extractors
      const films = extractAllFilms(text)
      const songs = extractAllSongs(text, query)
      const podcasts = extractAllPodcasts(text)
      const books = extractAllBooks(text, query)
      const tvSeries = extractAllTVSeries(text, query)
      const places = extractAllPlaces(text, query)
      const images = extractAllImages(text, query)
      const codeBlocks = extractCodeBlocks(text)
      const recipes = extractRecipes(text)
      const events = extractEvents(text)

      // Validate expected counts match
      if (seed.expected.films !== undefined) expect(films.length).toBe(seed.expected.films)
      if (seed.expected.songs !== undefined) expect(songs.length).toBe(seed.expected.songs)
      if (seed.expected.podcasts !== undefined) expect(podcasts.length).toBe(seed.expected.podcasts)
      if (seed.expected.books !== undefined) expect(books.length).toBe(seed.expected.books)
      if (seed.expected.tvSeries !== undefined) expect(tvSeries.length).toBe(seed.expected.tvSeries)
      if (seed.expected.places !== undefined) expect(places.length).toBe(seed.expected.places)
      if (seed.expected.images !== undefined) expect(images.length).toBe(seed.expected.images)
      if (seed.expected.codeBlocks !== undefined) expect(codeBlocks.length).toBe(seed.expected.codeBlocks)
      if (seed.expected.recipes !== undefined) expect(recipes.length).toBe(seed.expected.recipes)
      if (seed.expected.events !== undefined) expect(events.length).toBe(seed.expected.events)

      // Verify stripping leaves no tags
      const stripped = stripContentTags(text)
      expect(stripped).not.toMatch(/\[\[[^\]]+\]\]/)
    })
  }
})
