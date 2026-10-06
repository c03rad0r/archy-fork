import { nip19 } from "nostr-tools";
import { z } from "zod";

const RawEnv = z.object({
  PORT: z.coerce.number().int().min(1).max(65535).default(1337),
  NODE_ENV: z.enum(["development", "production", "test"]).default("production"),
  LOG_LEVEL: z
    .enum(["fatal", "error", "warn", "info", "debug", "trace", "silent"])
    .default("info"),

  CORS_ORIGIN: z.string().optional(),
  STATIC_DIR: z.string().optional(),

  DATUM_URL: z.string().url().optional(),
  DATUM_ADMIN_USER: z.string().default("admin"),
  DATUM_ADMIN_PASSWORD: z.string().min(1),
  DATUM_POLL_INTERVAL_MS: z.coerce.number().int().min(1000).default(5000),
  CONTRIBUTION_LEDGER_PATH: z.string().min(1).default("/data/contribution-ledger.json"),
  MEMPOOL_API_URL: z.string().url().default("https://tx1138.com/api"),

  NOSTR_ALLOWED_NPUBS: z.string().default(""),
  NOSTR_OWNER_PUBKEYS: z.string().default(""),
  CHAT_STORE_PATH: z.string().min(1).default("/data/chat.json"),
  ACCESS_LIST_PATH: z.string().default("/data/access.json"),
  ARCHIPELAGO_PROVIDER_PATH: z.string().optional(),

  JWT_SECRET: z
    .string()
    .min(32, "JWT_SECRET must be at least 32 chars (use openssl rand -hex 32)"),
  JWT_TTL_SECONDS: z.coerce.number().int().min(60).default(86400),
});

function parseAllowedNpubs(input: string): string[] {
  const out: string[] = [];
  for (const raw of input.split(",").map((s) => s.trim()).filter(Boolean)) {
    const decoded = nip19.decode(raw);
    if (decoded.type !== "npub") {
      throw new Error(`NOSTR_ALLOWED_NPUBS entry is not an npub: ${raw}`);
    }
    const hex = decoded.data;
    if (!/^[0-9a-f]{64}$/.test(hex)) {
      throw new Error(`Decoded npub is not 64-char hex: ${raw}`);
    }
    out.push(hex);
  }
  return out;
}

const parsed = RawEnv.parse(process.env);
const ownerHexPubkeys = parsed.NOSTR_OWNER_PUBKEYS.split(",").map(k => k.trim()).filter(Boolean);
if (ownerHexPubkeys.some(k => !/^[0-9a-f]{64}$/.test(k))) {
  throw new Error("NOSTR_OWNER_PUBKEYS must contain comma-separated 64-character hex public keys");
}
const allowedHexPubkeys = parseAllowedNpubs(parsed.NOSTR_ALLOWED_NPUBS);
if (!ownerHexPubkeys.length && !allowedHexPubkeys.length) {
  throw new Error("Configure at least one owner or allowlisted npub before starting Gashboard");
}

export const config = {
  port: parsed.PORT,
  nodeEnv: parsed.NODE_ENV,
  logLevel: parsed.LOG_LEVEL,
  corsOrigin: parsed.CORS_ORIGIN,
  staticDir: parsed.STATIC_DIR,
  chatStorePath: parsed.CHAT_STORE_PATH,
  providerPath: parsed.ARCHIPELAGO_PROVIDER_PATH,
  datum: {
    url: (parsed.DATUM_URL ?? "http://127.0.0.1:21000").replace(/\/$/, ""),
    adminUser: parsed.DATUM_ADMIN_USER,
    adminPassword: parsed.DATUM_ADMIN_PASSWORD,
    pollIntervalMs: parsed.DATUM_POLL_INTERVAL_MS,
    contributionLedgerPath: parsed.CONTRIBUTION_LEDGER_PATH,
  },
  mempool: {
    url: parsed.MEMPOOL_API_URL.replace(/\/$/, ""),
  },
  nostr: {
    allowedHexPubkeys,
    ownerHexPubkeys,
    accessListPath: parsed.ACCESS_LIST_PATH,
  },
  jwt: {
    secret: parsed.JWT_SECRET,
    ttlSeconds: parsed.JWT_TTL_SECONDS,
  },
} as const;
