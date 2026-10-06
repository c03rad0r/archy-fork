<template>
  <!-- Shared Content -->
  <div class="glass-card p-6">
    <!-- Desktop: side-by-side -->
    <div class="hidden md:flex items-center justify-between mb-4">
      <div class="flex items-center gap-3">
        <div class="flex-shrink-0 w-10 h-10 rounded-lg bg-white/10 flex items-center justify-center">
          <svg class="w-5 h-5 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 4v16M17 4v16M3 8h4m10 0h4M3 12h18M3 16h4m10 0h4M4 20h16a1 1 0 001-1V5a1 1 0 00-1-1H4a1 1 0 00-1 1v14a1 1 0 001 1z" />
          </svg>
        </div>
        <div>
          <h2 class="text-lg font-semibold text-white">{{ t('web5.content') }}</h2>
          <p class="text-xs text-white/60">{{ t('web5.contentDesc') }}</p>
        </div>
      </div>
      <div v-if="contentTab === 'mine'" class="flex items-center gap-2">
        <button @click="loadContentItems" :disabled="contentLoading" class="glass-button glass-button-sm px-3 rounded-lg text-sm font-medium">
          {{ contentLoading ? '...' : 'Refresh' }}
        </button>
        <button @click="showAddContentModal = true" class="glass-button glass-button-sm px-3 rounded-lg text-sm font-medium flex items-center gap-2">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
          </svg>
          Add
        </button>
      </div>
    </div>
    <!-- Mobile: stacked -->
    <div class="md:hidden mb-4">
      <div class="flex items-center gap-3 mb-2">
        <div class="flex-shrink-0 w-10 h-10 rounded-lg bg-white/10 flex items-center justify-center">
          <svg class="w-5 h-5 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 4v16M17 4v16M3 8h4m10 0h4M3 12h18M3 16h4m10 0h4M4 20h16a1 1 0 001-1V5a1 1 0 00-1-1H4a1 1 0 00-1 1v14a1 1 0 001 1z" />
          </svg>
        </div>
        <h2 class="text-lg font-semibold text-white">{{ t('web5.content') }}</h2>
      </div>
      <p class="text-xs text-white/60 mb-3">{{ t('web5.contentDesc') }}</p>
      <div v-if="contentTab === 'mine'" class="grid grid-cols-2 gap-2">
        <button @click="loadContentItems" :disabled="contentLoading" class="glass-button min-h-[44px] rounded-lg text-sm font-medium flex items-center justify-center">
          {{ contentLoading ? '...' : 'Refresh' }}
        </button>
        <button @click="showAddContentModal = true" class="glass-button min-h-[44px] rounded-lg text-sm font-medium flex items-center justify-center gap-2">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
          </svg>
          Add
        </button>
      </div>
    </div>

    <!-- Browse Peer Selector -->
    <div class="mb-4">
      <div class="flex items-center gap-3">
        <select
          v-model="browsePeerOnion"
          class="flex-1 px-3 py-2 rounded-lg bg-white/10 text-white text-sm border border-white/20 focus:border-orange-500 focus:ring-1 focus:ring-orange-500"
        >
          <option value="">{{ t('web5.selectPeer') }}</option>
          <option v-for="p in peers" :key="p.pubkey" :value="p.onion">
            {{ p.name || p.onion || (p.pubkey || '').slice(0, 12) + '...' }}
          </option>
        </select>
        <button
          @click="browsePeerContent"
          :disabled="!browsePeerOnion || browsingPeerContent"
          class="glass-button glass-button-sm px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50"
        >
          {{ browsingPeerContent ? t('common.loading') : t('web5.browse') }}
        </button>
      </div>
      <p v-if="browsePeerError" class="text-xs text-red-400 mt-2">{{ browsePeerError }}</p>
    </div>

    <!-- Tabs: My Content | Browse Peers -->
    <div class="flex gap-1 mb-4 border-b border-white/10">
      <button
        @click="contentTab = 'mine'"
        class="px-4 py-2 text-sm font-medium rounded-t-lg transition-colors"
        :class="contentTab === 'mine' ? 'bg-white/10 text-white' : 'text-white/60 hover:text-white/80 hover:bg-white/5'"
      >
        {{ t('web5.myContent') }}
        <span v-if="contentItems.length > 0" class="ml-1.5 text-xs text-white/50">({{ contentItems.length }})</span>
      </button>
      <button
        @click="contentTab = 'browse'"
        class="px-4 py-2 text-sm font-medium rounded-t-lg transition-colors"
        :class="contentTab === 'browse' ? 'bg-white/10 text-white' : 'text-white/60 hover:text-white/80 hover:bg-white/5'"
      >
        {{ t('web5.browsePeers') }}
      </button>
    </div>

    <!-- My Content tab -->
    <div v-show="contentTab === 'mine'">
      <div v-if="contentLoading && contentItems.length === 0" class="py-4 text-center">
        <svg class="animate-spin h-6 w-6 text-blue-400 mx-auto mb-2" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        <p class="text-white/50 text-sm">{{ t('common.loading') }}</p>
      </div>
      <div v-else-if="contentItems.length === 0" class="py-6 text-center">
        <svg class="w-12 h-12 text-white/20 mx-auto mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 4v16M17 4v16M3 8h4m10 0h4M3 12h18M3 16h4m10 0h4M4 20h16a1 1 0 001-1V5a1 1 0 00-1-1H4a1 1 0 00-1 1v14a1 1 0 001 1z" />
        </svg>
        <p class="text-white/60 text-sm mb-1">{{ t('web5.noSharedContent') }}</p>
        <p class="text-white/40 text-xs">{{ t('web5.addFilesToShare') }}</p>
      </div>
      <div v-else class="space-y-3">
        <div v-if="contentLoading" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
          <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
          </svg>
          Refreshing shared content...
        </div>
        <div
          v-for="(item, idx) in contentItems"
          :key="item.id"
          :class="{ 'card-stagger': showStagger }" class="p-4 bg-white/5 rounded-lg"
          :style="{ '--stagger-index': idx }"
        >
          <div class="flex items-start justify-between gap-3 mb-3">
            <div class="min-w-0 flex-1">
              <p class="text-sm font-medium text-white truncate">{{ item.filename }}</p>
              <p v-if="item.description" class="text-xs text-white/50 mt-0.5">{{ item.description }}</p>
              <p class="text-xs text-white/40 mt-0.5">{{ item.mime_type }} &middot; {{ formatBytes(item.size_bytes) }}</p>
            </div>
            <button
              @click="removeContentItem(item.id)"
              :disabled="removingContentId === item.id"
              class="p-2 rounded-lg text-white/40 hover:text-red-400 hover:bg-white/10 transition-colors shrink-0"
              title="Remove"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
              </svg>
            </button>
          </div>
          <div class="flex flex-wrap items-center gap-2 mb-2">
            <button
              v-for="opt in accessOptions"
              :key="opt.value"
              @click="setContentPricing(item, opt.value)"
              :disabled="updatingPricingId === item.id"
              class="px-3 py-1 text-xs rounded-lg border transition-colors"
              :class="getAccessType(item.access) === opt.value
                ? 'bg-white/15 border-white/30 text-white'
                : 'bg-white/5 border-white/10 text-white/50 hover:bg-white/10 hover:text-white/70'"
            >
              {{ opt.label }}
            </button>
          </div>
          <div v-if="getAccessType(item.access) === 'paid'" class="flex items-center gap-3 mt-2">
            <div class="flex items-center gap-2 flex-1">
              <input
                :value="getItemPrice(item.access)"
                @change="updateItemPrice(item, ($event.target as HTMLInputElement).value)"
                type="number"
                min="1"
                placeholder="100"
                class="w-24 px-2 py-1 text-xs rounded-lg bg-white/5 border border-white/10 text-white focus:outline-none focus:border-white/30"
              />
              <span class="text-xs text-white/50">sats</span>
            </div>
            <p class="text-xs text-orange-400/80">Peers will pay {{ getItemPrice(item.access) || 0 }} sats to access this</p>
          </div>
          <p v-else-if="getAccessType(item.access) === 'free'" class="text-xs text-green-400/70 mt-1">{{ t('web5.freeAccessDesc') }}</p>
          <p v-else-if="getAccessType(item.access) === 'peers_only'" class="text-xs text-blue-400/70 mt-1">{{ t('web5.peersOnlyAccessDesc') }}</p>
        </div>
      </div>
    </div>

    <!-- Browse Peers tab -->
    <div v-show="contentTab === 'browse'">
      <div v-if="browsingPeerContent && peerContentItems.length === 0" class="py-4 text-center">
        <svg class="animate-spin h-6 w-6 text-blue-400 mx-auto mb-2" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        <p class="text-white/50 text-sm">{{ t('web5.connectingToPeer') }}</p>
      </div>
      <div v-else-if="!browsePeerOnion && peerContentItems.length === 0" class="py-6 text-center">
        <svg class="w-12 h-12 text-white/20 mx-auto mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
        </svg>
        <p class="text-white/60 text-sm mb-1">{{ t('web5.selectPeerToBrowse') }}</p>
        <p class="text-white/40 text-xs">{{ t('web5.choosePeerDesc') }}</p>
      </div>
      <div v-else-if="peerContentItems.length === 0 && browsePeerOnion && !browsingPeerContent" class="py-6 text-center">
        <p class="text-white/60 text-sm">{{ t('web5.peerNoContent') }}</p>
      </div>
      <div v-else class="space-y-2">
        <div v-if="browsingPeerContent" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
          <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
          </svg>
          Refreshing peer content...
        </div>
        <div
          v-for="(pItem, idx) in peerContentItems"
          :key="pItem.id"
          :class="{ 'card-stagger': showStagger }" class="flex items-center gap-4 p-3 bg-white/5 rounded-lg"
          :style="{ '--stagger-index': idx }"
        >
          <div class="w-8 h-8 rounded-lg bg-white/10 flex items-center justify-center shrink-0">
            <svg v-if="isMediaType(pItem.mime_type)" class="w-4 h-4 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z" />
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
            <svg v-else class="w-4 h-4 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
            </svg>
          </div>
          <div class="flex-1 min-w-0">
            <p class="text-sm font-medium text-white truncate">{{ pItem.filename }}</p>
            <p v-if="pItem.description" class="text-xs text-white/50 truncate">{{ pItem.description }}</p>
            <div class="flex items-center gap-2 mt-0.5">
              <span class="text-xs text-white/40">{{ pItem.mime_type }}</span>
              <span class="text-xs text-white/30">&middot;</span>
              <span class="text-xs text-white/40">{{ formatBytes(pItem.size_bytes) }}</span>
              <span v-if="getItemPrice(pItem.access) > 0" class="text-xs text-orange-400 ml-1">{{ getItemPrice(pItem.access) }} sats</span>
              <span v-else class="text-xs text-green-400/70 ml-1">Free</span>
            </div>
          </div>
          <button
            v-if="isMediaType(pItem.mime_type) && getItemPrice(pItem.access) === 0"
            @click="streamPeerContent(pItem)"
            class="px-3 py-1.5 text-xs rounded-lg bg-orange-500/20 text-orange-400 hover:bg-orange-500/30 transition-colors shrink-0"
          >
            {{ t('web5.stream') }}
          </button>
          <button
            v-else-if="getItemPrice(pItem.access) > 0"
            @click="requestPurchase(pItem)"
            :disabled="purchasingId === pItem.id"
            class="px-3 py-1.5 text-xs rounded-lg bg-orange-500/20 text-orange-400 hover:bg-orange-500/30 transition-colors shrink-0 flex items-center gap-1"
          >
            <template v-if="purchasingId === pItem.id">
              <div class="w-3 h-3 border-2 border-orange-400/30 border-t-orange-400 rounded-full animate-spin"></div>
              Paying...
            </template>
            <template v-else>
              <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
              </svg>
              Buy {{ getItemPrice(pItem.access) }} sats
            </template>
          </button>
          <button
            v-else
            @click="downloadPeerContent(pItem)"
            class="px-3 py-1.5 text-xs rounded-lg bg-blue-500/20 text-blue-400 hover:bg-blue-500/30 transition-colors shrink-0"
          >
            {{ t('web5.download') }}
          </button>
        </div>
      </div>
    </div>
  </div>

  <!-- Content Streaming Player -->
  <Teleport to="body">
    <div v-if="streamingItem" class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-md" @click.self="closePlayer" @keydown.escape="closePlayer">
      <div class="glass-card p-0 w-full max-w-2xl overflow-hidden" role="dialog" aria-modal="true">
        <div class="flex items-center justify-between px-4 py-3 border-b border-white/10">
          <div class="min-w-0 flex-1">
            <p class="text-sm font-medium text-white truncate">{{ streamingItem.filename }}</p>
            <p class="text-xs text-white/50">{{ streamingItem.mime_type }}</p>
          </div>
          <div class="flex items-center gap-3 shrink-0">
            <div v-if="streamCostSats > 0" class="flex items-center gap-1 px-2 py-1 rounded bg-orange-500/20">
              <span class="text-xs text-orange-400 font-medium">{{ streamCostSats }} sats</span>
            </div>
            <button @click="closePlayer" class="p-2 rounded-lg text-white/50 hover:text-white hover:bg-white/10 transition-colors">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>
        <div class="p-4">
          <div v-if="streamingItem.mime_type.startsWith('audio/')">
            <audio ref="audioPlayerRef" :src="streamUrl" controls class="w-full" @timeupdate="onPlayerTimeUpdate" @error="onPlayerError"></audio>
          </div>
          <div v-else-if="streamingItem.mime_type.startsWith('video/')">
            <video ref="videoPlayerRef" :src="streamUrl" controls class="w-full rounded-lg max-h-[60vh]" @timeupdate="onPlayerTimeUpdate" @error="onPlayerError"></video>
          </div>
          <div v-if="playerError" class="mt-3 alert-error">
            <p>{{ playerError }}</p>
            <p class="text-white/50 text-xs mt-1">This may be a Tor-only resource. Copy the URL to use with a Tor-enabled media player.</p>
          </div>
          <div class="flex items-center justify-between mt-3">
            <div class="text-xs text-white/40">
              {{ formatBytes(streamingItem.size_bytes) }}
              <span v-if="streamProgress > 0"> &middot; {{ Math.round(streamProgress * 100) }}% streamed</span>
            </div>
            <button @click="copyStreamUrl" class="text-xs text-white/50 hover:text-white transition-colors">Copy URL</button>
          </div>
        </div>
      </div>
    </div>
  </Teleport>

  <!-- Purchase confirmation — payment NEVER starts before the user confirms -->
  <Teleport to="body">
    <div v-if="pendingPurchase" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-md" @click.self="pendingPurchase = null">
      <div class="glass-card p-6 w-full max-w-sm mx-4" role="dialog" aria-modal="true">
        <h3 class="text-base font-semibold text-white mb-1">Buy this file</h3>
        <p class="text-sm text-white/70 truncate mb-4">{{ pendingPurchase.filename.split('/').pop() }}</p>
        <div class="p-3 bg-white/5 rounded-lg space-y-1.5 mb-4">
          <div class="flex items-center justify-between">
            <span class="text-xs text-white/50">Ecash balance</span>
            <span class="text-sm font-medium text-white/80">{{ pendingBalance === null ? '…' : pendingBalance.toLocaleString() + ' sats' }}</span>
          </div>
          <div class="flex items-center justify-between">
            <span class="text-xs text-white/50">Price</span>
            <span class="text-sm font-medium text-white/80">−{{ getItemPrice(pendingPurchase.access).toLocaleString() }} sats</span>
          </div>
          <div class="flex items-center justify-between">
            <span class="text-xs text-white/50">Balance after</span>
            <span class="text-sm font-medium" :class="pendingBalance !== null && pendingBalance < getItemPrice(pendingPurchase.access) ? 'text-red-400' : 'text-white/80'">
              {{ pendingBalance === null ? '…' : (pendingBalance - getItemPrice(pendingPurchase.access)).toLocaleString() + ' sats' }}
            </span>
          </div>
        </div>
        <p v-if="pendingBalance !== null && pendingBalance < getItemPrice(pendingPurchase.access)" class="text-xs text-red-400 mb-3">Not enough ecash — fund your wallet, or buy from the Peers page to pay another way.</p>
        <div class="flex gap-3">
          <button @click="pendingPurchase = null" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm">Cancel</button>
          <button
            @click="confirmPendingPurchase"
            :disabled="pendingBalance !== null && pendingBalance < getItemPrice(pendingPurchase.access)"
            class="flex-1 glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50"
          >Pay from node ecash</button>
        </div>
      </div>
    </div>
  </Teleport>

  <!-- Add Content Modal -->
  <Teleport to="body">
    <div v-if="showAddContentModal" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-md" @click.self="showAddContentModal = false" @keydown.escape="showAddContentModal = false">
      <div class="glass-card p-6 w-full max-w-2xl mx-4 max-h-[90vh] overflow-y-auto" role="dialog" aria-modal="true" aria-labelledby="add-content-title">
        <h2 id="add-content-title" class="text-lg font-bold text-white mb-4">{{ t('web5.addContentTitle') }}</h2>
        <div class="space-y-4">
          <div>
            <label class="text-white/60 text-sm block mb-1">Filename</label>
            <input v-model="newContentFilename" type="text" placeholder="my-file.mp3" class="w-full input-glass" />
          </div>
          <div>
            <label class="text-white/60 text-sm block mb-1">MIME Type</label>
            <input v-model="newContentMimeType" type="text" placeholder="audio/mpeg" class="w-full input-glass" />
          </div>
          <div>
            <label class="text-white/60 text-sm block mb-1">Description (optional)</label>
            <input v-model="newContentDescription" type="text" placeholder="A short description" class="w-full input-glass" />
          </div>
          <div>
            <label class="text-white/60 text-sm block mb-2">Access</label>
            <div class="flex gap-2">
              <button
                v-for="opt in accessOptions"
                :key="opt.value"
                @click="newContentAccess = opt.value"
                class="px-3 py-1.5 text-xs rounded-lg border transition-colors"
                :class="newContentAccess === opt.value
                  ? 'bg-white/15 border-white/30 text-white'
                  : 'bg-white/5 border-white/10 text-white/50 hover:bg-white/10'"
              >{{ opt.label }}</button>
            </div>
          </div>
          <div v-if="newContentAccess === 'paid'">
            <label class="text-white/60 text-sm block mb-1">Price (sats)</label>
            <input v-model.number="newContentPrice" type="number" min="1" placeholder="100" class="w-full input-glass" />
            <p v-if="newContentPrice > 0" class="text-xs text-orange-400/80 mt-1">Peers will pay {{ newContentPrice }} sats to access this</p>
          </div>
        </div>
        <div v-if="addContentError" class="mt-3 alert-error">
          <p class="text-xs">{{ addContentError }}</p>
        </div>
        <div class="flex gap-3 mt-6">
          <button @click="showAddContentModal = false; addContentError = ''" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm">{{ t('common.cancel') }}</button>
          <button @click="addContentItem" :disabled="addingContent || !newContentFilename.trim()" class="flex-1 glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50">
            {{ addingContent ? 'Adding...' : 'Add' }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import { formatBytes, isMediaType, getAccessType, getItemPrice, safeClipboardWrite } from './utils'
import type { ContentItemData, PeerContentItem, Peer } from './types'

const { t } = useI18n()

defineProps<{
  showStagger: boolean
  peers: Peer[]
}>()

const emit = defineEmits<{
  toast: [text: string]
}>()

const contentItems = ref<ContentItemData[]>([])
const contentLoading = ref(false)
const contentTab = ref<'mine' | 'browse'>('mine')
const showAddContentModal = ref(false)
const newContentFilename = ref('')
const newContentMimeType = ref('application/octet-stream')
const newContentDescription = ref('')
const newContentAccess = ref<'free' | 'peers_only' | 'paid'>('free')
const newContentPrice = ref<number>(100)
const addingContent = ref(false)
const addContentError = ref('')
const removingContentId = ref<string | null>(null)
const updatingPricingId = ref<string | null>(null)

const accessOptions = [
  { value: 'free' as const, label: 'Free' },
  { value: 'peers_only' as const, label: 'Peers Only' },
  { value: 'paid' as const, label: 'Paid' },
]

// Browse peers
const browsePeerOnion = ref('')
const activeBrowsePeerOnion = ref('')
const browsingPeerContent = ref(false)
const browsePeerError = ref('')
const peerContentItems = ref<PeerContentItem[]>([])

// Purchase flow
const purchasingId = ref<string | null>(null)

// Streaming player
const streamingItem = ref<PeerContentItem | null>(null)
const streamUrl = ref('')
const streamCostSats = ref(0)
const streamProgress = ref(0)
const playerError = ref('')
const audioPlayerRef = ref<HTMLAudioElement | null>(null)
const videoPlayerRef = ref<HTMLVideoElement | null>(null)

async function loadContentItems() {
  contentLoading.value = true
  try {
    const res = await rpcClient.call<{ items: ContentItemData[] }>({ method: 'content.list-mine' })
    contentItems.value = res.items || []
  } catch {
    if (contentItems.value.length === 0) contentItems.value = []
  } finally {
    contentLoading.value = false
  }
}

async function addContentItem() {
  if (addingContent.value || !newContentFilename.value.trim()) return
  addingContent.value = true
  addContentError.value = ''
  try {
    await rpcClient.call({
      method: 'content.publish',
      params: {
        filename: newContentFilename.value.trim(),
        mime_type: newContentMimeType.value.trim() || 'application/octet-stream',
        description: newContentDescription.value.trim(),
        access: newContentAccess.value,
        availability: 'all_peers',
        ...(newContentAccess.value === 'paid' ? { price_sats: newContentPrice.value || 100 } : {}),
      },
    })
    showAddContentModal.value = false
    newContentFilename.value = ''
    newContentMimeType.value = 'application/octet-stream'
    newContentDescription.value = ''
    newContentAccess.value = 'free'
    newContentPrice.value = 100
    await loadContentItems()
    emit('toast', t('web5.contentAdded'))
  } catch (err: unknown) {
    addContentError.value = err instanceof Error ? err.message : t('web5.failedToAddContent')
  } finally {
    addingContent.value = false
  }
}

async function removeContentItem(id: string) {
  removingContentId.value = id
  try {
    await rpcClient.call({ method: 'content.remove', params: { id } })
    contentItems.value = contentItems.value.filter(i => i.id !== id)
    emit('toast', t('web5.contentRemoved'))
  } catch {
    emit('toast', t('web5.failedToRemoveContent'))
  } finally {
    removingContentId.value = null
  }
}

async function setContentPricing(item: ContentItemData, access: 'free' | 'peers_only' | 'paid') {
  updatingPricingId.value = item.id
  try {
    const params: Record<string, unknown> = { id: item.id, access }
    if (access === 'paid') {
      params.price_sats = getItemPrice(item.access) || 100
    }
    await rpcClient.call({ method: 'content.set-pricing', params })
    await loadContentItems()
  } catch {
    emit('toast', t('web5.failedToUpdatePricing'))
  } finally {
    updatingPricingId.value = null
  }
}

async function updateItemPrice(item: ContentItemData, value: string) {
  const price = parseInt(value, 10)
  if (!price || price <= 0) return
  updatingPricingId.value = item.id
  try {
    await rpcClient.call({
      method: 'content.set-pricing',
      params: { id: item.id, access: 'paid', price_sats: price },
    })
    await loadContentItems()
  } catch {
    emit('toast', t('web5.failedToUpdatePrice'))
  } finally {
    updatingPricingId.value = null
  }
}

async function browsePeerContent() {
  if (!browsePeerOnion.value || browsingPeerContent.value) return
  const isRefreshingActivePeer = browsePeerOnion.value === activeBrowsePeerOnion.value
  browsingPeerContent.value = true
  browsePeerError.value = ''
  if (!isRefreshingActivePeer) peerContentItems.value = []
  try {
    const res = await rpcClient.call<{ items: PeerContentItem[] }>({
      method: 'content.browse-peer',
      params: { onion: browsePeerOnion.value },
    })
    activeBrowsePeerOnion.value = browsePeerOnion.value
    peerContentItems.value = res.items || []
  } catch (err: unknown) {
    browsePeerError.value = err instanceof Error ? err.message : t('web5.failedToConnectPeer')
  } finally {
    browsingPeerContent.value = false
  }
}

function streamPeerContent(item: PeerContentItem) {
  if (!browsePeerOnion.value) return
  streamingItem.value = item
  streamUrl.value = `http://${browsePeerOnion.value}/content/${item.id}`
  streamCostSats.value = getItemPrice(item.access)
  streamProgress.value = 0
  playerError.value = ''
}

function downloadPeerContent(item: PeerContentItem) {
  if (!browsePeerOnion.value) return
  const url = `http://${browsePeerOnion.value}/content/${item.id}`
  emit('toast', t('web5.downloadUrlCopied'))
  safeClipboardWrite(url)
}

// Purchase is strictly two-step: the Buy click only opens the confirmation
// (with the balance impact); nothing is paid until the user confirms there.
const pendingPurchase = ref<PeerContentItem | null>(null)
const pendingBalance = ref<number | null>(null)

function requestPurchase(item: PeerContentItem) {
  if (purchasingId.value) return
  pendingPurchase.value = item
  pendingBalance.value = null
  rpcClient.call<{ balance_sats?: number }>({ method: 'wallet.ecash-balance' })
    .then((r) => { pendingBalance.value = r?.balance_sats ?? 0 })
    .catch(() => { pendingBalance.value = null })
}

async function confirmPendingPurchase() {
  const item = pendingPurchase.value
  pendingPurchase.value = null
  if (item) await purchaseAndDownload(item)
}

async function purchaseAndDownload(item: PeerContentItem) {
  if (!browsePeerOnion.value || purchasingId.value) return
  const price = getItemPrice(item.access)
  if (price <= 0) return

  purchasingId.value = item.id
  try {
    const result = await rpcClient.call<{ data?: string; owned?: boolean; owned_content_id?: string; error?: string }>({
      method: 'content.download-peer-paid',
      params: { onion: browsePeerOnion.value, content_id: item.id, price_sats: price, filename: item.filename, cache_only: true },
      timeout: 960000,
    })

    if (result?.owned === true) {
      const a = document.createElement('a')
      a.href = `/api/peer-content/${encodeURIComponent(browsePeerOnion.value)}/${encodeURIComponent(result.owned_content_id || item.id)}`
      a.download = item.filename.split('/').pop() || item.filename
      a.click()
      emit('toast', 'Purchase saved — download started')
    } else if (result?.data) {
      const blob = new Blob([Uint8Array.from(atob(result.data), c => c.charCodeAt(0))], { type: item.mime_type })
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = item.filename.split('/').pop() || item.filename
      a.click()
      setTimeout(() => URL.revokeObjectURL(url), 60000)
      emit('toast', 'Purchase saved — download started')
    } else {
      emit('toast', result?.error || 'Purchase could not be completed')
    }
  } catch (e: unknown) {
    emit('toast', e instanceof Error ? e.message : 'Purchase failed')
  } finally {
    purchasingId.value = null
  }
}

function closePlayer() {
  if (audioPlayerRef.value) {
    audioPlayerRef.value.pause()
    audioPlayerRef.value.src = ''
  }
  if (videoPlayerRef.value) {
    videoPlayerRef.value.pause()
    videoPlayerRef.value.src = ''
  }
  streamingItem.value = null
  streamUrl.value = ''
  streamProgress.value = 0
  playerError.value = ''
}

function onPlayerTimeUpdate() {
  const player = audioPlayerRef.value || videoPlayerRef.value
  if (player && player.duration > 0) {
    streamProgress.value = player.currentTime / player.duration
  }
}

function onPlayerError() {
  playerError.value = t('web5.playerError')
}

function copyStreamUrl() {
  if (streamUrl.value) {
    safeClipboardWrite(streamUrl.value)
    emit('toast', t('web5.streamUrlCopied'))
  }
}

defineExpose({ loadContentItems })
</script>
