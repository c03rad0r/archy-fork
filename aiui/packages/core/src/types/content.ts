export interface ContentBlock {
  contentType: string
  data: Record<string, unknown>
  title?: string
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
}

export interface FilmSource {
  type: 'plex' | 'nextcloud' | 'youtube' | 'free-web' | 'indeehub'
  name: string
  url: string
  quality?: string
  icon: string
}

export interface FilmRendererData {
  films: Film[]
  query?: string
  totalResults?: number
}

export interface SongSource {
  type: 'plex' | 'spotify' | 'youtube' | 'apple-music' | 'bandcamp' | 'soundcloud' | 'wavlake' | 'internet_archive' | 'jamendo' | 'odysee' | 'funkwhale'
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
}

export interface PodcastRendererData {
  podcasts: Podcast[]
  query?: string
  totalResults?: number
}

export interface TVSeries {
  id: string
  title: string
  year?: number
  endYear?: number
  posterUrl?: string
  backdropUrl?: string
  synopsis?: string
  genres?: string[]
  rating?: number
  seasons?: number
  episodes?: number
  status?: 'ongoing' | 'ended' | 'cancelled' | 'upcoming'
  network?: string
  creator?: string
  cast?: string[]
  sources?: TVSeriesSource[]
}

export interface TVSeriesSource {
  type: 'plex' | 'nextcloud' | 'youtube' | 'netflix' | 'free-web' | 'local'
  name: string
  url: string
  quality?: string
  icon?: string
}

export interface Book {
  id: string
  title: string
  author: string
  year?: number
  coverUrl?: string
  description?: string
  genres?: string[]
  pages?: number
  isbn?: string
  rating?: number
  sources?: BookSource[]
}

export interface BookSource {
  type: 'openlibrary' | 'gutenberg' | 'archive' | 'goodreads' | 'libgen' | 'local'
  name: string
  url: string
  icon?: string
}

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
}

export interface Place {
  id: string
  name: string
  address?: string
  city?: string
  cuisine?: string
  category?: string
  rating?: number
  priceLevel?: number // 1-4 ($-$$$$)
  phone?: string
  website?: string
  hours?: string
  description?: string
  photoUrl?: string
  lat?: number
  lng?: number
  sources?: PlaceSource[]
}

export interface PlaceSource {
  type: 'gmaps' | 'osm' | 'yelp' | 'tripadvisor' | 'foursquare' | 'local'
  name: string
  url: string
  icon?: string
}
