<template>
  <div class="mt-2 pt-2 border-t border-white/[0.05] space-y-3">
    <p class="text-xs text-accent/60 uppercase tracking-wider font-bold">Settings</p>

    <!-- Generic key-value settings editor -->
    <div class="space-y-2">
      <div v-for="(value, key) in localSettings" :key="key" class="flex items-center gap-2">
        <span class="text-xs text-white/40 min-w-[60px]">{{ key }}</span>
        <input
          :value="String(value ?? '')"
          class="flex-1 px-2 py-1.5 rounded-md text-base bg-white/5 text-white/70 outline-none focus:bg-white/10 transition-colors"
          @input="updateSetting(key as string, ($event.target as HTMLInputElement).value)"
        />
      </div>

      <!-- Add new setting -->
      <div class="flex gap-2">
        <input
          v-model="newSettingKey"
          type="text"
          placeholder="Key"
          class="flex-1 px-2 py-1.5 rounded-md text-base bg-white/5 text-white/70 placeholder:text-white/20 outline-none focus:bg-white/10 transition-colors"
        />
        <input
          v-model="newSettingValue"
          type="text"
          placeholder="Value"
          class="flex-1 px-2 py-1.5 rounded-md text-base bg-white/5 text-white/70 placeholder:text-white/20 outline-none focus:bg-white/10 transition-colors"
        />
        <button
          class="text-xs px-2 py-1.5 rounded-md bg-white/5 text-white/40 hover:text-white/70 hover:bg-white/10 transition-colors disabled:opacity-30"
          :disabled="!newSettingKey.trim()"
          @click="addSetting"
        >
          Add
        </button>
      </div>
    </div>

    <!-- Permissions toggles -->
    <div v-if="permissions.length > 0">
      <p class="text-xs text-accent/60 uppercase tracking-wider font-bold mb-2">Permissions</p>
      <div class="space-y-1">
        <label
          v-for="perm in permissions"
          :key="perm"
          class="flex items-center gap-2 text-xs cursor-pointer"
        >
          <input
            type="checkbox"
            :checked="localGranted.includes(perm)"
            class="rounded accent-[#F7931A]"
            @change="togglePermission(perm)"
          />
          <span class="text-white/50">{{ perm }}</span>
        </label>
      </div>
    </div>

    <button
      class="w-full py-2 rounded-lg text-xs bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors"
      @click="save"
    >
      Save Settings
    </button>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import type { PluginPermission } from '@/stores/pluginMarketplace'

const props = defineProps<{
  pluginId: string
  settings: Record<string, unknown>
  permissions: PluginPermission[]
  grantedPermissions: PluginPermission[]
}>()

const emit = defineEmits<{
  updateSettings: [settings: Record<string, unknown>]
  updatePermissions: [permissions: PluginPermission[]]
}>()

const localSettings = ref<Record<string, unknown>>({ ...props.settings })
const localGranted = ref<PluginPermission[]>([...props.grantedPermissions])
const newSettingKey = ref('')
const newSettingValue = ref('')

function updateSetting(key: string, value: string) {
  localSettings.value[key] = value
}

function addSetting() {
  if (!newSettingKey.value.trim()) return
  localSettings.value[newSettingKey.value.trim()] = newSettingValue.value
  newSettingKey.value = ''
  newSettingValue.value = ''
}

function togglePermission(perm: PluginPermission) {
  const idx = localGranted.value.indexOf(perm)
  if (idx >= 0) localGranted.value.splice(idx, 1)
  else localGranted.value.push(perm)
}

function save() {
  emit('updateSettings', { ...localSettings.value })
  emit('updatePermissions', [...localGranted.value])
}
</script>
