/**
 * Canonical internal/service app-id classification — the shared source for
 * every surface that must decide "is this a user-facing app or a component
 * of one": the My Apps/Services tabs (appsConfig) and the App Store listing
 * (the signed-catalog merge in discover/curatedApps must not publish
 * components as standalone store apps).
 *
 * Keep this module dependency-free: it is imported from both the apps view
 * and the catalog module (which must not import view code that reads the
 * store — circulars).
 */

// Service container ids (backend/infra, not user-facing). Mirrors the
// SERVICE_NAMES set that used to live in appsConfig.ts verbatim.
export const SERVICE_NAMES = new Set([
  'dwn', 'archy-mempool-db', 'archy-btcpay-db', 'archy-nbxplorer', 'archy-tor',
  // Cuprate is presented as one user-facing app. Its companion container
  // (archy-cuprate-ui) is the implementation detail; do not classify the
  // primary package as a Service or it disappears from My Apps and loses its
  // launch button during/after install.
  // Headless backends with no user-facing UI: the Fedimint ecash client daemon,
  // the Nostr relay, and the Meshtastic LoRa daemon (its chat UI lives in the
  // built-in Mesh tab) belong in Services, not My Apps.
  'fedimint-clientd', 'nostr-rs-relay', 'meshtastic',
  'immich_postgres', 'immich_redis',
  // immich is now a manifest-driven stack (app_id-named, hyphen). The server is
  // the launcher app; postgres/redis are backends → Services.
  'immich-postgres', 'immich-redis',
  'mysql-mempool', 'mempool-api', 'archy-mempool-web',
  'archy-bitcoin-ui', 'archy-lnd-ui', 'archy-electrs-ui',
  'bitcoin-ui', 'lnd-ui', 'electrs-ui',
  'indeedhub-postgres', 'indeedhub-redis', 'indeedhub-minio',
  'indeedhub-api', 'indeedhub-ffmpeg',
  'indeedhub-relay', 'indeedhub-build_api_1', 'indeedhub-build_ffmpeg-worker_1',
  'indeedhub-build_postgres_1', 'indeedhub-build_redis_1', 'indeedhub-build_minio_1',
  'indeedhub-build_minio-init_1', 'indeedhub-build_relay_1',
  // Pine voice-assistant stack: the two Wyoming engines are backends (STT/TTS)
  // reached by Home Assistant over host.containers.internal — the user-facing
  // card is "pine" (the setup/status launcher), so the engines go to Services.
  'pine-whisper', 'pine-piper', 'pine-openwakeword',
])

// Node-bundled internals that are real manifests (so they surface in the
// signed catalog's manifest list) but are not store apps: the assistant and
// the bundled relay/UIs ship with the node itself, the DWN is internal
// plumbing, and the mesh router + CLN are components of other surfaces.
export const NODE_INTERNAL_IDS = new Set([
  'aiui', 'fips-ui', 'strfry', 'web5-dwn', 'router', 'core-lightning',
])

/** Is this id a component/backend rather than a standalone user app? */
export function isServiceContainer(id: string): boolean {
  if (SERVICE_NAMES.has(id)) return true
  if (id.startsWith('indeedhub-build_')) return true
  if (id.startsWith('archy-')) return true
  // Backend naming patterns that never carry a user-facing UI: databases and
  // caches. Safe to classify by suffix (a database is never a launcher).
  if (/-(db|postgres|postgresql|redis|valkey|mariadb|mysql|cache)$/.test(id)) return true
  if (id.endsWith('_db')) return true
  return false
}

/** Should this app id appear in the App Store listing at all? */
export function isStoreListedApp(id: string): boolean {
  return !isServiceContainer(id) && !NODE_INTERNAL_IDS.has(id)
}
