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
user-facing. The visible history deliberately starts at v1.8.0-alpha; older
blocks are removed so this remains a concise product history rather than an
unbounded archive.
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
MIN_VISIBLE_VERSION = (1, 8, 0)

HEADER_RE = re.compile(r"^## (v\d+\.\d+\.\d+\S*) \((\d{4})-(\d{2})-(\d{2})\)")

# A header that names a version but carries no YYYY-MM-DD date. Such a line
# does not match HEADER_RE, so the version is INVISIBLE here — the tool then
# reports "all present" while the very release being cut has no modal block.
# That is what happened to v1.8.4-alpha on 2026-08-20: the changelog said
# "(draft — date set at cut)", this check passed, and create-release then
# aborted at the frontend step because the built bundle contained no version
# string at all (the What's New modal is the only place one appears).
UNDATED_RE = re.compile(r"^## (v\d+\.\d+\.\d+\S*) \((?!\d{4}-\d{2}-\d{2}\))")


def parse_changelog():
    """Return [(version, 'Month D, YYYY', [bullet, ...]), ...] newest-first."""
    entries = []
    cur = None
    for line in CHANGELOG.read_text().splitlines():
        m = HEADER_RE.match(line)
        if m:
            ver, y, mo, d = m.groups()
            cur = {"ver": ver, "date": f"{MONTHS[int(mo)]} {int(d)}, {y}", "bullets": []}
            if version_key(ver) >= MIN_VISIBLE_VERSION:
                entries.append(cur)
            else:
                cur = None
            continue
        if cur is not None and line.startswith("- "):
            text = line[2:].strip()
            if text.lower().startswith("validation "):
                continue  # dev-process note, not user-facing
            cur["bullets"].append(text)
    return entries


def undated_versions():
    """[(version, whole line), ...] for version headers with no real date."""
    found = []
    for line in CHANGELOG.read_text().splitlines():
        m = UNDATED_RE.match(line)
        if m:
            found.append((m.group(1), line.strip()))
    return found


def ordered_versions():
    """Return generated modal versions in display order (top to bottom)."""
    return re.findall(r"<!-- (v\d+\.\d+\.\d+\S*) -->", MODAL.read_text())


def legacy_blocks():
    """Return old hand-written alpha blocks that predate generated markers."""
    return re.findall(r"<!-- (alpha\.[^ ]+) -->", MODAL.read_text())


def version_key(version):
    match = re.match(r"v(\d+)\.(\d+)\.(\d+)", version)
    return tuple(map(int, match.groups()))


def sort_modal_blocks(entries, *, check=False):
    """Re-render current release-note blocks newest-first and remove old history."""
    lines = MODAL.read_text().splitlines(keepends=True)
    # Include the old hand-written `alpha.*` blocks in the replace range so
    # normalization can delete them. Previously the checker saw only generated
    # vX.Y.Z markers and falsely claimed the v1.8.0 history floor was enforced.
    marker = re.compile(r"^\s*<!-- ((?:v\d+\.\d+\.\d+\S*)|(?:alpha\.[^ ]+)) -->\s*$")
    blocks = []

    for start, line in enumerate(lines):
        match = marker.match(line)
        if not match:
            continue
        depth = 0
        opened = False
        for index in range(start + 1, len(lines)):
            for tag in re.findall(r"</?div\b[^>]*>", lines[index]):
                if tag.startswith("</"):
                    depth -= 1
                else:
                    depth += 1
                    opened = True
            if opened and depth == 0:
                blocks.append((start, index + 1, match.group(1), lines[start:index + 1]))
                break
        else:
            raise RuntimeError(f"unclosed What's New block for {match.group(1)}")

    if not blocks:
        return False
    for previous, following in zip(blocks, blocks[1:]):
        gap = "".join(lines[previous[1]:following[0]])
        if gap.strip():
            raise RuntimeError("unexpected content between What's New release blocks")

    by_version = {entry["ver"]: entry for entry in entries}
    retained = [
        b for b in blocks
        if b[2].startswith("v") and version_key(b[2]) >= MIN_VISIBLE_VERSION
    ]
    sorted_segments = [
        render_block(by_version[b[2]]).splitlines(keepends=True)
        if b[2] in by_version else b[3]
        for b in sorted(retained, key=lambda b: version_key(b[2]), reverse=True)
    ]
    output = lines[:blocks[0][0]]
    for segment in sorted_segments:
        output.extend(segment)
    output.extend(lines[blocks[-1][1]:])
    changed = output != lines
    if changed and not check:
        MODAL.write_text("".join(output))
    return changed


def to_html(text):
    text = text.replace("`", "")
    escaped = html.escape(text, quote=False)  # & < > (Vue template-safe)
    escaped = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", escaped)
    escaped = re.sub(r"(?<!\*)\*([^*]+?)\*(?!\*)", r"<em>\1</em>", escaped)
    return escaped


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

    # Refuse to reason about a changelog whose newest entry has no date:
    # silently skipping it is what makes this check pass while the release
    # is in fact missing its modal block.
    undated = undated_versions()
    if undated:
        for ver, line in undated:
            print(f"FAIL: CHANGELOG entry for {ver} carries no release date:",
                  file=sys.stderr)
            print(f"        {line}", file=sys.stderr)
        print("A dated '## vX.Y.Z (YYYY-MM-DD)' header is required before the "
              "modal can be synced — without it the version is invisible to "
              "this tool and never reaches the built frontend.", file=sys.stderr)
        return 1

    entries = parse_changelog()
    displayed = ordered_versions()
    have = set(displayed)
    missing = [e for e in entries if e["ver"] not in have]
    expected_order = sorted(displayed, key=version_key, reverse=True)
    out_of_order = displayed != expected_order
    too_old = [v for v in displayed if version_key(v) < MIN_VISIBLE_VERSION]
    too_old.extend(legacy_blocks())

    if not missing and not out_of_order and not too_old:
        changed = sort_modal_blocks(entries, check=check)
        if changed and check:
            print("FAIL: What's New dates or descriptions differ from CHANGELOG.md. "
                  "Run: python3 scripts/sync-whats-new.py", file=sys.stderr)
            return 1
        if changed:
            print("Re-rendered What's New blocks from the curated changelog.")
        else:
            print("What's New modal is in sync with CHANGELOG.md "
                  f"({len(entries)} changelog versions, all present and newest-first).")
        return 0

    names = ", ".join(e["ver"] for e in missing)
    if check:
        if missing:
            print("FAIL: these CHANGELOG versions have no block in the Settings "
                  f"What's New modal: {names}", file=sys.stderr)
        if out_of_order:
            print("FAIL: What's New entries are not newest-first; the modal currently "
                  f"opens at {displayed[0]} instead of {expected_order[0]}", file=sys.stderr)
        if too_old:
            print("FAIL: What's New contains entries older than the v1.8.0 history floor: "
                  + ", ".join(too_old), file=sys.stderr)
        print("Run: python3 scripts/sync-whats-new.py", file=sys.stderr)
        return 1

    if missing:
        # Insert before the first block; the full sort below makes this safe even
        # when a historical hand-written block was accidentally left at the top.
        lines = MODAL.read_text().splitlines(keepends=True)
        marker = re.compile(r"^\s*<!-- v\d+\.\d+\.\d+\S* -->\s*$")
        idx = next((i for i, ln in enumerate(lines) if marker.match(ln)), None)
        if idx is None:
            print("ERROR: could not find an existing version block marker in the modal.",
                  file=sys.stderr)
            return 2
        lines.insert(idx, "".join(render_block(e) for e in missing))
        MODAL.write_text("".join(lines))
        print(f"Inserted {len(missing)} block(s): {names}")

    if sort_modal_blocks(entries):
        print("Normalized What's New blocks (v1.8.0+ only, newest-first).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
