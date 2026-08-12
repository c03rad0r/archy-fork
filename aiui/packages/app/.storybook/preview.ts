import type { Preview } from '@storybook/vue3'
import '../src/styles/main.css'

const preview: Preview = {
  parameters: {
    backgrounds: {
      default: 'dark',
      values: [
        { name: 'dark', value: '#0a0a0a' },
      ],
    },
    layout: 'centered',
  },
}

export default preview
