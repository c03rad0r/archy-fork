/**
 * Authenticated fetch wrapper for dev API endpoints.
 * Adds Authorization: Bearer <token> header when VITE_DEV_API_TOKEN is set.
 */
const DEV_TOKEN = import.meta.env.VITE_DEV_API_TOKEN as string | undefined

export function apiFetch(url: string, init?: RequestInit): Promise<Response> {
  if (DEV_TOKEN) {
    const headers = new Headers(init?.headers)
    headers.set('Authorization', `Bearer ${DEV_TOKEN}`)
    return fetch(url, { ...init, headers })
  }
  return fetch(url, init)
}
