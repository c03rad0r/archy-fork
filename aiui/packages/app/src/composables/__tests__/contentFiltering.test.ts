// Regression: operator phrasing must not be mistaken for content queries.
//
// Reported on-device 2026-08-06 — the content panel showed "Podcast
// recommendations" for a query that had nothing to do with podcasts. Both
// classifiers (this one and ChatPage's loader label) matched a bare `show`,
// which is how an operator phrases most requests to a node assistant:
// "show me my files", "show the logs", "show installed apps".
import { describe, it, expect } from 'vitest'
import { preferredFirstTab } from '../contentFiltering'

describe('preferredFirstTab — operator phrasing is not a content query', () => {
  it('does not classify "show me ..." as a podcast query', () => {
    expect(preferredFirstTab('show me my files')).not.toBe('podcast')
    expect(preferredFirstTab('show the filebrowser logs')).not.toBe('podcast')
    expect(preferredFirstTab('show installed apps')).not.toBe('podcast')
  })

  it('still classifies genuine podcast queries', () => {
    expect(preferredFirstTab('recommend me a podcast')).toBe('podcast')
    expect(preferredFirstTab('any good episode about bitcoin?')).toBe('podcast')
    // NB: a bare "listen" belongs to the song rule, which is checked
    // first — only the explicit "listen to a podcast" phrasing lands here.
    expect(preferredFirstTab('listen to a podcast about bitcoin')).toBe('podcast')
  })

  it('keeps tv-show detection, which needs the two-word form', () => {
    expect(preferredFirstTab('best tv show to binge')).toBe('tvshow')
  })

  it('still classifies films and music', () => {
    expect(preferredFirstTab('recommend me 10 scifi films')).toBe('film')
    expect(preferredFirstTab('good album for working')).toBe('song')
  })
})
