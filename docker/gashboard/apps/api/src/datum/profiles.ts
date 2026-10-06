import type { MinerProfile } from "./types.js";

// Live UserAgent strings observed on this Umbrel (2026-05-06):
//   NerdQAxe → "NerdQAxe/BM1370/v1.0.36"  (self-identifies)
//   Bitaxe → "bitaxe/BM1368/v2.7.1" or "cgminer/4.11.1" depending on firmware
//   Avalon Nano 3 / Avalon Mini 3 → "cgminer/4.11.1" (match via worker-name suffix).
export const MINER_PROFILES: readonly MinerProfile[] = [
  {
    slug: "nerdqaxe",
    nickname: "QU4CK",
    model: "NerdQAxe+",
    ownerLabel: "shared",
    locationLabel: "Site A",
    expectedHashrateThs: 4.5,
    watts: 80,
    workerNameMatchers: ["nerdqaxe", "qu4ck", "quack"],
  },
  {
    slug: "bitaxe-bigpapa",
    nickname: "BigPapa",
    model: "Bitaxe",
    ownerLabel: "shared",
    locationLabel: "Site A",
    expectedHashrateThs: 1.2,
    watts: 18,
    workerNameMatchers: ["bigpapa", "big-papa", "big_papa"],
  },
  {
    slug: "bitaxe",
    nickname: "P1XEL",
    model: "Bitaxe",
    ownerLabel: "shared",
    locationLabel: "Site A",
    expectedHashrateThs: 1.2,
    watts: 18,
    workerNameMatchers: ["bitaxe", "p1xel", "pixel"],
  },
  {
    slug: "avalon-nano-3",
    nickname: "N4N0",
    model: "Avalon Canaan Nano 3",
    ownerLabel: "shared",
    locationLabel: "Site B",
    expectedHashrateThs: 4.0,
    watts: 140,
    workerNameMatchers: ["nano", "nano3", "n4n0"],
  },
  {
    slug: "avalon-mini-3",
    nickname: "M1N1",
    model: "Avalon Mini 3",
    ownerLabel: "shared",
    locationLabel: "Site B",
    expectedHashrateThs: 37.0,
    watts: 800,
    workerNameMatchers: ["mini", "mini3", "m1n1"],
  },
] as const;

export function profileForWorker(workerName: string): MinerProfile | null {
  const lower = workerName.toLowerCase();
  for (const p of MINER_PROFILES) {
    for (const m of p.workerNameMatchers) {
      if (lower.includes(m)) return p;
    }
  }
  return null;
}
