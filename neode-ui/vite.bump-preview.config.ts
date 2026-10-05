import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import path from 'node:path'
export default defineConfig({
  root: path.resolve(__dirname, 'previews/fee-bump'),
  base: '/fee-bump-preview/', publicDir: false, plugins: [vue()],
  resolve: { alias: [
    {find: '@/api/rpc-client', replacement: path.resolve(__dirname, 'previews/fee-bump/rpc.ts')},
    {find: '@/composables/useTxExplorer', replacement: path.resolve(__dirname, 'previews/fee-bump/explorer.ts')},
    {find: '@', replacement: path.resolve(__dirname, 'src')},
  ] },
  build: { outDir: '/tmp/archy-fee-bump-preview', emptyOutDir: true },
})
