import { type DeepReadonly, type Ref, ref, readonly } from 'vue'
import type { AIUIPlugin, PluginType } from '../types/plugin'
import type { RendererDefinition } from '../types/renderer'

const plugins = ref<Map<string, AIUIPlugin>>(new Map())
const renderers = ref<Map<string, RendererDefinition>>(new Map())

export function registerPlugin(plugin: AIUIPlugin): void {
  if (plugins.value.has(plugin.id)) {
    console.warn(`Plugin "${plugin.id}" is already registered. Skipping.`)
    return
  }
  plugins.value.set(plugin.id, plugin)
}

export function unregisterPlugin(pluginId: string): void {
  plugins.value.delete(pluginId)
}

export function getPlugin<T extends AIUIPlugin>(pluginId: string): T | undefined {
  return plugins.value.get(pluginId) as T | undefined
}

export function getPluginsByType<T extends AIUIPlugin>(type: PluginType): T[] {
  return Array.from(plugins.value.values()).filter(
    (p) => p.type === type
  ) as T[]
}

export function registerRenderer(renderer: RendererDefinition): void {
  if (renderers.value.has(renderer.id)) {
    console.warn(`Renderer "${renderer.id}" is already registered. Skipping.`)
    return
  }
  renderers.value.set(renderer.id, renderer)
}

export function getRendererForContentType(
  contentType: string
): RendererDefinition | undefined {
  return Array.from(renderers.value.values()).find(
    (r) => r.contentType === contentType
  )
}

export function getAllRenderers(): RendererDefinition[] {
  return Array.from(renderers.value.values())
}

export const pluginRegistry: DeepReadonly<Ref<Map<string, AIUIPlugin>>> = readonly(plugins)
export const rendererRegistry: DeepReadonly<Ref<Map<string, RendererDefinition>>> = readonly(renderers)
