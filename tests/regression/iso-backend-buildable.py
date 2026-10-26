#!/usr/bin/env python3
"""Assert the ISO backend stage can actually compile the code it ships.

Both checks encode a real, observed build failure (not a style preference):

1. TOOLCHAIN. `core/archipelago/src/monitoring/store.rs` uses
   `AtomicUsize::try_update`, which is unstable until Rust 1.99
   (rust-lang issue #135894). The ISO backend stage pinned
   `FROM rust:1.93-trixie`, so the pinned toolchain could not compile the
   pinned code: `error[E0658]: use of unstable library feature
   atomic_try_update`. Nothing caught this earlier because the host runs
   rustc 1.99.1 — only the container image is behind.

2. BUILD CONTEXT. `core/archipelago/src/api/rpc/package/runtime.rs` does
   `include_str!("../../../../../../apps/nginx-proxy-manager/manifest.yml")`,
   six levels up out of `core/`. The manifest is in the repo, but the stage
   only did `COPY core ./core`, so `apps/` never entered the build context:
   `error: couldn't read .../apps/nginx-proxy-manager/manifest.yml`.

The test reads the Dockerfile embedded in the ISO builder. It is a static
assertion because reproducing either failure requires a full multi-hour
container build; the failure modes are structural, so the textual contract
is the right gate.
"""
import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent.parent
BUILDER = REPO / "image-recipe/_archived/build-auto-installer-iso.sh"

# Rust 1.99 is the minimum that provides AtomicUsize::try_update.
MIN_RUST = (1, 99)


def parse_version(tag):
    m = re.match(r"(\d+)\.(\d+)", tag)
    return (int(m.group(1)), int(m.group(2))) if m else None


def main():
    if not BUILDER.is_file():
        print(f"FAIL: ISO builder not found: {BUILDER}")
        return 1
    text = BUILDER.read_text()

    # Locate the backend builder stage: the one that builds the archipelago bin.
    build_line = next(
        (ln for ln in text.splitlines()
         if "cargo build --release --bin archipelago" in ln),
        None,
    )
    if build_line is None:
        print("FAIL: no 'cargo build --release --bin archipelago' step found")
        return 1

    idx = text.index(build_line)
    head = text[:idx]

    froms = re.findall(r"^FROM\s+rust:([0-9][^\s]*)", head, re.MULTILINE)
    if not froms:
        print("FAIL: no 'FROM rust:<version>' found before the archipelago build")
        return 1
    tag = froms[-1]
    ver = parse_version(tag)
    if ver is None:
        print(f"FAIL: cannot parse rust version from tag {tag!r}")
        return 1

    failures = []
    if ver < MIN_RUST:
        failures.append(
            f"ISO backend pins rust:{tag}, but store.rs needs "
            f"try_update (Rust {MIN_RUST[0]}.{MIN_RUST[1]}+) — "
            f"error[E0658] atomic_try_update"
        )
    else:
        print(f"ok: backend toolchain rust:{tag} >= 1.99")

    # build context: apps/ must be copied in for the include_str! to resolve
    stage_text = head[head.rindex("FROM rust:"):]
    if not re.search(r"^COPY\s+apps\s+", stage_text, re.MULTILINE):
        failures.append(
            "backend stage never copies apps/ into the build context, but "
            "runtime.rs include_str!s apps/nginx-proxy-manager/manifest.yml"
        )
    else:
        print("ok: backend stage copies apps/ into the build context")

    # ...and apps/ must survive .dockerignore, which is deny-by-default (`*`)
    # with an allowlist. COPY of a filtered path fails at build time with
    # "no items matching glob ... (1 filtered out using .dockerignore)".
    dockerignore = REPO / ".dockerignore"
    if not dockerignore.is_file():
        print("ok: no .dockerignore (nothing to filter)")
    else:
        lines = [
            ln.strip() for ln in dockerignore.read_text().splitlines()
            if ln.strip() and not ln.strip().startswith("#")
        ]
        deny_all = "*" in lines
        allowed = any(
            ln in ("!apps", "!apps/", "!**/apps", "!*/apps") for ln in lines
        )
        if deny_all and not allowed:
            failures.append(
                ".dockerignore ignores everything (`*`) but never allows "
                "apps/, so `COPY apps ./apps` matches no files: "
                "'no items matching glob ... (1 filtered out)'"
            )
        else:
            print("ok: .dockerignore does not filter out apps/")

    if failures:
        print()
        for f in failures:
            print(f"FAIL: {f}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
