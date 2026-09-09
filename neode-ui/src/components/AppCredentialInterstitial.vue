<template>
  <BaseModal
    :show="store.credentialPrompt.show"
    :title="store.credentialPrompt.title"
    max-width="max-w-lg"
    z-index="z-[4100]"
    @close="store.cancelCredentialLaunch()"
  >
    <div v-if="store.credentialPrompt.loading" class="flex min-h-40 flex-col items-center justify-center text-center">
      <div class="h-8 w-8 animate-spin rounded-full border-2 border-white/15 border-t-orange-300" aria-hidden="true"></div>
      <p class="mt-4 text-sm text-white/55">{{ store.credentialPrompt.description }}</p>
    </div>
    <template v-else>
      <p v-if="store.credentialPrompt.description" class="-mt-1 mb-4 text-sm text-white/55">
        {{ store.credentialPrompt.description }}
      </p>
      <div class="space-y-3">
        <div v-for="credential in store.credentialPrompt.credentials" :key="credential.label" class="rounded-lg border border-white/10 bg-white/[0.04] p-3">
          <div class="mb-1 flex items-center justify-between gap-3">
            <span class="text-xs uppercase tracking-wide text-white/60">{{ credential.label }}</span>
            <button type="button" class="text-xs text-orange-300 hover:text-orange-200" @click="store.copyCredential(credential.label, credential.value)">
              {{ store.credentialPrompt.copied === credential.label ? 'Copied' : 'Copy' }}
            </button>
          </div>
          <p class="break-all font-mono text-sm text-white">{{ credential.value }}</p>
        </div>
      </div>
    </template>
    <template #footer>
      <div class="flex flex-col gap-3 sm:flex-row">
        <button type="button" class="w-full rounded-lg px-4 py-3 sm:flex-1 glass-button" @click="store.cancelCredentialLaunch()">Cancel</button>
        <button v-if="!store.credentialPrompt.loading" type="button" class="w-full rounded-lg px-4 py-3 font-semibold sm:flex-1 glass-button" @click="store.continueCredentialLaunch()">Continue to app</button>
      </div>
    </template>
  </BaseModal>
</template>

<script setup lang="ts">
import BaseModal from '@/components/BaseModal.vue'
import { useAppLauncherStore } from '@/stores/appLauncher'

const store = useAppLauncherStore()
</script>
