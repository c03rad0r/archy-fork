import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useMeshStore } from '@/stores/mesh'
import { routerWorkInProgress } from '@/stores/criticalOperations'

vi.mock('@/api/rpc-client', () => ({
  rpcClient: { call: vi.fn(async () => status) },
}))

const status = {
  enabled: false,
  device_type: 'meshcore',
  device_path: '/dev/mesh-radio',
  device_connected: false,
  detected_devices: ['/dev/mesh-radio'],
  detected_device_info: [{ path: '/dev/mesh-radio', plugged_at: 1 }],
  firmware_version: null,
  self_node_id: null,
  self_advert_name: null,
  peer_count: 0,
  channel_name: '',
  messages_sent: 0,
  messages_received: 0,
}

describe('LoRa detection during router jobs', () => {
  beforeEach(() => {
    routerWorkInProgress.value = false
    setActivePinia(createPinia())
  })

  it('shows a detected radio normally and hides it during a flash/provision', async () => {
    const mesh = useMeshStore()
    await mesh.fetchStatus()
    await mesh.fetchStatus()

    expect(mesh.undismissedDetectedDevices).toEqual(['/dev/mesh-radio'])

    routerWorkInProgress.value = true
    expect(mesh.undismissedDetectedDevices).toEqual([])

    routerWorkInProgress.value = false
    expect(mesh.undismissedDetectedDevices).toEqual(['/dev/mesh-radio'])
  })
})
