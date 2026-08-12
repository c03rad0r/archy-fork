#!/usr/bin/env python3
"""Sync the Settings "What's New" modal with CHANGELOG.md.

The modal (neode-ui/src/views/settings/AccountInfoSection.vue) hardcodes one
HTML block per release. It has repeatedly drifted behind CHANGELOG.md (it sat
at v1.7.84 while the fleet shipped through v1.7.92). This script is the fix:
for every version in CHANGELOG.md that has no block in the modal, it generates
a block (from the curated CHANGELOG bullets) and inserts it newest-first.

  python3 scripts/sync-whats-new.py            # insert any missing blocks
  python3 scripts/sync-whats-new.py --check    # exit 1 if anything is missing

Dev-process bullets ("Validation passed…/pending…") are dropped — the modal is
user-facing. Only CHANGELOG versions are managed; older hand-written blocks
(pre-CHANGELOG history) are never touched or removed.
"""
import re
import sys
import html
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CHANGELOG = REPO / "CHANGELOG.md"
MODAL = REPO / "neode-ui/src/views/settings/AccountInfoSection.vue"

MONTHS = ["", "January", "February", "March", "April", "May", "June", "July",
          "August", "September", "October", "November", "December"]

HEADER_RE = re.compile(r"^## (v\d+\.\d+\.\d+\S*) \((\d{4})-(\d{2})-(\d{2})\)")


def parse_changelog():
    """Return [(version, 'Month D, YYYY', [bullet, ...]), ...] newest-first."""
    entries = []
    cur = None
    for line in CHANGELOG.read_text().splitlines():
        m = HEADER_RE.match(line)
        if m:
            ver, y, mo, d = m.groups()
            cur = {"ver": ver, "date": f"{MONTHS[int(mo)]} {int(d)}, {y}", "bullets": []}
            entries.append(cur)
            continue
        if cur is not None and line.startswith("- "):
            text = line[2:].strip()
            if text.lower().startswith("validation "):
                continue  # dev-process note, not user-facing
            cur["bullets"].append(text)
    return entries


def existing_versions():
    text = MODAL.read_text()
    return set(re.findall(r"<!-- (v\d+\.\d+\.\d+\S*) -->", text))


def to_html(text):
    text = text.replace("`", "")          # drop markdown code ticks (plain prose)
    return html.escape(text, quote=False)  # & < >  (Vue template-safe)


def render_block(entry):
    paras = "\n".join(
        f"                  <p>{to_html(b)}</p>" for b in entry["bullets"]
    )
    return (
        f"              <!-- {entry['ver']} -->\n"
        f"              <div>\n"
        f"                <div class=\"flex items-center gap-2 mb-3\">\n"
        f"                  <span class=\"text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300\">{entry['ver']}</span>\n"
        f"                  <span class=\"text-xs text-white/40\">{entry['date']}</span>\n"
        f"                </div>\n"
        f"                <div class=\"space-y-3 text-sm text-white/80 pl-3 border-l border-white/10\">\n"
        f"{paras}\n"
        f"                </div>\n"
        f"              </div>\n"
    )


def main():
    check = "--check" in sys.argv
    entries = parse_changelog()
    have = existing_versions()
    missing = [e for e in entries if e["ver"] not in have]

    if not missing:
        print("What's New modal is in sync with CHANGELOG.md "
              f"({len(entries)} changelog versions, all present).")
        return 0

    names = ", ".join(e["ver"] for e in missing)
    if check:
        print("FAIL: these CHANGELOG versions have no block in the Settings "
              f"What's New modal: {names}", file=sys.stderr)
        print("Run: python3 scripts/sync-whats-new.py", file=sys.stderr)
        return 1

    # Insert missing blocks newest-first, immediately before the newest existing
    # block marker (the first "<!-- v... -->" line in the file).
    lines = MODAL.read_text().splitlines(keepends=True)
    marker = re.compile(r"^\s*<!-- v\d+\.\d+\.\d+\S* -->\s*$")
    idx = next((i for i, ln in enumerate(lines) if marker.match(ln)), None)
    if idx is None:
        print("ERROR: could not find an existing version block marker in the modal.",
              file=sys.stderr)
        return 2

    # newest-first: sort missing by their order in `entries` (already newest-first)
    block_text = "".join(render_block(e) for e in missing)
    lines.insert(idx, block_text)
    MODAL.write_text("".join(lines))
    print(f"Inserted {len(missing)} block(s): {names}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
