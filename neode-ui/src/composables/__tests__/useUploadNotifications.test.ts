import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, nextTick } from 'vue'
import { createRouter, createMemoryHistory } from 'vue-router'
import { useCloudStore } from '@/stores/cloud'
import { useUploadNotifications } from '../useUploadNotifications'
import UploadProgress from '@/components/cloud/UploadProgress.vue'
const action = vi.hoisted(() => vi.fn())
vi.mock('../useToast', () => ({ useToast: () => ({ action }) }))
const origin = '/dashboard/cloud/documents?path=/uploads'
async function setup() {
  const pinia = createPinia(); setActivePinia(pinia)
  const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/:pathMatch(.*)*', component: { template: '<div />' } }] })
  await router.push(origin); await router.isReady()
  const wrapper = mount(defineComponent({ components: { UploadProgress }, setup() { useUploadNotifications() }, template: '<UploadProgress />' }), { global: { plugins: [pinia, router] } })
  const store = useCloudStore()
  store.upload = { active: true, paused: false, filename: 'photo.png', destination: '/uploads', originRoute: origin, sent: 100, total: 100, completed: 0, count: 1, error: null, cancelled: false }
  await nextTick()
  return { store, router, wrapper }
}
beforeEach(() => action.mockClear())
describe('upload screen and completion notification', () => {
  it('shows Saving until the server finishes, then Complete with dismiss on the origin only', async () => {
    const { store, wrapper } = await setup()
    expect(wrapper.text()).toContain('Saving photo.png'); expect(action).not.toHaveBeenCalled()
    store.upload!.completed = 1; store.upload!.active = false; await nextTick()
    expect(wrapper.text()).toContain('Complete · photo.png'); expect(action).not.toHaveBeenCalled()
    await wrapper.get('button[aria-label="Dismiss upload"]').trigger('click')
    expect(store.upload).toBeNull(); wrapper.unmount()
  })
  it('hides progress on another screen, notifies once on completion and returns to the origin', async () => {
    const { store, router, wrapper } = await setup()
    await router.push('/dashboard/apps'); expect(wrapper.find('[role="status"]').exists()).toBe(false)
    expect(store.upload!.active).toBe(true)
    store.upload!.completed = 1; store.upload!.active = false; await nextTick()
    expect(action).toHaveBeenCalledTimes(1)
    expect(action.mock.calls[0]![0]).toBe('Upload complete · photo.png')
    action.mock.calls[0]![1].onClick(); await router.isReady(); await new Promise(resolve => setTimeout(resolve, 0))
    expect(router.currentRoute.value.fullPath).toBe(origin)
    expect(wrapper.text()).toContain('Complete · photo.png')
    await router.push('/dashboard'); expect(action).toHaveBeenCalledTimes(1); wrapper.unmount()
  })
  it('also hides on other Cloud folders and shows progress again on return', async () => {
    const { router, wrapper } = await setup()
    await router.push('/dashboard/cloud/documents?path=/different')
    expect(wrapper.find('[role="status"]').exists()).toBe(false)
    await router.push(origin); expect(wrapper.text()).toContain('Saving'); wrapper.unmount()
  })
  it.each(['error', 'cancelled'] as const)('never calls a %s outcome complete', async field => {
    const { store, router, wrapper } = await setup(); await router.push('/dashboard')
    if (field === 'error') store.upload!.error = 'No space'; else store.upload!.cancelled = true
    store.upload!.active = false; await nextTick()
    expect(action).toHaveBeenCalledTimes(1); expect(action.mock.calls[0]![0]).not.toContain('complete')
    expect(action.mock.calls[0]![0]).toContain('0/1 saved'); wrapper.unmount()
  })
})
