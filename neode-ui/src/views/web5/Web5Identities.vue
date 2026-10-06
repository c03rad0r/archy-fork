<template>
  <!-- Identity Management -->
  <div class="glass-card p-6 identities-card flex flex-col">
    <!-- Desktop: side-by-side -->
    <div class="hidden md:flex items-center justify-between mb-4">
      <div class="flex items-center gap-3">
        <div class="flex-shrink-0 w-10 h-10 rounded-lg bg-white/10 flex items-center justify-center">
          <svg class="w-5 h-5 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H5a2 2 0 00-2 2v9a2 2 0 002 2h14a2 2 0 002-2V8a2 2 0 00-2-2h-5m-4 0V5a2 2 0 114 0v1m-4 0a2 2 0 104 0m-5 8a2 2 0 100-4 2 2 0 000 4zm0 0c1.306 0 2.417.835 2.83 2M9 14a3.001 3.001 0 00-2.83 2M15 11h3m-3 4h2" />
          </svg>
        </div>
        <div>
          <h2 class="text-lg font-semibold text-white">{{ t('web5.identities') }}</h2>
          <p class="text-xs text-white/60">{{ t('web5.identitiesDesc') }}</p>
        </div>
      </div>
      <button @click="showCreateIdentityModal = true" class="web5-card-actions-top glass-button glass-button-sm px-3 rounded-lg text-sm font-medium items-center gap-2">
        Create
      </button>
    </div>
    <!-- Mobile: stacked -->
    <div class="md:hidden mb-4">
      <div class="flex items-center gap-3 mb-2">
        <div class="flex-shrink-0 w-10 h-10 rounded-lg bg-white/10 flex items-center justify-center">
          <svg class="w-5 h-5 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H5a2 2 0 00-2 2v9a2 2 0 002 2h14a2 2 0 002-2V8a2 2 0 00-2-2h-5m-4 0V5a2 2 0 114 0v1m-4 0a2 2 0 104 0m-5 8a2 2 0 100-4 2 2 0 000 4zm0 0c1.306 0 2.417.835 2.83 2M9 14a3.001 3.001 0 00-2.83 2M15 11h3m-3 4h2" />
          </svg>
        </div>
        <h2 class="text-lg font-semibold text-white">{{ t('web5.identities') }}</h2>
      </div>
      <p class="text-xs text-white/60 mb-3">{{ t('web5.identitiesDesc') }}</p>
    </div>

    <!-- Loading -->
    <div v-if="identitiesLoading && managedIdentities.length === 0" class="py-6 text-center">
      <svg class="animate-spin h-6 w-6 text-blue-400 mx-auto mb-2" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
        <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
        <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
      </svg>
      <p class="text-white/50 text-sm">{{ t('common.loading') }}</p>
    </div>

    <!-- Empty State -->
    <div v-else-if="managedIdentities.length === 0" class="py-6 text-center">
      <svg class="w-12 h-12 text-white/20 mx-auto mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
      </svg>
      <p class="text-white/60 text-sm mb-1">{{ t('web5.noIdentities') }}</p>
      <p class="text-white/40 text-xs">{{ t('web5.createFirstIdentity') }}</p>
    </div>

    <!-- Identity List -->
    <div v-else class="space-y-3">
      <div v-if="identitiesLoading" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
        <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Refreshing identities...
      </div>
      <div
        v-for="(identity, idx) in managedIdentities"
        :key="identity.id"
        :class="{ 'card-stagger': showStagger }"
        :style="{ '--stagger-index': idx }"
      >
        <!-- Narrow containers (phones): profile-card layout with banner +
             overlapping avatar. Hidden on wide containers where the classic
             row below renders instead. -->
        <div class="identity-card-m rounded-xl overflow-hidden bg-white/[0.08]">
          <div class="relative h-16" :class="bannerFallback(identity)">
            <img
              v-if="identity.profile?.banner"
              :src="displayableUrl(identity.profile.banner)"
              class="absolute inset-0 w-full h-full object-cover"
            />
          </div>
          <div class="px-4 pb-4">
            <div class="flex items-end justify-between -mt-7">
              <button @click="openProfileEditor(identity)" class="relative w-14 h-14 rounded-full overflow-hidden ring-4 ring-black/60 group shrink-0" title="Edit profile">
                <img
                  v-if="identity.profile?.picture && !listPictureFailed[identity.id]"
                  :src="displayableUrl(identity.profile.picture)"
                  class="w-full h-full object-cover"
                  @error="() => { listPictureFailed[identity.id] = true }"
                />
                <div v-if="!identity.profile?.picture || listPictureFailed[identity.id]" class="w-full h-full flex items-center justify-center" :class="{
                  'bg-blue-500/20': identity.purpose === 'personal',
                  'bg-orange-500/20': identity.purpose === 'business',
                  'bg-purple-500/20': identity.purpose === 'anonymous',
                }">
                  <span class="text-lg font-bold" :class="{
                    'text-blue-400': identity.purpose === 'personal',
                    'text-orange-400': identity.purpose === 'business',
                    'text-purple-400': identity.purpose === 'anonymous',
                  }">{{ identity.name.charAt(0).toUpperCase() }}</span>
                </div>
                <div class="absolute inset-0 bg-black/50 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center">
                  <svg class="w-4 h-4 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" /></svg>
                </div>
              </button>
              <div class="flex items-center gap-1 pt-8">
                <button @click="openKeyViewer(identity)" class="p-2 rounded-lg text-white/50 hover:text-white hover:bg-white/10 transition-colors" title="View keys">
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" />
                  </svg>
                </button>
                <button v-if="!identity.is_default" @click="setDefaultIdentity(identity.id)" class="p-2 rounded-lg text-white/50 hover:text-yellow-400 hover:bg-white/10 transition-colors" title="Set as default">
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z" />
                  </svg>
                </button>
                <button @click="confirmDeleteIdentity(identity)" class="p-2 rounded-lg text-white/50 hover:text-red-400 hover:bg-white/10 transition-colors" title="Delete">
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                  </svg>
                </button>
              </div>
            </div>
            <div class="mt-2 flex items-center gap-2 flex-wrap min-w-0">
              <span class="text-white font-semibold text-sm truncate">{{ identity.profile?.display_name || identity.name }}</span>
              <span v-if="identity.is_default" class="text-yellow-400 text-xs" title="Default identity">&#9733;</span>
              <span class="text-xs px-2 py-0.5 rounded-full capitalize" :class="{
                'bg-blue-500/20 text-blue-300': identity.purpose === 'personal',
                'bg-orange-500/20 text-orange-300': identity.purpose === 'business',
                'bg-purple-500/20 text-purple-300': identity.purpose === 'anonymous',
              }">{{ identity.purpose }}</span>
            </div>
            <p v-if="identity.profile?.about" class="text-white/60 text-xs mt-1 leading-relaxed">{{ identity.profile.about }}</p>
            <div class="mt-2 space-y-0.5">
              <div class="flex items-center gap-1 min-w-0">
                <p class="text-white/50 text-xs font-mono truncate" :title="identity.did">{{ identity.did }}</p>
                <button @click="copyIdentityDid(identity.did)" class="shrink-0 p-0.5 rounded text-white/30 hover:text-white/70 transition-colors" title="Copy DID">
                  <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" /></svg>
                </button>
              </div>
              <div v-if="identity.nostr_npub" class="flex items-center gap-1 min-w-0">
                <p class="text-white/40 text-xs font-mono truncate" :title="identity.nostr_npub">{{ identity.nostr_npub }}</p>
                <button @click="copyIdentityDid(identity.nostr_npub || '')" class="shrink-0 p-0.5 rounded text-white/30 hover:text-white/70 transition-colors" title="Copy npub">
                  <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" /></svg>
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- Wide containers: the original row — desktop appearance unchanged -->
        <div class="identity-row flex flex-col gap-3 p-4 bg-white/[0.08] rounded-lg">
      <div class="identity-row-main flex items-center gap-4 min-w-0">
        <!-- Avatar -->
        <button @click="openProfileEditor(identity)" class="relative flex-shrink-0 w-10 h-10 rounded-full overflow-hidden group" title="Edit profile">
          <img
            v-if="identity.profile?.picture && !listPictureFailed[identity.id]"
            :src="displayableUrl(identity.profile.picture)"
            class="w-full h-full object-cover"
            @error="() => { listPictureFailed[identity.id] = true }"
          />
          <div v-if="!identity.profile?.picture || listPictureFailed[identity.id]" class="w-full h-full flex items-center justify-center" :class="{
            'bg-blue-500/20': identity.purpose === 'personal',
            'bg-orange-500/20': identity.purpose === 'business',
            'bg-purple-500/20': identity.purpose === 'anonymous',
          }">
            <span class="text-sm font-bold" :class="{
              'text-blue-400': identity.purpose === 'personal',
              'text-orange-400': identity.purpose === 'business',
              'text-purple-400': identity.purpose === 'anonymous',
            }">{{ identity.name.charAt(0).toUpperCase() }}</span>
          </div>
          <div class="absolute inset-0 bg-black/50 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center">
            <svg class="w-4 h-4 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" /></svg>
          </div>
        </button>

        <!-- Info -->
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-2">
            <span class="text-white font-medium text-sm">{{ identity.profile?.display_name || identity.name }}</span>
            <span v-if="identity.profile?.display_name && identity.profile.display_name !== identity.name" class="text-white/40 text-xs truncate max-w-[160px]" :title="`Internal name: ${identity.name}`">({{ identity.name }})</span>
            <span v-if="identity.is_default" class="text-yellow-400 text-xs" title="Default identity">&#9733;</span>
            <span class="text-xs px-2 py-0.5 rounded-full capitalize" :class="{
              'bg-blue-500/20 text-blue-300': identity.purpose === 'personal',
              'bg-orange-500/20 text-orange-300': identity.purpose === 'business',
              'bg-purple-500/20 text-purple-300': identity.purpose === 'anonymous',
            }">{{ identity.purpose }}</span>
          </div>
          <div class="flex items-center gap-1 mt-0.5">
            <p class="text-white/50 text-xs font-mono truncate" :title="identity.did">{{ identity.did }}</p>
            <button @click="copyIdentityDid(identity.did)" class="shrink-0 p-0.5 rounded text-white/30 hover:text-white/70 transition-colors" title="Copy DID">
              <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" /></svg>
            </button>
          </div>
          <div v-if="identity.nostr_npub" class="flex items-center gap-1 mt-0.5">
            <p class="text-white/40 text-xs font-mono truncate" :title="identity.nostr_npub">{{ identity.nostr_npub }}</p>
            <button @click="copyIdentityDid(identity.nostr_npub || '')" class="shrink-0 p-0.5 rounded text-white/30 hover:text-white/70 transition-colors" title="Copy npub">
              <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" /></svg>
            </button>
          </div>
        </div>
      </div>

        <!-- Actions -->
        <div class="flex items-center justify-end gap-1 shrink-0">
          <button @click="openKeyViewer(identity)" class="p-2 rounded-lg text-white/50 hover:text-white hover:bg-white/10 transition-colors" title="View keys">
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" />
            </svg>
          </button>
          <button v-if="!identity.is_default" @click="setDefaultIdentity(identity.id)" class="p-2 rounded-lg text-white/50 hover:text-yellow-400 hover:bg-white/10 transition-colors" title="Set as default">
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z" />
            </svg>
          </button>
          <button @click="confirmDeleteIdentity(identity)" class="p-2 rounded-lg text-white/50 hover:text-red-400 hover:bg-white/10 transition-colors" title="Delete">
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
          </button>
        </div>
        </div>
      </div>
    </div>

    <div class="mt-auto pt-4">
      <button @click="showCreateIdentityModal = true" class="web5-card-actions-bottom mobile-card-action glass-button rounded-lg text-sm font-medium">
        Create
      </button>
    </div>
  </div>

  <!-- Create Identity Modal -->
  <Teleport to="body">
  <div v-if="showCreateIdentityModal" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-md" @click.self="showCreateIdentityModal = false" @keydown.escape="showCreateIdentityModal = false">
    <div class="glass-card p-6 w-full max-w-2xl mx-4 max-h-[90vh] overflow-y-auto" role="dialog" aria-modal="true" aria-labelledby="create-identity-title">
      <h2 id="create-identity-title" class="text-lg font-bold text-white mb-4">{{ t('web5.createIdentityTitle') }}</h2>
      <div class="space-y-4">
        <div>
          <label class="text-white/60 text-sm block mb-1">Name</label>
          <input v-model="newIdentityName" type="text" placeholder="Personal" class="w-full input-glass" />
        </div>
        <div>
          <label class="text-white/60 text-sm block mb-1">Purpose</label>
          <div class="grid grid-cols-3 gap-2">
            <button
              v-for="p in ['personal', 'business', 'anonymous']"
              :key="p"
              @click="newIdentityPurpose = p"
              class="px-3 py-2 rounded-lg text-sm capitalize transition-colors border"
              :class="newIdentityPurpose === p ? 'bg-white/15 border-white/30 text-white' : 'bg-white/5 border-white/10 text-white/60 hover:bg-white/10'"
            >{{ p }}</button>
          </div>
        </div>
      </div>
      <div v-if="createIdentityError" class="mt-3 alert-error">
        <p class="text-xs">{{ createIdentityError }}</p>
      </div>
      <div class="flex gap-3 mt-6">
        <button @click="showCreateIdentityModal = false" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm">{{ t('common.cancel') }}</button>
        <button @click="createIdentity" :disabled="creatingIdentity" class="flex-1 glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium">
          {{ creatingIdentity ? t('web5.creatingDid') : t('web5.createIdentity') }}
        </button>
      </div>
    </div>
  </div>
  </Teleport>

  <!-- Delete Confirmation Modal -->
  <Teleport to="body">
  <div v-if="deleteIdentityTarget" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-md" @click.self="deleteIdentityTarget = null" @keydown.escape="deleteIdentityTarget = null">
    <div class="glass-card p-6 w-full max-w-2xl mx-4 max-h-[90vh] overflow-y-auto" role="dialog" aria-modal="true" aria-labelledby="delete-identity-title">
      <h2 id="delete-identity-title" class="text-lg font-bold text-white mb-2">{{ t('web5.deleteIdentityTitle') }}</h2>
      <p class="text-white/60 text-sm mb-4">{{ t('web5.deleteIdentityConfirm') }}</p>
      <div class="flex gap-3">
        <button @click="deleteIdentityTarget = null" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm">{{ t('common.cancel') }}</button>
        <button @click="deleteIdentity" :disabled="deletingIdentity" class="flex-1 glass-button glass-button-danger px-4 py-2 rounded-lg text-sm font-medium">
          {{ deletingIdentity ? t('web5.deleting') : t('common.delete') }}
        </button>
      </div>
    </div>
  </div>
  </Teleport>

  <!-- Key Viewer Modal -->
  <Teleport to="body">
  <div v-if="keyViewerIdentity" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-md" @click.self="closeKeyViewer" @keydown.escape="closeKeyViewer">
    <div class="glass-card p-6 w-full max-w-2xl mx-4 max-h-[90vh] overflow-y-auto" role="dialog" aria-modal="true" aria-labelledby="key-viewer-title">
      <div class="flex items-center gap-3 mb-5">
        <div class="w-10 h-10 rounded-full flex items-center justify-center" :class="{
          'bg-blue-500/20': keyViewerIdentity.purpose === 'personal',
          'bg-orange-500/20': keyViewerIdentity.purpose === 'business',
          'bg-purple-500/20': keyViewerIdentity.purpose === 'anonymous',
        }">
          <svg class="w-5 h-5" :class="{
            'text-blue-400': keyViewerIdentity.purpose === 'personal',
            'text-orange-400': keyViewerIdentity.purpose === 'business',
            'text-purple-400': keyViewerIdentity.purpose === 'anonymous',
          }" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" />
          </svg>
        </div>
        <div>
          <h2 id="key-viewer-title" class="text-lg font-bold text-white">{{ keyViewerIdentity.name }}</h2>
          <p class="text-xs text-white/50 capitalize">{{ keyViewerIdentity.purpose }} identity</p>
        </div>
      </div>

      <!-- Public Keys -->
      <div class="space-y-3 mb-5">
        <h3 class="text-sm font-semibold text-white/80 flex items-center gap-2">
          <svg class="w-4 h-4 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" /></svg>
          Public Keys
        </h3>
        <div class="space-y-2">
          <div class="bg-black/30 rounded-lg p-3">
            <div class="flex items-center justify-between mb-1">
              <span class="text-xs text-white/50">DID (Ed25519)</span>
              <button @click="copyKeyValue('did', keyViewerIdentity.did)" class="text-xs text-white/40 hover:text-white/80 transition-colors">{{ keyViewerCopied === 'did' ? 'Copied!' : 'Copy' }}</button>
            </div>
            <p class="text-xs font-mono text-white/70 break-all">{{ keyViewerIdentity.did }}</p>
          </div>
          <div class="bg-black/30 rounded-lg p-3">
            <div class="flex items-center justify-between mb-1">
              <span class="text-xs text-white/50">Ed25519 Public Key (hex)</span>
              <button @click="copyKeyValue('pubkey', keyViewerIdentity.pubkey)" class="text-xs text-white/40 hover:text-white/80 transition-colors">{{ keyViewerCopied === 'pubkey' ? 'Copied!' : 'Copy' }}</button>
            </div>
            <p class="text-xs font-mono text-white/70 break-all">{{ keyViewerIdentity.pubkey }}</p>
          </div>
          <div v-if="keyViewerIdentity.nostr_npub" class="bg-black/30 rounded-lg p-3">
            <div class="flex items-center justify-between mb-1">
              <span class="text-xs text-white/50">Nostr npub (NIP-19)</span>
              <button @click="copyKeyValue('npub', keyViewerIdentity.nostr_npub!)" class="text-xs text-white/40 hover:text-white/80 transition-colors">{{ keyViewerCopied === 'npub' ? 'Copied!' : 'Copy' }}</button>
            </div>
            <p class="text-xs font-mono text-white/70 break-all">{{ keyViewerIdentity.nostr_npub }}</p>
          </div>
          <div v-if="keyViewerIdentity.nostr_pubkey" class="bg-black/30 rounded-lg p-3">
            <div class="flex items-center justify-between mb-1">
              <span class="text-xs text-white/50">Nostr Public Key (hex)</span>
              <button @click="copyKeyValue('nostr_hex', keyViewerIdentity.nostr_pubkey!)" class="text-xs text-white/40 hover:text-white/80 transition-colors">{{ keyViewerCopied === 'nostr_hex' ? 'Copied!' : 'Copy' }}</button>
            </div>
            <p class="text-xs font-mono text-white/70 break-all">{{ keyViewerIdentity.nostr_pubkey }}</p>
          </div>
        </div>
      </div>

      <!-- Private Keys Section -->
      <div class="border-t border-white/10 pt-5">
        <h3 class="text-sm font-semibold text-red-300/80 flex items-center gap-2 mb-3">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" /></svg>
          Private Keys
        </h3>
        <div v-if="!keyViewerPrivateKeys">
          <p class="text-xs text-white/40 mb-3">Enter your login password to reveal private keys. Never share these with anyone.</p>
          <div class="flex gap-2">
            <input v-model="keyViewerPassword" type="password" placeholder="Password" class="flex-1 input-glass" @keydown.enter="unlockPrivateKeys" />
            <button @click="unlockPrivateKeys" :disabled="!keyViewerPassword || keyViewerUnlocking" class="glass-button px-4 py-2 rounded-lg text-sm font-medium bg-red-500/10 border-red-500/20 hover:bg-red-500/20 disabled:opacity-50">
              {{ keyViewerUnlocking ? 'Verifying...' : 'Unlock' }}
            </button>
          </div>
          <p v-if="keyViewerError" class="text-red-400 text-xs mt-2">{{ keyViewerError }}</p>
        </div>
        <div v-else class="space-y-2">
          <div class="bg-red-500/5 border border-red-500/10 rounded-lg p-3">
            <div class="flex items-center justify-between mb-1">
              <span class="text-xs text-red-300/60">Ed25519 Secret Key (hex)</span>
              <button @click="copyKeyValue('ed25519_secret', keyViewerPrivateKeys.ed25519_secret_hex)" class="text-xs text-red-300/40 hover:text-red-300/80 transition-colors">{{ keyViewerCopied === 'ed25519_secret' ? 'Copied!' : 'Copy' }}</button>
            </div>
            <p class="text-xs font-mono text-red-200/70 break-all">{{ keyViewerPrivateKeys.ed25519_secret_hex }}</p>
          </div>
          <div v-if="keyViewerPrivateKeys.nostr_nsec" class="bg-red-500/5 border border-red-500/10 rounded-lg p-3">
            <div class="flex items-center justify-between mb-1">
              <span class="text-xs text-red-300/60">Nostr nsec (NIP-19)</span>
              <button @click="copyKeyValue('nsec', keyViewerPrivateKeys.nostr_nsec)" class="text-xs text-red-300/40 hover:text-red-300/80 transition-colors">{{ keyViewerCopied === 'nsec' ? 'Copied!' : 'Copy' }}</button>
            </div>
            <p class="text-xs font-mono text-red-200/70 break-all">{{ keyViewerPrivateKeys.nostr_nsec }}</p>
          </div>
          <div v-if="keyViewerPrivateKeys.nostr_secret_hex" class="bg-red-500/5 border border-red-500/10 rounded-lg p-3">
            <div class="flex items-center justify-between mb-1">
              <span class="text-xs text-red-300/60">Nostr Secret Key (hex)</span>
              <button @click="copyKeyValue('nostr_secret', keyViewerPrivateKeys.nostr_secret_hex)" class="text-xs text-red-300/40 hover:text-red-300/80 transition-colors">{{ keyViewerCopied === 'nostr_secret' ? 'Copied!' : 'Copy' }}</button>
            </div>
            <p class="text-xs font-mono text-red-200/70 break-all">{{ keyViewerPrivateKeys.nostr_secret_hex }}</p>
          </div>
          <button @click="keyViewerPrivateKeys = null" class="mt-2 text-xs text-white/40 hover:text-white/60 transition-colors">Lock private keys</button>
        </div>
      </div>

      <div class="flex justify-end mt-5">
        <button @click="closeKeyViewer" class="glass-button px-6 py-2 rounded-lg text-sm">Close</button>
      </div>
    </div>
  </div>
  </Teleport>

  <!-- Profile Editor Modal -->
  <Teleport to="body">
  <div v-if="profileEditorIdentity" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-md" @click.self="closeProfileEditor" @keydown.escape="closeProfileEditor">
    <div class="glass-card p-6 w-full max-w-2xl mx-4 max-h-[90vh] overflow-y-auto" role="dialog" aria-modal="true" aria-labelledby="profile-editor-title">
      <IdentitySuccessPane
        v-if="profileSuccess"
        :identity-name="profileSuccess.identityName"
        :event-id="profileSuccess.eventId"
        :accepted="profileSuccess.accepted"
        :attempted="profileSuccess.attempted"
        :relay-note="profileSuccess.relayNote"
        @again="profileSuccess = null"
        @done="closeProfileEditor"
      />
      <template v-else>
      <div class="flex items-center gap-3 mb-5">
        <div class="relative w-16 h-16 rounded-full overflow-hidden bg-white/10 shrink-0">
          <img
            v-if="profileForm.picture && !editorPictureFailed"
            :src="displayableUrl(profileForm.picture)"
            class="w-full h-full object-cover"
            @error="editorPictureFailed = true"
            @load="editorPictureFailed = false"
          />
          <div v-if="!profileForm.picture || editorPictureFailed" class="w-full h-full flex items-center justify-center">
            <span class="text-2xl font-bold text-white/40">{{ profileEditorIdentity.name.charAt(0).toUpperCase() }}</span>
          </div>
        </div>
        <div>
          <h2 id="profile-editor-title" class="text-lg font-bold text-white">Edit Profile</h2>
          <p class="text-xs text-white/50">{{ profileEditorIdentity.name }} &middot; {{ profileEditorIdentity.purpose }}</p>
        </div>
      </div>
      <div class="space-y-3">
        <div>
          <label class="text-white/60 text-xs block mb-1">Display Name</label>
          <input v-model="profileForm.display_name" type="text" :placeholder="profileEditorIdentity.name" class="w-full input-glass" />
        </div>
        <div>
          <label class="text-white/60 text-xs block mb-1">About / Bio</label>
          <textarea v-model="profileForm.about" rows="3" placeholder="A short bio..." class="w-full input-glass resize-none"></textarea>
        </div>
        <div>
          <label class="text-white/60 text-xs block mb-1">Profile Picture</label>
          <div class="flex gap-2">
            <input v-model="profileForm.picture" type="url" placeholder="https://… or upload below" class="flex-1 input-glass" />
            <label class="glass-button px-3 py-2 rounded-lg text-xs cursor-pointer whitespace-nowrap">
              {{ avatarUploading ? 'Uploading…' : 'Upload' }}
              <input type="file" accept="image/*" class="hidden" :disabled="avatarUploading" @change="uploadAsset($event, 'picture')" />
            </label>
          </div>
        </div>
        <div>
          <label class="text-white/60 text-xs block mb-1">Banner Image</label>
          <div class="flex gap-2">
            <input v-model="profileForm.banner" type="url" placeholder="https://… or upload below" class="flex-1 input-glass" />
            <label class="glass-button px-3 py-2 rounded-lg text-xs cursor-pointer whitespace-nowrap">
              {{ bannerUploading ? 'Uploading…' : 'Upload' }}
              <input type="file" accept="image/*" class="hidden" :disabled="bannerUploading" @change="uploadAsset($event, 'banner')" />
            </label>
          </div>
        </div>
        <div>
          <label class="text-white/60 text-xs block mb-1">Website</label>
          <input v-model="profileForm.website" type="url" placeholder="https://..." class="w-full input-glass" />
        </div>
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="text-white/60 text-xs block mb-1">NIP-05 (Nostr address)</label>
            <input v-model="profileForm.nip05" type="text" placeholder="you@domain.com" class="w-full input-glass" />
          </div>
          <div>
            <label class="text-white/60 text-xs block mb-1">Lightning Address (LUD-16)</label>
            <input v-model="profileForm.lud16" type="text" placeholder="you@getalby.com" class="w-full input-glass" />
          </div>
        </div>
      </div>
      <div v-if="profileError" class="mt-3 alert-error"><p class="text-xs">{{ profileError }}</p></div>
      <div class="flex gap-3 mt-5">
        <button @click="closeProfileEditor" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm">Cancel</button>
        <button @click="publishProfile" :disabled="profilePublishing" class="flex-1 glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium">{{ profilePublishing ? 'Saving & publishing…' : 'Save' }}</button>
      </div>
      </template>
    </div>
  </div>
  </Teleport>
</template>

<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import { safeClipboardWrite } from './utils'
import type { ManagedIdentity, IdentityProfile } from './types'
import IdentitySuccessPane from '@/components/IdentitySuccessPane.vue'

const { t } = useI18n()

const IDENTITIES_CACHE_KEY = 'archipelago.web5.identities.v1'

function readIdentitiesCache(): ManagedIdentity[] {
  if (typeof window === 'undefined') return []
  try {
    const raw = window.sessionStorage.getItem(IDENTITIES_CACHE_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw) as ManagedIdentity[]
    return Array.isArray(parsed) ? parsed : []
  } catch {
    return []
  }
}

function writeIdentitiesCache(identities: ManagedIdentity[]) {
  if (typeof window === 'undefined') return
  try {
    window.sessionStorage.setItem(IDENTITIES_CACHE_KEY, JSON.stringify(identities))
  } catch {
    // Cache is opportunistic only.
  }
}

defineProps<{
  showStagger: boolean
}>()

const emit = defineEmits<{
  toast: [text: string]
}>()

const managedIdentities = ref<ManagedIdentity[]>(readIdentitiesCache())
const identitiesLoading = ref(false)
const showCreateIdentityModal = ref(false)
const newIdentityName = ref('Personal')
const newIdentityPurpose = ref('personal')
const creatingIdentity = ref(false)
const createIdentityError = ref<string | null>(null)
const deleteIdentityTarget = ref<ManagedIdentity | null>(null)
const deletingIdentity = ref(false)

// Key viewer
const keyViewerIdentity = ref<ManagedIdentity | null>(null)
const keyViewerPrivateKeys = ref<{ ed25519_secret_hex: string; nostr_secret_hex: string; nostr_nsec: string } | null>(null)
const keyViewerPassword = ref('')
const keyViewerUnlocking = ref(false)
const keyViewerError = ref('')
const keyViewerCopied = ref<string | null>(null)

// Profile editor
const profileEditorIdentity = ref<ManagedIdentity | null>(null)
const profileForm = ref<IdentityProfile>({})
const profilePublishing = ref(false)
const avatarUploading = ref(false)
const bannerUploading = ref(false)

// Track image load failures so the UI can fall back to the initial/
// identicon placeholder instead of showing a blank square. Pasted URLs
// that 404 (or point at an onion the local browser can't reach) were
// previously silently hidden by a display:none handler that left the
// fallback unrendered.
const editorPictureFailed = ref(false)
const listPictureFailed = reactive<Record<string, boolean>>({})

// Reset the failure flag when the URL changes so a freshly pasted URL
// gets re-tried (the watcher fires once the form reacts).
watch(() => profileForm.value.picture, () => { editorPictureFailed.value = false })

// The backend returns onion-based public URLs for uploaded profile
// pictures (so they're fetchable by external Nostr clients), but the
// local browser session isn't Tor-routed and can't resolve .onion hosts.
// Rewrite onion-rooted `/blob/<cid>` URLs (with or without capability
// query) to same-origin `/blob/<cid>` so they render in this UI. Data
// URLs and plain external URLs pass through untouched.
// Gradient banner backdrop for the narrow profile card when the identity has
// no banner image, tinted by purpose to match the avatar/chip colors.
function bannerFallback(identity: ManagedIdentity): string {
  switch (identity.purpose) {
    case 'business': return 'bg-gradient-to-br from-orange-500/40 to-amber-500/15'
    case 'anonymous': return 'bg-gradient-to-br from-purple-500/40 to-fuchsia-500/15'
    default: return 'bg-gradient-to-br from-blue-500/40 to-indigo-500/15'
  }
}

function displayableUrl(url: string | null | undefined): string {
  if (!url) return ''
  if (url.startsWith('data:') || url.startsWith('/')) return url
  const onionMatch = url.match(/^https?:\/\/[a-z2-7]{16,56}\.onion(\/blob\/[0-9a-f]{64})(\?.*)?$/i)
  if (onionMatch && onionMatch[1]) return onionMatch[1]
  return url
}

// Upload to the node's blob store and drop a URL into the profile field.
// For small images (≤64KB) we inline the bytes as a data URL so external
// Nostr clients can render the picture without needing to reach a tor
// onion. Larger uploads fall back to the onion-rooted public_url.
const INLINE_MAX = 64 * 1024

async function uploadAsset(ev: Event, field: 'picture' | 'banner') {
  const input = ev.target as HTMLInputElement
  const file = input?.files?.[0]
  if (!file) return
  const flag = field === 'picture' ? avatarUploading : bannerUploading
  flag.value = true
  profileError.value = ''
  try {
    const buf = await file.arrayBuffer()
    // Inline small images as a data URL — universally fetchable by any
    // Nostr client and bypasses the "only reachable over Tor" limitation.
    if (buf.byteLength <= INLINE_MAX) {
      const mime = file.type || 'image/png'
      const b64 = btoa(Array.from(new Uint8Array(buf), (b) => String.fromCharCode(b)).join(''))
      profileForm.value[field] = `data:${mime};base64,${b64}`
      return
    }
    const resp = await fetch('/api/blob', {
      method: 'POST',
      credentials: 'include',
      headers: {
        'Content-Type': 'application/octet-stream',
        'X-Blob-Mime': file.type || 'application/octet-stream',
        'X-Blob-Filename': file.name,
      },
      body: buf,
    })
    if (!resp.ok) throw new Error(`upload failed: HTTP ${resp.status}`)
    const { public_url, self_test_url } = await resp.json() as { public_url?: string; self_test_url?: string }
    const url = public_url || self_test_url
    if (!url) throw new Error('blob API returned no URL')
    profileForm.value[field] = url
    // Heads-up for large uploads: onion URLs only render on Tor-routed
    // clients. Not an error, but worth telling the user.
    if (url.includes('.onion/')) {
      profileError.value = 'Large image stored on this node. Pasting a public https://… URL is recommended for Nostr visibility.'
    }
  } catch (e: unknown) {
    profileError.value = e instanceof Error ? e.message : `${field} upload failed`
  } finally {
    flag.value = false
    // Clear the input so selecting the same file again re-fires change.
    if (input) input.value = ''
  }
}
const profileError = ref('')
interface ProfilePublishSuccess {
  identityName: string
  eventId: string
  accepted: number
  attempted: number
  relayNote: string
}
const profileSuccess = ref<ProfilePublishSuccess | null>(null)

async function loadIdentities() {
  const hadIdentities = managedIdentities.value.length > 0
  identitiesLoading.value = true
  try {
    const res = await rpcClient.call<{ identities: ManagedIdentity[] }>({ method: 'identity.list' })
    managedIdentities.value = res.identities || []
    writeIdentitiesCache(managedIdentities.value)
  } catch {
    if (!hadIdentities) managedIdentities.value = []
  } finally {
    identitiesLoading.value = false
  }
}

async function createIdentity() {
  if (creatingIdentity.value) return
  createIdentityError.value = null
  creatingIdentity.value = true
  try {
    await rpcClient.call({
      method: 'identity.create',
      params: { name: newIdentityName.value.trim() || 'Personal', purpose: newIdentityPurpose.value },
    })
    showCreateIdentityModal.value = false
    newIdentityName.value = 'Personal'
    newIdentityPurpose.value = 'personal'
    await loadIdentities()
    emit('toast', t('web5.identityCreated'))
  } catch (err: unknown) {
    createIdentityError.value = err instanceof Error ? err.message : t('web5.failedToCreateIdentity')
  } finally {
    creatingIdentity.value = false
  }
}

function copyIdentityDid(did: string) {
  safeClipboardWrite(did)
  emit('toast', t('web5.didCopied'))
}

async function setDefaultIdentity(id: string) {
  try {
    await rpcClient.call({ method: 'identity.set-default', params: { id } })
    await loadIdentities()
    emit('toast', t('web5.defaultIdentityUpdated'))
  } catch {
    emit('toast', t('web5.failedToSetDefault'))
  }
}

function confirmDeleteIdentity(identity: ManagedIdentity) {
  deleteIdentityTarget.value = identity
}

async function deleteIdentity() {
  if (!deleteIdentityTarget.value || deletingIdentity.value) return
  deletingIdentity.value = true
  try {
    await rpcClient.call({ method: 'identity.delete', params: { id: deleteIdentityTarget.value.id } })
    deleteIdentityTarget.value = null
    await loadIdentities()
    emit('toast', t('web5.identityDeleted'))
  } catch {
    emit('toast', t('web5.failedToDeleteIdentity'))
  } finally {
    deletingIdentity.value = false
  }
}

function openKeyViewer(identity: ManagedIdentity) {
  keyViewerIdentity.value = identity
  keyViewerPrivateKeys.value = null
  keyViewerPassword.value = ''
  keyViewerError.value = ''
}

function closeKeyViewer() {
  keyViewerPrivateKeys.value = null
  keyViewerPassword.value = ''
  keyViewerError.value = ''
  keyViewerIdentity.value = null
}

async function unlockPrivateKeys() {
  if (!keyViewerIdentity.value || !keyViewerPassword.value || keyViewerUnlocking.value) return
  keyViewerUnlocking.value = true
  keyViewerError.value = ''
  try {
    const res = await rpcClient.call<{
      ed25519_secret_hex: string
      nostr_secret_hex: string | null
      nostr_nsec: string | null
    }>({
      method: 'identity.export-keys',
      params: { id: keyViewerIdentity.value.id, password: keyViewerPassword.value },
    })
    keyViewerPrivateKeys.value = {
      ed25519_secret_hex: res.ed25519_secret_hex,
      nostr_secret_hex: res.nostr_secret_hex || '',
      nostr_nsec: res.nostr_nsec || '',
    }
    keyViewerPassword.value = ''
  } catch (err: unknown) {
    keyViewerError.value = err instanceof Error ? err.message : 'Failed to unlock keys'
  } finally {
    keyViewerUnlocking.value = false
  }
}

function copyKeyValue(label: string, value: string) {
  safeClipboardWrite(value)
  keyViewerCopied.value = label
  setTimeout(() => { keyViewerCopied.value = null }, 2000)
}

function openProfileEditor(identity: ManagedIdentity) {
  profileEditorIdentity.value = identity
  profileForm.value = { ...identity.profile }
  profileError.value = ''
  profileSuccess.value = null
}

function closeProfileEditor() {
  profileEditorIdentity.value = null
  profileForm.value = {}
  profileError.value = ''
  profileSuccess.value = null
}

async function publishProfile() {
  if (!profileEditorIdentity.value || profilePublishing.value) return
  profilePublishing.value = true
  profileError.value = ''
  profileSuccess.value = null
  try {
    const identity = profileEditorIdentity.value
    await rpcClient.call({
      method: 'identity.update-profile',
      params: { id: identity.id, ...profileForm.value },
    })
    await loadIdentities()
    try {
      const res = await rpcClient.call<{
        event_id: string
        accepted: string[]
        rejected: Array<[string, string]>
        relays_attempted: number
        published: boolean
      }>({ method: 'identity.publish-profile', params: { id: identity.id } })
      const accepted = res.accepted?.length ?? 0
      const attempted = res.relays_attempted ?? 0
      const rejected = res.rejected?.[0]
      profileSuccess.value = {
        identityName: profileForm.value.display_name?.trim() || identity.name,
        eventId: res.event_id || '',
        accepted,
        attempted,
        relayNote: accepted === attempted
          ? ''
          : rejected
            ? `${rejected[0]} rejected the event: ${rejected[1]}`
            : 'The profile is saved on this node. Check Manage Relays before retrying publication.',
      }
    } catch (publishError: unknown) {
      profileSuccess.value = {
        identityName: profileForm.value.display_name?.trim() || identity.name,
        eventId: '',
        accepted: 0,
        attempted: 0,
        relayNote: `The profile is saved on this node, but relay publication failed: ${publishError instanceof Error ? publishError.message : 'unknown error'}`,
      }
    }
  } catch (err: unknown) {
    profileError.value = err instanceof Error ? err.message : 'Failed to save profile'
  } finally {
    profilePublishing.value = false
  }
}

defineExpose({ loadIdentities, managedIdentities, openProfileEditor, publishProfile })
</script>

<style scoped>
/* This card sits in a 2-column page grid, so its own rendered width is
   roughly half the viewport — a viewport media query can't tell whether
   the row actually has room for a horizontal layout. Use a container query
   instead, keyed to the row's real width. */
.identities-card {
  container-type: inline-size;
  container-name: identities-card;
}

/* Narrow containers (default): the profile card renders and the classic row
   is hidden. Wide containers: the original desktop row — untouched — and the
   profile card hides. Doubled selectors out-rank the Tailwind utilities on
   the same elements regardless of stylesheet order. */
.identities-card .identity-row {
  display: none;
}

@container identities-card (min-width: 560px) {
  .identities-card .identity-card-m {
    display: none;
  }

  .identities-card .identity-row {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 1rem;
  }

  .identity-row-main {
    flex: 1 1 auto;
  }
}
</style>
