# Design contract

The current baseline is defined by `neode-ui/src/style.css`,
`neode-ui/tailwind.config.js`, `BaseModal.vue`, `AppSearchField.vue`,
`EmptyState.vue`, `SkeletonCard.vue`, and `PaymentSuccessPane.vue`. Preserve the
dark canvas, glass-card surface, 4px spacing scale, semantic orange accent,
local licensed fonts, bottom card actions, 40px desktop/52px mobile search,
44px touch targets, visible focus, dynamic viewport, safe-area and embedded
canvas rules while the shared kit is extracted.

Use semantic tokens, not raw values. New controls must document keyboard,
focus, disabled, loading, validation, error, retry, success, responsive, and
reduced-motion behavior. No essential action may be hover-only. Use the existing
dialog footer/content split and restore focus after close.

Visual acceptance covers 320/390/768/1440px, phone landscape, enlarged text,
standalone/embedded modes, dark native controls, no-blur fallback, and actual
Companion WebView behavior where applicable.
