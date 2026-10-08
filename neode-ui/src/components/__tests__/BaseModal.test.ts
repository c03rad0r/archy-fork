import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createRouter, createMemoryHistory } from 'vue-router'
import BaseModal from '../BaseModal.vue'

describe('BaseModal', () => {
  it('lets a parent flow own history while panels change', async () => {
    const push = vi.spyOn(window.history, 'pushState')
    const back = vi.spyOn(window.history, 'back')
    const wrapper = mount(BaseModal, { props: { show: false, title: 'Panel', manageHistory: false } })
    await wrapper.setProps({ show: true })
    await wrapper.setProps({ show: false })
    expect(push).not.toHaveBeenCalled()
    expect(back).not.toHaveBeenCalled()
    wrapper.unmount(); push.mockRestore(); back.mockRestore()
  })

  afterEach(() => {
    document.body.style.overflow = ''
  })

  it('locks page scroll while open and restores it when closed', async () => {
    const wrapper = mount(BaseModal, {
      props: { show: true, title: 'Test modal' },
      slots: { default: '<p>Modal content</p>' },
      attachTo: document.body,
    })

    expect(document.body.style.overflow).toBe('hidden')

    await wrapper.setProps({ show: false })
    expect(document.body.style.overflow).toBe('')

    wrapper.unmount()
  })

  it('closes itself when the route changes', async () => {
    // Tab views are KeepAlive'd, so navigating away deactivates the owner
    // rather than unmounting it — a Teleported modal would otherwise keep
    // floating over the destination screen. Seen with the Lightning modal's
    // "Open a channel" / "Setup Guide" actions, which route away from inside
    // the wallet's own send/receive modal.
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: '/', component: { template: '<div />' } },
        { path: '/elsewhere', component: { template: '<div />' } },
      ],
    })
    await router.push('/')
    await router.isReady()

    const wrapper = mount(BaseModal, {
      props: { show: true, title: 'Test modal' },
      slots: { default: '<p>Modal content</p>' },
      global: { plugins: [router] },
    })
    expect(wrapper.emitted('close')).toBeUndefined()

    await router.push('/elsewhere')
    await wrapper.vm.$nextTick()

    expect(wrapper.emitted('close')).toHaveLength(1)
    wrapper.unmount()
  })

  it('does not emit close on a route change while hidden', async () => {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: '/', component: { template: '<div />' } },
        { path: '/elsewhere', component: { template: '<div />' } },
      ],
    })
    await router.push('/')
    await router.isReady()

    const wrapper = mount(BaseModal, {
      props: { show: false, title: 'Test modal' },
      global: { plugins: [router] },
    })
    await router.push('/elsewhere')
    await wrapper.vm.$nextTick()

    expect(wrapper.emitted('close')).toBeUndefined()
    wrapper.unmount()
  })
})
