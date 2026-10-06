import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import AppSessionFrame from '../AppSessionFrame.vue'

// Regression cover for the operator-reported defect: a container that is up
// but has not finished warming up (bitcoind serving RPC -28, lnd before the
// wallet unlocks) rendered the hard "App not reachable" failure copy for the
// whole warm-up window. The retry machinery already tolerated it — only the
// headline lied.

function mountFrame(props: Record<string, unknown> = {}) {
  return mount(AppSessionFrame, {
    props: {
      appUrl: 'http://localhost:8332/',
      appId: 'bitcoin-knots',
      appTitle: 'Bitcoin',
      appIcon: '/icons/bitcoin.png',
      loading: false,
      iframeBlocked: true,
      mustOpenNewTab: false,
      autoRetryCount: 1,
      refreshKey: 0,
      ...props,
    },
    global: { stubs: { AppLoadingScreen: true, Transition: false } },
  })
}

describe('AppSessionFrame warm-up state', () => {
  it('reads as starting, not unreachable, while the container is warming up', () => {
    const text = mountFrame({ warmingUp: true }).text()
    expect(text).toContain('Bitcoin is starting…')
    expect(text).not.toContain('App not reachable')
  })

  it('says the container is running so the copy does not imply it is stopped', () => {
    const text = mountFrame({ warmingUp: true }).text()
    expect(text).toContain("container is running but hasn't finished warming up")
    expect(text).not.toContain('the container is stopped')
  })

  it('still surfaces the automatic re-check while warming up', () => {
    expect(mountFrame({ warmingUp: true, autoRetryCount: 3 }).text()).toContain(
      'Checking again automatically (3)',
    )
  })

  it('reverts to the real failure once warm-up is over (retries exhausted)', () => {
    const text = mountFrame({ warmingUp: false, autoRetryCount: 6 }).text()
    expect(text).toContain('App not reachable')
    expect(text).not.toContain('is starting…')
  })

  it('leaves the explicit blocked-reason path untouched', () => {
    const text = mountFrame({
      warmingUp: false,
      blockedReason: 'Waiting for Bitcoin to finish syncing.',
      blockedTitle: 'Waiting for Bitcoin sync',
    }).text()
    expect(text).toContain('Waiting for Bitcoin sync')
    expect(text).not.toContain('App not reachable')
  })

  it('leaves the new-tab path untouched', () => {
    const text = mountFrame({ warmingUp: false, mustOpenNewTab: true }).text()
    expect(text).toContain('This app opens in a new tab')
  })
})

describe('HTTP readiness gate', () => {
  it('keeps the same iframe while reporting a slow load and lets the user dismiss the notice', async () => {
    const frame = mountFrame({ iframeBlocked: false, slowLoad: false });
    const original = frame.get('iframe').element;
    await frame.setProps({ slowLoad: true });
    expect(frame.get('iframe').element).toBe(original);
    expect(frame.get('[role="status"]').text()).toContain('Bitcoin is taking longer to load');
    await frame.get('[aria-label="Dismiss loading notice"]').trigger('click');
    expect(frame.emitted('dismissSlowLoad')).toHaveLength(1);
    await frame.setProps({ slowLoad: false });
    expect(frame.get('iframe').element).toBe(original);
    expect(frame.find('[role="status"]').exists()).toBe(false);
    frame.unmount();
  })
  it('does not show a missing-configuration error during initial installation', () => {
    const frame = mountFrame({ appUrl: '', uiReadyBlocked: true, blockedReason: 'Waiting for the app to be ready…' })
    expect(frame.text()).not.toContain('App not configured')
    expect(frame.find('iframe').exists()).toBe(false)
  })
  it('does not mount an iframe before readiness, then opens automatically', async () => {
    const frame = mountFrame({ iframeBlocked: false, uiReadyBlocked: true, blockedReason: 'Waiting for the app to be ready…', blockedTitle: 'App not ready' })
    expect(frame.find('iframe').exists()).toBe(false)
    expect(frame.text()).toContain('opens automatically')
    expect(frame.text()).not.toContain('Open in new tab')
    await frame.setProps({ uiReadyBlocked: false, blockedReason: '' })
    expect(frame.find('iframe').exists()).toBe(true)
  })
})
