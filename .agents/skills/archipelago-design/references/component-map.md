# Existing component map

Reuse these first:

- Structure: `BaseModal.vue`, `BackButton.vue`, `EmptyState.vue`,
  `SkeletonCard.vue`.
- Controls: `ToggleSwitch.vue`, `PasswordRevealInput.vue`,
  `AppSearchField.vue`, `CopyButton.vue`.
- Feedback: `ToastStack.vue`, `ContainerStatus.vue`, existing upload/progress
  components.
- Completion: `PaymentSuccessPane.vue`, `IdentitySuccessPane.vue`.
- App flows: `AppLauncherOverlay.vue`, `AppCredentialInterstitial.vue`.

The terminal must have an explicit keyboard ownership boundary and must not be
wrapped in the generic arrow-key modal behavior without adapting it.
