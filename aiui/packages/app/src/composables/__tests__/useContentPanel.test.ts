import { describe, it, expect, beforeEach } from 'vitest'
import { useContentPanel } from '../useContentPanel'
import type { Film, Song, ImageItem, Place } from '@aiui/core/types/content'

describe('useContentPanel', () => {
  let panel: ReturnType<typeof useContentPanel>

  beforeEach(() => {
    panel = useContentPanel()
    panel.closePanel()
  })

  it('starts with panel closed', () => {
    expect(panel.panelOpen.value).toBe(false)
    expect(panel.availableTabs.value).toHaveLength(0)
  })

  it('setActiveTab only sets valid tabs', () => {
    panel.availableTabs.value = ['film', 'song', 'prompt']
    panel.setActiveTab('film')
    expect(panel.activeTab.value).toBe('film')
    panel.setActiveTab('song')
    expect(panel.activeTab.value).toBe('song')
    // Invalid tab should not change
    panel.setActiveTab('podcast')
    expect(panel.activeTab.value).toBe('song')
  })

  it('openFilmDetail sets selected film and clears others', () => {
    const film = { id: 'f1', title: 'Test Film', year: 2025, genres: [], director: 'Dir' } as unknown as Film
    panel.openFilmDetail(film)
    expect(panel.selectedFilm.value).toEqual(film)
    expect(panel.selectedSong.value).toBeNull()
    expect(panel.selectedBook.value).toBeNull()
  })

  it('closeFilmDetail clears selection', () => {
    const film = { id: 'f1', title: 'Test Film', year: 2025, genres: [], director: 'Dir' } as unknown as Film
    panel.openFilmDetail(film)
    panel.closeFilmDetail()
    expect(panel.selectedFilm.value).toBeNull()
  })

  it('openSongDetail clears film selection', () => {
    const film = { id: 'f1', title: 'Test Film', year: 2025, genres: [], director: 'Dir' } as unknown as Film
    const song = { id: 's1', title: 'Test Song', artist: 'Artist', album: 'Album', genres: [] } as Song
    panel.openFilmDetail(film)
    panel.openSongDetail(song)
    expect(panel.selectedSong.value).toEqual(song)
    expect(panel.selectedFilm.value).toBeNull()
  })

  it('closePanel resets all state', () => {
    const film = { id: 'f1', title: 'Test Film', year: 2025, genres: [], director: 'Dir' } as unknown as Film
    panel.openFilmDetail(film)
    panel.panelOpen.value = true
    panel.availableTabs.value = ['film', 'prompt']
    panel.closePanel()
    expect(panel.panelOpen.value).toBe(false)
    expect(panel.selectedFilm.value).toBeNull()
    expect(panel.availableTabs.value).toHaveLength(0)
  })

  it('enterDesignSystemMode sets up design system tab', () => {
    panel.enterDesignSystemMode()
    expect(panel.panelOpen.value).toBe(true)
    expect(panel.activeTab.value).toBe('design-system')
    expect(panel.availableTabs.value).toEqual(['design-system'])
    expect(panel.panelTitle.value).toBe('Design System')
  })

  it('openImageDetail and closeImageDetail work', () => {
    const image = { id: 'i1', title: 'Test Image', url: 'https://example.com/img.jpg' } as ImageItem
    panel.openImageDetail(image)
    expect(panel.selectedImage.value).toEqual(image)
    panel.closeImageDetail()
    expect(panel.selectedImage.value).toBeNull()
  })

  // 13-11 (GAP-FOUND 2026-08-03): setArchyContent must make real,
  // non-empty node content actually visible — populating panelFilms/
  // panelSongs/panelPodcasts alone left the tab bar and panelOpen
  // untouched, so real data sat in memory and never rendered.
  describe('setArchyContent makes non-empty content actually appear', () => {
    it('opens the panel and adds a song tab when songs are non-empty', () => {
      const song = { id: 's1', title: 'Song', artist: 'Artist', album: 'Album', genres: [] } as Song
      panel.setArchyContent({ songs: [song] })
      expect(panel.panelOpen.value).toBe(true)
      expect(panel.availableTabs.value).toContain('song')
      expect(panel.availableTabs.value).toContain('prompt')
      expect(panel.activeTab.value).toBe('song')
      expect(panel.panelTitle.value).toBe('Song')
    })

    it('adds a film tab and a song tab together when both are non-empty', () => {
      const film = { id: 'f1', title: 'Film', year: 2025, genres: [], director: 'Dir' } as unknown as Film
      const song = { id: 's1', title: 'Song', artist: 'Artist', album: 'Album', genres: [] } as Song
      panel.setArchyContent({ films: [film], songs: [song] })
      expect(panel.availableTabs.value).toEqual(['film', 'song', 'prompt'])
      expect(panel.activeTab.value).toBe('film')
    })

    it('does not force the panel open when the bundle is entirely empty', () => {
      panel.closePanel()
      panel.setArchyContent({ films: [], songs: [], podcasts: [] })
      expect(panel.panelOpen.value).toBe(false)
      expect(panel.availableTabs.value).toHaveLength(0)
      // An empty delivery does NOT latch: nothing was supplied, so there is
      // no node truth to protect and extraction must stay writable. The
      // honest empty state is carried by the 'Nothing found' heading, not
      // by the latch.
      expect(panel.archyContentActive.value).toBe(false)
    })

    // Regression (live on archi-dev-box 2026-08-07): "show me paid for peer
    // files" — the turn's surface carried 3 purchased images, the reply text
    // was a plain markdown list matching no extraction pattern, and
    // updatePanelFromText then set panelOpen from the regex-only tab list,
    // CLOSING the panel setArchyContent had just opened. The probe showed
    // surfaces=1 arriving; the user saw only prose.
    it('keeps the panel open on node content when the reply text infers no tabs', () => {
      const img = { id: 'i1', title: 'signal-test.jpeg', url: '/content/abc', mimeType: 'image/jpeg' } as unknown as ImageItem
      panel.setArchyContent({ images: [img] })
      expect(panel.panelOpen.value).toBe(true)
      expect(panel.activeTab.value).toBe('image')

      // The turn's prose answer arrives after the surfaces and fires the
      // text path: a bullet list with no tags, no links, no patterns.
      panel.updatePanelFromText(
        "You've purchased 4 items from peer nodes:\n\n- signal-test.jpeg (170 KB) — 100 sats\n- Photos/got it!.jpg (253 KB) — 100 sats\n\nThese are mostly photos.",
        'show me paid for peer files',
      )

      expect(panel.panelOpen.value).toBe(true)
      expect(panel.availableTabs.value[0]).toBe('image')
      expect(panel.panelImages.value).toHaveLength(1)
    })

    // Regression (operator report 2026-08-07): "recommend me 10 scifi films"
    // — the node has no films, the model's reply carried [[film_ext:…]] tags,
    // and the global archy latch (set at mount by the node's own images)
    // meant extraction could never write panelFilms: ten recommendations
    // rendered as bare prose beside an empty panel.
    it('renders extracted recommendation previews in a bucket the node left empty', () => {
      const img = { id: 'i1', title: 'photo.jpg', url: '/content/x', mimeType: 'image/jpeg' } as unknown as ImageItem
      panel.setArchyContent({ images: [img], films: [], songs: [], podcasts: [] })

      // The turn: the films scope came back empty, the reply carries tags.
      panel.beginArchyContentLoad()
      panel.setArchyContent({ films: [], songs: [], podcasts: [], images: [] })
      panel.updatePanelFromText(
        'Here are two sci-fi films:\n\n[[film_ext:Blade Runner 2049|2017|Denis Villeneuve]] — A stunning neo-noir sequel.\n\n[[film_ext:Arrival|2016|Denis Villeneuve]] — Thoughtful first contact.',
        'recommend me sci-fi films',
      )

      expect(panel.panelFilms.value.map((f) => f.title)).toEqual(['Blade Runner 2049', 'Arrival'])
      expect(panel.panelOpen.value).toBe(true)
      expect(panel.availableTabs.value[0]).toBe('film')
    })

    it('never lets extracted tags overwrite a bucket the node actually filled', () => {
      const song = { id: 's1', title: 'Real Node Song', artist: 'A', album: 'B', genres: [] } as Song
      panel.setArchyContent({ songs: [song] })

      panel.updatePanelFromText(
        'You might also like [[song_ext:Fake Song|Nobody|2020]]',
        'what music do I have',
      )

      expect(panel.panelSongs.value).toHaveLength(1)
      expect(panel.panelSongs.value[0].title).toBe('Real Node Song')
    })

    it("keeps 'Nothing found' when the node and the reply both yield nothing", () => {
      panel.closePanel()
      panel.beginArchyContentLoad()
      panel.setArchyContent({ films: [], songs: [], podcasts: [], images: [] })
      expect(panel.panelTitle.value).toBe('Nothing found')

      panel.updatePanelFromText('I could not find anything matching that on this node.', 'show me films')

      expect(panel.panelTitle.value).toBe('Nothing found')
      expect(panel.panelOpen.value).toBe(false)
    })
  })

  it('openPlaceDetail and closePlaceDetail work', () => {
    const place = { id: 'p1', name: 'Test Place', address: '123 St', lat: 0, lng: 0 } as Place
    panel.openPlaceDetail(place)
    expect(panel.selectedPlace.value).toEqual(place)
    panel.closePlaceDetail()
    expect(panel.selectedPlace.value).toBeNull()
  })
})
