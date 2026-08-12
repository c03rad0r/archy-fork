export function normalizeCloudPath(path: unknown, fallback = '/'): string {
  if (typeof path !== 'string' || !path.trim()) return fallback
  const trimmed = path.trim()
  const withSlash = trimmed.startsWith('/') ? trimmed : `/${trimmed}`
  return withSlash.replace(/\/+/g, '/')
}

export function parentCloudPath(path: string): string {
  const normalized = normalizeCloudPath(path)
  if (normalized === '/') return '/'
  const parent = normalized.slice(0, normalized.lastIndexOf('/')) || '/'
  return parent
}
