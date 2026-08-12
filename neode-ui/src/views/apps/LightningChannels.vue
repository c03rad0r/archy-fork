<template>
  <div class="pb-16 md:pb-4">
    <BackButton :label="backLabel" desktop-margin="mb-6" @click="goBack" />

    <h1 class="text-2xl font-bold text-white mb-6">Lightning Channels</h1>

    <LightningChannelsPanel />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import BackButton from '@/components/BackButton.vue'
import LightningChannelsPanel from '@/components/LightningChannelsPanel.vue'

const route = useRoute()
const router = useRouter()

// When a setup wizard sent us here (?from=goal&goal=<id>), back returns to it.
const fromGoalId = computed(() =>
  route.query.from === 'goal' && typeof route.query.goal === 'string' ? route.query.goal : null,
)
const backLabel = computed(() => (fromGoalId.value ? 'Back to Setup' : 'Back to LND'))

function goBack() {
  if (fromGoalId.value) {
    router.push(`/dashboard/goals/${fromGoalId.value}`)
  } else {
    router.replace('/dashboard/apps/lnd')
  }
}
</script>
