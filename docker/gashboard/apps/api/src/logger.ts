import { pino } from "pino";
import { Writable } from "node:stream";
import { config } from "./config.js";

const levelNames: Record<number, string> = {
  10: "trace",
  20: "debug",
  30: "info",
  40: "warn",
  50: "error",
  60: "fatal",
};

let buffered = "";

const logStream = new Writable({
  write(chunk, _encoding, callback) {
    buffered += chunk.toString();
    const lines = buffered.split("\n");
    buffered = lines.pop() ?? "";

    for (const line of lines) {
      if (!line) continue;
      process.stdout.write(`${formatLogLine(line)}\n`);
    }

    callback();
  },
});

export const logger = pino({
  level: config.logLevel,
  redact: {
    paths: [
      'req.headers.authorization',
      'req.headers.cookie',
      '*.password',
      '*.token',
      '*.jwt',
      '*.sig',
    ],
    censor: "[REDACTED]",
  },
  base: { app: "gashboard" },
}, logStream);

function formatLogLine(line: string): string {
  try {
    const record = JSON.parse(line) as Record<string, unknown>;
    const handled = new Set([
      "level",
      "time",
      "pid",
      "hostname",
      "app",
      "req",
      "res",
      "responseTime",
      "port",
      "datum",
      "allowedNpubs",
      "staticDir",
      "url",
      "reason",
      "err",
      "msg",
    ]);
    const parts: string[] = [
      `time=${formatTime(record.time)}`,
      `level=${formatLevel(record.level)}`,
    ];

    pushField(parts, "app", record.app);
    pushRequestFields(parts, record.req);
    pushResponseFields(parts, record.res);
    pushField(parts, "responseTime", record.responseTime);
    pushField(parts, "port", record.port);
    pushField(parts, "datum", record.datum);
    pushField(parts, "allowedNpubs", record.allowedNpubs);
    pushField(parts, "staticDir", record.staticDir);
    pushField(parts, "url", record.url);
    pushField(parts, "reason", record.reason);

    if (record.err && typeof record.err === "object") {
      const err = record.err as Record<string, unknown>;
      pushField(parts, "err", err.message ?? err.type ?? record.err);
    }

    for (const [key, value] of Object.entries(record)) {
      if (handled.has(key)) continue;
      if (!isScalar(value)) continue;
      pushField(parts, key, value);
    }

    pushField(parts, "msg", record.msg);
    return parts.join(" ");
  } catch {
    return line;
  }
}

function formatTime(value: unknown): string {
  if (typeof value === "number") return new Date(value).toISOString();
  if (typeof value === "string") return value;
  return new Date().toISOString();
}

function formatLevel(value: unknown): string {
  if (typeof value === "number") return levelNames[value] ?? String(value);
  if (typeof value === "string") return value;
  return "info";
}

function pushRequestFields(parts: string[], value: unknown): void {
  if (!value || typeof value !== "object") return;
  const req = value as Record<string, unknown>;
  pushField(parts, "method", req.method);
  pushField(parts, "path", req.url);
  pushField(parts, "remote", req.remoteAddress);
}

function pushResponseFields(parts: string[], value: unknown): void {
  if (!value || typeof value !== "object") return;
  const res = value as Record<string, unknown>;
  pushField(parts, "status", res.statusCode);
}

function pushField(parts: string[], key: string, value: unknown): void {
  if (value === undefined || value === null || value === "") return;
  parts.push(`${key}=${formatValue(value)}`);
}

function isScalar(value: unknown): value is string | number | boolean {
  return typeof value === "string" || typeof value === "number" || typeof value === "boolean";
}

function formatValue(value: unknown): string {
  const raw = typeof value === "string" ? value : JSON.stringify(value);
  if (!raw) return '""';
  if (/^[A-Za-z0-9_./:@+-]+$/.test(raw)) return raw;
  return JSON.stringify(raw);
}
