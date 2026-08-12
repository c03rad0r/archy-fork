import baseConfig from './vite.config'
const cfg = { ...(baseConfig as Record<string, unknown>) } as any
cfg.server = { ...(cfg.server || {}), port: 8100, strictPort: true }
const proxy = { ...(cfg.server.proxy || {}) }
for (const k of Object.keys(proxy)) proxy[k] = { ...proxy[k], target: 'http://localhost:5959' }
cfg.server.proxy = proxy
export default cfg
