# Archipelago design system for agent-built apps

Status: planning draft, 2026-10-09. Companion to the
[terminal and developer environment plan](terminal-developer-environment-spec.md).
No tokens, components, templates or installed skills are changed by this draft.

An agent asked to build an Archipelago app should produce an app that belongs in
Archipelago by construction. Give it a maintained component kit, runnable
starters, documented screen patterns and visual references. A skill telling it
to “use glassmorphism and orange” is insufficient.

The same foundation should drive Terminal, setup screens, first-party apps and
newly generated apps. Existing third-party applications keep their own product UI;
their Archipelago entry, launch and integration surfaces follow this contract.

## Evidence and design authority

At repository baseline `2cb1bae5`, the dashboard's design is distributed across
`neode-ui/src/style.css`, `neode-ui/tailwind.config.js`, Vue components, and the
short standards section of `docs/developer-guide.md`. AIUI maintains a separate
token/style definition in `aiui/packages/app/src/styles/main.css`.

| Existing rule | Source observation | Contract to preserve or resolve |
| --- | --- | --- |
| Dark controls | Dashboard sets `color-scheme: dark` and explicit select colors | Dark baseline, including native controls |
| Glass cards | `.glass-card`: black at 0.65 alpha, white border at 0.18, 16px radius | Preserve the established surface, not a generic frosted card |
| Blur | Default card blur 18px; dashboard contexts deliberately suppress backdrop blur to avoid rendering corruption | Context-aware surface variants; never reintroduce blanket blur |
| Buttons | `.glass-button`: 44px minimum, 12px radius; established focus/hover/pressed variants | Reusable buttons with full interaction states |
| Accent | Dashboard focus/action uses `#fb923c`; AIUI accent is `#F7931A` | Named semantic tokens with explicit mapping; do not silently choose one for everything |
| Typography | Dashboard body uses Avenir Next/system fallback, headings have bundled Montserrat; AIUI uses Inter/system fallback | Approved roles, metrics, fallback behavior and licensed assets |
| Spacing | Tailwind extends a 4px grid | Shared spacing scale and layout examples |
| Search | Existing search is 40px on desktop, 52px below the 920px breakpoint; right clear control retains focus | Shared search component and documented responsive behavior |
| Card actions | Existing CSS places full-width actions at the bottom on desktop and mobile | Preserve this layout in generated card screens |
| Modal layout | `BaseModal.vue`: pinned title/footer with independently scrolling content | Shared dialog shell with tested focus and Back behavior |
| Mobile layout | Dynamic viewport, safe-area and audio-player offsets already exist | One documented inset contract; prevent double padding in embedded apps |
| Success | Shared `PaymentSuccessPane.vue` and `IdentitySuccessPane.vue` express branded completion | Reuse the relevant pattern only when its completion condition is actually met |
| Embedded canvas | AIUI embedded mode is transparent; standalone has its own background | Host owns wallpaper; apps choose explicit embedded/standalone mode |

These observations seed a visual audit; they do not make every legacy style a
rule. Capture representative current screens in the running supported UI before
extracting components. Approve Home, Apps, app detail, Settings, forms/dialogs,
list/detail screens, Terminal and mobile examples as reference baselines.

The source of truth becomes versioned tokens plus components and pattern docs.
Screenshots illustrate that contract and detect drift; they do not replace it.
Existing dashboard and AIUI differences need explicit migration decisions.
Preserve the current look first and make intentional design improvements visible
in review rather than introducing them incidentally through an app template.

## Deliverable shape

Proposed layout, to be finalized against repository packaging conventions:

```text
packages/archipelago-design/
  tokens/                 semantic definitions and generated CSS/JSON
  styles/                 scoped foundations and non-Vue component classes
  assets/                 approved fonts, icons and licenses
packages/archipelago-ui/
  components/             Vue components with documented states
  patterns/               app shells and composed screen patterns
docs/design-system/
  index.md                decision guide and version compatibility
  foundations.md          typography, color, surfaces, spacing, motion
  components.md           usage, states, accessibility, stable imports
  patterns.md             complete screen composition and behavior
  examples/               reference captures and their matching source
examples/archipelago-app/
  ...                     working starter with manifest and tests
skills/archipelago-design/
  SKILL.md                concise trigger, routing and workflow
  references/             versioned design index and evaluation rubric
```

Build an offline component gallery with the UI kit. Every example links to its
source, token usage and applicable app pattern. A developer can run the gallery
without a node session or provider API key. It must show loading, empty, error,
success, disabled, focused, hovered and pressed states, not only the ideal screen.

Publish packages only as part of an explicitly authorized later implementation
workflow. Until then these names and paths describe proposed deliverables.

## Token contract

Use framework-neutral CSS custom properties generated from a single definition.
Vue components and any Tailwind adapters consume the same values. Prefix tokens
with `--archy-`; do not expose arbitrary dashboard-global selectors to apps.

| Token family | Required coverage |
| --- | --- |
| Color | Canvas, card, inset, overlay, border, text primary/secondary/muted, interactive accent, focus, selected, success, warning, danger and info |
| Typography | Body, heading and monospace families; type scale, weights, line heights and numeric alignment |
| Space and size | 4px-based scale, page gutters, content widths, control heights, touch targets and icon sizes |
| Shape | Card, dialog, input, button and pill radii |
| Elevation | Card/overlay shadows, border strengths, blur-permitted and no-blur surfaces |
| Motion | Durations/easing, permitted hover/press transitions and reduced-motion equivalents |
| Layering | Header, navigation, menu, popover, modal and toast layers with ownership rules |
| Viewport | Host-provided safe areas, keyboard viewport, navigation/audio offsets and embedded mode |
| Terminal | Canvas, foreground, cursor, selection and a readable 16-color ANSI palette |

Make brand orange and interactive/focus orange distinct roles if preserving both
existing values. Status colors must communicate meaning alongside text/icons.
Name action variants by purpose: the existing orange “warning” class also serves
primary actions, so exporting that name unchanged would teach the wrong semantics.

Typography must have predictable dimensions on Debian, Android and desktop
browsers. Use only redistributable local font assets. Referencing Avenir or Courier
New in a fallback stack does not license bundling those fonts. Freeze a tested
body/heading/mono choice and line-height matrix in the visual review; do not let
each generated app choose its own fonts or fetch a font CDN.

Check contrast on actual composited surfaces and the brightest/darkest supported
backgrounds. Opacity values or CSS comments alone do not establish accessibility.
Small text must remain readable, and focus must remain visible where current
global styles suppress outlines. Low-power/no-backdrop-filter fallbacks retain
the same hierarchy and usable contrast.

## Components and patterns agents can reuse

Extract reusable behavior from existing code; avoid giving agents a second
lookalike library with slightly different interactions. Package components
without dashboard stores, router assumptions, privileged RPC clients or secrets.
Use adapters for integrations owned by the host.

| Foundation | First required components | Existing reference |
| --- | --- | --- |
| Structure | AppShell, PageHeader, Section, Card, InsetPanel, ActionRow | Dashboard CSS, app headers, card action rules |
| Controls | Button, IconButton, TextField, PasswordField, Select, Toggle, SearchField, SegmentedControl | `ToggleSwitch.vue`, `PasswordRevealInput.vue`, `AppSearchField.vue` |
| Navigation | Tabs, Breadcrumbs, Back action, contextual menu | Dashboard navigation, `BackButton.vue`, Cloud menus |
| Feedback | StatusBadge, EmptyState, Skeleton, Progress, InlineError, Toast | `EmptyState.vue`, `SkeletonCard.vue`, `ToastStack.vue`, existing progress patterns |
| Dialogs | Dialog, ConfirmDialog, credential handoff | `BaseModal.vue`, `AppConfirmModal.vue`, `AppCredentialInterstitial.vue` |
| Data | ListRow, responsive table/list, detail key/value row, copy action | Existing app lists, `CopyButton.vue` |
| Terminal | TerminalShell, SessionList, SessionRow, ConnectionStatus, mobile key strip | New consumers of the same foundation |

Each public component documents supported props/events/slots, sizing, keyboard
behavior, accessibility, states and examples. Shared helpers must respect context:
the current modal helper captures Escape and arrow keys, which is unsuitable
for a terminal, editor or other widget that owns those keys. Fix/reuse that
behavior through explicit contracts rather than blindly wrapping Terminal in it.

Provide composed patterns for:

- A searchable collection with filters, list/grid view, empty state and bottom
  card actions.
- A detail screen with back navigation, metadata, status and primary action.
- Settings grouped by task, with visible validation, save progress and retry.
- A multi-step setup flow with skip/resume, honest progress and recovery.
- First launch and credential handoff, including apps with their own login.
- A destructive confirmation that names its target and data-preservation effect.
- A session picker and terminal that clearly distinguish Close from End session.

Copywriting is part of the contract: concise task labels, useful error recovery,
explicit pending versus complete states, no raw internal exception as the sole
user message, and appropriate units/time formatting. Technical detail is
available on demand. App-generated progress must reflect actual work.

## App shells and host integration

Supply a preferred Vue/TypeScript starter and a small portable HTML/CSS example.
Existing React or other framework applications consume the tokens/styles and
behavior contracts; do not require a framework rewrite to package an app.
Full additional framework bindings follow actual demand and have their own
acceptance, rather than presenting CSS classes as equivalent accessible widgets.

New starters include local assets, locked dependencies, scripts, manifest,
container definition, health check, persistent-data example, tests and a complete
reference screen. A starter must build and run from its copied location without
monorepo-only aliases. It cannot require fetching current dashboard CSS at runtime.

The host integration contract defines:

- Explicit standalone versus embedded appearance. Embedded apps use a transparent
  canvas where appropriate; they do not add a duplicate wallpaper or navigation
  shell. Standalone mode still has a complete, readable canvas.
- Versioned, non-secret theme/inset messages if runtime synchronization is needed.
  Validate sender origin, source window and payload; no wildcard privileged
  message bridge. An app must remain usable with no host handshake.
- Exactly one owner for top/bottom safe-area padding and fixed player/navigation
  clearance. Test the actual Companion WebView, not just a mobile screenshot.
- Existing launch interfaces and app-gate behavior from the developer docs.
  Asset URLs work under their supported base path and HTTP/HTTPS entry points.
- Nostr signer, media controls and credential handoff only through their supported
  APIs. Visual kit installation does not give an app host management access.

Package tokens/components with the app's pinned kit version. Evolve them with
documented compatibility and migrations. Runtime theme values may be forwarded
through the agreed contract; runtime JavaScript/CSS replacement is not the update
mechanism. A platform update must not silently break every installed app.

## Agent workflow

The app skill always routes new UI work through `archipelago-design`. The system
skill does the same when a system modification affects a visible screen. A
backend-only request loads no unnecessary design material.

For relevant tasks the agent:

1. Reads the design index, supported kit version and matching pattern; identifies
   existing components before creating new ones.
2. Inspects the reference screen and its source, then selects the closest starter.
3. Implements the requested behavior using shared components and semantic tokens.
4. Exercises normal, loading, empty, invalid, failed and successful states.
5. Renders standalone and embedded previews at the required viewports; inspects
   captures, keyboard behavior and actual component geometry.
6. Runs app checks and supplies the working preview, component/token provenance,
   results and any deliberate design exceptions.

Make that loop easy: `archy app new`, `archy app preview` and `archy app check`
should supply the starter, preview fixtures and relevant checks. Those are
proposed tools in the terminal plan, not currently implemented commands.
Ordinary changes within the requested task proceed through this loop without
requiring approval for every edit. A new shared design primitive or intentional
departure is identified in the review, with its use case and visual evidence.

The skill should tell agents how to find and use the design system, not make
them recreate it from a long list of adjectives. Keep the entrypoint concise;
conditional references cover forms, layout, terminal interaction, media and
signer flows. Templates and screenshots belong in reusable assets, not prose
that the agent has to transcribe.

## Design conformance and definition of done

Combine targeted static checks, interaction tests and visual review. Static
checks cannot prove a coherent design; screenshots cannot prove an accessible
or correct interaction.

| Gate | Acceptance |
| --- | --- |
| Shared foundation | New UI imports the approved kit/version; bespoke color/font/radius/z-index values need a named exception, with allowances for content such as charts and user imagery |
| Visual hierarchy | Reference-matched page width/gutters, type roles, card surfaces, action placement and density |
| Responsive layout | 320, 390, 768 and 1440px widths plus phone landscape; 200% text zoom; no accidental horizontal page overflow |
| Input | Keyboard-only use, visible focus, sensible tab order, labels, dialog focus containment/restoration and Back behavior; terminal keys remain intact |
| Touch | 44px usable targets for new mobile controls, reachable persistent actions, no essential hover-only interaction |
| States | Loading, empty, offline, denied, validation error, retry and success demonstrated with deterministic fixtures |
| Appearance | Dark native controls, foreground/background contrast, reduced motion, no-blur fallback and locally available fonts |
| Host modes | Embedded/standalone, HTTP/HTTPS where supported, software keyboard, safe areas, audio-player offsets and Companion |
| Runtime truth | Success and readiness only follow actual confirmed results; theme/state changes do not erase form or session work |
| Regression | Shared component changes compared against representative dashboard and app reference captures before release |

Choose per-component visual-diff tolerances after baseline capture; avoid a
single permissive threshold that conceals layout regressions. Stabilize fonts,
viewport, fixture data and animations. Review intended baseline changes rather
than automatically accepting new screenshots when CI fails.

Behavioral skill evaluation should use realistic tasks with no hidden design
brief: build a bookmarks app, add a settings form, create an app with first-run
credentials, adapt a non-Vue app, and improve Terminal's session picker. Inspect
the generated artifacts and previews for component reuse, consistent appearance,
correct manifest integration and usable failure states. Repeat on a fresh
environment to ensure success does not rely on this checkout or an agent's
conversation history.

## Implementation order

1. Audit representative existing screens; resolve accent/typography/blur and
   embedded-mode decisions; record the approved reference set.
2. Extract semantic tokens and the first components while preserving current
   dashboard rendering. Migrate a small representative slice to prove parity.
3. Build the offline gallery, app shell and working starter. Use Terminal/setup
   as real consumers so the foundation is exercised immediately.
4. Package the app/design skills with matching docs and assets; run realistic
   generation tasks against the starter and gallery.
5. Add scoped conformance checks to app validation and qualification; migrate
   additional first-party surfaces incrementally.

App generation cannot be called complete while agents still invent the visual
foundation. Completion means a new agent, a new workspace and an ordinary app
request reliably produce a working, recognizably Archipelago result.
