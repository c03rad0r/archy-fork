import type { StorybookConfig } from '@storybook/vue3-vite'

const config: StorybookConfig = {
  stories: ['../src/**/__stories__/*.stories.ts'],
  framework: {
    name: '@storybook/vue3-vite',
    options: {},
  },
  addons: ['@storybook/addon-essentials'],
  viteFinal(config) {
    config.resolve ??= {}
    config.resolve.alias ??= {}
    // Match app aliases
    const alias = config.resolve.alias as Record<string, string>
    alias['@'] = new URL('../src', import.meta.url).pathname
    alias['@aiui/core'] = new URL('../../core/src', import.meta.url).pathname
    return config
  },
}

export default config
