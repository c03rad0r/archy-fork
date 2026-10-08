import { rpcClient } from '@/api/rpc-client'
import type { SignedAppCatalog } from '@/views/discover/curatedApps'

/** Setup uses the same verified catalogue and package installer as Apps. */
export async function installPublishingApp(id: 'blossom' | 'public-web-router') {
  const response = await fetch('/api/app-catalog', { credentials: 'include', signal: AbortSignal.timeout(20000) })
  if (!response.ok) throw new Error('The trusted app catalogue is unavailable. Try again from Apps.')
  const catalog = await response.json() as SignedAppCatalog
  const entry = catalog.apps?.[id]
  const app = entry?.manifest?.app
  const dockerImage = entry?.image || app?.container?.image || app?.container?.build?.tag
  if (!entry?.version || app?.id !== id || !dockerImage) throw new Error('This app is not available in the trusted catalogue yet.')
  return rpcClient.call({ method: 'package.install', params: { id, dockerImage, version: entry.version }, timeout: 600000, maxRetries: 0 })
}
