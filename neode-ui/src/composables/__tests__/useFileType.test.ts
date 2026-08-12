import { describe, it, expect } from 'vitest'
import { ref } from 'vue'
import { getFileCategory, useFileType, formatSize, formatDate } from '../useFileType'

describe('getFileCategory', () => {
  it('returns folder for directories', () => {
    expect(getFileCategory('', true)).toBe('folder')
    expect(getFileCategory('jpg', true)).toBe('folder')
  })

  it('identifies image extensions', () => {
    expect(getFileCategory('jpg', false)).toBe('image')
    expect(getFileCategory('jpeg', false)).toBe('image')
    expect(getFileCategory('png', false)).toBe('image')
    expect(getFileCategory('gif', false)).toBe('image')
    expect(getFileCategory('webp', false)).toBe('image')
    expect(getFileCategory('svg', false)).toBe('image')
    expect(getFileCategory('bmp', false)).toBe('image')
    expect(getFileCategory('ico', false)).toBe('image')
  })

  it('identifies audio extensions', () => {
    expect(getFileCategory('mp3', false)).toBe('audio')
    expect(getFileCategory('flac', false)).toBe('audio')
    expect(getFileCategory('wav', false)).toBe('audio')
    expect(getFileCategory('ogg', false)).toBe('audio')
    expect(getFileCategory('aac', false)).toBe('audio')
    expect(getFileCategory('m4a', false)).toBe('audio')
  })

  it('identifies video extensions', () => {
    expect(getFileCategory('mp4', false)).toBe('video')
    expect(getFileCategory('mkv', false)).toBe('video')
    expect(getFileCategory('avi', false)).toBe('video')
    expect(getFileCategory('mov', false)).toBe('video')
    expect(getFileCategory('webm', false)).toBe('video')
  })

  it('identifies document extensions', () => {
    expect(getFileCategory('pdf', false)).toBe('document')
    expect(getFileCategory('doc', false)).toBe('document')
    expect(getFileCategory('docx', false)).toBe('document')
    expect(getFileCategory('txt', false)).toBe('document')
    expect(getFileCategory('md', false)).toBe('document')
  })

  it('identifies spreadsheet extensions', () => {
    expect(getFileCategory('xls', false)).toBe('spreadsheet')
    expect(getFileCategory('xlsx', false)).toBe('spreadsheet')
    expect(getFileCategory('csv', false)).toBe('spreadsheet')
    expect(getFileCategory('ods', false)).toBe('spreadsheet')
  })

  it('identifies archive extensions', () => {
    expect(getFileCategory('zip', false)).toBe('archive')
    expect(getFileCategory('tar', false)).toBe('archive')
    expect(getFileCategory('gz', false)).toBe('archive')
    expect(getFileCategory('rar', false)).toBe('archive')
    expect(getFileCategory('7z', false)).toBe('archive')
  })

  it('returns file for unknown extensions', () => {
    expect(getFileCategory('xyz', false)).toBe('file')
    expect(getFileCategory('', false)).toBe('file')
    expect(getFileCategory('bin', false)).toBe('file')
  })
})

describe('useFileType', () => {
  it('returns correct category and computed values for an image', () => {
    const ext = ref('jpg')
    const isDir = ref(false)
    const result = useFileType(ext, isDir)

    expect(result.category.value).toBe('image')
    expect(result.isImage.value).toBe(true)
    expect(result.isAudio.value).toBe(false)
    expect(result.isVideo.value).toBe(false)
    expect(result.iconColor.value).toBe('text-blue-400')
    expect(result.badgeLabel.value).toBe('Image')
  })

  it('returns correct values for audio', () => {
    const ext = ref('mp3')
    const isDir = ref(false)
    const result = useFileType(ext, isDir)

    expect(result.category.value).toBe('audio')
    expect(result.isAudio.value).toBe(true)
    expect(result.isImage.value).toBe(false)
    expect(result.iconColor.value).toBe('text-orange-400')
    expect(result.badgeLabel.value).toBe('Audio')
  })

  it('returns correct values for video', () => {
    const ext = ref('mp4')
    const isDir = ref(false)
    const result = useFileType(ext, isDir)

    expect(result.category.value).toBe('video')
    expect(result.isVideo.value).toBe(true)
    expect(result.iconColor.value).toBe('text-purple-400')
  })

  it('returns folder when isDir is true', () => {
    const ext = ref('jpg')
    const isDir = ref(true)
    const result = useFileType(ext, isDir)

    expect(result.category.value).toBe('folder')
    expect(result.isImage.value).toBe(false)
    expect(result.iconColor.value).toBe('text-amber-400')
    expect(result.badgeLabel.value).toBe('Folder')
  })

  it('reacts to ref changes', () => {
    const ext = ref('jpg')
    const isDir = ref(false)
    const result = useFileType(ext, isDir)

    expect(result.category.value).toBe('image')

    ext.value = 'mp3'
    expect(result.category.value).toBe('audio')
    expect(result.isAudio.value).toBe(true)
    expect(result.isImage.value).toBe(false)
  })

  it('provides icon paths for each category', () => {
    const ext = ref('pdf')
    const isDir = ref(false)
    const result = useFileType(ext, isDir)

    expect(result.iconPaths.value).toBeDefined()
    expect(result.iconPaths.value.length).toBeGreaterThan(0)
  })

  it('provides badge class for each category', () => {
    const ext = ref('zip')
    const isDir = ref(false)
    const result = useFileType(ext, isDir)

    expect(result.badgeClass.value).toContain('bg-yellow')
  })
})

describe('formatSize', () => {
  it('formats 0 bytes', () => {
    expect(formatSize(0)).toBe('0 B')
  })

  it('formats bytes', () => {
    expect(formatSize(500)).toBe('500 B')
  })

  it('formats kilobytes', () => {
    expect(formatSize(1024)).toBe('1.0 KB')
    expect(formatSize(1536)).toBe('1.5 KB')
  })

  it('formats megabytes', () => {
    expect(formatSize(1048576)).toBe('1.0 MB')
  })

  it('formats gigabytes', () => {
    expect(formatSize(1073741824)).toBe('1.0 GB')
  })

  it('formats terabytes', () => {
    expect(formatSize(1099511627776)).toBe('1.0 TB')
  })
})

describe('formatDate', () => {
  it('returns "Just now" for very recent dates', () => {
    const now = new Date().toISOString()
    expect(formatDate(now)).toBe('Just now')
  })

  it('returns minutes ago for recent dates', () => {
    const fiveMinAgo = new Date(Date.now() - 5 * 60 * 1000).toISOString()
    expect(formatDate(fiveMinAgo)).toBe('5m ago')
  })

  it('returns hours ago for dates within 24h', () => {
    const threeHoursAgo = new Date(Date.now() - 3 * 60 * 60 * 1000).toISOString()
    expect(formatDate(threeHoursAgo)).toBe('3h ago')
  })

  it('returns days ago for dates within a week', () => {
    const twoDaysAgo = new Date(Date.now() - 2 * 24 * 60 * 60 * 1000).toISOString()
    expect(formatDate(twoDaysAgo)).toBe('2d ago')
  })

  it('returns formatted date for older dates', () => {
    const oldDate = new Date('2025-01-15').toISOString()
    const result = formatDate(oldDate)
    // Should be a locale date string, not a relative time
    expect(result).toMatch(/\d/)
    expect(result).not.toContain('ago')
  })
})
