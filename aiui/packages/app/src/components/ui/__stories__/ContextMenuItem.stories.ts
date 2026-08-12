import type { Meta, StoryObj } from '@storybook/vue3'
import ContextMenuItem from '../ContextMenuItem.vue'

const meta: Meta<typeof ContextMenuItem> = {
  title: 'UI/ContextMenuItem',
  component: ContextMenuItem,
  tags: ['autodocs'],
  decorators: [
    () => ({
      template: '<div class="glass-card p-1.5 rounded-xl min-w-[140px]"><story /></div>',
    }),
  ],
}

export default meta
type Story = StoryObj<typeof ContextMenuItem>

export const Default: Story = {
  render: () => ({
    components: { ContextMenuItem },
    template: '<ContextMenuItem @click="() => {}">Edit Message</ContextMenuItem>',
  }),
}

export const Destructive: Story = {
  render: () => ({
    components: { ContextMenuItem },
    template: '<ContextMenuItem destructive @click="() => {}">Delete</ContextMenuItem>',
  }),
}
