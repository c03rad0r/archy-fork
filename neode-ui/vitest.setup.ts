import { config } from '@vue/test-utils'
import { displayVersion } from '@/utils/version'

// The app registers `$ver` as a global template property in main.ts
// (app.config.globalProperties.$ver = displayVersion). Component tests mount
// without that bootstrap, so provide the same global here for every mounted
// component. Per-mount `global` options merge with this, so individual tests
// keep their own plugins/mocks.
config.global.mocks = { ...(config.global.mocks ?? {}), $ver: displayVersion }
