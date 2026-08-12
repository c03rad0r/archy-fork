import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import NodeList from '../NodeList.vue'
import type { FederatedNode } from '../types'

const trustedNode: FederatedNode = {
  did: 'did:key:z6MkTrustedNode',
  pubkey: 'trusted-pubkey',
  onion: 'trusted.onion',
  trust_level: 'trusted',
  added_at: '2026-06-10T00:00:00Z',
  name: 'Trusted Node',
  last_seen: '2026-06-10T00:00:00Z',
}

describe('NodeList', () => {
  it('keeps existing nodes visible during background refresh loading', () => {
    const wrapper = mount(NodeList, {
      props: {
        nodes: [trustedNode],
        loading: true,
        error: '',
        syncResults: [],
        dwnSyncDotClass: 'bg-green-400',
        cleaningNodes: false,
      },
    })

    expect(wrapper.text()).toContain('Trusted Node')
    expect(wrapper.text()).not.toContain('Loading nodes...')
  })

  // FED-02: a sync failure used to exist only as a debug! log line on the
  // node, so a peer that hadn't synced in days looked identical to one that
  // synced a minute ago. It must now be visible on the node's own row.
  it('shows a sync-error badge on a node whose last sync failed', () => {
    const failing: FederatedNode = {
      ...trustedNode,
      last_sync_error: 'Failed to reach federated peer',
      last_sync_error_at: '2026-06-11T00:00:00Z',
    }
    const wrapper = mount(NodeList, {
      props: {
        nodes: [failing],
        loading: false,
        error: '',
        syncResults: [],
        dwnSyncDotClass: 'bg-green-400',
        cleaningNodes: false,
      },
    })

    const badge = wrapper.find('[data-testid="sync-error-badge"]')
    expect(badge.exists()).toBe(true)
    expect(badge.text()).toBe('SYNC')
    // The full message + when it happened live in the tooltip, so the row
    // stays single-line (the node-name truncate idiom).
    expect(badge.attributes('title')).toContain('Failed to reach federated peer')
  })

  // The guard against a badge that always renders: a healthy peer (and a
  // peer that has recovered, since record_sync_result clears the field) must
  // show no sync-error badge at all.
  it('renders no sync-error badge when last_sync_error is unset', () => {
    const wrapper = mount(NodeList, {
      props: {
        nodes: [trustedNode],
        loading: false,
        error: '',
        syncResults: [],
        dwnSyncDotClass: 'bg-green-400',
        cleaningNodes: false,
      },
    })

    expect(wrapper.find('[data-testid="sync-error-badge"]').exists()).toBe(false)
  })

  // Peers (Observer level) render in a separate column — the badge must be
  // on that row too, or a failing peer stays invisible.
  it('shows the sync-error badge on an observer peer row', () => {
    const peer: FederatedNode = {
      ...trustedNode,
      did: 'did:key:z6MkPeerNode',
      trust_level: 'observer',
      name: 'Peer Node',
      last_sync_error: 'Peer returned 502 (via tor)',
      last_sync_error_at: '2026-06-11T00:00:00Z',
    }
    const wrapper = mount(NodeList, {
      props: {
        nodes: [peer],
        loading: false,
        error: '',
        syncResults: [],
        dwnSyncDotClass: 'bg-green-400',
        cleaningNodes: false,
      },
    })

    expect(wrapper.find('[data-testid="sync-error-badge"]').exists()).toBe(true)
  })
})
