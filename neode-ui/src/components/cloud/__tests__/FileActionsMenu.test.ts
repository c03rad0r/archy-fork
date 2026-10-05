import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import FileActionsMenu from '../FileActionsMenu.vue'
import FileCard from '../FileCard.vue'
import FileCardGrid from '../FileCardGrid.vue'
vi.mock('@/stores/cloud', () => ({ useCloudStore: () => ({ downloadUrl: (p: string) => '/download' + p }) }))
vi.mock('@/composables/useAudioPlayer', () => ({ useAudioPlayer: () => ({ playing: { value: false }, currentSrc: { value: '' } }) }))
const wrappers: ReturnType<typeof mount>[] = []
function render(component: any, props: any) { const w = mount(component, { props, attachTo: document.body }); wrappers.push(w); return w }
afterEach(() => { wrappers.forEach(w => w.unmount()); wrappers.length = 0; document.body.innerHTML = '' })
async function openMenu() { (document.querySelector('.file-actions-trigger') as HTMLButtonElement).click(); await flushPromises() }
function action(text: string) { return Array.from(document.querySelectorAll('.file-actions-panel button')).find(e => e.textContent === text) as HTMLButtonElement }
describe('visible file actions', () => {
 for (const card of [FileCard, FileCardGrid]) {
  it(`${card.__name} menu never opens a file and forwards share/delete correctly`, async () => {
   const item = { name: 'photo.jpg', path: '/photo.jpg', extension: 'jpg', isDir: false, size: 123, modified: '2026-10-05' }
   const w = render(card, { item }); await openMenu()
   expect(w.emitted('preview')).toBeUndefined()
   expect(document.querySelector('.file-actions-panel a')?.getAttribute('href')).toBe('/download/photo.jpg')
   action('Share with peers').click(); await flushPromises()
   expect(w.emitted('share')).toEqual([['/photo.jpg', 'photo.jpg', false]])
   expect(w.emitted('preview')).toBeUndefined()
   await openMenu(); action('Delete…').click(); await flushPromises()
   expect(w.emitted('delete')).toBeUndefined()
   action('Keep file').click(); await flushPromises()
   expect(w.emitted('delete')).toBeUndefined()
   action('Delete…').click(); await flushPromises(); action('Delete file').click(); await flushPromises()
   expect(w.emitted('delete')).toEqual([['/photo.jpg']])
   expect(w.emitted('preview')).toBeUndefined()
  })
 }
 it('omits actions that the caller cannot offer', async () => {
  render(FileActionsMenu, { name: 'Purchased image.jpg', downloadHref: 'blob:owned' }); await openMenu()
  expect(action('Delete…')).toBeUndefined(); expect(action('Share with peers')).toBeUndefined()
  expect(document.querySelector('.file-actions-panel a')).toBeTruthy()
 })
 it('dismisses on Escape and restores the trigger focus', async () => {
  render(FileActionsMenu, { name: 'file', canShare: true }); await openMenu()
  action('Share with peers').dispatchEvent(new KeyboardEvent('keydown', {key:'Escape', bubbles:true})); await flushPromises()
  expect(document.querySelector('.file-actions-panel')).toBeNull()
  expect(document.activeElement).toBe(document.querySelector('.file-actions-trigger'))
 })
 it('renders inside the fullscreen element so the menu remains reachable', async () => {
  const host = document.createElement('div');document.body.appendChild(host)
  Object.defineProperty(document, 'fullscreenElement', { configurable: true, value: host })
  try { render(FileActionsMenu, {name:'file', canShare:true}); await openMenu();expect(host.querySelector('.file-actions-panel')).toBeTruthy() }
  finally { Object.defineProperty(document, 'fullscreenElement', { configurable:true,value:null }) }
 })
})
