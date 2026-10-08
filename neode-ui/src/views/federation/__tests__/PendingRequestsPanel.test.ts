import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import PendingRequestsPanel from '../PendingRequestsPanel.vue'
import type { PendingPeerRequest } from '@/api/rpc-client'
const request: PendingPeerRequest = {
  id: 'request', from_did: 'did:key:peer', from_nostr_pubkey: 'key', from_nostr_npub: 'npub',
  from_name: 'Other node', message: null, outbound: false, state: 'approved',
  received_at: new Date(Date.now()-2*86400000).toISOString(),
}
describe('Nostr connection progress', () => {
  it('keeps unresolved approvals visible and distinguishes local membership', async () => {
    const wrapper=mount(PendingRequestsPanel,{props:{requests:[request],polling:false,busyId:null,peerDids:[]}})
    expect(wrapper.text()).toContain('Approved — connecting')
    expect(wrapper.text()).not.toContain('Peer added')
    await wrapper.setProps({requests:[{...request,received_at:new Date().toISOString()}],peerDids:[request.from_did]})
    expect(wrapper.text()).toContain('Peer added')
    expect(wrapper.text()).not.toContain('Approved — connecting')
    wrapper.unmount()
  })
  it('explains expired approval instead of claiming a connection is still progressing', () => {
    const wrapper=mount(PendingRequestsPanel,{props:{requests:[{...request,received_at:new Date(Date.now()-31*86400000).toISOString()}],polling:false,busyId:null}})
    expect(wrapper.text()).toContain('Approval expired — reconnect')
    wrapper.unmount()
  })
})

it('keeps the Requests tab useful when there are no requests', () => {
  const wrapper = mount(PendingRequestsPanel, { props: { requests: [], polling: false, busyId: null, showEmpty: true } })
  expect(wrapper.get('[role="status"]').text()).toContain('No current requests')
  expect(wrapper.findAll('button').some(b => b.text() === 'Poll Now')).toBe(true)
  wrapper.unmount()
})
