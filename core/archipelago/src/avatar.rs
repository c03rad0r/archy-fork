//! Deterministic default avatars derived from a Nostr/Ed25519 pubkey.
//!
//! Two flavours are generated as base64-encoded SVG data URLs so they can
//! live directly in `IdentityProfile.picture` without any blob-store round
//! trip:
//!
//! - [`identicon`] — a 5×5 symmetric grid (GitHub-style) for sub-identities.
//! - [`master_node_svg`] — a hexagonal-network motif for the primary
//!   seed-derived identity (derivation index 0). Distinct at a glance from
//!   the identicons so the user can tell their own node at 48 px.
//!
//! Both read the first 8 bytes of the hex pubkey, so the same key always
//! produces the same avatar — useful for reconstructing history without
//! storing the blob.

use base64::Engine;

/// Convert a byte to an HSL triple biased toward readable foregrounds on
/// dark backgrounds (saturation 60–85%, lightness 52–70%).
fn hue_color(seed: u8) -> String {
    let hue = (seed as u32) * 360 / 256;
    format!("hsl({}, 72%, 60%)", hue)
}

fn accent_color(seed: u8) -> String {
    let hue = (seed as u32) * 360 / 256;
    format!("hsl({}, 80%, 68%)", hue)
}

fn encode_svg(svg: &str) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
    format!("data:image/svg+xml;base64,{}", b64)
}

/// Parse the first 8 bytes from a hex pubkey. Returns `[0u8; 8]` if the
/// input is too short or malformed — callers get a consistent default
/// avatar rather than an error.
fn seed_bytes(pubkey_hex: &str) -> [u8; 8] {
    let mut out = [0u8; 8];
    let clean: String = pubkey_hex
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect();
    for (i, byte) in out.iter_mut().enumerate() {
        let lo = i * 2;
        if clean.len() >= lo + 2 {
            *byte = u8::from_str_radix(&clean[lo..lo + 2], 16).unwrap_or(0);
        }
    }
    out
}

/// 5×5 mirrored identicon. ~700 bytes of SVG, ~1 KB as a data URL.
pub fn identicon(pubkey_hex: &str) -> String {
    let bytes = seed_bytes(pubkey_hex);
    let fg = hue_color(bytes[0]);
    let bg = "#171a24";

    // 15 bit slots (3 visible columns × 5 rows). Mirror to 5×5.
    // Use bytes[1..=2] as 16 bits, drop the MSB so we get 15.
    let bits = u16::from_be_bytes([bytes[1], bytes[2]]) & 0x7fff;

    let mut cells = String::with_capacity(512);
    let cell_px: u32 = 16;
    for row in 0..5u32 {
        for col in 0..5u32 {
            let src_col = if col < 3 { col } else { 4 - col };
            let bit_idx = row * 3 + src_col;
            if (bits >> bit_idx) & 1 == 1 {
                let x = col * cell_px;
                let y = row * cell_px;
                cells.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                    x, y, cell_px, cell_px
                ));
            }
        }
    }

    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 80 80\" \
         shape-rendering=\"crispEdges\">\
         <rect width=\"80\" height=\"80\" fill=\"{bg}\"/>\
         <g fill=\"{fg}\">{cells}</g>\
         </svg>"
    );
    encode_svg(&svg)
}

/// Hex-network motif for the master (seed-index-0) identity. Central hex
/// plus six ring hexes connected by faint edges, with an accent colour
/// derived from the pubkey. Distinct silhouette from the 5×5 identicon so
/// the node identity reads differently at every size.
pub fn master_node_svg(pubkey_hex: &str) -> String {
    let bytes = seed_bytes(pubkey_hex);
    let accent = accent_color(bytes[0]);
    let accent2 = accent_color(bytes[0].wrapping_add(64));
    let pattern = bytes[3] & 0x3f; // 6 bits — one per ring hex

    // Hexagon vertices (point-up) at radius 16, centred on (c, c).
    let hex_path = |cx: f64, cy: f64, r: f64| -> String {
        let mut pts = String::new();
        for i in 0..6 {
            let theta = std::f64::consts::FRAC_PI_3 * (i as f64) - std::f64::consts::FRAC_PI_2;
            let x = cx + r * theta.cos();
            let y = cy + r * theta.sin();
            if i == 0 {
                pts.push_str(&format!("M{:.2},{:.2}", x, y));
            } else {
                pts.push_str(&format!(" L{:.2},{:.2}", x, y));
            }
        }
        pts.push_str(" Z");
        pts
    };

    let c = 64.0;
    let ring_r = 36.0;

    // Ring centres (6 hexes at 60° intervals around centre).
    let ring_centres: Vec<(f64, f64)> = (0..6)
        .map(|i| {
            let theta = std::f64::consts::FRAC_PI_3 * (i as f64) - std::f64::consts::FRAC_PI_2;
            (c + ring_r * theta.cos(), c + ring_r * theta.sin())
        })
        .collect();

    let mut ring_hexes = String::new();
    let mut edges = String::new();
    for (i, (rx, ry)) in ring_centres.iter().enumerate() {
        // Alternate fill/stroke based on pattern bits so two nodes never
        // share the same ring silhouette.
        let filled = (pattern >> i) & 1 == 1;
        let fill = if filled { &accent } else { "none" };
        let stroke_w = if filled { 0.0 } else { 1.4 };
        ring_hexes.push_str(&format!(
            "<path d=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\" opacity=\"0.92\"/>",
            hex_path(*rx, *ry, 10.5),
            fill,
            &accent,
            stroke_w
        ));
        // Edge from centre to this ring node.
        edges.push_str(&format!(
            "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" \
             stroke=\"{}\" stroke-width=\"1\" opacity=\"0.35\"/>",
            c, c, rx, ry, &accent2
        ));
    }

    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 128 128\">\
         <defs>\
         <radialGradient id=\"bg\" cx=\"50%\" cy=\"50%\" r=\"60%\">\
         <stop offset=\"0%\" stop-color=\"#1c2030\"/>\
         <stop offset=\"100%\" stop-color=\"#0a0d16\"/>\
         </radialGradient>\
         </defs>\
         <rect width=\"128\" height=\"128\" fill=\"url(#bg)\"/>\
         {edges}\
         {ring_hexes}\
         <path d=\"{centre_hex}\" fill=\"{accent}\" stroke=\"#ffffff\" stroke-width=\"1.5\"/>\
         </svg>",
        centre_hex = hex_path(c, c, 16.0),
    );
    encode_svg(&svg)
}

/// Build a default [`IdentityProfile`]-shaped picture for the given
/// identity. The master (seed index 0) gets the node SVG; everyone else
/// gets the identicon.
pub fn default_picture(pubkey_hex: &str, is_master: bool) -> String {
    if is_master {
        master_node_svg(pubkey_hex)
    } else {
        identicon(pubkey_hex)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identicon_is_deterministic() {
        let a = identicon("aabbccddeeff0011");
        let b = identicon("aabbccddeeff0011");
        assert_eq!(a, b);
        assert!(a.starts_with("data:image/svg+xml;base64,"));
    }

    #[test]
    fn master_is_distinct_from_identicon() {
        let pk = "aabbccddeeff0011";
        assert_ne!(identicon(pk), master_node_svg(pk));
    }

    #[test]
    fn handles_short_or_malformed_hex() {
        // Shouldn't panic, should still return a valid data URL.
        let a = identicon("");
        assert!(a.starts_with("data:image/svg+xml;base64,"));
        let b = master_node_svg("xyz!!!");
        assert!(b.starts_with("data:image/svg+xml;base64,"));
    }
}
