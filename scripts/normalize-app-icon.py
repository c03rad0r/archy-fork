#!/usr/bin/env python3
"""Normalize a third-party SVG mark onto the Archipelago app-icon canvas.

Store tiles render icons as-is — there is deliberately no runtime inset
(it would double-margin every icon that already ships whitespace), so the
margin must be baked into the file. House icons carry it naturally;
third-party marks are usually full-bleed (Alby Hub's tile icon was the
first to land edge-to-edge in the store, 2026-08-14). This wraps any SVG
in a square canvas with a uniform inner margin, preserving the original
untouched as a nested <svg>.

Usage:
  scripts/normalize-app-icon.py in.svg out.svg [--margin 0.12]

Margin is a fraction of the canvas per side (default 12%, the house look).
"""

import argparse
import re
import sys


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("src")
    ap.add_argument("dst")
    ap.add_argument("--margin", type=float, default=0.12,
                    help="inner margin per side, fraction of canvas (default 0.12)")
    args = ap.parse_args()

    svg = open(args.src, encoding="utf-8").read()

    m = re.search(r"<svg\b[^>]*>", svg, re.S)
    if not m:
        print("error: no <svg> root element found", file=sys.stderr)
        return 1
    root = m.group(0)

    vb = re.search(r'viewBox\s*=\s*["\']([\d.eE+\s,-]+)["\']', root)
    if vb:
        nums = [float(x) for x in re.split(r"[\s,]+", vb.group(1).strip())]
        if len(nums) != 4:
            print("error: unparseable viewBox", file=sys.stderr)
            return 1
        _, _, w, h = nums
        inner_viewbox = vb.group(1).strip()
    else:
        wm = re.search(r'width\s*=\s*["\']([\d.]+)', root)
        hm = re.search(r'height\s*=\s*["\']([\d.]+)', root)
        if not (wm and hm):
            print("error: no viewBox and no width/height to derive one", file=sys.stderr)
            return 1
        w, h = float(wm.group(1)), float(hm.group(1))
        inner_viewbox = f"0 0 {w} {h}"

    # Square canvas fitting the larger dimension; the mark centers in it.
    side = max(w, h)
    canvas = 100.0
    margin = canvas * args.margin
    avail = canvas - 2 * margin
    scale_w = avail * (w / side)
    scale_h = avail * (h / side)
    x = (canvas - scale_w) / 2
    y = (canvas - scale_h) / 2

    # Strip any XML prolog from the original; it nests inside the wrapper.
    inner = re.sub(r"^\s*<\?xml[^>]*\?>\s*", "", svg)

    out = (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {canvas:g} {canvas:g}">\n'
        f'  <!-- normalized by scripts/normalize-app-icon.py: margin={args.margin:g} per side -->\n'
        f'  <svg x="{x:.3f}" y="{y:.3f}" width="{scale_w:.3f}" height="{scale_h:.3f}" '
        f'viewBox="{inner_viewbox}" preserveAspectRatio="xMidYMid meet">\n'
        f'{inner}\n'
        f'  </svg>\n'
        f'</svg>\n'
    )
    open(args.dst, "w", encoding="utf-8").write(out)
    print(f"wrote {args.dst}: {canvas:g}x{canvas:g} canvas, mark {scale_w:.1f}x{scale_h:.1f} at {args.margin:.0%} margin")
    return 0


if __name__ == "__main__":
    sys.exit(main())
