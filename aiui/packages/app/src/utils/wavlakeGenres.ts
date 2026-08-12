/** Map common genre names to Wavlake chart genre param (top-level from list.csv) */
export const WAVLAKE_GENRE_MAP: Record<string, string> = {
  'math rock': 'alternative',
  'indie rock': 'alternative',
  'post-rock': 'alternative',
  'prog rock': 'rock',
  'progressive rock': 'rock',
  'rock': 'rock',
  'alternative': 'alternative',
  'emo': 'alternative',
  'post-hardcore': 'alternative',
  'hip-hop': 'hip-hop/rap',
  'hip hop': 'hip-hop/rap',
  'rap': 'hip-hop/rap',
  'electronic': 'electronic',
  'ambient': 'electronic',
  'house': 'dance/edm',
  'techno': 'dance/edm',
  'jazz': 'jazz',
  'blues': 'blues',
  'folk': 'singer/songwriter',
  'country': 'country',
  'classical': 'classical',
  'pop': 'pop',
  'r&b': 'r&b/soul',
  'soul': 'r&b/soul',
  'reggae': 'reggae',
  'world': 'world',
}

export function mapToWavlakeGenre(genres: string[]): string | null {
  for (const g of genres) {
    const key = g.toLowerCase().trim()
    const mapped = WAVLAKE_GENRE_MAP[key]
    if (mapped) return mapped
  }
  return null
}
