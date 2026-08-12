import type { AppCredentialsResponse } from '@/types/api'

const FALLBACK_CREDENTIALS: Record<string, AppCredentialsResponse> = {
  filebrowser: {
    title: 'File Browser credentials',
    description: 'Use these credentials when File Browser asks you to sign in.',
    credentials: [
      { label: 'Username', value: 'admin' },
      { label: 'Password', value: 'admin', sensitive: true },
    ],
  },
  photoprism: {
    title: 'PhotoPrism credentials',
    description: 'Use these credentials when PhotoPrism asks you to sign in.',
    credentials: [
      { label: 'Username', value: 'admin' },
      { label: 'Password', value: 'archipelago', sensitive: true },
    ],
  },
}

export function resolveAppCredentials(appId: string, response?: AppCredentialsResponse | null): AppCredentialsResponse | null {
  if (response?.credentials?.length) return response
  return FALLBACK_CREDENTIALS[appId] ?? null
}
