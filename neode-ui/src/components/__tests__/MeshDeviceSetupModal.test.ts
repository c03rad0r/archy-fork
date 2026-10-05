import { mount, flushPromises } from '@vue/test-utils'
import { reactive } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import MeshDeviceSetupModal from '../mesh/MeshDeviceSetupModal.vue'

const state = reactive({
  flashFlowPath: null as string | null,
  undismissedDetectedDevices: [] as string[],
  status: {
    device_type: 'meshcore',
    detected_device_info: [{ path: '/dev/fixture', vid: '10c4', pid: 'ea60' }],
  },
  flashDevice: vi.fn(),
  flashStatus: vi.fn(),
  closeFlashFlow: vi.fn(),
})
vi.mock('@/stores/mesh', () => ({ useMeshStore: () => state }))
vi.mock('@/stores/app', () => ({ useAppStore: () => ({ serverName: 'Fixture' }) }))
vi.mock('vue-router', () => ({ useRouter: () => ({ push: vi.fn() }) }))

async function open() {
  const wrapper = mount(MeshDeviceSetupModal, {
    global: { stubs: { BaseModal: { template: '<div><slot /></div>' } } },
  })
  state.flashFlowPath = '/dev/fixture'
  await flushPromises()
  return wrapper
}

describe('LoRa firmware board selection', () => {
  beforeEach(() => {
    state.flashFlowPath = null
    state.flashDevice.mockReset()
  })

  it('does not infer Heltec V3 from a generic CP2102 adapter', async () => {
    const wrapper = await open()
    const board = wrapper.findAll('select')[1]!
    expect((board.element as HTMLSelectElement).value).toBe('')
    await wrapper.find('input[type="checkbox"]').setValue(true)
    const flash = wrapper.findAll('button').find(button => button.text().includes('Erase & Flash Now'))!
    expect((flash.element as HTMLButtonElement).disabled).toBe(true)
    expect(state.flashDevice).not.toHaveBeenCalled()
    await board.setValue('heltec-v3')
    await flash.trigger('click')
    expect(state.flashDevice).toHaveBeenCalledExactlyOnceWith('/dev/fixture', 'meshcore', 'heltec-v3')
    wrapper.unmount()
  })

  it('shows a failed tool preflight without entering flashing progress', async () => {
    state.flashDevice.mockRejectedValueOnce(new Error('Radio flashing tools are missing'))
    const wrapper = await open()
    await wrapper.findAll('select')[1]!.setValue('heltec-v4')
    await wrapper.find('input[type="checkbox"]').setValue(true)
    await wrapper.findAll('button').find(button => button.text().includes('Erase & Flash Now'))!.trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('Radio flashing tools are missing')
    expect(wrapper.text()).toContain('Erase & Flash Now')
    expect(state.flashStatus).not.toHaveBeenCalled()
    wrapper.unmount()
  })
})
