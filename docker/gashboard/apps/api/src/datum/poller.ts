import { config } from "../config.js";
import { logger } from "../logger.js";
import { digestFetch } from "./digest.js";
import { parseClientsHtml, parseThreadsHtml } from "./parse.js";
import type { CurrentJob, DatumSnapshot, MinerStat, NetworkStat, PoolStat } from "./types.js";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { dirname } from "node:path";

const DEFAULT_TIMEOUT_MS = 10_000;

const EMPTY_POOL: PoolStat = {
  combinedHashrateThs: 0,
  totalConnections: 0,
  totalSubscriptions: 0,
  activeThreads: 0,
  sharesAccepted: 0,
  sharesRejected: 0,
  uptimeSeconds: 0,
  connectionStatus: "unknown",
  poolHost: "",
  poolTag: "",
  minerTag: "",
  poolDifficulty: 0,
};

const EMPTY_JOB: CurrentJob = {
  blockHeight: 0,
  blockValueSats: 0,
  difficulty: 0,
  bits: "",
  prevBlock: "",
  target: "",
  version: "",
  weight: 0,
  size: 0,
  txnCount: 0,
  timeInfo: "",
};

const EMPTY_NETWORK: NetworkStat = {
  blockHeight: 0,
  hashrateEh: 0,
  difficulty: 0,
  source: "",
};

let lastSnapshot: DatumSnapshot = {
  ok: false,
  fetchedAt: 0,
  pool: EMPTY_POOL,
  job: EMPTY_JOB,
  network: EMPTY_NETWORK,
  miners: [],
  error: { code: "NOT_YET_POLLED", message: "Poller has not run yet" },
};
let timer: NodeJS.Timeout | null = null;
let pollInFlight = false;

type ContributionEntry = {
  miner: MinerStat;
  lastRawAcceptedWork: number;
  lastRawAcceptedShares: number;
  totalAcceptedWork: number;
  totalAcceptedShares: number;
  firstSeenAt: number;
  lastSeenAt: number;
  currentlyConnected: boolean;
};

const contributionLedger = new Map<string, ContributionEntry>();
let ledgerLoaded = false;
let ledgerDirty = false;
let ledgerWriteInFlight: Promise<void> | null = null;

type PersistedContributionLedger = {
  version: 1;
  updatedAt: number;
  entries: Array<[string, ContributionEntry]>;
};

function formatErr(err: unknown): string {
  if (!(err instanceof Error)) return String(err);
  const cause = (err as Error & { cause?: unknown }).cause;
  if (cause instanceof Error) {
    const code = (cause as Error & { code?: string }).code;
    const hostname = (cause as Error & { hostname?: string }).hostname;
    if (code === "ENOTFOUND" && hostname) {
      const datumHost = new URL(config.datum.url).hostname;
      const hint =
        hostname === datumHost
          ? "; make sure gashboard is attached to Umbrel's Docker network or set DATUM_URL to a hostname/IP reachable from the API container"
          : "";
      return `${err.message}: DNS could not resolve ${hostname} (${code})${hint}`;
    }
    return code ? `${err.message}: ${cause.message} (${code})` : `${err.message}: ${cause.message}`;
  }
  return err.message;
}

function abortableSignal(timeoutMs: number): { signal: AbortSignal; cancel: () => void } {
  const ctrl = new AbortController();
  const t = setTimeout(() => ctrl.abort(new Error("upstream_timeout")), timeoutMs);
  return { signal: ctrl.signal, cancel: () => clearTimeout(t) };
}

function datumHttpMessage(res: Response, path: string): string {
  const location = res.headers.get("location");
  if (res.status >= 300 && res.status < 400) {
    return location
      ? `Datum ${path} redirected to ${location}; DATUM_URL is probably pointing at the Umbrel web proxy, not the Datum admin API`
      : `Datum ${path} redirected; DATUM_URL is probably pointing at the Umbrel web proxy, not the Datum admin API`;
  }
  return `Datum ${path} returned ${res.status}`;
}

function isUmbrelShellHtml(html: string): boolean {
  return /<title>\s*Umbrel\s*<\/title>/i.test(html) || /<div\s+id=["']root["']/i.test(html);
}

type MempoolHashrate = {
  currentHashrate?: number;
  currentDifficulty?: number;
};

function minerIdentity(m: MinerStat): string {
  return m.authUsername || m.remoteHost || m.nickname;
}

function validContributionEntry(entry: ContributionEntry): boolean {
  return (
    entry &&
    typeof entry === "object" &&
    typeof entry.lastRawAcceptedWork === "number" &&
    typeof entry.lastRawAcceptedShares === "number" &&
    typeof entry.totalAcceptedWork === "number" &&
    typeof entry.totalAcceptedShares === "number" &&
    typeof entry.firstSeenAt === "number" &&
    typeof entry.lastSeenAt === "number" &&
    entry.miner &&
    typeof entry.miner === "object"
  );
}

async function loadContributionLedger(): Promise<void> {
  if (ledgerLoaded) return;
  ledgerLoaded = true;
  try {
    const raw = await readFile(config.datum.contributionLedgerPath, "utf8");
    const parsed = JSON.parse(raw) as PersistedContributionLedger;
    if (parsed.version !== 1 || !Array.isArray(parsed.entries)) {
      throw new Error("unsupported contribution ledger format");
    }
    contributionLedger.clear();
    for (const [key, entry] of parsed.entries) {
      if (typeof key === "string" && validContributionEntry(entry)) {
        contributionLedger.set(key, { ...entry, currentlyConnected: false });
      }
    }
    logger.info(
      { entries: contributionLedger.size, path: config.datum.contributionLedgerPath },
      "contribution_ledger_loaded",
    );
  } catch (err) {
    const code = err instanceof Error ? (err as Error & { code?: string }).code : undefined;
    if (code === "ENOENT") return;
    logger.warn({ reason: formatErr(err), path: config.datum.contributionLedgerPath }, "contribution_ledger_load_failed");
  }
}

function scheduleContributionLedgerSave(): void {
  ledgerDirty = true;
  if (ledgerWriteInFlight) return;
  ledgerWriteInFlight = saveContributionLedger()
    .catch((err) => {
      logger.warn({ reason: formatErr(err), path: config.datum.contributionLedgerPath }, "contribution_ledger_save_failed");
    })
    .finally(() => {
      ledgerWriteInFlight = null;
      if (ledgerDirty) scheduleContributionLedgerSave();
    });
}

async function saveContributionLedger(): Promise<void> {
  if (!ledgerDirty) return;
  ledgerDirty = false;
  const path = config.datum.contributionLedgerPath;
  await mkdir(dirname(path), { recursive: true });
  const tmpPath = `${path}.${process.pid}.tmp`;
  const payload: PersistedContributionLedger = {
    version: 1,
    updatedAt: Date.now(),
    entries: [...contributionLedger.entries()],
  };
  await writeFile(tmpPath, `${JSON.stringify(payload)}\n`, "utf8");
  await rename(tmpPath, path);
}

async function applyContributionLedger(miners: MinerStat[], fetchedAt: number): Promise<MinerStat[]> {
  await loadContributionLedger();
  let changed = false;

  for (const entry of contributionLedger.values()) {
    entry.currentlyConnected = false;
  }

  const connectedKeys = new Set<string>();
  for (const miner of miners) {
    const key = minerIdentity(miner);
    connectedKeys.add(key);
    const existing = contributionLedger.get(key);

    if (!existing) {
      contributionLedger.set(key, {
        miner,
        lastRawAcceptedWork: miner.diffAcceptedSum,
        lastRawAcceptedShares: miner.diffAcceptedCount,
        totalAcceptedWork: miner.diffAcceptedSum,
        totalAcceptedShares: miner.diffAcceptedCount,
        firstSeenAt: fetchedAt,
        lastSeenAt: fetchedAt,
        currentlyConnected: true,
      });
      changed = true;
      continue;
    }

    const workDelta = miner.diffAcceptedSum - existing.lastRawAcceptedWork;
    const shareDelta = miner.diffAcceptedCount - existing.lastRawAcceptedShares;

    existing.totalAcceptedWork += workDelta >= 0 ? workDelta : miner.diffAcceptedSum;
    existing.totalAcceptedShares += shareDelta >= 0 ? shareDelta : miner.diffAcceptedCount;
    if (workDelta > 0 || shareDelta > 0 || workDelta < 0 || shareDelta < 0) changed = true;
    existing.lastRawAcceptedWork = miner.diffAcceptedSum;
    existing.lastRawAcceptedShares = miner.diffAcceptedCount;
    existing.lastSeenAt = fetchedAt;
    existing.currentlyConnected = true;
    existing.miner = miner;
  }

  if (changed) scheduleContributionLedgerSave();

  const totalWork = [...contributionLedger.values()].reduce((sum, entry) => sum + entry.totalAcceptedWork, 0);
  const decorate = (miner: MinerStat, entry: ContributionEntry): MinerStat => ({
    ...miner,
    contributionAcceptedWork: entry.totalAcceptedWork,
    contributionAcceptedShares: entry.totalAcceptedShares,
    contributionPct: totalWork > 0 ? (entry.totalAcceptedWork / totalWork) * 100 : 0,
    contributionFirstSeenAt: entry.firstSeenAt,
    contributionLastSeenAt: entry.lastSeenAt,
    currentlyConnected: entry.currentlyConnected,
  });

  const live = miners.map((miner) => decorate(miner, contributionLedger.get(minerIdentity(miner))!));
  const offline = [...contributionLedger.entries()]
    .filter(([key]) => !connectedKeys.has(key))
    .map(([, entry]) =>
      decorate(
        {
          ...entry.miner,
          hashrateThs: 0,
          hashrateAgeS: null,
          subscribed: false,
          status: "idle",
        },
        entry,
      ),
    );

  return [...live, ...offline].sort((a, b) => (b.contributionAcceptedWork ?? 0) - (a.contributionAcceptedWork ?? 0));
}

async function fetchNetworkStats(): Promise<NetworkStat> {
  const timeout = abortableSignal(DEFAULT_TIMEOUT_MS);
  try {
    const [heightRes, hashrateRes] = await Promise.all([
      fetch(`${config.mempool.url}/blocks/tip/height`, { signal: timeout.signal }),
      fetch(`${config.mempool.url}/v1/mining/hashrate/3d`, { signal: timeout.signal }),
    ]);
    if (!heightRes.ok || !hashrateRes.ok) {
      throw new Error(`mempool returned ${heightRes.status}/${hashrateRes.status}`);
    }
    const [heightText, hashrateJson] = await Promise.all([
      heightRes.text(),
      hashrateRes.json() as Promise<MempoolHashrate>,
    ]);
    const blockHeight = Number(heightText);
    return {
      blockHeight: Number.isFinite(blockHeight) ? blockHeight : 0,
      hashrateEh: (hashrateJson.currentHashrate ?? 0) / 1e18,
      difficulty: hashrateJson.currentDifficulty ?? 0,
      source: config.mempool.url,
    };
  } finally {
    timeout.cancel();
  }
}

async function pollOnce(): Promise<DatumSnapshot> {
  const fetchedAt = Date.now();
  const datumUrl = config.datum.url;

  // /clients (admin Digest-gated) and /threads (public) in parallel.
  const clientsTimeout = abortableSignal(DEFAULT_TIMEOUT_MS);
  const threadsTimeout = abortableSignal(DEFAULT_TIMEOUT_MS);

  try {
    const [clientsResSettled, threadsResSettled] = await Promise.allSettled([
      digestFetch({
        url: `${datumUrl}/clients`,
        creds: { username: config.datum.adminUser, password: config.datum.adminPassword },
        signal: clientsTimeout.signal,
      }),
      fetch(`${datumUrl}/threads`, { signal: threadsTimeout.signal, redirect: "manual" }),
    ]);
    clientsTimeout.cancel();
    threadsTimeout.cancel();

    if (clientsResSettled.status === "rejected") {
      const reason = formatErr(clientsResSettled.reason);
      logger.warn({ reason, url: `${datumUrl}/clients` }, "datum_clients_fetch_failed");
      return {
        ...lastSnapshot,
        ok: false,
        fetchedAt,
        error: { code: "DATUM_UNREACHABLE", message: reason },
      };
    }
    const clientsRes = clientsResSettled.value;
    if (clientsRes.status === 401) {
      return {
        ...lastSnapshot,
        ok: false,
        fetchedAt,
        error: {
          code: "DATUM_AUTH_FAIL",
          message: "Datum rejected admin credentials (check DATUM_ADMIN_PASSWORD)",
        },
      };
    }
    if (clientsRes.status !== 200) {
      const message = datumHttpMessage(clientsRes, "/clients");
      logger.warn(
        { status: clientsRes.status, url: `${datumUrl}/clients`, location: clientsRes.headers.get("location") },
        "datum_clients_bad_status",
      );
      return {
        ...lastSnapshot,
        ok: false,
        fetchedAt,
        error: { code: "DATUM_HTTP", message },
      };
    }

    const clientsHtml = await clientsRes.text();
    if (isUmbrelShellHtml(clientsHtml)) {
      const message =
        "DATUM_URL returned the Umbrel web shell, not Datum /clients; use the Datum container admin API URL";
      logger.warn({ url: `${datumUrl}/clients` }, "datum_clients_wrong_html");
      return {
        ...lastSnapshot,
        ok: false,
        fetchedAt,
        error: { code: "DATUM_BAD_RESPONSE", message },
      };
    }

    let threadsHtml = "";
    if (threadsResSettled.status === "fulfilled" && threadsResSettled.value.ok) {
      threadsHtml = await threadsResSettled.value.text();
    } else if (
      threadsResSettled.status === "fulfilled" &&
      threadsResSettled.value.status >= 300 &&
      threadsResSettled.value.status < 400
    ) {
      logger.warn(
        {
          status: threadsResSettled.value.status,
          url: `${datumUrl}/threads`,
          location: threadsResSettled.value.headers.get("location"),
        },
        "datum_threads_redirected",
      );
    } else if (threadsResSettled.status === "rejected") {
      logger.warn(
        { reason: formatErr(threadsResSettled.reason) },
        "datum_threads_fetch_failed",
      );
    }

    const miners = await applyContributionLedger(parseClientsHtml(clientsHtml), fetchedAt);
    const threads = threadsHtml ? parseThreadsHtml(threadsHtml) : [];
    if (miners.length === 0) {
      logger.warn(
        { url: `${datumUrl}/clients`, bytes: clientsHtml.length },
        "datum_clients_no_miners_parsed",
      );
    }

    const combinedHashrateThs = miners.reduce((s, m) => s + m.hashrateThs, 0);
    const totalSubscriptions =
      threads.reduce((s, t) => s + t.subscriptions, 0) ||
      miners.filter((m) => m.subscribed).length;
    const totalConnections =
      threads.reduce((s, t) => s + t.connections, 0) || miners.length;
    const sharesAccepted = miners.reduce((s, m) => s + m.diffAcceptedCount, 0);
    const sharesRejected = miners.reduce((s, m) => s + m.diffRejectedCount, 0);
    let network = lastSnapshot.network.blockHeight > 0 ? lastSnapshot.network : EMPTY_NETWORK;
    try {
      network = await fetchNetworkStats();
    } catch (err) {
      logger.warn({ reason: formatErr(err), url: config.mempool.url }, "mempool_fetch_failed");
    }

    return {
      ok: true,
      fetchedAt,
      pool: {
        ...EMPTY_POOL,
        combinedHashrateThs,
        totalConnections,
        totalSubscriptions,
        activeThreads: threads.length,
        sharesAccepted,
        sharesRejected,
        connectionStatus: "ok",
      },
      job: {
        ...EMPTY_JOB,
        blockHeight: network.blockHeight,
        difficulty: network.difficulty,
      },
      network,
      miners,
    };
  } catch (err) {
    clientsTimeout.cancel();
    threadsTimeout.cancel();
    const reason = formatErr(err);
    logger.warn({ reason }, "datum_poll_unexpected_error");
    return {
      ...lastSnapshot,
      ok: false,
      fetchedAt,
      error: { code: "POLL_ERROR", message: reason },
    };
  }
}

export function startPoller(): void {
  if (timer) return;
  const tick = async (): Promise<void> => {
    if (pollInFlight) return;
    pollInFlight = true;
    try {
      lastSnapshot = await pollOnce();
    } finally {
      pollInFlight = false;
    }
  };
  void tick();
  timer = setInterval(() => void tick(), config.datum.pollIntervalMs);
}

export function stopPoller(): void {
  if (timer) clearInterval(timer);
  timer = null;
}

export function getSnapshot(): DatumSnapshot {
  return lastSnapshot;
}
