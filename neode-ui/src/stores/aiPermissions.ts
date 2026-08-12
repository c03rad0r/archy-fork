import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { AIContextCategory } from '@/types/aiui-protocol'
import { rpcClient } from '@/api/rpc-client'

const STORAGE_KEY = 'archipelago-ai-permissions'

export interface AIPermissionCategory {
  id: AIContextCategory
  label: string
  description: string
  icon: string
  group: string
  /** True when the assistant can SEE this data but has no tools to act on it.
   *  Five categories (wallet, files, search, ai-local, notes) currently have a
   *  context arm and zero tools — the toggle copy must say so, or Settings
   *  implies actions that do not exist (operator decision, 2026-08-09). Remove
   *  per-category as real tools land. */
  contextOnly?: boolean
}

export const AI_PERMISSION_CATEGORIES: AIPermissionCategory[] = [
  {
    id: 'apps',
    label: 'Installed Apps',
    description: 'App names, status, and health — no credentials or config details',
    icon: 'M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z',
    group: 'Node Data',
  },
  {
    id: 'system',
    label: 'System Stats',
    description: 'CPU, RAM, disk usage — no file paths or IP addresses',
    icon: 'M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z',
    group: 'Node Data',
  },
  {
    id: 'network',
    label: 'Network Status',
    description: 'Connection status, peer count — no IP addresses or keys',
    icon: 'M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01',
    group: 'Node Data',
  },
  {
    id: 'bitcoin',
    label: 'Bitcoin Node',
    description: 'Block height, sync progress, mempool stats — no wallet keys',
    icon: 'M12 8c-1.657 0-3 .895-3 2s1.343 2 3 2 3 .895 3 2-1.343 2-3 2m0-8c1.11 0 2.08.402 2.599 1M12 8V7m0 1v8m0 0v1m0-1c-1.11 0-2.08-.402-2.599-1M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
    group: 'Node Data',
  },
  {
    id: 'media',
    label: 'Media Libraries',
    description: 'Local media libraries — film, music, podcast titles and metadata, no file paths',
    icon: 'M7 4v16M17 4v16M3 8h4m10 0h4M3 12h18M3 16h4m10 0h4M4 20h16a1 1 0 001-1V5a1 1 0 00-1-1H4a1 1 0 00-1 1v14a1 1 0 001 1z',
    group: 'Media & Files',
  },
  {
    id: 'files',
    contextOnly: true,
    label: 'File Names',
    description: 'Folder and file names in Cloud — no file contents',
    icon: 'M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z',
    group: 'Media & Files',
  },
  {
    id: 'notes',
    contextOnly: true,
    label: 'Documents & Notes',
    description: 'Document and note titles — no contents',
    icon: 'M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z',
    group: 'Media & Files',
  },
  {
    id: 'search',
    contextOnly: true,
    label: 'Web Search',
    description: 'Web search via your private SearXNG instance',
    icon: 'M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z',
    group: 'AI & Search',
  },
  {
    id: 'ai-local',
    contextOnly: true,
    label: 'Local AI Models',
    description: 'Local AI models via Ollama — model names and availability',
    icon: 'M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z',
    group: 'AI & Search',
  },
  {
    id: 'wallet',
    contextOnly: true,
    label: 'Wallet Overview',
    description: 'Balance, channel count — no private keys, seeds, or addresses',
    icon: 'M17 9V7a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2m2 4h10a2 2 0 002-2v-6a2 2 0 00-2-2H9a2 2 0 00-2 2v6a2 2 0 002 2zm7-5a2 2 0 11-4 0 2 2 0 014 0z',
    group: 'Financial',
  },
]

export const useAIPermissionsStore = defineStore('aiPermissions', () => {
  // Seeded from localStorage so the toggles paint immediately, then reconciled
  // with the node in hydrate(). The node is authoritative.
  const enabled = ref<Set<AIContextCategory>>(loadFromStorage())
  const hydrated = ref(false)

  /**
   * Reconcile with the node, which is where these grants actually live.
   *
   * They used to live ONLY in localStorage, which is scoped to an origin — and
   * a node answers on several (LAN address, Tailscale address, <host>.local,
   * hostname). Granting over one and returning by another showed every switch
   * off again: not reset, just never set *there*. Reported as "the AI Data
   * Access settings are not persistent through sessions", and it made a working
   * content path look broken, because every scope silently returns nothing
   * without a grant.
   *
   * Migration, not replacement: if this browser holds grants and the node holds
   * none, the local set is pushed UP rather than being wiped by an empty node.
   * That covers everyone who granted permissions before this change — without
   * it, upgrading would silently revoke them. The reverse (node has grants,
   * browser does not) is the normal case on a new device and the node wins.
   */
  async function hydrate(): Promise<void> {
    try {
      const res = await rpcClient.call<{ granted?: string[] }>({ method: 'ai.permissions.get' })
      const remote = new Set(
        (res.granted ?? []).filter((c): c is AIContextCategory =>
          AI_PERMISSION_CATEGORIES.some(cat => cat.id === c),
        ),
      )

      if (remote.size === 0 && enabled.value.size > 0) {
        await pushToNode()
      } else {
        enabled.value = remote
        save()
      }
    } catch (e) {
      // Offline, or a node too old to know the method: keep whatever
      // localStorage had. Degrading to the old behaviour is strictly better
      // than blanking the operator's grants because a request failed.
      if (import.meta.env.DEV) console.warn('AI permissions: node unreachable, using local', e)
    } finally {
      hydrated.value = true
    }
  }

  async function pushToNode(): Promise<void> {
    try {
      await rpcClient.call({
        method: 'ai.permissions.set',
        params: { granted: [...enabled.value] },
      })
    } catch (e) {
      // The local write already happened, so the UI stays consistent with what
      // the operator just clicked; it will be pushed again on the next change
      // or the next hydrate().
      if (import.meta.env.DEV) console.warn('AI permissions: failed to persist to node', e)
    }
  }

  function loadFromStorage(): Set<AIContextCategory> {
    try {
      const stored = localStorage.getItem(STORAGE_KEY)
      if (stored) {
        const parsed: unknown = JSON.parse(stored)
        if (!Array.isArray(parsed)) return new Set()
        return new Set(parsed.filter((c: unknown) => typeof c === 'string' && AI_PERMISSION_CATEGORIES.some(cat => cat.id === c)) as AIContextCategory[])
      }
    } catch (e) {
      if (import.meta.env.DEV) console.warn('Failed to load AI permissions from storage', e)
    }
    return new Set()
  }

  function save() {
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify([...enabled.value])) } catch { /* localStorage full or unavailable */ }
  }

  function isEnabled(category: AIContextCategory): boolean {
    return enabled.value.has(category)
  }

  function toggle(category: AIContextCategory) {
    if (enabled.value.has(category)) {
      enabled.value.delete(category)
    } else {
      enabled.value.add(category)
    }
    // Trigger reactivity
    enabled.value = new Set(enabled.value)
    save()
    void pushToNode()
  }

  function enableAll() {
    enabled.value = new Set(AI_PERMISSION_CATEGORIES.map(c => c.id))
    save()
    void pushToNode()
  }

  function disableAll() {
    enabled.value = new Set()
    save()
    void pushToNode()
  }

  const enabledCategories = computed(() => [...enabled.value])
  const allEnabled = computed(() => enabled.value.size === AI_PERMISSION_CATEGORIES.length)
  const noneEnabled = computed(() => enabled.value.size === 0)

  return {
    enabled,
    hydrated,
    hydrate,
    isEnabled,
    toggle,
    enableAll,
    disableAll,
    enabledCategories,
    allEnabled,
    noneEnabled,
  }
})
