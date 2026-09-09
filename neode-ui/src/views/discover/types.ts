import type { MarketplaceAppInfo } from '@/composables/useMarketplaceApp'

/** Container config that can be passed from the remote catalog to the backend
 *  for apps not hardcoded in config.rs */
export interface ContainerConfig {
  ports?: string[]
  volumes?: string[]
  env?: string[]
  command?: string
  args?: string[]
}

export type MarketplaceApp = Partial<MarketplaceAppInfo> & {
  id: string
  trustScore?: number
  trustTier?: string
  relayCount?: number
  containerConfig?: ContainerConfig
  requires?: string[]
  tier?: string
  maintainerNpub?: string
}

export type FeaturedApp = MarketplaceApp & {
  featuredDescription: string
  privacyTag: string
  bannerImage?: string
}

export interface InstallProgress {
  id: string
  title: string
  status: 'downloading' | 'installing' | 'starting' | 'complete' | 'error'
  progress: number
  message: string
  attempt: number
}

export interface CategoryDef {
  id: string
  name: string
}
