import { computed, type Ref } from 'vue'

const IMAGE_EXTS = new Set(['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'bmp', 'ico'])
const AUDIO_EXTS = new Set(['mp3', 'flac', 'wav', 'ogg', 'aac', 'm4a', 'wma'])
const VIDEO_EXTS = new Set(['mp4', 'mkv', 'avi', 'mov', 'webm', 'wmv', 'flv'])
const DOC_EXTS = new Set(['pdf', 'doc', 'docx', 'txt', 'rtf', 'odt', 'md'])
const SHEET_EXTS = new Set(['xls', 'xlsx', 'csv', 'ods'])
const ARCHIVE_EXTS = new Set(['zip', 'tar', 'gz', 'rar', '7z', 'bz2'])

export type FileCategory = 'folder' | 'image' | 'audio' | 'video' | 'document' | 'spreadsheet' | 'archive' | 'file'

export function getFileCategory(ext: string, isDir: boolean): FileCategory {
  if (isDir) return 'folder'
  if (IMAGE_EXTS.has(ext)) return 'image'
  if (AUDIO_EXTS.has(ext)) return 'audio'
  if (VIDEO_EXTS.has(ext)) return 'video'
  if (DOC_EXTS.has(ext)) return 'document'
  if (SHEET_EXTS.has(ext)) return 'spreadsheet'
  if (ARCHIVE_EXTS.has(ext)) return 'archive'
  return 'file'
}

const CATEGORY_ICONS: Record<FileCategory, string[]> = {
  folder: ['M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z'],
  audio: ['M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3'],
  video: ['M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z', 'M21 12a9 9 0 11-18 0 9 9 0 0118 0z'],
  image: ['M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z'],
  document: ['M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z'],
  spreadsheet: ['M3 10h18M3 14h18M3 6h18M3 18h18M8 6v12M16 6v12'],
  archive: ['M5 8h14M5 8a2 2 0 110-4h14a2 2 0 110 4M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8m-9 4h4'],
  file: ['M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z'],
}

const CATEGORY_COLORS: Record<FileCategory, string> = {
  folder: 'text-amber-400',
  audio: 'text-orange-400',
  video: 'text-purple-400',
  image: 'text-blue-400',
  document: 'text-green-400',
  spreadsheet: 'text-emerald-400',
  archive: 'text-yellow-400',
  file: 'text-white/50',
}

const CATEGORY_LABELS: Record<FileCategory, string> = {
  folder: 'Folder',
  audio: 'Audio',
  video: 'Video',
  image: 'Image',
  document: 'Document',
  spreadsheet: 'Spreadsheet',
  archive: 'Archive',
  file: 'File',
}

const CATEGORY_BADGE_CLASSES: Record<FileCategory, string> = {
  folder: 'bg-amber-500/15 text-amber-400/70',
  audio: 'bg-orange-500/15 text-orange-400/70',
  video: 'bg-purple-500/15 text-purple-400/70',
  image: 'bg-blue-500/15 text-blue-400/70',
  document: 'bg-green-500/15 text-green-400/70',
  spreadsheet: 'bg-emerald-500/15 text-emerald-400/70',
  archive: 'bg-yellow-500/15 text-yellow-400/70',
  file: 'bg-white/8 text-white/50',
}

export function useFileType(ext: Ref<string>, isDir: Ref<boolean>) {
  const category = computed(() => getFileCategory(ext.value, isDir.value))
  const isImage = computed(() => category.value === 'image')
  const isAudio = computed(() => category.value === 'audio')
  const isVideo = computed(() => category.value === 'video')
  const iconPaths = computed(() => CATEGORY_ICONS[category.value])
  const iconColor = computed(() => CATEGORY_COLORS[category.value])
  const badgeLabel = computed(() => CATEGORY_LABELS[category.value])
  const badgeClass = computed(() => CATEGORY_BADGE_CLASSES[category.value])

  return { category, isImage, isAudio, isVideo, iconPaths, iconColor, badgeLabel, badgeClass }
}

export function formatSize(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0)} ${units[i]}`
}

export function formatDate(iso: string): string {
  const date = new Date(iso)
  const now = Date.now()
  const diff = now - date.getTime()
  const mins = Math.floor(diff / 60000)
  if (mins < 1) return 'Just now'
  if (mins < 60) return `${mins}m ago`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  if (days < 7) return `${days}d ago`
  return date.toLocaleDateString()
}
