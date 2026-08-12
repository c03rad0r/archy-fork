export const APP_STORE_CATEGORIES = [
  { id: 'all', name: 'All' },
  { id: 'community', name: 'Community' },
  { id: 'nostr', name: 'Nostr' },
  { id: 'commerce', name: 'Commerce' },
  { id: 'money', name: 'Money' },
  { id: 'data', name: 'Data' },
  { id: 'home', name: 'Home' },
  { id: 'networking', name: 'Networking' },
  { id: 'other', name: 'Other' },
] as const

export const APP_STORE_SECTIONS = [
  { id: 'discover', name: 'Discover' },
  ...APP_STORE_CATEGORIES,
] as const
