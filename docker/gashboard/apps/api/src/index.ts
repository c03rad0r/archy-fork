import { config } from "./config.js";
import { logger } from "./logger.js";
import { startPoller } from "./datum/poller.js";
import { buildApp } from "./server.js";

startPoller();

buildApp().listen(config.port, () => {
  logger.info(
    {
      port: config.port,
      datum: config.datum.url,
      allowedNpubs: config.nostr.allowedHexPubkeys.length,
    },
    "gashboard api listening",
  );
});
