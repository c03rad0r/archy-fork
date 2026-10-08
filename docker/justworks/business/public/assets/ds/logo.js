/*
 * The Just Works mark.
 *
 * One continuous line, drawn as a single closed path, with no beginning and no
 * end. As it travels it settles through three states: a plain loop, a section
 * squared into a card outline (payments), and a section drawn out into a
 * speech bubble (social). The resting state carries a pointer notch on the
 * left lobe (website), so the mark reads correctly even with motion disabled.
 *
 * The path is the ONLY thing that changes. Stroke, gradient, size and caps are
 * styled in components.css from tokens, so the mark follows the theme.
 *
 * Rendered from here rather than written into each page, so the topbar, the
 * footer and /design all draw the same mark from one source.
 */

/*
 * All four states share the same anchor budget and the same left-lobe opening,
 * so the morph travels rather than jumps. Authored on a 56x28 field with the
 * crossing point at the centre.
 */
export const LOGO_STATES = {
  /* The loop at rest, with the pointer notch on the left lobe. */
  loop: "M13.5 14C13.5 7.5 20 7 27 14C34 21 41 20.5 41.5 14C42 7.5 34.5 7 27 14C20.5 20.2 15 21 13.5 14Z",
  /* Right lobe squared into a card. */
  card: "M13.5 14C13.5 7.5 20 7 27 14C29.5 16.5 30 19.5 33 19.5L41 19.5C43 19.5 44 18.5 44 16.5L44 11.5C44 9.5 43 8.5 41 8.5L33 8.5C30 8.5 29.5 11.5 27 14C20.5 20.2 15 21 13.5 14Z",
  /* Right lobe drawn out into a speech bubble, tail dropping below. */
  bubble: "M13.5 14C13.5 7.5 20 7 27 14C29 16 29 18.5 30.5 19.5C31.6 20.3 31 23.4 32.6 22.3C34 21.3 34.6 19.5 36.6 19.5L39 19.5C42 19.5 44 17 44 14C44 11 42 8.5 39 8.5L33 8.5C29.5 8.5 29 11.5 27 14C20.5 20.2 15 21 13.5 14Z",
  /* Lab 07: the right lobe opens into a round doorway. The resting mark for
     now, per the direction of 2026-08-18, until the lab work settles. */
  portal: "M13.5 14C13.5 7.5 20 7 27 14C30.5 17.5 34.5 19.5 37.5 18C35.5 15.5 35 13.5 36.5 12C38 10.5 41 10.5 42 12.5C43 14.5 42 16 40.5 17.5C43 17.5 44 14.5 43 11.5C41.5 7.5 34 6.5 30 10.5C28.5 12 28 13 27 14C20.5 20.2 15 21 13.5 14Z",
  /* Left lobe pulled into a cursor arrow. */
  cursor: "M13.5 14C10.6 8.2 9.8 4.6 11.6 4.2C13.4 3.8 20.6 9.6 27 14C34 21 41 20.5 41.5 14C42 7.5 34.5 7 27 14C23 16.6 18.6 21.6 17.1 22.6C15.6 23.6 14.6 19.8 13.5 14Z"
};

/*
 * The lab: further single-line ideas, same method as LOGO_STATES — one
 * continuous closed path on the 56x28 field, each working a different motif
 * into the loop's travel. Rendered in /design for comparison; not part of the
 * live cycle. Keep adding here rather than replacing.
 */
export const LOGO_LAB = {
  /* Payments and Bitcoin. */
  "sat-symbol": "M13.5 14C13.5 7.5 20 7 27 14C30 17 30 19 34 19L40 19M34 9L40 9M31 14L43 14C43 14 43.5 10 41.5 8.5C38 6.5 31 9.5 27 14C20.5 20.2 15 21 13.5 14Z",
  "coin-edge": "M13.5 14C13.5 7.5 20 7 27 14C30 17 32 18.5 35 18.5C39 18.5 42.5 16.5 42.5 13.5C42.5 10.5 39 8.5 35 8.5C32 8.5 30 11 27 14C24 17 21 19.5 17.5 19.5C14.5 19.5 13.5 17 13.5 14Z",
  "zap-bolt": "M13.5 14C13.5 7.5 20 7 27 14C30 17 33 18 35 15.5L32 15.5L38 8.5L36 13L39.5 13C41.5 13 42 10 40 8.5C36.5 6 30.5 10 27 14C20.5 20.2 15 21 13.5 14Z",
  "lightning-path": "M13.5 14C13.5 7.5 20 7 27 14C34 21 41 20.5 41.5 14C42 7.5 34.5 7 27 14C25 16 23.5 14.5 22 16.5C20.5 18.5 19 16.5 17.5 18.5C15.5 21 13.5 17.5 13.5 14Z",
  "banknote": "M13.5 14C13.5 7.5 20 7 27 14C29.5 16.5 30 18.5 33 18.5L42 18.5C43.5 18.5 44 18 44 16.5L44 11.5C44 10 43.5 9.5 42 9.5L33 9.5C30 9.5 29.5 11.5 27 14C20.5 20.2 15 21 13.5 14ZM38.5 11.7A2.3 2.3 0 1 0 38.5 16.3A2.3 2.3 0 1 0 38.5 11.7Z",
  /* Identity and keys. */
  "key-bit": "M13.5 14C13.5 7.5 20 7 27 14C30 17 32 18 35 18L37 18L37 20L39 20L39 18L41 18L41 20.5L43.5 20.5L43.5 16C43.5 16 42 15.5 40 13C37.5 10 34 7.5 31 9.5C28.5 11 28.5 12.5 27 14C20.5 20.2 15 21 13.5 14Z",
  "portal": "M13.5 14C13.5 7.5 20 7 27 14C30.5 17.5 34.5 19.5 37.5 18C35.5 15.5 35 13.5 36.5 12C38 10.5 41 10.5 42 12.5C43 14.5 42 16 40.5 17.5C43 17.5 44 14.5 43 11.5C41.5 7.5 34 6.5 30 10.5C28.5 12 28 13 27 14C20.5 20.2 15 21 13.5 14Z",
  "signature": "M13.5 14C13.5 7.5 20 7 27 14C29 16 30.5 17 32 15.5C33.5 14 32 12 33.5 10.5C35 9 36.5 11 38 9.5C39.5 8 41 9.5 42.5 8C43.5 7 44 8.5 43 10C41 13 37 19 33 19C30 19 29 16 27 14C20.5 20.2 15 21 13.5 14Z",
  /* Web and social. */
  "at-sign": "M13.5 14C13.5 7.5 20 7 27 14C29.5 16.5 31.5 19 35.5 19C39.5 19 43 16.5 43 13C43 9.5 40.5 8 38 8C35.5 8 33.5 9.5 33.5 12C33.5 14.5 35.5 16 37.5 15.5C39.5 15 40 13 39.5 11.5C39 13.5 38 14.5 36.5 14C35 13.5 35.5 10.5 38 10.5C40 10.5 41 12 41 13.5C41 15.5 39 17 36.5 17C33.5 17 31 15 29.5 12.5C28.5 11 28 12.8 27 14C20.5 20.2 15 21 13.5 14Z",
  "pin-drop": "M13.5 14C13.5 7.5 20 7 27 14C30 17 33.5 20.5 35.5 22C37.5 20.5 41 16.5 41 12.5C41 9 38.5 7 35.5 7C32.5 7 31 9 29.5 11.5C28.7 12.8 28 13 27 14C20.5 20.2 15 21 13.5 14ZM35.5 10.5A2 2 0 1 0 35.5 14.5A2 2 0 1 0 35.5 10.5Z",
  "heart-zap": "M13.5 14C13.5 7.5 20 7 27 14C29 16 31 18.5 34.5 21C38 18.5 43 14.5 43 11C43 8.5 41 7.5 39.5 7.5C37.5 7.5 36.5 9 34.5 11C32.5 9 31.5 7.5 29.5 7.5C28.5 7.5 27.5 8 27 9.5C26 12 24 16 20 18.5C16.5 20.5 13.5 17.5 13.5 14Z",
  "follow-plus": "M13.5 14C13.5 7.5 20 7 27 14C30 17 32 18 34.5 18L34.5 21L37.5 21L37.5 18L40.5 18L40.5 15L37.5 15L37.5 12L34.5 12L34.5 15C32 15 30 12 27 14C20.5 20.2 15 21 13.5 14Z",
  /* Continuity and craft. */
  "mobius": "M13.5 14C13.5 7.5 21 6 27 14C33 22 40.5 20.5 41.5 14C42.5 7 35 6.5 29 12C24 16.5 18 22 14.5 18C12.5 15.8 13 15 13.5 14Z",
  "knot": "M13.5 14C13.5 8 19 6.5 24 10.5C27 13 25 17.5 28.5 19C32 20.5 35 18 34.5 14.5C34 11 30 10 27 14C24.5 17.5 21 21.5 16.5 20C13.5 19 13.5 16 13.5 14Z",
  "orbit-dot": "M13.5 14C13.5 7.5 20 7 27 14C34 21 41 20.5 41.5 14C42 7.5 34.5 7 27 14C20.5 20.2 15 21 13.5 14ZM45 8A1.8 1.8 0 1 0 45 11.6A1.8 1.8 0 1 0 45 8Z",
  "spark-tail": "M13.5 14C13.5 7.5 20 7 27 14C33 20 39 21 41 17C42 15 41 13.5 43 12.5C44.5 11.8 45 10 44 9C42.5 7.5 40 8 38.5 9.5C34.5 6.5 30 10.5 27 14C20.5 20.2 15 21 13.5 14Z",
  "wave-crest": "M13.5 14C13.5 7.5 20 7 27 14C30 17 32 18.5 34 16.5C36 14.5 34.5 12 36.5 10C38.5 8 41.5 8.5 42.5 11C43.5 13.5 42 16 39.5 17.5C42.5 17.5 44.5 14.5 43.5 11C42 6.5 36 6.5 33 10C31 12.3 28.7 12.3 27 14C20.5 20.2 15 21 13.5 14Z",
  "double-cross": "M13.5 14C13.5 8 19 6.5 24.5 11C27 13 29 15 31.5 17C34.5 19.5 38.5 20 41 17.5C43.5 15 42.5 11 39.5 9C36.5 7 33 8.5 30.5 11C28 13.5 25 16.5 21 18.5C16.5 20.5 13.5 17.5 13.5 14Z",
  "qr-corner": "M13.5 14C13.5 7.5 20 7 27 14C29.5 16.5 30.5 18.5 33.5 18.5L43.5 18.5L43.5 8.5L33.5 8.5C30.5 8.5 29.5 11.5 27 14C20.5 20.2 15 21 13.5 14ZM36 11L41 11L41 16L36 16Z",
  "storefront": "M13.5 14C13.5 7.5 20 7 27 14C29 16 29.5 18.5 32 18.5L42 18.5L42 12.5C43.5 12 44 10.5 43 9.5C42 8.3 40 8.5 36.5 8.5C33 8.5 31.5 8.5 30.5 9.5C29.5 10.5 30 12 31.5 12.5L31.5 15C31 13 29 12 27 14C20.5 20.2 15 21 13.5 14Z",
  /* Batch two — 2026-08-18. */
  "relay-waves": "M13.5 14C13.5 7.5 20 7 27 14C30 17 32 18.5 34 17C32.5 15.5 32.5 12.5 34 11C35 13 37 13.5 38 12C36.5 10.5 37 8.5 39 8C41.5 7.4 43.5 9.5 43.5 12C43.5 16.5 38 20 33 17.5C30.5 16.2 29 15.8 27 14C20.5 20.2 15 21 13.5 14Z",
  "antenna-mast": "M13.5 14C13.5 7.5 20 7 27 14C29.5 16.5 32 18 35 18L36.5 18L36.5 9.5C35 9 34.5 7 36 6C37.5 5 39.5 6 39.5 7.5C39.5 8.5 39 9.2 38 9.5L38 18L40 18C42.5 18 43.5 15.5 42.5 13C44.5 14 44.5 17.5 42 19C38.5 21 32 19 29 15.5C28.2 14.6 27.7 14.6 27 14C20.5 20.2 15 21 13.5 14Z",
  "chain-link": "M13.5 14C13.5 7.5 20 7 27 14C29 16 30 17.5 32.5 17.5C34.5 17.5 36 16 36 14C36 12 34.5 10.5 32.5 10.5C31 10.5 30 11.5 29 13C30 10 32.5 8 35.5 9C38.5 10 39.5 13.5 38 16C39.5 17 42 16.5 43 14.5C43.5 17.5 40 19.5 37 18.5C33.5 17.4 31 14 29.5 12.5C28.6 13.1 27.8 13.4 27 14C20.5 20.2 15 21 13.5 14Z",
  "menu-book": "M13.5 14C13.5 7.5 20 7 27 14C29 16 29.5 18.5 32 18.5L36.5 18.5L36.5 9.5C34.5 8.5 31.5 9 30 10.5C29.2 11.5 28.2 12.6 27 14C20.5 20.2 15 21 13.5 14ZM38 18.5L42.5 18.5C43.5 18.5 44 18 44 17L44 10.5C44 9.5 43.5 9 42.5 9L38 9Z",
  "cup-steam": "M13.5 14C13.5 7.5 20 7 27 14C29 16 29.5 18 32 18.5C34.5 19 37.5 19 39.5 18C41.5 17 42 15 42 13L33 13C33 11.5 33.5 10 35 9C34 7.5 34.5 6 36 5.5C35.5 7 36.5 7.5 36.5 9L42 9C43.5 9 44 10 43.5 11.5L42.9 13.4C44.5 13.6 45 15 44 16C43.4 16.6 42.5 16.6 41.8 16.2C40.5 18.8 36.5 20.5 32.5 19.5C29.5 18.7 28.5 15.6 27 14C20.5 20.2 15 21 13.5 14Z",
  "receipt": "M13.5 14C13.5 7.5 20 7 27 14C29 16 29.5 18.5 32 19L32 20.5L33.8 19.3L35.6 20.5L37.4 19.3L39.2 20.5L41 19.3L42.5 20.3L42.5 9.5C42.5 8.5 42 8 41 8L34 8C31 8 29.5 11.5 27 14C20.5 20.2 15 21 13.5 14ZM35 12L40 12M35 15L40 15",
  "price-tag": "M13.5 14C13.5 7.5 20 7 27 14C29.5 16.5 32 19.5 34.5 20.5C35.5 21 36.5 20.8 37.5 19.8L42.8 14.5C43.8 13.5 43.8 12.2 42.8 11.2L38.5 7.5C36 7 32 7.5 30 10C28.8 11.5 28 13 27 14C20.5 20.2 15 21 13.5 14ZM39 11A1.5 1.5 0 1 0 39 14A1.5 1.5 0 1 0 39 11Z",
  "calendar-day": "M13.5 14C13.5 7.5 20 7 27 14C29 16 29.5 18.5 32 18.5L42 18.5C43.5 18.5 44 18 44 16.5L44 10.5C44 9 43.5 8.5 42 8.5L40.5 8.5L40.5 7L38.5 7L38.5 8.5L35 8.5L35 7L33 7L33 8.7C30.8 9.6 29.2 11.6 27 14C20.5 20.2 15 21 13.5 14ZM32 12L44 12",
  "clock-open": "M13.5 14C13.5 7.5 20 7 27 14C29.5 16.5 31.5 18.5 35 18.5C39 18.5 42 16.5 42 13.5C42 10.5 39.5 8.5 36.5 8.5C33.5 8.5 31.5 10.5 29.5 12.5C28.6 13.3 27.9 13.2 27 14C20.5 20.2 15 21 13.5 14ZM36.5 10.5L36.5 13.5L39 15",
  "compass-needle": "M13.5 14C13.5 7.5 20 7 27 14C34 21 41 20.5 41.5 14C41.9 9 37.5 7.2 33 10L36.5 12.5L31 17L29.5 12.2C28.6 12.9 27.8 13.4 27 14C20.5 20.2 15 21 13.5 14Z",
  "north-arrow": "M13.5 14C13.5 7.5 20 7 27 14C30 17 32.5 19.5 35.5 21.5L37.5 12.5L42 14.5C41 10 36.5 6.5 32 8.5C29.8 9.6 28.5 12 27 14C20.5 20.2 15 21 13.5 14Z",
  "camera-eye": "M13.5 14C13.5 7.5 20 7 27 14C29 16 29.5 18.5 32.5 18.5L41 18.5C42.5 18.5 43.5 17.5 43.5 16L43.5 11C43.5 9.5 42.5 8.5 41 8.5L39.5 8.5L38.5 7L35.5 7L34.5 8.5C31.5 8.5 29.5 11.5 27 14C20.5 20.2 15 21 13.5 14ZM37 10.5A3 3 0 1 0 37 16.5A3 3 0 1 0 37 10.5Z",
  "mic-live": "M13.5 14C13.5 7.5 20 7 27 14C29 16 30 18 32.5 18.5L34.5 18.5L34.5 16.8C32.5 16.3 31.5 14.8 31.5 13L33 13C33 14.5 34.5 15.5 36 15.5C37.5 15.5 39 14.5 39 13L40.5 13C40.5 14.8 39.5 16.3 37.5 16.8L37.5 18.5C40 18.5 42 17 42.5 14.5C44 15.5 43.5 18 41 19.3C37.5 21 31 19.5 29 15.8C28.3 14.9 27.7 14.6 27 14C20.5 20.2 15 21 13.5 14ZM34 9.5C34 8 35 7 36 7C37 7 38 8 38 9.5L38 12C38 13.5 37 14 36 14C35 14 34 13.5 34 12Z",
  "note-beam": "M13.5 14C13.5 7.5 20 7 27 14C29.5 16.5 31 18 33 17.5L33 10L41 8L41 15.5C42.5 15.8 43.5 17 43 18.3C42.3 19.8 40 20 39.3 18.5C38.9 17.6 39.3 16.6 40.3 16.2L40.3 10.5L33.7 12.2L33.7 18C34.2 19.8 32 21 30.3 20C28.8 19 28.8 17 30.3 16.2C29 15.5 28 14.8 27 14C20.5 20.2 15 21 13.5 14Z",
  "ticket-stub": "M13.5 14C13.5 7.5 20 7 27 14C29 16 29.5 18.5 32 18.5L42 18.5C43 18.5 43.5 18 43.5 17C42.5 17 41.8 16.2 41.8 15.2C41.8 14.2 42.5 13.5 43.5 13.5L43.5 10C43.5 9 43 8.5 42 8.5L33.5 8.5C30.5 8.5 29.5 11.5 27 14C20.5 20.2 15 21 13.5 14ZM37.5 8.5L37.5 18.5",
  "bell-ring": "M13.5 14C13.5 7.5 20 7 27 14C29 16 30 17.5 32 18L32 19C32 20 32.8 20.8 33.8 20.8C34.8 20.8 35.6 20 35.6 19L41.5 19C41.5 17.5 40 17 40 14.5C40 11 38 8.5 35 8.5C32 8.5 30.8 11 29.3 12.7C28.5 13.4 27.7 13.4 27 14C20.5 20.2 15 21 13.5 14ZM30 19L41.5 19",
  "star-review": "M13.5 14C13.5 7.5 20 7 27 14C29.5 16.5 31.5 18.5 33.5 20L35.5 15.5L31.5 12.5L36.5 12.5L38 8L39.5 12.5L44.5 12.5L40.5 15.5L42 20.2L38 17.4C36 19 33 20.7 30 18C28.9 17 28 15 27 14C20.5 20.2 15 21 13.5 14Z",
  "check-badge": "M13.5 14C13.5 7.5 20 7 27 14C29 16 30 17.8 32.3 18.4C32.1 19.4 32.8 20.3 33.9 20.3C34.9 20.3 35.6 19.5 35.5 18.5C37 18.2 38.4 17.3 39.2 16C40.2 16.4 41.4 16 41.9 15C42.4 14 42 12.9 41.1 12.4C41.2 10.8 40.5 9.3 39.2 8.4C36.4 6.5 32.5 7.8 30.5 10.7C29.4 12.2 28.2 13 27 14C20.5 20.2 15 21 13.5 14ZM33.5 13.5L35.5 15.5L39.5 11.5",
  "handshake-loop": "M13.5 14C13.5 8 18.5 6.5 23.5 10C26 11.7 28.5 13.8 31 15.8C33 17.5 36 18 38.5 16.5C41.5 14.7 42 11 39.5 9C37 7 33.5 8 31.5 10.5C30 12.4 28 15.5 24.5 17.8C20 20.8 13.5 19 13.5 14Z",
  "ribbon-award": "M13.5 14C13.5 7.5 20 7 27 14C29.5 16.5 31 18 33.5 18.5L32.5 21.5L35 20L37.5 21.5L36.5 18.5C39.5 18.5 42 16.5 42 13.5C42 10 39.5 8 36.5 8C33 8 31 10.5 29.2 12.7C28.4 13.4 27.7 13.4 27 14C20.5 20.2 15 21 13.5 14ZM35 11A2.6 2.6 0 1 0 35 16.2A2.6 2.6 0 1 0 35 11Z"
};

/* Parked while the mark rests on `portal`; restore the startLogoMotion
   calls in application.js and design.js to revive the cycle. */
/*
 * Portal at small sizes: the full portal's inner spiral cannot resolve under
 * ~40px, so icon-scale marks use this variant — the same continuous line,
 * with the doorway drawn as a literal opening in the right lobe. One line,
 * one gap: the portal.
 */
export const LOGO_PORTAL_SMALL = "M41.5 16.5C40 20.8 33.5 20.5 27 14C20 7 13.5 7.5 13.5 14C13.5 21 20.5 20.2 27 14C34.5 7 42 7.5 41.7 11.5";

const ORDER = ["portal", "loop", "card", "bubble", "cursor"];

/*
 * THE MARK — "07 Portal", from the logo exploration deck: a doorway whose far
 * side is lit. The frame is a heavy silhouette at maximum contrast so it
 * survives any ground; all of the depth lives in the interior gradient,
 * running from the ramp's mid tone at the head to its faintest at the floor.
 * The frame's legs run to the floor with no bottom bar: the door does not
 * close behind you.
 *
 * Colours come from CSS (.jw-portal-*), so the mark follows the theme.
 */
export const LOGO_MARKUP = `
<svg class="jw-mark jw-mark-portal" viewBox="0 0 32 32" role="img" aria-hidden="true" focusable="false">
  <defs>
    <linearGradient class="jw-portal-glow" x1="0" y1="0" x2="0" y2="1">
      <stop class="jw-portal-head" offset="0%"></stop>
      <stop class="jw-portal-floor" offset="100%"></stop>
    </linearGradient>
  </defs>
  <path class="jw-portal-light" d="M9 32V16a7 7 0 0 1 14 0v16Z"></path>
  <path class="jw-portal-frame" fill-rule="evenodd"
        d="M2 32V16C2 8.3 8.3 2 16 2s14 6.3 14 14v16h-7V16a7 7 0 0 0-14 0v16Z"></path>
</svg>`.trim();

/* The one-line loop markup, kept for the /design specimens and the lab. */
export const LOGO_LINE_MARKUP = `
<svg class="jw-mark jw-mark-line" viewBox="0 0 56 28" role="img" aria-hidden="true" focusable="false">
  <defs>
    <linearGradient class="jw-mark-gradient" x1="0" y1="0" x2="1" y2="1">
      <stop class="jw-mark-stop-a" offset="0%"></stop>
      <stop class="jw-mark-stop-b" offset="100%"></stop>
    </linearGradient>
  </defs>
  <path class="jw-mark-path" d="${LOGO_STATES.portal}"></path>
</svg>`.trim();

/* Each mounted mark needs its own gradient id, or the second one on the page
   would reference the first one's definition. */
let gradientSeed = 0;

/**
 * Renders the mark into `host` and returns the created <svg>.
 * Pass `{ icon: true }` to wrap it in the app-icon container.
 * Motion is opt-in via startLogoMotion so a static mark costs nothing.
 */
export function mountLogo(host, { icon = false, variant = "portal" } = {}) {
  if (!host) return null;

  /* "portal" is the brand mark (the lit doorway). "line" is the one-line loop
     used by the /design specimens and the lab. `icon` wraps either in the
     app-icon container from components.css. */
  const markup = variant === "line" ? LOGO_LINE_MARKUP : LOGO_MARKUP;
  host.innerHTML = icon ? `<span class="jw-app-icon">${markup}</span>` : markup;

  const svg = host.querySelector(".jw-mark");
  gradientSeed += 1;

  if (variant === "line") {
    const gradient = svg.querySelector(".jw-mark-gradient");
    const path = svg.querySelector(".jw-mark-path");
    const id = `jw-mark-gradient-${gradientSeed}`;
    gradient.id = id;
    path.setAttribute("stroke", `url(#${id})`);
  } else {
    const glow = svg.querySelector(".jw-portal-glow");
    const id = `jw-portal-glow-${gradientSeed}`;
    glow.id = id;
    svg.querySelector(".jw-portal-light").setAttribute("fill", `url(#${id})`);
  }

  return svg;
}

/**
 * Cycles the mark through its states. Returns a teardown.
 *
 * `gsap` and `MorphSVGPlugin` are passed in rather than imported so this stays
 * usable without motion: if either is missing the mark simply rests on `loop`.
 */
export function startLogoMotion(svg, { gsap, MorphSVGPlugin, hold = 2.4, morph = 1.1 } = {}) {
  if (!svg || !gsap || !MorphSVGPlugin) return () => {};

  const path = svg.querySelector(".jw-mark-path");
  if (!path) return () => {};

  const timeline = gsap.timeline({ repeat: -1, defaults: { ease: "power2.inOut" } });

  for (let index = 1; index <= ORDER.length; index += 1) {
    /* Wraps back to `loop` on the last step, so the cycle closes seamlessly. */
    const next = ORDER[index % ORDER.length];
    timeline.to(path, {
      duration: morph,
      morphSVG: { shape: LOGO_STATES[next], shapeIndex: 0 }
    }, `+=${hold}`);
  }

  return () => timeline.kill();
}
