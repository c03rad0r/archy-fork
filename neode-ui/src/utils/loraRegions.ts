/**
 * LoRa regional frequency plans — the canonical region list the backend's
 * Meshtastic driver can program (core/.../mesh/meshtastic.rs region table),
 * annotated with band / duty-cycle / power constraints from the Meshtastic
 * firmware `regions[]` table so the UI can surface legality hints.
 *
 * Also used to derive suggested radio parameters for MeshCore and RNode
 * presets (those firmwares take raw frequency/BW/SF/CR, not a region enum).
 */

export interface LoraRegion {
  /** Wire value for mesh.configure { lora_region } */
  code: string
  label: string
  /** Frequency range, MHz (display) */
  band: string
  /** Regional duty-cycle cap, percent (100 = none) */
  dutyCyclePct: number
  /** Max TX power, dBm */
  maxPowerDbm: number
}

export const LORA_REGIONS: LoraRegion[] = [
  { code: 'US', label: 'United States / Americas (915 MHz)', band: '902–928', dutyCyclePct: 100, maxPowerDbm: 30 },
  { code: 'EU_868', label: 'Europe (868 MHz)', band: '869.4–869.65', dutyCyclePct: 10, maxPowerDbm: 27 },
  { code: 'EU_433', label: 'Europe (433 MHz)', band: '433–434', dutyCyclePct: 10, maxPowerDbm: 10 },
  { code: 'ANZ', label: 'Australia / New Zealand (915 MHz)', band: '915–928', dutyCyclePct: 100, maxPowerDbm: 30 },
  { code: 'ANZ_433', label: 'Australia / New Zealand (433 MHz)', band: '433.05–434.79', dutyCyclePct: 100, maxPowerDbm: 14 },
  { code: 'NZ_865', label: 'New Zealand (865 MHz)', band: '864–868', dutyCyclePct: 100, maxPowerDbm: 36 },
  { code: 'CN', label: 'China (470 MHz)', band: '470–510', dutyCyclePct: 100, maxPowerDbm: 19 },
  { code: 'JP', label: 'Japan (920 MHz)', band: '920.5–923.5', dutyCyclePct: 100, maxPowerDbm: 13 },
  { code: 'KR', label: 'South Korea (920 MHz)', band: '920–923', dutyCyclePct: 100, maxPowerDbm: 23 },
  { code: 'TW', label: 'Taiwan (920 MHz)', band: '920–925', dutyCyclePct: 100, maxPowerDbm: 27 },
  { code: 'RU', label: 'Russia (868 MHz)', band: '868.7–869.2', dutyCyclePct: 100, maxPowerDbm: 20 },
  { code: 'IN', label: 'India (865 MHz)', band: '865–867', dutyCyclePct: 100, maxPowerDbm: 30 },
  { code: 'TH', label: 'Thailand (920 MHz)', band: '920–925', dutyCyclePct: 10, maxPowerDbm: 27 },
  { code: 'UA_868', label: 'Ukraine (868 MHz)', band: '868–868.6', dutyCyclePct: 1, maxPowerDbm: 14 },
  { code: 'UA_433', label: 'Ukraine (433 MHz)', band: '433–434.7', dutyCyclePct: 10, maxPowerDbm: 10 },
  { code: 'MY_919', label: 'Malaysia (919 MHz)', band: '919–924', dutyCyclePct: 100, maxPowerDbm: 27 },
  { code: 'MY_433', label: 'Malaysia (433 MHz)', band: '433–435', dutyCyclePct: 100, maxPowerDbm: 20 },
  { code: 'SG_923', label: 'Singapore (923 MHz)', band: '917–925', dutyCyclePct: 100, maxPowerDbm: 20 },
  { code: 'PH_915', label: 'Philippines (915 MHz)', band: '915–918', dutyCyclePct: 100, maxPowerDbm: 24 },
  { code: 'PH_868', label: 'Philippines (868 MHz)', band: '868–869.4', dutyCyclePct: 100, maxPowerDbm: 14 },
  { code: 'PH_433', label: 'Philippines (433 MHz)', band: '433–434.7', dutyCyclePct: 100, maxPowerDbm: 10 },
  { code: 'LORA_24', label: 'Worldwide (2.4 GHz, SX1280 only)', band: '2400–2483.5', dutyCyclePct: 100, maxPowerDbm: 10 },
]

export function regionByCode(code: string | null | undefined): LoraRegion | undefined {
  if (!code) return undefined
  return LORA_REGIONS.find(r => r.code === code.toUpperCase())
}

/**
 * Coarse lat/lon → LoRa region suggestion. Bounding boxes, most-specific
 * first — good enough to preselect the dropdown; the user confirms.
 */
export function suggestRegionFromLatLon(lat: number, lon: number): LoraRegion | undefined {
  const boxes: Array<{ code: string; latMin: number; latMax: number; lonMin: number; lonMax: number }> = [
    { code: 'JP', latMin: 24, latMax: 46, lonMin: 123, lonMax: 146 },
    { code: 'KR', latMin: 33, latMax: 39, lonMin: 124, lonMax: 132 },
    { code: 'TW', latMin: 21.5, latMax: 25.5, lonMin: 119.5, lonMax: 122.5 },
    { code: 'PH_915', latMin: 4, latMax: 21, lonMin: 116, lonMax: 127 },
    { code: 'SG_923', latMin: 1, latMax: 1.6, lonMin: 103.5, lonMax: 104.2 },
    { code: 'MY_919', latMin: 0.8, latMax: 7.5, lonMin: 99, lonMax: 119.5 },
    { code: 'TH', latMin: 5.5, latMax: 20.5, lonMin: 97, lonMax: 106 },
    { code: 'IN', latMin: 6, latMax: 36, lonMin: 68, lonMax: 97.5 },
    { code: 'CN', latMin: 18, latMax: 54, lonMin: 73, lonMax: 135 },
    { code: 'UA_868', latMin: 44, latMax: 52.5, lonMin: 22, lonMax: 40.5 },
    { code: 'RU', latMin: 41, latMax: 82, lonMin: 27, lonMax: 180 },
    { code: 'ANZ', latMin: -48, latMax: -9, lonMin: 112, lonMax: 180 },
    { code: 'EU_868', latMin: 34, latMax: 72, lonMin: -25, lonMax: 45 },
    // Americas
    { code: 'US', latMin: -56, latMax: 72, lonMin: -170, lonMax: -30 },
  ]
  for (const b of boxes) {
    if (lat >= b.latMin && lat <= b.latMax && lon >= b.lonMin && lon <= b.lonMax) {
      return regionByCode(b.code)
    }
  }
  return undefined
}

/** Community MeshCore radio plan for a region, where one is well established.
 *  MeshCore has no region enum — the firmware takes raw freq/BW/SF/CR — so
 *  these are display-level suggestions (EU is the verified community default;
 *  other regions show the legal band and defer to the local community plan). */
export interface MeshcorePlan {
  freqMhz: number
  bwKhz: number
  sf: number
  cr: number
}

export const MESHCORE_PLANS: Record<string, MeshcorePlan> = {
  EU_868: { freqMhz: 869.525, bwKhz: 250, sf: 11, cr: 5 },
  EU_433: { freqMhz: 433.65, bwKhz: 250, sf: 11, cr: 5 },
}

export function meshcorePlanFor(code: string | null | undefined): MeshcorePlan | undefined {
  if (!code) return undefined
  return MESHCORE_PLANS[code.toUpperCase()]
}

/** A named MeshCore RF preset for the Device-panel dropdown. */
export interface MeshcoreRfPreset {
  id: string
  label: string
  freqMhz: number
  bwKhz: number
  sf: number
  cr: number
}

/**
 * Prepopulated MeshCore RF plans by country/region for the Device panel's
 * preset dropdown ("Custom" in the UI unlocks free entry). Values are only
 * added here when verifiable: the MeshCore community plans above, the
 * MeshCore firmware's own build defaults (platformio.ini `arduino_base`:
 * 869.618/62.5/SF8, CR default 5; companion example: 915.0/250/SF10/CR5),
 * and operator-attested deployment plans.
 */
export const MESHCORE_RF_PRESETS: MeshcoreRfPreset[] = [
  { id: 'eu_868', label: 'Europe / UK — 869.525 MHz, 250 kHz, SF11, CR4/5', freqMhz: 869.525, bwKhz: 250, sf: 11, cr: 5 },
  { id: 'pt', label: 'Portugal — 869.618 MHz, 62.5 kHz, SF8, CR4/8', freqMhz: 869.618, bwKhz: 62.5, sf: 8, cr: 8 },
  { id: 'fw_default', label: 'MeshCore firmware default — 869.618 MHz, 62.5 kHz, SF8, CR4/5', freqMhz: 869.618, bwKhz: 62.5, sf: 8, cr: 5 },
  { id: 'us_anz_915', label: 'US / Canada / ANZ — 915.0 MHz, 250 kHz, SF10, CR4/5', freqMhz: 915.0, bwKhz: 250, sf: 10, cr: 5 },
  { id: 'eu_433', label: 'Europe (433 MHz) — 433.65 MHz, 250 kHz, SF11, CR4/5', freqMhz: 433.65, bwKhz: 250, sf: 11, cr: 5 },
]

/** Recommended Reticulum RNode RF plan per region. Applied via
 *  mesh.rnode-config-apply (the daemon restarts the radio with these and the
 *  radio confirms them back). EU868 is the operator-validated Portugal plan:
 *  869.4625 MHz sits in the 10%-duty 869.4–869.65 sub-band clear of the
 *  default community channel, with the EU airtime locks written in. Others
 *  follow RNS community conventions with the region's legal power cap. */
export interface RnodeRegionPlan {
  frequency: number
  bandwidth: number
  spreading_factor: number
  coding_rate: number
  txpower: number
  airtime_limit_short: number | null
  airtime_limit_long: number | null
}
export const RNODE_REGION_PLANS: Record<string, RnodeRegionPlan> = {
  EU868: { frequency: 869462500, bandwidth: 125000, spreading_factor: 8, coding_rate: 5, txpower: 14, airtime_limit_short: 25, airtime_limit_long: 10 },
  US915: { frequency: 914875000, bandwidth: 125000, spreading_factor: 8, coding_rate: 5, txpower: 17, airtime_limit_short: null, airtime_limit_long: null },
  AU915: { frequency: 916800000, bandwidth: 125000, spreading_factor: 8, coding_rate: 5, txpower: 17, airtime_limit_short: null, airtime_limit_long: null },
  ANZ:   { frequency: 916800000, bandwidth: 125000, spreading_factor: 8, coding_rate: 5, txpower: 17, airtime_limit_short: null, airtime_limit_long: null },
  AS923: { frequency: 923200000, bandwidth: 125000, spreading_factor: 8, coding_rate: 5, txpower: 13, airtime_limit_short: null, airtime_limit_long: null },
  IN865: { frequency: 866000000, bandwidth: 125000, spreading_factor: 8, coding_rate: 5, txpower: 17, airtime_limit_short: null, airtime_limit_long: null },
}
