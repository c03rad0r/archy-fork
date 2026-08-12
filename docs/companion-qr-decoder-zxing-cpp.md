# Companion QR decoder — the zxing-cpp option (deferred)

*2026-08-11. Status: **NOT actioned.** Held as the next lever if the tuned
ZXing-Java pipeline proves insufficient in field testing. Companion-only —
touches `Android/` and nothing else.*

Related: [`qr-scanner-snappiness-handover.md`](qr-scanner-snappiness-handover.md)
(web + native survey, 2026-07-29), [`companion-pairing-qr.md`](companion-pairing-qr.md)
(the payload being scanned).

## Where we actually landed first

Before reaching for a new decoder, the native scanner
(`Android/app/src/main/java/com/archipelago/app/ui/components/QrScannerOverlay.kt`)
was rebuilt around one rule:

> **Every frame costs the same, and every frame sees the whole scene.**

Per frame: centre ROI at full resolution (dense invoices keep their
pixels-per-module) + the whole frame at half resolution (coverage) + one
alternating `GlobalHistogramBinarizer` pass. Bounded extras only: an inverted
ROI every 8th frame, one `TRY_HARDER` pass over the *half*-frame at most once
a second.

Two bugs were fixed on the way, both worth remembering because they are easy
to reintroduce:

1. **Escalation-on-failure is backwards.** An earlier version unlocked
   progressively more expensive searches on each frame that missed, ending in
   a `TRY_HARDER` pass over the full 2 MP frame (150–300 ms). The result was a
   scanner that locked on instantly when the code was already in view at open,
   and crawled when the user opened the camera and then moved to the code —
   because hunting collapsed the rate from ~30 attempts/sec to ~4, each on a
   motion-blurred frame. Failure means the user is still aiming, which is when
   the scanner must be *fastest*, not most thorough.
2. **A one-shot `startFocusAndMetering` locks the lens.** It puts AF in AUTO
   until auto-cancel; the 5 s default spans exactly the window where the user
   is swinging the phone toward the code, and a locked lens cannot follow.
   Auto-cancel is now 1 s so `CONTROL_AF_MODE_CONTINUOUS_PICTURE` does the
   tracking.

Plus `CONTROL_AE_TARGET_FPS_RANGE` pinned to the highest floor the back camera
offers at ≤30 fps, which caps exposure (~33 ms) and kills the motion blur that
indoor auto-exposure otherwise bakes into every hand-held frame.

That combination tested better on device (2026-08-11). This document covers
what to do **if it is still not good enough.**

## The remaining structural limit

The decoder engine itself. ZXing's Java implementation is both the slow part
and the picky part — most relevantly, it rejects perspective-skewed codes
outright, which is much of what the sensor sees while the user is moving. No
amount of frame budgeting fixes a decoder that will not accept the frame.

## The candidate: zxing-cpp

`io.github.zxing-cpp:android` — the maintained C++ rewrite of ZXing with an
official Android/Kotlin wrapper.

**Why it clears the project's dependency bar** (`~/.claude/CLAUDE.md`):
Apache-2.0, established OSS, fully on-device, no telemetry, no Play Services,
no account or network dependency. This is the distinguishing point against
**ML Kit**, which is the other fast option and is disqualified: it is
proprietary and Play-Services-backed.

**What it buys:**

- Roughly 5–10× faster than ZXing-Java on the same frames.
- Materially better on the cases that actually fail here: perspective/rotation
  (`tryRotate`, and its detector handles warp rather than rejecting it),
  blur, low contrast, damaged codes.
- Built-in inversion handling (`tryInvert`), removing our alternating
  inverted-ROI pass.
- Accepts an `ImageProxy` directly, so the manual Y-plane crop/copy machinery
  in `QrCodeAnalyzer` can largely be deleted — including the reused
  `roiBuffer`/`halfBuffer` and the `pixelStride` handling.

**Costs / risks:**

- New native dependency. APK grows ~1–2 MB — limited because the app is
  already arm64-only (`abiFilters += "arm64-v8a"`), so only one ABI ships.
- Adds a native attack/maintenance surface next to the existing Rust FIPS
  core. Pin the version exactly, per project rules.
- The tuned camera work above (AE FPS floor, AF auto-cancel, flat per-frame
  budget) stays relevant regardless — a faster decoder does not fix a blurred
  or out-of-focus frame. Do **not** rip that out as part of this change.

## Integration sketch

> ⚠️ Coordinates and API surface below are from memory and were **not**
> verified against Maven Central — the machine this was written on had no
> network. Confirm the current artifact version and wrapper API on the first
> online Gradle sync before trusting the snippet.

`Android/app/build.gradle.kts`:

```kotlin
// Replaces com.google.zxing:core for the live-camera path.
implementation("io.github.zxing-cpp:android:<pin-exact-version>")
```

`QrCodeAnalyzer` collapses to roughly:

```kotlin
private val reader = BarcodeReader().apply {
    options = BarcodeReader.Options(
        formats = setOf(BarcodeFormat.QR_CODE),
        tryHarder = true,
        tryRotate = true,
        tryInvert = true,
    )
}

override fun analyze(image: ImageProxy) {
    try {
        reader.read(image).firstOrNull()?.text?.let(onDecoded)
    } finally {
        image.close()
    }
}
```

Keep `com.google.zxing:core` for now regardless: the still-image path
(`decodeQrFromUri` in `WalletQrScannerModal.kt`, used by "Upload image") and
`prewarmQrScanner` both use it, and neither is on the hot path.

## Decision trigger

Action this only if field testing shows the current pipeline still failing the
**move-to-the-code** case — open the scanner pointing at nothing, then bring it
to a QR at a normal hand-held distance. If that reads within about a second in
ordinary room light, the Java decoder is doing its job and this stays on the
shelf.
