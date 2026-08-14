<template>
  <div class="app-details-container pb-16 md:pb-16">
    <BackButton :label="backButtonLabel" desktop-margin="mb-6" @click="goBack" />

    <Transition name="content-fade" mode="out-in">
    <!-- Loading State -->
    <div v-if="loading" key="loading" class="glass-card p-12 text-center">
      <svg class="animate-spin h-12 w-12 text-blue-400 mx-auto mb-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
        <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
        <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
      </svg>
      <p class="text-white/70">{{ t('marketplaceDetails.loadingDetails') }}</p>
    </div>

    <!-- App Details -->
    <div v-else-if="app" key="content">
      <!-- Compact Hero Section -->
      <div class="glass-card p-6 mb-6">
        <!-- Desktop: Single Row Layout -->
        <div class="hidden md:flex items-center gap-6">
          <!-- App Icon -->
          <img
            v-if="app.icon"
            :src="app.icon"
            :alt="app.title"
            class="w-20 h-20 rounded-xl shadow-xl flex-shrink-0 archy-app-icon"
            @error="handleImageError"
          />
          <div v-else class="w-20 h-20 rounded-xl bg-white/10 flex items-center justify-center flex-shrink-0">
            <svg class="w-10 h-10 text-white/40" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
            </svg>
          </div>
          
          <!-- App Info (grows to fill space) -->
          <div class="flex-1 min-w-0">
            <h1 class="text-2xl font-bold text-white mb-1">{{ app.title }}</h1>
            <p class="text-white/70 text-sm mb-2">{{ shortDescription }}</p>
            <div class="flex items-center gap-2">
              <span
                v-if="isInstalled"
                class="inline-flex items-center px-2.5 py-1 rounded-lg text-xs font-medium bg-green-500/20 text-green-200 border border-green-500/30"
              >
                <span class="w-1.5 h-1.5 rounded-full bg-green-400 mr-1.5"></span>
                {{ t('marketplaceDetails.installed') }}
              </span>
              <span class="text-white/50 text-xs">{{ app.version ? $ver(app.version) : 'latest' }}</span>
            </div>
          </div>

          <!-- Action Buttons -->
          <div class="flex items-center gap-2 flex-shrink-0">
            <button
              v-if="isInstalled"
              @click="goToInstalledApp"
              class="glass-button glass-button-sm px-6 py-2.5 rounded-lg text-sm font-semibold flex items-center gap-2"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
              </svg>
              {{ t('marketplaceDetails.open') }}
            </button>
            <select
              v-if="!isInstalled && installVersions.length > 1"
              v-model="selectedInstallVersion"
              :disabled="installing"
              :aria-label="t('marketplaceDetails.selectVersion')"
              class="rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-blue-400/60"
            >
              <option v-for="v in installVersions" :key="v.version" :value="v.version">
                {{ $ver(v.version) }}{{ v.default ? ' — latest' : '' }}{{ v.deprecated ? ' (deprecated)' : '' }}
              </option>
            </select>
            <button
              v-if="!isInstalled"
              @click="installApp"
              :disabled="demoNoInstall || installing || (!installBlockedReason && !app.manifestUrl && !app.dockerImage)"
              :title="demoNoInstall ? 'Not available in the demo' : (installBlockedReason || undefined)"
              class="glass-button glass-button-sm px-6 py-2.5 rounded-lg text-sm font-semibold flex items-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <svg v-if="installing" class="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              <svg v-else class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
              </svg>
              {{ demoNoInstall ? 'Not available in demo' : installBlockedReason ? 'Bitcoin Pruned' : installing ? t('common.installing') : t('common.install') }}
            </button>
          </div>
        </div>

        <!-- Mobile: Two Column Grid Layout -->
        <div class="md:hidden">
          <!-- Top: Icon + Info -->
          <div class="grid grid-cols-[80px_1fr] gap-4 mb-4">
            <!-- App Icon -->
            <img
              v-if="app.icon"
              :src="app.icon"
              :alt="app.title"
              class="w-20 h-20 rounded-xl shadow-xl archy-app-icon"
              @error="handleImageError"
            />
            <div v-else class="w-20 h-20 rounded-xl bg-white/10 flex items-center justify-center">
              <svg class="w-10 h-10 text-white/40" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
              </svg>
            </div>

            <!-- App Info -->
            <div class="min-w-0">
              <h1 class="text-xl font-bold text-white mb-1">{{ app.title }}</h1>
              <p class="text-white/70 text-xs mb-2 line-clamp-2">{{ shortDescription }}</p>
              <div class="flex flex-wrap items-center gap-2">
                <span
                  v-if="isInstalled"
                  class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-green-500/20 text-green-200 border border-green-500/30"
                >
                  <span class="w-1.5 h-1.5 rounded-full bg-green-400 mr-1"></span>
                  {{ t('marketplaceDetails.installed') }}
                </span>
                <span class="text-white/50 text-xs">{{ app.version ? $ver(app.version) : 'latest' }}</span>
              </div>
            </div>
          </div>

          <!-- Install-time version selector (multi-version apps) -->
          <select
            v-if="!isInstalled && installVersions.length > 1"
            v-model="selectedInstallVersion"
            :disabled="installing"
            :aria-label="t('marketplaceDetails.selectVersion')"
            class="w-full mb-2 rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-blue-400/60"
          >
            <option v-for="v in installVersions" :key="v.version" :value="v.version">
              {{ $ver(v.version) }}{{ v.default ? ' — latest' : '' }}{{ v.deprecated ? ' (deprecated)' : '' }}
            </option>
          </select>

          <!-- Bottom: Action Buttons -->
          <div class="grid grid-cols-2 gap-2">
            <button
              v-if="isInstalled"
              @click="goToInstalledApp"
              class="glass-button glass-button-sm px-4 py-2.5 rounded-lg text-sm font-semibold flex items-center justify-center gap-2"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
              </svg>
              {{ t('marketplaceDetails.open') }}
            </button>
            <button
              v-else
              @click="installApp"
              :disabled="demoNoInstall || installing || (!installBlockedReason && !app.manifestUrl && !app.dockerImage)"
              :title="demoNoInstall ? 'Not available in the demo' : (installBlockedReason || undefined)"
              class="glass-button glass-button-sm px-4 py-2.5 rounded-lg text-sm font-semibold flex items-center justify-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed col-span-2"
            >
              <svg v-if="installing" class="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              <svg v-else class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
              </svg>
              {{ demoNoInstall ? 'Not available in demo' : installBlockedReason ? 'Bitcoin Pruned' : installing ? t('common.installing') : t('common.install') }}
            </button>
          </div>

          <!-- Installation Error Banner (Mobile) -->
          <div v-if="installError" class="mt-4 p-3 bg-red-500/20 border border-red-500/40 rounded-lg">
            <div class="flex items-start gap-2">
              <svg class="w-4 h-4 text-red-400 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              <div class="flex-1">
                <p class="text-red-200 font-medium text-sm">{{ t('marketplaceDetails.installFailed') }}</p>
                <p class="text-red-300 text-xs mt-1">{{ installError }}</p>
              </div>
            </div>
          </div>
        </div>
        
        <!-- Installation Error Banner (Desktop) -->
        <div v-if="installError" class="hidden md:block mt-4 p-4 bg-red-500/20 border border-red-500/40 rounded-lg">
          <div class="flex items-start gap-3">
            <svg class="w-5 h-5 text-red-400 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
            <div class="flex-1">
              <p class="text-red-200 font-medium">{{ t('marketplaceDetails.installFailed') }}</p>
              <p class="text-red-300 text-sm mt-1">{{ installError }}</p>
            </div>
          </div>
        </div>
        <div v-if="installBlockedReason" class="hidden md:block mt-4 p-4 bg-yellow-500/15 border border-yellow-500/30 rounded-lg">
          <p class="text-yellow-100 font-medium">Bitcoin is in pruned mode</p>
          <p class="text-yellow-200/80 text-sm mt-1">{{ installBlockedReason }}</p>
        </div>
      </div>

      <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <!-- Main Content -->
        <div class="lg:col-span-2 space-y-6">
          <!-- Screenshots Gallery -->
          <div v-if="screenshots.length > 0" class="glass-card p-6">
            <h2 class="text-2xl font-bold text-white mb-4">{{ t('marketplaceDetails.screenshots') }}</h2>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
              <img
                v-for="screenshot in screenshots"
                :key="screenshot.src"
                :src="screenshot.src"
                :alt="screenshot.alt"
                class="aspect-video w-full rounded-xl border border-white/10 object-cover"
                loading="lazy"
              />
            </div>
          </div>

          <!-- Description -->
          <div class="glass-card p-6">
            <h2 class="text-2xl font-bold text-white mb-4">{{ t('marketplaceDetails.about', { name: app.title }) }}</h2>
            <p class="text-white/80 leading-relaxed whitespace-pre-line">
              {{ longDescription }}
            </p>
          </div>

          <!-- Features -->
          <div class="glass-card p-6">
            <h2 class="text-2xl font-bold text-white mb-4">{{ t('marketplaceDetails.features') }}</h2>
            <ul class="space-y-3">
              <li
                v-for="(feature, index) in features"
                :key="index"
                class="flex items-start gap-3 text-white/80"
              >
                <svg class="w-6 h-6 text-green-400 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
                <span>{{ feature }}</span>
              </li>
            </ul>
          </div>
        </div>

        <!-- Sidebar -->
        <div class="space-y-6">
          <!-- App Info Card -->
          <div class="glass-card p-6">
            <h3 class="text-lg font-bold text-white mb-4">{{ t('marketplaceDetails.information') }}</h3>
            <div class="space-y-3">
              <div class="flex items-center justify-between py-2 border-b border-white/10">
                <span class="text-white/60 text-sm">{{ t('common.version') }}</span>
                <span class="text-white font-medium">{{ app.version || 'latest' }}</span>
              </div>
              <div v-if="app.author" class="flex items-center justify-between py-2 border-b border-white/10">
                <span class="text-white/60 text-sm">{{ t('common.developer') }}</span>
                <span class="text-white font-medium">{{ app.author }}</span>
              </div>
              <div class="flex items-center justify-between py-2 border-b border-white/10">
                <span class="text-white/60 text-sm">{{ t('common.status') }}</span>
                <span class="text-white font-medium">{{ isInstalled ? t('marketplaceDetails.installed') : t('marketplaceDetails.notInstalled') }}</span>
              </div>
              <div class="flex items-center justify-between py-2 border-b border-white/10">
                <span class="text-white/60 text-sm">{{ t('common.category') }}</span>
                <span class="text-white font-medium capitalize">{{ app.category || 'App' }}</span>
              </div>
              <div v-if="app.manifestUrl" class="flex items-center justify-between py-2">
                <span class="text-white/60 text-sm">Package</span>
                <span class="text-white font-medium text-xs">.s9pk</span>
              </div>
            </div>
          </div>

          <!-- Requirements Card -->
          <div class="glass-card p-6">
            <h3 class="text-lg font-bold text-white mb-4">{{ t('marketplaceDetails.requirements') }}</h3>
            <div class="space-y-3">
              <!-- App Dependencies -->
              <div v-if="dependencies.length > 0" class="space-y-2 mb-4">
                <div
                  v-for="dep in dependencies"
                  :key="dep.id"
                  class="flex items-center gap-3 py-2 border-b border-white/10"
                >
                  <!-- Status indicator -->
                  <svg v-if="dep.status === 'running'" class="w-5 h-5 text-green-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                  <svg v-else-if="dep.status === 'stopped'" class="w-5 h-5 text-yellow-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                  </svg>
                  <svg v-else class="w-5 h-5 text-red-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                  <div class="flex-1">
                    <p class="text-white/80 font-medium text-sm">{{ dep.title }}</p>
                    <p class="text-white/50 text-xs">
                      {{ dep.status === 'running' ? t('marketplaceDetails.depRunning') : dep.status === 'stopped' ? t('marketplaceDetails.depStopped') : t('marketplaceDetails.depNotInstalled') }}
                    </p>
                  </div>
                </div>
                <!-- Install missing dependencies button -->
                <button
                  v-if="dependencies.some(d => d.status === 'missing')"
                  @click="installDependencies"
                  :disabled="installingDeps"
                  class="glass-button w-full mt-3 px-4 py-2 rounded-lg text-sm font-medium flex items-center justify-center gap-2"
                >
                  <svg v-if="installingDeps" class="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                    <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                    <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                  </svg>
                  {{ installingDeps ? t('common.installing') : t('marketplaceDetails.installRequirements') }}
                </button>
              </div>
              <div v-else class="py-2 border-b border-white/10">
                <p class="text-white/60 text-sm">{{ t('marketplaceDetails.noRequirements') }}</p>
              </div>
              <div class="flex items-start gap-3">
                <svg class="w-5 h-5 text-blue-400 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z" />
                </svg>
                <div class="flex-1">
                  <p class="text-white/80 font-medium">{{ t('appDetails.ram') }}</p>
                  <p class="text-white/60 text-sm">{{ t('appDetails.ramDesc') }}</p>
                </div>
              </div>
              <div class="flex items-start gap-3">
                <svg class="w-5 h-5 text-purple-400 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4" />
                </svg>
                <div class="flex-1">
                  <p class="text-white/80 font-medium">{{ t('appDetails.storage') }}</p>
                  <p class="text-white/60 text-sm">{{ t('appDetails.storageDesc') }}</p>
                </div>
              </div>
            </div>
          </div>

          <!-- Links Card (no GitHub - repo link removed per product) -->
          <div v-if="app.manifestUrl" class="glass-card p-6">
            <h3 class="text-lg font-bold text-white mb-4">{{ t('marketplaceDetails.links') }}</h3>
            <div class="space-y-2">
              <a
                :href="app.manifestUrl"
                target="_blank"
                rel="noopener noreferrer"
                class="flex items-center gap-2 text-blue-400 hover:text-blue-300 transition-colors"
              >
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 10v6m0 0l-3-3m3 3l3-3m2 8H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                </svg>
                {{ t('marketplaceDetails.downloadPackage') }}
              </a>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- App Not Found -->
    <div v-else key="not-found" class="glass-card p-12 text-center">
      <svg class="w-24 h-24 text-white/20 mx-auto mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.172 16.172a4 4 0 015.656 0M9 10h.01M15 10h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
      </svg>
      <h3 class="text-2xl font-semibold text-white mb-2">{{ t('marketplaceDetails.notFoundTitle') }}</h3>
      <p class="text-white/70">{{ t('marketplaceDetails.notFoundMessage') }}</p>
    </div>
    </Transition>

  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { IS_DEMO, isDemoApp } from '@/composables/useDemoIntro'
import { useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useAppStore } from '../stores/app'
import { rpcClient } from '../api/rpc-client'
import { useCachedResource } from '@/composables/useCachedResource'
import type { PackageVersionsResponse } from '../api/rpc-client'
import { useMarketplaceApp, type MarketplaceAppInfo } from '../composables/useMarketplaceApp'
import { useAppLauncherStore } from '../stores/appLauncher'
import BackButton from '@/components/BackButton.vue'
import { useToast } from '../composables/useToast'
import { handleImageError } from './apps/appsConfig'

const { t } = useI18n()
const toast = useToast()

const router = useRouter()
const route = useRoute()
const store = useAppStore()
const { getCurrentApp } = useMarketplaceApp()

const app = ref<MarketplaceAppInfo | null>(null)
const installing = ref(false)
const installingDeps = ref(false)
const installError = ref<string | null>(null)
const loading = ref(true)
const bitcoinPruned = ref(false)

// Multi-version support: install-time version choice. Populated from the signed
// catalog for apps that offer multiple versions (e.g. Bitcoin Core / Knots).
// Hidden when an app offers only one version — install stays one-click.
const installVersions = ref<{ version: string; default: boolean; deprecated: boolean; eol: string | null }[]>([])
const selectedInstallVersion = ref('')
const backButtonLabel = computed(() => route.query.from === 'home' ? t('marketplaceDetails.backToHome') : t('marketplaceDetails.backToStore'))
const electrumxArchiveWarning = 'You need a full archival bitcoin node before downloading ElectrumX'

const appId = computed(() => route.params.id as string)

// Keyed per marketplace app id (D-04). Catalog version metadata is
// near-static, so this can take a longer TTL than the 30s default; it holds
// nothing credential/DID/wallet/tx-history-shaped so persist:true is safe
// (explicit, not defaulted — CR-01 follow-up removed the implicit default).
// Note per 02-FINDINGS.md: this is the only one of MarketplaceAppDetails's
// captured RPC calls that isn't confounded by the Home-tab-transit artifact —
// getCurrentApp() is a synchronous store read (no RPC) and the bitcoin-prune
// check below is a plain fetch(), not an rpcClient call.
const versionsResource = useCachedResource<PackageVersionsResponse>({
  key: `app-details:versions:${appId.value}`,
  fetcher: (signal) => rpcClient.call<PackageVersionsResponse>({
    method: 'package.versions',
    params: { id: appId.value },
    signal,
    dedup: true,
    timeout: 15000,
  }),
  ttlMs: 120_000,
  persist: true, // package version list — public catalog metadata, no identity/money
  immediate: false,
})

// Web-only apps (no container, just a URL) — always treated as "installed"
const isWebOnly = computed(() => {
  return !!(app.value?.webUrl && !app.value?.dockerImage)
})

// Check if app is already installed
const isInstalled = computed(() => {
  if (isWebOnly.value) return true
  return !!store.packages[appId.value]
})

// Extract descriptions with safety checks
const shortDescription = computed(() => {
  try {
    if (!app.value) return ''
    const desc = app.value.description
    if (typeof desc === 'object' && desc) {
      return desc.short || desc.long || ''
    }
    return desc || ''
  } catch (e) {
    if (import.meta.env.DEV) console.error('[MarketplaceAppDetails] Error in shortDescription:', e)
    return ''
  }
})

const longDescription = computed(() => {
  try {
    if (!app.value) return ''
    const desc = app.value.description
    if (typeof desc === 'object' && desc) {
      return desc.long || desc.short || ''
    }
    return desc || ''
  } catch (e) {
    if (import.meta.env.DEV) console.error('[MarketplaceAppDetails] Error in longDescription:', e)
    return ''
  }
})

const screenshots = computed(() => normalizeScreenshots(app.value?.screenshots))

function normalizeScreenshots(items: MarketplaceAppInfo['screenshots'] | undefined) {
  if (!Array.isArray(items)) return []
  return items
    .map((item, index) => {
      if (typeof item === 'string') {
        const src = item.trim()
        return src ? { src, alt: `${app.value?.title || 'App'} screenshot ${index + 1}` } : null
      }
      const src = item.src?.trim()
      if (!src) return null
      return {
        src,
        alt: item.alt?.trim() || `${app.value?.title || 'App'} screenshot ${index + 1}`,
      }
    })
    .filter((item): item is { src: string; alt: string } => item !== null)
}

// Placeholder features
const features = computed(() => {
  return [
    'Self-hosted and privacy-focused',
    'Easy installation and updates',
    'Automatic backups',
    'Secure by default',
    'Open source'
  ]
})

/** App dependency definitions */
const R = 'source.archipelago-foundation.org/lfg2025'
const APP_DEPENDENCIES: Record<string, { id: string; title: string; dockerImage: string }[]> = {
  'electrumx': [{ id: 'bitcoin-knots', title: 'Bitcoin Knots', dockerImage: `${R}/bitcoin-knots:latest` }],
  'lnd': [{ id: 'bitcoin-knots', title: 'Bitcoin Knots', dockerImage: `${R}/bitcoin-knots:latest` }],
  'btcpay-server': [{ id: 'bitcoin-knots', title: 'Bitcoin Knots', dockerImage: `${R}/bitcoin-knots:latest` }],
  'mempool': [
    { id: 'bitcoin-knots', title: 'Bitcoin Knots', dockerImage: `${R}/bitcoin-knots:latest` },
    { id: 'electrumx', title: 'ElectrumX', dockerImage: `${R}/electrumx:v1.18.0` },
  ],
  'fedimint': [{ id: 'bitcoin-knots', title: 'Bitcoin Knots', dockerImage: `${R}/bitcoin-knots:latest` }],
}

/** Check dependency status against installed packages */
const dependencies = computed(() => {
  if (!app.value) return []
  const deps = APP_DEPENDENCIES[app.value.id]
  if (!deps) return []
  return deps.map(dep => {
    const pkg = store.packages[dep.id]
    let status: 'running' | 'stopped' | 'missing' = 'missing'
    if (pkg) {
      status = pkg.state === 'running' ? 'running' : 'stopped'
    }
    return { ...dep, status }
  })
})

const installBlockedReason = computed(() => {
  const id = app.value?.id
  if (!bitcoinPruned.value || !id) return ''
  if (id !== 'electrumx' && id !== 'electrs' && id !== 'mempool-electrs') return ''
  return electrumxArchiveWarning
})

// Demo: only demoable apps can be installed; the rest show "Not available in demo".
const demoNoInstall = computed(() => IS_DEMO && !!app.value?.id && !isDemoApp(app.value.id))

let pendingRedirect: ReturnType<typeof setTimeout> | null = null

onMounted(() => {
  if (import.meta.env.DEV) console.log('[MarketplaceAppDetails] Loading app ID:', appId.value)
  
  try {
    const loadedApp = getCurrentApp()
    
    if (loadedApp && loadedApp.id === appId.value) {
      app.value = loadedApp
      loading.value = false
    } else {
      loading.value = false
      pendingRedirect = setTimeout(() => {
        router.push('/dashboard/marketplace').catch(() => {})
      }, 500)
    }
  } catch (e) {
    if (import.meta.env.DEV) console.error('[MarketplaceAppDetails] Error loading app data:', e)
    loading.value = false
    pendingRedirect = setTimeout(() => {
      router.push('/dashboard/marketplace').catch(() => {})
    }, 500)
  }
  loadBitcoinPruneStatus()
  void loadInstallVersions()
})

// Fetch the catalog's selectable versions so the install panel can offer a
// choice (latest pre-selected). Best-effort: on any failure the selector stays
// hidden and install proceeds at the catalog default. Only refetches when the
// cached entry is missing or past its TTL, so a repeat open inside the TTL
// paints from cache with no new RPC.
async function loadInstallVersions() {
  if (versionsResource.data.value === null || versionsResource.isStale.value) {
    await versionsResource.refresh()
  }
  const info = versionsResource.data.value
  if (!info || !info.supportsVersions || info.versions.length < 2) {
    installVersions.value = []
    return
  }
  installVersions.value = info.versions
  selectedInstallVersion.value = info.default || info.versions.find(v => v.default)?.version || info.versions[0]?.version || ''
}

async function loadBitcoinPruneStatus() {
  try {
    const res = await fetch('/bitcoin-status', { credentials: 'include', signal: AbortSignal.timeout(8000) })
    if (!res.ok) return
    const status = await res.json()
    bitcoinPruned.value = status?.blockchain_info?.pruned === true
  } catch (e) {
    if (import.meta.env.DEV) console.warn('[MarketplaceAppDetails] Bitcoin prune status unavailable:', e)
  }
}

onBeforeUnmount(() => {
  if (pendingRedirect) { clearTimeout(pendingRedirect); pendingRedirect = null }
})

function goBack() {
  if (route.query.from === 'home') {
    router.push('/dashboard').catch(() => {})
  } else if (route.query.from === 'discover') {
    router.push('/dashboard/discover').catch(() => {})
  } else {
    router.push('/dashboard/marketplace').catch(() => {})
  }
}

function goToInstalledApp() {
  // Web-only apps: launch directly via appLauncher
  if (isWebOnly.value && app.value?.webUrl) {
    useAppLauncherStore().open({
      url: app.value.webUrl,
      title: app.value.title || appId.value,
    })
    return
  }
  router.push({
    path: `/dashboard/apps/${appId.value}`,
    query: { from: 'marketplace' }
  }).catch(() => {})
}

async function installDependencies() {
  if (installingDeps.value) return
  const missingDeps = dependencies.value.filter(d => d.status === 'missing')
  if (!missingDeps.length) return
  if (bitcoinPruned.value && missingDeps.some(d => d.id === 'electrumx' || d.id === 'electrs' || d.id === 'mempool-electrs')) {
    installError.value = electrumxArchiveWarning
    toast.error(electrumxArchiveWarning)
    return
  }

  installingDeps.value = true
  installError.value = null

  try {
    // Install dependencies sequentially (order matters: bitcoin before electrumx)
    for (const dep of missingDeps) {
      await rpcClient.call({
        method: 'package.install',
        params: {
          id: dep.id,
          dockerImage: dep.dockerImage,
        },
        timeout: 15000,
      })
      // Wait for package to register before installing next
      await new Promise(resolve => setTimeout(resolve, 2000))
    }
  } catch (err: unknown) {
    installError.value = err instanceof Error ? err.message : t('marketplaceDetails.installFailed')
    if (import.meta.env.DEV) console.error('[MarketplaceAppDetails] Failed to install dependencies:', err)
  } finally {
    installingDeps.value = false
  }
}

async function installApp() {
  if (installing.value || !app.value) return
  if (installBlockedReason.value) {
    installError.value = installBlockedReason.value
    toast.error(installBlockedReason.value)
    return
  }
  if (!app.value.manifestUrl && !app.value.dockerImage) {
    if (import.meta.env.DEV) console.warn('[MarketplaceAppDetails] Cannot install - no manifestUrl or dockerImage:', app.value)
    return
  }

  installing.value = true
  installError.value = null

  // Multi-version: a runner-chosen version (when offered) overrides the default.
  const chosenVersion = (installVersions.value.length > 1 && selectedInstallVersion.value)
    ? selectedInstallVersion.value
    : app.value.version

  try {
    if (app.value.dockerImage) {
      // Docker-based app installation
      const installParams: Record<string, unknown> = {
        id: app.value.id,
        dockerImage: app.value.dockerImage,
        version: chosenVersion,
      }
      if (app.value.containerConfig) installParams.containerConfig = app.value.containerConfig
      await rpcClient.call({
        method: 'package.install',
        params: installParams,
        timeout: 600000,
      })
    } else {
      // Package-based installation
      const installUrl = app.value.url || app.value.manifestUrl
      await rpcClient.call({
        method: 'package.install',
        params: {
          id: app.value.id,
          url: installUrl,
          version: chosenVersion,
        },
        timeout: 600000,
      })
    }

    // Wait a moment for the package to be registered
    await new Promise(resolve => setTimeout(resolve, 1000))

    router.push(`/dashboard/apps/${appId.value}`).catch(() => {})
  } catch (err: unknown) {
    installError.value = err instanceof Error ? err.message : t('marketplaceDetails.installFailed')
    if (import.meta.env.DEV) console.error('[MarketplaceAppDetails] Failed to install app:', err)
  } finally {
    installing.value = false
  }
}
</script>
