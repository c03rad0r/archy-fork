# Neode Onboarding Flow

## Complete User Journey (Vue 3)

### 1. **Splash Screen** (First Visit Only)
**Duration**: ~23 seconds (skippable)

#### Sequence:
1. **Alien Terminal Intro** (0-16s)
   - Line 1: "Initializing Neode OS..." (typing animation)
   - Line 2: "Connecting to distributed network..."
   - Line 3: "Loading sovereignty protocols..."
   - Line 4: "System ready."
   - Green `$` prompts, white text
   - Skip button in bottom right

2. **Welcome Message** (16-19s)
   - "Welcome to Neode" with typing animation
   - Fades in after terminal lines complete

3. **Neode Logo** (19-23s)
   - Large "NEODE" SVG logo
   - Background image fades in
   - Smooth transition

#### Local Storage:
- Sets: `neode_intro_seen = '1'`
- Next visit: Skip splash entirely

---

### 2. **Onboarding Intro** 
**Route**: `/onboarding/intro`

#### Content:
- **Neode Logo** at top (large SVG)
- **Heading**: "Welcome to Neode"
- **Subheading**: "Your personal server for a sovereign digital life"
- **Features**:
  - 🔒 Self-Sovereign: Own your data and applications completely
  - ⚡ Powerful: Run any service with one click
  - 🛡️ Private: Tor-first architecture for maximum privacy
- **Button**: "Get Started →"

#### Action:
- Navigates to `/onboarding/options`

---

### 3. **Onboarding Options**
**Route**: `/onboarding/options`

#### Content:
- **Neode Logo** at top
- **Heading**: "Choose Your Setup"
- **Subheading**: "How would you like to get started?"

#### Three Glass Cards:
1. **Fresh Start**
   - Icon: Plus symbol
   - Description: Set up a new server from scratch
   
2. **Restore Backup**
   - Icon: Upload symbol
   - Description: Restore from a previous backup
   
3. **Connect Existing**
   - Icon: Link symbol
   - Description: Connect to an existing Neode server

#### Selection:
- Cards have hover effects
- Selected card: Brighter, glowing border
- **Button**: "Continue →" (enabled when option selected)

#### Action:
- Sets: `neode_onboarding_complete = '1'`
- Navigates to `/login`

---

### 4. **Login Page**
**Route**: `/login`

#### Content:
- **Neode Logo** floating above card
- **Glass Card** with:
  - Title: "Welcome to Neode"
  - Password input field
  - Login button
  - "Forgot password?" link

#### Auth Flow:
- Submit → Pinia store `login()` action
- Success → Navigate to `/dashboard`
- Error → Show error message in red glass banner

---

### 5. **Dashboard**
**Route**: `/dashboard`

#### Layout:
- **Sidebar** (glass):
  - Neode logo at top
  - Server name + version
  - Navigation menu (Home, Apps, Marketplace, Server, Settings)
  - Logout button at bottom

- **Main Content**:
  - Dynamic based on route (Home, Apps, etc.)
  - Connection status banner (if offline)
  - Glass cards throughout

---

## Flow Diagram

```
┌─────────────────┐
│  First Visit?   │
└────────┬────────┘
         │
    ┌────▼────┐
    │   Yes   │──────┐
    └─────────┘      │
         │           │
    ┌────▼────────────▼────┐
    │  Splash Screen       │ (23s, skippable)
    │  - Alien Intro       │
    │  - Welcome Message   │
    │  - Neode Logo        │
    └──────────┬───────────┘
               │
         [Sets: neode_intro_seen]
               │
    ┌──────────▼───────────┐
    │ Onboarding Intro     │
    │ - Logo               │
    │ - Features           │
    │ - Get Started        │
    └──────────┬───────────┘
               │
    ┌──────────▼───────────┐
    │ Onboarding Options   │
    │ - Fresh Start        │
    │ - Restore            │
    │ - Connect            │
    └──────────┬───────────┘
               │
         [Sets: neode_onboarding_complete]
               │
    ┌──────────▼───────────┐
    │    Login Page        │
    │ - Password Input     │
    └──────────┬───────────┘
               │
         [Authenticate]
               │
    ┌──────────▼───────────┐
    │     Dashboard        │
    │ - Sidebar + Content  │
    └──────────────────────┘
```

---

## Returning User Flow

### Second Visit Onwards:

```
Open App
   │
   ├─ neode_intro_seen? YES
   ├─ neode_onboarding_complete? YES
   │
   └──> Login Page (direct)
```

- **No splash screen**
- **No onboarding**
- Goes straight to `/login`

---

## Local Storage Keys

| Key | Value | Set By | Effect |
|-----|-------|--------|--------|
| `neode_intro_seen` | `'1'` | SplashScreen.vue | Skip splash on return |
| `neode_onboarding_complete` | `'1'` | OnboardingOptions.vue | Skip onboarding on return |

---

## Branding Consistency

### Neode Logo Usage

**SVG Logo** (`/assets/img/logo-large.svg`):
- ✅ Splash screen (large, centered)
- ✅ Onboarding intro (medium, top)
- ✅ Onboarding options (medium, top)
- ✅ Login page (floating above card)
- ✅ Dashboard sidebar (small, top left)

**Icon** (`/assets/img/icon.png`):
- ✅ Browser favicon
- ✅ Apple touch icon

### No Start9 Branding
All Start9 references removed. Pure Neode branding throughout.

---

## Design Consistency

### Glassmorphism
Every screen uses:
- Glass cards with `backdrop-filter: blur(18px)`
- Black background with transparency
- White borders with 18% opacity
- Drop shadows for depth

### Colors
- Background: `rgba(0, 0, 0, 0.35)`
- Text: White with opacity (96%, 80%, 70%)
- Accents: Green `#00ff41` (terminal prompts)
- Borders: `rgba(255, 255, 255, 0.18)`

### Typography
- Primary: Avenir Next
- Mono: Courier New (terminal/splash)
- Size scale: 4px grid system

---

## Testing the Flow

### Test as New User:
```bash
# Clear storage
localStorage.clear()

# Reload
location.reload()
```

**Expected**:
1. Splash → Alien intro → Welcome → Logo
2. Onboarding intro → Features
3. Onboarding options → Select option
4. Login → Enter password
5. Dashboard → Home screen

### Test as Returning User:
```bash
# Storage should have:
localStorage.getItem('neode_intro_seen') // '1'
localStorage.getItem('neode_onboarding_complete') // '1'

# Reload
location.reload()
```

**Expected**:
1. Login (direct, no splash/onboarding)
2. Dashboard → Home screen

---

## Skip Behaviors

### Skip Splash
- Button: "Skip Intro" (bottom right)
- Effect: Jumps to logo display
- Still navigates to onboarding intro

### Skip Onboarding
User can navigate directly to `/login` if they know the URL.

---

## Future Enhancements

- [ ] Different flows for each setup option (Fresh/Restore/Connect)
- [ ] Progress indicators during setup
- [ ] Animated transitions between onboarding steps
- [ ] Video/GIF demos on feature cards
- [ ] Personalization (server name input during onboarding)
- [ ] Setup wizard for advanced users

---

## Key Files

| File | Purpose |
|------|---------|
| `src/App.vue` | Manages splash display, handles completion |
| `src/components/SplashScreen.vue` | Alien intro, animations, skip button |
| `src/views/OnboardingIntro.vue` | Welcome screen, feature highlights |
| `src/views/OnboardingOptions.vue` | Setup method selection |
| `src/views/Login.vue` | Authentication |
| `src/views/Dashboard.vue` | Main app layout |
| `src/router/index.ts` | Route definitions, auth guards |

---

**Complete, cohesive, and beautiful!** 🎨⚡

