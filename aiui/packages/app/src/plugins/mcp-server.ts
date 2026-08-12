/**
 * MCP (Model Context Protocol) server plugin.
 * Exposes AIUI capabilities as MCP tools that Claude can call via tool_use.
 */
import { mockFilms } from '@/mocks/films'
import { mockSongs } from '@/mocks/songs'
import { mockPodcasts } from '@/mocks/podcasts'

// Demo-site content pack (operator decision 2026-08-07): demo/dev only.
const demoFilms = (import.meta.env.DEV || import.meta.env.VITE_DEMO_CONTENT === 'true') ? mockFilms : []
const demoSongs = (import.meta.env.DEV || import.meta.env.VITE_DEMO_CONTENT === 'true') ? mockSongs : []
const demoPodcasts = (import.meta.env.DEV || import.meta.env.VITE_DEMO_CONTENT === 'true') ? mockPodcasts : []

export interface MCPTool {
  name: string
  description: string
  input_schema: {
    type: 'object'
    properties: Record<string, { type: string; description: string }>
    required?: string[]
  }
}

export interface MCPToolCall {
  id: string
  name: string
  input: Record<string, unknown>
}

export interface MCPToolResult {
  type: 'tool_result'
  tool_use_id: string
  content: string
}

/** Available MCP tools exposed by AIUI */
export const mcpTools: MCPTool[] = [
  {
    name: 'search_films',
    description: 'Search the user\'s film library by title, director, or genre',
    input_schema: {
      type: 'object',
      properties: {
        query: { type: 'string', description: 'Search query (title, director, or genre)' },
      },
      required: ['query'],
    },
  },
  {
    name: 'search_songs',
    description: 'Search the user\'s song library by title, artist, or genre',
    input_schema: {
      type: 'object',
      properties: {
        query: { type: 'string', description: 'Search query (title, artist, or genre)' },
      },
      required: ['query'],
    },
  },
  {
    name: 'search_podcasts',
    description: 'Search the user\'s podcast library by title, host, or genre',
    input_schema: {
      type: 'object',
      properties: {
        query: { type: 'string', description: 'Search query (title, host, or genre)' },
      },
      required: ['query'],
    },
  },
  {
    name: 'get_library_stats',
    description: 'Get statistics about the user\'s content library',
    input_schema: {
      type: 'object',
      properties: {},
    },
  },
]

/** Route a tool call to the appropriate handler and return the result */
export function handleToolCall(call: MCPToolCall): MCPToolResult {
  const handler = toolHandlers[call.name]
  if (!handler) {
    return {
      type: 'tool_result',
      tool_use_id: call.id,
      content: JSON.stringify({ error: `Unknown tool: ${call.name}` }),
    }
  }

  const result = handler(call.input)
  return {
    type: 'tool_result',
    tool_use_id: call.id,
    content: typeof result === 'string' ? result : JSON.stringify(result),
  }
}

type ToolHandler = (input: Record<string, unknown>) => unknown

const toolHandlers: Record<string, ToolHandler> = {
  search_films(input) {
    const query = (input.query as string ?? '').toLowerCase()
    const results = demoFilms.filter(f =>
      f.title.toLowerCase().includes(query) ||
      f.director.toLowerCase().includes(query) ||
      f.genres.some(g => g.toLowerCase().includes(query)),
    )
    return {
      count: results.length,
      films: results.slice(0, 10).map(f => ({
        id: f.id,
        title: f.title,
        year: f.year,
        director: f.director,
        genres: f.genres,
        rating: f.rating,
      })),
    }
  },

  search_songs(input) {
    const query = (input.query as string ?? '').toLowerCase()
    const results = demoSongs.filter(s =>
      s.title.toLowerCase().includes(query) ||
      s.artist.toLowerCase().includes(query) ||
      (s.genres ?? []).some(g => g.toLowerCase().includes(query)),
    )
    return {
      count: results.length,
      songs: results.slice(0, 10).map(s => ({
        id: s.id,
        title: s.title,
        artist: s.artist,
        album: s.album,
        year: s.year,
        genres: s.genres,
      })),
    }
  },

  search_podcasts(input) {
    const query = (input.query as string ?? '').toLowerCase()
    const results = demoPodcasts.filter(p =>
      p.title.toLowerCase().includes(query) ||
      (p.host ?? '').toLowerCase().includes(query) ||
      (p.genres ?? []).some(g => g.toLowerCase().includes(query)),
    )
    return {
      count: results.length,
      podcasts: results.slice(0, 10).map(p => ({
        id: p.id,
        title: p.title,
        host: p.host,
        year: p.year,
        genres: p.genres,
      })),
    }
  },

  get_library_stats() {
    return {
      films: demoFilms.length,
      songs: demoSongs.length,
      podcasts: demoPodcasts.length,
      genres: {
        film: [...new Set(demoFilms.flatMap(f => f.genres))].sort(),
        song: [...new Set(demoSongs.flatMap(s => s.genres ?? []))].sort(),
        podcast: [...new Set(demoPodcasts.flatMap(p => p.genres ?? []))].sort(),
      },
    }
  },
}

/** Parse tool_use blocks from a Claude SSE response and check if tool calls are present */
export function parseToolUseBlocks(content: unknown[]): MCPToolCall[] {
  if (!Array.isArray(content)) return []
  return content
    .filter((block): block is { type: 'tool_use'; id: string; name: string; input: Record<string, unknown> } =>
      typeof block === 'object' && block !== null && (block as { type: string }).type === 'tool_use',
    )
    .map(block => ({
      id: block.id,
      name: block.name,
      input: block.input,
    }))
}
