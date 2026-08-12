import { describe, it, expect } from 'vitest'
import {
  adaptContentItems,
  adaptToFilm,
  adaptToPodcast,
  adaptLibraryTracks,
  adaptLibraryAlbums,
  classifyByMime,
  sortDeterministic,
  type ArchyContentItem,
  type ArchyLibraryTrack,
} from '../archyContentAdapter'

function item(overrides: Partial<ArchyContentItem>): ArchyContentItem {
  return {
    id: 'id-1',
    filename: 'file.bin',
    mime_type: 'application/octet-stream',
    size_bytes: 1024,
    ...overrides,
  }
}

describe('classifyByMime', () => {
  it('classifies a video mime as video', () => {
    expect(classifyByMime(item({ mime_type: 'video/mp4', filename: 'movie.mp4' }))).toBe('video')
  })

  it('classifies an audio mime as audio', () => {
    expect(classifyByMime(item({ mime_type: 'audio/mpeg', filename: 'song.mp3' }))).toBe('audio')
  })

  it('classifies images as image, and still excludes documents', () => {
    expect(classifyByMime(item({ mime_type: 'image/jpeg', filename: 'photo.jpg' }))).toBe('image')
    expect(classifyByMime(item({ mime_type: 'application/pdf', filename: 'doc.pdf' }))).toBe('excluded')
  })

  it('classifies images by extension when the mime is generic', () => {
    // A node whose catalog is mostly photos shared with an unidentified
    // mime must not present as empty.
    expect(classifyByMime(item({ mime_type: 'application/octet-stream', filename: 'p.webp' }))).toBe('image')
    expect(classifyByMime(item({ mime_type: 'application/octet-stream', filename: 'p.heic' }))).toBe('image')
  })

  it('classifies m4a, aac, opus and wma as audio via extension fallback (ShareModal blind spot)', () => {
    expect(classifyByMime(item({ mime_type: 'application/octet-stream', filename: 'track.m4a' }))).toBe('audio')
    expect(classifyByMime(item({ mime_type: 'application/octet-stream', filename: 'track.aac' }))).toBe('audio')
    expect(classifyByMime(item({ mime_type: 'application/octet-stream', filename: 'track.opus' }))).toBe('audio')
    expect(classifyByMime(item({ mime_type: 'application/octet-stream', filename: 'track.wma' }))).toBe('audio')
  })

  it('also classifies the correct audio/* mime for those four extensions', () => {
    expect(classifyByMime(item({ mime_type: 'audio/mp4', filename: 'track.m4a' }))).toBe('audio')
    expect(classifyByMime(item({ mime_type: 'audio/aac', filename: 'track.aac' }))).toBe('audio')
    expect(classifyByMime(item({ mime_type: 'audio/opus', filename: 'track.opus' }))).toBe('audio')
    expect(classifyByMime(item({ mime_type: 'audio/x-ms-wma', filename: 'track.wma' }))).toBe('audio')
  })
})

describe('adaptContentItems', () => {
  it('maps a video-mime item to a Film with id carried through and one source entry', () => {
    const bundle = adaptContentItems(
      [item({ id: 'film-1', filename: 'The Movie.mp4', mime_type: 'video/mp4', added_at: '2026-01-01T00:00:00Z' })],
      { source: 'own' },
    )
    expect(bundle.films).toHaveLength(1)
    expect(bundle.films[0]!.id).toBe('film-1')
    expect(bundle.films[0]!.title).toBe('The Movie')
    expect(bundle.films[0]!.sources).toHaveLength(1)
    expect(bundle.songs).toHaveLength(0)
    expect(bundle.podcasts).toHaveLength(0)
  })

  it('maps an audio-mime item to a Song', () => {
    const bundle = adaptContentItems(
      [item({ id: 'song-1', filename: 'Track.mp3', mime_type: 'audio/mpeg', added_at: '2026-01-01T00:00:00Z' })],
      { source: 'own' },
    )
    expect(bundle.songs).toHaveLength(1)
    expect(bundle.songs[0]!.id).toBe('song-1')
    expect(bundle.films).toHaveLength(0)
  })

  it('routes images to the images bucket and still excludes documents', () => {
    const bundle = adaptContentItems(
      [
        item({ id: 'img-1', filename: 'photo.jpg', mime_type: 'image/jpeg' }),
        item({ id: 'doc-1', filename: 'report.pdf', mime_type: 'application/pdf' }),
      ],
      { source: 'own' },
    )
    expect(bundle.films).toHaveLength(0)
    expect(bundle.songs).toHaveLength(0)
    expect(bundle.podcasts).toHaveLength(0)
    // The photo renders; the PDF has no grid, so it stays out of every bucket.
    expect(bundle.images).toHaveLength(1)
    expect(bundle.images[0]!.id).toBe('img-1')
    expect(bundle.images[0]!.url).toBe('/content/img-1')
  })

  it('never locks an OWN paid image: the node serves the authenticated owner (owner-bypass), price stays as a badge', () => {
    const bundle = adaptContentItems(
      [item({ id: 'p-1', filename: 'photo.jpg', mime_type: 'image/jpeg', access: { paid: { price_sats: 100 } } })],
      { source: 'own' },
    )
    expect(bundle.images[0]!.locked).toBe(false)
    expect(bundle.images[0]!.priceSats).toBe(100)
    expect(bundle.images[0]!.url).toBe('/content/p-1')
  })

  it('locks a PEER paid image: price carried, no URL to fetch bytes the user has not bought', () => {
    const bundle = adaptContentItems(
      [item({ id: 'p-1', filename: 'photo.jpg', mime_type: 'image/jpeg', access: { paid: { price_sats: 100 } } })],
      { source: 'peer', peerOnion: 'seller.onion' },
    )
    expect(bundle.images[0]!.locked).toBe(true)
    expect(bundle.images[0]!.priceSats).toBe(100)
    expect(bundle.images[0]!.url).toBe('')
  })

  it('maps an access:Paid item with a price and a locked flag, and no playable source URL', () => {
    const bundle = adaptContentItems(
      [
        item({
          id: 'paid-1',
          filename: 'premium.mp4',
          mime_type: 'video/mp4',
          access: { paid: { price_sats: 5000 } },
        }),
      ],
      { source: 'peer', peerOnion: 'abc123.onion' },
    )
    const film = bundle.films[0]!
    expect(film.locked).toBe(true)
    expect(film.priceSats).toBe(5000)
    expect(film.sources[0]!.url).toBe('')
  })

  it('gives two items with identical filename and size but different id two distinct cards (adjacency)', () => {
    const bundle = adaptContentItems(
      [
        item({ id: 'peer-a', filename: 'same.mp4', mime_type: 'video/mp4', size_bytes: 500, added_at: '2026-01-01T00:00:00Z' }),
        item({ id: 'peer-b', filename: 'same.mp4', mime_type: 'video/mp4', size_bytes: 500, added_at: '2026-01-01T00:00:00Z' }),
      ],
      { source: 'peer', peerOnion: 'peer.onion' },
    )
    expect(bundle.films).toHaveLength(2)
    const ids = bundle.films.map((f) => f.id)
    expect(new Set(ids).size).toBe(2)
    expect(ids).toContain('peer-a')
    expect(ids).toContain('peer-b')
  })

  it('an item present both in own library and a peer share appears once per source (adjacency, cross-source)', () => {
    const own = adaptContentItems(
      [item({ id: 'shared-item', filename: 'clip.mp4', mime_type: 'video/mp4', size_bytes: 100, added_at: '2026-01-01T00:00:00Z' })],
      { source: 'own' },
    )
    const peer = adaptContentItems(
      [item({ id: 'shared-item', filename: 'clip.mp4', mime_type: 'video/mp4', size_bytes: 100, added_at: '2026-01-01T00:00:00Z' })],
      { source: 'peer', peerOnion: 'peer.onion' },
    )
    // Each source's bundle carries its own single card for the id — the
    // broker (Task 2) is responsible for not silently merging bundles from
    // different sources into one deduplicated list.
    expect(own.films).toHaveLength(1)
    expect(peer.films).toHaveLength(1)
    expect(own.films[0]!.sources[0]!.type).not.toBe(peer.films[0]!.sources[0]!.type)
  })

  it('an empty input array produces empty films/songs/podcasts arrays, not undefined or an error', () => {
    const bundle = adaptContentItems([], { source: 'own' })
    expect(bundle).toEqual({ films: [], songs: [], podcasts: [], images: [] })
  })

  it('handles null/undefined input the same as an empty array', () => {
    expect(adaptContentItems(null, { source: 'own' })).toEqual({ films: [], songs: [], podcasts: [], images: [] })
    expect(adaptContentItems(undefined, { source: 'own' })).toEqual({ films: [], songs: [], podcasts: [], images: [] })
  })

  it('maps a null/absent description to an empty string, never the literal "null"', () => {
    const withNull = adaptContentItems(
      [item({ id: 'f1', filename: 'a.mp4', mime_type: 'video/mp4', description: null })],
      { source: 'own' },
    )
    const withAbsent = adaptContentItems(
      [item({ id: 'f2', filename: 'b.mp4', mime_type: 'video/mp4' })],
      { source: 'own' },
    )
    expect(withNull.films[0]!.synopsis).toBe('')
    expect(withAbsent.films[0]!.synopsis).toBe('')
  })

  it('sorts added_at descending with id ascending as the deterministic tiebreak', () => {
    const bundle = adaptContentItems(
      [
        item({ id: 'z', filename: 'a.mp4', mime_type: 'video/mp4', added_at: '2026-01-01T00:00:00Z' }),
        item({ id: 'a', filename: 'b.mp4', mime_type: 'video/mp4', added_at: '2026-01-01T00:00:00Z' }),
        item({ id: 'm', filename: 'c.mp4', mime_type: 'video/mp4', added_at: '2026-02-01T00:00:00Z' }),
      ],
      { source: 'own' },
    )
    // Newest added_at first (m), then the 2026-01-01 pair tie-broken by id ascending (a, z).
    expect(bundle.films.map((f) => f.id)).toEqual(['m', 'a', 'z'])
  })

  it('a null added_at sorts last rather than crashing the comparator', () => {
    const bundle = adaptContentItems(
      [
        item({ id: 'has-date', filename: 'a.mp4', mime_type: 'video/mp4', added_at: '2026-01-01T00:00:00Z' }),
        item({ id: 'no-date', filename: 'b.mp4', mime_type: 'video/mp4', added_at: null }),
      ],
      { source: 'own' },
    )
    expect(bundle.films.map((f) => f.id)).toEqual(['has-date', 'no-date'])
  })

  it('produces identical output order regardless of input array order (repeat-call stability)', () => {
    const items = [
      item({ id: 'a', filename: 'a.mp4', mime_type: 'video/mp4', added_at: '2026-01-01T00:00:00Z' }),
      item({ id: 'b', filename: 'b.mp4', mime_type: 'video/mp4', added_at: '2026-02-01T00:00:00Z' }),
      item({ id: 'c', filename: 'c.mp4', mime_type: 'video/mp4', added_at: '2026-01-15T00:00:00Z' }),
    ]
    const first = adaptContentItems(items, { source: 'own' })
    const second = adaptContentItems([...items].reverse(), { source: 'own' })
    expect(first.films.map((f) => f.id)).toEqual(second.films.map((f) => f.id))
  })

  it('never produces a URL carrying a credential as a query parameter', () => {
    const bundle = adaptContentItems(
      [
        item({ id: 'own-1', filename: 'a.mp4', mime_type: 'video/mp4' }),
        item({ id: 'song-1', filename: 'b.mp3', mime_type: 'audio/mpeg' }),
      ],
      { source: 'own' },
    )
    const peerBundle = adaptContentItems(
      [item({ id: 'peer-1', filename: 'c.mp4', mime_type: 'video/mp4' })],
      { source: 'peer', peerOnion: 'somepeer.onion' },
    )
    const allUrls = [
      ...bundle.films.flatMap((f) => f.sources.map((s) => s.url)),
      ...bundle.songs.flatMap((s) => (s.sources ?? []).map((src) => src.url)),
      ...peerBundle.films.flatMap((f) => f.sources.map((s) => s.url)),
    ]
    for (const url of allUrls) {
      expect(url).not.toMatch(/[?&](auth|token)=/)
    }
  })

  it('shape-pins every field FilmGrid.vue and SongGrid.vue read', () => {
    const bundle = adaptContentItems(
      [
        item({ id: 'film-shape', filename: 'Shape Test.mp4', mime_type: 'video/mp4', added_at: '2026-01-01T00:00:00Z' }),
        item({ id: 'song-shape', filename: 'Shape Song.mp3', mime_type: 'audio/mpeg', added_at: '2026-01-01T00:00:00Z' }),
      ],
      { source: 'own' },
    )
    const film = bundle.films[0]!
    // FilmGrid.vue reads: id, title, year, director, cast (search/aria-label),
    // rating, sources[].type (badges), coverSrc()/fallbackSrc() consume
    // posterUrl/backdropUrl/title/year, genres (topGenres filter).
    expect(typeof film.id).toBe('string')
    expect(typeof film.title).toBe('string')
    expect(typeof film.year).toBe('number')
    expect(typeof film.director).toBe('string')
    expect(Array.isArray(film.cast)).toBe(true)
    expect(typeof film.rating).toBe('number')
    expect(Array.isArray(film.genres)).toBe(true)
    expect(Array.isArray(film.sources)).toBe(true)
    expect(film.sources.length).toBeGreaterThan(0)
    expect(typeof film.sources[0]!.type).toBe('string')

    const song = bundle.songs[0]!
    // SongGrid.vue reads: id, title, artist (search/aria-label), album
    // (search), genres (topGenres), coverUrl, sources[].type (badges).
    expect(typeof song.id).toBe('string')
    expect(typeof song.title).toBe('string')
    expect(typeof song.artist).toBe('string')
    expect(Array.isArray(song.sources)).toBe(true)
    expect((song.sources ?? []).length).toBeGreaterThan(0)
    expect(typeof song.sources![0]!.type).toBe('string')
  })

  it('pins the three source-badge literal values for own/peer/indeehub films', () => {
    const own = adaptToFilm(item({ id: 'x', filename: 'x.mp4', mime_type: 'video/mp4' }), { source: 'own' })
    const peer = adaptToFilm(item({ id: 'y', filename: 'y.mp4', mime_type: 'video/mp4' }), { source: 'peer', peerOnion: 'p.onion' })
    const indeehub = adaptToFilm(item({ id: 'z', filename: 'z.mp4', mime_type: 'video/mp4' }), { source: 'indeehub' })
    expect(own.sources[0]!.type).toBe('nextcloud')
    expect(peer.sources[0]!.type).toBe('plex')
    expect(indeehub.sources[0]!.type).toBe('indeehub')
  })
})

describe('adaptToPodcast', () => {
  it('maps a ContentItem to a Podcast shape (exported for completeness; not reachable via adaptContentItems today)', () => {
    const podcast = adaptToPodcast(
      item({ id: 'pod-1', filename: 'Episode One.mp3', mime_type: 'audio/mpeg', description: 'A description' }),
      { source: 'own' },
    )
    expect(podcast.id).toBe('pod-1')
    expect(podcast.title).toBe('Episode One')
    expect(podcast.description).toBe('A description')
    expect(Array.isArray(podcast.sources)).toBe(true)
  })
})

describe('sortDeterministic', () => {
  it('is a pure function that does not mutate its input', () => {
    const items = [
      item({ id: 'b', filename: 'b.mp4', added_at: '2026-01-01T00:00:00Z' }),
      item({ id: 'a', filename: 'a.mp4', added_at: '2026-02-01T00:00:00Z' }),
    ]
    const copy = [...items]
    sortDeterministic(items)
    expect(items).toEqual(copy)
  })
})

// ─── Library mapping (13-11) ────────────────────────────────────────────

function track(overrides: Partial<ArchyLibraryTrack>): ArchyLibraryTrack {
  return {
    id: { source: 'OwnLibrary', path: '/var/lib/archipelago/filebrowser/Music/Artist/Album/01 Song.flac' },
    title: 'Song',
    artist: 'Artist',
    album: 'Album',
    album_artist: 'Artist',
    track_number: 1,
    disc_number: 1,
    year: 2024,
    duration_secs: 210,
    has_tags: true,
    content_hash: null,
    ...overrides,
  }
}

describe('adaptLibraryTracks', () => {
  it('maps a music.list-tracks record to a Song with title/artist/album/duration carried through from tags', () => {
    const [song] = adaptLibraryTracks([
      track({ title: 'Night Drive', artist: 'The Synths', album: 'Neon', duration_secs: 187 }),
    ])
    expect(song!.title).toBe('Night Drive')
    expect(song!.artist).toBe('The Synths')
    expect(song!.album).toBe('Neon')
    expect(song!.duration).toBe(187)
  })

  it('falls back to album_artist when artist is absent, and to an empty string when both are absent — never null/undefined', () => {
    const [withAlbumArtist] = adaptLibraryTracks([track({ artist: null, album_artist: 'Various' })])
    expect(withAlbumArtist!.artist).toBe('Various')

    const [withNeither] = adaptLibraryTracks([track({ artist: null, album_artist: null })])
    expect(withNeither!.artist).toBe('')
    expect(withNeither!.artist).not.toBe('null')
    expect(withNeither!.artist).not.toBeUndefined()
  })

  it('preserves the index\'s own deterministic order — calling the adapter twice on the same input yields the same order', () => {
    const tracks = [
      track({ id: { source: 'OwnLibrary', path: '/a' }, title: 'A' }),
      track({ id: { source: 'OwnLibrary', path: '/b' }, title: 'B' }),
      track({ id: { source: 'OwnLibrary', path: '/c' }, title: 'C' }),
    ]
    const first = adaptLibraryTracks(tracks).map((s) => s.title)
    const second = adaptLibraryTracks(tracks).map((s) => s.title)
    expect(first).toEqual(['A', 'B', 'C'])
    expect(second).toEqual(first)
  })

  it('a track with no cover art maps with an absent coverUrl, not a broken-image URL', () => {
    const [song] = adaptLibraryTracks([track({})])
    expect(song!.coverUrl).toBeUndefined()
  })

  it('a peer-sourced track carries a source entry distinguishing it from an own-library track (same three pinned literals)', () => {
    const [own] = adaptLibraryTracks([track({ id: { source: 'OwnLibrary', path: '/x' } })])
    const [peer] = adaptLibraryTracks([
      track({ id: { source: { Peer: { onion: 'abc123.onion' } }, path: '/var/lib/archipelago/purchased-content/abc123.onion/content-1' } }),
    ])
    expect(own!.sources![0]!.type).toBe('funkwhale')
    expect(peer!.sources![0]!.type).toBe('plex')
    expect(own!.sources![0]!.type).not.toBe(peer!.sources![0]!.type)
  })

  it('never produces a playback URL carrying a credential as a query parameter', () => {
    const songs = adaptLibraryTracks([
      track({ id: { source: 'OwnLibrary', path: '/var/lib/archipelago/filebrowser/Music/a.flac' } }),
      track({
        id: { source: { Peer: { onion: 'peer.onion' } }, path: '/var/lib/archipelago/purchased-content/peer.onion/content-9' },
      }),
    ])
    for (const song of songs) {
      for (const source of song.sources ?? []) {
        expect(source.url).not.toMatch(/[?&](auth|token)=/)
      }
    }
  })

  it('an own-library track resolves through the existing FileBrowser raw-file route with no query string', () => {
    const [song] = adaptLibraryTracks([
      track({ id: { source: 'OwnLibrary', path: '/var/lib/archipelago/filebrowser/Music/Artist/Song.flac' } }),
    ])
    expect(song!.sources![0]!.url).toBe('/app/filebrowser/api/raw/Music/Artist/Song.flac')
  })

  it('a peer track resolves through the existing peer Range-streaming proxy', () => {
    const [song] = adaptLibraryTracks([
      track({
        id: { source: { Peer: { onion: 'xyz.onion' } }, path: '/var/lib/archipelago/purchased-content/xyz.onion/content-42' },
      }),
    ])
    expect(song!.sources![0]!.url).toBe('/api/peer-content/xyz.onion/content-42')
  })

  it('an empty library produces an empty songs array, not undefined', () => {
    expect(adaptLibraryTracks([])).toEqual([])
    expect(adaptLibraryTracks(null)).toEqual([])
    expect(adaptLibraryTracks(undefined)).toEqual([])
  })
})

describe('adaptLibraryAlbums', () => {
  it('groups tracks by (album_artist, album), preserving first-seen order', () => {
    const tracks = [
      track({ id: { source: 'OwnLibrary', path: '/1' }, title: 'T1', album: 'Beta', album_artist: 'X' }),
      track({ id: { source: 'OwnLibrary', path: '/2' }, title: 'T2', album: 'Alpha', album_artist: 'Y' }),
      track({ id: { source: 'OwnLibrary', path: '/3' }, title: 'T3', album: 'Beta', album_artist: 'X' }),
    ]
    const albums = adaptLibraryAlbums(tracks)
    expect(albums.map((a) => a.album)).toEqual(['Beta', 'Alpha'])
    expect(albums[0]!.tracks.map((t) => t.title)).toEqual(['T1', 'T3'])
    expect(albums[1]!.tracks.map((t) => t.title)).toEqual(['T2'])
  })

  it('a track with no album tag forms no album bucket', () => {
    const albums = adaptLibraryAlbums([track({ album: null })])
    expect(albums).toEqual([])
  })

  it('is stable across repeat calls on the same input', () => {
    const tracks = [track({ id: { source: 'OwnLibrary', path: '/1' }, album: 'A' })]
    const first = adaptLibraryAlbums(tracks)
    const second = adaptLibraryAlbums(tracks)
    expect(first).toEqual(second)
  })
})
