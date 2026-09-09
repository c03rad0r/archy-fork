import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import NodeCertificateSection from '../NodeCertificateSection.vue'
import { installCertificateInCompanion } from '@/utils/openExternal'

vi.mock('@/utils/openExternal', () => ({
  installCertificateInCompanion: vi.fn(),
}))

const installCertificate = vi.mocked(installCertificateInCompanion)

describe('NodeCertificateSection', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      text: async () => '-----BEGIN CERTIFICATE-----\nAQ==\n-----END CERTIFICATE-----',
    }))
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    vi.clearAllMocks()
  })

  it('uses the native installer and cancels WebView navigation in the companion', async () => {
    installCertificate.mockReturnValue(true)
    const wrapper = mount(NodeCertificateSection)
    await flushPromises()
    await vi.waitFor(() => expect(wrapper.find('a[download]').exists()).toBe(true))

    wrapper.get('a[download]').element.setAttribute('href', '#certificate-test')
    const click = new MouseEvent('click', { bubbles: true, cancelable: true })
    wrapper.get('a[download]').element.dispatchEvent(click)

    expect(installCertificate).toHaveBeenCalledOnce()
    expect(click.defaultPrevented).toBe(true)
  })

  it('preserves the ordinary browser download when no native installer exists', async () => {
    installCertificate.mockReturnValue(false)
    const wrapper = mount(NodeCertificateSection)
    await flushPromises()
    await vi.waitFor(() => expect(wrapper.find('a[download]').exists()).toBe(true))

    wrapper.get('a[download]').element.setAttribute('href', '#certificate-test')
    const click = new MouseEvent('click', { bubbles: true, cancelable: true })
    wrapper.get('a[download]').element.dispatchEvent(click)
    const componentPreservedDownload = !click.defaultPrevented

    expect(installCertificate).toHaveBeenCalledOnce()
    expect(componentPreservedDownload).toBe(true)
  })

  it('includes the complete trust, browser restart, DNS, and troubleshooting guidance', async () => {
    const wrapper = mount(NodeCertificateSection)
    await flushPromises()
    await vi.waitFor(() => expect(wrapper.find('details').exists()).toBe(true))
    const text = wrapper.text()

    expect(text).toContain('Certificate Trust Settings')
    expect(text).toContain('Trusted Root Certification Authorities')
    expect(text).toContain('update-ca-trust extract')
    expect(text).toContain('Restart the browser first')
    expect(text).toContain('thisisunsafe')
    expect(text).toContain('Tailscale MagicDNS')
    expect(text).toContain("This site can't be reached / DNS error")
    expect(text).toContain('curl works, browser does not')
  })
})
