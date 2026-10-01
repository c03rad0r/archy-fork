import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'

vi.mock('@/api/filebrowser-client', () => ({
  fileBrowserClient: {
    login: vi.fn(),
    listDirectory: vi.fn(),
    upload: vi.fn(),
    deleteItem: vi.fn(),
    downloadUrl: vi.fn(),
    createFolder: vi.fn(),
  },
}))

import { useCloudStore } from '../cloud'
import { fileBrowserClient } from '@/api/filebrowser-client'

const mockedClient = vi.mocked(fileBrowserClient)

const mockItems = [
  { name: 'photos', path: '/photos', size: 0, modified: '2026-01-01', isDir: true, type: '', extension: '' },
  { name: 'readme.md', path: '/readme.md', size: 256, modified: '2026-01-02', isDir: false, type: '', extension: 'md' },
  { name: 'archive.zip', path: '/archive.zip', size: 4096, modified: '2026-01-03', isDir: false, type: '', extension: 'zip' },
]

describe('useCloudStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('starts with default state', () => {
    const store = useCloudStore()
    expect(store.currentPath).toBe('/')
    expect(store.items).toEqual([])
    expect(store.loading).toBe(false)
    expect(store.error).toBeNull()
    expect(store.authenticated).toBe(false)
  })

  it('init authenticates with filebrowser', async () => {
    mockedClient.login.mockResolvedValue(true)
    const store = useCloudStore()

    const result = await store.init()

    expect(result).toBe(true)
    expect(store.authenticated).toBe(true)
    expect(mockedClient.login).toHaveBeenCalledOnce()
  })

  it('init returns false on auth failure', async () => {
    mockedClient.login.mockResolvedValue(false)
    const store = useCloudStore()

    const result = await store.init()

    expect(result).toBe(false)
    expect(store.authenticated).toBe(false)
  })

  it('init skips login if already authenticated', async () => {
    mockedClient.login.mockResolvedValue(true)
    const store = useCloudStore()

    await store.init()
    await store.init()

    expect(mockedClient.login).toHaveBeenCalledOnce()
  })

  it('navigate loads items and updates path', async () => {
    mockedClient.login.mockResolvedValue(true)
    mockedClient.listDirectory.mockResolvedValue(mockItems)
    const store = useCloudStore()
    store.authenticated = true

    await store.navigate('/photos')

    expect(store.items).toEqual(mockItems)
    expect(store.currentPath).toBe('/photos')
    expect(store.loading).toBe(false)
    expect(store.error).toBeNull()
  })

  it('navigate authenticates if not authenticated', async () => {
    mockedClient.login.mockResolvedValue(true)
    mockedClient.listDirectory.mockResolvedValue(mockItems)
    const store = useCloudStore()

    await store.navigate('/')

    expect(mockedClient.login).toHaveBeenCalled()
    expect(store.items).toEqual(mockItems)
  })

  it('navigate sets error on auth failure', async () => {
    mockedClient.login.mockResolvedValue(false)
    const store = useCloudStore()

    await store.navigate('/')

    expect(store.error).toBe('Failed to authenticate with File Browser')
    expect(store.loading).toBe(false)
  })

  it('navigate falls back to creating directory on list failure', async () => {
    const store = useCloudStore()
    store.authenticated = true

    // First listDirectory fails, then createFolder succeeds, then retry succeeds
    mockedClient.listDirectory
      .mockRejectedValueOnce(new Error('Not found'))
      .mockResolvedValueOnce([])
    mockedClient.createFolder.mockResolvedValue(undefined)

    await store.navigate('/new-folder')

    expect(mockedClient.createFolder).toHaveBeenCalledWith('/', 'new-folder')
    expect(store.currentPath).toBe('/new-folder')
  })

  it('navigate falls back to root when directory creation also fails', async () => {
    const store = useCloudStore()
    store.authenticated = true

    // Call 1: listDirectory('/deep/nested') rejects
    // Call 2: listDirectory('/') in the fallback catch resolves
    mockedClient.listDirectory
      .mockRejectedValueOnce(new Error('Not found'))
      .mockResolvedValueOnce(mockItems)

    mockedClient.createFolder.mockRejectedValueOnce(new Error('Create failed'))

    await store.navigate('/deep/nested')

    expect(store.currentPath).toBe('/')
    expect(store.items).toEqual(mockItems)
  })

  it('navigate sets error when root listing fails', async () => {
    const store = useCloudStore()
    store.authenticated = true

    mockedClient.listDirectory.mockRejectedValueOnce(new Error('Server error'))

    await store.navigate('/')

    expect(store.error).toBe('Failed to list root directory')
  })

  it('breadcrumbs computes from path', () => {
    const store = useCloudStore()
    store.currentPath = '/photos/vacation/2026'

    expect(store.breadcrumbs).toEqual([
      { name: 'Home', path: '/' },
      { name: 'photos', path: '/photos' },
      { name: 'vacation', path: '/photos/vacation' },
      { name: '2026', path: '/photos/vacation/2026' },
    ])
  })

  it('breadcrumbs at root only shows Home', () => {
    const store = useCloudStore()
    expect(store.breadcrumbs).toEqual([{ name: 'Home', path: '/' }])
  })

  it('sortedItems puts directories first, sorted alphabetically', () => {
    const store = useCloudStore()
    store.items = [
      { name: 'readme.md', path: '/readme.md', size: 256, modified: '2026-01-01', isDir: false, type: '', extension: 'md' },
      { name: 'docs', path: '/docs', size: 0, modified: '2026-01-01', isDir: true, type: '', extension: '' },
      { name: 'archive.zip', path: '/archive.zip', size: 4096, modified: '2026-01-01', isDir: false, type: '', extension: 'zip' },
      { name: 'assets', path: '/assets', size: 0, modified: '2026-01-01', isDir: true, type: '', extension: '' },
    ]

    const sorted = store.sortedItems
    expect(sorted.map((i) => i.name)).toEqual(['assets', 'docs', 'archive.zip', 'readme.md'])
  })

  it('uploadFile uploads and refreshes', async () => {
    const store = useCloudStore()
    store.authenticated = true
    store.currentPath = '/uploads'
    mockedClient.upload.mockResolvedValue(undefined)
    mockedClient.listDirectory.mockResolvedValue([])

    const file = new File(['test'], 'test.txt')
    await store.uploadFile(file)

    expect(mockedClient.upload).toHaveBeenCalledWith('/uploads', file)
    expect(mockedClient.listDirectory).toHaveBeenCalled()
  })

  it('deleteItem deletes and refreshes', async () => {
    const store = useCloudStore()
    store.authenticated = true
    store.currentPath = '/'
    mockedClient.deleteItem.mockResolvedValue(undefined)
    mockedClient.listDirectory.mockResolvedValue([])

    await store.deleteItem('/old-file.txt')

    expect(mockedClient.deleteItem).toHaveBeenCalledWith('/old-file.txt')
    expect(mockedClient.listDirectory).toHaveBeenCalled()
  })

  it('downloadUrl delegates to filebrowser client', () => {
    mockedClient.downloadUrl.mockReturnValue('http://localhost/api/raw/file.txt?auth=token')
    const store = useCloudStore()

    const url = store.downloadUrl('/file.txt')

    expect(url).toBe('http://localhost/api/raw/file.txt?auth=token')
    expect(mockedClient.downloadUrl).toHaveBeenCalledWith('/file.txt')
  })

  it('reset clears all state', () => {
    const store = useCloudStore()
    store.currentPath = '/deep/path'
    store.items = mockItems
    store.loading = true
    store.error = 'something'

    store.reset()

    expect(store.currentPath).toBe('/')
    expect(store.items).toEqual([])
    expect(store.loading).toBe(false)
    expect(store.error).toBeNull()
  })
})

describe('persistent upload batch', () => {
  beforeEach(() => { setActivePinia(createPinia()); vi.clearAllMocks() })
  it('retains the destination and byte progress across folder navigation', async () => {
    const store = useCloudStore(); store.authenticated = true; store.currentPath = '/original'
    let release!: () => void
    mockedClient.upload.mockImplementationOnce(async (_path, _file, options) => {
      options!.onProgress(2)
      await new Promise<void>(resolve => { release = resolve })
    }).mockResolvedValueOnce(undefined)
    mockedClient.listDirectory.mockResolvedValue([])
    const job = store.uploadFiles([new File(['abcd'], 'one'), new File(['ef'], 'two')])
    expect(store.upload?.sent).toBe(2)
    expect(store.upload?.total).toBe(6)
    await store.navigate('/elsewhere')
    expect(useCloudStore().upload?.active).toBe(true)
    release(); await job
    expect(mockedClient.upload.mock.calls.map(call => call[0])).toEqual(['/original', '/original'])
    expect(store.currentPath).toBe('/elsewhere')
    expect(store.upload).toMatchObject({ active: false, sent: 6, completed: 2, error: null })
  })
  it('aborts the active request, stops the queue and preserves completed files', async () => {
    const store = useCloudStore(); store.authenticated = true
    mockedClient.listDirectory.mockResolvedValue([])
    mockedClient.upload.mockResolvedValueOnce(undefined).mockImplementationOnce((_path, _file, options) => new Promise((_resolve, reject) => {
      options!.signal.addEventListener('abort', () => reject(new DOMException('Cancelled', 'AbortError')))
    }))
    const files = ['one', 'two', 'three'].map(name => new File(['abc'], name))
    const job = store.uploadFiles(files)
    await Promise.resolve(); await Promise.resolve()
    store.cancelUpload(); await job
    expect(mockedClient.upload).toHaveBeenCalledTimes(2)
    expect(store.upload).toMatchObject({ active: false, completed: 1, cancelled: true, error: null })
    store.dismissUpload(); expect(store.upload).toBeNull()
  })
  it('keeps failures visible and does not start a second overlapping batch', async () => {
    const store = useCloudStore(); store.authenticated = true
    mockedClient.listDirectory.mockResolvedValue([])
    let fail!: (error: Error) => void
    mockedClient.upload.mockImplementationOnce(() => new Promise((_resolve, reject) => { fail = reject }))
    const file = new File(['x'], 'one')
    const job = store.uploadFiles([file]); await store.uploadFiles([file])
    expect(mockedClient.upload).toHaveBeenCalledTimes(1)
    fail(new Error('No space')); await job
    expect(store.upload).toMatchObject({ active: false, error: 'No space', completed: 0 })
  })
})
