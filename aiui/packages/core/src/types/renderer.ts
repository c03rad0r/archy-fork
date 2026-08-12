import type { Component } from 'vue'

export type SurfaceType =
  | 'chat-preview'
  | 'chat-play'
  | 'panel-preview'
  | 'panel-play'
  | 'panel-edit'

export interface RendererDefinition {
  id: string
  name: string
  contentType: string
  surfaces: SurfaceType[]
  chatPreview?: Component | (() => Promise<Component>)
  chatPlay?: Component | (() => Promise<Component>)
  panelPreview?: Component | (() => Promise<Component>)
  panelPlay?: Component | (() => Promise<Component>)
  panelEdit?: Component | (() => Promise<Component>)
}
