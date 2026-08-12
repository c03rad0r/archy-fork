<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAIPermissionsStore, AI_PERMISSION_CATEGORIES } from '@/stores/aiPermissions'
import ToggleSwitch from '@/components/ToggleSwitch.vue'

const { t } = useI18n()
const aiPermissions = useAIPermissionsStore()

// Grants live on the node, not in this browser's localStorage — reconcile on
// open so the switches show the node's truth rather than whatever this origin
// happens to remember. Without this the same node shows different settings at
// its LAN address and its Tailscale address.
onMounted(() => { void aiPermissions.hydrate() })

const aiCategoryGroups = computed(() => {
  const groups: { label: string; items: typeof AI_PERMISSION_CATEGORIES }[] = []
  for (const cat of AI_PERMISSION_CATEGORIES) {
    const existing = groups.find(g => g.label === cat.group)
    if (existing) {
      existing.items.push(cat)
    } else {
      groups.push({ label: cat.group, items: [cat] })
    }
  }
  return groups
})
</script>

<template>
  <!-- AI Data Access Section — id is the banner's #ai-data-access hash target -->
  <div id="ai-data-access" class="glass-card px-6 py-6 mb-6 scroll-mt-4">
    <div class="mb-2">
      <h2 class="text-xl font-semibold text-white/96">{{ t('settings.aiDataAccess') }}</h2>
    </div>
    <p class="text-sm text-white/60 mb-6">{{ t('settings.aiDataAccessDesc') }}</p>
    <button
      @click="aiPermissions.allEnabled ? aiPermissions.disableAll() : aiPermissions.enableAll()"
      class="w-full flex items-center gap-4 p-4 rounded-xl border transition-all text-left mb-6"
      :class="aiPermissions.allEnabled
        ? 'bg-white/10 border-orange-500/40'
        : 'bg-black/20 border-white/10 hover:border-white/20'"
    >
      <svg class="w-5 h-5 shrink-0" :class="aiPermissions.allEnabled ? 'text-orange-400' : 'text-white/40'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
      </svg>
      <div class="flex-1 min-w-0">
        <p class="text-sm font-medium" :class="aiPermissions.allEnabled ? 'text-white/95' : 'text-white/70'">{{ t('common.enableAll') }}</p>
        <p class="text-xs text-white/50 mt-0.5">{{ t('settings.enableAllDesc') }}</p>
      </div>
      <ToggleSwitch :model-value="aiPermissions.allEnabled" @update:model-value="aiPermissions.allEnabled ? aiPermissions.disableAll() : aiPermissions.enableAll()" @click.stop />
    </button>
    <div class="space-y-5">
      <div v-for="group in aiCategoryGroups" :key="group.label">
        <p class="text-xs font-medium text-white/40 uppercase tracking-wider mb-2 px-1">{{ group.label }}</p>
        <div class="space-y-2">
          <button
            v-for="cat in group.items"
            :key="cat.id"
            @click="aiPermissions.toggle(cat.id)"
            class="w-full flex items-center gap-4 p-4 rounded-xl border transition-all text-left"
            :class="aiPermissions.isEnabled(cat.id)
              ? 'bg-white/10 border-orange-500/40'
              : 'bg-black/20 border-white/10 hover:border-white/20'"
          >
            <svg class="w-5 h-5 shrink-0" :class="aiPermissions.isEnabled(cat.id) ? 'text-orange-400' : 'text-white/40'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="cat.icon" />
            </svg>
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2">
                <p class="text-sm font-medium" :class="aiPermissions.isEnabled(cat.id) ? 'text-white/95' : 'text-white/70'">{{ cat.label }}</p>
                <!-- Honest-copy rule: these categories grant the assistant
                     visibility only — it has no tools to act on them yet, and
                     the toggle must not imply otherwise. -->
                <span v-if="cat.contextOnly" class="shrink-0 px-1.5 py-0.5 text-[10px] rounded bg-white/10 text-white/50" title="The assistant can see this information but has no actions for it yet">context only</span>
              </div>
              <p class="text-xs text-white/50 mt-0.5">{{ cat.description }}<template v-if="cat.contextOnly"> — the assistant can see this but can't take actions with it yet</template></p>
            </div>
            <ToggleSwitch :model-value="aiPermissions.isEnabled(cat.id)" @update:model-value="aiPermissions.toggle(cat.id)" @click.stop />
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
