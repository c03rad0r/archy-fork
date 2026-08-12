/**
 * Archy content adapter — maps `core/archipelago/src/content_server.rs`'s
 * `ContentItem` (peer files, this node's own shared files, IndeeHub movies,
 * paid/owned purchases) onto AIUI's `Film`/`Song`/`Podcast` shapes so its
 * existing `FilmGrid`/`SongGrid`/`NewsGrid` components can render real node
 * data instead of records regex-scraped out of the model's own reply text
 * (D-12, 13-CONTEXT.md).
 *
 * RESEARCH Pitfall 4: `ContentItem` (`id`, `filename`, `mime_type`,
 * `size_bytes`, `description`, `access`, `availability`, `added_at`) has NO
 * shape overlap with `Film`/`Song`/`Podcast` (`posterUrl`, `coverUrl`,
 * `sources[]`, `genres`, `runtime`, `director`, ...). This is a hand-written
 * adapter, not a pass-through — every field below is a deliberate mapping
 * decision, pinned by `__tests__/archyContentAdapter.test.ts`.
 *
 * neode-ui does not depend on `@aiui/core` (D-19 keeps the two packages
 * decoupled even though they now live in one repo), so the target shapes are
 * declared locally here rather than imported. They are kept structurally
 * identical to `aiui/packages/core/src/types/content.ts`'s `Film`/`Song`/
 * `Podcast`, plus a small Archipelago-only extension (`locked`/`priceSats`)
 * that AIUI's grids simply ignore today (D-14's paid-unlock state) — the
 * "shape pinning" test below is the regression pin against silent drift.
 */

// ─── Target shapes (structurally match aiui/packages/core/src/types/content.ts) ───

export interface FilmSource {
  type: 'plex' | 'nextcloud' | 'youtube' | 'free-web' | 'indeehub'
  name: string
  url: string
  quality?: string
  icon: string
}

export interface Film {
  id: string
  title: string
  year: number
  posterUrl: string
  backdropUrl?: string
  synopsis: string
  genres: string[]
  rating: number
  runtime: number
  director: string
  cast: string[]
  trailerUrl?: string
  sources: FilmSource[]
  /** Archipelago extension (not part of AIUI's `Film` type): set when this
   * item is `access: 'Paid'` and not yet unlocked. AIUI's `FilmGrid` reads
   * only the fields above and ignores unknown ones, so this is additive. */
  locked?: boolean
  priceSats?: number
}

export interface SongSource {
  type:
    | 'plex'
    | 'spotify'
    | 'youtube'
    | 'apple-music'
    | 'bandcamp'
    | 'soundcloud'
    | 'wavlake'
    | 'internet_archive'
    | 'jamendo'
    | 'odysee'
    | 'funkwhale'
  name: string
  url: string
  icon?: string
}

export interface Song {
  id: string
  title: string
  artist: string
  album?: string
  year?: number
  coverUrl?: string
  duration?: number
  genres?: string[]
  sources?: SongSource[]
  locked?: boolean
  priceSats?: number
}

export interface PodcastSource {
  type: 'fountain' | 'rumble' | 'youtube' | 'podcastindex' | 'castopod' | 'odysee' | 'podverse' | 'ipfs' | 'rss'
  name: string
  url: string
  icon?: string
}

export interface Podcast {
  id: string
  title: string
  host?: string
  description?: string
  coverUrl?: string
  year?: number
  episodeCount?: number
  genres?: string[]
  sources: PodcastSource[]
  locked?: boolean
  priceSats?: number
}

/** Structurally matches `aiui/packages/core/src/types/content.ts`'s
 * `ImageItem` — declared locally for the same D-19 reason as the shapes
 * above (neode-ui does not depend on `@aiui/core`). */
export interface ImageItem {
  id: string
  url: string
  title?: string
  description?: string
  alt?: string
  width?: number
  height?: number
  source?: string
  attribution?: string
  locked?: boolean
  priceSats?: number
}

// ─── Source shape: content_server.rs's ContentItem, as seen over RPC ───

/** `AccessControl` (`core/archipelago/src/content_server.rs`) serializes via
 * serde's default externally-tagged representation with `rename_all =
 * "lowercase"`: unit variants become bare strings, the struct variant
 * becomes `{ paid: { price_sats, accepted } }`. */
export type ArchyAccessControl =
  | 'free'
  | 'peersonly'
  | { paid: { price_sats: number; accepted?: string[] } }

/** `Availability`, same serialization convention. Not consumed by the
 * adapter's mapping logic today (RPC scope already decides what's fetched);
 * kept on the type for fixture fidelity and future use. */
export type ArchyAvailability = 'nobody' | 'allpeers' | { specific: { peers: string[] } }

/** The wire shape of `content_server::ContentItem`, mirrored field-for-field. */
export interface ArchyContentItem {
  id: string
  filename: string
  mime_type: string
  size_bytes: number
  description?: string | null
  access?: ArchyAccessControl
  availability?: ArchyAvailability
  added_at?: string | null
}

export interface ArchyContentBundle {
  films: Film[]
  songs: Song[]
  podcasts: Podcast[]
  /** Shared photos. Images were previously classified 'excluded' and
   * dropped on the floor, so a node sharing mostly photos rendered as an
   * empty grid — the single biggest gap between what the assistant could
   * DESCRIBE and what the surface could SHOW. AIUI has had an image grid
   * (`panelImages`/`ImageGrid`) the whole time; nothing fed it. */
  images: ImageItem[]
}

export interface AdaptContentOptions {
  /** Where this batch of items came from — decides the `sources[]` badge
   * and how a playable URL is built. Not a per-item field: a whole RPC
   * response (one node's catalog, one peer's catalog, or IndeeHub) shares
   * one source. */
  source: 'own' | 'peer' | 'indeehub'
  /** Required when `source === 'peer'` — the peer's onion address, needed
   * to build the Range-streaming proxy URL. */
  peerOnion?: string
}

// ─── Classification ───────────────────────────────────────────────────────

type ContentBucket = 'video' | 'audio' | 'image' | 'excluded'

// `m4a`, `aac`, `opus` and `wma` classify as audio via this extension
// fallback — `ShareModal.vue`'s mime map omits exactly these four today, so
// files shared with those extensions arrive here as generic
// `application/octet-stream` (or another wrong mime) rather than `audio/*`.
// Without the fallback they would be silently mis-typed as `excluded`
// instead of routing to the Songs bucket. 13-11 fixes the share side; this
// adapter must not inherit the same blind spot in the meantime.
const AUDIO_EXT_FALLBACK = new Set([
  'm4a',
  'aac',
  'opus',
  'wma',
  'mp3',
  'flac',
  'wav',
  'ogg',
])

const VIDEO_EXT_FALLBACK = new Set(['mp4', 'mkv', 'avi', 'mov', 'webm', 'm4v'])

// Same reasoning as the audio fallback above: a photo shared with a mime
// this node could not identify still has an unambiguous extension, and a
// node whose catalog is mostly photos should not present as empty.
const IMAGE_EXT_FALLBACK = new Set(['jpg', 'jpeg', 'png', 'gif', 'webp', 'avif', 'heic', 'bmp'])

function extensionOf(filename: string): string {
  const base = filename.includes('/') ? filename.slice(filename.lastIndexOf('/') + 1) : filename
  const idx = base.lastIndexOf('.')
  return idx > 0 ? base.slice(idx + 1).toLowerCase() : ''
}

/** Strip the extension from a filename to derive a display title. Directory
 * separators are stripped first so a full relative path collapses to a
 * bare filename-derived title. */
function stripExtension(filename: string): string {
  const base = filename.includes('/') ? filename.slice(filename.lastIndexOf('/') + 1) : filename
  const idx = base.lastIndexOf('.')
  return idx > 0 ? base.slice(0, idx) : base
}

/**
 * Decide which grid bucket a `ContentItem` belongs to. Video and audio mimes
 * (and, as a fallback for a wrong/generic mime, video and audio extensions)
 * route to Film/Song respectively; everything else (image, document, or
 * anything unrecognized) is excluded from all three buckets rather than
 * mis-typed into one. `ContentItem` carries no podcast-specific signal
 * (no episode/feed metadata), so nothing classifies as `podcast` here —
 * `adaptToPodcast` exists for shape completeness and future reuse (e.g. an
 * RSS/podcast-feed source) but `adaptContentItems` never calls it today.
 */
export function classifyByMime(item: Pick<ArchyContentItem, 'mime_type' | 'filename'>): ContentBucket {
  const mime = (item.mime_type || '').toLowerCase().trim()
  if (mime.startsWith('video/')) return 'video'
  if (mime.startsWith('audio/')) return 'audio'
  if (mime.startsWith('image/')) return 'image'

  const ext = extensionOf(item.filename || '')
  if (AUDIO_EXT_FALLBACK.has(ext)) return 'audio'
  if (VIDEO_EXT_FALLBACK.has(ext)) return 'video'
  if (IMAGE_EXT_FALLBACK.has(ext)) return 'image'
  return 'excluded'
}

// ─── Access / paid-lock helpers ────────────────────────────────────────────

function paidPriceSats(access: ArchyAccessControl | undefined): number | null {
  if (access && typeof access === 'object' && 'paid' in access) {
    return access.paid.price_sats
  }
  return null
}

// ─── Source badge + URL building ───────────────────────────────────────────

// FilmSource's type union has no literal that means "this node" or "a peer"
// by name — these are infrastructure-flavored badges borrowed from AIUI's
// existing (unmodified, D-12) vocabulary. 'nextcloud' (self-hosted file
// storage) stands in for this node's own catalog; 'plex' (a media server)
// stands in for a peer's shared catalog; 'indeehub' is IndeeHub's own
// literal. SongSource's union has no 'nextcloud' entry at all, so the
// self-hosted analogue there is 'funkwhale' (a self-hosted, federated audio
// server) — the closest existing badge to "this node". These three literal
// values are pinned by the adapter's tests so a later refactor cannot
// quietly change what a grid badge means (13-06-PLAN.md Task 1).
const FILM_SOURCE_TYPE: Record<AdaptContentOptions['source'], FilmSource['type']> = {
  own: 'nextcloud',
  peer: 'plex',
  indeehub: 'indeehub',
}

const SONG_SOURCE_TYPE: Record<AdaptContentOptions['source'], SongSource['type']> = {
  own: 'funkwhale',
  peer: 'plex',
  // IndeeHub carries no audio catalog in this plan's scope — audio arriving
  // tagged 'indeehub' is not an expected path, so this falls back to the
  // generic peer-media badge rather than an invalid literal.
  indeehub: 'plex',
}

const PODCAST_SOURCE_TYPE: Record<AdaptContentOptions['source'], PodcastSource['type']> = {
  own: 'rss',
  peer: 'rss',
  indeehub: 'rss',
}

const SOURCE_LABEL: Record<AdaptContentOptions['source'], string> = {
  own: 'This node',
  peer: 'Peer',
  indeehub: 'IndeeHub',
}

/**
 * Build a playable media URL for an unlocked item. **Never** builds a URL
 * containing a credential in its query string (T-13-32): own-node media
 * resolves through the existing content endpoint (`/content/<id>`, which is
 * itself unauthenticated by design — content_server access control is
 * per-item, not per-session), and peer media through the existing Rust
 * Range-streaming proxy (`/api/peer-content/<onion>/<id>`, which rides the
 * page's own session cookie automatically as a same-origin request). Both
 * already exist; this function names them, it does not mint anything new.
 */
function buildMediaUrl(item: ArchyContentItem, opts: AdaptContentOptions): string {
  if (opts.source === 'peer') {
    if (!opts.peerOnion) return ''
    return `/api/peer-content/${encodeURIComponent(opts.peerOnion)}/${encodeURIComponent(item.id)}`
  }
  // 'own' and 'indeehub' items both live in this node's own catalog once
  // added (IndeeHub ingestion still lands an entry in the same catalog —
  // D-14 routes it through the existing content subsystem, not a new one).
  return `/content/${encodeURIComponent(item.id)}`
}

// ─── Per-type mapping ───────────────────────────────────────────────────────

export function adaptToFilm(item: ArchyContentItem, opts: AdaptContentOptions): Film {
  const priceSats = paidPriceSats(item.access)
  // 'own' items are served to the authenticated owner by the node's
  // owner-bypass (`serve_content`) even when they're listed paid for
  // buyers — the operator never pays for their own files, so never lock
  // them (a locked card suppresses the playable URL, which is exactly the
  // placeholder-only grid the operator reported).
  const locked = opts.source !== 'own' && priceSats !== null
  const sourceType = FILM_SOURCE_TYPE[opts.source]
  return {
    id: item.id,
    title: stripExtension(item.filename || ''),
    year: 0,
    posterUrl: '',
    synopsis: item.description ?? '',
    genres: [],
    rating: 0,
    runtime: 0,
    director: '',
    cast: [],
    sources: [
      {
        type: sourceType,
        name: SOURCE_LABEL[opts.source],
        url: locked ? '' : buildMediaUrl(item, opts),
        icon: sourceType,
      },
    ],
    locked,
    ...(priceSats !== null ? { priceSats } : {}),
  }
}

/**
 * Map a shared photo onto AIUI's `ImageItem`. A locked (paid, not yet
 * bought) image gets an EMPTY `url` for the same reason a locked film
 * does: the card should render its price and lock, not silently fetch
 * bytes the operator has not paid for.
 */
export function adaptToImage(item: ArchyContentItem, opts: AdaptContentOptions): ImageItem {
  const priceSats = paidPriceSats(item.access)
  // 'own' items are served to the authenticated owner by the node's
  // owner-bypass (`serve_content`) even when they're listed paid for
  // buyers — the operator never pays for their own files, so never lock
  // them (a locked card suppresses the playable URL, which is exactly the
  // placeholder-only grid the operator reported).
  const locked = opts.source !== 'own' && priceSats !== null
  const title = stripExtension(item.filename || '')
  return {
    id: item.id,
    url: locked ? '' : buildMediaUrl(item, opts),
    title,
    description: item.description ?? '',
    // `alt` falls back to the title rather than being left empty — a photo
    // grid with no alt text is unreadable to a screen reader.
    alt: title,
    source: SOURCE_LABEL[opts.source],
    locked,
    ...(priceSats !== null ? { priceSats } : {}),
  }
}

export function adaptToSong(item: ArchyContentItem, opts: AdaptContentOptions): Song {
  const priceSats = paidPriceSats(item.access)
  // 'own' items are served to the authenticated owner by the node's
  // owner-bypass (`serve_content`) even when they're listed paid for
  // buyers — the operator never pays for their own files, so never lock
  // them (a locked card suppresses the playable URL, which is exactly the
  // placeholder-only grid the operator reported).
  const locked = opts.source !== 'own' && priceSats !== null
  const sourceType = SONG_SOURCE_TYPE[opts.source]
  return {
    id: item.id,
    title: stripExtension(item.filename || ''),
    artist: '',
    sources: [
      {
        type: sourceType,
        name: SOURCE_LABEL[opts.source],
        url: locked ? '' : buildMediaUrl(item, opts),
        icon: sourceType,
      },
    ],
    locked,
    ...(priceSats !== null ? { priceSats } : {}),
  }
}

/** Exported for shape completeness and future reuse (see `classifyByMime`'s
 * doc comment) — not called by `adaptContentItems` today, since
 * `ContentItem` carries no signal that would classify an item as a podcast
 * rather than a song. */
export function adaptToPodcast(item: ArchyContentItem, opts: AdaptContentOptions): Podcast {
  const priceSats = paidPriceSats(item.access)
  // 'own' items are served to the authenticated owner by the node's
  // owner-bypass (`serve_content`) even when they're listed paid for
  // buyers — the operator never pays for their own files, so never lock
  // them (a locked card suppresses the playable URL, which is exactly the
  // placeholder-only grid the operator reported).
  const locked = opts.source !== 'own' && priceSats !== null
  const sourceType = PODCAST_SOURCE_TYPE[opts.source]
  return {
    id: item.id,
    title: stripExtension(item.filename || ''),
    description: item.description ?? '',
    sources: [
      {
        type: sourceType,
        name: SOURCE_LABEL[opts.source],
        url: locked ? '' : buildMediaUrl(item, opts),
        icon: sourceType,
      },
    ],
    locked,
    ...(priceSats !== null ? { priceSats } : {}),
  }
}

// ─── Deterministic ordering ─────────────────────────────────────────────────

/**
 * Sort by `added_at` descending, `id` ascending as the tiebreak. A missing
 * `added_at` sorts as the oldest possible value rather than throwing or
 * sorting first. Calling this twice on the same input in a different array
 * order yields identical output order — the property the AIUI-03 "ordering"
 * edge requires.
 */
export function sortDeterministic(items: ArchyContentItem[]): ArchyContentItem[] {
  return [...items].sort((a, b) => {
    const at = a.added_at ?? ''
    const bt = b.added_at ?? ''
    if (at !== bt) return at > bt ? -1 : 1
    if (a.id === b.id) return 0
    return a.id < b.id ? -1 : 1
  })
}

// ─── Entry point ─────────────────────────────────────────────────────────

/**
 * Map a batch of `ContentItem`s (all from the same source — this node, one
 * peer, or IndeeHub) into grid-ready `Film`/`Song`/`Podcast` records.
 *
 * - An empty input produces `{ films: [], songs: [], podcasts: [] }` — never
 *   `undefined`, never a thrown error.
 * - Two items with identical `filename`/`size_bytes` but different `id`
 *   produce two distinct cards — cards key on `id`, never on filename+size
 *   (the AIUI-03 "adjacency" edge; also T-13-37).
 * - Output ordering is deterministic (see `sortDeterministic`).
 */
export function adaptContentItems(
  items: ArchyContentItem[] | null | undefined,
  opts: AdaptContentOptions,
): ArchyContentBundle {
  const sorted = sortDeterministic(items ?? [])
  const films: Film[] = []
  const songs: Song[] = []
  const podcasts: Podcast[] = []
  const images: ImageItem[] = []

  for (const item of sorted) {
    const bucket = classifyByMime(item)
    if (bucket === 'video') films.push(adaptToFilm(item, opts))
    else if (bucket === 'audio') songs.push(adaptToSong(item, opts))
    else if (bucket === 'image') images.push(adaptToImage(item, opts))
    // 'excluded' (documents/archives/other) — no grid renders these, so
    // they stay out of every bucket rather than being mistyped into one.
  }

  return { films, songs, podcasts, images }
}

// ─── Library mapping (13-11) ────────────────────────────────────────────
//
// `music.list-tracks` (13-07, `core/archipelago/src/api/rpc/music.rs`)
// returns real tag-extracted metadata (title/artist/album/duration) for
// this node's indexed library — a materially different, richer input than
// `ContentItem` above (which carries no tag data at all; `adaptToSong`
// above always sets `artist: ''`). `adaptLibraryTracks` is a sibling
// mapping, not a replacement: it feeds the same `songs` bucket of the
// `ArchyContentBundle`/`content:push` shape with real metadata instead of
// filename-derived guesses.

/** `MusicSource` (`core/archipelago/src/music/mod.rs`), as seen over RPC.
 * Serde's default externally-tagged representation: the unit variant
 * `OwnLibrary` becomes the bare string `"OwnLibrary"`; the struct variant
 * `Peer { onion }` becomes `{ "Peer": { "onion": string } }`. */
export type ArchyMusicSource = 'OwnLibrary' | { Peer: { onion: string } }

/** The wire shape of `core/archipelago/src/music/mod.rs`'s `Track`, as
 * returned by `music.list-tracks` — field names match the Rust struct
 * verbatim (no serde rename). */
export interface ArchyLibraryTrack {
  id: { source: ArchyMusicSource; path: string }
  title: string
  artist?: string | null
  album?: string | null
  album_artist?: string | null
  track_number?: number | null
  disc_number?: number | null
  year?: number | null
  duration_secs: number
  has_tags: boolean
  content_hash?: string | null
}

/** A library track grouped under its album — exported for shape
 * completeness and future reuse (`adaptLibraryAlbums`'s doc comment),
 * mirroring `adaptToPodcast`'s status in this same file: not consumed by
 * this plan's own wiring (`SongGrid` renders a flat track list), but a
 * real, tested mapping a future album-detail view can reuse without
 * re-deriving the grouping. */
export interface ArchyLibraryAlbum {
  album: string
  albumArtist: string
  tracks: Song[]
}

function isPeerMusicSource(source: ArchyMusicSource): source is { Peer: { onion: string } } {
  return typeof source === 'object' && source !== null && 'Peer' in source
}

/** Build a playable URL for a library track. **Never** builds a URL
 * carrying a credential in its query string (T-13-71, same rule as
 * `buildMediaUrl` above).
 *
 * An `OwnLibrary` track's `path` is an absolute, canonicalized filesystem
 * path rooted at `media_roots()`'s first entry
 * (`data_dir/filebrowser/Music`, 13-04/13-07) — everything after the
 * `/filebrowser/` path segment is the exact same FileBrowser-relative path
 * `filebrowser-client.ts`'s `streamUrl` already serves via
 * `/app/filebrowser/api/raw<path>` (the T-13-39 fix, 13-06), so this reuses
 * that existing route rather than minting a new one.
 *
 * A `Peer` track's `path` is the local byte-cache layout
 * `<data_dir>/purchased-content/<onion>/<content_id>` (13-07's second media
 * root) and resolves through the existing peer Range-streaming proxy —
 * exactly `buildMediaUrl`'s peer branch above, just deriving `onion` from
 * `MusicSource::Peer` instead of an adapter option and `content_id` from
 * the path's own basename. */
function buildLibraryTrackUrl(track: ArchyLibraryTrack): string {
  if (isPeerMusicSource(track.id.source)) {
    const onion = track.id.source.Peer.onion
    const segments = track.id.path.split('/').filter(Boolean)
    const contentId = segments[segments.length - 1]
    if (!onion || !contentId) return ''
    return `/api/peer-content/${encodeURIComponent(onion)}/${encodeURIComponent(contentId)}`
  }
  const marker = '/filebrowser/'
  const idx = track.id.path.indexOf(marker)
  if (idx === -1) return ''
  const relative = track.id.path.slice(idx + marker.length)
  if (!relative) return ''
  const encoded = relative
    .split('/')
    .filter(Boolean)
    .map((seg) => encodeURIComponent(seg))
    .join('/')
  return `/app/filebrowser/api/raw/${encoded}`
}

function librarySourceKey(source: ArchyMusicSource): string {
  return isPeerMusicSource(source) ? `peer:${source.Peer.onion}` : 'own'
}

/** Stable per-track id: `TrackId` (`{ source, path }`) has no single string
 * identity on the wire, so one is derived here deterministically from the
 * same two fields — the same input always produces the same id. */
function libraryTrackId(track: ArchyLibraryTrack): string {
  return `${librarySourceKey(track.id.source)}:${track.id.path}`
}

/**
 * Map `music.list-tracks` records onto AIUI's `Song` shape.
 *
 * - Title/artist/album/duration are carried through from the extracted
 *   tags (`title`, `artist`, `album`, `duration_secs` → `duration`).
 * - A track whose `artist` tag is absent falls back to `album_artist`, and
 *   to `''` if that is absent too — never the literal `null`/`undefined`.
 * - Ordering is **not** recomputed here: `music.list-tracks`'s own
 *   response is already deterministically ordered
 *   `(disc, track number, title)` with `(source, path)` as the final
 *   tiebreak (13-07) — re-sorting by a different key in the browser would
 *   make the grid and the RPC disagree about what "first" means, so this
 *   is a straight, order-preserving map.
 * - No cover art is ever set (`coverUrl` stays `undefined`): `Track`
 *   carries no artwork field at all, and AIUI's own artwork sources are
 *   dev-server-only Vite middleware, 404 on a node (13-CONTEXT.md
 *   landmine) — `SongGrid`'s existing no-artwork fallback renders instead
 *   of a broken image, unchanged.
 * - A peer-sourced track's `sources[0].type` differs from an own-library
 *   track's, using the same `'funkwhale'`/`'plex'` literals `SONG_SOURCE_TYPE`
 *   already pins above (13-06).
 * - No produced URL ever carries a credential in its query string.
 * - `null`/`undefined`/empty input produces `[]`, never `undefined`.
 */
export function adaptLibraryTracks(tracks: ArchyLibraryTrack[] | null | undefined): Song[] {
  return (tracks ?? []).map((track) => {
    const peer = isPeerMusicSource(track.id.source)
    const sourceType: SongSource['type'] = peer ? 'plex' : 'funkwhale'
    const artist = track.artist ?? track.album_artist ?? ''
    return {
      id: libraryTrackId(track),
      title: track.title,
      artist,
      album: track.album ?? undefined,
      year: track.year ?? undefined,
      duration: track.duration_secs,
      sources: [
        {
          type: sourceType,
          name: peer ? 'Peer' : 'This node',
          url: buildLibraryTrackUrl(track),
          icon: sourceType,
        },
      ],
    }
  })
}

/**
 * Group `music.list-tracks` records into albums, keyed on
 * `(album_artist, album)` — the same derived-albums grouping
 * `13-MUSIC-MODEL.md` defines server-side for `music.list-albums`, computed
 * here over already-adapted `Song`s so a future album-detail view can reuse
 * it without a second RPC round trip. A track with no `album` tag forms no
 * album bucket (nothing to group it under) but is still present in
 * `adaptLibraryTracks`'s flat output. Grouping preserves the input's own
 * order — first-seen album first, tracks in the order they appear — so
 * calling this twice on the same input array yields identical output order.
 */
export function adaptLibraryAlbums(tracks: ArchyLibraryTrack[] | null | undefined): ArchyLibraryAlbum[] {
  const list = tracks ?? []
  const songs = adaptLibraryTracks(list)
  const albums: ArchyLibraryAlbum[] = []
  const index = new Map<string, ArchyLibraryAlbum>()

  list.forEach((track, i) => {
    const album = track.album ?? ''
    if (!album) return
    const albumArtist = track.album_artist ?? track.artist ?? ''
    const key = `${albumArtist}::${album}`
    let bucket = index.get(key)
    if (!bucket) {
      bucket = { album, albumArtist, tracks: [] }
      index.set(key, bucket)
      albums.push(bucket)
    }
    bucket.tracks.push(songs[i]!)
  })

  return albums
}
