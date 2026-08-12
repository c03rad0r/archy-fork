# Replace Intro & Dashboard Backgrounds

To change the intro splash and dashboard tab backgrounds **without touching any code**, overwrite these files with your own assets. Use the exact names and locations below.

**Location:** All images go in `neode-ui/public/assets/img/`  
**Format:** JPG recommended. Portrait or landscape; they use `background-size: cover` and `center center`.

---

## Intro Background

| Filename | Used for |
|----------|----------|
| **`bg-intro.jpg`** | Intro splash (alien typing + video poster + fallback), Dashboard default |

---

## Intro Video

| Filename | Where | Used for |
|----------|-------|----------|
| **`video-intro.mp4`** | `neode-ui/public/assets/video/` | Welcome Noderunner + logo, onboarding, login |

**Format:** MP4 (H.264). Keep under ~5MB for web. See `VIDEO_COMPRESSION_GUIDE.md` for optimization.

---

---

## Dashboard Tab Backgrounds

| Filename | Tab |
|----------|-----|
| **`bg-home.webp`** | Home |
| **`bg-web5.jpg`** | Web5 |
| **`bg-network.jpg`** | Server / Network |
| **`bg-settings.webp`** | Settings |
| **`bg-myapps.webp`** | My Apps |
| **`bg-appstore.webp`** | App Store / Marketplace |
| **`bg-cloud.webp`** | Cloud |
| **`bg-intro.jpg`** | Default (also intro) |
| **`bg-intro-3.jpg`** | Alternate layer during transitions |

---

## Intro Flow Backgrounds (onboarding)

| Filename | Used for |
|----------|----------|
| **`bg-intro-1.webp`** | Onboarding done, login |
| **`bg-intro-2.jpg`** | Onboarding verify |
| **`bg-intro-3.jpg`** | Onboarding path, dashboard transition layer |
| **`bg-intro-4.webp`** | Onboarding options |
| **`bg-intro-5.webp`** | Onboarding did |
| **`bg-intro-6.webp`** | Onboarding backup |

---

## Quick Reference

| Asset | Full path |
|-------|-----------|
| Intro image | `neode-ui/public/assets/img/bg-intro.jpg` |
| Intro video | `neode-ui/public/assets/video/video-intro.mp4` |
| Home | `neode-ui/public/assets/img/bg-home.webp` |
| Web5 | `neode-ui/public/assets/img/bg-web5.jpg` |
| Network | `neode-ui/public/assets/img/bg-network.jpg` |
| Settings | `neode-ui/public/assets/img/bg-settings.webp` |
| My Apps | `neode-ui/public/assets/img/bg-myapps.webp` |
| App Store | `neode-ui/public/assets/img/bg-appstore.webp` |
| Cloud | `neode-ui/public/assets/img/bg-cloud.webp` |
| Default | `neode-ui/public/assets/img/bg-intro.jpg` |
| Transition | `neode-ui/public/assets/img/bg-intro-3.jpg` |
| Intro 1–6 | `neode-ui/public/assets/img/bg-intro-1.webp` … `bg-intro-6.webp` (intro-2 and intro-3 remain `.jpg` — WebP came out larger for those) |

---

## Steps to Replace

1. Put your images in `neode-ui/public/assets/img/` with the exact filenames above.
2. Put your video in `neode-ui/public/assets/video/video-intro.mp4`.
3. Run `npm run build` (or deploy) so the new assets are included.

No code changes required.
