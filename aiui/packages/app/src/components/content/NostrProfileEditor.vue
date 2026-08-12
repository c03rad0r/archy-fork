<template>
  <div class="h-full flex flex-col">
    <div class="p-4 border-b border-white/[0.08]">
      <h3 class="text-sm font-bold text-white/90">Nostr Profile</h3>
    </div>

    <div v-if="!isLoggedIn" class="flex-1 flex items-center justify-center">
      <p class="text-xs text-white/30">Sign in with Nostr to edit your profile</p>
    </div>

    <div v-else class="flex-1 overflow-y-auto custom-scrollbar px-4 pt-3 pb-16 space-y-4">
      <!-- Profile card preview -->
      <div class="rounded-xl overflow-hidden border border-white/5">
        <div
          class="h-24 bg-cover bg-center"
          :style="profile.banner ? { backgroundImage: `url(${profile.banner})` } : {}"
          :class="!profile.banner ? 'bg-gradient-to-r from-accent/20 to-purple-500/20' : ''"
        />
        <div class="px-4 pb-4 -mt-8">
          <div
            class="w-16 h-16 rounded-full border-2 border-black bg-cover bg-center flex items-center justify-center"
            :style="profile.picture ? { backgroundImage: `url(${profile.picture})` } : {}"
            :class="!profile.picture ? 'bg-accent/20' : ''"
          >
            <span v-if="!profile.picture" class="text-lg font-bold text-accent">
              {{ (profile.display_name || profile.name || '?').charAt(0).toUpperCase() }}
            </span>
          </div>
          <p class="text-sm font-bold text-white/90 mt-2">{{ profile.display_name || profile.name || 'Anonymous' }}</p>
          <p v-if="profile.nip05" class="text-xs text-purple-400/60">{{ profile.nip05 }}</p>
          <p v-if="profile.about" class="text-xs text-white/50 mt-1 line-clamp-2">{{ profile.about }}</p>
        </div>
      </div>

      <!-- Edit form -->
      <div class="space-y-3">
        <div>
          <label class="text-xs text-white/30 block mb-1">Display Name</label>
          <input
            v-model="profile.display_name"
            type="text"
            class="w-full px-3 py-2 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors"
            placeholder="Your display name"
          />
        </div>

        <div>
          <label class="text-xs text-white/30 block mb-1">Username</label>
          <input
            v-model="profile.name"
            type="text"
            class="w-full px-3 py-2 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors"
            placeholder="username"
          />
        </div>

        <div>
          <label class="text-xs text-white/30 block mb-1">Bio</label>
          <textarea
            v-model="profile.about"
            class="w-full px-3 py-2 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors resize-none min-h-[60px]"
            placeholder="Tell the world about yourself"
          />
        </div>

        <div>
          <label class="text-xs text-white/30 block mb-1">Avatar URL</label>
          <input
            v-model="profile.picture"
            type="url"
            class="w-full px-3 py-2 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors font-mono"
            placeholder="https://example.com/avatar.jpg"
          />
        </div>

        <div>
          <label class="text-xs text-white/30 block mb-1">Banner URL</label>
          <input
            v-model="profile.banner"
            type="url"
            class="w-full px-3 py-2 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors font-mono"
            placeholder="https://example.com/banner.jpg"
          />
        </div>

        <div>
          <label class="text-xs text-white/30 block mb-1">Website</label>
          <input
            v-model="profile.website"
            type="url"
            class="w-full px-3 py-2 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors font-mono"
            placeholder="https://example.com"
          />
        </div>

        <div>
          <label class="text-xs text-white/30 block mb-1">NIP-05 Address</label>
          <input
            v-model="profile.nip05"
            type="text"
            class="w-full px-3 py-2 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors font-mono"
            placeholder="you@example.com"
          />
        </div>

        <div>
          <label class="text-xs text-white/30 block mb-1">Lightning Address</label>
          <input
            v-model="profile.lud16"
            type="text"
            class="w-full px-3 py-2 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors font-mono"
            placeholder="you@getalby.com"
          />
        </div>

        <!-- Publish button -->
        <button
          class="w-full min-h-[44px] rounded-lg text-sm font-medium bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors disabled:opacity-30"
          :disabled="isPublishing"
          @click="publishProfile"
        >
          {{ isPublishing ? 'Publishing...' : 'Publish Profile (kind:0)' }}
        </button>

        <!-- Status -->
        <div v-if="publishStatus" class="text-xs text-center" :class="publishSuccess ? 'text-emerald-400/60' : 'text-red-400/60'">
          {{ publishStatus }}
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, onMounted } from 'vue'
import { useNostrIdentity } from '@/composables/useNostrIdentity'
import { useNostr } from '@/composables/useNostr'

const { isLoggedIn, signEvent, pubkey } = useNostrIdentity()
const { publishEvent, fetchNote } = useNostr()

interface ProfileData {
  name: string
  display_name: string
  about: string
  picture: string
  banner: string
  website: string
  nip05: string
  lud16: string
}

const profile = reactive<ProfileData>({
  name: '',
  display_name: '',
  about: '',
  picture: '',
  banner: '',
  website: '',
  nip05: '',
  lud16: '',
})

const isPublishing = ref(false)
const publishStatus = ref('')
const publishSuccess = ref(false)

async function loadExistingProfile() {
  if (!pubkey.value) return
  const note = await fetchNote(pubkey.value, 5000)
  if (note && note.kind === 0) {
    try {
      const data = JSON.parse(note.content) as Partial<ProfileData>
      Object.assign(profile, data)
    } catch { /* invalid JSON */ }
  }
}

async function publishProfile() {
  if (!isLoggedIn.value) return

  isPublishing.value = true
  publishStatus.value = ''

  const content: Record<string, string> = {}
  for (const [key, val] of Object.entries(profile)) {
    if (val) content[key] = val
  }

  const unsigned = {
    kind: 0,
    created_at: Math.floor(Date.now() / 1000),
    tags: [],
    content: JSON.stringify(content),
  }

  const signed = await signEvent(unsigned)
  if (!signed) {
    isPublishing.value = false
    publishStatus.value = 'Signing failed'
    publishSuccess.value = false
    return
  }

  const results = await publishEvent(signed)
  const successes = results.filter(r => r.success).length
  isPublishing.value = false

  if (successes > 0) {
    publishStatus.value = `Published to ${successes}/${results.length} relays`
    publishSuccess.value = true
  } else {
    publishStatus.value = 'Failed to publish to any relay'
    publishSuccess.value = false
  }
}

onMounted(() => {
  loadExistingProfile()
})
</script>
