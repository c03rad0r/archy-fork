import type { Meta, StoryObj } from '@storybook/vue3'
import { ref } from 'vue'
import BottomSheet from '../BottomSheet.vue'

const meta: Meta<typeof BottomSheet> = {
  title: 'UI/BottomSheet',
  component: BottomSheet,
  tags: ['autodocs'],
}

export default meta
type Story = StoryObj<typeof BottomSheet>

export const Default: Story = {
  render: () => ({
    components: { BottomSheet },
    setup() {
      const isOpen = ref(true)
      return { isOpen }
    },
    template: `
      <BottomSheet :is-open="isOpen" @close="isOpen = false">
        <div class="p-6 space-y-4">
          <h3 class="text-lg font-semibold text-white/90">Bottom Sheet</h3>
          <p class="text-sm text-white/60">Drag the handle to resize or swipe down to close.</p>
          <div v-for="i in 5" :key="i" class="h-12 rounded-xl bg-white/5 border border-white/10" />
        </div>
      </BottomSheet>
    `,
  }),
}

export const CustomSnapPoints: Story = {
  render: () => ({
    components: { BottomSheet },
    setup() {
      const isOpen = ref(true)
      return { isOpen }
    },
    template: `
      <BottomSheet :is-open="isOpen" :snap-points="[30, 60, 100]" @close="isOpen = false">
        <div class="p-6">
          <h3 class="text-lg font-semibold text-white/90">Custom Snap Points</h3>
          <p class="text-sm text-white/60 mt-2">Snaps to 30%, 60%, and 100%.</p>
        </div>
      </BottomSheet>
    `,
  }),
}
