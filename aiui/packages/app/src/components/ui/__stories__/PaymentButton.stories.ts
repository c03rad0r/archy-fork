import type { Meta, StoryObj } from '@storybook/vue3'
import PaymentButton from '../PaymentButton.vue'

const meta: Meta<typeof PaymentButton> = {
  title: 'UI/PaymentButton',
  component: PaymentButton,
  tags: ['autodocs'],
}

export default meta
type Story = StoryObj<typeof PaymentButton>

export const Default: Story = {
  args: {
    invoice: 'lnbc1000n1pjexample',
  },
}

export const WithAmount: Story = {
  args: {
    invoice: 'lnbc1000n1pjexample',
    amount: 1000,
  },
}
