import express from "express";
import helmet from "helmet";
import cors from "cors";
import { pinoHttp } from "pino-http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import fs from "node:fs";

import { accessRouter } from "./access/routes.js";
import { config } from "./config.js";
import { logger } from "./logger.js";
import { authRouter } from "./auth/routes.js";
import { chatRouter } from "./chat/routes.js";
import { datumRouter } from "./datum/routes.js";
import { errorHandler } from "./errors.js";

export function buildApp() {
  const app = express();
  app.disable("x-powered-by");
  app.set("trust proxy", 1);

  app.use(
    helmet({
      contentSecurityPolicy: {
        useDefaults: true,
        directives: {
          "default-src": ["'self'"],
          "script-src": ["'self'"],
          "style-src": ["'self'", "'unsafe-inline'"],
          "img-src": ["'self'", "data:", "https:"],
          "connect-src": ["'self'", "wss://relay.primal.net"],
          "font-src": ["'self'", "data:"],
          "manifest-src": ["'self'"],
          "worker-src": ["'self'"],
          "frame-ancestors": ["'none'"],
          // The canonical provider validates the same-host dashboard broker.
          "frame-src": config.providerPath ? ["'self'", "http:", "https:"] : ["'self'"],
          "upgrade-insecure-requests": null,
        },
      },
      crossOriginEmbedderPolicy: false,
      crossOriginOpenerPolicy: false,
      originAgentCluster: false,
    }),
  );

  if (config.corsOrigin) {
    app.use(cors({ origin: config.corsOrigin, credentials: false }));
  }

  app.use(express.json({ limit: "32kb" }));
  app.use(
    pinoHttp({
      logger,
      autoLogging: {
        ignore: (req) =>
          req.url === "/healthz" ||
          req.url === "/favicon.ico" ||
          req.url.startsWith("/assets/"),
      },
    }),
  );

  app.get("/healthz", (_req, res) => {
    res.json({ ok: true });
  });
  app.use("/api/auth", authRouter);
  app.use("/api/access", accessRouter);
  app.use("/api/chat", chatRouter);
  app.use("/api/datum", datumRouter);

  app.get("/nostr-provider.js", (_req, res) => {
    res.setHeader("Cache-Control", "no-cache, no-store");
    const provider = config.providerPath && (fs.existsSync(config.providerPath) ? config.providerPath : "/app/nostr-provider.js");
    if (!provider || !fs.existsSync(provider)) {
      res.status(503).type("application/javascript").send("/* Archipelago signer is not installed. */");
      return;
    }
    res.type("application/javascript").send("if (!window.nostr) {\n" + fs.readFileSync(provider, "utf8") + "\n}");
  });
  // Unknown API paths must never become a successful HTML SPA response.
  app.use("/api", (_req, res) => { res.status(404).json({ error: { code: "not_found" } }); });

  const staticDir = config.staticDir
    ? config.staticDir
    : resolveDefaultStaticDir();

  if (staticDir && fs.existsSync(staticDir)) {
    logger.info({ staticDir }, "serving web assets");
    app.use(
      express.static(staticDir, {
        index: false,
        maxAge: "1h",
        setHeaders: (res, filePath) => {
          if (filePath.endsWith("sw.js") || filePath.endsWith("manifest.webmanifest")) {
            res.setHeader("Cache-Control", "no-cache");
          }
        },
      }),
    );
    app.get(/.*/, (_req, res, next) => {
      const indexFile = path.join(staticDir, "index.html");
      if (!fs.existsSync(indexFile)) return next();
      res.setHeader("Cache-Control", "no-cache");
      if (config.providerPath) {
        const html = fs.readFileSync(indexFile, "utf8");
        res.type("html").send(html.replace("</head>",
          '<script src="/nostr-provider.js?v=archipelago-v1" data-app-id="gashboard" data-no-nip98></script></head>'));
      } else {
        res.sendFile(indexFile);
      }
    });
  } else {
    logger.warn({ staticDir }, "web assets directory not found");
  }

  app.use(errorHandler);
  return app;
}

function resolveDefaultStaticDir(): string {
  const here = path.dirname(fileURLToPath(import.meta.url));
  return path.resolve(here, "../../web/dist");
}
