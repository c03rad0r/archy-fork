export type ReleaseNote = {
  hash: string;
  title: string;
  bullets: string[];
};

export const RELEASE_NOTES: ReleaseNote[] = [
  {
    hash: "b715c3f",
    title: "Nostr chat drawer",
    bullets: [
      "Added a signed Nostr chat panel, because apparently the miners needed a comments section for witnesses.",
      "Desktop slides in from the right; mobile rises as a half-screen sheet like a tiny gossip bunker.",
      "Loads kind-0 profile names and avatars, so the abuse has proper attribution.",
    ],
  },
  {
    hash: "b2f81a1",
    title: "Tidier hero stats",
    bullets: [
      "Pinned the hero stat boxes down so rotating jokes stop rearranging the furniture.",
      "Aligned block height, network hashrate, and share stats like they have jobs.",
    ],
  },
  {
    hash: "1f86bf9",
    title: "Readable odds",
    bullets: [
      "Replaced cursed science notation with human-readable odds like '1 in 2.0B', which is still awful but now legible.",
      "Slowed rotating labels and lottery jokes to 10 seconds so the dashboard stops acting caffeinated.",
    ],
  },
  {
    hash: "fa707e2",
    title: "Network stats and rotating jokes",
    bullets: [
      "Added block height, network hashrate, and difficulty from tx1138 mempool, because Datum was keeping the good gossip to itself.",
      "Solo odds now use current network hashrate, so the humiliation is freshly calculated.",
      "Added rotating labels and a large pile of darker lottery jokes about fiat, Bitcoin, and electrical self-deception.",
      "Race view now compares actual miners to a large-pool-ish bully and the total network, on a mercy scale so the children remain visible.",
    ],
  },
  {
    hash: "7e1f7a1",
    title: "Heat-reactive theme",
    bullets: [
      "Interface now gets redder from 10 TH/s to 70 TH/s, because someone's getting warmed tonight.",
    ],
  },
  {
    hash: "243cd0d",
    title: "Boomer Heater got worse",
    bullets: [
      "Gave Boomer Heater a dead status, skull marker, and custom zero-stat labels because it contributes exactly fuck all.",
      "Updated the heater asset to say 'NO HASHES. JUST GHEI.' Accurate thermodynamics, questionable monetary policy.",
    ],
  },
  {
    hash: "d6f8fd7",
    title: "Boomer Heater on status",
    bullets: [
      "Added fake Boomer Heater to the status screen, always last, where fiat appliances belong.",
      "Race copy now says 'just fiat and a pension.' No sats, no shares, no future.",
    ],
  },
  {
    hash: "0c8e26f",
    title: "PWA and miner race",
    bullets: [
      "Made gashboard installable as a PWA with icons, service worker, and cache rules that should stop stale bundles haunting the cupboard.",
      "Remote signer login can resume after Primal/Amber returns to the PWA callback, because mobile auth should not require ritual sacrifice.",
      "Added miner race visuals with local miner assets, including the non-hashing radiator of shame.",
    ],
  },
  {
    hash: "360b1eb",
    title: "BigPapa",
    bullets: ["Renamed the unknown Datum miner from ??? to BigPapa. Same mystery, more authority."],
  },
  {
    hash: "c2c376f",
    title: "Shameboard calculations",
    bullets: [
      "Added best observed share/work, expected block time, sats/day fantasy math, share velocity, reject pain, efficiency, BTU/hr, and per-miner insults. A spreadsheet with a personality disorder.",
    ],
  },
  {
    hash: "c77c746",
    title: "Graphs and remote signer login",
    bullets: [
      "Added graph tab with hashrate telemetry, share flow, miner mix, and browser-local history so the suffering can be trended.",
      "Added Nostr Connect 'open signer app' login for Primal/Amber handoff, replacing the bunker URI homework nobody asked for.",
      "Fixed Portainer-on-Umbrel deployment compose for host networking and direct Datum URL, because Portainer's fake network was cosplay.",
    ],
  },
];
