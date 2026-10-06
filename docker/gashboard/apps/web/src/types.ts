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
  status: "hashing" | "stale" | "idle";
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

export type HistoryPoint = {
  t: number;
  ths: number;
  accepted: number;
  rejected: number;
  connections: number;
  subscriptions: number;
  height: number;
  miners: Record<string, number>;
  minerWork: Record<string, number>;
  minerShares: Record<string, number>;
  minerStatus: Record<string, MinerStat["status"]>;
};

export type LoginResponse = {
  token: string;
  npub: string;
  expiresAt: number;
};

export type ChatSnapshot = {
  messages: Array<{
    id: string;
    pubkey: string;
    content: string;
    createdAt: number;
    replyToId: string;
    replyToPubkey: string;
  }>;
  reactions: Array<{
    id: string;
    pubkey: string;
    messageId: string;
    content: string;
    createdAt: number;
  }>;
  keyWraps: Array<{
    recipientPubkey: string;
    senderPubkey: string;
    ciphertext: string;
    updatedAt: number;
  }>;
};

export type ApiError = { error: { code: string; message: string } };

declare global {
  interface Window {
    archipelagoNostr?: {
      selectIdentity(): Promise<unknown>;
      getSelectedIdentity(): { nostr_pubkey: string } | null;
    };
    nostr?: {
      getPublicKey(): Promise<string>;
      signEvent(event: {
        kind: number;
        created_at: number;
        tags: string[][];
        content: string;
      }): Promise<{
        id: string;
        pubkey: string;
        kind: number;
        created_at: number;
        tags: string[][];
        content: string;
        sig: string;
      }>;
      nip44?: {
        encrypt(pubkey: string, plaintext: string): Promise<string>;
        decrypt(pubkey: string, ciphertext: string): Promise<string>;
      };
      nip04?: {
        encrypt(pubkey: string, plaintext: string): Promise<string>;
        decrypt(pubkey: string, ciphertext: string): Promise<string>;
      };
    };
  }
}
