<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import { appConfirm } from '@/composables/useAppConfirm'
import SeedRevealPanel from '@/components/SeedRevealPanel.vue'

const { t } = useI18n()

// Backup & Restore
interface BackupEntry {
  id: string
  created_at: string
  size_bytes: number
  encrypted: boolean
  description: string | null
}
const backupList = ref<BackupEntry[]>([])
const loadingBackups = ref(false)
const showCreateBackupModal = ref(false)
const backupPassphrase = ref('')
const backupDescription = ref('')
const creatingBackup = ref(false)
const showRestoreModal = ref(false)
const restoreBackupId = ref('')
const restorePassphrase = ref('')
const restoringBackup = ref(false)
const verifyingBackupId = ref<string | null>(null)
const deletingBackupId = ref<string | null>(null)
const backupStatusMsg = ref('')
const backupStatusType = ref<'success' | 'error'>('success')
const backupLoadError = ref('')

function formatBackupSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

function showBackupStatus(msg: string, type: 'success' | 'error') {
  backupStatusMsg.value = msg
  backupStatusType.value = type
  setTimeout(() => { backupStatusMsg.value = '' }, 5000)
}

async function loadBackups() {
  const hadBackups = backupList.value.length > 0
  loadingBackups.value = true
  backupLoadError.value = ''
  try {
    const res = await rpcClient.call<{ backups: BackupEntry[] }>({ method: 'backup.list' })
    backupList.value = res.backups || []
  } catch (e: unknown) {
    backupLoadError.value = e instanceof Error ? e.message : t('settings.backupListFailed')
    if (!hadBackups) backupList.value = []
  } finally {
    loadingBackups.value = false
  }
}

async function createBackup() {
  if (creatingBackup.value || !backupPassphrase.value) return
  creatingBackup.value = true
  try {
    await rpcClient.call({ method: 'backup.create', params: { passphrase: backupPassphrase.value, description: backupDescription.value || undefined } })
    showCreateBackupModal.value = false
    backupPassphrase.value = ''
    backupDescription.value = ''
    showBackupStatus(t('settings.backupCreatedSuccess'), 'success')
    await loadBackups()
  } catch {
    showBackupStatus(t('settings.backupCreateFailed'), 'error')
  } finally {
    creatingBackup.value = false
  }
}

async function verifyBackup(id: string) {
  const passphrase = prompt(t('settings.verifyPassphrasePrompt'))
  if (!passphrase) return
  verifyingBackupId.value = id
  try {
    const res = await rpcClient.call<{ valid: boolean; error: string | null }>({ method: 'backup.verify', params: { id, passphrase } })
    if (res.valid) {
      showBackupStatus(t('settings.backupVerifiedOk'), 'success')
    } else {
      showBackupStatus(t('settings.backupVerifyFailed', { error: res.error || 'Unknown error' }), 'error')
    }
  } catch {
    showBackupStatus(t('settings.backupVerifyRequestFailed'), 'error')
  } finally {
    verifyingBackupId.value = null
  }
}

function confirmRestoreBackup(id: string) {
  restoreBackupId.value = id
  restorePassphrase.value = ''
  showRestoreModal.value = true
}

async function restoreBackup() {
  if (restoringBackup.value || !restorePassphrase.value) return
  restoringBackup.value = true
  try {
    await rpcClient.call({ method: 'backup.restore', params: { id: restoreBackupId.value, passphrase: restorePassphrase.value } })
    showRestoreModal.value = false
    showBackupStatus(t('settings.backupRestored'), 'success')
  } catch {
    showBackupStatus(t('settings.backupRestoreFailed'), 'error')
  } finally {
    restoringBackup.value = false
  }
}

async function deleteBackup(id: string) {
  const ok = await appConfirm({
    message: t('settings.deleteBackupConfirm'),
    confirmLabel: t('common.delete'),
    danger: true,
  })
  if (!ok) return
  deletingBackupId.value = id
  try {
    await rpcClient.call({ method: 'backup.delete', params: { id } })
    showBackupStatus(t('settings.backupDeleted'), 'success')
    await loadBackups()
  } catch {
    showBackupStatus(t('settings.backupDeleteFailed'), 'error')
  } finally {
    deletingBackupId.value = null
  }
}

// Download the encrypted backup archive through the browser — the only
// path off the node for remote/companion users with no USB access.
const downloadingId = ref<string | null>(null)
async function downloadBackup(id: string) {
  downloadingId.value = id
  try {
    const res = await fetch(`/api/blob/backup/${encodeURIComponent(id)}`, { credentials: 'include' })
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    const blob = await res.blob()
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `archipelago-backup-${id}.bak`
    document.body.appendChild(a)
    a.click()
    a.remove()
    URL.revokeObjectURL(url)
    showBackupStatus('Backup download started', 'success')
  } catch {
    showBackupStatus('Backup download failed', 'error')
  } finally {
    downloadingId.value = null
  }
}

// Recovery phrase reveal — re-auth gated (password + 2FA when enabled).
const showRevealModal = ref(false)
const revealPassword = ref('')
const revealCode = ref('')
const revealPassphrase = ref('')
const revealing = ref(false)
const revealError = ref('')
const revealedWords = ref<string[]>([])
const wordsHidden = ref(true)
const wordsCopied = ref(false)

function openReveal() {
  revealPassword.value = ''
  revealCode.value = ''
  revealPassphrase.value = ''
  revealError.value = ''
  revealedWords.value = []
  wordsHidden.value = true
  showRevealModal.value = true
}

async function submitReveal() {
  if (revealing.value || !revealPassword.value) return
  revealing.value = true
  revealError.value = ''
  try {
    const params: Record<string, string> = { password: revealPassword.value }
    if (revealCode.value) params.code = revealCode.value
    if (revealPassphrase.value) params.passphrase = revealPassphrase.value
    const res = await rpcClient.call<{ words: string[] }>({ method: 'seed.reveal', params })
    revealedWords.value = res.words || []
    wordsHidden.value = true
  } catch (e: unknown) {
    revealError.value = e instanceof Error ? e.message : t('settings.revealSeedFailed')
  } finally {
    revealing.value = false
  }
}

function closeReveal() {
  showRevealModal.value = false
  revealedWords.value = []
  revealPassword.value = ''
  revealCode.value = ''
  revealPassphrase.value = ''
}

async function copyRevealedWords() {
  try {
    await navigator.clipboard.writeText(revealedWords.value.join(' '))
    wordsCopied.value = true
    setTimeout(() => { wordsCopied.value = false }, 2000)
  } catch { /* clipboard unavailable */ }
}

// USB Drive Backup
interface UsbDriveInfo {
  device: string
  mount_point: string | null
  label: string | null
  size_bytes: number
  removable: boolean
}
const usbCopyingId = ref<string | null>(null)

async function backupToUsb(backupId: string) {
  usbCopyingId.value = backupId
  try {
    const drivesRes = await rpcClient.call<{ drives: UsbDriveInfo[] }>({ method: 'backup.list-drives' })
    const drives = drivesRes.drives || []
    const mounted = drives.filter(d => d.mount_point)
    const target = mounted[0]
    if (!target?.mount_point) {
      showBackupStatus(t('settings.noUsbDrives'), 'error')
      return
    }
    const label = target.label || target.device
    const ok = await appConfirm({
      message: `Copy backup to USB drive "${label}" at ${target.mount_point}?`,
      confirmLabel: 'Copy',
    })
    if (!ok) return
    await rpcClient.call({ method: 'backup.to-usb', params: { id: backupId, mount_point: target.mount_point } })
    showBackupStatus(t('settings.backupCopiedToUsb', { path: target.mount_point }), 'success')
  } catch {
    showBackupStatus(t('settings.backupUsbFailed'), 'error')
  } finally {
    usbCopyingId.value = null
  }
}

// Lightning channel backup
const exportingChannelBackup = ref(false)
const channelBackupData = ref('')
const channelBackupChannels = ref(0)
const channelBackupTime = ref('')
const channelBackupError = ref('')
const channelBackupCopied = ref(false)

async function exportChannelBackup() {
  exportingChannelBackup.value = true
  channelBackupError.value = ''
  try {
    const res = await rpcClient.call<{ backup: string; channel_count: number; timestamp: string }>({
      method: 'lnd.export-channel-backup',
      timeout: 15000,
    })
    channelBackupData.value = res.backup
    channelBackupChannels.value = res.channel_count
    channelBackupTime.value = new Date(res.timestamp).toLocaleString()
  } catch (err: unknown) {
    channelBackupError.value = err instanceof Error ? err.message : 'Failed to export'
  } finally {
    exportingChannelBackup.value = false
  }
}

function copyChannelBackup() {
  if (channelBackupData.value) {
    navigator.clipboard.writeText(channelBackupData.value).catch(() => {})
    channelBackupCopied.value = true
    setTimeout(() => { channelBackupCopied.value = false }, 2000)
  }
}

loadBackups()

defineExpose({ loadBackups })
</script>

<template>
  <!-- Recovery phrase Section -->
  <div class="glass-card px-6 py-6 mb-6">
    <div class="flex items-start justify-between gap-4">
      <div class="min-w-0">
        <h2 class="text-xl font-semibold text-white/96 mb-1">Recovery phrase</h2>
        <p class="text-sm text-white/60">
          View this node's 24-word recovery phrase. You'll need to confirm your password
          (and 2FA code, if enabled). Only reveal it somewhere private — anyone with these
          words controls this node.
        </p>
        <a
          href="/entropy/"
          target="_blank"
          rel="noopener"
          class="inline-flex items-center gap-1 mt-2 text-sm text-orange-300/90 hover:text-orange-200 transition-colors"
        >
          How your seed &amp; keys work — the full guide
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"/></svg>
        </a>
      </div>
      <button
        type="button"
        class="shrink-0 glass-button rounded-lg px-4 py-2 text-sm font-medium"
        @click="openReveal"
      >Reveal</button>
    </div>
  </div>

  <!-- Reveal recovery phrase modal -->
  <Teleport to="body">
    <div v-if="showRevealModal" class="fixed inset-0 z-[3000] flex items-center justify-center p-4 bg-black/60 backdrop-blur-md" @click.self="closeReveal">
      <div class="glass-card p-6 w-full max-w-md" role="dialog" aria-modal="true" aria-labelledby="reveal-seed-title">
        <h3 id="reveal-seed-title" class="text-lg font-semibold text-white mb-1">Reveal recovery phrase</h3>

        <template v-if="revealedWords.length === 0">
          <p class="text-sm text-white/60 mb-4">Confirm your credentials to display the 24-word phrase.</p>
          <form @submit.prevent="submitReveal" class="space-y-3">
            <div>
              <label class="block text-xs text-white/60 mb-1">Password</label>
              <input v-model="revealPassword" type="password" autocomplete="current-password" class="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm focus:outline-none focus:border-white/30" placeholder="Your login password" />
            </div>
            <div>
              <label class="block text-xs text-white/60 mb-1">2FA code <span class="text-white/30">(if enabled)</span></label>
              <input v-model="revealCode" inputmode="numeric" autocomplete="one-time-code" class="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm font-mono tracking-widest focus:outline-none focus:border-white/30" placeholder="123456" />
            </div>
            <div>
              <label class="block text-xs text-white/60 mb-1">Backup passphrase <span class="text-white/30">(only if different from password)</span></label>
              <input v-model="revealPassphrase" type="password" class="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm focus:outline-none focus:border-white/30" placeholder="Leave blank to use password" />
            </div>
            <p v-if="revealError" class="text-xs text-red-300 bg-red-500/10 border border-red-400/20 rounded-lg px-3 py-2">{{ revealError }}</p>
            <div class="flex gap-2 pt-1">
              <button type="button" @click="closeReveal" class="flex-1 glass-button rounded-lg px-4 py-2 text-sm font-medium">Cancel</button>
              <button type="submit" :disabled="revealing || !revealPassword" class="flex-1 glass-button rounded-lg px-4 py-2 text-sm font-medium bg-orange-500/20 border-orange-400/30 disabled:opacity-50">
                {{ revealing ? 'Verifying…' : 'Reveal' }}
              </button>
            </div>
          </form>
        </template>

        <template v-else>
          <SeedRevealPanel :words="revealedWords" />
          <div class="flex gap-2 pt-4">
            <button type="button" @click="copyRevealedWords" class="flex-1 glass-button rounded-lg px-4 py-2 text-sm font-medium">{{ wordsCopied ? 'Copied!' : 'Copy' }}</button>
            <button type="button" @click="closeReveal" class="flex-1 glass-button rounded-lg px-4 py-2 text-sm font-medium bg-orange-500/20 border-orange-400/30">Done</button>
          </div>
        </template>
      </div>
    </div>
  </Teleport>

  <!-- Backup & Restore Section -->
  <div class="glass-card px-6 py-6 mb-6">
    <div class="mb-4">
      <h2 class="text-xl font-semibold text-white/96 mb-1">{{ t('settings.backup') }}</h2>
      <p class="text-sm text-white/60 mb-3">{{ t('settings.backupRestoreDesc') }}</p>
      <button @click="showCreateBackupModal = true" class="w-full min-h-[44px] glass-button rounded-lg text-sm font-medium flex items-center justify-center gap-2">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
        </svg>
        {{ t('settings.createBackup') }}
      </button>
    </div>
    <div v-if="loadingBackups && backupList.length === 0" class="text-sm text-white/40 py-4 text-center">{{ t('settings.loadingBackups') }}</div>
    <div v-else-if="backupList.length === 0" class="text-sm text-white/40 py-4 text-center">{{ t('settings.noBackups') }}</div>
    <div v-else class="space-y-2">
      <div v-if="loadingBackups" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
        <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Refreshing backups...
      </div>
      <div v-else-if="backupLoadError" class="p-2 rounded-lg border border-red-400/20 bg-red-500/10 text-red-200/85 text-xs">
        {{ backupLoadError }}
      </div>
      <div v-for="b in backupList" :key="b.id" class="flex flex-col sm:flex-row sm:items-center sm:justify-between p-3 bg-white/5 rounded-lg gap-2">
        <div class="min-w-0">
          <div class="text-sm text-white font-medium">{{ b.description || t('settings.systemBackup') }}</div>
          <div class="text-xs text-white/50">{{ new Date(b.created_at).toLocaleString() }} &middot; {{ formatBackupSize(b.size_bytes) }}</div>
        </div>
        <div class="flex items-center gap-2 shrink-0 flex-wrap">
          <button @click="verifyBackup(b.id)" :disabled="verifyingBackupId === b.id" class="glass-button glass-button-sm px-3 py-1.5 rounded text-xs disabled:opacity-50" :title="t('common.verify')">
            {{ verifyingBackupId === b.id ? '...' : t('common.verify') }}
          </button>
          <button @click="backupToUsb(b.id)" :disabled="usbCopyingId === b.id" class="glass-button glass-button-sm px-3 py-1.5 rounded text-xs text-blue-400 disabled:opacity-50" :title="t('settings.copyToUsb')">
            {{ usbCopyingId === b.id ? '...' : 'USB' }}
          </button>
          <button @click="downloadBackup(b.id)" :disabled="downloadingId === b.id" class="glass-button glass-button-sm px-3 py-1.5 rounded text-xs text-green-400 disabled:opacity-50" title="Download backup file">
            {{ downloadingId === b.id ? '...' : 'Download' }}
          </button>
          <button @click="confirmRestoreBackup(b.id)" class="glass-button glass-button-sm px-3 py-1.5 rounded text-xs text-orange-400" :title="t('common.restore')">
            {{ t('common.restore') }}
          </button>
          <button @click="deleteBackup(b.id)" :disabled="deletingBackupId === b.id" :aria-label="t('settings.deleteBackup')" class="glass-button glass-button-sm px-3 py-1.5 rounded text-xs text-red-400 disabled:opacity-50" :title="t('common.delete')">
            &times;
          </button>
        </div>
      </div>
    </div>
    <div v-if="backupStatusMsg" role="status" aria-live="polite" class="mt-3 text-xs px-3 py-2 rounded-lg" :class="backupStatusType === 'error' ? 'alert-error' : 'alert-success'">
      {{ backupStatusMsg }}
    </div>
  </div>

  <!-- Create Backup Modal -->
  <Teleport to="body">
    <div v-if="showCreateBackupModal" class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-md" @click.self="showCreateBackupModal = false">
      <div class="glass-card p-6 w-full max-w-md" role="dialog" aria-modal="true" aria-labelledby="create-backup-title">
        <h3 id="create-backup-title" class="text-lg font-semibold text-white mb-4">{{ t('settings.createEncryptedBackup') }}</h3>
        <div class="space-y-3">
          <div>
            <label class="text-xs text-white/50 block mb-1">{{ t('settings.encryptionPassphrase') }}</label>
            <input v-model="backupPassphrase" type="password" :placeholder="t('settings.enterPassphrase')" class="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-white/30 focus:outline-none focus:border-blue-500/50" />
          </div>
          <div>
            <label class="text-xs text-white/50 block mb-1">{{ t('settings.descriptionOptional') }}</label>
            <input v-model="backupDescription" type="text" :placeholder="t('settings.descriptionPlaceholder')" class="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-white/30 focus:outline-none focus:border-blue-500/50" />
          </div>
        </div>
        <div class="flex gap-3 mt-5">
          <button @click="showCreateBackupModal = false" class="glass-button px-4 py-2 rounded-lg text-sm flex-1">{{ t('common.cancel') }}</button>
          <button @click="createBackup" :disabled="creatingBackup || !backupPassphrase" class="glass-button glass-button-warning px-4 py-2 rounded-lg text-sm flex-1 disabled:opacity-50">
            {{ creatingBackup ? t('settings.creatingBackup') : t('settings.createBackup') }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>

  <!-- Restore Backup Modal -->
  <Teleport to="body">
    <div v-if="showRestoreModal" class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-md" @click.self="showRestoreModal = false">
      <div class="glass-card p-6 w-full max-w-md" role="dialog" aria-modal="true" aria-labelledby="restore-backup-title">
        <h3 id="restore-backup-title" class="text-lg font-semibold text-white mb-2">{{ t('settings.restoreBackupTitle') }}</h3>
        <p class="text-sm text-red-400/80 mb-4">{{ t('settings.restoreWarning') }}</p>
        <div>
          <label class="text-xs text-white/50 block mb-1">{{ t('settings.encryptionPassphrase') }}</label>
          <input v-model="restorePassphrase" type="password" :placeholder="t('settings.enterBackupPassphrase')" class="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-white/30 focus:outline-none focus:border-blue-500/50" />
        </div>
        <div class="flex gap-3 mt-5">
          <button @click="showRestoreModal = false" class="glass-button px-4 py-2 rounded-lg text-sm flex-1">{{ t('common.cancel') }}</button>
          <button @click="restoreBackup" :disabled="restoringBackup || !restorePassphrase" class="glass-button glass-button-danger px-4 py-2 rounded-lg text-sm flex-1 disabled:opacity-50">
            {{ restoringBackup ? t('common.restoring') : t('common.restore') }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>

  <!-- Lightning Channel Backup -->
  <div class="glass-card px-6 py-6 mb-6">
    <div class="flex flex-col md:flex-row md:items-center md:justify-between gap-3">
      <div>
        <h2 class="text-xl font-semibold text-white/96 mb-1">Lightning Channel Backup</h2>
        <p class="text-sm text-white/60">Export your channel state so you can restore channels on a new node. Does not include on-chain wallet seed.</p>
      </div>
      <button @click="exportChannelBackup" :disabled="exportingChannelBackup" class="glass-button px-4 py-2 rounded-lg text-sm flex items-center justify-center gap-2 w-full md:w-auto shrink-0">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
        </svg>
        {{ exportingChannelBackup ? 'Exporting...' : 'Export Channel Backup' }}
      </button>
    </div>
    <div v-if="channelBackupData" class="mt-3 bg-black/30 rounded-lg p-3">
      <p class="text-xs text-white/40 mb-1">{{ channelBackupChannels }} channel{{ channelBackupChannels !== 1 ? 's' : '' }} backed up at {{ channelBackupTime }}</p>
      <textarea readonly :value="channelBackupData" rows="3" class="w-full bg-black/20 text-xs font-mono text-white/60 rounded p-2 resize-none border border-white/10"></textarea>
      <button @click="copyChannelBackup" class="mt-2 glass-button px-3 py-1.5 rounded text-xs">{{ channelBackupCopied ? 'Copied!' : 'Copy Backup Data' }}</button>
    </div>
    <p v-if="channelBackupError" class="mt-2 text-xs text-red-400">{{ channelBackupError }}</p>
  </div>
</template>
