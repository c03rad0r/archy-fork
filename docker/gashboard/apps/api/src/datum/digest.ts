import { createHash, randomBytes } from "node:crypto";

export type DigestCreds = {
  username: string;
  password: string;
};

export type DigestFetchOptions = {
  url: string;
  method?: string;
  creds: DigestCreds;
  signal?: AbortSignal;
  headers?: Record<string, string>;
};

function sha256Hex(input: string): string {
  return createHash("sha256").update(input).digest("hex");
}

type Challenge = {
  realm: string;
  nonce: string;
  qop: string;
  algorithm: string;
  opaque?: string;
};

function parseChallenge(header: string): Challenge | null {
  if (!/^digest\s/i.test(header)) return null;
  const body = header.slice(7);
  const out: Record<string, string> = {};
  const re = /(\w+)\s*=\s*(?:"([^"]*)"|([^,\s]+))/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(body)) !== null) {
    out[m[1]!.toLowerCase()] = m[2] !== undefined ? m[2] : (m[3] ?? "");
  }
  if (!out["realm"] || !out["nonce"]) return null;
  return {
    realm: out["realm"],
    nonce: out["nonce"],
    qop: out["qop"] ?? "auth",
    algorithm: out["algorithm"] ?? "MD5",
    ...(out["opaque"] !== undefined ? { opaque: out["opaque"] } : {}),
  };
}

function buildAuthHeader(args: {
  user: string;
  password: string;
  challenge: Challenge;
  uri: string;
  method: string;
}): string {
  const algoUpper = args.challenge.algorithm.toUpperCase();
  if (!algoUpper.includes("SHA-256") && !algoUpper.includes("SHA256")) {
    throw new Error(`Unsupported Datum digest algorithm: ${args.challenge.algorithm}`);
  }
  const nc = "00000001";
  const cnonce = randomBytes(8).toString("hex");
  const ha1 = sha256Hex(`${args.user}:${args.challenge.realm}:${args.password}`);
  const ha2 = sha256Hex(`${args.method}:${args.uri}`);
  const response = sha256Hex(
    `${ha1}:${args.challenge.nonce}:${nc}:${cnonce}:${args.challenge.qop}:${ha2}`,
  );
  const parts = [
    `username="${args.user}"`,
    `realm="${args.challenge.realm}"`,
    `nonce="${args.challenge.nonce}"`,
    `uri="${args.uri}"`,
    `algorithm=${args.challenge.algorithm}`,
    `response="${response}"`,
    `qop=${args.challenge.qop}`,
    `nc=${nc}`,
    `cnonce="${cnonce}"`,
  ];
  if (args.challenge.opaque) parts.push(`opaque="${args.challenge.opaque}"`);
  return "Digest " + parts.join(", ");
}

export async function digestFetch(opts: DigestFetchOptions): Promise<Response> {
  const method = opts.method ?? "GET";
  const url = new URL(opts.url);
  const uri = url.pathname + url.search;
  const baseInit: RequestInit = {
    method,
    redirect: "manual",
    headers: { ...(opts.headers ?? {}) },
    ...(opts.signal ? { signal: opts.signal } : {}),
  };

  const first = await fetch(opts.url, baseInit);
  if (first.status !== 401) return first;

  const challengeHdr = first.headers.get("www-authenticate");
  if (!challengeHdr) return first;

  const challenge = parseChallenge(challengeHdr);
  if (!challenge) return first;

  await first.body?.cancel().catch(() => {});

  const auth = buildAuthHeader({
    user: opts.creds.username,
    password: opts.creds.password,
    challenge,
    uri,
    method,
  });

  return fetch(opts.url, {
    ...baseInit,
    headers: { ...(opts.headers ?? {}), authorization: auth },
  });
}
