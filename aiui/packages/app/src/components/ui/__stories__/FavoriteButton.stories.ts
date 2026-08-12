import type { Meta, StoryObj } from '@storybook/vue3'
import { ref } from 'vue'
import FavoriteButton from '../FavoriteButton.vue'

const meta: Meta<typeof FavoriteButton> = {
  title: 'UI/FavoriteButton',
  component: FavoriteButton,
  tags: ['autodocs'],
}

export default meta
type Story = StoryObj<typeof FavoriteButton>

export const Unfavorited: Story = {
  render: () => ({
    components: { FavoriteButton },
    setup() {
      const favorited = ref(false)
      return { favorited }
    },
    template: '<FavoriteButton :favorited="favorited" @toggle="favorited = !favorited" />',
  }),
}

export const Favorited: Story = {
  render: () => ({
    components: { FavoriteButton },
    setup() {
      const favorited = ref(true)
      return { favorited }
    },
    template: '<FavoriteButton :favorited="favorited" @toggle="favorited = !favorited" />',
  }),
}
