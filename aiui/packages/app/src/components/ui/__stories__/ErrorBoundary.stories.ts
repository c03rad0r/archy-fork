import type { Meta, StoryObj } from '@storybook/vue3'
import { defineComponent } from 'vue'
import ErrorBoundary from '../ErrorBoundary.vue'

const meta: Meta<typeof ErrorBoundary> = {
  title: 'UI/ErrorBoundary',
  component: ErrorBoundary,
  tags: ['autodocs'],
}

export default meta
type Story = StoryObj<typeof ErrorBoundary>

const BrokenChild = defineComponent({
  setup() {
    throw new Error('Something went wrong in the child component')
  },
  template: '<div>This should not render</div>',
})

export const WithError: Story = {
  render: () => ({
    components: { ErrorBoundary, BrokenChild },
    template: `
      <ErrorBoundary title="Component Error">
        <BrokenChild />
      </ErrorBoundary>
    `,
  }),
}

export const NoError: Story = {
  render: () => ({
    components: { ErrorBoundary },
    template: `
      <ErrorBoundary>
        <div class="p-4 text-white/80">Everything is working fine.</div>
      </ErrorBoundary>
    `,
  }),
}
