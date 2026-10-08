export type ChatSticker = {
  label: string;
  text: string;
  kind?: "sticker" | "gif";
  image?: string;
};

export const STICKER_PREFIX = "[gashboard sticker]";
const GIF_MARKER = "[gif]";
const IMAGE_MARKER = "[img:";

export const CHAT_STICKERS: ChatSticker[] = [
  { label: "fiat heater", text: "FIAT HEATER DETECTED: warm room, cold wallet." },
  { label: "no hashes", text: "NO HASHES. JUST GHEI. just fiat and a pension." },
  { label: "cope block", text: "BLOCK FOUND: not by us, obviously. carry on pretending." },
  { label: "desk miner", text: "DESK MINER ENERGY: tiny fan, enormous delusion." },
  { label: "pool bully", text: "BIG POOL ENTERED CHAT: scaled up to not shame the children." },
  { label: "paper hands", text: "PAPER HANDS, PLASTIC HEATER, ZERO SATS." },
  { label: "nonce cult", text: "NONCE CULT MEETING: attendance low, conviction insane." },
  { label: "fiat friend", text: "YOUR FIAT FRIEND THINKS THE HEATER IS AN INVESTMENT." },
  { label: "warm tonight", text: "SOMEONE'S GETTING WARMED TONIGHT. maybe not paid." },
  { label: "difficulty", text: "DIFFICULTY ADJUSTMENT: reality filed another complaint." },
  { label: "best calc", text: "BEST CALC: almost important, spiritually devastating." },
  { label: "hash high", text: "HASHRATE UP. ODDS STILL NEED A TELESCOPE." },
  { label: "pension", text: "JUST FIAT AND A PENSION: the saddest proof-of-warmth." },
  { label: "sovereign", text: "SOVEREIGN MINER: no block, no boss, no apology." },
  { label: "amber", text: "SIGNED BY A REAL SIGNER, UNLIKE YOUR BOOMER'S DIRECT DEBIT." },
  { label: "mempool", text: "MEMPOOL SAYS NO. DASHBOARD SAYS LOL." },
  { label: "reject", text: "REJECTED SHARE: even the server had standards." },
  { label: "bigpapa", text: "BIGPAPA IS WATCHING. probably collecting fuck all." },
  { label: "lottery", text: "SOLO LOTTERY TICKET PURCHASED WITH ELECTRICITY AND DENIAL." },
  { label: "fiat people", text: "FIAT PEOPLE SEE A HEATER. MINERS SEE A COWARD." },
  { label: "gif cope", kind: "gif", text: "LIVE FOOTAGE OF ME PRETENDING THE ODDS ARE FINE." },
  { label: "gif panic", kind: "gif", text: "THE HASHRATE MOVED 0.01 TH/S AND I HAVE BECOME UNBEARABLE." },
  { label: "gif refresh", kind: "gif", text: "REFRESHING THE DASHBOARD LIKE IT OWES ME MONEY." },
  { label: "gif chart", kind: "gif", text: "GRAPH GOES UP. BRAIN GOES OFF." },
  { label: "gif no block", kind: "gif", text: "NO BLOCK? MUST BE A CONSPIRACY AGAINST MY USB PORT." },
  { label: "gif office", kind: "gif", text: "CORPORATE NEEDS YOU TO FIND THE DIFFERENCE: HEATER / MINER." },
  { label: "gif maths", kind: "gif", text: "ME EXPLAINING 1 IN 2 BILLION LIKE IT IS A PLAN." },
  { label: "gif deploy", kind: "gif", text: "DEPLOYED TO PRODUCTION. MAY GOD REVIEW THE DIFF." },
  { label: "gif smug", kind: "gif", text: "I MINE SOLO, WHICH IS HOW YOU KNOW I AM DIFFICULT AT DINNER." },
  { label: "gif invoice", kind: "gif", text: "ELECTRIC BILL ARRIVED. IT IS NOW A WHITEPAPER." },
  { label: "ngmi heater", text: "NGMI: NOT GENERATING MINING INCOME." },
  { label: "printer money", text: "FIAT PRINTER WENT BRRR. MINER WENT PLEASE NOTICE ME." },
  { label: "normie mode", text: "NORMIE MODE: PAYING FOR HEAT WITHOUT EVEN ATTEMPTING A BLOCK." },
  { label: "bullish fan", text: "LOUD FAN, QUIET BALANCE, MAXIMUM CONVICTION." },
  { label: "chart crime", text: "THIS CHART IS A CRIME SCENE WITH AXES." },
  { label: "share cope", text: "ACCEPTED SHARE: A PARTICIPATION TROPHY WITH VOLTAGE." },
  { label: "stack sats", text: "STACKING SATS? NO. STACKING RECEIPTS FROM THE POWER COMPANY." },
  { label: "main character", text: "MAIN CHARACTER ENERGY, BACKGROUND HASHRATE." },
  { label: "block soon", text: "BLOCK SOON. SOURCE: THE SAME PART OF MY BRAIN THAT BUYS TOPS." },
  { label: "heater max", text: "HEATER ON MAX. HASHES ON VACATION." },
  {
    label: "anon miner",
    text: "ANON DISCOVERS SOLO MINING AND IMMEDIATELY BECOMES A PROBLEM.",
    image: "/stickers/anon-miner.svg",
  },
  {
    label: "fiat npc",
    text: "I JUST USE A NORMAL HEATER BECAUSE THE BANK SAID IT WAS FINE.",
    image: "/stickers/fiat-npc.svg",
  },
  {
    label: "chad hash",
    text: "LOUD, WARM, WRONG, AND STILL MORE SOVEREIGN THAN YOUR SAVINGS ACCOUNT.",
    image: "/stickers/chad-hash.svg",
  },
  {
    label: "money printer",
    text: "CENTRAL BANK MONETARY POLICY, ARTIST'S IMPRESSION.",
    image: "/stickers/money-printer.svg",
  },
  {
    label: "block denied",
    text: "THE NETWORK REVIEWED YOUR NONCE AND CHOSE VIOLENCE.",
    image: "/stickers/block-denied.svg",
  },
  {
    label: "boomer unit",
    text: "LEGACY HEATER ACHIEVES ZERO HASHES PER RETIREMENT.",
    image: "/stickers/boomer-unit.svg",
  },
  {
    label: "chart crime",
    text: "TECHNICAL ANALYSIS CONDUCTED BY A FAN WITH A WALLET.",
    image: "/stickers/chart-crime.svg",
  },
  {
    label: "odds wizard",
    text: "I HAVE CONSULTED THE ODDS AND THEY SAID LOL.",
    image: "/stickers/odds-wizard.svg",
  },
  {
    label: "gif fan panic",
    kind: "gif",
    text: "LIVE FAN FOOTAGE AFTER HASHRATE MOVES BY A DECIMAL.",
    image: "/stickers/fan-panic.svg",
  },
  {
    label: "gif printer",
    kind: "gif",
    text: "FIAT POLICY IN ONE LOOP.",
    image: "/stickers/printer-loop.svg",
  },
  {
    label: "gif block hopium",
    kind: "gif",
    text: "SOLO MINER WATCHING EVERY NEW BLOCK LIKE IT OWES HIM MONEY.",
    image: "/stickers/block-hopium.svg",
  },
  {
    label: "gif heat death",
    kind: "gif",
    text: "THE ROOM IS WARMER THAN THE INVESTMENT THESIS.",
    image: "/stickers/heat-death.svg",
  },
];

export function stickerContent(sticker: ChatSticker): string {
  const image = sticker.image ? `${IMAGE_MARKER}${sticker.image}] ` : "";
  return `${STICKER_PREFIX} ${image}${sticker.kind === "gif" ? `${GIF_MARKER} ` : ""}${sticker.text}`;
}

export function unsticker(
  content: string,
): { text: string; kind: "sticker" | "gif"; image?: string } | null {
  if (!content.startsWith(`${STICKER_PREFIX} `)) return null;
  let body = content.slice(STICKER_PREFIX.length + 1);
  let image: string | undefined;
  if (body.startsWith(IMAGE_MARKER)) {
    const end = body.indexOf("] ");
    if (end > IMAGE_MARKER.length) {
      image = body.slice(IMAGE_MARKER.length, end);
      body = body.slice(end + 2);
    }
  }
  if (body.startsWith(`${GIF_MARKER} `)) {
    const parsed = { text: body.slice(GIF_MARKER.length + 1), kind: "gif" as const };
    return image ? { ...parsed, image } : parsed;
  }
  const parsed = { text: body, kind: "sticker" as const };
  return image ? { ...parsed, image } : parsed;
}
