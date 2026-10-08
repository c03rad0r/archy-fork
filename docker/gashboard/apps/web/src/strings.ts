import { oneIn } from "./format";

// All user-facing copy. Lean into the futility — these are 4 hobby boards
// trying to hit a 1-in-quadrillions lottery. Take the piss with affection.

export const ROASTS: Record<string, string> = {
  nerdqaxe:     "wired together by a guy on Telegram. somehow still hashing.",
  bitaxe:       "single chip, single mum, one in 10²² odds. heart of gold.",
  "avalon-nano-3": "desk ornament with a fan. occasionally impersonates infrastructure.",
  "avalon-mini-3": "the only one of you that pays for itself. don't get a big head.",
  unknown:      "we don't know what this is. we love it anyway.",
};

export const STATUS_LINES = {
  hashing: "hashing",
  stale:   "thinking real hard",
  idle:    "having a lie down",
};

export const LOTTERY_LINES = [
  (oddsPerDay: number) =>
    `today: ${formatPct(oddsPerDay)}. A rounding error wearing a cape.`,
  (oddsPerDay: number) =>
    `expected wait: ${humanYears(1 / Math.max(oddsPerDay, 1e-30) / 365)}. Your descendants' descendants will still be checking the fan noise.`,
  () =>
    `the math says no in every language, including ones we haven't invented yet.`,
  (oddsPerDay: number) =>
    `versus lightning today: ${ratioVsLightning(oddsPerDay)}x. Even the sky has a better business plan.`,
  () =>
    `four little heaters bullying SHA-256 and calling it a retirement strategy.`,
  () =>
    `every share is proof of work. Not useful work. Let's not get carried away.`,
  () =>
    `solo mining: where hope goes to become server logs.`,
  () =>
    `this is not financial advice. It is barely electrical advice.`,
  () =>
    `the fiat system is shaking, but not because of this. This is mostly fan noise.`,
  () =>
    `somewhere a central banker felt nothing at all.`,
  () =>
    `if this finds a block, check the logs, the stars, and whoever touched the thermostat.`,
  () =>
    `the orange pill was supposed to be freedom, not watching a desk fan beg for destiny.`,
  () =>
    `your odds are non-zero, which is how all terrible ideas get funding.`,
  () =>
    `the network difficulty looked at this rig and filed a noise complaint.`,
  () =>
    `proof of work? More like proof of warm.`,
  () =>
    `fiat dies slowly. This block attempt dies every ten minutes.`,
  () =>
    `the hash is strong. The probability is in witness protection.`,
  () =>
    `the IMF has not yet issued a statement about your 0.000000whatever percent chance.`,
  () =>
    `solo mining is just buying a lottery ticket that also heats the hallway.`,
  () =>
    `the block reward remains technically available, like buried treasure on Mars.`,
  () =>
    `every rejected share is a tiny central bank meeting with worse catering.`,
  () =>
    `some call it sovereign mining. Others call it an elaborate way to run a heater.`,
  () =>
    `the machines are awake. The odds are legally dead.`,
  () =>
    `if math had a face, it would be avoiding eye contact.`,
  () =>
    `hash harder, little toaster. The empire will not dismantle itself.`,
  () =>
    `Bitcoin is permissionless, including permission to embarrass yourself electrically.`,
  () =>
    `your miners are attacking fiat with the force of a damp receipt.`,
  () =>
    `the conspiracy is real: the big pools have more ASICs than your shelf.`,
  () =>
    `this is how monetary revolution sounds when it has a 40mm fan.`,
  () =>
    `the Cantillon effect remains undefeated by the cupboard.`,
  () =>
    `the nonce search continues. So does the comedy.`,
  () =>
    `somewhere, Satoshi is either proud or changing the Wi-Fi password.`,
  () =>
    `the chance is small enough to qualify as abstract art.`,
  () =>
    `mining a block today would be less prediction, more paranormal incident.`,
  () =>
    `these rigs are not decentralizing power. They are decentralizing warmth.`,
  () =>
    `the mempool sees you. It is trying not to laugh.`,
  () =>
    `fiat printers go brrr. Your fan goes whrrr. Guess who has better margins.`,
  () =>
    `today's block odds have been forwarded to the Ministry of Delusion.`,
  () =>
    `the network hashrate is a dragon. You brought teaspoons.`,
  () =>
    `this is sovereign self-custody of disappointment.`,
  () =>
    `if a block appears, assume divine intervention or a rounding bug.`,
  () =>
    `monetary debasement is tragic. This rig's odds are slapstick.`,
  () =>
    `your ASICs are yelling at SHA-256 through a letterbox.`,
  () =>
    `the pools have warehouses. You have vibes and a power strip.`,
  () =>
    `a beautiful act of rebellion, if rebellion paid in almost nothing.`,
  () =>
    `the block subsidy is safe from you for now.`,
  () =>
    `hashing like this is how you whisper at a volcano.`,
  () =>
    `the odds are dark, but at least the room isn't cold.`,
  () =>
    `fiat friends buy heaters. You bought statistically complicated heaters.`,
  () =>
    `the nonce lottery has reviewed your application and placed it under a very large pile.`,
  () =>
    `this is not a get-rich-quick scheme. It is barely a get-warm-slowly scheme.`,
  () =>
    `the global banking cartel remains untroubled by this particular fan curve.`,
  () =>
    `somebody said "be your own bank" and now the shelf is making whale noises.`,
  () =>
    `the blockchain is immutable. So is the humiliation.`,
  () =>
    `your chance is smaller than a politician's apology and twice as expensive to heat.`,
  () =>
    `the rig believes. The spreadsheet has concerns.`,
  () =>
    `mining solo is what happens when optimism discovers electricity.`,
  () =>
    `the odds are so low they qualify as a privacy feature.`,
  () =>
    `keep stacking sats. These machines are mostly stacking anecdotes.`,
  () =>
    `the revolution will be decentralized, but apparently not profitable today.`,
];

export const BLOCK_CELEBRATION_LINES = [
  "WAIT. WHAT. CHECK THE LOG. YOU FOUND A — actually let me check that twice.",
  "no but seriously, one of these little boards just won the lottery.",
  "the network has been notified. so have your friends. so has god.",
];

export const STALE_LINES = [
  "hasn't submitted in a while. either thinking really hard or the breaker tripped.",
  "currently meditating. has been for some time.",
  "not dead, just resting. probably.",
];

function formatPct(p: number): string {
  if (!isFinite(p) || p <= 0) return "ε";
  if (p >= 0.01) return `${(p * 100).toFixed(4)}%`;
  return oneIn(p);
}

function humanYears(years: number): string {
  if (!isFinite(years)) return "forever";
  if (years < 1) return `${(years * 365).toFixed(1)} days`;
  if (years < 100) return `${years.toFixed(1)} years`;
  if (years < 1e6) return `${(years / 1e3).toFixed(1)} thousand years`;
  if (years < 1e9) return `${(years / 1e6).toFixed(1)} million years`;
  return `${(years / 1e9).toFixed(1)} billion years`;
}

function ratioVsLightning(oddsPerDay: number): string {
  const lightning = 0.00000028;
  const r = oddsPerDay / lightning;
  if (r < 0.001) return `about ${oneIn(1 / r)} as likely`;
  if (r < 1) return r.toFixed(3);
  if (r < 1000) return r.toFixed(1);
  return `${(r / 1000).toFixed(1)}K`;
}
