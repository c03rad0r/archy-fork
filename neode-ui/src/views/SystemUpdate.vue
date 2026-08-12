<template>
  <div class="pb-6">
    <BackButton label="Back to Settings" desktop-margin="mb-6" @click="$router.push('/dashboard/settings')" />
    <div class="mb-6">
      <h1 class="text-3xl font-bold text-white mb-2">{{ t('systemUpdate.title') }}</h1>
      <p class="text-white/70">{{ t('systemUpdate.subtitle') }}</p>
    </div>

    <!-- Status message -->
    <div
      v-if="statusMessage"
      class="mb-4 p-3 rounded-lg text-sm"
      :class="statusIsError ? 'bg-red-500/20 text-red-300' : 'bg-green-500/20 text-green-300'"
    >
      {{ statusMessage }}
    </div>

    <!-- Current Version -->
    <div class="glass-card p-6 mb-6">
      <h2 class="text-lg font-semibold text-white mb-4">{{ t('systemUpdate.currentSystem') }}</h2>
      <div class="grid grid-cols-1 sm:grid-cols-3 gap-4">
        <div class="p-4 bg-white/5 rounded-lg">
          <p class="text-xs text-white/60 mb-1">{{ t('common.version') }}</p>
          <p class="text-xl font-bold text-white">{{ $ver(currentVersion) }}</p>
        </div>
        <div class="p-4 bg-white/5 rounded-lg">
          <p class="text-xs text-white/60 mb-1">{{ t('systemUpdate.lastChecked') }}</p>
          <p class="text-sm font-medium text-white">{{ lastCheckDisplay }}</p>
        </div>
        <div class="p-4 bg-white/5 rounded-lg">
          <p class="text-xs text-white/60 mb-1">{{ t('common.status') }}</p>
          <div class="flex items-center gap-2">
            <div class="w-2 h-2 rounded-full" :class="statusDotColor"></div>
            <p class="text-sm font-medium" :class="statusTextColor">{{ statusLabel }}</p>
          </div>
        </div>
      </div>
    </div>

    <!-- Available Update -->
    <div v-if="updateInfo" class="glass-card p-6 mb-6 border border-orange-400/30">
      <div class="flex items-start justify-between mb-4">
        <div>
          <h2 class="text-lg font-semibold text-white">{{ t('systemUpdate.updateAvailable') }}</h2>
          <p class="text-sm text-white/60">Version {{ updateInfo.version }} &mdash; {{ updateInfo.release_date }}</p>
          <p v-if="manifestMirrorLabel" class="text-xs text-white/40 mt-1 flex items-center gap-1.5">
            <svg class="w-3 h-3 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 12h14M12 5l7 7-7 7" />
            </svg>
            <span>Served by <span class="text-white/70">{{ manifestMirrorLabel }}</span></span>
          </p>
        </div>
        <span class="px-3 py-1 bg-orange-500/20 text-orange-400 text-xs font-medium rounded-full">{{ t('systemUpdate.new') }}</span>
      </div>

      <!-- Changelog -->
      <div v-if="updateInfo.changelog.length" class="mb-4">
        <h3 class="text-sm font-medium text-white/80 mb-2">{{ t('systemUpdate.changelog') }}</h3>
        <ul class="space-y-1">
          <li v-for="(entry, i) in updateInfo.changelog" :key="i" class="text-sm text-white/60 flex gap-2">
            <span class="text-orange-400 shrink-0">&bull;</span>
            <span>{{ entry }}</span>
          </li>
        </ul>
      </div>

      <!-- Components -->
      <div v-if="updateInfo.components > 0" class="mb-4 p-3 bg-white/5 rounded-lg">
        <p class="text-xs text-white/60">{{ t('systemUpdate.componentsToUpdate', { count: updateInfo.components }) }}</p>
      </div>

      <!-- Actions -->
      <div class="flex flex-col gap-3 sm:flex-row">
        <!-- Git path: one-shot pull+rebuild+restart -->
        <button
          v-if="updateMethod === 'git' && !applying"
          @click="requestGitApply"
          class="glass-button w-full rounded-lg px-6 py-2 text-sm font-medium bg-orange-500/20 border-orange-400/30 sm:w-auto"
        >
          {{ t('systemUpdate.pullAndRebuild') }}
        </button>
        <!-- Manifest path: download then apply -->
        <button
          v-if="updateMethod !== 'git' && !downloading && !applying && !downloaded"
          @click="downloadUpdate"
          class="glass-button w-full rounded-lg px-6 py-2 text-sm font-medium sm:w-auto"
        >
          {{ t('systemUpdate.downloadUpdate') }}
        </button>
        <button
          v-if="updateMethod !== 'git' && downloaded && !applying"
          @click="requestApply"
          class="glass-button w-full rounded-lg px-6 py-2 text-sm font-medium bg-orange-500/20 border-orange-400/30 sm:w-auto"
        >
          {{ t('systemUpdate.applyUpdate') }}
        </button>
      </div>
      <p v-if="updateMethod === 'git'" class="text-xs text-white/40 mt-3">
        {{ t('systemUpdate.gitMethodHint') }}
      </p>
    </div>

    <!-- No update available -->
    <div v-else-if="!loading" class="glass-card p-6 mb-6">
      <div class="flex items-center gap-3 mb-4">
        <svg class="w-6 h-6 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
        </svg>
        <h2 class="text-lg font-semibold text-white">{{ t('systemUpdate.upToDate') }}</h2>
      </div>
      <p class="text-sm text-white/60">{{ t('systemUpdate.upToDateMessage') }}</p>
    </div>

    <!-- Download Progress -->
    <div v-if="downloading" class="glass-card p-6 mb-6">
      <h2 class="text-lg font-semibold text-white mb-4">{{ t('systemUpdate.downloading') }}</h2>
      <div class="w-full h-3 bg-white/10 rounded-full overflow-hidden mb-2">
        <div
          class="h-full rounded-full transition-all duration-500"
          :class="downloadStalled ? 'bg-amber-400' : 'bg-orange-400'"
          :style="{ width: downloadPercentFormatted + '%' }"
        ></div>
      </div>
      <div class="flex items-center justify-between gap-3 flex-wrap">
        <div class="flex items-center gap-2 min-w-0">
          <div v-if="downloadFinishing && !downloadStalled" class="w-3 h-3 border-2 border-orange-400 border-t-transparent rounded-full animate-spin shrink-0"></div>
          <p class="text-xs" :class="downloadStalled ? 'text-amber-300' : 'text-white/60'">
            {{ downloadStalled
               ? t('systemUpdate.downloadStalled')
               : downloadFinishing
                 ? t('systemUpdate.finishingDownload')
                 : t('systemUpdate.percentComplete', { percent: downloadPercentFormatted }) }}
          </p>
        </div>
        <button
          @click="requestCancelDownload"
          :disabled="cancelingDownload"
          class="glass-button rounded-lg px-4 py-1.5 text-xs font-medium disabled:opacity-40 shrink-0"
          :class="downloadStalled ? 'bg-amber-500/20 border-amber-400/40 text-amber-200' : ''"
        >
          {{ cancelingDownload ? t('systemUpdate.cancelingDownload') : t('systemUpdate.cancelDownload') }}
        </button>
      </div>
    </div>

    <!-- Applying -->
    <div v-if="applying" class="glass-card p-6 mb-6">
      <h2 class="text-lg font-semibold text-white mb-4">{{ t('systemUpdate.applying') }}</h2>
      <div class="flex flex-col items-start gap-3 sm:flex-row sm:items-center">
        <svg
          class="w-5 h-5 shrink-0 animate-spin text-orange-400"
          viewBox="0 0 24 24"
          fill="none"
          aria-hidden="true"
        >
          <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" stroke-opacity="0.2"></circle>
          <path fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
        </svg>
        <p class="text-sm text-white/70">{{ t('systemUpdate.applyWarning') }}</p>
      </div>
    </div>

    <!-- Update Schedule -->
    <div class="glass-card p-6 mb-6">
      <h2 class="text-lg font-semibold text-white mb-2">{{ t('systemUpdate.updateSchedule') }}</h2>
      <p class="text-sm text-white/60 mb-4">{{ t('systemUpdate.subtitle') }}</p>
      <div class="space-y-3">
        <label
          v-for="opt in scheduleOptions"
          :key="opt.value"
          class="flex items-start gap-3 p-3 bg-white/5 rounded-lg cursor-pointer hover:bg-white/10 transition-colors"
          :class="{ 'ring-1 ring-orange-400/50 bg-orange-500/10': schedule === opt.value }"
        >
          <input
            type="radio"
            name="update-schedule"
            :value="opt.value"
            :checked="schedule === opt.value"
            @change="setSchedule(opt.value)"
            class="mt-1 accent-orange-400"
          />
          <div>
            <p class="text-sm font-medium text-white">{{ opt.label }}</p>
            <p class="text-xs text-white/50">{{ opt.description }}</p>
          </div>
        </label>
      </div>
    </div>

    <!-- Update source (origin vs DHT swarm) -->
    <div class="glass-card p-6 mb-6">
      <h2 class="text-lg font-semibold text-white mb-1">Update &amp; app source</h2>
      <p class="text-sm text-white/60 mb-4">
        Where this node fetches updates and apps. <span class="text-white/80">Stable origin</span> is the
        known-good default. <span class="text-white/80">DHT swarm</span> pulls content-addressed blobs from
        peers first and falls back to the origin automatically — use it to live-test the swarm; flip back any time.
      </p>
      <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
        <button
          type="button"
          :disabled="sourceSaving"
          class="text-left p-4 rounded-lg border transition-colors disabled:opacity-60"
          :class="updateSource === 'origin' ? 'border-green-400/40 bg-green-500/10' : 'border-white/10 bg-white/5 hover:bg-white/10'"
          @click="setSource('origin')"
        >
          <div class="flex items-center gap-2 mb-1">
            <div class="w-2.5 h-2.5 rounded-full" :class="updateSource === 'origin' ? 'bg-green-400' : 'bg-white/30'"></div>
            <span class="text-sm font-medium text-white">Stable origin</span>
            <span class="text-[10px] font-mono px-1.5 py-0.5 rounded bg-white/10 text-white/60">DEFAULT</span>
          </div>
          <p class="text-xs text-white/50">HTTP from the mirrors below. Safe, predictable, always available.</p>
        </button>
        <button
          type="button"
          :disabled="sourceSaving"
          class="text-left p-4 rounded-lg border transition-colors disabled:opacity-60"
          :class="updateSource === 'swarm' ? 'border-blue-400/40 bg-blue-500/10' : 'border-white/10 bg-white/5 hover:bg-white/10'"
          @click="setSource('swarm')"
        >
          <div class="flex items-center gap-2 mb-1">
            <div class="w-2.5 h-2.5 rounded-full" :class="updateSource === 'swarm' ? 'bg-blue-400' : 'bg-white/30'"></div>
            <span class="text-sm font-medium text-white">DHT swarm</span>
            <span class="text-[10px] font-mono px-1.5 py-0.5 rounded bg-blue-500/20 text-blue-300">EXPERIMENTAL</span>
          </div>
          <p class="text-xs text-white/50">Peer-to-peer first, origin fallback. For testing the distributed network.</p>
        </button>
      </div>
      <p v-if="updateSource === 'swarm' && !swarmAvailable" class="mt-3 text-xs text-orange-300/90 flex items-start gap-1.5">
        <svg class="w-3.5 h-3.5 shrink-0 mt-px" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
        <span>This build doesn't include the swarm engine, so DHT mode currently behaves exactly like origin. A swarm-enabled build is required for true peer fetching.</span>
      </p>

      <!-- Provide to the swarm (seed/serve to peers) -->
      <div class="mt-5 pt-5 border-t border-white/10 flex items-start justify-between gap-4">
        <div class="min-w-0">
          <p class="text-sm font-medium text-white mb-0.5">Provide to the swarm</p>
          <p class="text-xs text-white/50">
            Let other nodes fetch updates and apps from this one (seeding). On by default so the network has providers.
            Turn off if this node shouldn't serve content to peers — it won't change how this node fetches.
          </p>
        </div>
        <ToggleSwitch :model-value="provideDht" :disabled="sourceSaving" @update:model-value="setProvide" />
      </div>
    </div>

    <!-- Mirrors -->
    <div class="glass-card p-6 mb-6">
      <div class="flex items-start justify-between gap-4 mb-2">
        <h2 class="text-lg font-semibold text-white">Update mirrors</h2>
        <button
          type="button"
          class="text-xs px-3 py-1.5 rounded-md bg-white/5 hover:bg-white/10 text-white/70 hover:text-white transition-colors"
          @click="openAddMirror"
        >+ Add mirror</button>
      </div>
      <p class="text-sm text-white/60 mb-4">
        Sources this node checks for updates. The primary is tried first; if it's slow or unreachable, the next one is tried automatically. Downloads always come from the source that served the manifest — switching primary switches where files come from.
      </p>
      <p class="text-sm text-white/50 mb-4">
        The two built-in entries are the <strong class="text-white/70">same server</strong>, not two servers: the second reaches it by IP without DNS or TLS, which recovers a node whose DNS is broken or whose clock is wrong. It does not help if the server itself is down. Every update is signature-checked whichever source serves it, so an unencrypted fetch can't substitute a tampered build. For real redundancy, add a mirror on a different host.
      </p>
      <ul v-if="mirrors.length" class="space-y-2">
        <li v-for="(m, i) in mirrors" :key="m.url" class="p-3 bg-white/5 rounded-lg">
          <div class="flex items-start gap-3">
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 mb-0.5 flex-wrap">
                <p class="text-sm font-medium text-white truncate">{{ m.label || `Mirror ${i + 1}` }}</p>
                <span v-if="i === 0" class="text-[10px] font-mono px-2 py-0.5 rounded bg-green-500/20 text-green-300">PRIMARY</span>
              </div>
              <p class="text-xs text-white/50 font-mono break-all">{{ m.url }}</p>
            </div>
            <div class="shrink-0 flex items-center gap-1">
              <button
                type="button"
                class="w-8 h-8 flex items-center justify-center rounded-md text-white/60 hover:text-white hover:bg-white/10 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                :disabled="mirrorTests[m.url]?.testing"
                title="Test reachability"
                @click="testMirror(m.url)"
              >
                <svg v-if="mirrorTests[m.url]?.testing" class="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
                  <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" stroke-opacity="0.25"></circle>
                  <path fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
                </svg>
                <svg v-else class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
                </svg>
              </button>
              <button
                v-if="i !== 0"
                type="button"
                class="w-8 h-8 flex items-center justify-center rounded-md text-white/60 hover:text-yellow-300 hover:bg-white/10 transition-colors"
                title="Make primary"
                @click="setPrimaryMirror(m.url)"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z" />
                </svg>
              </button>
              <button
                v-if="mirrors.length > 1"
                type="button"
                class="w-8 h-8 flex items-center justify-center rounded-md text-white/60 hover:text-red-300 hover:bg-red-400/10 transition-colors"
                title="Remove mirror"
                @click="removeMirror(m.url)"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                </svg>
              </button>
            </div>
          </div>
          <div v-if="mirrorTests[m.url] && !mirrorTests[m.url]?.testing" class="mt-2 pt-2 border-t border-white/5 text-xs">
            <span v-if="mirrorTests[m.url]?.reachable" class="inline-flex items-center gap-1.5 text-green-300">
              <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" />
              </svg>
              Reachable &middot; {{ mirrorTests[m.url]?.latency_ms }}ms
            </span>
            <span v-else class="inline-flex items-center gap-1.5 text-red-300">
              <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M6 18L18 6M6 6l12 12" />
              </svg>
              <span class="truncate">{{ mirrorTests[m.url]?.error || 'Unreachable' }}</span>
            </span>
          </div>
        </li>
      </ul>
    </div>

    <!-- Actions row -->
    <div class="glass-card p-6">
      <h2 class="text-lg font-semibold text-white mb-4">{{ t('systemUpdate.actions') }}</h2>
      <div class="flex flex-col gap-3 sm:flex-row sm:flex-wrap">
        <button
          @click="checkForUpdates"
          :disabled="loading"
          class="glass-button w-full rounded-lg px-5 py-2 text-sm font-medium disabled:opacity-40 sm:w-auto"
        >
          {{ loading ? t('systemUpdate.checking') : t('systemUpdate.checkForUpdates') }}
        </button>
        <button
          v-if="rollbackAvailable"
          @click="requestRollback"
          class="glass-button w-full rounded-lg px-5 py-2 text-sm font-medium bg-red-500/10 border-red-400/20 sm:w-auto"
        >
          {{ t('systemUpdate.rollback') }}
        </button>
      </div>
    </div>

    <!-- Install progress overlay — covers the UI while the backend
         swaps files, restarts, and comes back up on the new version.
         Auto-reloads the page as soon as /health reports the target
         version. Styled to match the screensaver (ASCII logo, full-
         screen black). -->
    <Teleport to="body">
      <Transition name="fade">
        <div
          v-if="installing"
          class="fixed inset-0 z-[3000] bg-black flex flex-col items-center justify-center overflow-hidden"
        >
          <!-- Centered ASCII logo — same asset used by the screensaver -->
          <div class="install-overlay-ascii">
            <BitcoinFaceAscii />
          </div>

          <!-- Status text + progress bar underneath -->
          <div class="mt-8 w-[min(520px,80vw)] text-center">
            <h2 class="text-xl font-semibold text-white mb-1">
              {{ installStage === 'applying' ? t('systemUpdate.overlayApplying')
               : installStage === 'restarting' ? t('systemUpdate.overlayRestarting')
               : installStage === 'reconnecting' ? t('systemUpdate.overlayReconnecting')
               : installStage === 'ready' ? t('systemUpdate.overlayReady')
               : t('systemUpdate.overlayStalled') }}
            </h2>
            <p v-if="installTargetVersion" class="text-sm text-white/60 mb-4">
              {{ t('systemUpdate.overlayTarget', { version: installTargetVersion }) }}
            </p>

            <!-- Animated bar: indeterminate stripe while working; full
                 orange when ready; steady at 50% (paused look) when
                 stalled so it reads as "something needs the user". -->
            <div class="w-full h-2 bg-white/10 rounded-full overflow-hidden mb-3 relative">
              <div
                v-if="installStage === 'ready'"
                class="absolute inset-0 bg-green-400"
              ></div>
              <div
                v-else-if="installStage === 'stalled'"
                class="absolute inset-y-0 left-0 w-1/2 bg-orange-400/60"
              ></div>
              <div
                v-else
                class="absolute inset-y-0 w-1/3 bg-orange-400 rounded-full install-overlay-bar-anim"
              ></div>
            </div>

            <p class="text-xs text-white/40">{{ installElapsedLabel }}</p>

            <button
              v-if="installStage === 'stalled'"
              @click="reloadNow"
              class="mt-5 glass-button rounded-lg px-5 py-2 text-sm font-medium bg-orange-500/20 border-orange-400/30"
            >
              {{ t('systemUpdate.overlayReloadNow') }}
            </button>
          </div>
        </div>
      </Transition>
    </Teleport>

    <Teleport to="body">
      <!-- Add-mirror modal -->
      <Transition name="fade">
        <div v-if="addingMirror" class="fixed inset-0 z-[3000] flex items-center justify-center p-4 bg-black/60 backdrop-blur-md" @click.self="cancelAddMirror">
          <div class="glass-card p-6 max-w-md w-full">
            <h3 class="text-lg font-semibold text-white mb-1">Add update mirror</h3>
            <p class="text-sm text-white/60 mb-5">
              The URL should point directly at a <span class="font-mono text-white/80">manifest.json</span> served by a Gitea mirror or equivalent. It's added to the end of the list; use "Make primary" to change order.
            </p>
            <form class="space-y-3" @submit.prevent="submitMirror">
              <div>
                <label class="block text-xs text-white/60 mb-1">Manifest URL</label>
                <input
                  v-model="mirrorDraft.url"
                  type="text"
                  autofocus
                  placeholder="https://host/.../manifest.json"
                  class="w-full px-3 py-2 rounded-md bg-white/5 border border-white/10 text-sm text-white focus:border-white/30 focus:outline-none font-mono"
                />
              </div>
              <div>
                <label class="block text-xs text-white/60 mb-1">Label (optional)</label>
                <input
                  v-model="mirrorDraft.label"
                  type="text"
                  placeholder="Home VPS"
                  class="w-full px-3 py-2 rounded-md bg-white/5 border border-white/10 text-sm text-white focus:border-white/30 focus:outline-none"
                />
              </div>
              <div class="flex gap-3 justify-end pt-2">
                <button
                  type="button"
                  @click="cancelAddMirror"
                  class="glass-button rounded-lg px-4 py-2 text-sm font-medium"
                >{{ t('common.cancel') }}</button>
                <button
                  type="submit"
                  class="glass-button rounded-lg px-4 py-2 text-sm font-medium bg-orange-500/20 border-orange-400/30 disabled:opacity-50 disabled:cursor-not-allowed"
                  :disabled="mirrorSaving || !mirrorDraft.url.trim()"
                >{{ mirrorSaving ? 'Adding…' : 'Add mirror' }}</button>
              </div>
            </form>
          </div>
        </div>
      </Transition>

      <!-- Confirmation modal -->
      <Transition name="fade">
        <div v-if="confirmAction" class="fixed inset-0 z-[3000] flex items-center justify-center p-4 bg-black/60 backdrop-blur-md" @click.self="cancelConfirm">
          <div class="glass-card p-6 max-w-sm w-full">
            <h3 class="text-lg font-semibold text-white mb-3">
              {{ confirmAction === 'rollback'
                ? t('systemUpdate.rollbackTitle')
                : confirmAction === 'git-apply'
                  ? t('systemUpdate.gitApplyTitle')
                  : confirmAction === 'cancel-download'
                    ? t('systemUpdate.cancelDownloadTitle')
                    : t('systemUpdate.applyTitle') }}
            </h3>
            <p class="text-sm text-white/70 mb-6">
              {{ confirmAction === 'rollback'
                ? t('systemUpdate.rollbackMessage')
                : confirmAction === 'git-apply'
                  ? t('systemUpdate.gitApplyMessage')
                  : confirmAction === 'cancel-download'
                    ? t('systemUpdate.cancelDownloadConfirm')
                    : t('systemUpdate.applyMessage') }}
            </p>
            <div class="flex gap-3 justify-end">
              <button @click="cancelConfirm" class="glass-button rounded-lg px-4 py-2 text-sm font-medium">
                {{ t('common.cancel') }}
              </button>
              <button
                @click="executeConfirm"
                class="glass-button rounded-lg px-4 py-2 text-sm font-medium"
                :class="(confirmAction === 'rollback' || confirmAction === 'cancel-download') ? 'bg-red-500/20 border-red-400/30' : 'bg-orange-500/20 border-orange-400/30'"
              >
                {{ confirmAction === 'rollback'
                  ? t('systemUpdate.rollbackButton')
                  : confirmAction === 'git-apply'
                    ? t('systemUpdate.pullAndRebuild')
                    : confirmAction === 'cancel-download'
                      ? t('systemUpdate.cancelDownloadButton')
                      : t('systemUpdate.applyNow') }}
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, reactive } from 'vue'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import BitcoinFaceAscii from '@/views/discover/BitcoinFaceAscii.vue'
import BackButton from '@/components/BackButton.vue'
import ToggleSwitch from '@/components/ToggleSwitch.vue'

interface UpdateDetail {
  version: string
  release_date: string
  changelog: string[]
  components: number
}

type ScheduleValue = 'manual' | 'daily_check' | 'auto_apply'

const { t } = useI18n()

const scheduleOptions = computed<{ value: ScheduleValue; label: string; description: string }[]>(() => [
  { value: 'manual', label: t('systemUpdate.manualOnly'), description: t('systemUpdate.manualOnlyDesc') },
  { value: 'daily_check', label: t('systemUpdate.dailyCheck'), description: t('systemUpdate.dailyCheckDesc') },
  { value: 'auto_apply', label: t('systemUpdate.autoApply'), description: t('systemUpdate.autoApplyDesc') },
])

const schedule = ref<ScheduleValue>('daily_check')
const loading = ref(false)
const downloading = ref(false)
const downloaded = ref(false)
const applying = ref(false)
const cancelingDownload = ref(false)
const downloadStalled = ref(false)
const confirmAction = ref<'apply' | 'git-apply' | 'rollback' | 'cancel-download' | null>(null)
const currentVersion = ref('0.0.0')
const lastCheck = ref<string | null>(null)
const updateInfo = ref<UpdateDetail | null>(null)
const updateMethod = ref<'git' | 'manifest' | null>(null)
const rollbackAvailable = ref(false)
const updateInProgress = ref(false)
const statusMessage = ref('')
const statusIsError = ref(false)
const downloadPercent = ref(0)
const downloadPercentFormatted = computed(() => downloadPercent.value.toFixed(2))

// Update/app fetch source: "origin" (HTTP mirrors, default/safe) vs "swarm"
// (DHT peer-assist with origin fallback). Lets us live-test the DHT on a node
// and flip back instantly. swarmAvailable reflects whether this binary was
// built with the iroh swarm engine at all.
const updateSource = ref<'origin' | 'swarm'>('origin')
const provideDht = ref(true)
const swarmAvailable = ref(false)
const swarmEnabled = ref(false)
const sourceSaving = ref(false)

// Mirrors — servers this node tries for the manifest, in priority
// order. First entry is the primary. Add/remove/set-primary are wired
// to update.*-mirror RPCs; downloads automatically go to the mirror
// that served the manifest.
interface UpdateMirror { url: string; label: string }
const mirrors = ref<UpdateMirror[]>([])
const addingMirror = ref(false)
const mirrorSaving = ref(false)
const mirrorDraft = reactive({ url: '', label: '' })

// URL of the mirror that served the currently-available-update manifest.
// Backend reports it in update.status and update.check responses; the UI
// resolves it to a friendly label by matching against the mirrors list.
const manifestMirror = ref<string | null>(null)
const manifestMirrorLabel = computed(() => {
  const url = manifestMirror.value
  if (!url) return null
  const match = mirrors.value.find(m => m.url === url)
  if (match && match.label) return match.label
  try {
    const u = new URL(url)
    return u.host
  } catch {
    return url
  }
})

// Per-mirror test state. Populated by testMirror(); each entry is either
// { testing: true } while in flight or the backend response shape on
// completion. Rendered inline under each mirror row.
interface MirrorTestState {
  testing?: boolean
  reachable?: boolean
  latency_ms?: number
  http_status?: number | null
  error?: string | null
}
const mirrorTests = ref<Record<string, MirrorTestState>>({})

async function testMirror(url: string) {
  mirrorTests.value = { ...mirrorTests.value, [url]: { testing: true } }
  try {
    const res = await rpcClient.call<{
      reachable: boolean
      latency_ms: number
      http_status: number | null
      error: string | null
    }>({ method: 'update.test-mirror', params: { url } })
    mirrorTests.value = { ...mirrorTests.value, [url]: { ...res, testing: false } }
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    mirrorTests.value = { ...mirrorTests.value, [url]: { testing: false, reachable: false, error: msg } }
  }
}

function openAddMirror() {
  mirrorDraft.url = ''
  mirrorDraft.label = ''
  addingMirror.value = true
}
function cancelAddMirror() {
  addingMirror.value = false
}

async function loadMirrors() {
  try {
    const res = await rpcClient.call<{ mirrors: UpdateMirror[] }>({ method: 'update.list-mirrors' })
    mirrors.value = res.mirrors
  } catch (e) {
    if (import.meta.env.DEV) console.warn('update.list-mirrors failed', e)
  }
}

interface SourceState { source: 'origin' | 'swarm'; provide_dht: boolean; swarm_available: boolean; swarm_enabled: boolean }

async function loadSource() {
  try {
    const res = await rpcClient.call<SourceState>({ method: 'update.get-source' })
    updateSource.value = res.source
    swarmAvailable.value = res.swarm_available
    swarmEnabled.value = res.swarm_enabled
    if (typeof res.provide_dht === 'boolean') provideDht.value = res.provide_dht
  } catch (e) {
    if (import.meta.env.DEV) console.warn('update.get-source failed', e)
  }
}

async function setSource(source: 'origin' | 'swarm') {
  if (sourceSaving.value || updateSource.value === source) return
  sourceSaving.value = true
  try {
    const res = await rpcClient.call<SourceState>({ method: 'update.set-source', params: { source } })
    updateSource.value = res.source
    showStatus(source === 'swarm'
      ? 'Update source set to DHT swarm — origin still used as fallback.'
      : 'Update source set to stable origin.')
  } catch (e) {
    showStatus(`Failed to change source: ${e instanceof Error ? e.message : e}`, true)
  } finally {
    sourceSaving.value = false
  }
}

async function setProvide(provide: boolean) {
  if (sourceSaving.value || provideDht.value === provide) return
  sourceSaving.value = true
  try {
    const res = await rpcClient.call<SourceState>({ method: 'update.set-source', params: { provide } })
    if (typeof res.provide_dht === 'boolean') provideDht.value = res.provide_dht
    showStatus(provide
      ? 'This node will now seed updates/apps to peers.'
      : 'This node will no longer provide to the swarm.')
  } catch (e) {
    showStatus(`Failed to change setting: ${e instanceof Error ? e.message : e}`, true)
  } finally {
    sourceSaving.value = false
  }
}

async function submitMirror() {
  const url = mirrorDraft.url.trim()
  if (!url) return
  if (!url.startsWith('http://') && !url.startsWith('https://')) {
    showStatus('Mirror URL must start with http:// or https://', true)
    return
  }
  mirrorSaving.value = true
  try {
    const res = await rpcClient.call<{ mirrors: UpdateMirror[] }>({
      method: 'update.add-mirror',
      params: { url, label: mirrorDraft.label.trim() },
    })
    mirrors.value = res.mirrors
    mirrorDraft.url = ''
    mirrorDraft.label = ''
    addingMirror.value = false
    showStatus('Mirror added.')
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    showStatus(`Add mirror failed: ${msg}`, true)
  } finally {
    mirrorSaving.value = false
  }
}

async function removeMirror(url: string) {
  try {
    const res = await rpcClient.call<{ mirrors: UpdateMirror[] }>({
      method: 'update.remove-mirror',
      params: { url },
    })
    mirrors.value = res.mirrors
    showStatus('Mirror removed.')
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    showStatus(`Remove failed: ${msg}`, true)
  }
}

async function setPrimaryMirror(url: string) {
  try {
    const res = await rpcClient.call<{ mirrors: UpdateMirror[] }>({
      method: 'update.set-primary-mirror',
      params: { url },
    })
    mirrors.value = res.mirrors
    showStatus('Primary mirror updated. Next update check will try it first.')
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    showStatus(`Set primary failed: ${msg}`, true)
  }
}

// Poll the backend for the real bytes_downloaded / total_bytes so the
// progress bar tracks actual download state (and survives route
// changes). Returns true if a download is currently in progress.
async function pollDownloadProgress(): Promise<boolean> {
  try {
    const res = await rpcClient.call<{
      download_progress?: {
        bytes_downloaded: number
        total_bytes: number
        active: boolean
        stalled?: boolean
      } | null
    }>({ method: 'update.status' })
    const p = res.download_progress
    if (p && p.total_bytes > 0) {
      downloadPercent.value = Math.min(100, (p.bytes_downloaded / p.total_bytes) * 100)
      downloadStalled.value = !!p.stalled
      return p.active
    }
    downloadStalled.value = false
    return false
  } catch {
    return false
  }
}
// Shown next to the progress bar when the fake increment has maxed out
// at 95% but the real RPC hasn't returned yet — lets the user know the
// UI hasn't frozen while SHA verification and disk writes finish.
const downloadFinishing = computed(() => downloading.value && downloadPercent.value >= 95)

// Install overlay state — drives the full-screen progress modal shown
// while the backend swaps files, restarts, and comes back up on the
// new version. The overlay polls /health and auto-reloads the browser
// as soon as the backend reports the target version, so the user
// doesn't need to manually refresh.
type InstallStage = 'applying' | 'restarting' | 'reconnecting' | 'ready' | 'stalled'
const installing = ref(false)
const installStage = ref<InstallStage>('applying')
const installTargetVersion = ref<string | null>(null)
const installStartedAt = ref<number>(0)
const installElapsedSec = ref(0)
let installPollTimer: ReturnType<typeof setInterval> | null = null
let installElapsedTimer: ReturnType<typeof setInterval> | null = null
let installReadyTimer: ReturnType<typeof setTimeout> | null = null
const installElapsedLabel = computed(() => {
  const s = installElapsedSec.value
  if (s < 60) return `Elapsed: ${s}s`
  return `Elapsed: ${Math.floor(s / 60)}m${s % 60 < 10 ? '0' : ''}${s % 60}s`
})
function clearInstallTimers() {
  if (installPollTimer) {
    clearInterval(installPollTimer)
    installPollTimer = null
  }
  if (installElapsedTimer) {
    clearInterval(installElapsedTimer)
    installElapsedTimer = null
  }
  if (installReadyTimer) {
    clearTimeout(installReadyTimer)
    installReadyTimer = null
  }
}
function startInstallOverlay(targetVersion: string) {
  clearInstallTimers()
  installing.value = true
  installStage.value = 'applying'
  installTargetVersion.value = targetVersion
  installStartedAt.value = Date.now()
  installElapsedSec.value = 0
  // Tick an elapsed counter once per second for the UI.
  installElapsedTimer = setInterval(() => {
    installElapsedSec.value = Math.floor((Date.now() - installStartedAt.value) / 1000)
    // Stop polling after 3 min — surface the manual reload button.
    if (installElapsedSec.value >= 180 && installStage.value !== 'ready') {
      installStage.value = 'stalled'
    }
  }, 1000)
  // Start polling /health after a short delay — the backend restarts 2s
  // after replying to update.apply, so an immediate poll would see the
  // old backend and conclude nothing happened.
  installReadyTimer = setTimeout(() => {
    installStage.value = 'restarting'
    installPollTimer = setInterval(pollHealth, 1500)
  }, 2500)
}
async function pollHealth() {
  if (installStage.value === 'ready' || installStage.value === 'stalled') return
  try {
    const res = await fetch('/health', { signal: AbortSignal.timeout(2000) })
    if (!res.ok) throw new Error(`health ${res.status}`)
    const data = await res.json() as { version?: string }
    if (data.version && data.version === installTargetVersion.value) {
      installStage.value = 'ready'
      if (installPollTimer) { clearInterval(installPollTimer); installPollTimer = null }
      // Brief pause so the user sees the "Ready" state before the reload.
      installReadyTimer = setTimeout(() => { window.location.reload() }, 1200)
    } else {
      // Backend is up but still reporting the old version — frontend
      // and backend are mid-swap. Signal to the user.
      installStage.value = 'reconnecting'
      void confirmBackendUpdateSettled()
    }
  } catch {
    // Fetch fails while the server is mid-restart. Stay in 'restarting'.
  }
}
async function confirmBackendUpdateSettled() {
  try {
    const res = await rpcClient.call<{
      current_version: string
      update_in_progress: boolean
    }>({ method: 'update.status' })
    if (!res.update_in_progress && installStage.value !== 'ready' && installStage.value !== 'stalled') {
      installStage.value = 'ready'
      if (installPollTimer) { clearInterval(installPollTimer); installPollTimer = null }
      installReadyTimer = setTimeout(() => { window.location.reload() }, 800)
    }
  } catch {
    // Keep waiting on /health.
  }
}
function reloadNow() { window.location.reload() }
// Cleanup if the component is torn down mid-install (unlikely but safe).
import { onBeforeUnmount } from 'vue'
onBeforeUnmount(() => {
  clearInstallTimers()
})

const lastCheckDisplay = computed(() => {
  if (!lastCheck.value) return t('common.never')
  try {
    const d = new Date(lastCheck.value)
    return d.toLocaleString()
  } catch {
    return lastCheck.value
  }
})

const statusLabel = computed(() => {
  if (applying.value) return t('systemUpdate.applying')
  if (downloading.value) return t('systemUpdate.downloading')
  if (updateInProgress.value) return t('systemUpdate.applying')
  if (updateInfo.value) return t('systemUpdate.updateAvailable')
  if (rollbackAvailable.value) return t('systemUpdate.rollback')
  return t('systemUpdate.upToDate')
})

const statusDotColor = computed(() => {
  if (applying.value || downloading.value) return 'bg-orange-400 animate-pulse'
  if (updateInfo.value || updateInProgress.value) return 'bg-orange-400'
  return 'bg-green-400'
})

const statusTextColor = computed(() => {
  if (applying.value || downloading.value || updateInfo.value || updateInProgress.value) return 'text-orange-400'
  return 'text-green-400'
})

function showStatus(msg: string, isError = false) {
  statusMessage.value = msg
  statusIsError.value = isError
  setTimeout(() => { statusMessage.value = '' }, 8000)
}

function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}

async function loadStatus() {
  try {
    const res = await rpcClient.call<{
      current_version: string
      last_check: string | null
      update_available: boolean
      update_in_progress: boolean
      rollback_available: boolean
      manifest_mirror: string | null
    }>({ method: 'update.status' })
    currentVersion.value = res.current_version
    lastCheck.value = res.last_check
    updateInProgress.value = res.update_in_progress
    rollbackAvailable.value = res.rollback_available
    manifestMirror.value = res.manifest_mirror ?? null

    if (res.update_in_progress) {
      downloaded.value = true
    }
  } catch (e) {
    if (import.meta.env.DEV) console.warn('Failed to load update status', e)
  }
}

async function checkForUpdates() {
  loading.value = true
  statusMessage.value = ''
  try {
    const res = await rpcClient.call<{
      current_version: string
      last_check: string | null
      update_available: boolean
      update: UpdateDetail | null
      update_method?: string
      manifest_mirror?: string | null
    }>({ method: 'update.check' })
    currentVersion.value = res.current_version
    lastCheck.value = res.last_check
    updateInfo.value = res.update
    updateMethod.value = res.update_method === 'git' ? 'git' : 'manifest'
    manifestMirror.value = res.manifest_mirror ?? null
    if (!res.update_available) {
      showStatus(t('systemUpdate.upToDateMessage'))
    }
  } catch (e) {
    showStatus(t('systemUpdate.checkFailed'), true)
    if (import.meta.env.DEV) console.warn('Update check failed', e)
  } finally {
    loading.value = false
  }
}

async function downloadUpdate() {
  downloading.value = true
  downloadPercent.value = 0
  statusMessage.value = ''

  // Poll the backend's real byte counter every second instead of
  // faking progress. The backend exposes bytes_downloaded/total_bytes
  // via update.status, updated per chunk. This also means the bar
  // resumes correctly after navigating away and back — no more
  // "95% for some time" mystery.
  const progressInterval = setInterval(() => { void pollDownloadProgress() }, 1000)

  try {
    const res = await rpcClient.call<{
      total_bytes: number
      downloaded_bytes: number
      components_downloaded: number
    }>({ method: 'update.download', timeout: 3_900_000 })
    downloadPercent.value = 100
    downloaded.value = true
    const sizeMB = (res.downloaded_bytes / 1_048_576).toFixed(1)
    showStatus(t('systemUpdate.downloadSuccess', { count: res.components_downloaded, size: sizeMB }))
  } catch (e) {
    const msg = errorMessage(e)
    if (/no update.*available/i.test(msg)) {
      updateInfo.value = null
      updateMethod.value = null
      downloaded.value = false
      updateInProgress.value = false
      await loadStatus()
      showStatus(t('systemUpdate.upToDateMessage'))
    } else if (/already running/i.test(msg)) {
      // A download/apply is already in flight (auto-updater or another tab) —
      // that's progress, not failure. Re-sync state and say so calmly.
      await loadStatus()
      showStatus(t('systemUpdate.downloadInProgress'))
    } else {
      // A failed download is NOT a staged update — return the UI to the
      // Download button so the user can retry, instead of stranding them on
      // Install. Re-sync from the backend (its self-heal clears a stale
      // update_in_progress once the partial staging is cleaned up).
      downloaded.value = false
      updateInProgress.value = false
      await loadStatus()
      showStatus(`${t('systemUpdate.downloadFailed')} ${msg}`, true)
    }
    if (import.meta.env.DEV) console.warn('Download failed', e)
  } finally {
    clearInterval(progressInterval)
    downloading.value = false
  }
}

function requestApply() {
  confirmAction.value = 'apply'
}

function requestGitApply() {
  confirmAction.value = 'git-apply'
}

function requestRollback() {
  confirmAction.value = 'rollback'
}

function requestCancelDownload() {
  confirmAction.value = 'cancel-download'
}

function cancelConfirm() {
  confirmAction.value = null
}

async function executeConfirm() {
  const action = confirmAction.value
  confirmAction.value = null
  if (action === 'apply') {
    await applyUpdate()
  } else if (action === 'git-apply') {
    await applyUpdateGitWithOverlay()
  } else if (action === 'rollback') {
    await rollbackUpdate()
  } else if (action === 'cancel-download') {
    await cancelDownload()
  }
}

async function cancelDownload() {
  cancelingDownload.value = true
  try {
    await rpcClient.call({ method: 'update.cancel-download' })
    downloading.value = false
    downloaded.value = false
    downloadPercent.value = 0
    downloadStalled.value = false
    showStatus(t('systemUpdate.cancelDownloadSuccess'))
  } catch (e) {
    showStatus(t('systemUpdate.cancelDownloadFailed'), true)
    if (import.meta.env.DEV) console.warn('Cancel download failed', e)
  } finally {
    cancelingDownload.value = false
  }
}

async function applyUpdate() {
  applying.value = true
  statusMessage.value = ''
  const target = updateInfo.value?.version || null
  try {
    await rpcClient.call({ method: 'update.apply', timeout: 300_000 })
    // Apply succeeded. Backend scheduled a restart 2s after returning;
    // show the full-screen overlay while we wait for the new backend
    // to report the target version, then auto-reload.
    applying.value = false
    if (target) {
      startInstallOverlay(target)
    } else {
      // No target version known (legacy path) — fall back to the old
      // flash-and-reload behaviour.
      showStatus(t('systemUpdate.applySuccess'))
      setTimeout(() => window.location.reload(), 3000)
    }
  } catch (e) {
    // "Another update operation is already running" is NOT a failure — an
    // apply (or the auto-updater) is already doing the work. Presenting it
    // as an error made a successfully-updating node look broken (OptiPlex,
    // v1.7.118 rollout). Join the in-flight apply instead: same overlay,
    // same wait-for-new-version polling.
    if (/already running/i.test(errorMessage(e))) {
      applying.value = false
      if (target) {
        startInstallOverlay(target)
      } else {
        showStatus(t('systemUpdate.applyInProgress'))
      }
      return
    }
    // Surface the backend's actual, actionable message when it gave one
    // ("Update download was incomplete — download again", etc.) instead of a
    // generic dead end. The staged files are preserved across a failed apply,
    // so re-download stays available and a retry is always possible.
    const detail = errorMessage(e)
    const actionable = /^Update\b/.test(detail)
    showStatus(actionable ? detail : t('systemUpdate.applyFailed'), true)
    // A staging inconsistency means the download is the thing to redo — drop
    // the downloaded flag so the button offers Download again, not Apply.
    if (/incomplete|not staged|re-download|inconsistent/i.test(detail)) {
      downloaded.value = false
    }
    if (import.meta.env.DEV) console.warn('Apply failed', e)
    applying.value = false
  }
}

async function applyUpdateGitWithOverlay() {
  // Git-apply (dev path) also restarts the service — reuse the overlay
  // so the UX matches the manifest path. Target version isn't known up
  // front for git-apply; we just wait for a version change on /health.
  applying.value = true
  statusMessage.value = ''
  try {
    await rpcClient.call({ method: 'update.git-apply', timeout: 900_000 })
    applying.value = false
    startInstallOverlay(updateInfo.value?.version || currentVersion.value)
  } catch (e) {
    showStatus(t('systemUpdate.applyFailed'), true)
    if (import.meta.env.DEV) console.warn('Git apply failed', e)
    applying.value = false
  }
}

async function rollbackUpdate() {
  try {
    await rpcClient.call({ method: 'update.rollback' })
    showStatus(t('systemUpdate.rollbackSuccess'))
    rollbackAvailable.value = false
    await loadStatus()
  } catch (e) {
    showStatus(t('systemUpdate.rollbackFailed'), true)
    if (import.meta.env.DEV) console.warn('Rollback failed', e)
  }
}

async function loadSchedule() {
  try {
    const res = await rpcClient.call<{ schedule: ScheduleValue }>({ method: 'update.get-schedule' })
    schedule.value = res.schedule
  } catch {
    if (import.meta.env.DEV) console.warn('Failed to load update schedule')
  }
}

async function setSchedule(value: ScheduleValue) {
  schedule.value = value
  try {
    await rpcClient.call({ method: 'update.set-schedule', params: { schedule: value } })
    showStatus(`Schedule set to ${scheduleOptions.value.find(o => o.value === value)?.label}`)
  } catch (e) {
    showStatus('Failed to save schedule', true)
    if (import.meta.env.DEV) console.warn('Set schedule failed', e)
  }
}

onMounted(async () => {
  await Promise.all([loadStatus(), loadSchedule(), loadMirrors(), loadSource(), checkForUpdates()])
  // If a download was already running when the user navigated here
  // (or refreshed), pick up the progress bar where it is and keep
  // polling until the backend reports done. No RPC call to start the
  // download — the backend's already running it.
  const active = await pollDownloadProgress()
  if (active) {
    downloading.value = true
    const resumeInterval = setInterval(async () => {
      const stillActive = await pollDownloadProgress()
      if (!stillActive) {
        clearInterval(resumeInterval)
        downloading.value = false
        downloaded.value = true
      }
    }, 1000)
  }
})
</script>

<style scoped>
/* Centered ASCII logo — clamped so the overlay doesn't blow out on
   narrow viewports. :deep so the rule reaches BitcoinFaceAscii's
   inner <pre>. */
.install-overlay-ascii :deep(pre) {
  font-size: clamp(6px, 1.2vw, 12px);
  line-height: 1.1;
  color: rgba(255, 255, 255, 0.85);
  margin: 0;
}

/* Indeterminate progress stripe that slides left-to-right. */
.install-overlay-bar-anim {
  animation: installBarSlide 1.8s ease-in-out infinite;
}
@keyframes installBarSlide {
  0%   { transform: translateX(-100%); }
  50%  { transform: translateX(120%); }
  100% { transform: translateX(300%); }
}
</style>
