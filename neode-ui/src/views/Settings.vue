<script setup lang="ts">
import AccountSection from '@/views/settings/AccountSection.vue'
import SystemUpdatesSection from '@/views/settings/SystemUpdatesSection.vue'
import AppRegistriesSection from '@/views/settings/AppRegistriesSection.vue'
import SystemSection from '@/views/settings/SystemSection.vue'
</script>

<template>
  <div class="pb-6">
    <AccountSection />
    <SystemUpdatesSection />
    <AppRegistriesSection />
    <SystemSection />
  </div>
</template>
