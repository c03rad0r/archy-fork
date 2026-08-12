import type { Meta, StoryObj } from '@storybook/vue3'
import SearchResults from '../SearchResults.vue'

const meta: Meta<typeof SearchResults> = {
  title: 'UI/SearchResults',
  component: SearchResults,
  tags: ['autodocs'],
  decorators: [
    () => ({
      template: '<div style="position:relative;height:400px;width:360px;"><story /></div>',
    }),
  ],
}

export default meta
type Story = StoryObj<typeof SearchResults>

export const WithResults: Story = {
  args: {
    isSearching: false,
    results: [
      { id: '1', title: 'Blade Runner 2049', subtitle: 'Denis Villeneuve · 2017', type: 'film', data: null },
      { id: '2', title: 'Arrival', subtitle: 'Denis Villeneuve · 2016', type: 'film', data: null },
      { id: '3', title: 'Never Meant', subtitle: 'American Football', type: 'song', data: null },
      { id: '4', title: 'What Bitcoin Did', subtitle: 'Peter McCormack', type: 'podcast', data: null },
    ],
  },
}

export const Searching: Story = {
  args: {
    isSearching: true,
    results: [],
  },
}
