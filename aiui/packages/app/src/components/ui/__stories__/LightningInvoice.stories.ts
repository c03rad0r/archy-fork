import type { Meta, StoryObj } from '@storybook/vue3'
import LightningInvoice from '../LightningInvoice.vue'

const meta: Meta<typeof LightningInvoice> = {
  title: 'UI/LightningInvoice',
  component: LightningInvoice,
  tags: ['autodocs'],
}

export default meta
type Story = StoryObj<typeof LightningInvoice>

export const Default: Story = {
  args: {
    invoice: 'lnbc10u1pjexampleinvoicethatisverylongandcontainsmanycharacters0123456789abcdef',
    expirySeconds: 600,
  },
}

export const Expiring: Story = {
  args: {
    invoice: 'lnbc10u1pjexampleinvoicethatisverylongandcontainsmanycharacters0123456789abcdef',
    expirySeconds: 30,
  },
}
