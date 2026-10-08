// Narrow packaging changes against the pinned upstream source. Fail instead of
// silently losing the bridge or re-enabling automatic deletion after an update.
const mainPath = '/app/main.ts';
let main = await Deno.readTextFile(mainPath);
const prune = 'const pruneEnabled = config.storage.rules.length > 0 ||\n  config.storage.removeWhenNoOwners;';
if (!main.includes(prune)) throw new Error('Upstream prune integration changed');
main = main.replace(prune, '// Archipelago owns retention: no automatic deletion of published assets.\nconst pruneEnabled = false;');
await Deno.writeTextFile(mainPath, main);
const serverPath = '/app/src/server.ts';
let server = await Deno.readTextFile(serverPath);
const marker = '  app.route("/", buildBlossomRouter(db, storage, config));';
if (!server.includes(marker)) throw new Error('Upstream route integration changed');
server = server.replace(marker, `
  // Uploaded HTML/SVG must never execute with the app gate or signer origin.
  app.use('*', async (c, next) => {
    await next();
    if (/^\\/[a-f0-9]{64}(?:\\.[a-zA-Z0-9]+)?$/.test(c.req.path)) {
      c.header('Content-Security-Policy', "sandbox; default-src 'none'; base-uri 'none'; form-action 'none'");
      c.header('Content-Disposition', 'attachment');
      c.header('X-Content-Type-Options', 'nosniff');
    }
  });
  // Static local UI and the canonical host-managed signer bridge only.
  app.get('/', async c => c.html(await Deno.readTextFile('/app/archy-ui/index.html')));
  app.get('/app.js', async c => c.body(await Deno.readTextFile('/app/archy-ui/app.js'), 200, { 'Content-Type': 'application/javascript', 'Cache-Control': 'no-store' }));
  app.get('/nostr-provider.js', async c => {
    try { return c.body(await Deno.readTextFile('/bridge/nostr-provider.js'), 200, { 'Content-Type': 'application/javascript', 'Cache-Control': 'no-cache, no-store, must-revalidate' }); }
    catch { return c.text('Archipelago signer bridge is not installed', 503); }
  });
  app.get('/healthz', c => c.json({ ready: true, storage: 'local' }));
${marker}`);
await Deno.writeTextFile(serverPath, server);
