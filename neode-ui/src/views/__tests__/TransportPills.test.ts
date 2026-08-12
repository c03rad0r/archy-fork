/**
 * ─────────────────────────────────────────────────────────────────────────────
 *  THE FIPS / TOR TRANSPORT PILLS ARE A PERMANENT, USER-REQUESTED FEATURE.
 * ─────────────────────────────────────────────────────────────────────────────
 *
 * Dorian asked for these pills explicitly ("really helpful") and asked that no
 * future cleanup or refactor ever remove them. They are the one place the app
 * tells a user whether their file arrived over the fast encrypted mesh (FIPS)
 * or over Tor — a security signal, not decoration. Requirement UIFIX-01.
 *
 * IF A TEST IN THIS FILE FAILS, THE MOST LIKELY CAUSE IS THAT SOMEONE REMOVED
 * OR RENAMED A TRANSPORT PILL — not that the test went stale. Put the pill
 * back. If a pill genuinely has to move, move the assertion with it; do not
 * delete the assertion.
 *
 * Each `it()` below is keyed to ONE specific render site so that deleting the
 * pill from that site, and only that site, fails. A pill somewhere else in the
 * app does not satisfy these assertions.
 *
 * Render sites pinned here (audited 2026-08-02 at 390×740 and 320×640):
 *   S1  Cloud.vue      — peer card badge row, Folders tab   (renders a pill)
 *   S2  Cloud.vue      — Peer Files aggregated rows          (no pill, by decision)
 *   S3  Cloud.vue      — Paid Files rows                     (no pill, by decision)
 *   S4  PeerFiles.vue  — header, desktop copy + mobile copy  (renders a pill)
 *   S5  PeerFiles.vue  — per-file card body                  (no pill, by decision)
 *
 * The S2/S3/S5 "no pill" assertions pin a recorded product decision, not a
 * bug: transport is measured PER PEER PER BROWSE, never per file, so a
 * per-file pill would claim a reading the app never took. The reasoning is
 * If you deliberately add a per-file pill, update that decision record and
 * this test together.
 */
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { describe, expect, it, vi, beforeEach } from 'vitest'
import Cloud from '../Cloud.vue'
import PeerFiles from '../PeerFiles.vue'
import { rpcClient } from '@/api/rpc-client'

vi.mock('vue-router', () => ({
  useRouter: () => ({ push: vi.fn() }),
  RouterLink: { name: 'RouterLink', props: ['to'], template: '<a><slot /></a>' },
}))

vi.mock('@/stores/app', () => ({
  useAppStore: () => ({ packages: {} }),
}))

vi.mock('@/composables/useAudioPlayer', () => ({
  useAudioPlayer: () => ({ play: vi.fn() }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
    federationListNodes: vi.fn(),
  },
}))

const PEER_ONION = 'peeraaaa1111bbbb2222cccc3333dddd4444eeee.onion'

function makePeer(overrides: Record<string, unknown> = {}) {
  return {
    did: 'did:key:peerAlpha',
    pubkey: 'peer',
    onion: PEER_ONION,
    name: 'Peer Alpha',
    trust_level: 'trusted',
    added_at: '2026-06-10T10:00:00Z',
    ...overrides,
  }
}

function makeItem(overrides: Record<string, unknown> = {}) {
  return {
    id: 'file-1',
    filename: 'notes.txt',
    mime_type: 'text/plain',
    size_bytes: 128,
    description: '',
    access: 'free',
    ...overrides,
  }
}

/** Mount Cloud.vue with one peer whose last browse resolved over `transport`.
 *  Pass `transport: null` for the "we have not observed a transport" edge. */
async function mountCloud(transport: string | null, peerOverrides: Record<string, unknown> = {}) {
  vi.mocked(rpcClient.federationListNodes).mockResolvedValue({ nodes: [makePeer(peerOverrides)] } as never)
  vi.mocked(rpcClient.call).mockImplementation((async (req: { method: string }) => {
    if (req.method === 'content.browse-peer') {
      return transport === null ? { items: [makeItem()] } : { items: [makeItem()], transport }
    }
    if (req.method === 'content.owned-list') {
      return {
        items: [{
          onion: PEER_ONION,
          content_id: 'file-1',
          filename: 'paid-track.mp3',
          mime_type: 'audio/mpeg',
          size_bytes: 4096,
          paid_sats: 2500,
          purchased_at: '2026-07-30T10:00:00Z',
        }],
      }
    }
    return {}
  }) as never)

  const wrapper = mount(Cloud, {
    global: { plugins: [createPinia()], stubs: { Teleport: true } },
  })
  await flushPromises()
  await flushPromises()
  return wrapper
}

async function mountPeerFiles(transport: string | null) {
  vi.mocked(rpcClient.federationListNodes).mockResolvedValue({ nodes: [makePeer()] } as never)
  vi.mocked(rpcClient.call).mockImplementation((async (req: { method: string }) => {
    if (req.method === 'content.browse-peer') {
      return transport === null ? { items: [makeItem()] } : { items: [makeItem()], transport }
    }
    if (req.method === 'content.owned-list') return { items: [] }
    return {}
  }) as never)

  const wrapper = mount(PeerFiles, {
    props: { peerId: PEER_ONION },
    global: { plugins: [createPinia()], stubs: { Teleport: true } },
  })
  await flushPromises()
  await flushPromises()
  return wrapper
}

/** The Cloud.vue peer-card badge pill renders "<TRANSPORT> · <n>s" — a shape
 *  no other pill in either view produces, which is what keys these assertions
 *  to site S1 specifically. */
function cloudPeerCardPill(wrapper: ReturnType<typeof mount>) {
  return wrapper.findAll('span').find(s => /^(FIPS|TOR|MESH|LAN)\s·\s[\d.]+s$/.test(s.text().trim()))
}

beforeEach(() => {
  vi.clearAllMocks()
  // The peer-browse cache is a persist:true key — it snapshots into
  // sessionStorage, which outlives a per-test `createPinia()`. Without this,
  // the transport from an earlier test leaks into the next one and the
  // unknown-transport case would "pass" against a stale FIPS reading.
  sessionStorage.clear()
})

// ── S1: Cloud.vue peer card badge row ───────────────────────────────────────
describe('S1 — Cloud.vue peer card transport pill (Folders tab)', () => {
  it('renders the FIPS pill on the peer card when the last browse used FIPS', async () => {
    const wrapper = await mountCloud('fips')
    const pill = cloudPeerCardPill(wrapper)

    expect(pill, 'peer card transport pill is missing — see the header of this file').toBeTruthy()
    expect(pill!.text()).toMatch(/^FIPS · [\d.]+s$/)
    // FIPS reads as the good/fast path — emerald, matching the canonical palette.
    expect(pill!.classes().join(' ')).toContain('bg-emerald-500/15')
    expect(pill!.attributes('title')).toContain('FIPS')
  })

  it('renders the TOR pill, in the slow-path colour, when the last browse used Tor', async () => {
    const wrapper = await mountCloud('tor')
    const pill = cloudPeerCardPill(wrapper)

    expect(pill, 'peer card transport pill is missing — see the header of this file').toBeTruthy()
    expect(pill!.text()).toMatch(/^TOR · [\d.]+s$/)
    // Tor is the slow fallback — amber, visibly different from FIPS.
    expect(pill!.classes().join(' ')).toContain('bg-amber-500/15')
    expect(pill!.attributes('title')).toContain('TOR')
  })

  it('fabricates no pill when no transport has been observed, and keeps the not-known treatment', async () => {
    const wrapper = await mountCloud(null)

    // T-01-78: never claim a transport the app has not actually measured.
    expect(cloudPeerCardPill(wrapper)).toBeUndefined()
    expect(wrapper.text()).not.toMatch(/\bFIPS\b/)
    // The existing "we don't know yet" treatment stays.
    expect(wrapper.text()).toContain('Peer Node')
  })

  // UIFIX-01 mobile half. The badge row has no horizontal give at 320px: with a
  // longer trust label and no wrapping, flexbox compresses the transport badge
  // until its OWN text breaks mid-label ("TOR ·" / "120.0s") — measured in a
  // real browser at 320×640. flex-wrap makes the badge drop to a second line
  // intact instead, and shrink-0 stops it being squeezed on the way there.
  it('lets the badge row wrap and keeps the transport pill unsqueezed (mobile legibility)', async () => {
    const wrapper = await mountCloud('tor')
    const pill = cloudPeerCardPill(wrapper)
    expect(pill).toBeTruthy()

    expect(pill!.classes(), 'transport pill must not be compressible').toContain('shrink-0')

    const row = pill!.element.parentElement as HTMLElement
    expect(row, 'transport pill has no parent row').toBeTruthy()
    expect(
      Array.from(row.classList),
      'the peer-card badge row must wrap, or the pill text breaks mid-label at 320px',
    ).toContain('flex-wrap')
  })
})

// ── S2/S3: Cloud.vue file lists carry no per-file transport pill (decision) ──
describe('S2/S3 — Cloud.vue file rows carry no per-file transport pill (recorded decision)', () => {
  it('shows no transport pill on the aggregated Peer Files rows or the Paid Files rows', async () => {
    const wrapper = await mountCloud('fips')
    const vm = wrapper.vm as unknown as { activeTab: string }

    for (const tab of ['peers', 'paid']) {
      vm.activeTab = tab
      await flushPromises()

      // Transport is a per-peer, per-browse reading. These rows list files —
      // from many peers at once in the Peer Files case, and from the local
      // purchase cache (no live transport at all) in the Paid Files case.
      // A pill here would assert a per-file measurement that was never taken.
      expect(
        cloudPeerCardPill(wrapper),
        `a transport pill appeared on the "${tab}" rows — see the header of this file`,
      ).toBeUndefined()
      expect(wrapper.text(), `"${tab}" rows must not label files with a transport`).not.toMatch(/\bFIPS\b/)
    }
  })
})

// ── S4: PeerFiles.vue header, desktop copy AND mobile copy ──────────────────
describe('S4 — PeerFiles.vue header transport pill', () => {
  it('renders the pill in the desktop title block', async () => {
    const wrapper = await mountPeerFiles('fips')

    const desktopBlock = wrapper.findAll('div').find(d => {
      const c = d.classes()
      return c.includes('hidden') && c.includes('md:block')
    })
    expect(desktopBlock, 'PeerFiles desktop title block is missing').toBeTruthy()

    const pill = desktopBlock!.findAll('span').find(s => s.text().trim() === 'FIPS')
    expect(pill, 'desktop header transport pill is missing — see the header of this file').toBeTruthy()
    // Canonical mapping (PeerFiles.vue transportPill), not a duplicated table.
    expect(pill!.classes().join(' ')).toContain('bg-green-500/20')
    expect(pill!.attributes('title')).toContain('FIPS')
  })

  it('renders a separate mobile copy of the pill, because the desktop title block is hidden on a phone', async () => {
    const wrapper = await mountPeerFiles('fips')

    // This is the UIFIX-01 mobile half for this site: the title block that
    // carries the desktop pill is `hidden md:block`, so without this copy a
    // phone user would see no transport at all on the peer's file page.
    const mobilePill = wrapper.findAll('span').find(s =>
      s.classes().includes('md:hidden') && s.text().trim() === 'FIPS',
    )
    expect(
      mobilePill,
      'the md:hidden mobile transport pill is missing — a phone would show no transport here',
    ).toBeTruthy()
    expect(mobilePill!.classes().join(' ')).toContain('bg-green-500/20')
  })

  it.each([
    ['fips', 'FIPS', 'bg-green-500/20'],
    ['mesh', 'Mesh', 'bg-green-500/20'],
    ['lan', 'LAN', 'bg-blue-500/20'],
    ['tor', 'Tor', 'bg-amber-500/20'],
  ])('maps transport %s to the canonical label %s and its canonical colour', async (transport, label, colour) => {
    const wrapper = await mountPeerFiles(transport)

    const pills = wrapper.findAll('span').filter(s => s.text().trim() === label)
    // One desktop copy + one mobile copy, both from the same canonical mapping.
    expect(pills.length, `expected the ${label} pill in both the desktop and mobile header copies`).toBe(2)
    for (const p of pills) expect(p.classes().join(' ')).toContain(colour)
  })

  it('fabricates no pill for an unobserved transport, in either the desktop or the mobile copy', async () => {
    const wrapper = await mountPeerFiles(null)

    for (const label of ['FIPS', 'Mesh', 'LAN', 'Tor']) {
      expect(
        wrapper.findAll('span').some(s => s.text().trim() === label),
        `PeerFiles fabricated a "${label}" pill with no observed transport`,
      ).toBe(false)
    }
    // The file list itself is unaffected — only the transport claim is absent.
    expect(wrapper.text()).toContain('notes.txt')
  })
})

// ── S5: PeerFiles.vue per-file cards carry no transport pill (decision) ─────
describe('S5 — PeerFiles.vue per-file cards carry no transport pill (recorded decision)', () => {
  it('labels file cards with access only, leaving transport to the single peer-level pill', async () => {
    const wrapper = await mountPeerFiles('tor')

    // Every file on this page came from the same peer over the same transport,
    // so the header pill already states it once. Repeating it per card would
    // add no information and would crowd the row at 320px.
    const cardBody = wrapper.findAll('div').find(d => {
      const c = d.classes()
      return c.includes('p-4') && c.includes('flex') && c.includes('mt-auto')
    })
    expect(cardBody, 'PeerFiles per-file card body is missing').toBeTruthy()

    for (const label of ['FIPS', 'Mesh', 'LAN', 'Tor']) {
      expect(
        cardBody!.findAll('span').some(s => s.text().trim() === label),
        `a transport pill appeared on a per-file card — see the header of this file`,
      ).toBe(false)
    }
  })
})
