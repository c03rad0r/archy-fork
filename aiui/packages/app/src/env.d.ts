/// <reference types="vite/client" />

declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<object, object, unknown>
  export default component
}

interface ImportMetaEnv {
  readonly VITE_OPENROUTER_API_KEY: string
  readonly VITE_ANTHROPIC_API_KEY: string
  readonly VITE_ANTHROPIC_TOKEN: string
  readonly VITE_TMDB_API_KEY: string
  readonly VITE_DEV_MODE: string
  readonly VITE_MOCK_MEDIA_SOURCES: string
  readonly VITE_DISABLE_CRYPTO: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
