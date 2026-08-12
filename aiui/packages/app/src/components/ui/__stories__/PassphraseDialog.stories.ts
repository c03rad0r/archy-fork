import type { Meta, StoryObj } from '@storybook/vue3'
import PassphraseDialog from '../PassphraseDialog.vue'

const meta: Meta<typeof PassphraseDialog> = {
  title: 'UI/PassphraseDialog',
  component: PassphraseDialog,
  tags: ['autodocs'],
}

export default meta
type Story = StoryObj<typeof PassphraseDialog>

export const Unlock: Story = {
  args: {
    visible: true,
    isCreating: false,
  },
}

export const Create: Story = {
  args: {
    visible: true,
    isCreating: true,
  },
}
