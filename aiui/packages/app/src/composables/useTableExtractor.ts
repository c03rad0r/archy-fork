/**
 * Extract markdown tables from text into structured data
 * for interactive table rendering.
 */

export interface TableData {
  headers: string[]
  rows: string[][]
  raw: string
}

// Matches a markdown table: header row, separator row, data rows
const TABLE_RE = /^(\|[^\n]+\|)\n(\|[\s:|-]+\|)\n((?:\|[^\n]+\|\n?)+)/gm

export function extractTables(text: string): TableData[] {
  const results: TableData[] = []
  let m: RegExpExecArray | null
  const re = new RegExp(TABLE_RE.source, TABLE_RE.flags)

  while ((m = re.exec(text)) !== null) {
    const headerLine = m[1]
    const dataBlock = m[3]

    const headers = parseRow(headerLine)
    const rows = dataBlock
      .trim()
      .split('\n')
      .map(parseRow)
      .filter((r) => r.length > 0)

    if (headers.length > 0 && rows.length > 0) {
      results.push({ headers, rows, raw: m[0] })
    }
  }

  return results
}

function parseRow(line: string): string[] {
  return line
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map((cell) => cell.trim())
}

export function hasTables(text: string): boolean {
  return TABLE_RE.test(text)
}
