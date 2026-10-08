// Cloud's local library currently uses File Browser, including legacy container IDs.
export function isCloudStorageApp(appId: string): boolean {
  return appId === 'filebrowser' || appId === 'archy-filebrowser'
}
