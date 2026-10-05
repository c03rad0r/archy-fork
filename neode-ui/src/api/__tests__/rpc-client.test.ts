import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

// We need to test the RPCClient class, so import it by re-creating the module
// Import the actual class and instance
const mockFetch = vi.fn()
vi.stubGlobal('fetch', mockFetch)

// Import after stubbing fetch
const { rpcClient } = await import('../rpc-client')

function jsonResponse(body: unknown, status = 200, statusText = 'OK'): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText,
    json: () => Promise.resolve(body),
    headers: new Headers(),
    redirected: false,
    type: 'basic' as ResponseType,
    url: '',
    clone: () => jsonResponse(body, status, statusText),
    body: null,
    bodyUsed: false,
    arrayBuffer: () => Promise.resolve(new ArrayBuffer(0)),
    blob: () => Promise.resolve(new Blob()),
    formData: () => Promise.resolve(new FormData()),
    text: () => Promise.resolve(JSON.stringify(body)),
    bytes: () => Promise.resolve(new Uint8Array()),
  }
}

describe('RPCClient', () => {
  beforeEach(() => {
    mockFetch.mockReset()
    vi.useFakeTimers({ shouldAdvanceTime: true })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('makes a successful RPC call and returns the result', async () => {
    mockFetch.mockResolvedValueOnce(jsonResponse({ result: { did: 'did:key:z123' } }))

    const result = await rpcClient.call<{ did: string }>({
      method: 'node.did',
      params: {},
    })

    expect(result).toEqual({ did: 'did:key:z123' })
    expect(mockFetch).toHaveBeenCalledOnce()
    const [url, init] = mockFetch.mock.calls[0]!
    expect(url).toBe('/rpc/v1')
    expect(init.method).toBe('POST')
    expect(init.credentials).toBe('include')
    expect(JSON.parse(init.body)).toEqual({ method: 'node.did', params: {} })
  })

  it('includes credentials for session cookies', async () => {
    mockFetch.mockResolvedValueOnce(jsonResponse({ result: 'ok' }))

    await rpcClient.call({ method: 'test', params: {} })

    const [, init] = mockFetch.mock.calls[0]!
    expect(init.credentials).toBe('include')
  })

  it('retries on 502 Bad Gateway and eventually succeeds', async () => {
    mockFetch
      .mockResolvedValueOnce(jsonResponse(null, 502, 'Bad Gateway'))
      .mockResolvedValueOnce(jsonResponse({ result: 'ok' }))

    const result = await rpcClient.call({ method: 'test' })

    expect(result).toBe('ok')
    expect(mockFetch).toHaveBeenCalledTimes(2)
  })

  it('retries on 503 Service Unavailable and eventually succeeds', async () => {
    mockFetch
      .mockResolvedValueOnce(jsonResponse(null, 503, 'Service Unavailable'))
      .mockResolvedValueOnce(jsonResponse({ result: 'recovered' }))

    const result = await rpcClient.call({ method: 'test' })

    expect(result).toBe('recovered')
    expect(mockFetch).toHaveBeenCalledTimes(2)
  })

  it('throws after max retries on persistent 502', async () => {
    mockFetch
      .mockResolvedValue(jsonResponse(null, 502, 'Bad Gateway'))

    await expect(rpcClient.call({ method: 'test' })).rejects.toThrow('HTTP 502: Bad Gateway')
    expect(mockFetch).toHaveBeenCalledTimes(3)
  })

  it('throws immediately on non-retryable HTTP errors (e.g. 401)', async () => {
    mockFetch.mockResolvedValueOnce(jsonResponse(null, 401, 'Unauthorized'))

    await expect(rpcClient.call({ method: 'test' })).rejects.toThrow('Session expired')
    expect(mockFetch).toHaveBeenCalledOnce()
  })

  it('throws on RPC-level error in response body', async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ error: { code: -32600, message: 'Invalid method' } }),
    )

    await expect(rpcClient.call({ method: 'bad.method' })).rejects.toThrow('Invalid method')
    expect(mockFetch).toHaveBeenCalledOnce()
  })

  it('throws timeout error when request times out', async () => {
    const abortError = Object.assign(new Error('The operation was aborted.'), { name: 'AbortError' })
    mockFetch.mockRejectedValue(abortError)

    await expect(
      rpcClient.call({ method: 'slow', timeout: 100 }),
    ).rejects.toThrow('Request timeout')
    expect(mockFetch).toHaveBeenCalledTimes(3)
  })

  it('retries on network/fetch errors and eventually succeeds', async () => {
    mockFetch
      .mockRejectedValueOnce(new Error('fetch failed'))
      .mockResolvedValueOnce(jsonResponse({ result: 'back online' }))

    const result = await rpcClient.call({ method: 'test' })

    expect(result).toBe('back online')
    expect(mockFetch).toHaveBeenCalledTimes(2)
  })

  it('throws on non-retryable errors immediately', async () => {
    mockFetch.mockRejectedValueOnce(new Error('some random error'))

    await expect(rpcClient.call({ method: 'test' })).rejects.toThrow('some random error')
    expect(mockFetch).toHaveBeenCalledOnce()
  })

  it('handles unknown (non-Error) thrown values', async () => {
    mockFetch.mockRejectedValueOnce('string error')

    await expect(rpcClient.call({ method: 'test' })).rejects.toThrow('Unknown error occurred')
  })

  it('uses default params when none provided', async () => {
    mockFetch.mockResolvedValueOnce(jsonResponse({ result: 'ok' }))

    await rpcClient.call({ method: 'test' })

    const body = JSON.parse(mockFetch.mock.calls[0]![1].body)
    expect(body.params).toEqual({})
  })

  it('sends an abort signal for timeout', async () => {
    mockFetch.mockResolvedValueOnce(jsonResponse({ result: 'ok' }))

    await rpcClient.call({ method: 'test', timeout: 5000 })

    const [, init] = mockFetch.mock.calls[0]!
    expect(init.signal).toBeInstanceOf(AbortSignal)
  })
})

describe('RPCClient convenience methods', () => {
  beforeEach(() => {
    mockFetch.mockReset()
    vi.useFakeTimers({ shouldAdvanceTime: true })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  function mockSuccess(result: unknown) {
    mockFetch.mockResolvedValueOnce(jsonResponse({ result }))
  }

  function getLastMethod(): string {
    const body = JSON.parse(mockFetch.mock.calls[0]![1].body)
    return body.method
  }

  function getLastParams(): Record<string, unknown> {
    const body = JSON.parse(mockFetch.mock.calls[0]![1].body)
    return body.params
  }

  it('login calls auth.login with password', async () => {
    mockSuccess(null)
    await rpcClient.login('test123')
    expect(getLastMethod()).toBe('auth.login')
    expect(getLastParams().password).toBe('test123')
  })

  it('loginTotp calls auth.login.totp', async () => {
    mockSuccess({ success: true })
    await rpcClient.loginTotp('123456')
    expect(getLastMethod()).toBe('auth.login.totp')
    expect(getLastParams().code).toBe('123456')
  })

  it('loginBackup calls auth.login.backup', async () => {
    mockSuccess({ success: true })
    await rpcClient.loginBackup('ABCD-1234')
    expect(getLastMethod()).toBe('auth.login.backup')
    expect(getLastParams().code).toBe('ABCD-1234')
  })

  it('totpSetupBegin calls auth.totp.setup.begin', async () => {
    mockSuccess({ qr_svg: '<svg/>', secret_base32: 'ABC', pending_token: 'tok' })
    await rpcClient.totpSetupBegin('password')
    expect(getLastMethod()).toBe('auth.totp.setup.begin')
  })

  it('totpSetupConfirm calls auth.totp.setup.confirm', async () => {
    mockSuccess({ enabled: true, backup_codes: ['A', 'B'] })
    await rpcClient.totpSetupConfirm({ code: '123456', password: 'pw', pendingToken: 'tok' })
    expect(getLastMethod()).toBe('auth.totp.setup.confirm')
  })

  it('totpDisable calls auth.totp.disable', async () => {
    mockSuccess({ disabled: true })
    await rpcClient.totpDisable('pw', '123456')
    expect(getLastMethod()).toBe('auth.totp.disable')
  })

  it('totpStatus calls auth.totp.status', async () => {
    mockSuccess({ enabled: false })
    await rpcClient.totpStatus()
    expect(getLastMethod()).toBe('auth.totp.status')
  })

  it('changePassword calls auth.changePassword', async () => {
    mockSuccess({ success: true })
    await rpcClient.changePassword({ currentPassword: 'old', newPassword: 'new' })
    expect(getLastMethod()).toBe('auth.changePassword')
    expect(getLastParams().alsoChangeSsh).toBe(true)
  })

  it('changePassword respects alsoChangeSsh option', async () => {
    mockSuccess({ success: true })
    await rpcClient.changePassword({ currentPassword: 'old', newPassword: 'new', alsoChangeSsh: false })
    expect(getLastParams().alsoChangeSsh).toBe(false)
  })

  it('logout calls auth.logout', async () => {
    mockSuccess(undefined)
    await rpcClient.logout()
    expect(getLastMethod()).toBe('auth.logout')
  })

  it('completeOnboarding calls auth.onboardingComplete', async () => {
    mockSuccess(true)
    await rpcClient.completeOnboarding()
    expect(getLastMethod()).toBe('auth.onboardingComplete')
  })

  it('isOnboardingComplete calls auth.isOnboardingComplete', async () => {
    mockSuccess(true)
    const result = await rpcClient.isOnboardingComplete()
    expect(result).toBe(true)
    expect(getLastMethod()).toBe('auth.isOnboardingComplete')
  })

  it('resetOnboarding calls auth.resetOnboarding', async () => {
    mockSuccess(true)
    await rpcClient.resetOnboarding()
    expect(getLastMethod()).toBe('auth.resetOnboarding')
  })

  it('getNodeDid calls node.did', async () => {
    mockSuccess({ did: 'did:key:z123', pubkey: 'abc' })
    const result = await rpcClient.getNodeDid()
    expect(result.did).toBe('did:key:z123')
    expect(getLastMethod()).toBe('node.did')
  })

  it('signChallenge calls node.signChallenge', async () => {
    mockSuccess({ signature: 'sig123' })
    await rpcClient.signChallenge('test-challenge')
    expect(getLastMethod()).toBe('node.signChallenge')
    expect(getLastParams().challenge).toBe('test-challenge')
  })

  it('createBackup calls node.createBackup', async () => {
    mockSuccess({ version: 1, did: 'did:key:z', pubkey: 'pk', kid: 'k1', encrypted: true, blob: 'data', timestamp: '2026-01-01' })
    await rpcClient.createBackup('passphrase')
    expect(getLastMethod()).toBe('node.createBackup')
  })

  it('resolveDid calls identity.resolve-did', async () => {
    mockSuccess({})
    await rpcClient.resolveDid('did:key:z123')
    expect(getLastMethod()).toBe('identity.resolve-did')
    expect(getLastParams().did).toBe('did:key:z123')
  })

  it('resolveDid without did sends empty params', async () => {
    mockSuccess({})
    await rpcClient.resolveDid()
    expect(getLastParams()).toEqual({})
  })

  it('createPresentation calls identity.create-presentation', async () => {
    mockSuccess({})
    await rpcClient.createPresentation({ holderId: 'h1', credentialIds: ['c1'] })
    expect(getLastMethod()).toBe('identity.create-presentation')
  })

  it('verifyPresentation calls identity.verify-presentation', async () => {
    mockSuccess({ valid: true, holder_valid: true, credentials: [] })
    await rpcClient.verifyPresentation({ type: 'test' })
    expect(getLastMethod()).toBe('identity.verify-presentation')
  })

  it('createPsbt calls lnd.create-psbt', async () => {
    mockSuccess({ psbt_base64: 'psbt', change_output_index: 0, total_amount_sats: 1000, fee_rate_sat_per_vbyte: 10 })
    await rpcClient.createPsbt({ outputs: [{ address: 'bc1q...', amount_sats: 1000 }] })
    expect(getLastMethod()).toBe('lnd.create-psbt')
    expect(getLastParams()).not.toHaveProperty('fee_rate_sat_per_vbyte')
  })

  it('preserves the explicit hardware-wallet fee rate', async () => {
    mockSuccess({ psbt_base64: 'psbt', fee_rate_sat_per_vbyte: 17 })
    await rpcClient.createPsbt({ outputs: [{ address: 'bc1q...', amount_sats: 1000 }], feeRateSatPerVbyte: 17 })
    expect(getLastParams().fee_rate_sat_per_vbyte).toBe(17)
  })

  it.each([NaN, Infinity, 0, -1, 0.5, 5001])('rejects invalid PSBT rate %s rather than silently selecting a default', async rate => {
    await expect(rpcClient.createPsbt({ outputs: [{ address: 'bc1q...', amount_sats: 1000 }], feeRateSatPerVbyte: rate })).rejects.toThrow('whole number')
  })

  it('finalizePsbt calls lnd.finalize-psbt', async () => {
    mockSuccess({ raw_final_tx: 'rawtx', broadcast: true })
    await rpcClient.finalizePsbt('signed-psbt')
    expect(getLastMethod()).toBe('lnd.finalize-psbt')
  })

  it('publishNostrIdentity calls node.nostr-publish', async () => {
    mockSuccess({ event_id: 'evt', success: 1, failed: 0 })
    await rpcClient.publishNostrIdentity()
    expect(getLastMethod()).toBe('node.nostr-publish')
  })

  it('getNostrPubkey calls node.nostr-pubkey', async () => {
    mockSuccess({ nostr_pubkey: 'npub1...' })
    await rpcClient.getNostrPubkey()
    expect(getLastMethod()).toBe('node.nostr-pubkey')
  })

  it('listPeers calls node-list-peers', async () => {
    mockSuccess({ peers: [] })
    await rpcClient.listPeers()
    expect(getLastMethod()).toBe('node-list-peers')
  })

  it('addPeer calls node-add-peer', async () => {
    mockSuccess({ peers: [] })
    await rpcClient.addPeer({ onion: 'abc.onion', pubkey: 'pk' })
    expect(getLastMethod()).toBe('node-add-peer')
  })

  it('removePeer calls node-remove-peer', async () => {
    mockSuccess({ peers: [] })
    await rpcClient.removePeer('pk123')
    expect(getLastMethod()).toBe('node-remove-peer')
  })

  it('sendMessageToPeer calls node-send-message', async () => {
    mockSuccess({ ok: true, sent_to: 'abc.onion' })
    await rpcClient.sendMessageToPeer('abc.onion', 'hello')
    expect(getLastMethod()).toBe('node-send-message')
  })

  it('checkPeerReachable calls node-check-peer', async () => {
    mockSuccess({ onion: 'abc.onion', reachable: true })
    await rpcClient.checkPeerReachable('abc.onion')
    expect(getLastMethod()).toBe('node-check-peer')
  })

  it('getReceivedMessages calls node-messages-received', async () => {
    mockSuccess({ messages: [] })
    await rpcClient.getReceivedMessages()
    expect(getLastMethod()).toBe('node-messages-received')
  })

  it('discoverNodes calls node-nostr-discover', async () => {
    mockSuccess({ nodes: [] })
    await rpcClient.discoverNodes()
    expect(getLastMethod()).toBe('node-nostr-discover')
  })

  it('getTorAddress calls node.tor-address', async () => {
    mockSuccess({ tor_address: 'abc123.onion' })
    await rpcClient.getTorAddress()
    expect(getLastMethod()).toBe('node.tor-address')
  })

  it('verifyNostrRevoked calls node-nostr-verify-revoked', async () => {
    mockSuccess({ revoked: false, nostr_pubkey: 'npub' })
    await rpcClient.verifyNostrRevoked()
    expect(getLastMethod()).toBe('node-nostr-verify-revoked')
  })

  it('echo calls server.echo', async () => {
    mockSuccess('hello')
    const result = await rpcClient.echo('hello')
    expect(result).toBe('hello')
    expect(getLastMethod()).toBe('server.echo')
  })

  it('getSystemTime calls server.time', async () => {
    mockSuccess({ now: '2026-03-11', uptime: 3600 })
    await rpcClient.getSystemTime()
    expect(getLastMethod()).toBe('server.time')
  })

  it('getMetrics calls server.metrics', async () => {
    mockSuccess({ cpu: 50 })
    await rpcClient.getMetrics()
    expect(getLastMethod()).toBe('server.metrics')
  })

  it('updateServer calls server.update', async () => {
    mockSuccess('no-updates')
    await rpcClient.updateServer('https://example.com')
    expect(getLastMethod()).toBe('server.update')
  })

  it('detectUsbDevices calls system.detect-usb-devices', async () => {
    mockSuccess({ devices: [] })
    await rpcClient.detectUsbDevices()
    expect(getLastMethod()).toBe('system.detect-usb-devices')
  })

  it('restartServer calls server.restart', async () => {
    mockSuccess(undefined)
    await rpcClient.restartServer()
    expect(getLastMethod()).toBe('server.restart')
  })

  it('shutdownServer calls server.shutdown', async () => {
    mockSuccess(undefined)
    await rpcClient.shutdownServer()
    expect(getLastMethod()).toBe('server.shutdown')
  })

  it('installPackage calls package.install', async () => {
    mockSuccess('bitcoin-knots')
    await rpcClient.installPackage('btc', 'https://mp.com', '1.0')
    expect(getLastMethod()).toBe('package.install')
  })

  it('uninstallPackage calls package.uninstall', async () => {
    mockSuccess(undefined)
    await rpcClient.uninstallPackage('btc')
    expect(getLastMethod()).toBe('package.uninstall')
  })

  it('uninstallPackage forwards preserve_data when requested', async () => {
    mockSuccess(undefined)
    await rpcClient.uninstallPackage('btc', { preserveData: true })
    expect(getLastParams()).toEqual({ id: 'btc', preserve_data: true })
  })

  it('startPackage calls package.start', async () => {
    mockSuccess(undefined)
    await rpcClient.startPackage('btc')
    expect(getLastMethod()).toBe('package.start')
  })

  it('stopPackage calls package.stop', async () => {
    mockSuccess(undefined)
    await rpcClient.stopPackage('btc')
    expect(getLastMethod()).toBe('package.stop')
  })

  it('restartPackage calls package.restart', async () => {
    mockSuccess(undefined)
    await rpcClient.restartPackage('btc')
    expect(getLastMethod()).toBe('package.restart')
  })

  it('getMarketplace calls marketplace.get', async () => {
    mockSuccess({})
    await rpcClient.getMarketplace('https://mp.com')
    expect(getLastMethod()).toBe('marketplace.get')
  })

  it('federationInvite calls federation.invite', async () => {
    mockSuccess({ code: 'ABC', did: 'did:key:z', onion: 'abc.onion' })
    await rpcClient.federationInvite()
    expect(getLastMethod()).toBe('federation.invite')
  })

  it('federationInvite omits password when none is given', async () => {
    mockSuccess({ code: 'ABC', did: 'did:key:z', onion: 'abc.onion' })
    await rpcClient.federationInvite('observer')
    expect(getLastParams()).not.toHaveProperty('password')
  })

  it('federationInvite forwards the password for a trusted invite', async () => {
    mockSuccess({ code: 'ABC', did: 'did:key:z', onion: 'abc.onion' })
    await rpcClient.federationInvite('trusted', 'hunter2')
    expect(getLastParams()).toMatchObject({ trust_level: 'trusted', password: 'hunter2' })
  })

  it('federationJoin calls federation.join', async () => {
    mockSuccess({ joined: true, node: {} })
    await rpcClient.federationJoin('invite-code')
    expect(getLastMethod()).toBe('federation.join')
  })

  it('federationListNodes calls federation.list-nodes', async () => {
    mockSuccess({ nodes: [] })
    await rpcClient.federationListNodes()
    expect(getLastMethod()).toBe('federation.list-nodes')
  })

  it('federationRemoveNode calls federation.remove-node', async () => {
    mockSuccess({ removed: true, nodes_remaining: 0 })
    await rpcClient.federationRemoveNode('did:key:z')
    expect(getLastMethod()).toBe('federation.remove-node')
  })

  it('federationSetTrust calls federation.set-trust', async () => {
    mockSuccess({ updated: true, did: 'did:key:z', trust_level: 'trusted' })
    await rpcClient.federationSetTrust('did:key:z', 'trusted')
    expect(getLastMethod()).toBe('federation.set-trust')
  })

  it('federationSetTrust omits password on demotion', async () => {
    mockSuccess({ updated: true, did: 'did:key:z', trust_level: 'observer' })
    await rpcClient.federationSetTrust('did:key:z', 'observer')
    expect(getLastParams()).not.toHaveProperty('password')
  })

  it('federationSetTrust forwards the password when promoting', async () => {
    mockSuccess({ updated: true, did: 'did:key:z', trust_level: 'trusted' })
    await rpcClient.federationSetTrust('did:key:z', 'trusted', 'hunter2')
    expect(getLastParams()).toMatchObject({
      did: 'did:key:z',
      trust_level: 'trusted',
      password: 'hunter2',
    })
  })

  it('federationSyncState calls federation.sync-state', async () => {
    mockSuccess({ synced: 1, failed: 0, results: [] })
    await rpcClient.federationSyncState()
    expect(getLastMethod()).toBe('federation.sync-state')
  })

  it('federationDeployApp calls federation.deploy-app', async () => {
    mockSuccess({ deployed: true, app_id: 'btc', peer_did: 'did', peer_onion: 'onion' })
    await rpcClient.federationDeployApp({ did: 'did:key:z', appId: 'btc' })
    expect(getLastMethod()).toBe('federation.deploy-app')
    expect(getLastParams().version).toBe('latest')
  })

  it('vpnStatus calls vpn.status', async () => {
    mockSuccess({ connected: false, peers_connected: 0, bytes_in: 0, bytes_out: 0, configured: false, configured_provider: '' })
    await rpcClient.vpnStatus()
    expect(getLastMethod()).toBe('vpn.status')
  })

  it('vpnConfigure calls vpn.configure', async () => {
    mockSuccess({ configured: true, provider: 'tailscale' })
    await rpcClient.vpnConfigure({ provider: 'tailscale', auth_key: 'key' })
    expect(getLastMethod()).toBe('vpn.configure')
  })

  it('vpnDisconnect calls vpn.disconnect', async () => {
    mockSuccess({ disconnected: true })
    await rpcClient.vpnDisconnect()
    expect(getLastMethod()).toBe('vpn.disconnect')
  })

  it('marketplaceDiscover calls marketplace.discover', async () => {
    mockSuccess({ apps: [], relay_count: 0 })
    await rpcClient.marketplaceDiscover()
    expect(getLastMethod()).toBe('marketplace.discover')
  })

  it('dnsStatus calls network.dns-status', async () => {
    mockSuccess({ provider: 'system', servers: [], doh_enabled: false, doh_url: null, resolv_conf_servers: [] })
    await rpcClient.dnsStatus()
    expect(getLastMethod()).toBe('network.dns-status')
  })

  it('configureDns calls network.configure-dns', async () => {
    mockSuccess({ ok: true, provider: 'cloudflare', servers: [], doh_enabled: true, doh_url: null })
    await rpcClient.configureDns({ provider: 'cloudflare' })
    expect(getLastMethod()).toBe('network.configure-dns')
  })

  it('diskStatus calls system.disk-status', async () => {
    mockSuccess({ used_bytes: 100, total_bytes: 1000, free_bytes: 900, used_percent: 10, level: 'ok' })
    await rpcClient.diskStatus()
    expect(getLastMethod()).toBe('system.disk-status')
  })

  it('diskCleanup calls system.disk-cleanup', async () => {
    mockSuccess({ freed_bytes: 500, freed_human: '500B', actions: [] })
    await rpcClient.diskCleanup()
    expect(getLastMethod()).toBe('system.disk-cleanup')
  })
})
