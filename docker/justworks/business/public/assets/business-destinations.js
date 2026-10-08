import { link, base } from './ds/frame.js';
import { prod } from './ds/env.js';
import { moduleQrUrl } from './module-qr.js?v=2';

// The dashboard and the printable master use one destination catalog.
export const freeDefinitions = [
  ['justworks', 'Just Works page', 'Your public merchant page', 'JW'],
  ['tip', 'Tip', 'Bitcoin tips and community wall', '⚡'],
  ['nsite', 'nsite', 'Your site on Nostr', 'NS'],
  ['nostr', 'Nostr profile', 'Your signed identity', 'NP'],
  ['maps', 'Maps', 'Find your place on the map', '⌖'],
  ['gallery', 'Gallery', 'Photos and discovery', '□'],
];
export const businessDefinitions = [
  ['pos', 'POS', 'Run orders and payments', 'Counter view', '↗', false],
  ['order', 'Order', 'Order to table', 'Customer', '＋', true],
  ['pay', 'Pay Bill', 'Settle an open order', 'Customer', '€', true],
  ['payandgo', 'Just Pay', 'Pay a paper receipt', 'Customer', '₿', true],
  ['booking', 'Bookings', 'Tables now or later', 'Customer', '⌁', true, 'book'],
  ['screen', 'Screen', 'Merchant media display', '4K display', '▣', true],
  ['stats', 'Stats', 'Audience, actions and revenue', 'Merchant view', '↗', true, 'business'],
  ['events', 'Events', 'Merchant Nostr events', 'Coming soon', '◌', true],
];

export function businessUrl(npub, sheet = false) {
  const path = sheet ? '/qr-sheet' : prod ? '/' : '/dashboard';
  return `${prod ? 'https://business.justworks.cash' : location.origin + base}${path}?npub=${encodeURIComponent(npub)}`;
}

export function moduleDestination(definition, data, npub) {
  const [id, , , , , configurable, target = id] = definition;
  const state = data.modules?.[id] || {};
  const paymentSetup = ['pay', 'payandgo'].includes(id) && !data.payment_ready;
  const disabled = id === 'events' || state.status === 'coming-soon' || paymentSetup;
  const url = paymentSetup
    ? `${base}/profile?npub=${encodeURIComponent(npub)}#lightning`
    : id === 'stats'
      ? `${prod ? 'https://business.justworks.cash' : base}/stats?npub=${encodeURIComponent(npub)}`
      : prod ? `https://${target}.justworks.cash/?npub=${encodeURIComponent(npub)}` : link(target);
  return {state, paymentSetup, disabled, url,
    qrUrl: disabled ? '' : moduleQrUrl(url, npub),
    available: !disabled && (!configurable || !!state.enabled)};
}

export function sheetDestinations(data, npub) {
  return [
    {id: 'business', title: 'Business dashboard', url: businessUrl(npub)},
    ...freeDefinitions.filter(([id]) => data.links?.[id] && data.links[id] !== '#')
      .map(([id, title]) => ({id, title, url: data.links[id]})),
    ...businessDefinitions.flatMap(definition => {
      const destination = moduleDestination(definition, data, npub);
      return destination.available ? [{id: definition[0], title: definition[1], url: destination.qrUrl}] : [];
    }),
  ];
}

// Verify the Bech32 checksum and the 32-byte public-key payload, without a CDN.
export function validNpub(value) {
  if (!/^npub1[023456789acdefghjklmnpqrstuvwxyz]{58}$/.test(value || '')) return false;
  const alphabet = 'qpzry9x8gf2tvdw0s3jn54khce6mua7l';
  const words = [...value.slice(5)].map(char => alphabet.indexOf(char));
  let checksum = 1;
  const expanded = [...'npub'].map(char => char.charCodeAt(0) >> 5)
    .concat(0, [...'npub'].map(char => char.charCodeAt(0) & 31), words);
  const generators = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
  for (const word of expanded) {
    const top = checksum >>> 25;
    checksum = ((checksum & 0x1ffffff) << 5) ^ word;
    for (let bit = 0; bit < 5; bit++) if ((top >>> bit) & 1) checksum ^= generators[bit];
  }
  return checksum === 1 && (words[51] & 15) === 0;
}
