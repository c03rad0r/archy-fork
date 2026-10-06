export type MinerProfile = {
  slug: string;
  nickname: string;
  model: string;
  ownerLabel: string;
  locationLabel: string;
  expectedHashrateThs: number;
  watts: number;
  workerNameMatchers: readonly string[];
};

export type MinerStat = {
  authUsername: string;
  remoteHost: string;
  nickname: string;
  model: string;
  location: string;
  expectedHashrateThs: number;
  watts: number;
  hashrateThs: number;
  hashrateAgeS: number | null;
  lastShareAgeS: number | null;
  diffAcceptedSum: number;
  diffAcceptedCount: number;
  diffRejectedSum: number;
  diffRejectedCount: number;
  rejectPct: number;
  vdiff: number;
  userAgent: string;
  subscribed: boolean;
  status: 'hashing' | 'stale' | 'idle';
  contributionAcceptedWork?: number;
  contributionAcceptedShares?: number;
  contributionPct?: number;
  contributionFirstSeenAt?: number;
  contributionLastSeenAt?: number;
  currentlyConnected?: boolean;
};

export type PoolStat = {
  combinedHashrateThs: number;
  totalConnections: number;
  totalSubscriptions: number;
  activeThreads: number;
  sharesAccepted: number;
  sharesRejected: number;
  uptimeSeconds: number;
  connectionStatus: string;
  poolHost: string;
  poolTag: string;
  minerTag: string;
  poolDifficulty: number;
};

export type CurrentJob = {
  blockHeight: number;
  blockValueSats: number;
  difficulty: number;
  bits: string;
  prevBlock: string;
  target: string;
  version: string;
  weight: number;
  size: number;
  txnCount: number;
  timeInfo: string;
};

export type NetworkStat = {
  blockHeight: number;
  hashrateEh: number;
  difficulty: number;
  source: string;
};

export type DatumSnapshot = {
  ok: boolean;
  fetchedAt: number;
  pool: PoolStat;
  job: CurrentJob;
  network: NetworkStat;
  miners: MinerStat[];
  error?: { code: string; message: string };
};
