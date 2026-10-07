<template>
  <div class="pb-6">
    <!-- Header with back button — shared component so peer files match local
         files (CloudFolder) on both desktop and mobile. -->
    <div class="shrink-0 mb-4">
      <BackButton label="Back to Cloud" @click="goBack" />

      <!-- Peer Header -->
      <div class="flex items-center gap-4">
        <div class="flex-shrink-0 w-12 h-12 rounded-xl flex items-center justify-center bg-purple-500/15">
          <svg class="w-7 h-7 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2" />
          </svg>
        </div>
        <div class="hidden md:block">
          <div class="flex items-center gap-2">
            <h1 class="text-2xl font-bold text-white">{{ peerDisplayName }}</h1>
            <span
              v-if="transportPill"
              :class="transportPill.cls"
              :title="transportPill.title"
              class="text-xs px-2 py-0.5 rounded-full font-medium"
            >{{ transportPill.label }}</span>
          </div>
          <p v-if="currentPeer?.did" class="text-sm text-white/50 font-mono truncate max-w-md" :title="currentPeer.did">{{ currentPeer.did }}</p>
          <p v-else class="text-sm text-white/50">Peer files</p>
        </div>
        <!-- Mobile: the title block above is hidden (the global header carries the
             peer name), so the transport pill would vanish with it. Render it on
             its own here so mobile also sees whether this peer is FIPS or Tor. -->
        <span
          v-if="transportPill"
          :class="transportPill.cls"
          :title="transportPill.title"
          class="md:hidden text-xs px-2 py-0.5 rounded-full font-medium"
        >{{ transportPill.label }}</span>
      </div>
    </div>

    <!-- Loading -->
    <div v-if="loading && catalogItems.length === 0" class="glass-card p-8 text-center">
      <svg class="animate-spin h-6 w-6 text-purple-400 mx-auto mb-2" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
        <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
        <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
      </svg>
      <p class="text-white/50 text-sm">Connecting to peer… This may take a few seconds.</p>
    </div>

    <!-- Error -->
    <div v-else-if="catalogError && catalogItems.length === 0" class="glass-card p-6">
      <div class="alert-error mb-4">{{ catalogError }}</div>
      <button class="glass-button px-4 py-2 rounded-lg text-sm" @click="loadCatalog">Retry</button>
    </div>

    <!-- Empty -->
    <div v-else-if="catalogItems.length === 0 && !loading" class="glass-card p-8 text-center">
      <p class="text-white/50">This peer has no shared files.</p>
    </div>

    <!-- Purchase error -->
    <div v-if="purchaseError" class="glass-card p-3 mb-4 flex items-center gap-3 border border-red-500/30">
      <svg class="w-4 h-4 text-red-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
      </svg>
      <span class="text-sm text-red-400 flex-1">{{ purchaseError }}</span>
      <button class="text-xs text-white/50 hover:text-white" @click="purchaseError = null">Dismiss</button>
    </div>

    <!-- File Grid -->
    <div v-if="catalogItems.length > 0" class="space-y-3">
      <div v-if="loading" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
        <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Refreshing peer files...
      </div>
      <div v-else-if="catalogError" class="p-3 rounded-lg border border-red-400/20 bg-red-500/10 text-red-200/85 text-sm">
        {{ catalogError }}
      </div>
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        <div
          v-for="item in catalogItems"
          :key="item.id"
          class="glass-card overflow-hidden flex flex-col h-full"
        >
        <!-- Media preview (images / videos / audio) -->
        <div
          v-if="isMediaMime(item.mime_type)"
          class="relative aspect-video overflow-hidden cursor-pointer group"
          @click="openItem(item)"
        >
          <img
            v-if="item.mime_type.startsWith('image/') && previewUrls[item.id]"
            :src="previewUrls[item.id]"
            :alt="item.filename"
            class="w-full h-full object-cover"
            :style="(isPaidItem(item.access) && !isOwned(item)) ? 'filter: blur(16px); transform: scale(1.15);' : ''"
          />
          <video
            v-else-if="item.mime_type.startsWith('video/') && previewUrls[item.id]"
            :src="previewUrls[item.id]"
            class="w-full h-full object-cover pointer-events-none"
            muted
            autoplay
            loop
            playsinline
          />
          <!-- Audio waveform placeholder -->
          <div v-else-if="item.mime_type.startsWith('audio/')" class="w-full h-full flex flex-col items-center justify-center bg-gradient-to-br from-orange-500/10 to-orange-600/5">
            <svg class="w-12 h-12 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3" />
            </svg>
            <p class="mt-2 text-xs text-white/50 truncate max-w-[80%]">{{ item.filename.split('/').pop() }}</p>
          </div>
          <div v-else class="w-full h-full flex items-center justify-center" :class="fileIconBg(item.mime_type)">
            <svg class="w-10 h-10" :class="fileIconColor(item.mime_type)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="fileIconPath(item.mime_type)" />
            </svg>
          </div>
          <!-- Play button overlay for video/audio -->
          <div
            v-if="isPlayable(item.mime_type)"
            class="absolute inset-0 flex items-center justify-center bg-black/0 group-hover:bg-black/30 transition-colors"
          >
            <div class="w-12 h-12 rounded-full bg-white/20 backdrop-blur-sm flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity">
              <svg class="w-6 h-6 text-white ml-0.5" fill="currentColor" viewBox="0 0 24 24">
                <path d="M8 5v14l11-7L8 5z" />
              </svg>
            </div>
          </div>
          <!-- Owned badge (top-right) — purchased, unlocked for this buyer -->
          <div v-if="isPaidItem(item.access) && isOwned(item)" class="absolute top-2 right-2 flex items-center gap-1.5 px-2 py-1 rounded-lg bg-green-500/20 backdrop-blur-sm">
            <svg class="w-4 h-4 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
            </svg>
            <span class="text-xs font-medium text-green-400">Owned</span>
          </div>
          <!-- Paid badge (top-right) — not yet purchased -->
          <div v-else-if="isPaidItem(item.access)" class="absolute top-2 right-2 flex items-center gap-1.5 px-2 py-1 rounded-lg bg-black/60 backdrop-blur-sm">
            <svg class="w-4 h-4 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
            </svg>
            <span class="text-xs font-medium text-orange-400">{{ getItemPrice(item.access) }} sats</span>
          </div>
          <!-- Preview badge for paid playable items (only before purchase) -->
          <div v-if="isPaidItem(item.access) && !isOwned(item) && isPlayable(item.mime_type)" class="absolute bottom-2 left-2 px-2 py-0.5 rounded bg-black/60 backdrop-blur-sm">
            <span class="text-xs text-white/70">10% preview</span>
          </div>
        </div>

        <!-- Card body — pinned to the bottom so the filename + action buttons
             line up across cards of differing preview heights. -->
        <div class="p-4 flex items-center gap-4 mt-auto">
          <div v-if="!isMediaMime(item.mime_type)" class="flex-shrink-0 w-10 h-10 rounded-lg flex items-center justify-center" :class="fileIconBg(item.mime_type)">
            <svg class="w-5 h-5" :class="fileIconColor(item.mime_type)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="fileIconPath(item.mime_type)" />
            </svg>
          </div>
          <div class="flex-1 min-w-0">
            <p class="text-sm font-medium text-white truncate">{{ item.filename }}</p>
            <p class="text-xs text-white/40">{{ formatSize(item.size_bytes) }}</p>
          </div>
          <div class="flex items-center gap-2">
            <span
              v-if="!isPaidItem(item.access)"
              class="text-xs px-2 py-0.5 rounded-full"
              :class="accessBadgeClass(item.access)"
            >
              {{ accessLabel(item.access) }}
            </span>
            <!-- View button for owned content (image/video/audio), from cache -->
            <button
              v-if="isOwned(item) && isMediaMime(item.mime_type)"
              class="glass-button px-3 py-1.5 rounded-lg text-xs font-medium flex items-center gap-1.5"
              :disabled="playing === item.id"
              @click="viewOwned(item)"
            >
              <template v-if="playing === item.id">
                <div class="w-3 h-3 border-2 border-white/20 border-t-white/80 rounded-full animate-spin"></div>
                <span>Opening...</span>
              </template>
              <template v-else>
                <svg class="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M8 5v14l11-7L8 5z" />
                </svg>
                <span>View</span>
              </template>
            </button>
            <!-- Preview/Play button — pre-purchase only -->
            <button
              v-else-if="isPlayable(item.mime_type)"
              class="glass-button px-3 py-1.5 rounded-lg text-xs font-medium flex items-center gap-1.5"
              :disabled="playing === item.id"
              @click="playMedia(item)"
            >
              <template v-if="playing === item.id">
                <div class="w-3 h-3 border-2 border-white/20 border-t-white/80 rounded-full animate-spin"></div>
                <span>Loading...</span>
              </template>
              <template v-else>
                <svg class="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M8 5v14l11-7L8 5z" />
                </svg>
                <span>{{ isPaidItem(item.access) ? 'Preview' : 'Play' }}</span>
              </template>
            </button>
            <button
              class="glass-button px-3 py-1.5 rounded-lg text-xs font-medium flex items-center gap-1.5"
              :disabled="downloading === item.id"
              @click="downloadFile(item)"
            >
              <template v-if="downloading === item.id">
                <div class="w-3 h-3 border-2 border-white/20 border-t-white/80 rounded-full animate-spin"></div>
                <span>{{ isPaidItem(item.access) && !isOwned(item) ? 'Paying...' : 'Loading...' }}</span>
              </template>
              <template v-else-if="isPaidItem(item.access) && !isOwned(item)">
                <svg class="w-3.5 h-3.5 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
                </svg>
                <span>Buy {{ getItemPrice(item.access) }} sats</span>
              </template>
              <template v-else>
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                </svg>
                <span>{{ isOwned(item) ? 'Save' : 'Download' }}</span>
              </template>
            </button>
          </div>
        </div>
        </div>
      </div>
    </div>

    <!-- Video player modal -->
    <Teleport to="body">
      <Transition name="fade">
        <div
          v-if="videoPlayerUrl && videoPlayerItem"
          class="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm"
          @click.self="closeVideoPlayer"
        >
          <div class="relative w-full max-w-4xl mx-4">
            <!-- Close button -->
            <button
              class="absolute -top-10 right-0 text-white/60 hover:text-white transition-colors"
              @click="closeVideoPlayer"
            >
              <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
            <!-- Picture-in-picture -->
            <button
              v-if="pipSupported"
              class="absolute -top-10 right-10 text-white/60 hover:text-white transition-colors"
              title="Picture-in-picture"
              @click="togglePip(peerVideoRef)"
            >
              <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <rect x="3" y="5" width="18" height="14" rx="2" stroke-width="2" />
                <rect x="12" y="12" width="7" height="5" rx="1" stroke-width="2" />
              </svg>
            </button>
            <!-- Video element -->
            <div class="relative">
              <video
                ref="peerVideoRef"
                :src="videoPlayerUrl"
                class="w-full rounded-xl bg-black"
                controls
                autoplay
                @playing="videoLoading = false"
                @canplay="videoLoading = false"
                @error="videoLoading = false; videoError = true"
              />
              <!-- Loader while the stream connects over mesh/Tor -->
              <div v-if="videoLoading && !videoError" class="absolute inset-0 flex flex-col items-center justify-center gap-3 rounded-xl bg-black/60 pointer-events-none">
                <svg class="w-8 h-8 animate-spin text-white/80" fill="none" viewBox="0 0 24 24">
                  <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
                  <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.4 0 0 5.4 0 12h4z" />
                </svg>
                <span class="text-sm text-white/70">Connecting to peer…</span>
              </div>
              <!-- Error state -->
              <div v-if="videoError" class="absolute inset-0 flex flex-col items-center justify-center gap-2 rounded-xl bg-black/70 text-center px-4">
                <p class="text-sm text-white/80">Couldn't play this video</p>
                <p class="text-xs text-white/50">The peer may be offline, or this preview can't be played. Try downloading it instead.</p>
              </div>
            </div>
            <!-- Info bar -->
            <div class="mt-3 flex items-center justify-between">
              <div>
                <p class="text-sm font-medium text-white">{{ videoPlayerItem.filename.split('/').pop() }}</p>
                <p class="text-xs text-white/40">{{ formatSize(videoPlayerItem.size_bytes) }}</p>
              </div>
              <div v-if="videoPlayerPaid" class="flex items-center gap-1.5 px-2 py-1 rounded-lg bg-orange-500/15">
                <svg class="w-3.5 h-3.5 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                </svg>
                <span class="text-xs text-orange-400">10% preview - Buy to unlock full</span>
              </div>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- Owned-content viewer — full file from the local cache (no re-payment) -->
    <Teleport to="body">
      <Transition name="fade">
        <div
          v-if="viewerUrl && viewerItem"
          class="fixed inset-0 z-50 flex items-center justify-center bg-black/90 backdrop-blur-sm p-4"
          @click.self="closeViewer"
        >
          <div class="relative w-full max-w-4xl mx-4">
            <button
              class="absolute -top-10 right-0 text-white/60 hover:text-white transition-colors"
              @click="closeViewer"
            >
              <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
            <img
              v-if="viewerMime.startsWith('image/')"
              :src="viewerUrl"
              :alt="viewerItem.filename"
              class="w-full max-h-[80vh] object-contain rounded-xl bg-black"
            />
            <video
              v-else-if="viewerMime.startsWith('video/')"
              :src="viewerUrl"
              class="w-full max-h-[80vh] rounded-xl bg-black"
              controls
              autoplay
              playsinline
            />
            <audio
              v-else-if="viewerMime.startsWith('audio/')"
              :src="viewerUrl"
              class="w-full"
              controls
              autoplay
            />
            <div v-else class="glass-card p-8 rounded-xl text-center">
              <p class="text-sm text-white/70">This file type can't be previewed. Use Save to download it.</p>
            </div>
            <div class="mt-3 flex items-center justify-between gap-3">
              <div class="min-w-0">
                <p class="text-sm font-medium text-white truncate">{{ viewerItem.filename.split('/').pop() }}</p>
                <p v-if="isOwned(viewerItem)" class="text-xs text-green-400">Owned · unlocked</p>
                <p v-else class="text-xs text-white/50">Free · shared by peer</p>
              </div>
              <button
                class="glass-button px-3 py-1.5 rounded-lg text-xs font-medium flex items-center gap-1.5 shrink-0"
                @click="saveViewerItem(viewerItem)"
              >
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                </svg>
                <span>Save</span>
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- Payment method picker / Lightning invoice QR (#46) -->
    <Teleport to="body">
      <Transition name="fade">
        <div
          v-if="payItem"
          class="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm p-4"
          @click.self="closePayModal"
        >
          <div class="glass-card w-full max-w-md max-h-[calc(100dvh-2rem)] overflow-y-auto p-5 rounded-2xl relative">
            <button
              class="absolute top-1 right-1 min-h-11 min-w-11 flex items-center justify-center text-white/50 hover:text-white transition-colors"
              aria-label="Close payment"
              @click="closePayModal"
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>

            <h3 class="text-base font-semibold text-white mb-1">Buy this file</h3>
            <p class="text-xs text-white/50 mb-4 truncate">
              {{ payItem.filename.split('/').pop() }} · {{ getItemPrice(payItem.access) }} sats
            </p>

            <!-- Step 1: choose a payment method — only the methods the SELLER
                 accepts for this item are offered -->
            <div v-if="payMode === 'choose'" class="space-y-3">
              <button
                v-if="!hasBlockingLightningReceipt && (acceptsMethod(payItem.access, 'ecash') || acceptsMethod(payItem.access, 'fedimint'))"
                class="w-full glass-button px-4 py-3 rounded-xl flex items-center justify-start gap-3 text-left"
                :disabled="paymentActionBusy"
                @click="prepareEcashPay"
              >
                <svg class="w-6 h-6 text-green-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z" />
                </svg>
                <span>
                  <span class="block text-base text-white">{{ ecashPreparing ? 'Checking your wallets…' : 'Pay from this node’s ecash wallet' }}</span>
                  <span class="block text-sm text-white/50">Instant, using your Cashu or Fedimint balance</span>
                </span>
              </button>

              <button
                v-if="acceptsMethod(payItem.access, 'lightning')"
                class="w-full glass-button px-4 py-3 rounded-xl flex items-center justify-start gap-3 text-left"
                :disabled="paymentActionBusy"
                @click="payWithLightning"
              >
                <svg class="w-6 h-6 text-yellow-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
                </svg>
                <span>
                  <span class="block text-base text-white">{{ lnPaying ? (hasBlockingLightningReceipt ? 'Checking payment…' : 'Paying…') : (hasBlockingLightningReceipt ? 'Check payment / retry download' : 'Pay with my Lightning node') }}</span>
                  <span class="block text-sm text-white/50">{{ hasBlockingLightningReceipt ? 'Checks the saved attempt; does not send more sats' : 'Pays the seller’s invoice from your node’s Lightning wallet' }}</span>
                </span>
              </button>

              <button
                v-if="acceptsMethod(payItem.access, 'lightning') || acceptsMethod(payItem.access, 'onchain')"
                class="w-full glass-button px-4 py-3 rounded-xl flex items-center justify-start gap-3 text-left"
                :disabled="paymentActionBusy"
                @click="openQrPay"
              >
                <svg class="w-6 h-6 text-amber-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.5 1.5H8.25A2.25 2.25 0 006 3.75v16.5a2.25 2.25 0 002.25 2.25h7.5A2.25 2.25 0 0018 20.25V3.75a2.25 2.25 0 00-2.25-2.25H13.5m-3 0V3h3V1.5m-3 0h3m-3 18.75h3" />
                </svg>
                <span>
                  <span class="block text-base text-white">Pay from another wallet (QR)</span>
                  <span class="block text-sm text-white/50">Scan an on-chain or Lightning QR with any wallet</span>
                </span>
              </button>

              <button
                v-if="acceptsMethod(payItem.access, 'onchain')"
                class="w-full glass-button px-4 py-3 rounded-xl flex items-center justify-start gap-3 text-left"
                :disabled="paymentActionBusy || getItemPrice(payItem.access) < 546"
                @click="payOnchain"
              >
                <svg class="w-6 h-6 text-orange-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1" />
                </svg>
                <span>
                  <span class="block text-base text-white">{{ onchainPaying ? 'Sending…' : 'Pay on-chain from my node' }}</span>
                  <span class="block text-sm text-white/50">Sends Bitcoin on-chain from your node’s wallet (slower)</span>
                </span>
              </button>

              <p v-if="lnError" class="text-xs text-red-400 px-1">{{ lnError }}</p>
            </div>

            <!-- Step 1b: ecash confirmation — show which wallet will be spent -->
            <div v-else-if="payMode === 'ecash-confirm' && ecashPlan" class="space-y-4">
              <div class="text-center py-2">
                <div class="text-3xl font-bold text-white">{{ ecashPlan.chosen === 'cashu' && cashuQuote ? cashuQuote.wallet_debit_sats : getItemPrice(payItem.access) }} <span class="text-lg text-white/50">sats</span></div>
                <div class="text-xs text-white/50 mt-1">from your node’s ecash wallet</div>
              </div>

              <p v-if="ecashPlan.chosen === 'cashu' && cashuQuote" class="text-sm text-white/70 text-center">
                <span class="block">Cashu · {{ cashuQuote.network === 'testnet' ? 'Testnet' : 'Mainnet' }}</span>
                <span class="block break-all">Mint: {{ cashuQuote.mint_url }}</span>
                File: {{ cashuQuote.seller_net_sats }} sats · fees/rounding: {{ cashuQuote.wallet_debit_sats - cashuQuote.seller_net_sats }} sats
              </p>
              <p v-if="hasBlockingCashuPurchase" class="text-xs text-white/60">The original purchase is saved on your node. Retry recovers it without starting another payment.</p>
              <!-- Backend selector: the chosen one is highlighted; the user can
                   switch to the other if it has enough balance. -->
              <div class="space-y-2">
                <button
                  v-for="b in (['cashu', 'fedimint', 'ark'] as const)"
                  :key="b"
                  @click="selectEcashBackend(b)"
                  :disabled="paymentActionBusy || (b !== 'cashu' && hasBlockingCashuPurchase) || (b !== 'cashu' && ecashBalanceOf(b) < getItemPrice(payItem.access))"
                  class="w-full px-4 py-3 rounded-xl flex items-center gap-3 text-left border transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                  :class="ecashPlan.chosen === b ? 'border-green-400/70 bg-green-400/10' : 'border-white/10 bg-white/5 hover:bg-white/10'"
                >
                  <span class="text-xl shrink-0">{{ b === 'cashu' ? '🥜' : b === 'fedimint' ? '🤝' : '⚓' }}</span>
                  <span class="flex-1 min-w-0">
                    <span class="block text-base text-white">{{ b === 'cashu' ? 'Cashu' : b === 'fedimint' ? 'Fedimint' : 'Ark' }}</span>
                    <span class="block text-xs text-white/50">Balance: {{ ecashBalanceOf(b).toLocaleString() }} sats<span v-if="ecashBalanceOf(b) < getItemPrice(payItem.access)"> · not enough</span></span>
                  </span>
                  <svg v-if="ecashPlan.chosen === b" class="w-5 h-5 text-green-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                  </svg>
                </button>
              </div>

              <p v-if="purchaseError" class="text-sm text-red-400">{{ purchaseError }}</p>

              <button v-if="cashuQuote" class="w-full glass-button px-4 py-2.5 rounded-xl text-sm text-white/70" :disabled="paymentActionBusy" @click="cancelCashuPurchase">Cancel unpaid quote</button>
              <div class="flex gap-2 pt-1">
                <button
                  class="flex-1 glass-button px-4 py-2.5 rounded-xl text-sm text-white/70"
                  :disabled="paymentActionBusy"
                  @click="payMode = 'choose'"
                >Back</button>
                <button
                  class="flex-1 px-4 py-2.5 rounded-xl text-sm font-semibold text-black bg-green-400 hover:bg-green-300 transition-colors disabled:opacity-50"
                  :disabled="!ecashPlan.chosen || paymentActionBusy"
                  @click="confirmEcashPay"
                >{{ paymentActionBusy ? 'Working…' : ecashPlan.chosen === 'cashu' && !cashuQuote ? 'Check original purchase' : 'Pay' }}</button>
              </div>
            </div>

            <!-- Step 2: pay from another wallet — tabbed QR (on-chain default) -->
            <div v-else>
              <!-- Method tabs, styled like the wallet Send/Receive modal;
                   only tabs the seller accepts are shown -->
              <div class="flex gap-1 mb-4 p-1 bg-white/5 rounded-lg">
                <button
                  v-for="m in (['onchain', 'lightning'] as const).filter(m => acceptsMethod(payItem!.access, m))"
                  :key="m"
                  @click="selectQrTab(m)"
                  class="flex-1 px-2 py-1.5 rounded text-xs font-medium transition-colors"
                  :class="qrTab === m ? 'bg-white/15 text-white' : 'text-white/50 hover:text-white/80'"
                >{{ m === 'onchain' ? 'On-chain' : 'Lightning' }}</button>
              </div>

              <!-- On-chain QR -->
              <div v-if="qrTab === 'onchain'" class="text-center">
                <div v-if="onchainWaiting && !onchainData" class="py-10 flex flex-col items-center gap-3">
                  <svg class="w-7 h-7 animate-spin text-white/80" fill="none" viewBox="0 0 24 24">
                    <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
                    <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.4 0 0 5.4 0 12h4z" />
                  </svg>
                  <span class="text-sm text-white/70">Requesting an address from the seller…</span>
                </div>
                <div v-else-if="onchainData">
                  <div v-if="onchainQr" class="bg-white rounded-xl p-3 inline-block mb-3">
                    <img :src="onchainQr" alt="On-chain payment QR" class="w-48 h-48" />
                  </div>
                  <p class="text-sm text-white mb-1">{{ onchainData.amount_sats }} sats</p>
                  <p class="text-xs text-white/50 mb-3 flex items-center justify-center gap-2">
                    <svg class="w-3.5 h-3.5 animate-spin text-orange-400" fill="none" viewBox="0 0 24 24">
                      <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
                      <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.4 0 0 5.4 0 12h4z" />
                    </svg>
                    Waiting for payment…
                  </p>
                  <div class="flex items-center gap-2 bg-black/40 rounded-lg px-2 py-1.5">
                    <code class="text-[10px] text-white/60 truncate flex-1 text-left">{{ onchainData.address }}</code>
                    <button class="text-xs text-white/60 hover:text-white shrink-0" @click="copyOnchain">
                      {{ onchainCopied ? 'Copied!' : 'Copy' }}
                    </button>
                  </div>
                  <p class="text-[10px] text-white/40 mt-2">Needs 1 confirmation before the file unlocks.</p>
                </div>
                <p v-if="onchainError" class="text-sm text-red-400 mt-3">{{ onchainError }}</p>
              </div>

              <!-- Lightning invoice QR -->
              <div v-else class="text-center">
                <div v-if="invoiceWaiting && !invoiceData" class="py-10 flex flex-col items-center gap-3">
                  <svg class="w-7 h-7 animate-spin text-white/80" fill="none" viewBox="0 0 24 24">
                    <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
                    <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.4 0 0 5.4 0 12h4z" />
                  </svg>
                  <span class="text-sm text-white/70">Requesting invoice from seller…</span>
                </div>
                <div v-else-if="invoiceData">
                  <div v-if="invoiceQr" class="bg-white rounded-xl p-3 inline-block mb-3">
                    <img :src="invoiceQr" alt="Lightning invoice QR" class="w-48 h-48" />
                  </div>
                  <p class="text-sm text-white mb-1">{{ invoiceData.price_sats }} sats</p>
                  <p class="text-xs text-white/50 mb-3 flex items-center justify-center gap-2">
                    <svg class="w-3.5 h-3.5 animate-spin text-amber-400" fill="none" viewBox="0 0 24 24">
                      <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
                      <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.4 0 0 5.4 0 12h4z" />
                    </svg>
                    Waiting for payment…
                  </p>
                  <div class="flex items-center gap-2 bg-black/40 rounded-lg px-2 py-1.5">
                    <code class="text-[10px] text-white/60 truncate flex-1 text-left">{{ invoiceData.bolt11 }}</code>
                    <button class="text-xs text-white/60 hover:text-white shrink-0" @click="copyInvoice">
                      {{ invoiceCopied ? 'Copied!' : 'Copy' }}
                    </button>
                  </div>
                </div>
                <p v-if="invoiceError" class="text-sm text-red-400 mt-3">{{ invoiceError }}</p>
              </div>

              <button
                class="glass-button px-4 py-2 rounded-lg text-sm mt-4 w-full"
                @click="payMode = 'choose'"
              >
                Back
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { parseCashuQuote, readCashuAttempt, keepCashuAttempt, keepAuthoritativeCashuQuote, archiveMalformedCashuAttempt, clearCashuAttempt, type CashuQuote } from '@/composables/peerCashuPurchase'
import { usePeerPaymentOperations } from '@/composables/peerPaymentOperations'
import { ref, computed, reactive, watch, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import QRCode from 'qrcode'
import { rpcClient } from '@/api/rpc-client'
import { useResourcesStore } from '@/stores/resources'
import { useAudioPlayer } from '@/composables/useAudioPlayer'
import { pipSupported, togglePip } from '@/utils/pip'
import BackButton from '@/components/BackButton.vue'

const props = defineProps<{
  peerId?: string
}>()

const router = useRouter()

interface PeerNode {
  did: string
  pubkey: string
  onion: string
  name?: string
  trust_level: string
}

interface CatalogItem {
  id: string
  filename: string
  mime_type: string
  size_bytes: number
  description: string
  access: string | { paid: { price_sats: number } }
}

const resources = useResourcesStore()
const currentPeer = ref<PeerNode | null>(null)
const downloading = ref<string | null>(null)
const playing = ref<string | null>(null)
const purchaseError = ref<string | null>(null)

// The catalog is the SAME cached entry Cloud.vue's per-peer fan-in fills
// (`cloud.peer-browse:<onion>`): arriving here from the Cloud page paints
// the file list instantly from cache and revalidates behind it.
interface PeerBrowse {
  items: CatalogItem[]
  transport: string | null
  latencyMs: number
}
const peerOnion = computed(() => props.peerId || currentPeer.value?.onion || '')
function browseEntry() {
  // persist:true matches this key's existing resources.refresh() call in
  // loadCatalog() below, which never passes `{ persist: false }` either —
  // WR-04 just makes that pre-existing decision explicit instead of relying
  // on a default.
  return resources.entry<PeerBrowse>(`cloud.peer-browse:${peerOnion.value}`, true)
}
const catalogItems = computed(() => browseEntry().data?.items ?? [])
const catalogError = computed(() => browseEntry().error ?? '')
const loading = computed(() => {
  const s = browseEntry().loadState
  return s === 'loading' || s === 'refreshing' || (s === 'idle' && !!peerOnion.value)
})
// Transport actually used to reach this peer (returned by content.browse-peer)
// so we can show a FIPS/Tor pill instead of always assuming Tor (B21).
const transport = computed(() => browseEntry().data?.transport ?? null)
const transportPill = computed(() => {
  switch (transport.value) {
    case 'fips':
      return { label: 'FIPS', cls: 'bg-green-500/20 text-green-300', title: 'Connected over the fast encrypted mesh (FIPS)' }
    case 'mesh':
      return { label: 'Mesh', cls: 'bg-green-500/20 text-green-300', title: 'Connected over the mesh' }
    case 'lan':
      return { label: 'LAN', cls: 'bg-blue-500/20 text-blue-300', title: 'Connected over the local network' }
    case 'tor':
      return { label: 'Tor', cls: 'bg-amber-500/20 text-amber-300', title: 'Connected over Tor (slower)' }
    default:
      return null
  }
})
const previewUrls = reactive<Record<string, string>>({})
const audioPlayer = useAudioPlayer()

// ─── Owned (purchased) content ───────────────────────────────────────────
// A paid item the buyer has purchased stays unlocked for them: the backend
// caches the bytes keyed by (seller onion, content_id). We load that set so the
// gallery renders owned items unblurred and opens them in-app from the local
// cache — no re-payment, and no reliance on a browser download (which silently
// fails on the mobile companion, the "paid but never unlocked" report).
const ownedKeys = ref<Set<string>>(new Set())
function ownKey(onion: string, id: string): string { return `${onion}::${id}` }
function isOwned(item: CatalogItem): boolean {
  const onion = props.peerId || currentPeer.value?.onion
  return !!onion && ownedKeys.value.has(ownKey(onion, item.id))
}
async function loadOwned() {
  try {
    const res = await rpcClient.call<{ items: { onion: string; content_id: string }[] }>({ method: 'content.owned-list', timeout: 10000 })
    const set = new Set<string>()
    for (const it of res?.items ?? []) set.add(ownKey(it.onion, it.content_id))
    ownedKeys.value = set
  } catch { /* keep last-known owned set on a transient failure */ }
}

function base64ToBlob(base64: string, mime: string): Blob {
  return new Blob([Uint8Array.from(atob(base64), c => c.charCodeAt(0))], { type: mime })
}

// In-app viewer (lightbox) for owned content AND free images. Owned content is
// loaded through local cached streaming; free images point straight at
// the Range-capable streaming proxy (no base64 round-trip).
const viewerItem = ref<CatalogItem | null>(null)
const viewerUrl = ref<string | null>(null)
const viewerMime = ref<string>('')
/** Revoke the current viewer URL only when it's a blob: URL — free items use a
 *  plain /api/peer-content URL that must not be passed to revokeObjectURL. */
function releaseViewerUrl() {
  if (viewerUrl.value?.startsWith('blob:')) URL.revokeObjectURL(viewerUrl.value)
}
async function viewOwned(item: CatalogItem) {
  const onion = props.peerId || currentPeer.value?.onion
  if (!onion) return
  playing.value = item.id
  purchaseError.value = null
  try {
    const res = await rpcClient.call<{ data?: string; owned?: boolean; owned_content_id?: string; mime_type?: string; error?: string }>({
      method: 'content.owned-get',
      params: { onion, content_id: item.id, filename: item.filename, cache_only: true },
      timeout: 60000,
    })
    if (res?.data === undefined && res?.owned !== true) { purchaseError.value = res?.error || 'Could not open your purchased file'; return }
    const mime = res.mime_type || item.mime_type
    const url = res.data !== undefined ? URL.createObjectURL(base64ToBlob(res.data, mime))
      : `/api/peer-content/${encodeURIComponent(onion)}/${encodeURIComponent(res.owned_content_id || item.id)}`
    // Audio always plays in the global bottom-bar player — never the lightbox
    // (the blob URL is intentionally not revoked while the bar plays it).
    if (mime.startsWith('audio/')) {
      audioPlayer.play(url, item.filename.split('/').pop() || item.filename)
      return
    }
    releaseViewerUrl()
    viewerUrl.value = url
    viewerMime.value = mime
    viewerItem.value = item
  } catch (e: unknown) {
    purchaseError.value = e instanceof Error ? e.message : 'Could not open your purchased file'
  } finally {
    playing.value = null
  }
}
function closeViewer() {
  releaseViewerUrl()
  viewerUrl.value = null
  viewerItem.value = null
  viewerMime.value = ''
}

/**
 * Card click dispatcher: every peer file opens in the appropriate viewer.
 * - owned → in-app viewer / bottom-bar player from the purchase cache
 * - paid + playable → 10% preview (existing behaviour)
 * - paid + non-playable (blurred images etc.) → pay modal — the full content
 *   is never fetched or revealed before purchase
 * - free image → full-screen lightbox streaming from the Range proxy
 * - free audio/video → bottom-bar player / video modal (via playMedia)
 */
function openItem(item: CatalogItem) {
  if (isOwned(item)) {
    void viewOwned(item)
    return
  }
  if (isPaidItem(item.access)) {
    if (isPlayable(item.mime_type)) void playMedia(item)
    else openPayModal(item)
    return
  }
  if (isPlayable(item.mime_type)) {
    void playMedia(item)
    return
  }
  if (item.mime_type.startsWith('image/')) {
    const onion = props.peerId || currentPeer.value?.onion
    if (!onion) return
    releaseViewerUrl()
    viewerUrl.value = `/api/peer-content/${encodeURIComponent(onion)}/${encodeURIComponent(item.id)}`
    viewerMime.value = item.mime_type
    viewerItem.value = item
  }
}

/** Viewer footer Save: owned content saves from the purchase cache; free
 *  content streams from the Range proxy. */
function saveViewerItem(item: CatalogItem) {
  if (isOwned(item)) {
    void saveOwned(item)
    return
  }
  const onion = props.peerId || currentPeer.value?.onion
  if (!onion) return
  streamDownload(`/api/peer-content/${encodeURIComponent(onion)}/${encodeURIComponent(item.id)}`, item)
}
// Save the owned file to disk from cache (the optional "downloadable" path).
async function saveOwned(item: CatalogItem) {
  const onion = props.peerId || currentPeer.value?.onion
  if (!onion) return
  try {
    const res = await rpcClient.call<{ data?: string; owned?: boolean; owned_content_id?: string; mime_type?: string; error?: string }>({
      method: 'content.owned-get',
      params: { onion, content_id: item.id, filename: item.filename, cache_only: true },
      timeout: 60000,
    })
    if (res?.owned === true) streamDownload(`/api/peer-content/${encodeURIComponent(onion)}/${encodeURIComponent(res.owned_content_id || item.id)}`, item)
    else if (res?.data) triggerDownload(res.data, item)
    else purchaseError.value = res?.error || 'Could not save your purchased file'
  } catch (e: unknown) {
    purchaseError.value = e instanceof Error ? e.message : 'Could not save your purchased file'
  }
}

// ─── Payment picker (#46) ────────────────────────────────────────────────
// When buying a paid item the user chooses how to pay: their local ecash
// wallet (instant), or a Lightning invoice drawn on the SELLER's node that
// they can pay from any external wallet by scanning a QR.
const payItem = ref<CatalogItem | null>(null)
const paymentGeneration = ref(0)
const paymentOperations = usePeerPaymentOperations(() => {
  const peer = props.peerId || currentPeer.value?.onion
  return payItem.value && peer ? { peer, item: payItem.value.id, generation: paymentGeneration.value } : null
})
const payMode = ref<'choose' | 'ecash-confirm' | 'qr'>('choose')
// Ecash confirmation step: after the user picks "pay from this node's ecash",
// we look at both balances, decide which backend covers the price, and show a
// confirm screen so they see (and can switch) which ecash is spent (#3).
type EcashBackend = 'cashu' | 'fedimint' | 'ark'
const ecashPlan = ref<{
  cashu: number
  fedimint: number
  ark: number
  total: number
  chosen: EcashBackend | null
} | null>(null)
const ecashPreparing = ref(false)
const cashuQuote = ref<CashuQuote | null>(null)
const cashuRecoveryRequired = ref(false)
const cashuRecoveryError = ref(false)
const hasBlockingCashuPurchase = computed(() => cashuRecoveryRequired.value || cashuRecoveryError.value)
let cashuLookup: Promise<void> = Promise.resolve()
async function lookupCashuPurchase(onion: string, item: CatalogItem, generation: number) {
  const selected = () => paymentGeneration.value === generation && activePaymentMatches(onion, item.id)
  try {
    const state = await rpcClient.call<{attempts?: Array<{operation_id?: string;state?: string}>}>({method:'content.payment-status',params:{onion,content_id:item.id},timeout:15000})
    if (!Array.isArray(state?.attempts)) throw new Error('Could not verify saved purchases; recover the original payment before choosing another method.')
    if (selected() && state.attempts.some(attempt => attempt.state !== 'cancelled_unspent')) {
      cashuRecoveryRequired.value=true
      lnError.value='A Cashu purchase is saved on this node. Choose ecash to recover or cancel that operation.'
    }
  } catch (error) {
    if (selected()) { cashuRecoveryError.value=true;lnError.value=error instanceof Error?error.message:'Could not verify saved purchases' }
  }
}
async function permitFreshOtherRail(item: CatalogItem, onion: string) {
  const generation=paymentGeneration.value
  await cashuLookup
  if (paymentGeneration.value!==generation || !activePaymentMatches(onion,item.id)) return false
  if (hasBlockingCashuPurchase.value) { lnError.value='Recover or cancel the saved Cashu purchase before choosing another method.'; return false }
  return true
}


// Pay-from-another-wallet QR view: tabbed like the wallet's Send/Receive modal,
// on-chain first (the default).
const qrTab = ref<'onchain' | 'lightning'>('onchain')
const invoiceData = ref<{ bolt11: string; payment_hash: string; price_sats: number } | null>(null)
const invoiceQr = ref('')
const invoiceWaiting = ref(false)
const invoiceError = ref('')
const invoiceCopied = ref(false)
// On-chain QR (pay the seller's address from any external wallet).
const onchainData = ref<{ address: string; amount_sats: number } | null>(null)
const onchainQr = ref('')
const onchainWaiting = ref(false)
const onchainError = ref('')
const onchainCopied = ref(false)
const lnPaying = ref(false)
const lnError = ref('')
type LightningReceipt = { bolt11: string; payment_hash: string; price_sats: number; state?: 'pending' | 'succeeded' | 'failed'; failure_reason?: string }
const lnReceipt = ref<LightningReceipt | null>(null)
const lnReceiptReadError = ref(false)
const hasBlockingLightningReceipt = computed(() => lnReceiptReadError.value || Boolean(lnReceipt.value && lnReceipt.value.state !== 'failed'))
const paymentActionBusy = computed(() => paymentOperations.busy.value || lnPaying.value || onchainPaying.value || ecashPreparing.value || downloading.value === payItem.value?.id)
function activePaymentMatches(onion: string, id: string) {
  return payItem.value?.id === id && (props.peerId || currentPeer.value?.onion) === onion
}
function receiptKey(onion: string, id: string) { return `peer-file-lightning:${onion}:${id}` }
function readReceipt(onion: string, id: string): LightningReceipt | null {
  const raw = localStorage.getItem(receiptKey(onion, id))
  if (!raw) return null
  const receipt = JSON.parse(raw) as LightningReceipt
  if (!receipt.bolt11 || !/^[a-f0-9]{64}$/i.test(receipt.payment_hash) || (receipt.state !== undefined && !['pending', 'succeeded', 'failed'].includes(receipt.state))) throw new Error('Saved payment needs recovery. Do not pay again.')
  return receipt
}
function keepReceipt(onion: string, id: string, receipt: LightningReceipt, selected = () => activePaymentMatches(onion, id)) {
  // Must succeed before handing an invoice to a payer. A failed transfer or
  // navigation must never turn Retry into a second payment.
  localStorage.setItem(receiptKey(onion, id), JSON.stringify(receipt))
  if (selected()) lnReceipt.value = receipt
}
function keepFailedLightningAttempt(onion: string, id: string, receipt: LightningReceipt, reason: string, selected = () => activePaymentMatches(onion, id)) {
  keepReceipt(onion, id, { ...receipt, state: 'failed', failure_reason: reason }, selected)
  if (selected()) lnError.value = `Lightning attempt failed: ${reason}. No sats were sent by this attempt. You can choose another payment method.`
}
type InvoiceLifecycle = { paid?: boolean; state?: string; can_switch_method?: boolean }
function keepCanceledInvoice(onion: string, id: string, receipt: LightningReceipt, result: InvoiceLifecycle | undefined, selected: () => boolean) {
  if (receipt.state === 'succeeded' || result?.paid !== false || result.state !== 'canceled' || result.can_switch_method !== true) return false
  keepReceipt(onion, id, { ...receipt, state: 'failed', failure_reason: 'Seller confirmed the invoice is canceled and unpaid' }, selected)
  if (selected()) lnError.value = 'The seller confirmed this invoice is canceled and unpaid. You can choose another payment method.'
  return true
}
async function recoverFailedLightningAttempt(onion: string, id: string, receipt: LightningReceipt, selected = () => activePaymentMatches(onion, id)): Promise<'failed' | 'pending' | 'other'> {
  // Old backends throw for terminal failures. Only LND's matching payment status
  // can distinguish that from a lost reply; never infer failure from error text.
  try {
    const result = await rpcClient.call<{ status?: string; failure_reason?: string }>({
      method: 'lnd.paymentstatus', params: { payment_hash: receipt.payment_hash }, timeout: 15000,
    })
    if (result?.status === 'failed') {
      keepFailedLightningAttempt(onion, id, receipt, result.failure_reason || 'Payment failed', selected)
      return 'failed'
    }
    if (result?.status === 'pending' || result?.status === 'in_flight') return 'pending'
  } catch { /* unavailable/unknown is still recoverable, never permission to pay again */ }
  try {
    const result = await rpcClient.call<InvoiceLifecycle>({
      method: 'content.invoice-status', params: { onion, content_id: id, payment_hash: receipt.payment_hash }, timeout: 15000,
    })
    if (keepCanceledInvoice(onion, id, receipt, result, selected)) return 'failed'
    if (result?.paid === true) keepReceipt(onion, id, { ...receipt, state: 'succeeded' }, selected)
    else if (result?.state === 'open' || result?.state === 'accepted') return 'pending'
  } catch { /* Seller outage or old boolean-only response cannot authorize another payment. */ }
  return 'other'
}
const onchainPaying = ref(false)
let onchainPollTimer: ReturnType<typeof setTimeout> | null = null
let invoicePollTimer: ReturnType<typeof setTimeout> | null = null

// Video player modal state
const peerVideoRef = ref<HTMLVideoElement | null>(null)
const videoPlayerItem = ref<CatalogItem | null>(null)
const videoPlayerUrl = ref<string | null>(null)
const videoPlayerPaid = ref(false)
// Streaming a peer's file connects over mesh/Tor before the first frame, so
// show a loader until the element can actually play (or errors).
const videoLoading = ref(false)
const videoError = ref(false)

const peerDisplayName = computed(() => {
  if (currentPeer.value?.name) return currentPeer.value.name
  if (currentPeer.value?.did) return truncateDid(currentPeer.value.did)
  return props.peerId ? truncateOnion(props.peerId) : 'Peer Files'
})

function goBack() {
  router.push({ name: 'cloud' })
}

onMounted(async () => {
  if (props.peerId) {
    // The peer-name lookup is cosmetic — the catalog only needs the onion we
    // already have. Serialized, it added a full mesh round-trip before the
    // files even started loading.
    await Promise.all([
      rpcClient.federationListNodes()
        .then((result) => {
          const peers = result?.nodes ?? []
          currentPeer.value = peers.find((p: PeerNode) => p.onion === props.peerId) || null
        })
        .catch(() => { /* continue with just the onion address */ }),
      loadCatalog(),
      loadOwned(),
    ])
  }
  // No peerId → peerOnion is empty and `loading` stays false on its own.
})

function loadCatalog(): Promise<void> {
  const onion = peerOnion.value
  if (!onion) return Promise.resolve()
  return resources.refresh<PeerBrowse>(`cloud.peer-browse:${onion}`, async () => {
    const t0 = Date.now()
    const result = await rpcClient.call<{ items?: CatalogItem[]; transport?: string }>({
      method: 'content.browse-peer',
      params: { onion },
      timeout: 30000,
      // The caller has its own timeout UX; retry×3 turned one slow peer
      // into a 90s spinner.
      maxRetries: 1,
      dedup: true,
    })
    return { items: result?.items ?? [], transport: result?.transport ?? null, latencyMs: Date.now() - t0 }
  }, { persist: true }) // matches peerBrowseEntry()'s explicit persist:true for this same key
}

// Load visual previews for image and video items when catalog loads.
// Audio files don't need visual thumbnails — they show a waveform icon.
// The fan-out is capped (3 concurrent) and aborts on unmount — it used to
// fire one 30s RPC per media item all at once, unbounded.
const previewAborter = new AbortController()
onUnmounted(() => { previewAborter.abort(); closePayModal() })
const previewQueued = new Set<string>()
let previewQueue: CatalogItem[] = []
let previewWorkers = 0
const PREVIEW_CONCURRENCY = 3

function pumpPreviews(onion: string) {
  while (previewWorkers < PREVIEW_CONCURRENCY && previewQueue.length > 0) {
    const item = previewQueue.shift()!
    previewWorkers++
    void loadPreview(onion, item).finally(() => {
      previewWorkers--
      pumpPreviews(onion)
    })
  }
}

watch(catalogItems, (items) => {
  const onion = peerOnion.value
  if (!onion) return
  for (const item of items) {
    const isVisual = item.mime_type.startsWith('image/') || item.mime_type.startsWith('video/')
    if (isVisual && !previewUrls[item.id] && !previewQueued.has(item.id)) {
      previewQueued.add(item.id)
      previewQueue.push(item)
    }
  }
  pumpPreviews(onion)
}, { immediate: true })

async function loadPreview(onion: string, item: CatalogItem) {
  try {
    const result = await rpcClient.call<{ data?: string; content_type?: string }>({
      method: 'content.preview-peer',
      params: { onion, content_id: item.id },
      timeout: 30000,
      maxRetries: 1,
      signal: previewAborter.signal,
    })
    if (result?.data) {
      const mime = result.content_type || item.mime_type
      const bytes = Uint8Array.from(atob(result.data), c => c.charCodeAt(0))
      const blob = new Blob([bytes], { type: mime })
      previewUrls[item.id] = URL.createObjectURL(blob)
    }
  } catch {
    // Preview not available — icon fallback is fine
  }
}

function truncateDid(did: string): string {
  if (did.length <= 24) return did
  return did.slice(0, 16) + '...' + did.slice(-8)
}

function truncateOnion(onion: string): string {
  if (onion.length <= 20) return onion
  return onion.slice(0, 12) + '...'
}

function formatSize(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return (bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0) + ' ' + units[i]
}

function fileIconBg(mime: string): string {
  if (mime.startsWith('image/')) return 'bg-blue-500/15'
  if (mime.startsWith('audio/')) return 'bg-orange-500/15'
  if (mime.startsWith('video/')) return 'bg-pink-500/15'
  if (mime.startsWith('text/')) return 'bg-green-500/15'
  return 'bg-white/10'
}

function fileIconColor(mime: string): string {
  if (mime.startsWith('image/')) return 'text-blue-400'
  if (mime.startsWith('audio/')) return 'text-orange-400'
  if (mime.startsWith('video/')) return 'text-pink-400'
  if (mime.startsWith('text/')) return 'text-green-400'
  return 'text-white/60'
}

function fileIconPath(mime: string): string {
  if (mime.startsWith('image/')) return 'M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z'
  if (mime.startsWith('audio/')) return 'M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3'
  if (mime.startsWith('video/')) return 'M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z'
  return 'M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z'
}

function accessLabel(access: CatalogItem['access']): string {
  if (access === 'free') return 'Free'
  if (access === 'peersonly') return 'Peers Only'
  if (typeof access === 'object' && 'paid' in access) return `${access.paid.price_sats} sats`
  return String(access)
}

function accessBadgeClass(access: CatalogItem['access']): string {
  if (access === 'free') return 'bg-green-500/15 text-green-400'
  if (access === 'peersonly') return 'bg-blue-500/15 text-blue-400'
  if (typeof access === 'object' && 'paid' in access) return 'bg-orange-500/15 text-orange-400'
  return 'bg-white/10 text-white/50'
}

function isMediaMime(mime: string): boolean {
  return mime.startsWith('image/') || mime.startsWith('video/') || mime.startsWith('audio/')
}

function isPlayable(mime: string): boolean {
  return mime.startsWith('video/') || mime.startsWith('audio/')
}

function isPaidItem(access: CatalogItem['access']): boolean {
  return typeof access === 'object' && 'paid' in access
}

function getItemPrice(access: CatalogItem['access']): number {
  if (typeof access === 'object' && 'paid' in access) return access.paid.price_sats
  return 0
}

/** Payment methods the seller accepts for this item. Missing/empty = all
 *  (items shared before sellers could restrict methods). */
function acceptsMethod(access: CatalogItem['access'], method: string): boolean {
  if (typeof access !== 'object' || !('paid' in access)) return true
  const accepted = (access.paid as { accepted?: string[] }).accepted
  if (!Array.isArray(accepted) || accepted.length === 0) return true
  return accepted.includes(method)
}

async function downloadFile(item: CatalogItem) {
  const onion = props.peerId || currentPeer.value?.onion
  if (!onion) return
  purchaseError.value = null

  // Already purchased: save from the local cache, no payment.
  if (isOwned(item)) {
    await saveOwned(item)
    return
  }

  const price = getItemPrice(item.access)
  if (price > 0) {
    // Let the buyer choose how to pay (local ecash vs external-wallet QR).
    openPayModal(item)
    return
  }

  // Free / peers-only download: stream straight from the Range-capable proxy
  // (B3) to disk instead of pulling the whole file as a base64 blob over RPC.
  // The old base64-over-RPC path buffered the entire file in node memory AND
  // the browser, which failed outright for large files (#38). A tiny probe
  // first surfaces the real server error (peer unreachable on mesh and Tor)
  // instead of a generic failure (#30).
  downloading.value = item.id
  try {
    const streamUrl = `/api/peer-content/${encodeURIComponent(onion)}/${encodeURIComponent(item.id)}`
    const probe = await probePeerContent(streamUrl)
    if (probe !== true) {
      purchaseError.value = probe
      return
    }
    streamDownload(streamUrl, item)
  } catch (e: unknown) {
    purchaseError.value = e instanceof Error ? e.message : 'Download failed'
  } finally {
    downloading.value = null
  }
}

function openPayModal(item: CatalogItem) {
  paymentGeneration.value++
  cashuQuote.value = null
  cashuRecoveryRequired.value = false
  cashuRecoveryError.value = false
  try {
    const saved = readCashuAttempt(props.peerId || currentPeer.value?.onion || '', item.id)
    if (saved) { cashuQuote.value = saved.quote; cashuRecoveryRequired.value = true }
  } catch { cashuRecoveryError.value = true }
  payItem.value = item
  payMode.value = 'choose'
  qrTab.value = 'onchain'
  invoiceData.value = null
  invoiceQr.value = ''
  invoiceWaiting.value = false
  invoiceError.value = ''
  invoiceCopied.value = false
  onchainData.value = null
  onchainQr.value = ''
  onchainWaiting.value = false
  onchainError.value = ''
  onchainCopied.value = false
  lnPaying.value = false
  lnError.value = ''
  lnReceipt.value = null
  lnReceiptReadError.value = false
  try {
    lnReceipt.value = readReceipt(props.peerId || currentPeer.value?.onion || '', item.id)
    if (lnReceipt.value?.state === 'failed') lnError.value = `Previous Lightning attempt failed: ${lnReceipt.value.failure_reason || 'Payment failed'}. You can choose another method.`
  } catch { lnReceiptReadError.value = true; lnError.value = 'Saved payment could not be read. Do not pay again.' }
  onchainPaying.value = false
  cashuLookup = lookupCashuPurchase(props.peerId || currentPeer.value?.onion || '', item, paymentGeneration.value)
}

function closePayModal() {
  paymentGeneration.value++
  if (invoicePollTimer) { clearTimeout(invoicePollTimer); invoicePollTimer = null }
  if (onchainPollTimer) { clearTimeout(onchainPollTimer); onchainPollTimer = null }
  payItem.value = null
  payMode.value = 'choose'
  ecashPlan.value = null
  cashuQuote.value = null
  cashuRecoveryRequired.value = false
  cashuRecoveryError.value = false
  ecashPreparing.value = false
  invoiceWaiting.value = false
  onchainWaiting.value = false
  onchainPaying.value = false
}

/**
 * Open the "pay from another wallet" view: tabbed QR like the wallet's
 * Send/Receive modal, defaulting to the on-chain tab (so a QR is shown
 * immediately for any external wallet).
 */
function openQrPay() {
  payMode.value = 'qr'
  if (hasBlockingLightningReceipt.value) { qrTab.value = 'lightning'; void payWithInvoice(); return }
  payMode.value = 'qr'
  invoiceData.value = null
  invoiceQr.value = ''
  invoiceError.value = ''
  invoiceWaiting.value = false
  onchainData.value = null
  onchainQr.value = ''
  onchainError.value = ''
  // Start on the first tab the seller actually accepts.
  if (payItem.value && (!acceptsMethod(payItem.value.access, 'onchain') || getItemPrice(payItem.value.access) < 546)) {
    qrTab.value = 'lightning'
    payWithInvoice()
  } else {
    qrTab.value = 'onchain'
    loadOnchainQr()
  }
}

/** Switch QR tab, lazily loading that method's QR the first time it's shown and
 * resuming its payment poll if it was already loaded (so switching back and
 * forth doesn't silently stop watching for payment). */
function selectQrTab(tab: 'onchain' | 'lightning') {
  if (qrTab.value === tab) return
  if (tab === 'onchain' && hasBlockingLightningReceipt.value) {
    invoiceError.value = 'A Lightning payment is saved. Recover it before choosing another payment method.'
    return
  }
  qrTab.value = tab
  if (tab === 'onchain') {
    if (invoicePollTimer) { clearTimeout(invoicePollTimer); invoicePollTimer = null }
    if (!onchainData.value && !onchainWaiting.value) {
      loadOnchainQr()
    } else if (onchainData.value && !onchainPaying.value) {
      onchainPaying.value = true
      pollOnchain(onchainData.value.address)
    }
  } else {
    if (onchainPollTimer) { clearTimeout(onchainPollTimer); onchainPollTimer = null }
    onchainPaying.value = false
    if (!invoiceData.value && !invoiceWaiting.value) {
      payWithInvoice()
    } else if (invoiceData.value) {
      scheduleInvoicePoll()
    }
  }
}

/**
 * On-chain QR: ask the seller for a fresh address + amount, render a
 * `bitcoin:` QR for any external wallet, and poll the seller until the payment
 * lands, then release the file (the address is the gate token).
 */
async function loadOnchainQr() {
  if (hasBlockingLightningReceipt.value) return
  const item = payItem.value
  const onion = props.peerId || currentPeer.value?.onion
  if (!item || !onion) return
  if (!await permitFreshOtherRail(item, onion)) return
  if (getItemPrice(item.access) < 546) { onchainError.value = 'On-chain payment requires at least 546 sats. Choose Lightning or ecash for this file.'; return }
  const operation = paymentOperations.begin('onchain-qr', onion, item.id)
  if (!operation) return
  onchainError.value = ''
  onchainData.value = null
  onchainQr.value = ''
  onchainWaiting.value = true
  try {
    const req = await rpcClient.call<{ address?: string; amount_sats?: number; error?: string }>({ method: 'content.request-onchain', params: { onion, content_id: item.id }, timeout: 45000, maxRetries: 1 })
    if (!req?.address || !req?.amount_sats) throw new Error(req?.error || 'The seller could not provide an on-chain address.')
    if (!Number.isSafeInteger(req.amount_sats) || req.amount_sats !== getItemPrice(item.access) || req.amount_sats < 546) throw new Error('The seller changed the payment amount. Refresh the file before paying.')
    // A closed modal has never displayed this address: do not expose a late QR.
    if (!paymentOperations.selected(operation)) return
    let image = ''
    try { image = await QRCode.toDataURL(`bitcoin:${req.address}?amount=${(req.amount_sats / 1e8).toFixed(8)}`, { margin: 1, width: 240 }) } catch { /* raw address is available */ }
    if (!paymentOperations.selected(operation)) return
    onchainData.value = { address: req.address, amount_sats: req.amount_sats }
    onchainQr.value = image
    onchainPaying.value = true
    void pollOnchain(req.address)
  } catch (error) {
    if (paymentOperations.selected(operation)) onchainError.value = error instanceof Error ? error.message : 'Could not request an on-chain address'
  } finally { if (paymentOperations.finish(operation)) onchainWaiting.value = false }
}

async function copyOnchain() {
  const original = onchainData.value
  const generation = paymentGeneration.value
  if (!original) return
  const selected = () => generation === paymentGeneration.value && onchainData.value === original && payItem.value !== null
  try {
    await navigator.clipboard.writeText(original.address)
    if (!selected()) return
    onchainCopied.value = true
    setTimeout(() => { if (selected()) onchainCopied.value = false }, 1500)
  } catch { /* clipboard denied */ }
}

/**
 * Pay on-chain from THIS node's wallet: ask the seller for a fresh address +
 * amount, broadcast with lnd.sendcoins, then poll the seller until it detects
 * the payment and release the file (address is the gate token). Slower than LN
 * because the seller waits for the tx to appear/confirm.
 */
async function payOnchain() {
  const item = payItem.value
  const onion = props.peerId || currentPeer.value?.onion
  if (!item || !onion || paymentActionBusy.value) return
  if (!await permitFreshOtherRail(item, onion)) return
  if (hasBlockingLightningReceipt.value) { lnError.value = 'Check the saved Lightning attempt before choosing another method.'; return }
  if (getItemPrice(item.access) < 546) { lnError.value = 'On-chain payment requires at least 546 sats. Choose Lightning or ecash for this file.'; return }
  const operation = paymentOperations.begin('onchain-send', onion, item.id)
  if (!operation) return
  onchainPaying.value = true
  lnError.value = ''
  let polling = false
  try {
    const req = await rpcClient.call<{ address?: string; amount_sats?: number; error?: string }>({ method: 'content.request-onchain', params: { onion, content_id: item.id }, timeout: 45000, maxRetries: 1 })
    if (!req?.address || !req?.amount_sats) throw new Error(req?.error || 'The seller could not provide an on-chain address.')
    if (!Number.isSafeInteger(req.amount_sats) || req.amount_sats !== getItemPrice(item.access) || req.amount_sats < 546) throw new Error('The seller changed the payment amount. Refresh the file before paying.')
    if (!paymentOperations.selected(operation)) return
    const send = await rpcClient.call<{ txid?: string; error?: string }>({ method: 'lnd.sendcoins', params: { addr: req.address, amount: req.amount_sats }, timeout: 60000, maxRetries: 1 })
    if (!send?.txid) throw new Error(send?.error || 'The on-chain result is unconfirmed. Check this wallet transaction before sending again.')
    if (!paymentOperations.selected(operation)) return
    polling = true
    void pollOnchain(req.address)
  } catch (error) {
    if (paymentOperations.selected(operation)) lnError.value = error instanceof Error ? error.message : 'Could not confirm on-chain payment'
  } finally {
    if (paymentOperations.finish(operation) && !polling) onchainPaying.value = false
  }
}

type OnchainPollScope = { item: CatalogItem; onion: string; address: string; generation: number }
function onchainScopeSelected(scope: OnchainPollScope) {
  return paymentGeneration.value === scope.generation && activePaymentMatches(scope.onion, scope.item.id)
}
async function pollOnchain(address: string, original?: OnchainPollScope) {
  const item = original?.item || payItem.value
  const onion = original?.onion || props.peerId || currentPeer.value?.onion
  if (!item || !onion) return
  const scope = original || { item, onion, address, generation: paymentGeneration.value }
  if (!onchainScopeSelected(scope)) return
  try {
    const res = await rpcClient.call<{ paid?: boolean }>({ method: 'content.onchain-status', params: { onion, content_id: item.id, address: scope.address }, timeout: 30000 })
    if (!onchainScopeSelected(scope)) return
    if (res?.paid === true) {
      const dl = await rpcClient.call<{ data?: string; owned?: boolean; owned_content_id?: string; mime_type?: string; error?: string }>({
        method: 'content.download-peer-onchain', params: { onion, content_id: item.id, address: scope.address, filename: item.filename, price_sats: getItemPrice(item.access), cache_only: true }, timeout: 960000, maxRetries: 1,
      })
      if (!onchainScopeSelected(scope)) return
      onchainPaying.value = false
      if (dl?.data !== undefined || dl?.owned === true) {
        openPurchased(item, dl.data, dl.mime_type, onion, dl.owned_content_id)
      } else lnError.value = dl?.error || 'Payment is confirmed; delivery is still recoverable with this address.'
      return
    }
  } catch { /* Retry read-only status for the original item only. */ }
  if (onchainScopeSelected(scope) && onchainPaying.value) onchainPollTimer = setTimeout(() => pollOnchain(scope.address, scope), 5000)
}

/** Spendable balance for a given ecash backend in the current plan. */
function ecashBalanceOf(b: EcashBackend): number {
  if (!ecashPlan.value) return 0
  return b === 'cashu' ? ecashPlan.value.cashu : b === 'fedimint' ? ecashPlan.value.fedimint : ecashPlan.value.ark
}

/**
 * Step 1b: look at BOTH ecash balances, pick the backend that covers the price
 * (Cashu preferred, else Fedimint), and show a confirmation screen so the user
 * sees exactly which wallet is spent and can switch before committing (#3).
 */
async function prepareEcashPay() {
  const item = payItem.value
  const onion = props.peerId || currentPeer.value?.onion
  if (!item || !onion || paymentActionBusy.value) return
  if (hasBlockingLightningReceipt.value) { lnError.value = 'Check the saved Lightning attempt before choosing another method.'; return }
  const generation = paymentGeneration.value
  await cashuLookup
  if (paymentGeneration.value !== generation || !activePaymentMatches(onion, item.id)) return
  const operation = paymentOperations.begin('prepare-ecash', onion, item.id)
  if (!operation) return
  const price = getItemPrice(item.access)
  ecashPreparing.value = true
  purchaseError.value = null
  try {
    let cashu = 0
    let fedimint = 0
    let ark = 0
    try {
      const res = await rpcClient.call<{ cashu_sats?: number; fedimint_sats?: number; ark_sats?: number; total_sats?: number; balance_sats?: number }>({
        method: 'wallet.ecash-balance',
      })
      cashu = res?.cashu_sats ?? res?.balance_sats ?? 0
      fedimint = res?.fedimint_sats ?? 0
      ark = res?.ark_sats ?? 0
    } catch {
      // Couldn't read balances — let the user try anyway (auto backend).
    }
    if (!paymentOperations.selected(operation)) return
    const total = cashu + fedimint + ark
    // Prefer Cashu when it covers the price, else Fedimint, else Ark, else
    // leave null (insufficient — shown in the confirm screen, Confirm disabled).
    const chosen: EcashBackend | null =
      acceptsMethod(item.access, 'ecash') || hasBlockingCashuPurchase.value ? 'cashu' : fedimint >= price ? 'fedimint' : ark >= price ? 'ark' : null
    ecashPlan.value = { cashu, fedimint, ark, total, chosen }
    if (!chosen) {
      purchaseError.value = `Not enough funds: Cashu ${cashu} + Fedimint ${fedimint} + Ark ${ark} sats, need ${price}. Fund a wallet, or pay another way.`
    }
    payMode.value = 'ecash-confirm'
    if (chosen === 'cashu') await requestCashuPurchase(item, onion, operation, false)
  } catch (error) {
    if (paymentOperations.selected(operation)) purchaseError.value = error instanceof Error ? error.message : 'Could not recover the Cashu purchase'
  } finally {
    if (paymentOperations.finish(operation)) ecashPreparing.value = false
  }
}

/**
 * Shared post-payment success handling for ALL payment rails (ecash,
 * lightning-from-node, external invoice QR, on-chain): mark the item owned,
 * open it in-app — audio autoplays in the global bottom-bar player,
 * image/video open in the viewer modal — and refresh the owned list.
 * A browser download used to fire here instead, which silently fails on the
 * mobile companion ("paid but never unlocked"); the viewer's Save button
 * still offers an explicit download.
 */
function openPurchased(item: CatalogItem, base64Data: string | undefined, mimeType?: string, seller?: string, cachedId?: string) {
  const onion = seller || props.peerId || currentPeer.value?.onion
  const url = base64Data !== undefined
    ? URL.createObjectURL(base64ToBlob(base64Data, mimeType || item.mime_type))
    : `/api/peer-content/${encodeURIComponent(onion || "")}/${encodeURIComponent(cachedId || item.id)}`
  if (onion) {
    try { localStorage.removeItem(receiptKey(onion, item.id)) } catch { /* owned cache remains authoritative */ }
    lnReceipt.value = null
  }
  if (onion) ownedKeys.value = new Set(ownedKeys.value).add(ownKey(onion, item.id))
  const mime = mimeType || item.mime_type
  if (mime.startsWith('audio/')) {
    // Straight to the bottom-bar player — the blob URL intentionally stays
    // alive while the bar owns playback.
    audioPlayer.play(
      url,
      item.filename.split('/').pop() || item.filename,
    )
  } else {
    releaseViewerUrl()
    viewerUrl.value = url
    viewerMime.value = mime
    viewerItem.value = item
  }
  closePayModal()
  void loadOwned()
}

type CashuPurchaseReply = Partial<Omit<CashuQuote, 'state'>> & { state?: string; owned?: boolean; owned_content_id?: string; mime_type?: string; error?: string }
async function requestCashuPurchase(item: CatalogItem, onion: string, operation: NonNullable<ReturnType<typeof paymentOperations.begin>>, confirm: boolean) {
  const selected = () => paymentOperations.selected(operation)
  let saved: ReturnType<typeof readCashuAttempt> = null
  let unreadable = false
  try { saved = readCashuAttempt(onion, item.id) } catch { unreadable = true }
  // A corrupt browser marker may query/recover node-owned state, but cannot
  // supply consent or authorize a newly selected payment.
  const quote = unreadable ? null : saved?.quote || (selected() ? cashuQuote.value : null)
  if (confirm && quote) keepCashuAttempt(onion, item.id, quote, true)
  const result = await rpcClient.call<CashuPurchaseReply>({
    method: 'content.purchase',
    params: { onion, content_id: item.id, filename: item.filename,
      max_wallet_debit: confirm && quote ? quote.wallet_debit_sats : Number.MAX_SAFE_INTEGER,
      ...(confirm && quote ? { consent: { operation_id: quote.operation_id, envelope_sha256: quote.envelope_sha256, wallet_debit_sats: quote.wallet_debit_sats } } : {}) },
    timeout: 960000, maxRetries: 1,
  })
  if (result?.state === 'confirmation_required') {
    const next = parseCashuQuote(result)
    keepAuthoritativeCashuQuote(onion, item.id, next)
    if (selected()) { cashuQuote.value = next; cashuRecoveryRequired.value = true; cashuRecoveryError.value = false }
    return
  }
  if (result?.state === 'delivered' && result.owned === true && result.owned_content_id) {
    archiveMalformedCashuAttempt(onion, item.id)
    clearCashuAttempt(onion, item.id)
    if (selected()) openPurchased(item, undefined, result.mime_type, onion, result.owned_content_id)
    return
  }
  if (result?.state === 'cancelled_unspent') {
    archiveMalformedCashuAttempt(onion, item.id)
    clearCashuAttempt(onion, item.id)
    if (selected()) { cashuQuote.value = null; cashuRecoveryRequired.value = false; cashuRecoveryError.value = false; payMode.value = 'choose' }
    return
  }
  throw new Error(result?.error || 'The original Cashu purchase is not confirmed yet. Retry recovers it; do not pay using another method.')
}
async function selectEcashBackend(backend: EcashBackend) {
  if (!ecashPlan.value || paymentActionBusy.value) return
  if (backend !== 'cashu' && hasBlockingCashuPurchase.value) { purchaseError.value = 'Cancel the original unpaid Cashu quote before selecting another wallet.'; return }
  ecashPlan.value.chosen = backend
  if (backend === 'cashu') await prepareEcashPay()
}
async function cancelCashuPurchase() {
  const item = payItem.value
  const onion = props.peerId || currentPeer.value?.onion
  const quote = cashuQuote.value
  if (!item || !onion || !quote || paymentActionBusy.value) return
  const generation = paymentGeneration.value
  await cashuLookup
  if (paymentGeneration.value !== generation || !activePaymentMatches(onion,item.id)) return
  const operation = paymentOperations.begin('cashu-cancel', onion, item.id)
  if (!operation) return
  try {
    const result = await rpcClient.call<{state?: string;operation_id?: string}>({ method:'content.cancel-purchase',
      params:{onion,operation_id:quote.operation_id}, timeout:60000,maxRetries:1 })
    if (result?.state !== 'cancelled_unspent' || result.operation_id !== quote.operation_id) throw new Error('Cancellation is not confirmed. Recover the original purchase before paying another way.')
    clearCashuAttempt(onion,item.id)
    if (paymentOperations.selected(operation)) {
      cashuQuote.value=null;cashuRecoveryRequired.value=false;cashuRecoveryError.value=false
      purchaseError.value=null;payMode.value='choose'
    }
  } catch (error) {
    if (paymentOperations.selected(operation)) purchaseError.value=error instanceof Error?error.message:'Could not confirm cancellation'
  } finally { paymentOperations.finish(operation) }
}

/** Confirm the ecash payment with the backend the user selected. */
async function confirmEcashPay() {
  const item = payItem.value
  const onion = props.peerId || currentPeer.value?.onion
  const method = ecashPlan.value?.chosen
  if (!item || !onion || !method || paymentActionBusy.value || hasBlockingLightningReceipt.value) return
  if (method !== 'cashu' && !await permitFreshOtherRail(item, onion)) return
  const operation = paymentOperations.begin('ecash-send', onion, item.id)
  if (!operation) return
  const selected = () => paymentOperations.selected(operation)
  const price = getItemPrice(item.access)

  purchaseError.value = null
  try {
    if (method === 'cashu') { await requestCashuPurchase(item, onion, operation, true); return }
    if (hasBlockingCashuPurchase.value) throw new Error('Recover or cancel the saved Cashu purchase first.')
    const result = await rpcClient.call<{ data?: string; owned?: boolean; owned_content_id?: string; error?: string; ecash_backend?: string; mime_type?: string }>({
      method: 'content.download-peer-paid',
      params: { onion, content_id: item.id, price_sats: price, method, filename: item.filename, cache_only: true },
      timeout: 960000, maxRetries: 1,
    })
    if (result?.data !== undefined || result?.owned === true) {
      // The purchase is now cached + owned by this node (backend persisted it).
      if (selected()) openPurchased(item, result.data, result.mime_type, onion, result.owned_content_id)
    } else if (selected() && result?.error) {
      // Preserve the backend's recovery explanation; an error is not proof of nonpayment.
      purchaseError.value = result.error
    }
  } catch (e: unknown) {
    if (selected()) purchaseError.value = e instanceof Error ? e.message : 'Download failed'
  } finally {
    paymentOperations.finish(operation)
  }
}

/** Ask the seller for a Lightning invoice, render a QR, and poll for payment. */
async function payWithInvoice() {
  const item = payItem.value
  const onion = props.peerId || currentPeer.value?.onion
  if (!item || !onion) return
  if (!await permitFreshOtherRail(item, onion)) return
  const operation = paymentOperations.begin('invoice', onion, item.id)
  if (!operation) return
  payMode.value = 'qr'
  invoiceError.value = ''
  invoiceWaiting.value = true
  try {
    const saved = readReceipt(onion, item.id)
    const res = (saved?.state === 'failed' ? null : saved) as (LightningReceipt & { error?: string }) | null || await rpcClient.call<{ bolt11?: string; payment_hash?: string; price_sats?: number; error?: string }>({
      method: 'content.request-invoice', params: { onion, content_id: item.id }, timeout: 45000, maxRetries: 1,
    })
    if (!res?.bolt11 || !res?.payment_hash) throw new Error(res?.error || 'The seller could not create an invoice.')
    const receipt: LightningReceipt = { bolt11: res.bolt11, payment_hash: res.payment_hash, price_sats: res.price_sats ?? getItemPrice(item.access), state: 'pending' }
    // Preserve this request even if the modal closed; never expose it in a new modal.
    localStorage.setItem(receiptKey(onion, item.id), JSON.stringify(receipt))
    if (!paymentOperations.selected(operation)) return
    lnReceipt.value = receipt
    invoiceData.value = receipt
    let image = ''
    try { image = await QRCode.toDataURL(res.bolt11.toUpperCase(), { margin: 1, width: 240 }) } catch { /* raw invoice remains usable */ }
    if (!paymentOperations.selected(operation)) return
    invoiceQr.value = image
    scheduleInvoicePoll()
  } catch (error) {
    if (paymentOperations.selected(operation)) invoiceError.value = error instanceof Error ? error.message : 'Could not request an invoice'
  } finally {
    if (paymentOperations.finish(operation)) invoiceWaiting.value = false
  }
}

/**
 * Pay the seller's invoice straight from THIS node's Lightning wallet, then
 * release the file. Keep the invoice before payment so uncertain outcomes can
 * retry seller verification and delivery without sending a second payment.
 */
async function payWithLightning() {
  const item = payItem.value
  const onion = props.peerId || currentPeer.value?.onion
  if (!item || !onion || paymentActionBusy.value || lnReceiptReadError.value) return
  if (!await permitFreshOtherRail(item, onion)) return
  const operation = paymentOperations.begin('lightning', onion, item.id)
  if (!operation) return
  const selected = () => paymentOperations.selected(operation)
  lnPaying.value = true
  lnError.value = ''
  let inv: LightningReceipt | null = null
  try {
    inv = readReceipt(onion, item.id)
    if (inv && inv.state !== 'failed') {
      const state = await recoverFailedLightningAttempt(onion, item.id, inv, selected)
      if (state === 'failed') return
      if (state === 'pending') {
        if (selected()) lnError.value = 'Payment is still settling. You can close this window; check this saved attempt later without sending more sats.'
        return
      }
    }
    if (!selected()) return
    if (!inv || inv.state === 'failed') {
      const result = await rpcClient.call<{ bolt11?: string; payment_hash?: string; error?: string }>({
        method: 'content.request-invoice', params: { onion, content_id: item.id }, timeout: 45000, maxRetries: 1,
      })
      if (!result?.bolt11 || !result.payment_hash) throw new Error(result?.error || 'The seller could not create an invoice.')
      if (!selected()) return
      inv = { bolt11: result.bolt11, payment_hash: result.payment_hash, price_sats: getItemPrice(item.access), state: 'pending' }
      keepReceipt(onion, item.id, inv, selected)
      const pay = await rpcClient.payLightningInvoice({ payment_request: inv.bolt11 })
      if (pay.status === 'failed') {
        keepFailedLightningAttempt(onion, item.id, inv, pay.failure_reason || 'Payment failed', selected)
        return
      }
      if (pay.status !== 'succeeded') {
        if (selected()) lnError.value = 'Payment is still settling. Retry checks this payment without sending more sats.'
        return
      }
      inv = { ...inv, state: 'succeeded' }
      keepReceipt(onion, item.id, inv, selected)
    }
    const dl = await rpcClient.call<{ data?: string; owned?: boolean; owned_content_id?: string; mime_type?: string; error?: string }>({
      method: 'content.download-peer-invoice',
      params: { onion, content_id: item.id, payment_hash: inv.payment_hash, filename: item.filename, price_sats: inv.price_sats, cache_only: true },
      timeout: 960000,
    })
    if (!selected()) return
    if (dl?.data !== undefined || dl?.owned === true) openPurchased(item, dl.data, dl.mime_type, onion)
    else lnError.value = dl?.error || 'Download unavailable. Retry uses this payment without sending more sats.'
  } catch (e: unknown) {
    if (inv && inv.state !== 'failed' && inv.state !== 'succeeded' && await recoverFailedLightningAttempt(onion, item.id, inv, selected) === 'failed') return
    if (selected()) lnError.value = (e instanceof Error ? e.message : 'Payment or download could not be confirmed') + ' Retry checks the saved payment; do not pay again.'
  } finally { if (paymentOperations.finish(operation)) lnPaying.value = false }
}

type InvoicePollScope = { item: CatalogItem; onion: string; invoice: LightningReceipt; generation: number }
function invoiceScopeSelected(scope: InvoicePollScope) {
  return paymentGeneration.value === scope.generation && activePaymentMatches(scope.onion, scope.item.id)
    && invoiceData.value?.payment_hash === scope.invoice.payment_hash
}
function scheduleInvoicePoll(scope?: InvoicePollScope) {
  if (!scope) {
    const item = payItem.value
    const onion = props.peerId || currentPeer.value?.onion
    const invoice = invoiceData.value
    if (!item || !onion || !invoice) return
    scope = { item, onion, invoice, generation: paymentGeneration.value }
  }
  if (!invoiceScopeSelected(scope)) return
  if (invoicePollTimer) clearTimeout(invoicePollTimer)
  const original = scope
  invoicePollTimer = setTimeout(() => pollInvoice(original), 1000)
}
async function pollInvoice(scope: InvoicePollScope) {
  if (!invoiceScopeSelected(scope)) return
  const { item, onion, invoice: inv } = scope
  try {
    const res = await rpcClient.call<InvoiceLifecycle>({
      method: 'content.invoice-status',
      params: { onion, content_id: item.id, payment_hash: inv.payment_hash, filename: item.filename, price_sats: inv.price_sats, cache_only: true }, timeout: 30000,
    })
    if (!invoiceScopeSelected(scope)) return
    if (keepCanceledInvoice(onion, item.id, inv, res, () => invoiceScopeSelected(scope))) {
      invoiceWaiting.value = false
      invoiceData.value = null
      invoiceQr.value = ''
      payMode.value = 'choose'
      return
    }
    if (res?.paid === true) {
      localStorage.setItem(receiptKey(onion, item.id), JSON.stringify({ ...inv, state: 'succeeded' }))
      const dl = await rpcClient.call<{ data?: string; owned?: boolean; owned_content_id?: string; mime_type?: string; error?: string }>({
        method: 'content.download-peer-invoice',
        params: { onion, content_id: item.id, payment_hash: inv.payment_hash, filename: item.filename, price_sats: inv.price_sats, cache_only: true }, timeout: 960000, maxRetries: 1,
      })
      if (!invoiceScopeSelected(scope)) return
      if (dl?.data !== undefined || dl?.owned === true) openPurchased(item, dl.data, dl.mime_type, onion, dl.owned_content_id)
      else invoiceError.value = dl?.error || 'Payment is confirmed; delivery is still recoverable with this invoice.'
      return
    }
  } catch { /* A transient status failure never permits a new payment. */ }
  if (invoiceScopeSelected(scope)) scheduleInvoicePoll(scope)
}

async function copyInvoice() {
  const original = invoiceData.value
  const generation = paymentGeneration.value
  if (!original) return
  const selected = () => generation === paymentGeneration.value && invoiceData.value === original && payItem.value !== null
  try {
    await navigator.clipboard.writeText(original.bolt11)
    if (!selected()) return
    invoiceCopied.value = true
    setTimeout(() => { if (selected()) invoiceCopied.value = false }, 2000)
  } catch { /* clipboard denied */ }
}

/** Play audio/video inline. For free items, downloads full file; for paid, uses the preview. */
async function playMedia(item: CatalogItem) {
  const onion = props.peerId || currentPeer.value?.onion
  if (!onion) return

  const paid = isPaidItem(item.access)

  // Free content: stream via the Range-capable proxy (B3) so the player can
  // seek and start instantly, instead of downloading the whole file as a
  // base64 blob into a non-seekable Blob URL (which broke video/large audio).
  if (!paid) {
    const streamUrl = `/api/peer-content/${encodeURIComponent(onion)}/${encodeURIComponent(item.id)}`
    if (item.mime_type.startsWith('audio/')) {
      audioPlayer.play(streamUrl, item.filename.split('/').pop() || item.filename)
    } else if (item.mime_type.startsWith('video/')) {
      videoPlayerItem.value = item
      videoPlayerUrl.value = streamUrl
      videoPlayerPaid.value = false
    }
    return
  }

  // Paid content: use the preview/download flow below.
  // If we already have a preview blob URL, use it
  const existingUrl = previewUrls[item.id]
  if (existingUrl) {
    if (item.mime_type.startsWith('audio/')) {
      audioPlayer.play(existingUrl, item.filename.split('/').pop() || item.filename)
    } else if (item.mime_type.startsWith('video/')) {
      videoPlayerItem.value = item
      videoPlayerUrl.value = existingUrl
      videoPlayerPaid.value = paid
    }
    return
  }

  // Download the content (preview for paid, full for free)
  playing.value = item.id
  try {
    const method = paid ? 'content.preview-peer' : 'content.download-peer'
    const result = await rpcClient.call<{ data?: string; content_type?: string }>({
      method,
      params: { onion, content_id: item.id },
      timeout: 120000,
    })

    if (result?.data) {
      const mime = result.content_type || item.mime_type
      const bytes = Uint8Array.from(atob(result.data), c => c.charCodeAt(0))
      const blob = new Blob([bytes], { type: mime })
      const blobUrl = URL.createObjectURL(blob)
      previewUrls[item.id] = blobUrl

      if (item.mime_type.startsWith('audio/')) {
        audioPlayer.play(blobUrl, item.filename.split('/').pop() || item.filename)
      } else if (item.mime_type.startsWith('video/')) {
        videoPlayerItem.value = item
        videoPlayerUrl.value = blobUrl
        videoPlayerPaid.value = paid
      }
    } else {
      purchaseError.value = 'No preview data returned — peer may be offline'
    }
  } catch (e: unknown) {
    purchaseError.value = e instanceof Error ? e.message : 'Failed to load media — peer may be offline'
  } finally {
    playing.value = null
  }
}

function closeVideoPlayer() {
  videoPlayerItem.value = null
  videoPlayerUrl.value = null
  videoPlayerPaid.value = false
  videoLoading.value = false
  videoError.value = false
}

// Show the loader the moment a video opens; the element's playing/canplay/error
// events clear it.
watch(videoPlayerUrl, (url) => {
  if (url) {
    videoLoading.value = true
    videoError.value = false
  }
})

/**
 * Probe the streaming proxy with a 1-byte Range request so we can report the
 * real failure reason (peer offline on both mesh and Tor) before kicking off a
 * browser-managed download that can't surface HTTP status. Returns `true` on
 * success, or a human-readable error string. Aborts immediately on success so
 * a large body is never drained.
 */
async function probePeerContent(url: string): Promise<true | string> {
  const controller = new AbortController()
  try {
    const res = await fetch(url, {
      headers: { Range: 'bytes=0-0' },
      signal: controller.signal,
    })
    if (res.ok || res.status === 206) {
      controller.abort()
      return true
    }
    let msg = `Peer returned an error (${res.status})`
    try {
      const body = await res.json()
      if (body?.error) msg = String(body.error)
    } catch {
      // non-JSON error body — keep the status-based message
    }
    return msg
  } catch (e) {
    return e instanceof Error ? e.message : 'Could not reach the peer — it may be offline'
  }
}

/**
 * Stream a free file to disk via the Range-capable proxy. The browser manages
 * the transfer with constant memory (no base64, no in-memory Blob), so large
 * files download reliably (#38). Same-origin, so the session cookie rides along.
 */
function streamDownload(url: string, item: CatalogItem) {
  const a = document.createElement('a')
  a.href = url
  a.download = item.filename.split('/').pop() || item.filename
  document.body.appendChild(a)
  a.click()
  a.remove()
}

function triggerDownload(base64Data: string, item: CatalogItem) {
  const blob = new Blob(
    [Uint8Array.from(atob(base64Data), c => c.charCodeAt(0))],
    { type: item.mime_type },
  )
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = item.filename.split('/').pop() || item.filename
  a.click()
  URL.revokeObjectURL(url)
}

defineExpose({ loadCatalog })
</script>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
