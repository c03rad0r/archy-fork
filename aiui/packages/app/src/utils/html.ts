/** Check if a URL has a safe http(s) protocol */
export function isSafeImgSrc(src: string): boolean {
  try {
    const u = new URL(src)
    return /^https?:$/i.test(u.protocol)
  } catch {
    return false
  }
}

/** Type-guard variant: check if value is a defined, safe http(s) URL */
export function isSafeUrl(u: string | undefined): u is string {
  return !!u && typeof u === 'string' && /^https?:\/\//i.test(u.trim())
}

import DOMPurify from 'dompurify'

const ALLOWED_TAGS_LIST = ['p', 'br', 'a', 'strong', 'em', 'b', 'i', 'ul', 'ol', 'li', 'blockquote', 'h1', 'h2', 'h3', 'h4', 'span', 'div']

/** Sanitize HTML using DOMPurify with restricted tag set */
export function sanitizeHtml(html: string): string {
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS: ALLOWED_TAGS_LIST,
    ALLOWED_ATTR: ['href', 'src', 'target', 'rel'],
  })
}

/** Escape HTML entities for safe rendering in a text context */
export function escapeHtml(text: string): string {
  return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

/** Extract a domain from a URL, stripping www prefix */
export function formatDomain(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, '')
  } catch {
    return url
  }
}
