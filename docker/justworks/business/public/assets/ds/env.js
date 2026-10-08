// Shared environment helpers. In production (justworks.cash) pages behave as deployed;
// anywhere else (local dev) pages default to the demo merchant and local routes.
export const prod=location.hostname.endsWith('justworks.cash');
export const DEMO_NPUB='npub15yfu23xhew6d3d2dqs6ntt5w8myj6p5dj9m0d2trz00au5kv0rnsdhw6za';
export const npubFrom=(search=location.search)=>new URLSearchParams(search).get('npub')?.trim()||'';
