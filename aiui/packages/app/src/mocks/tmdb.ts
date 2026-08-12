export interface TMDBSearchResult {
  id: number
  title: string
  overview: string
  poster_path: string | null
  backdrop_path: string | null
  release_date: string
  vote_average: number
  genre_ids: number[]
}

export interface TMDBSearchResponse {
  page: number
  results: TMDBSearchResult[]
  total_pages: number
  total_results: number
}

export const mockTMDBGenres: Record<number, string> = {
  28: 'Action', 12: 'Adventure', 16: 'Animation', 35: 'Comedy', 80: 'Crime',
  99: 'Documentary', 18: 'Drama', 10751: 'Family', 14: 'Fantasy', 36: 'History',
  27: 'Horror', 10402: 'Music', 9648: 'Mystery', 10749: 'Romance', 878: 'Sci-Fi',
  53: 'Thriller', 10752: 'War', 37: 'Western',
}

export const mockTMDBSearchResponses: Record<string, TMDBSearchResponse> = {
  'blade runner': {
    page: 1,
    total_pages: 1,
    total_results: 2,
    results: [
      { id: 335984, title: 'Blade Runner 2049', overview: 'A young blade runner discovers a long-buried secret...', poster_path: '/gajva2L0rPYkEWjzgFlBXCAVBE5.jpg', backdrop_path: '/sAtoMqDVhNDQBc3QJL3RF6hlhGq.jpg', release_date: '2017-10-04', vote_average: 7.5, genre_ids: [878, 18, 53] },
      { id: 78, title: 'Blade Runner', overview: 'In the smog-choked dystopian Los Angeles of 2019...', poster_path: '/63N9uy8nd9j7Eog2axPQ8lbr3Wj.jpg', backdrop_path: '/sXNGrxZRsqmyVv9CnUX2NZQzwSg.jpg', release_date: '1982-06-25', vote_average: 7.9, genre_ids: [878, 18, 53] },
    ],
  },
  'dune': {
    page: 1,
    total_pages: 1,
    total_results: 2,
    results: [
      { id: 438631, title: 'Dune', overview: 'Paul Atreides, a brilliant and gifted young man...', poster_path: '/d5NXSklXo0qyIYkgV94XAgMIckC.jpg', backdrop_path: '/jYEW5xZkZk2WTrdbMGAPFuBqbDc.jpg', release_date: '2021-09-15', vote_average: 7.8, genre_ids: [878, 12, 18] },
      { id: 693134, title: 'Dune: Part Two', overview: 'Paul Atreides unites with Chani and the Fremen...', poster_path: '/8b8R8l88Qje9dn9OE8PY05Nxl1X.jpg', backdrop_path: '/xOMo8BRK7PfcJv9JCnx7s5hj0PX.jpg', release_date: '2024-02-27', vote_average: 8.3, genre_ids: [878, 12, 18] },
    ],
  },
  'interstellar': {
    page: 1,
    total_pages: 1,
    total_results: 1,
    results: [
      { id: 157336, title: 'Interstellar', overview: 'A team of explorers travel through a wormhole...', poster_path: '/gEU2QniE6E77NI6lCU6MxlNBvIx.jpg', backdrop_path: '/xJHokMbljvjADYdit5fK1DDtAoB.jpg', release_date: '2014-11-05', vote_average: 8.6, genre_ids: [878, 12, 18] },
    ],
  },
}

export function mockTMDBSearch(query: string): TMDBSearchResponse {
  const key = query.toLowerCase()
  for (const [term, response] of Object.entries(mockTMDBSearchResponses)) {
    if (key.includes(term)) return response
  }
  return { page: 1, results: [], total_pages: 0, total_results: 0 }
}
