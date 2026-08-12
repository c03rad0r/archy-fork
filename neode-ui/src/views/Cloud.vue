<template>
  <div class="apps-view pb-6">

    <!-- Nav header — tabs + categories + search, matching the Apps layout -->
    <div class="mb-4">
      <!-- Desktop: page tabs + category tabs + search on one row -->
      <div class="app-header-desktop items-center gap-4">
        <div class="flex-shrink-0">
          <div class="mode-switcher hidden md:inline-flex">
            <button
              v-for="tab in TABS"
              :key="tab.id"
              class="mode-switcher-btn"
              :class="{ 'mode-switcher-btn-active': activeTab === tab.id }"
              @click="activeTab = tab.id"
            >{{ tab.name }}</button>
          </div>
        </div>
        <div v-show="showCategories" class="mode-switcher category-tabs-wide hidden md:inline-flex">
          <button
            v-for="category in CATEGORIES"
            :key="category.id"
            @click="selectedCategory = category.id"
            class="mode-switcher-btn"
            :class="{ 'mode-switcher-btn-active': selectedCategory === category.id }"
          >{{ category.name }}</button>
        </div>
        <div class="app-header-search-wrap flex items-center gap-2">
          <input
            v-model="searchQuery"
            type="text"
            placeholder="Search your files and peer files…"
            aria-label="Search files"
            data-controller-no-submit
            class="app-header-search min-w-0 flex-1 text-white placeholder-white/50 focus:outline-none transition-colors"
          />
        </div>
      </div>

      <!-- Mobile: full-width tab switcher (distinct from the category pills),
           category pill strip, then search. .mobile-category-strip opts the
           pills out of the dashboard's swipe-to-switch-page gesture. -->
      <div class="app-header-mobile mb-4">
        <div class="cloud-tab-switcher cloud-tab-switcher-full mb-3">
          <button
            v-for="tab in TABS"
            :key="tab.id"
            @click="activeTab = tab.id"
            class="cloud-tab-btn"
            :class="{ 'cloud-tab-btn-active': activeTab === tab.id }"
            type="button"
          >{{ tab.name }}</button>
        </div>
        <div v-if="showCategories" class="mobile-category-strip mb-3" aria-label="File categories">
          <button
            v-for="category in CATEGORIES"
            :key="category.id"
            @click="selectedCategory = category.id"
            class="mobile-category-pill"
            :class="{ 'mobile-category-pill-active': selectedCategory === category.id }"
            type="button"
          >{{ category.name }}</button>
        </div>
        <input
          v-model="searchQuery"
          type="text"
          placeholder="Search your files and peer files…"
          aria-label="Search files"
          data-controller-no-submit
          class="app-header-search w-full min-w-0 text-white placeholder-white/50 focus:outline-none transition-colors"
        />
      </div>
    </div>

    <!-- ═════════════ Search results (any tab, when a query is active) ═════════════ -->
    <div v-if="searchActive">
      <div v-if="searching" class="glass-card p-8 text-center text-white/50 text-sm flex items-center justify-center gap-3">
        <svg class="animate-spin h-4 w-4" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Searching your files and peers…
      </div>
      <template v-else>
        <div v-if="filteredSearchResults.length === 0" class="glass-card p-8 text-center text-white/40 text-sm">
          No files match “{{ searchQuery }}”.
        </div>
        <div v-else class="cloud-file-list">
          <template v-for="r in filteredSearchResults" :key="r.key">
            <!-- Own files act like real file rows: actions + click opens the file -->
            <FileCard
              v-if="r.item"
              :item="r.item"
              @delete="handleDelete"
              @share="handleShare"
              @play="handlePlay"
              @preview="(p: string) => handlePreview(p, searchMineItems)"
            />
            <button
              v-else
              class="w-full glass-card px-4 py-3 flex items-center gap-3 text-left hover:bg-white/10 transition-colors"
              @click="router.push({ name: 'peer-files', params: { peerId: r.peerOnion } })"
            >
              <span class="w-9 h-9 rounded-lg flex items-center justify-center shrink-0" :class="categoryMeta(r.category).iconBg">
                <svg class="w-5 h-5" :class="categoryMeta(r.category).iconColor" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path v-for="(p, i) in categoryMeta(r.category).iconPaths" :key="i" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="p" />
                </svg>
              </span>
              <span class="flex-1 min-w-0">
                <span class="block text-sm text-white truncate">{{ r.name }}</span>
                <span class="block text-[11px] text-white/40 truncate">{{ r.detail }}</span>
              </span>
              <span class="text-[10px] px-2 py-0.5 rounded-full bg-purple-500/15 text-purple-400 shrink-0">{{ r.peerName }}</span>
            </button>
          </template>
        </div>
      </template>
    </div>

    <!-- ═════════════ My Files — every own file, flat, with real file actions ═════════════ -->
    <div v-else-if="activeTab === 'mine'">
      <div v-if="myFilesLoading" class="glass-card p-8 text-center text-white/50 text-sm flex items-center justify-center gap-3">
        <svg class="animate-spin h-4 w-4" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Loading your files…
      </div>
      <div v-else-if="!fileBrowserRunning" class="glass-card p-8 text-center">
        <p class="text-white/60 mb-3">Install File Browser from the App Store to get started with your cloud storage.</p>
        <RouterLink to="/dashboard/marketplace" class="glass-button inline-flex items-center gap-2 px-5 py-2.5 rounded-lg text-sm font-medium">
          Open App Store
        </RouterLink>
      </div>
      <div v-else-if="filteredMyFiles.length === 0" class="glass-card p-8 text-center text-white/40 text-sm">
        {{ selectedCategory === 'all' ? 'No files yet — upload some from the Folders tab.' : 'No files in this category.' }}
      </div>
      <div v-else class="cloud-file-list">
        <FileCard
          v-for="item in filteredMyFiles"
          :key="item.path"
          :item="item"
          @delete="handleDelete"
          @share="handleShare"
          @play="handlePlay"
          @preview="(p: string) => handlePreview(p, filteredMyFiles)"
        />
      </div>
    </div>

    <!-- ═════════════ Paid Files tab — everything this node has purchased ═════════════
         Source of truth is the purchase cache (content.owned-list): filename,
         type, price paid and when — the file itself was also auto-filed into
         Photos/Music/Documents at purchase time (2026-07-22). -->
    <div v-else-if="activeTab === 'paid'">
      <div v-if="paidLoading" class="glass-card p-8 text-center text-white/50 text-sm">Loading purchases…</div>
      <div v-else-if="paidItems.length === 0" class="glass-card p-8 text-center text-white/40 text-sm">
        Nothing purchased yet — files you buy from peers appear here and are saved into your folders automatically.
      </div>
      <div v-else class="space-y-2">
        <div
          v-for="it in paidItems"
          :key="it.onion + it.content_id"
          class="glass-card p-3 flex items-center gap-3 transition-colors"
          :class="paidItemViewer.opening.value === paidItemKey(it) ? 'cursor-default' : 'cursor-pointer hover:bg-white/5'"
          @click="paidItemViewer.opening.value === paidItemKey(it) ? null : viewPaidItem(it)"
        >
          <span class="text-xl shrink-0">{{ it.mime_type.startsWith('image/') ? '🖼️' : it.mime_type.startsWith('video/') ? '🎬' : it.mime_type.startsWith('audio/') ? '🎵' : '📄' }}</span>
          <div class="min-w-0 flex-1">
            <p class="text-sm text-white/90 truncate">{{ it.filename.split('/').pop() }}</p>
            <p class="text-[11px] text-white/40">
              {{ (it.size_bytes / 1024).toFixed(0) }} KB ·
              <span class="text-orange-300/80">{{ it.paid_sats.toLocaleString() }} sats</span>
              <span v-if="it.purchased_at"> · {{ new Date(it.purchased_at).toLocaleDateString() }}</span>
            </p>
          </div>
          <span
            v-if="paidItemViewer.opening.value === paidItemKey(it)"
            class="text-[10px] px-2 py-0.5 rounded-full bg-white/10 text-white/60 shrink-0 flex items-center gap-1.5"
          >
            <span class="w-3 h-3 border-2 border-white/20 border-t-white/80 rounded-full animate-spin"></span>
            Opening…
          </span>
          <span v-else class="text-[10px] px-2 py-0.5 rounded-full bg-emerald-400/15 text-emerald-300 shrink-0">Paid</span>
        </div>
      </div>
    </div>

    <!-- ═════════════ Peer Files tab — every file shared by every peer ═════════════ -->
    <div v-else-if="activeTab === 'peers'">
      <div v-if="peerFilesLoading" class="glass-card p-8 text-center text-white/50 text-sm flex items-center justify-center gap-3">
        <svg class="animate-spin h-4 w-4" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Fetching files from {{ peerNodes.length || '' }} peer{{ peerNodes.length === 1 ? '' : 's' }}…
      </div>
      <template v-else>
        <div v-if="peerNodes.length === 0" class="glass-card p-8 text-center">
          <p class="text-white/60 mb-3">No peers yet. Set up federation to browse files shared by other nodes.</p>
          <RouterLink to="/dashboard/server/federation" class="glass-button inline-flex items-center gap-2 px-5 py-2.5 rounded-lg text-sm font-medium">
            Open Federation
          </RouterLink>
        </div>
        <div v-else-if="filteredPeerFiles.length === 0 && peerFilesPending === 0" class="glass-card p-8 text-center text-white/40 text-sm">
          {{ selectedCategory === 'all' ? 'Your peers are not sharing any files yet.' : 'No peer files in this category.' }}
        </div>
        <div v-else class="space-y-2">
          <button
            v-for="f in filteredPeerFiles"
            :key="f.key"
            class="w-full glass-card px-4 py-3 flex items-center gap-3 text-left hover:bg-white/10 transition-colors"
            @click="router.push({ name: 'peer-files', params: { peerId: f.peerOnion } })"
          >
            <span class="w-9 h-9 rounded-lg flex items-center justify-center shrink-0" :class="categoryMeta(f.category).iconBg">
              <svg class="w-5 h-5" :class="categoryMeta(f.category).iconColor" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path v-for="(p, i) in categoryMeta(f.category).iconPaths" :key="i" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="p" />
              </svg>
            </span>
            <span class="flex-1 min-w-0">
              <span class="block text-sm text-white truncate">{{ f.filename }}</span>
              <span class="block text-[11px] text-white/40 truncate">{{ formatSize(f.sizeBytes) }}<template v-if="f.priceSats"> · {{ f.priceSats.toLocaleString() }} sats</template></span>
            </span>
            <span class="text-[10px] px-2 py-0.5 rounded-full bg-purple-500/15 text-purple-400 shrink-0">{{ f.peerName }}</span>
          </button>
        </div>
        <p v-if="peerFilesPending > 0" class="text-[11px] text-white/35 text-center mt-3 flex items-center justify-center gap-2">
          <svg class="animate-spin h-3 w-3" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
          </svg>
          Still fetching from {{ peerFilesPending }} peer{{ peerFilesPending === 1 ? '' : 's' }}…
        </p>
        <p v-if="peerFilesErrors > 0" class="text-[11px] text-white/35 text-center mt-3">
          {{ peerFilesErrors }} peer{{ peerFilesErrors === 1 ? '' : 's' }} unreachable — showing what answered.
        </p>
      </template>
    </div>

    <!-- ═════════════ Folders — section (+ peer) cards ═════════════ -->
    <div v-else class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
      <div
        v-for="section in contentSections"
        :key="section.id"
        data-controller-container
        tabindex="0"
        class="glass-card p-6 cursor-pointer transition-all hover:-translate-y-1 hover:bg-white/10"
        @click="openSection(section)"
      >
        <div class="flex items-center gap-4 mb-4">
          <div class="flex-shrink-0 w-12 h-12 rounded-xl flex items-center justify-center" :class="section.iconBg">
            <svg class="w-7 h-7" :class="section.iconColor" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                v-for="(path, index) in section.iconPaths"
                :key="index"
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                :d="path"
              />
            </svg>
          </div>
          <div class="flex-1 min-w-0">
            <h3 class="text-lg font-semibold text-white mb-0.5 truncate">{{ section.name }}</h3>
            <p class="text-xs text-white/50">{{ section.description }}</p>
          </div>
          <!-- Arrow indicator -->
          <svg class="w-5 h-5 text-white/30" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
        </div>

        <!-- App status -->
        <div class="flex items-center gap-2 text-xs">
          <span
            class="inline-flex items-center gap-1.5 px-2 py-1 rounded-full"
            :class="isAppRunning(section.appId) ? 'bg-green-500/15 text-green-400' : 'bg-white/5 text-white/40'"
          >
            <span class="w-1.5 h-1.5 rounded-full" :class="isAppRunning(section.appId) ? 'bg-green-400' : 'bg-white/30'"></span>
            {{ section.appLabel }}
          </span>
          <span v-if="!isAppRunning(section.appId)" class="text-white/30">Not installed</span>
          <span v-else-if="countsLoading" class="text-white/30 animate-pulse">Loading...</span>
          <span v-else-if="sectionCounts[section.id] !== undefined" class="text-white/30">{{ sectionCounts[section.id] }} items</span>
        </div>
      </div>

      <!-- Individual Peer Cards -->
      <div
        v-for="peer in peerNodes"
        :key="peer.did"
        data-controller-container
        tabindex="0"
        class="glass-card p-6 cursor-pointer transition-all hover:-translate-y-1 hover:bg-white/10"
        @click="router.push({ name: 'peer-files', params: { peerId: peer.onion } })"
      >
        <div class="flex items-center gap-4 mb-4">
          <div class="flex-shrink-0 w-12 h-12 rounded-xl flex items-center justify-center bg-purple-500/15">
            <svg class="w-7 h-7 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2" />
            </svg>
          </div>
          <div class="flex-1 min-w-0">
            <h3 class="text-lg font-semibold text-white mb-0.5 truncate" :title="peer.did">{{ peer.name || peerDisplayName(peer.did) }}</h3>
            <p class="text-xs text-white/40 truncate">{{ peer.name ? peer.did.slice(0, 20) + '...' : 'Peer node' }}</p>
          </div>
          <svg class="w-5 h-5 text-white/30" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
        </div>
        <!-- flex-wrap (UIFIX-01): this row has no horizontal give at phone
             widths. Without wrapping, a longer trust label squeezes the
             transport badge until ITS OWN text breaks mid-label ("TOR ·" /
             "120.0s") — measured at 320px, the narrowest width we support.
             Wrapping drops the transport badge to a second line intact
             instead. It is inert whenever the row already fits, so every
             desktop width renders exactly as before. -->
        <div class="flex flex-wrap items-center gap-2 text-xs">
          <span
            class="inline-flex items-center gap-1.5 px-2 py-1 rounded-full"
            :class="peer.trust_level === 'trusted' ? 'bg-green-500/15 text-green-400' : 'bg-purple-500/15 text-purple-400'"
          >
            <span class="w-1.5 h-1.5 rounded-full" :class="peer.trust_level === 'trusted' ? 'bg-green-400' : 'bg-purple-400'"></span>
            {{ peer.trust_level }}
          </span>
          <!-- Live transport badge — which route actually served the last
               browse (FIPS = direct mesh, fast; Tor = fallback, slow).
               shrink-0 keeps the transport WORD whole: this badge is the
               security signal a phone user reads, so it wraps to the next
               line at full size rather than being compressed into a
               two-line stub (UIFIX-01). -->
          <span
            v-if="peerTransport(peer.onion)"
            class="inline-flex shrink-0 items-center gap-1.5 px-2 py-1 rounded-full"
            :class="peerTransport(peer.onion)!.transport === 'fips' ? 'bg-emerald-500/15 text-emerald-300' : 'bg-amber-500/15 text-amber-300'"
            :title="`Last browse served via ${peerTransport(peer.onion)!.transport.toUpperCase()}`"
          >
            <span class="w-1.5 h-1.5 rounded-full" :class="peerTransport(peer.onion)!.transport === 'fips' ? 'bg-emerald-400' : 'bg-amber-400'"></span>
            {{ peerTransport(peer.onion)!.transport.toUpperCase() }} · {{ (peerTransport(peer.onion)!.latencyMs / 1000).toFixed(1) }}s
          </span>
          <span v-else class="text-white/30">Peer Node</span>
        </div>
      </div>

      <div
        v-if="(peersLoading || peersRefreshing) && peerNodes.length > 0"
        class="glass-card p-3 text-center text-white/45 text-xs md:col-span-2 lg:col-span-3 flex items-center justify-center gap-2"
      >
        <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Refreshing peer nodes...
      </div>

      <!-- No Peers placeholder (only if no peers found) -->
      <div
        v-if="!peersLoading && peerNodes.length === 0"
        data-controller-container
        tabindex="0"
        class="glass-card p-6 cursor-pointer transition-all hover:-translate-y-1 hover:bg-white/10"
        @click="router.push('/dashboard/server/federation')"
      >
        <div class="flex items-center gap-4 mb-4">
          <div class="flex-shrink-0 w-12 h-12 rounded-xl flex items-center justify-center bg-purple-500/15">
            <svg class="w-7 h-7 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9" />
            </svg>
          </div>
          <div class="flex-1 min-w-0">
            <h3 class="text-lg font-semibold text-white mb-0.5 truncate">Peer Files</h3>
            <p class="text-xs text-white/50">Set up federation to share files with peers</p>
          </div>
          <svg class="w-5 h-5 text-white/30" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
        </div>
        <div class="flex items-center gap-2 text-xs">
          <span class="inline-flex items-center gap-1.5 px-2 py-1 rounded-full bg-white/5 text-white/40">
            <span class="w-1.5 h-1.5 rounded-full bg-white/30"></span>
            No peers yet
          </span>
        </div>
      </div>
    </div>

    <!-- Error State -->
    <div v-if="loadError" class="alert-error mt-4">
      {{ loadError }}
    </div>

    <!-- Not Installed Hint (Folders tab only — My Files has its own) -->
    <div v-if="!fileBrowserRunning && !searchActive && activeTab === 'folders'" class="glass-card p-8 mt-6 text-center">
      <p class="text-white/60 mb-3">Install File Browser from the App Store to get started with your cloud storage.</p>
      <RouterLink to="/dashboard/marketplace" class="glass-button inline-flex items-center gap-2 px-5 py-2.5 rounded-lg text-sm font-medium">
        Open App Store
      </RouterLink>
    </div>

    <!-- Share with peers -->
    <ShareModal
      v-if="shareTarget"
      :filename="shareTarget.name"
      :filepath="shareTarget.path"
      :is-dir="shareTarget.isDir"
      @close="shareTarget = null"
      @saved="shareTarget = null"
    />

    <!-- Media viewer for own files -->
    <MediaLightbox
      v-if="lightboxIndex !== null"
      :items="lightboxItems"
      :start-index="lightboxIndex"
      :show="lightboxIndex !== null"
      :fetch-blob-url="cloudStore.fetchBlobUrl"
      :stream-url="cloudStore.streamUrl"
      @close="lightboxIndex = null"
    />

    <!-- Media viewer for purchased items (UIFIX-04) — synthetic items fed by
         usePaidItemViewer, resolved from the already-fetched blob URL. -->
    <MediaLightbox
      v-if="paidItemViewer.lightboxIndex.value !== null"
      :items="paidItemViewer.lightboxItems.value"
      :start-index="paidItemViewer.lightboxIndex.value"
      :show="paidItemViewer.lightboxIndex.value !== null"
      :fetch-blob-url="paidItemViewer.resolveBlobUrl"
      :stream-url="paidItemViewer.resolveBlobUrl"
      @close="paidItemViewer.closeLightbox()"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch, onActivated, onMounted, onUnmounted } from 'vue'
import { useRouter, RouterLink } from 'vue-router'
import { useAppStore } from '../stores/app'
import { useCloudStore } from '../stores/cloud'
import { useResourcesStore } from '../stores/resources'
import { useCachedResource } from '../composables/useCachedResource'
import { fileBrowserClient, type FileBrowserItem } from '@/api/filebrowser-client'
import { rpcClient } from '@/api/rpc-client'
import { getFileCategory } from '../composables/useFileType'
import { useAudioPlayer } from '../composables/useAudioPlayer'
import { usePaidItemViewer, paidItemKey } from '../composables/usePaidItemViewer'
import FileCard from '../components/cloud/FileCard.vue'
import ShareModal from '../components/cloud/ShareModal.vue'
import MediaLightbox from '../components/cloud/MediaLightbox.vue'

const router = useRouter()
const store = useAppStore()
const cloudStore = useCloudStore()
const resources = useResourcesStore()
const audioPlayer = useAudioPlayer()

// Section counts — cached: revisits render the last-known counts instantly
// and refresh in the background (sticky-ready never regresses to "Loading").
const countsResource = useCachedResource<Record<string, number>>({
  key: 'cloud.section-counts',
  fetcher: fetchCounts,
  ttlMs: 30_000,
  persist: true, // bare per-section item counts — no file names/content/identity
  immediate: false, // gated on fileBrowserRunning; kicked from onMounted/watch
})
const sectionCounts = computed(() => countsResource.entry.data ?? {})
const countsLoading = computed(() => countsResource.entry.loadState === 'loading')

// ── Tabs / categories / search state ────────────────────────────────────────
type TabId = 'folders' | 'mine' | 'peers' | 'paid'
type CategoryId = 'all' | 'photos' | 'music' | 'documents'

const TABS: Array<{ id: TabId; name: string }> = [
  { id: 'folders', name: 'Folders' },
  { id: 'mine', name: 'My Files' },
  { id: 'peers', name: 'Peer Files' },
  { id: 'paid', name: 'Paid Files' },
]
const CATEGORIES: Array<{ id: CategoryId; name: string }> = [
  { id: 'all', name: 'All' },
  { id: 'photos', name: 'Photos & Video' },
  { id: 'music', name: 'Music' },
  { id: 'documents', name: 'Documents' },
]

const activeTab = ref<TabId>('folders')

// ── Paid Files tab ──────────────────────────────────────────────────────────
interface PaidItem { onion: string; content_id: string; filename: string; mime_type: string; size_bytes: number; paid_sats: number; purchased_at: string }
const paidResource = useCachedResource<PaidItem[]>({
  key: 'cloud.paid-items',
  fetcher: async (signal) => {
    const res = await rpcClient.call<{ items: PaidItem[] }>({ method: 'content.owned-list', signal, dedup: true })
    return (res.items || []).slice().reverse()
  },
  persist: false, // PaidItem carries paid_sats + purchase history — financial data (T-02-01)
  immediate: false, // loaded when the Paid tab is opened
})
const paidItems = computed(() => paidResource.entry.data ?? [])
const paidLoading = computed(() => paidResource.entry.loadState === 'loading')
function loadPaidItems() {
  return paidResource.refresh()
}
// Fetch, decode, route-to-viewer state for a purchased item (UIFIX-04
// lightbox routing + UIFIX-06 loading/error surfacing). See the composable
// for the mime-routing rules and URL-ownership contract.
const paidItemViewer = usePaidItemViewer()
async function viewPaidItem(it: PaidItem) {
  await paidItemViewer.open(it)
  if (paidItemViewer.error.value) loadError.value = paidItemViewer.error.value
}
watch(activeTab, (t) => { if (t === 'paid') void loadPaidItems() })

const selectedCategory = ref<CategoryId>('all')
const searchQuery = ref('')
const searchActive = computed(() => searchQuery.value.trim().length > 0)
// Categories narrow file LISTS; the Folders tab is already organized by kind.
const showCategories = computed(() => activeTab.value !== 'folders' || searchActive.value)

interface PeerNode {
  did: string
  pubkey: string
  onion: string
  name?: string
  trust_level: string
}

// Federation peers — cached so the Folders tab's peer cards paint instantly
// on revisit while the list revalidates behind them.
const peersResource = useCachedResource<PeerNode[]>({
  key: 'cloud.peer-nodes',
  fetcher: async (signal) => {
    const result = await rpcClient.federationListNodes()
    void signal
    return result?.nodes ?? []
  },
  ttlMs: 30_000,
  persist: false, // PeerNode carries did/pubkey/onion — peer identity payload (T-02-01)
  immediate: false, // kicked from onMounted (keeps the legacy load order)
})
const peerNodes = computed(() => peersResource.entry.data ?? [])
const peersLoading = computed(() => peersResource.entry.loadState === 'loading')
const peersRefreshing = computed(() => peersResource.entry.loadState === 'refreshing')
const loadError = ref('')

const APP_ALIASES: Record<string, string[]> = {
  immich: ['immich_server', 'immich-server'],
  nextcloud: ['nextcloud-aio', 'nextcloud-server'],
}

function isAppRunning(appId: string): boolean {
  const packages = store.packages
  if (packages[appId]?.state === 'running') return true
  const aliases = APP_ALIASES[appId]
  if (aliases) {
    for (const alias of aliases) {
      if (packages[alias]?.state === 'running') return true
    }
  }
  return false
}

const fileBrowserRunning = computed(() => isAppRunning('filebrowser'))

interface ContentSection {
  id: string
  name: string
  description: string
  appId: string
  appLabel: string
  iconPaths: string[]
  iconBg: string
  iconColor: string
}

const contentSections: ContentSection[] = [
  {
    id: 'photos',
    name: 'Photos & Videos',
    description: 'Auto-backup & browse your media',
    appId: 'filebrowser',
    appLabel: 'File Browser',
    iconPaths: ['M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z'],
    iconBg: 'bg-blue-500/15',
    iconColor: 'text-blue-400',
  },
  {
    id: 'music',
    name: 'Music',
    description: 'Your music collection',
    appId: 'filebrowser',
    appLabel: 'File Browser',
    iconPaths: ['M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3'],
    iconBg: 'bg-orange-500/15',
    iconColor: 'text-orange-400',
  },
  {
    id: 'documents',
    name: 'Documents',
    description: 'Files, docs & spreadsheets',
    appId: 'filebrowser',
    appLabel: 'File Browser',
    iconPaths: ['M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z'],
    iconBg: 'bg-green-500/15',
    iconColor: 'text-green-400',
  },
  {
    id: 'files',
    name: 'All Files',
    description: 'Browse your server file system',
    appId: 'filebrowser',
    appLabel: 'File Browser',
    iconPaths: ['M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z'],
    iconBg: 'bg-white/10',
    iconColor: 'text-white/70',
  },
]

const SECTION_PATHS: Record<string, string> = {
  photos: '/Photos',
  music: '/Music',
  documents: '/Documents',
  files: '/',
}

// ── Category helpers ─────────────────────────────────────────────────────────
function categoryOf(nameOrMime: string): Exclude<CategoryId, 'all'> {
  const s = nameOrMime.toLowerCase()
  if (s.startsWith('image/') || s.startsWith('video/') || /\.(jpe?g|png|gif|webp|heic|svg|mp4|mov|mkv|webm|avi)$/.test(s)) return 'photos'
  if (s.startsWith('audio/') || /\.(mp3|flac|wav|ogg|m4a|aac|opus)$/.test(s)) return 'music'
  return 'documents'
}

const FALLBACK_SECTION: ContentSection = contentSections[2]!
function categoryMeta(cat: Exclude<CategoryId, 'all'>): ContentSection {
  return contentSections.find(s => s.id === cat) ?? FALLBACK_SECTION
}

function formatSize(bytes: number): string {
  if (!bytes) return '—'
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
}

// ── My Files (flat list of every own file across the sections) ──────────────
// Cached: revisiting the tab renders the last walk instantly and re-walks in
// the background only when stale.
const myFilesResource = useCachedResource<FileBrowserItem[]>({
  key: 'cloud.my-files',
  fetcher: fetchMyFiles,
  ttlMs: 60_000,
  // Fail-safe choice, not an obvious money/identity/peer-identity case: a
  // flat listing of the user's own filenames/paths is private content, not
  // squarely one of the three T-02-01 categories. Flagged for human review
  // rather than defaulted to persist:true.
  persist: false,
  immediate: false, // loaded when the My Files tab (or search) needs it
})
const myFiles = computed(() => myFilesResource.entry.data ?? [])
const myFilesLoading = computed(() => myFilesResource.entry.loadState === 'loading')

/** Depth-limited walk of the section folders; flat file list, capped. */
async function fetchMyFiles(): Promise<FileBrowserItem[]> {
  if (!fileBrowserRunning.value) return []
  const ok = await cloudStore.init()
  if (!ok) return []
  const out: FileBrowserItem[] = []
  for (const [sectionId, root] of Object.entries(SECTION_PATHS)) {
    if (sectionId === 'files') continue // '/' would double-visit the sections
    const queue: Array<{ path: string; depth: number }> = [{ path: root, depth: 0 }]
    while (queue.length > 0 && out.length < 500) {
      const { path, depth } = queue.shift()!
      let items: FileBrowserItem[]
      try { items = await fileBrowserClient.listDirectory(path) } catch { continue }
      for (const item of items) {
        const itemPath = item.path || `${path.replace(/\/$/, '')}/${item.name}`
        if (item.isDir) {
          if (depth < 3) queue.push({ path: itemPath, depth: depth + 1 })
        } else {
          out.push({ ...item, path: itemPath })
        }
      }
    }
  }
  out.sort((a, b) => a.name.localeCompare(b.name))
  return out
}

/** Load if never fetched or stale; `force` always re-walks. */
function loadMyFiles(force = false): Promise<void> {
  if (force) return myFilesResource.refresh()
  if (myFilesResource.entry.data === null || myFilesResource.isStale.value) {
    return myFilesResource.refresh()
  }
  return Promise.resolve()
}

const filteredMyFiles = computed(() =>
  selectedCategory.value === 'all'
    ? myFiles.value
    : myFiles.value.filter(f => categoryOf(f.name) === selectedCategory.value),
)

watch(activeTab, (tab) => {
  if (tab === 'mine') void loadMyFiles()
  if (tab === 'peers') void loadPeerFiles()
  if (tab === 'folders') selectedCategory.value = 'all' // no hidden filter behind the cards
})

// ── File actions shared by My Files rows and own-file search results ────────
const shareTarget = ref<{ path: string; name: string; isDir: boolean } | null>(null)
const lightboxIndex = ref<number | null>(null)
const lightboxItems = ref<FileBrowserItem[]>([])

function handleShare(path: string, name: string, isDir: boolean) {
  shareTarget.value = { path, name, isDir }
}

/** Music opens in the global bottom-bar player (GlobalAudioPlayer in App.vue). */
async function handlePlay(path: string, name: string) {
  const url = await cloudStore.streamUrl(path)
  audioPlayer.play(url, name)
}

function handlePreview(path: string, context: FileBrowserItem[]) {
  // Audio never opens the lightbox — it belongs to the bottom-bar player.
  const clicked = context.find(item => item.path === path)
  if (clicked) {
    const ext = clicked.name.includes('.') ? clicked.name.split('.').pop()!.toLowerCase() : ''
    if (getFileCategory(ext, clicked.isDir) === 'audio') {
      void handlePlay(path, clicked.name)
      return
    }
  }
  // MediaLightbox filters to media internally; index within that filtered list.
  const mediaItems = context.filter(item => {
    const ext = item.name.includes('.') ? item.name.split('.').pop()!.toLowerCase() : ''
    const cat = getFileCategory(ext, item.isDir)
    return cat === 'image' || cat === 'video' || cat === 'audio'
  })
  const idx = mediaItems.findIndex(item => item.path === path)
  lightboxItems.value = context
  lightboxIndex.value = idx >= 0 ? idx : 0
}

async function handleDelete(path: string) {
  try {
    await cloudStore.deleteItem(path)
    // Delete confirmed — update the cache in place (no rollback needed).
    myFilesResource.optimistic((cur) => (cur ?? []).filter(f => f.path !== path))
    searchResults.value = searchResults.value.filter(r => r.item?.path !== path)
  } catch (e) {
    loadError.value = e instanceof Error ? e.message : 'Delete failed'
  }
}

// ── Peer files (aggregated across every federation peer) ────────────────────
interface PeerFileEntry {
  key: string
  filename: string
  sizeBytes: number
  priceSats: number
  category: Exclude<CategoryId, 'all'>
  peerName: string
  peerOnion: string
}

interface CatalogItem {
  id: string
  filename: string
  mime_type: string
  size_bytes: number
  description: string
  access: string | { paid: { price_sats: number } }
}

function priceOf(access: CatalogItem['access']): number {
  return typeof access === 'object' && access?.paid ? access.paid.price_sats : 0
}

// Per-peer browse results live as individual cached entries so (a) each
// peer's card/rows render the moment THAT peer answers — no more blocking on
// the slowest peer via Promise.allSettled — and (b) revisits paint from
// cache. The response's `transport` (fips/tor) + measured latency ride
// along, giving every peer a live transport badge.
interface PeerBrowse {
  items: CatalogItem[]
  transport: string | null
  latencyMs: number
}

const peerBrowseKey = (onion: string) => `cloud.peer-browse:${onion}`

// 02-08 fix: content.browse-peer has no server-side timeout for an
// unreachable/dead .onion peer — confirmed on real hardware that 13 of 14
// concurrent calls to this method (one per connected peer, previously fired
// with zero concurrency cap from loadPeerFiles below) never settled at all
// within 15s+. That many simultaneously-open, indefinitely-pending same-
// origin requests starved Chromium's connection pool and silently broke
// every OTHER same-origin fetch for the rest of the session — including the
// lazy route chunk any later tab/folder navigation needs (the actual cause
// behind "no folders open on click" after a first Cloud visit). A single
// dead peer's own RPC still costs at most this timeout once (maxRetries: 1
// already prevented the ×3 retry-storm version of this bug); the fix here
// is bounding how many of these can be simultaneously in flight and how
// long any one of them is allowed to hold a connection open, mirroring the
// PREVIEW_CONCURRENCY pattern PeerFiles.vue already uses for the identical
// class of problem (content.preview-peer fan-out).
const BROWSE_PEER_TIMEOUT_MS = 10_000
const browsePeerAborter = new AbortController()
onUnmounted(() => browsePeerAborter.abort())

function browsePeer(peer: PeerNode): Promise<void> {
  return resources.refresh<PeerBrowse>(peerBrowseKey(peer.onion), async () => {
    const t0 = Date.now()
    const res = await rpcClient.call<{ items?: CatalogItem[]; transport?: string }>({
      method: 'content.browse-peer',
      params: { onion: peer.onion },
      timeout: BROWSE_PEER_TIMEOUT_MS,
      // One slow/unreachable peer must cost its timeout ONCE, not ×3 —
      // the retry loop is why one dead peer meant a 90s spinner.
      maxRetries: 1,
      dedup: true,
      signal: browsePeerAborter.signal,
    })
    return { items: res?.items ?? [], transport: res?.transport ?? null, latencyMs: Date.now() - t0 }
  }, { persist: true }) // matches peerBrowseEntry()'s explicit persist:true for this same key
  // A timed-out/failed peer resolves through resources.ts's own catch path
  // (loadState -> 'error', no throw here) — it never blocks or delays any
  // other peer's slot, and the UI already surfaces this silently (the
  // muted "N peers unreachable — showing what answered" line below, no
  // toast) rather than as an error the user must dismiss.
}

/** Concurrency-capped content.browse-peer fan-out — bounds how many of
 * these can be simultaneously in flight (see BROWSE_PEER_TIMEOUT_MS note
 * above). Matches PeerFiles.vue's PREVIEW_CONCURRENCY convention. */
const BROWSE_PEER_CONCURRENCY = 3
async function runBrowsePeerPool(targets: PeerNode[]): Promise<void> {
  let cursor = 0
  async function worker(): Promise<void> {
    while (cursor < targets.length) {
      const peer = targets[cursor++]!
      await browsePeer(peer).catch(() => {}) // errors already land in the resource's own error state
    }
  }
  const workerCount = Math.min(BROWSE_PEER_CONCURRENCY, targets.length)
  await Promise.all(Array.from({ length: workerCount }, () => worker()))
}

function peerBrowseEntry(onion: string) {
  // persist:true matches this key's existing resources.refresh() calls
  // above, which never pass `{ persist: false }` either — WR-04 just makes
  // that pre-existing decision explicit instead of relying on a default.
  return resources.entry<PeerBrowse>(peerBrowseKey(onion), true)
}

/** Transport badge data for a peer (null until its first browse resolves). */
function peerTransport(onion: string): { transport: string; latencyMs: number } | null {
  const e = peerBrowseEntry(onion)
  if (!e.data?.transport) return null
  return { transport: e.data.transport, latencyMs: e.data.latencyMs }
}

/** Aggregated peer files, incrementally updated as each peer resolves. */
const peerFiles = computed<PeerFileEntry[]>(() => {
  const merged: PeerFileEntry[] = []
  for (const peer of peerNodes.value) {
    const e = peerBrowseEntry(peer.onion)
    if (!e.data) continue
    const peerName = peer.name || peerDisplayName(peer.did)
    for (const item of e.data.items) {
      merged.push({
        key: `${peer.onion}:${item.id}`,
        filename: item.filename,
        sizeBytes: item.size_bytes,
        priceSats: priceOf(item.access),
        category: categoryOf(item.mime_type || item.filename),
        peerName,
        peerOnion: peer.onion,
      })
    }
  }
  merged.sort((a, b) => a.filename.localeCompare(b.filename))
  return merged
})

/** Peers still on their first in-flight browse (nothing cached yet). */
const peerFilesPending = computed(() =>
  peerNodes.value.filter(p => {
    const s = peerBrowseEntry(p.onion).loadState
    return s === 'loading' || s === 'idle'
  }).length,
)
/** Peers whose browse failed with no cached data to show. */
const peerFilesErrors = computed(() =>
  peerNodes.value.filter(p => peerBrowseEntry(p.onion).loadState === 'error').length,
)
/** All-or-nothing spinner ONLY when nothing has ever been cached. */
const peerFilesLoading = computed(() =>
  peerNodes.value.length > 0 && peerFiles.value.length === 0 && peerFilesPending.value > 0 && peerFilesErrors.value < peerNodes.value.length,
)

/** Fan out content.browse-peer, capped at BROWSE_PEER_CONCURRENCY at a time;
 * each peer renders as it resolves regardless of the cap (the aggregation
 * above is reactive per-entry, not gated on the whole batch finishing). */
async function loadPeerFiles(force = false) {
  if (peerNodes.value.length === 0) await loadPeers()
  const targets = peerNodes.value.filter(p => {
    if (force) return true
    const e = peerBrowseEntry(p.onion)
    const stale = e.fetchedAt === null || Date.now() - e.fetchedAt > 30_000
    return e.loadState === 'idle' || e.loadState === 'error' ? true : stale
  })
  await runBrowsePeerPool(targets)
}

const filteredPeerFiles = computed(() =>
  selectedCategory.value === 'all'
    ? peerFiles.value
    : peerFiles.value.filter(f => f.category === selectedCategory.value),
)

// ── Search (own files + all peer files) ─────────────────────────────────────
interface SearchResult {
  key: string
  name: string
  detail: string
  category: Exclude<CategoryId, 'all'>
  item?: FileBrowserItem
  peerName?: string
  peerOnion?: string
}

const searching = ref(false)
const searchResults = ref<SearchResult[]>([])
let searchTimer: ReturnType<typeof setTimeout> | null = null
let searchSeq = 0

watch(searchQuery, () => {
  if (searchTimer) clearTimeout(searchTimer)
  if (!searchActive.value) { searchResults.value = []; searching.value = false; return }
  searching.value = true
  searchTimer = setTimeout(() => void runSearch(), 350)
})

async function runSearch() {
  const query = searchQuery.value.trim()
  const seq = ++searchSeq
  searching.value = true
  try {
    // Both corpora are cached flat lists after their first load.
    await Promise.all([loadMyFiles(), loadPeerFiles()])
    if (seq !== searchSeq) return // a newer query superseded this run
    const q = query.toLowerCase()
    const mine: SearchResult[] = myFiles.value
      .filter(f => f.name.toLowerCase().includes(q))
      .map(f => ({
        key: `mine:${f.path}`,
        name: f.name,
        detail: f.path,
        category: categoryOf(f.name),
        item: f,
      }))
    const peers: SearchResult[] = peerFiles.value
      .filter(f => f.filename.toLowerCase().includes(q))
      .map(f => ({
        key: `peer:${f.key}`,
        name: f.filename,
        detail: `${formatSize(f.sizeBytes)}${f.priceSats ? ` · ${f.priceSats.toLocaleString()} sats` : ''}`,
        category: f.category,
        peerName: f.peerName,
        peerOnion: f.peerOnion,
      }))
    searchResults.value = [...mine, ...peers]
  } finally {
    if (seq === searchSeq) searching.value = false
  }
}

const filteredSearchResults = computed(() =>
  selectedCategory.value === 'all'
    ? searchResults.value
    : searchResults.value.filter(r => r.category === selectedCategory.value),
)

/** Own-file items among the current search results (lightbox context). */
const searchMineItems = computed(() =>
  filteredSearchResults.value.flatMap(r => (r.item ? [r.item] : [])),
)

// ── Existing counts / peers loading ──────────────────────────────────────────
async function fetchCounts(): Promise<Record<string, number>> {
  if (!fileBrowserRunning.value) return {}
  const ok = await fileBrowserClient.login()
  if (!ok) throw new Error('File Browser login failed')
  const counts: Record<string, number> = {}
  for (const section of contentSections) {
    const path = SECTION_PATHS[section.id]
    if (!path) continue
    try {
      counts[section.id] = (await fileBrowserClient.listDirectory(path)).length
    } catch {
      counts[section.id] = 0
    }
  }
  return counts
}

function loadCounts() {
  if (countsResource.entry.data === null || countsResource.isStale.value) {
    void countsResource.refresh()
  }
}

// Every-entry: countsResource and peersResource are useCachedResource-backed
// and already self-heal on reactivation via the composable's own TTL-gated
// onActivated hook, but loadPeerFiles's per-peer transport/reachability
// badges bypass useCachedResource entirely (raw resources store via
// browsePeer/peerBrowseEntry) and would otherwise sit frozen for the rest of
// the session once Cloud.vue is kept alive — peer reachability must never
// render from a stale state without revalidating (T-02-13). loadCounts and
// loadPeerFiles are each internally staleness-gated (fetchedAt/TTL checks),
// and loadPeers's forced refresh is deduped against an identical in-flight
// call by resources.ts's own inflight map, so re-running this sequence on
// every activation does no extra RPC when still fresh.
//
// Called from BOTH onMounted and onActivated deliberately: onActivated is a
// no-op outside a <KeepAlive> boundary (confirmed by useCachedResource.ts's
// own comment and by CloudPeersRefresh.test.ts, which mounts this view bare)
// — Vue only fires it automatically on first mount for a component that
// already has a KeepAlive ancestor. onMounted guarantees this still runs
// once for a bare mount.
//
// 02-08 fix (real-hardware regression, not merely a "harmless extra pass"):
// individually each of loadCounts/loadPeers/loadPeerFiles is staleness- or
// inflight-deduped, so the ORIGINAL 02-04 assumption that firing this twice
// back-to-back is free held up per-resource. But the two back-to-back calls
// still fire loadPeerFiles's full content.browse-peer fan-out (one RPC per
// connected peer) essentially twice in the same tick before either pass's
// results land, and layered on top of whatever other KeepAlive-cached view
// happened to be mounting/activating at the same moment (Home/Server's own
// onMounted bursts), this measurably doubles the concurrent same-origin
// request volume at the single riskiest instant in the session — first
// activation. On a test node that volume was large enough to leave one
// in-flight File Browser request permanently stuck (never resolving,
// confirmed via direct reproduction), which then starved the browser's
// per-origin connection pool and silently broke every subsequent
// same-origin fetch for the rest of the session, including the lazy route
// chunks any later tab/folder navigation needs — "no folders open on
// click" was a downstream symptom of that stall, not a router or click-
// handler bug. A fresh-mount guard (the same pattern already used in
// Home.vue/Web5.vue/Mesh.vue/Server.vue) removes the redundant duplicate
// pass on first activation without changing steady-state reactivation
// behavior at all: onActivated still re-syncs normally on every later
// KeepAlive round-trip, exactly as before.
let cloudFreshMount = true
async function syncOnEntry() {
  loadCounts()
  await loadPeers()
  // Warm the per-peer browse cache in the background: peer cards get their
  // FIPS/Tor badge and the Peer Files tab is instant. Staleness-gated, so
  // quick revisits don't refetch.
  void loadPeerFiles()
}
onMounted(() => { void syncOnEntry() })
onActivated(() => {
  if (cloudFreshMount) { cloudFreshMount = false; return }
  void syncOnEntry()
})

// File Browser can finish its startup scan after we mount — pick counts up
// the moment it becomes available instead of showing a permanent blank.
watch(fileBrowserRunning, (running) => {
  if (running) loadCounts()
})

async function loadPeers() {
  await peersResource.refresh()
  // Surface refresh failures in the banner — the cached peer list stays
  // visible either way (keep-last-known-value).
  const e = peersResource.entry
  if (e.error) loadError.value = e.error
}

function peerDisplayName(did: string): string {
  const suffix = did.replace(/^did:key:z6Mk/, '').slice(-6).toUpperCase()
  return `Node-${suffix}`
}

function openSection(section: ContentSection) {
  router.push({ name: 'cloud-folder', params: { folderId: section.id } })
}

defineExpose({ loadPeers })
</script>

<style scoped>
/* Mobile-only top-level tab switcher — deliberately a DIFFERENT look from the
   neutral category pills below it (orange tint). Desktop keeps the standard
   mode-switcher styling. */
.cloud-tab-switcher {
  gap: 2px;
  padding: 3px;
  border-radius: 0.5rem;
  background: rgba(247, 147, 26, 0.08);
  border: 1px solid rgba(247, 147, 26, 0.22);
}

.cloud-tab-btn {
  padding: 0.45rem 0.9rem;
  border-radius: 0.375rem;
  border: none;
  background: transparent;
  color: rgba(255, 255, 255, 0.6);
  font-size: 0.8rem;
  font-weight: 600;
  white-space: nowrap;
  cursor: pointer;
  transition: background-color 0.15s ease, color 0.15s ease;
}

.cloud-tab-btn:hover {
  color: rgba(255, 255, 255, 0.9);
}

.cloud-tab-btn-active {
  background: rgba(247, 147, 26, 0.25);
  color: #ffd9a8;
}

/* Mobile: the three tabs share the full width equally. */
.cloud-tab-switcher-full {
  display: flex;
  width: 100%;
}

.cloud-tab-switcher-full .cloud-tab-btn {
  flex: 1 1 0;
  text-align: center;
  padding: 0.6rem 0.25rem;
}
</style>
