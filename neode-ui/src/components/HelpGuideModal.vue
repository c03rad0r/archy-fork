<template>
  <BaseModal :show="show" :title="title" max-width="max-w-lg" content-class="max-h-[80vh] overflow-y-auto" @close="$emit('close')">
          <div class="text-white/80 prose prose-invert max-w-none">
            <p class="whitespace-pre-wrap">{{ content }}</p>
          </div>
          <div v-if="relatedPath" class="mt-4">
            <router-link
              :to="relatedPath"
              class="inline-flex items-center gap-2 px-4 py-2 glass-button rounded-lg text-sm font-medium"
              @click="$emit('close')"
            >
              Go to related page
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
              </svg>
            </router-link>
          </div>
  </BaseModal>
</template>

<script setup lang="ts">
import BaseModal from '@/components/BaseModal.vue'

defineProps<{
  show: boolean
  title: string
  content: string
  relatedPath?: string
}>()

defineEmits<{
  close: []
}>()
</script>
