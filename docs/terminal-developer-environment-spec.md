# Archipelago terminal and developer environment plan

Status: planning draft, 2026-10-09. No runtime implementation or installation is
part of this change. Repository baseline: `2cb1bae5ce244d387679f90951d62fd030ebf228`.

Archipelago should let an owner open a real terminal, resume previous work,
configure their system, and ask a preinstalled coding agent to build an app that
looks and behaves like Archipelago. The experience must work from the dashboard,
local console, and SSH, with the same tools and discoverable commands.

The user requirements are:

- Bring Omarchy's initial setup and development capabilities to Archipelago.
- Ship Codex ready to launch, with guided personal authentication.
- Make closing and resuming terminal sessions easy and reliable.
- Ship local skills that help agents modify the system and build apps using the
  actual developer documentation.
- Make visual and interaction consistency an enforced part of app creation.
- Plan in a separate worktree, without conflicting with current release work.

The companion [agent design system plan](archipelago-agent-design-system-spec.md)
defines the shared UI kit, agent workflow, app templates, and visual acceptance.
That work is a dependency of the app-building experience, not finishing polish.

## Product outcome

On a newly installed node, Terminal opens to a usable shell with a small welcome
panel offering Setup, Resume, Build an app, Work on Archipelago, and Help. The
shell is immediately usable; onboarding is dismissible and resumable. Existing
owners get the same environment through an upgrade that preserves their files,
configuration, credentials, and uninstall decisions.

A representative first session:

1. Open Terminal from the dashboard or launch it locally.
2. See the node, workspace, account, and any existing sessions clearly identified.
3. Run the setup guide, select editor/toolchains, and sign in to Codex.
4. Choose Build an app and describe the app. The agent loads Archipelago's app
   and design guidance and starts from the maintained starter.
5. Review a working local preview, including mobile and failure states.
6. Close Terminal while a build or agent task runs.
7. Reopen it, select the named session, and continue where it was left.
8. Install the candidate through the existing app lifecycle on a chosen test
   node, then contribute through ngit when publication is requested.

## Current implementation and gaps

These are source findings, not live-node acceptance results.

| Surface | Current source | Required change |
| --- | --- | --- |
| Dashboard terminal | `neode-ui/src/components/CLIPopup.vue`: development-only simulated commands; production SSH instructions and a literal default password | Real terminal and session picker; connection information derived from actual setup state |
| Terminal state | `neode-ui/src/stores/cli.ts`: open/close boolean | Server-owned session inventory; client UI state independent of process lifetime |
| Keyboard | `neode-ui/src/App.vue`: global F and other shortcuts; `useModalKeyboard.ts` captures arrows and Escape | A terminal focus boundary so shell, editor and agent keys reach the PTY |
| Console setup | `image-recipe/archipelago-scripts/archipelago-menu.sh`: legacy menu, eager installer-tool installation, direct container setup | Shared command catalog and current orchestration; opening a menu does not install software |
| Shell onboarding | `scripts/welcome-banner.sh`: banner and SSH information, including literal default password; may wait for networking | Short local welcome, accurate access state, fast offline shell startup |
| ISO | `image-recipe/build-debian-iso.sh` invokes a relocated copy of `_archived/build-auto-installer-iso.sh` | Integrate the actual active build path, despite its archived filename |
| Upgrade | `scripts/self-update.sh`, `core/archipelago/src/bootstrap.rs`, runtime assets | One versioned environment payload and repeatable migration shared with ISO |
| Existing development access | Pinned ngit installer and GitWorkshop app already exist | Reuse these; keep ngit the contribution platform |
| App packaging | `docs/app-developer-guide.md`, manifest spec and Rust parser | Turn existing contracts into agent workflows, validated starters and development commands |
| Design | Dashboard CSS/Tailwind, reusable Vue components, separate AIUI CSS | Extract and document a coherent shared contract; see companion plan |

The backend uses Hyper/Tokio and existing WebSocket handlers; the developer guide
currently calls it Axum. Several contributor documents also show unrestricted
`cargo test`, conflicting with `AGENTS.md`. Correct those documentation examples
before packaging them as agent instructions. On a live node, backend execution
must use `scripts/test-backend-isolated.sh`.

## Omarchy reference and capability mapping

Research baseline: the official [Omarchy repository](https://github.com/omacom/omarchy),
formerly reached through `basecamp/omarchy`. Its default branch was `quattro` at
[`fcf9eeb5c454739f3f23cdfd7d57d77b12961025`](https://github.com/omacom/omarchy/tree/fcf9eeb5c454739f3f23cdfd7d57d77b12961025).
The latest published release returned during research was
[v4.0.4](https://github.com/omacom/omarchy/releases/tag/v4.0.4), published September 15,
2026, resolving to `c668141e9c42b13c80c9ca4ea108e11708c5e8a5`.
The inventory below describes the pinned default-branch source; it does not
assert that every item is present in that published release.

“All setup capabilities” means explicit disposition of the setup surface, not
silently omitting desktop-specific features. The proposed core release covers
the terminal, system setup and development rows below. Hardware/desktop options
remain named follow-on work; completion of the core is not full Omarchy parity.

| Omarchy capability | Proposed Archipelago equivalent | Delivery |
| --- | --- | --- |
| Owner, keyboard, hostname, timezone, optional Git name/email | Resume existing node onboarding; configure developer identity separately from appliance identity | Core |
| Deferred provisioning for another owner | Leave personal developer credentials unset; first owner completes setup | Core |
| Separate packaged defaults, user finalization and migrations | Versioned system payload, per-user setup receipts, explicit reset with backup | Core |
| First-login welcome, shortcuts, network/update guidance | Welcome and searchable help shared by terminal and dashboard | Core |
| Bash completion, history, prompt, fuzzy finding and directory navigation | Bash, completion, Starship, fzf and zoxide with readable console fallback | Core |
| File/search/system tools | ripgrep, fd, bat, eza, jq, less, man, tldr, btop and fastfetch | Core |
| Terminal selection: Foot, Alacritty, Ghostty, Kitty | Browser terminal plus console/SSH; local emulator adapters compatible with the actual kiosk display stack | Browser/console core; native choices follow-on |
| tmux sessions, panes, developer layouts | Named persistent sessions and editor/agent/shell layouts; same sessions accessible by SSH | Core |
| Neovim and selectable editors | nano available immediately; maintained Neovim profile and editor preference; GUI editors when desktop support exists | Core terminal editors; GUI follow-on |
| Mise development environments | Versioned Node/npm, Rust, Python/uv profiles first; project-local versions | Core |
| Ruby/Rails, Bun, Deno, Go, PHP/Laravel/Symfony, Elixir/Phoenix, Java, Zig, OCaml, .NET, Clojure, Scala | Optional profiles in the same installer catalog, each with architecture and verification metadata | Parity follow-on |
| Git, lazygit, GitHub CLI | Git/lazygit and ngit/GitWorkshop first-class; gh optional for other upstream projects | Core |
| Lazy agent launchers | Codex actually bundled; optional adapters for other agent CLIs | Core Codex; provider expansion follow-on |
| Default-agent selector and starter prompts | Codex selected initially for a fresh setup; preserve existing preference; system/app/design entry prompts | Core |
| Agent account selection and usage panel | Explicit account context and supported login status; manual account profiles before considering usage automation | Follow-on |
| Local model tools | Integrate the existing Ollama app and compatible provider setup; downloads and model resource needs visible | Follow-on |
| Bundled system and app-building skills | Archipelago system, app and design skills with offline references and templates | Core |
| Development databases: MySQL, PostgreSQL, Redis, MongoDB, MariaDB, MSSQL | Scoped rootless Podman development recipes, isolated ports/data and generated credentials | Core PostgreSQL/Redis-compatible recipe; remaining recipes follow-on |
| Container tooling | Existing rootless Podman; documented Compose compatibility where tested | Core |
| DNS, Wi-Fi, network QR, SSH daemon and SSH agent setup | Existing network/SSH controls exposed through shared setup operations, with accurate connection details | Core |
| Fingerprint, FIDO2 and privilege preferences | Hardware-aware account-security setup; no copied Arch PAM configuration | Follow-on |
| Monitors, keyboard bindings, input, XCompose | Kiosk/console equivalents through existing system configuration; desktop-specific adapters separately | Core console basics; desktop follow-on |
| Browser, terminal, editor and dictation defaults | Editor/agent/terminal choices first; browser/dictation surfaced when relevant to the device | Core subset; follow-on adapters |
| Themes, fonts, background and prompt | Shared Archipelago tokens and terminal palette; preserve owner customization | Core |
| Shell plugins and customization hooks | Versioned extension points and documented user overrides; dashboard extensions require a separate supported contract | User overrides core; plugins follow-on |
| Package, TUI, web-app and development installation menus | Curated tool catalog plus existing Archipelago app catalog; distinguish developer tools from managed apps | Core |
| Commercial services, GUI apps, gaming, Windows VM | Individual optional app/desktop integrations, inventoried as a separate parity backlog | Follow-on, not preinstalled on nodes |
| Update, reset, snapshots, direct boot | Integrate Archipelago update/recovery choices; explicit reset scope and backup | Core existing operations; new boot/snapshot features follow-on |
| Hardware detection and vendor fixes | Existing Archipelago hardware configuration, capability detection and separately qualified device fixes | Core detection; device adapters follow-on |
| Crash diagnosis skill | Sanitized diagnostics with an explicit handoff to the chosen agent; preserve source versus live evidence | Follow-on |

Primary source routes for the inventory:

- [Provisioning and file layout](https://github.com/omacom/omarchy/blob/fcf9eeb5c454739f3f23cdfd7d57d77b12961025/docs/file-layout.md),
  [setup form](https://github.com/omacom/omarchy/blob/fcf9eeb5c454739f3f23cdfd7d57d77b12961025/install/provisioning/setup-form.sh),
  [setup and installation menu](https://github.com/omacom/omarchy/blob/fcf9eeb5c454739f3f23cdfd7d57d77b12961025/default/omarchy/omarchy-menu.jsonc).
- [Base packages](https://github.com/omacom/omarchy/blob/fcf9eeb5c454739f3f23cdfd7d57d77b12961025/install/omarchy-base.packages),
  [shell initialization](https://github.com/omacom/omarchy/blob/fcf9eeb5c454739f3f23cdfd7d57d77b12961025/default/bash/init),
  [terminal and tmux](https://github.com/omacom/omarchy/blob/fcf9eeb5c454739f3f23cdfd7d57d77b12961025/manual/15-terminal.md).
- [Development profiles](https://github.com/omacom/omarchy/blob/fcf9eeb5c454739f3f23cdfd7d57d77b12961025/bin/omarchy-install-dev-env),
  [agent setup](https://github.com/omacom/omarchy/blob/fcf9eeb5c454739f3f23cdfd7d57d77b12961025/install/user/mise.sh),
  [agent launcher implementation](https://github.com/omacom/omarchy/blob/fcf9eeb5c454739f3f23cdfd7d57d77b12961025/bin/omarchy-mise-install),
  [development databases](https://github.com/omacom/omarchy/blob/fcf9eeb5c454739f3f23cdfd7d57d77b12961025/bin/omarchy-install-docker-dbs).
- [System skill](https://github.com/omacom/omarchy/blob/fcf9eeb5c454739f3f23cdfd7d57d77b12961025/default/agents/skills/omarchy/SKILL.md)
  and [app skill](https://github.com/omacom/omarchy/blob/fcf9eeb5c454739f3f23cdfd7d57d77b12961025/default/agents/skills/omarchy-app/SKILL.md).

Adapt the workflows to Debian and Archipelago's manifest-driven runtime. In
particular, Omarchy's app skill produces Qt desktop apps; Archipelago's starter
must produce an Archipelago app. Omarchy's agent launchers download tools on use;
the user's requirement here is stronger: Codex must already be installed. Its
database recipes and permission-changing aliases are not defaults to transplant.
Any reused upstream files need their original license notices and provenance.

## Terminal experience

Use a real PTY with a bundled terminal renderer, proposed `@xterm/xterm` with
fit/search support. It must run Bash, nano/Neovim, lazygit, tmux and Codex without
special simulated command handling. Exact dependency versions are selected and
locked during implementation qualification.

The dashboard entry opens a large resizable terminal on desktop and a full-height
surface on mobile. It provides session name, workspace/node context, connection
status, session switcher, New, Resume, Search, copy/paste, font size, fullscreen,
and Close. End session belongs in a separate menu with a clear running-work warning.
Mobile adds an Esc/Ctrl/Tab/arrows key strip and works with the software keyboard.

Use the shared Archipelago shell/components around an opaque, readable terminal
canvas. Terminal content uses a packaged monospace font with Unicode support;
the console gets an ASCII-compatible fallback. Apply the design plan's focus,
contrast, reduced-motion and safe-area rules.

While terminal input has focus, F, Escape, arrows, Ctrl+C, Ctrl+D, Ctrl+R, Tab,
editor keys and tmux prefixes belong to the terminal. Closing uses a visible
control or a documented terminal-specific shortcut. Do not reuse the current
modal keyboard handler unchanged. Ctrl+C interrupts the foreground job; Ctrl+D
has normal shell semantics. Focus can move to toolbar controls and back.

## Persistent sessions and one-click resume

This is a core release criterion, including the first usable terminal milestone.

Session execution lives on the node in a dedicated, supervised user environment.
Use tmux as the initial persistence engine; a browser WebSocket is an attachment,
not the owner of the shell process. Keep the tmux server and worker scope outside
the dashboard/backend service's kill group. Ordinary manager deployment must not
tear down developer sessions.

Prefer one tmux server/socket and supervised process scope per managed session,
so ending one session cannot kill a shared server containing other work. Keep
those sockets private to the developer account. SSH and local-console resume use
the same session registry and attachment helper rather than guessing tmux names.
Cross-device resume refers to browsers connected to the same node; moving live
processes between different nodes is outside this contract.

| Event | Required behavior |
| --- | --- |
| Close terminal panel, navigate away, close tab, kill browser | Detach; shell, build and agent continue |
| Reopen Terminal | Show running and recent sessions; Resume most recent available in one action; do not auto-create duplicates |
| Switch session | Detach the old view and attach the selected session |
| Network drop or mobile sleep | Show Reconnecting; reauthenticate if needed, then attach to the existing session |
| Dashboard reload or backend restart | Session survives; rediscover it from node state |
| Second browser/device | Same authenticated owner sees sessions; explicit transfer of input control |
| Logout/session expiry | Revoke browser attachments immediately; background work remains; a fresh owner login is required to reattach |
| Explicit End session | Confirm when work is active, terminate that session's process scope, record ended state; do not remove its files |
| Shell exits | Show ended status and exit information where available; offer a new shell in that workspace |
| Broker crash | tmux/process scope survives if its supervisor survives; reconcile inventory, without replaying input |
| Node reboot/power failure | Processes stop. Keep workspace/session metadata and offer Reopen workspace and Resume Codex conversation; never label this a live process resume |

The picker shows a user-editable name, workspace, creation/last-attachment time,
Running/Detached/Ended/Interrupted state, and optional pinned status. Scope every
entry to a node and stable owner identity. LocalStorage may remember selection;
it is never the authoritative session registry.

Persist metadata atomically under the workspace account's private state directory.
Record schema version, opaque session ID, owner ID, node ID, boot ID, tmux target,
workspace ID/path, created/attached timestamps and lifecycle state. Treat tmux and
supervised process state as authority for whether a session is still alive.
After a crash, reconcile orphan sessions and interrupted creates before accepting
another create with the same request ID.

Place workspaces and agent state on durable storage with an explicit ownership,
quota and backup policy; do not consume a small system partition accidentally.
Expose a familiar `~/Work` entrypoint while recording the actual workspace root.
Reconnecting never requires a fresh Git checkout. A missing or unmounted workspace
is a visible recovery state, not permission to create an empty replacement at the
same path. Deleting session metadata and deleting project files are distinct actions.

Keep bounded in-memory scrollback and restore the current terminal screen from
the surviving tmux attachment. Do not assume client-side scrollback survived.
Reboot persistence covers metadata and saved files; transcript recording is a
separate opt-in setting because terminal output can include credentials.

Initial proposed limits: eight sessions per owner, one active input controller
per session, a 10,000-line scrollback ceiling plus a byte ceiling, and bounded
per-connection queues. Do not kill detached work because a browser idle timer
expired. Make resource use visible and allow explicit stop/cleanup. Qualify CPU,
memory and disk limits on the smallest supported node before choosing defaults.

Input is never automatically replayed after reconnect: a lost acknowledgement
does not establish whether Enter or a command reached the shell. Resize events
are idempotent. A newly attached terminal redraws from the current PTY state.
Revoke the previous writer before granting a new writer lease; read-only views
can be a later addition. User labels and working directories must never be
interpolated into shell command strings.

## Execution and connection architecture

Proposed components and responsibility boundaries:

```text
Dashboard terminal entry / SSH / local console
                  |
       authenticated attachment
                  |
Terminal session service -- session metadata and ownership
                  |
  supervised developer user + tmux + PTY
                  |
  Bash / editor / Codex / project toolchains
                  |
  archy CLI -- existing typed management operations
```

Add a small terminal session service with a local Unix-socket control interface.
The existing management backend validates owner authorization and brokers
short-lived attachments; it does not execute arbitrary shell strings in an RPC
handler. Use typed arguments and fixed executable paths for session creation.
An implementation spike must verify PTY allocation, systemd ownership, tmux
reattachment and Codex rendering before committing to the broker library.

Prefer a dedicated developer Unix account, separate from the existing
`archipelago` service account, with its own home, rootless container storage and
no mounts of production wallets/secrets. Keep the current service account and
data ownership intact. The inspected ISO builder grants the service account broad
passwordless sudo; using that account for a browser shell would grant equivalent
host authority. A new developer account alone is not proof of isolation: test
actual filesystem permissions, sockets, groups and sudo policy.

Provide an explicit System administration context for system changes. Routine
supported operations should use the same typed management operations as the UI;
arbitrary host-shell administration needs a distinct authenticated operator
session. These boundaries must still permit authorized agents to configure the
system efficiently. Do not require repeated consent for each step of an already
authorized operation, or treat a skill as an access-control mechanism.

Initial web access is for the node owner. Public app sessions, guests, peer
identities and iframe signers do not imply terminal authorization. Multi-user
workspace sharing is outside the first release; reject unsupported mappings.

Prefer a dedicated terminal browser origin with a minimal self-hosted bundle,
no app iframes or external scripts, and narrow authenticated handoff from the
dashboard. A same-origin subpage reduces bundle complexity but does not isolate
it from same-origin scripts. Resolve origin, certificate and Companion handling
in the connection spike before shipping. The
[xterm.js integration guide](https://xtermjs.org/docs/guides/security/) specifically
requires application-level WebSocket authentication/origin handling and careful
treatment of terminal output.

Remote terminal transport requires verified HTTPS/WSS. Existing plain-HTTP
dashboard users get a working secure-terminal entry and SSH alternative; do not
force a global dashboard HTTPS migration as a side effect. Retain current private
management ingress controls, IPv6 support and explicit proxy trust. Never infer
terminal authorization from a forwarded hostname or a private IP alone.

The protocol contract must include:

- Create/list/rename/end session operations with CSRF checks, stable owner
  binding and idempotent create/end behavior.
- A single-use, short-lived attachment grant bound to session, owner and exact
  terminal origin. No reusable dashboard cookie or API key in a query string.
- An authenticated WebSocket with bounded pre-auth time and no PTY output before
  authorization; validate Origin separately from CORS and revalidate on reconnect.
- Input bytes, output bytes, resize, connection state and exit messages; bounded
  frame sizes, queue backpressure and slow-reader behavior.
- Immediate attachment revocation on logout/credential revocation. No automatic
  input replay, automatic command rerun or unauthenticated reconnect.
- Process-group cleanup only for explicit termination; metadata-only audit logs,
  excluding command contents, keystrokes, credentials and terminal output.
- Terminal titles/links treated as untrusted text; external links require a user
  gesture; clipboard escape sequences cannot silently read/write the clipboard.

## Setup and tool distribution

Provide a searchable `archy` CLI and matching setup TUI; the existing
`archipelago` executable remains the backend daemon. All `archy` commands in
this document are proposed interfaces, not commands available today.

| Proposed command | Purpose |
| --- | --- |
| `archy setup` | Resume setup, show installed/available/deferred items |
| `archy commands --json` | Agent-readable command descriptions, inputs, privileges and side effects |
| `archy doctor` | Read-only, bounded health checks with actionable findings |
| `archy session list` / `resume <id>` | Discover and attach to existing work |
| `archy dev setup <profile>` | Install a declared, versioned toolchain profile |
| `archy agent` | Launch selected agent in the selected workspace |
| `archy app new <id>` | Create from a maintained Archipelago starter |
| `archy app check <path>` | Manifest, build, integration and design checks |
| `archy app preview <path>` | Start an isolated local preview and return its URL |
| `archy app install <path> --node <target>` | Explicit candidate deployment through supported orchestration |
| `archy system <operation>` | Discoverable adapters to supported system operations |
| `archy skills status` | Show installed skill/doc/kit versions and local overrides |

Use one command catalog for the CLI, setup menu and skill references. Each entry
declares what it reads/changes, required privilege, supported machines, expected
output, progress and rollback/recovery behavior. Structured status is for agents;
the human menu uses clear task names. Missing capability is a visible explanation.

First-run sequence: verify network/time/access status; create or select a workspace;
set optional Git identity and editor; confirm installed tools; offer toolchain
profiles; sign in to Codex; offer a sample app or system task. Users can skip and
resume individual steps. Authentication is never an ISO build step. Run nothing
interactive in noninteractive shells, SCP/SFTP, remote command execution or CI.

Ship offline: shell essentials, tmux, Git/ngit, Codex binary, local documentation,
skills and design-kit assets. Developer profiles add the complete Node/frontend,
Rust/backend or Python environment and build prerequisites. Offline capability
must be stated precisely: shell/docs/Codex launch can work offline; model calls,
uncached dependencies and external login require connectivity. Plan a full
offline developer bundle as a separate profile if all build caches are required.

Use signed/versioned payloads and per-architecture hashes. Debian packages and
user toolchains have separate ownership. Core tools update through qualified
releases; opening a shell or running `codex` must not silently update binaries.
Optional tool installs show download size, source, version and progress. Respect
package-manager locks and existing mise/rustup/nvm installations. Do not prune a
binary version beneath a running session.

State is versioned per machine and per user with pending/running/done/failed/skipped
steps; write completion only after verification. Concurrent setup is serialized.
Preserve user dotfiles through small managed includes and explicit overrides;
preview changes and back up touched configuration during an explicit reset.
Network interruption, disk exhaustion and reboot leave resumable state.

## Codex integration

Bundle a qualified stable Codex release for each supported architecture, verified
against a pinned artifact. The official
[CLI installation documentation](https://learn.chatgpt.com/docs/codex/cli)
documents standalone and npm installation; the release implementation should
resolve exact packaging and pin it rather than executing an unversioned installer
on every node.

Use normal Codex authentication in the developer user's private environment.
Support browser login and the official device-code option for remote/headless
sessions when available; API-key login is another explicit option. The
[authentication documentation](https://learn.chatgpt.com/docs/auth) describes
these flows. Do not collect credentials in an Archipelago transcript or copy the
node's wallet identity into agent configuration. The UI reports Installed,
Sign-in required, Ready or Error based on actual results.

Honor the user's Codex configuration, permissions and model choice. A launcher
selects a workspace and skill context; it does not inject unrestricted execution
flags. A local skill is local guidance, not a claim that Codex inference is local.
Explain the selected provider and what workspace content may be sent to it during
setup, alongside any self-hosted alternative.

During an ordinary disconnect, resume the same running Codex process in tmux.
After an ended session or reboot, offer the official `codex resume` workflow in
the matching workspace. Do not automatically reissue the last task. Preserve
agent history/configuration separately from disposable build caches, and keep
credentials out of ordinary app export/support bundles.

## Local skills that enable real work

Ship a small coordinated skill family with one obvious entrypoint. Keep the
instructions actionable and references focused; avoid copying the whole manual
into every prompt. The skill-creator guidance informs this structure: precise
triggers, reusable resources, progressive disclosure and behavioral validation.

| Skill | Trigger and outcome | Required resources |
| --- | --- | --- |
| `archipelago` | Configure, troubleshoot or change an Archipelago system; route app/UI work to the companion skills | System map, command catalog, config ownership, task recipes and recovery |
| `archipelago-app` | Build, package, test or update an Archipelago app using the developer contract | Developer docs, manifest schema, starter assets, launch/auth/signer examples, lifecycle checks |
| `archipelago-design` | Create or change an Archipelago UI, including apps, setup and Terminal | Shared tokens/components, pattern gallery, app shells, screenshots and visual checks |

For a system task, the agent should locate the correct config/operation, inspect
current state, make the requested scoped change, validate it and report the result.
Teach recipes for networking, SSH, tool installation, terminal preferences,
service diagnosis and supported app configuration. Prefer maintained management
commands; when direct configuration is necessary, explain owned versus generated
files, the minimal affected service and reversal. Repository changes go into a
separate branch/worktree; installed-node changes target an explicitly identified
node and retain a scoped backup. Planning requests stay planning requests.

For an app task, the agent reads `docs/app-developer-guide.md` and
`docs/app-manifest-spec.md`, follows the design plan, chooses a starter, implements
the requested behavior, validates it and supplies a runnable preview. Package
with pinned images/build contexts, rootless execution, declared storage/secrets,
health checks and truthful interfaces. Include HTTP/HTTPS, iframe/Companion,
first-run credentials and Nostr signer behavior where relevant. Publication and
live installation are distinct requested steps.

The skill must explain non-obvious runtime facts: manifests copied only into
`/opt/archipelago/apps` are replaced at backend start; runtime payload promotion
and signed-catalog precedence matter. Installed status is not launch readiness.
Pre-catalog testing must not replace the signed catalog. Test uninstall/reinstall
with data preservation on a disposable target. Use `AGENTS.md` over stale testing
examples. Payments, wallets and uninstall decisions retain their existing
invariants; a generic repair must not reset them.

Proposed source layout: `skills/archipelago*` plus a versioned reference bundle and
app starter assets. Ship managed copies under a versioned read-only system path
and expose one discoverable link per skill per agent. Current official
[Codex skill guidance](https://learn.chatgpt.com/docs/build-skills) supports
repository `.agents/skills`, user `~/.agents/skills`, administrator
`/etc/codex/skills`, and symlinked skill directories. Qualify discovery with the
pinned CLI, including from outside this repository. Preserve local skills and
overrides; avoid duplicate skill names from multiple discovery paths.

Each bundle records the source revision, supported Archipelago version, manifest
schema and design-kit version. Installed skills use matching offline docs;
repository development uses that checkout's docs. Version mismatch is visible.
Future command names in this plan must not be taught as available until shipped.

## App creation and design integration

The default new app is a Vue/TypeScript app using the shared Archipelago UI kit,
with a pinned container build, manifest, icon, tests and local preview. Supply a
framework-neutral CSS/token starter for existing non-Vue apps; port semantic
behavior deliberately instead of depending on dashboard globals.

Provide a complete example app with a useful list/detail/settings flow, persistent
data, loading/empty/error/success states, a manifest and lifecycle evidence.
Extend it with optional signer/media examples only when those features are used.
The starter must render correctly both embedded and standalone. A third-party
upstream app can retain its own UI; the consistency contract governs the app's
Archipelago wrapper and newly authored Archipelago screens.

Local development data and ports are separate from production apps. Bind previews
to loopback by default and provide an authenticated preview path for remote users.
Development databases get unique project-scoped names, persistent volumes and
credentials; they must not attach to live Bitcoin/LND or production app databases.

## Delivery sequence

| Phase | Deliverable | Exit evidence |
| --- | --- | --- |
| 0 | Finalize command/session contracts, origin/account choice and design baseline | Reviewed wireframes, reference screens, privilege map and pinned Omarchy inventory |
| 1A | Persistent terminal service and browser/SSH attachment | Real shell + Codex TUI; close/reopen, network loss, manager restart and ownership tests |
| 1B | Shared design foundation, gallery and starter | Dashboard-derived tokens/components; consistent app at desktop/mobile sizes |
| 2 | Core setup/tool payload and Codex onboarding | Fresh/offline/upgrade/retry matrix; correct account state and user override preservation |
| 3 | System/app/design skills and app-development commands | Realistic agent tasks produce correct scoped system changes and a consistent packaged app |
| 4 | Integrated candidate qualification | Real node and Companion resume/design acceptance, resource tests and packaged ISO/OTA checks |
| 5 | Remaining Omarchy parity adapters | Each inventory row supported or explicitly retained with reason and acceptance target |

1A and 1B can be independent implementation workstreams once phase 0 contracts
are agreed. This planning session has not launched implementation agents.
App-generation functionality is incomplete until 1B and the skill acceptance pass.

Expected code boundaries: Terminal UI/store/keyboard handling; new terminal
service and typed management routes; shared CLI/setup catalog; versioned tool and
skill packaging; design-kit extraction; app starter/validation; ISO and OTA
integration. Keep each in a focused contribution. Coordinate shared frontend
styles, backend routing and packaging files before implementation starts.

## Acceptance criteria

| ID | Required proof |
| --- | --- |
| TERM-01 | Real PTY supports editors, completion, colors, Unicode, signals and Codex; no fake production commands |
| TERM-02 | Start a long-running fixture and edit a file, close Terminal/tab/browser, reopen and resume the exact session and process |
| TERM-03 | Offline/reconnect, mobile sleep, frontend reload, backend restart and attachment-service restart do not duplicate execution |
| TERM-04 | A second device resumes after owner authentication; writer transfer is atomic; another identity cannot list/attach/terminate |
| TERM-05 | End session stops only its process scope; logout revokes access; neither operation deletes workspace files |
| TERM-06 | Reboot retains workspace metadata, labels interrupted sessions accurately and offers Codex conversation resume without rerunning commands |
| TERM-07 | Terminal keyboard ownership, focus, text selection, paste, resize, mobile keyboard and screen-reader mode work |
| AUTH-01 | Missing/expired/replayed grants, cross-origin sockets, forged proxy headers and app/guest credentials fail before shell I/O |
| AUTH-02 | Developer user cannot read production wallets/secrets or control production container sockets; authorized system workflow works |
| SETUP-01 | Fresh install and existing-node upgrade deliver the same core capabilities; Codex version works before network access |
| SETUP-02 | Failed download, package lock, low disk, reboot and concurrent setup recover without false completion or broken existing tools |
| SETUP-03 | Existing dotfiles, editor/agent choices, credentials, app data and uninstall decisions are preserved |
| AGENT-01 | A system-change task uses the correct operation/config, verifies its result and preserves unrelated services |
| AGENT-02 | An app-building task follows actual developer docs and passes manifest/build/launch/lifecycle checks |
| DESIGN-01 | Agent-built apps satisfy the companion design plan using shared assets/components, including failure and mobile states |
| PKG-01 | Exact candidate OTA and ISO contain matching tools, docs, skills and UI-kit versions; restore previous payload without removing user work |

Backend unit execution uses `scripts/test-backend-isolated.sh`. PTY/process and
account-isolation integration tests run in disposable users/VMs, not a funded
production node. Terminal continuity tests use observable process IDs/output and
file checks, not a mocked “resumed” label. UI tests include 320/390/768/1440 pixel
layouts, landscape, enlarged text and actual Companion input on a device.

Record source tests, disposable integration, actual-node acceptance and packaged
artifact results separately. Preserve unfinished requirements in
`docs/post-1.8.22-regressions-20261001.md` and the current release acceptance ledger;
this feature plan closes none of them. Later publication follows ngit review and
merge, then identical accepted main/tag objects on both ngit and Gitea, with the
required mirror checks. No release version or publication date is reserved here.

## Decisions to resolve during design review

The proposed defaults are a dedicated developer account, tmux persistence,
minimal terminal origin, bundled Codex, Bash, and a Vue starter with portable
design tokens. The implementation review must settle terminal-origin/certificate
handling on every supported ingress, the supported operator-shell model, the
exact first-release tool profile, and the approved visual baseline. Native
terminal emulators and the broader desktop parity backlog need separate device
compatibility decisions. None of these questions prevents reviewing this plan.
