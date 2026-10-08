import { parse, type HTMLElement } from "node-html-parser";
import { MINER_PROFILES } from "./profiles.js";
import type { MinerProfile, MinerStat } from "./types.js";

const UNKNOWN_PROFILE: MinerProfile = {
  slug: "unknown",
  nickname: "Unknown miner",
  model: "Unknown",
  ownerLabel: "unknown",
  locationLabel: "unknown",
  expectedHashrateThs: 0,
  watts: 0,
  workerNameMatchers: [],
};

export type ThreadRow = {
  tid: number;
  connections: number;
  subscriptions: number;
  hashrateThs: number;
};

function num(s: string | undefined | null): number {
  if (!s) return 0;
  const m = s.replace(/,/g, "").match(/-?\d+(\.\d+)?/);
  return m ? Number(m[0]) : 0;
}

function thsFromHashrateField(field: string): number {
  const numMatch = field.match(/-?\d+(?:\.\d+)?/);
  if (!numMatch) return 0;
  const v = Number(numMatch[0]);
  if (/Gh\/s/i.test(field)) return v / 1_000;
  if (/Mh\/s/i.test(field)) return v / 1_000_000;
  if (/Kh\/s/i.test(field)) return v / 1_000_000_000;
  return v;
}

function parseAgeS(s: string): number | null {
  const m = s.match(/-?\d+(?:\.\d+)?/);
  return m ? Number(m[0]) : null;
}

function findTableContaining(html: string, marker: string): HTMLElement | null {
  const root = parse(html);
  for (const t of root.querySelectorAll("table")) {
    if (t.text.includes(marker)) return t;
  }
  return null;
}

export function matchProfile(userAgent: string, authUsername: string): MinerProfile {
  if (/nerdq?axe/i.test(userAgent)) {
    const p = MINER_PROFILES.find((x) => x.slug === "nerdqaxe");
    if (p) return p;
  }
  if (/bitaxe/i.test(userAgent)) {
    const p = MINER_PROFILES.find((x) => x.slug === "bitaxe");
    if (p) return p;
  }
  const dotIdx = authUsername.indexOf(".");
  if (dotIdx >= 0) {
    const worker = authUsername.slice(dotIdx + 1).toLowerCase();
    for (const p of MINER_PROFILES) {
      for (const m of p.workerNameMatchers) {
        if (worker.includes(m.toLowerCase())) return p;
      }
    }
  }
  return UNKNOWN_PROFILE;
}

export function parseClientsHtml(html: string): MinerStat[] {
  const table = findTableContaining(html, "Auth Username");
  if (!table) return [];
  const out: MinerStat[] = [];
  const trs = table.querySelectorAll("tr");
  for (let i = 1; i < trs.length; i++) {
    const tds = trs[i]!.querySelectorAll("td");
    if (tds.length < 11) continue;

    const remoteHost = tds[1]!.text.trim();
    const authUsername = tds[2]!.text.trim();
    const subbedField = tds[3]!.text.trim();
    const lastAcceptedField = tds[4]!.text.trim();
    const vdiffField = tds[5]!.text.trim();
    const diffAField = tds[6]!.text.trim();
    const diffRField = tds[7]!.text.trim();
    const hashrateField = tds[8]!.text.trim();
    const userAgent = tds[10]!.text.trim();

    const subscribed = !subbedField.toLowerCase().startsWith("not subscribed");
    const lastShareAgeS = lastAcceptedField === "N/A" ? null : parseAgeS(lastAcceptedField);
    const vdiff = num(vdiffField);

    const diffAMatch = diffAField.match(/(-?\d+(?:\.\d+)?)\s*\((\d+)\)/);
    const diffAcceptedSum = diffAMatch ? Number(diffAMatch[1]) : 0;
    const diffAcceptedCount = diffAMatch ? Number(diffAMatch[2]) : 0;

    const diffRMatch = diffRField.match(/(-?\d+(?:\.\d+)?)\s*\((\d+)\)\s*([\d.]+)%/);
    const diffRejectedSum = diffRMatch ? Number(diffRMatch[1]) : 0;
    const diffRejectedCount = diffRMatch ? Number(diffRMatch[2]) : 0;
    const rejectPct = diffRMatch ? Number(diffRMatch[3]) : 0;

    let hashrateThs = 0;
    let hashrateAgeS: number | null = null;
    if (hashrateField !== "N/A") {
      hashrateThs = thsFromHashrateField(hashrateField);
      const ageMatch = hashrateField.match(/\(\s*(-?\d+(?:\.\d+)?)\s*s\s*\)/);
      hashrateAgeS = ageMatch ? Number(ageMatch[1]) : null;
    }

    const profile = matchProfile(userAgent, authUsername);

    let status: MinerStat["status"];
    if (!subscribed) status = "idle";
    else if (hashrateThs <= 0) status = "idle";
    else if (hashrateAgeS !== null && hashrateAgeS > 180) status = "stale";
    else status = "hashing";

    out.push({
      authUsername,
      remoteHost,
      nickname: authUsername.split(".").slice(1).join(".") || authUsername || remoteHost || "Unknown miner",
      model: profile.model,
      location: "unassigned",
      expectedHashrateThs: profile.expectedHashrateThs,
      watts: profile.watts,
      hashrateThs,
      hashrateAgeS,
      lastShareAgeS,
      diffAcceptedSum,
      diffAcceptedCount,
      diffRejectedSum,
      diffRejectedCount,
      rejectPct,
      vdiff,
      userAgent,
      subscribed,
      status,
    });
  }
  return out;
}

export function parseThreadsHtml(html: string): ThreadRow[] {
  const table = findTableContaining(html, "Approx. Hashrate");
  if (!table) return [];
  const out: ThreadRow[] = [];
  const trs = table.querySelectorAll("tr");
  for (let i = 1; i < trs.length; i++) {
    const tds = trs[i]!.querySelectorAll("td");
    if (tds.length < 4) continue;
    out.push({
      tid: num(tds[0]!.text),
      connections: num(tds[1]!.text),
      subscriptions: num(tds[2]!.text),
      hashrateThs: thsFromHashrateField(tds[3]!.text.trim()),
    });
  }
  return out;
}
