#!/usr/bin/env bash
#
# validate-app-manifest.sh - validate an Archipelago app manifest.
#
# Usage:
#   ./scripts/validate-app-manifest.sh [--repo-audit] apps/my-app/manifest.yml
#
# This intentionally mirrors the public app contract documented in
# docs/app-manifest-spec.md: manifests have a top-level `app:` block and are
# ultimately validated by the Rust parser in core/container/src/manifest.rs.
# This script is the contributor-friendly preflight; the Rust parser remains
# canonical.

set -euo pipefail

REPO_AUDIT=0
if [[ "${1:-}" == "--repo-audit" ]]; then
    REPO_AUDIT=1
    shift
fi

if [[ $# -ne 1 ]]; then
    echo "Usage: $0 [--repo-audit] <manifest.yml>"
    exit 1
fi

MANIFEST="$1"
PASS=0
FAIL=0
WARN=0

check() {
    local desc="$1" result="$2"
    case "$result" in
        pass)
            PASS=$((PASS + 1))
            echo "  PASS: $desc"
            ;;
        warn)
            WARN=$((WARN + 1))
            echo "  WARN: $desc"
            ;;
        *)
            FAIL=$((FAIL + 1))
            echo "  FAIL: $desc"
            ;;
    esac
}

# Preflight the YAML parser BEFORE any check runs. This used to shell out to
# ruby with stderr discarded, so a machine without ruby reported "invalid YAML"
# and rejected every manifest that was in fact perfectly valid — the first tool
# an app developer runs, failing with a message that sent them to fix the wrong
# thing. Fail loudly about the real cause instead.
if ! command -v python3 >/dev/null 2>&1; then
    echo "ERROR: python3 is required to validate manifests, but was not found." >&2
    exit 3
fi
if ! python3 -c 'import yaml' >/dev/null 2>&1; then
    echo "ERROR: the PyYAML module is required to validate manifests." >&2
    echo "  Install it with:  python3 -m pip install pyyaml" >&2
    echo "  (Debian/Ubuntu:   apt-get install python3-yaml)" >&2
    exit 3
fi

# Evaluate a path expression against the manifest's top-level `app` block.
# Missing keys yield an empty string rather than an error, so callers can write
# a plain chain like app["container"]["build"]["tag"] without guarding each hop.
yaml_eval() {
    python3 -c '
import sys, yaml

class Nil:
    """Absent value: indexes to itself, is falsy, prints as empty."""
    def __getitem__(self, key): return self
    def __bool__(self): return False
    def __str__(self): return ""
    def __iter__(self): return iter(())

NIL = Nil()

class SafeDict(dict):
    def __missing__(self, key): return NIL

def wrap(value):
    if isinstance(value, dict):
        return SafeDict({k: wrap(v) for k, v in value.items()})
    if isinstance(value, list):
        return [wrap(v) for v in value]
    return value

path, expr = sys.argv[1], sys.argv[2]
with open(path) as fh:
    data = yaml.safe_load(fh)
app = data.get("app") if isinstance(data, dict) else None
if not isinstance(app, dict):
    sys.exit("missing top-level app block")
app = wrap(app)

value = eval(expr, {"__builtins__": {}}, {"app": app})
if isinstance(value, list):
    print("\n".join(str(v) for v in value))
elif isinstance(value, dict):
    print("\n".join(f"{k}={v}" for k, v in value.items()))
elif value is None or isinstance(value, Nil):
    print("")
elif isinstance(value, bool):
    print("true" if value else "false")
else:
    print(value)
' "$MANIFEST" "$1"
}

echo "Validating: $MANIFEST"
echo ""

if [[ ! -f "$MANIFEST" ]]; then
    echo "  FAIL: File not found: $MANIFEST"
    exit 1
fi
check "File exists" "pass"

if ! python3 -c '
import sys, yaml
with open(sys.argv[1]) as fh:
    data = yaml.safe_load(fh)
sys.exit(0 if isinstance(data, dict) and isinstance(data.get("app"), dict) else 1)
' "$MANIFEST" 2>/dev/null; then
    check "Valid YAML with top-level app block" "fail"
    echo ""
    echo "Results: $PASS passed, $FAIL failed, $WARN warnings"
    echo "STATUS: REJECTED - fix failures before resubmitting"
    exit 1
fi
check "Valid YAML with top-level app block" "pass"

APP_ID="$(yaml_eval 'app["id"]')"
APP_NAME="$(yaml_eval 'app["name"]')"
APP_VERSION="$(yaml_eval 'app["version"]')"
APP_DESCRIPTION="$(yaml_eval 'app["description"]')"
APP_INTERNAL="$(yaml_eval 'app["internal"]')"
IMAGE="$(yaml_eval 'app["container"]["image"]')"
BUILD_CONTEXT="$(yaml_eval 'app["container"]["build"]["context"]')"
BUILD_TAG="$(yaml_eval 'app["container"]["build"]["tag"]')"

if [[ "$APP_ID" =~ ^[a-z0-9]+(-[a-z0-9]+)*$ ]]; then
    check "app.id is lowercase kebab-case ($APP_ID)" "pass"
else
    check "app.id is lowercase kebab-case" "fail"
fi

if [[ -n "$APP_NAME" ]]; then
    check "app.name present" "pass"
else
    check "app.name present" "fail"
fi

if [[ "$APP_VERSION" =~ [0-9] ]]; then
    check "app.version present and contains a digit" "pass"
else
    check "app.version present and contains a digit" "fail"
fi

if [[ -n "$APP_DESCRIPTION" ]]; then
    check "app.description present" "pass"
else
    check "app.description present" "warn"
fi

HAS_IMAGE=0
HAS_BUILD=0
[[ -n "$IMAGE" ]] && HAS_IMAGE=1
[[ -n "$BUILD_CONTEXT" || -n "$BUILD_TAG" ]] && HAS_BUILD=1

if [[ "$HAS_IMAGE" -eq 1 && "$HAS_BUILD" -eq 0 ]]; then
    check "container.image specified" "pass"
elif [[ "$HAS_IMAGE" -eq 0 && "$HAS_BUILD" -eq 1 ]]; then
    if [[ -n "$BUILD_CONTEXT" && -n "$BUILD_TAG" ]]; then
        check "container.build specified with context and tag" "pass"
    else
        check "container.build requires context and tag" "fail"
    fi
else
    check "exactly one of container.image or container.build specified" "fail"
fi

if [[ -n "$IMAGE" ]]; then
    TRUSTED=false
    for reg in "docker.io" "ghcr.io" "quay.io" "registry.hub.docker.com" "source.archipelago-foundation.org" "localhost/"; do
        if [[ "$IMAGE" == *"$reg"* ]]; then
            TRUSTED=true
            break
        fi
    done
    if [[ "$TRUSTED" == "true" || "$IMAGE" != */* ]]; then
        check "image registry is recognized" "pass"
    else
        check "image registry is not in the reviewed list ($IMAGE)" "warn"
    fi

    if [[ "$IMAGE" == *":latest" ]]; then
        if [[ "$APP_INTERNAL" == "true" || "$IMAGE" == localhost/* ]]; then
            check "internal/local build uses :latest ($IMAGE)" "warn"
        elif [[ "$REPO_AUDIT" -eq 1 ]]; then
            check "existing manifest uses :latest and must be pinned before public app submission ($IMAGE)" "warn"
        else
            check "image tag is pinned and not :latest ($IMAGE)" "fail"
        fi
    elif [[ "$IMAGE" != *:* ]]; then
        check "image tag is explicit ($IMAGE)" "warn"
    else
        check "image tag is pinned" "pass"
    fi
fi

MEMORY_LIMIT="$(yaml_eval 'app["resources"]["memory_limit"] or app["resources"]["memory"]')"
CPU_LIMIT="$(yaml_eval 'app["resources"]["cpu_limit"] or app["resources"]["cpu"]')"
[[ -n "$MEMORY_LIMIT" ]] && check "resources.memory_limit specified ($MEMORY_LIMIT)" "pass" || check "resources.memory_limit specified" "warn"
[[ -n "$CPU_LIMIT" ]] && check "resources.cpu_limit specified ($CPU_LIMIT)" "pass" || check "resources.cpu_limit specified" "warn"

READONLY_ROOT="$(yaml_eval 'app["security"]["readonly_root"]')"
NO_NEW_PRIVS="$(yaml_eval 'app["security"]["no_new_privileges"]')"
NETWORK_POLICY="$(yaml_eval 'app["security"]["network_policy"]')"
CONTAINER_NETWORK="$(yaml_eval 'app["container"]["network"]')"

if [[ "$READONLY_ROOT" == "true" || -z "$READONLY_ROOT" ]]; then
    check "security.readonly_root true (explicit or Rust default)" "pass"
else
    check "security.readonly_root true or explicitly justified" "warn"
fi

if [[ "$NO_NEW_PRIVS" == "true" || -z "$NO_NEW_PRIVS" ]]; then
    check "security.no_new_privileges true (explicit or Rust default)" "pass"
elif [[ "$REPO_AUDIT" -eq 1 ]]; then
    check "existing manifest disables security.no_new_privileges and needs review" "warn"
else
    check "security.no_new_privileges true" "fail"
fi

if [[ "$NETWORK_POLICY" == "isolated" || "$NETWORK_POLICY" == "bridge" || "$NETWORK_POLICY" == "host" || -z "$NETWORK_POLICY" ]]; then
    check "security.network_policy valid" "pass"
else
    check "security.network_policy valid" "fail"
fi

if [[ "$CONTAINER_NETWORK" == container:* || "$CONTAINER_NETWORK" == ns:* ]]; then
    check "container.network does not share another namespace" "fail"
else
    check "container.network does not share another namespace" "pass"
fi

SECRET_ENV="$(yaml_eval 'app["environment"]')"
if echo "$SECRET_ENV" | grep -iqE '^[A-Z0-9_]*(PASSWORD|PASS|SECRET|TOKEN|API_KEY|PRIVATE_KEY)[A-Z0-9_]*=.+$'; then
    check "no hardcoded secret-like values in app.environment" "warn"
else
    check "no hardcoded secret-like values in app.environment" "pass"
fi

if [[ -n "$APP_ID" && -n "$MANIFEST" ]]; then
    EXPECTED_DIR="$(basename "$(dirname "$MANIFEST")")"
    if [[ "$EXPECTED_DIR" == "$APP_ID" ]]; then
        check "app.id matches directory name" "pass"
    elif [[ "$REPO_AUDIT" -eq 1 ]]; then
        check "existing manifest app.id differs from directory name ($EXPECTED_DIR)" "warn"
    else
        check "app.id matches directory name ($EXPECTED_DIR)" "fail"
    fi
fi

PORT_CHECK="$(python3 -c '
import glob, os, sys, yaml

def load_app(path):
    try:
        with open(path) as fh:
            data = yaml.safe_load(fh)
    except Exception:
        return None
    return data.get("app") if isinstance(data, dict) else None

current = sys.argv[1]
current_id = os.path.basename(os.path.dirname(current))

def port_keys(app):
    for entry in (app.get("ports") or []):
        if not isinstance(entry, dict):
            continue
        host = entry.get("host")
        if not host:
            continue
        yield (host, entry.get("protocol") or "tcp", entry.get("bind") or "")

claimed = {}
for path in sorted(glob.glob("apps/*/manifest.yml")):
    app = load_app(path)
    if not isinstance(app, dict):
        continue
    app_id = app.get("id") or os.path.basename(os.path.dirname(path))
    if app_id == current_id:
        continue
    for key in port_keys(app):
        claimed[key] = app_id

app = load_app(current)
if isinstance(app, dict):
    for key in port_keys(app):
        if key in claimed:
            host, proto, bind = key
            shown = bind if bind else "*"
            print(f"{shown}:{host}/{proto} already used by {claimed[key]}")
' "$MANIFEST")"
if [[ -n "$PORT_CHECK" ]]; then
    while IFS= read -r conflict; do
        check "port conflict: $conflict" "warn"
    done <<< "$PORT_CHECK"
else
    check "no duplicate host port bindings" "pass"
fi

echo ""
echo "Results: $PASS passed, $FAIL failed, $WARN warnings"

if [[ "$FAIL" -gt 0 ]]; then
    echo "STATUS: REJECTED - fix failures before resubmitting"
    exit 1
fi

echo "STATUS: APPROVED (with $WARN warnings)"
