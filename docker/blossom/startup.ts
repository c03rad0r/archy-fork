// Public profile keys only; no private keys or dashboard credentials enter this
// container. Changes to the node's identity allowlist take effect on restart.
const keys = (Deno.env.get('ARCHY_BLOSSOM_PUBKEYS') ?? '').split(',').filter(Boolean);
if (!keys.length || keys.some(k => !/^[a-f0-9]{64}$/.test(k))) throw new Error('Create a profile identity in Archipelago before starting Blossom');
const config = JSON.parse(await Deno.readTextFile('/config/config.json'));
config.host = '0.0.0.0'; config.port = 3000;
config.database = { path: '/data/sqlite.db' };
config.storage = { backend: 'local', local: { dir: '/data/blobs' }, removeWhenNoOwners: false, rules: [{ type: '*', expiration: '100 years', pubkeys: keys }] };
config.upload = { enabled: true, requireAuth: true, requirePubkeyInRule: true, maxSize: 16777216, workers: 1 };
config.delete = { requireAuth: true };
config.list = { enabled: true, requireAuth: true, allowListOthers: false };
config.mirror = { enabled: false, requireAuth: true };
config.media = { enabled: false, requireAuth: true, requirePubkeyInRule: true };
config.report = { enabled: false };
config.landing = { enabled: false };
config.dashboard = { enabled: false };
// No URL/host facts are baked into the UI. BUD-11 uses the actual request host
// unless the operator explicitly configured the server's canonical domain.
await Deno.writeTextFile('/tmp/blossom-config.json', JSON.stringify(config));
Deno.args.splice(0, Deno.args.length, '/tmp/blossom-config.json');
await import('./main.ts');
