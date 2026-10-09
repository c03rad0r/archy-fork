<script setup lang="ts">
import { nextTick, ref } from 'vue'
import { useRouter } from 'vue-router'
import BackButton from '@/components/BackButton.vue'
import DeviceModel from '@/components/DeviceModel.vue'
import HardwareMarketplace from '@/components/HardwareMarketplace.vue'

const router = useRouter()
const shopOpen = ref(false)
const showcase = ref<HTMLElement>()
const showcaseHeight = ref<string>()

function openShop() {
  const height = showcase.value?.getBoundingClientRect().height
  if (height) showcaseHeight.value = `${height}px`
  shopOpen.value = true
}
async function closeShop() {
  shopOpen.value = false
  await nextTick()
  showcaseHeight.value = undefined
}
</script>

<template>
  <div class="pb-12">
    <div class="flex items-center gap-3 mb-6">
      <BackButton desktop-margin="mb-0" @click="router.push({ path: '/dashboard', query: { tab: 'setup' } })" />
      <h1 class="text-lg font-semibold text-white">Get a non surveillance phone</h1>
    </div>
    <p class="text-sm text-white/75 max-w-2xl">Your phone should answer to you. Choose a supported Pixel, install GrapheneOS, and keep verified boot on.</p>

    <div ref="showcase" class="glass-card p-6 overflow-hidden phone-showcase" :style="showcaseHeight ? { height: showcaseHeight } : undefined">
      <div class="grid grid-cols-1 lg:grid-cols-[minmax(0,1fr)_minmax(280px,360px)] gap-6 items-start phone-showcase__grid">
        <DeviceModel v-if="!shopOpen" model="pixel-graphene" label="Pixel phone with GrapheneOS verified boot" fill-viewport :controls="false" />
        <HardwareMarketplace kind="phone" :expanded="shopOpen" @open="openShop" @close="closeShop" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.phone-showcase { margin: 24px 0 48px; }
.phone-showcase__grid { height: 100%; min-height: 0; grid-template-rows: minmax(0, 1fr); }
@media (min-width: 1024px) {
  .phone-showcase__grid > :deep(.router-marketplace) { align-self: end; margin-top: 0; }
}
</style>
