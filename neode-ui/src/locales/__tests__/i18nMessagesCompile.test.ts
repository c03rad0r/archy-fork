// Every message string must survive vue-i18n's message compiler. Found the
// hard way (2026-09-08): a bare `@` in a message is parsed as the start of
// "linked message" syntax (`@:key`), so a literal `@` (an email/handle-style
// placeholder, e.g. "user@example.com") throws a SyntaxError the first time
// it's *rendered*, not at build time — see [[vue-i18n-bare-at-sign-crash]]
// in project memory for the full incident (it blanked a whole modal in both
// the browser and the Android companion's WebView). A literal `@`, `{`, `}`
// or other message-syntax character must be escaped as e.g. `{'@'}`.
//
// This walks every string in every locale file and asks the real compiler
// to parse it — no rendering, no component needed, so it's fast and catches
// the whole class of bug regardless of which component ever ends up using
// the string.
import { describe, it, expect } from 'vitest'
import i18n from '@/i18n'
import en from '../en.json'
import es from '../es.json'

function collectStrings(obj: unknown, path: string, out: Array<[string, string]>) {
  if (typeof obj === 'string') {
    out.push([path, obj])
  } else if (obj && typeof obj === 'object') {
    for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
      collectStrings(v, path ? `${path}.${k}` : k, out)
    }
  }
}

describe('locale messages compile', () => {
  it.each([
    ['en', en],
    ['es', es],
  ])('every %s message string compiles under the real vue-i18n compiler', (_locale, messages) => {
    const strings: Array<[string, string]> = []
    collectStrings(messages, '', strings)
    expect(strings.length).toBeGreaterThan(100)

    const failures: string[] = []
    for (const [path, msg] of strings) {
      try {
        i18n.global.t(path)
      } catch (e) {
        failures.push(`${path}: ${(e as Error).message.split('\n')[0]} (source: ${JSON.stringify(msg)})`)
      }
    }
    expect(failures).toEqual([])
  })
})
