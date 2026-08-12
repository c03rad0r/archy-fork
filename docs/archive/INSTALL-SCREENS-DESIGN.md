# Archipelago Installer — Screen Designs

Edit these screens to match your vision. I'll implement exactly what you specify.
Each screen is what the user sees at that moment on the console (80 columns wide).

Constraints: bash TUI only (no ncurses). ANSI colors available:
- `\033[1;37m` = bold white, `\033[1;33m` = bold yellow/orange
- `\033[32m` = green, `\033[31m` = red, `\033[37m` = dim gray
- `\033[0m` = reset. Box-drawing chars: ━ ─ │ ╭ ╮ ╰ ╯ ╔ ╗ ╚ ╝ █ ▓ ░ ▌▐
- Spinners possible: ⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ or ◐◓◑◒ or |/-\

---

## Screen 1: Welcome / Press Enter

```
  (clear screen, centered)

  a r c h i p e l a g o
  ━━━━━━━━━━━━━━━━━━━━━
  automatic installer

  Press Enter to install  |  Ctrl+C for shell
```

---

## Screen 2: Detecting Disk

```
  a r c h i p e l a g o
  ━━━━━━━━━━━━━━━━━━━━━

  [1/7]  Checking tools .............. ✓
  [2/7]  Detecting disks

         Found: /dev/sda (465.8G) — TOSHIBA MQ01ACF0

  ──────────────────────────────────────────

  ⚠  All data on /dev/sda will be erased.

  Press Enter to install  |  Ctrl+C to cancel
```

---

## Screen 3: Installing (progress)

```
  a r c h i p e l a g o
  ━━━━━━━━━━━━━━━━━━━━━

  [1/7]  Checking tools .............. ✓
  [2/7]  Detecting disks ............. ✓
  [3/7]  Creating partitions ......... ✓
  [4/7]  Formatting .................. ✓
  [5/7]  Installing system ........... ✓
  [6/7]  Encrypting data partition ◐
         AES-256-XTS (AES-NI detected)

  ──────────────────────────────────────────
```

---

## Screen 4: Bootloader

```
  a r c h i p e l a g o
  ━━━━━━━━━━━━━━━━━━━━━

  [1/7]  Checking tools .............. ✓
  [2/7]  Detecting disks ............. ✓
  [3/7]  Creating partitions ......... ✓
  [4/7]  Formatting .................. ✓
  [5/7]  Installing system ........... ✓
  [6/7]  Encrypting data ............. ✓
  [7/7]  Installing bootloader ....... ✓

  ──────────────────────────────────────────
```

---

## Screen 5: Complete

```
  a r c h i p e l a g o
  ━━━━━━━━━━━━━━━━━━━━━
  Installation Complete

  After reboot, open the Web UI from any device:

    http://archipelago.local

    SSH:       ssh archipelago@archipelago.local
    Password:  archipelago
    Web Login: password123

  ──────────────────────────────────────────

  >>> REMOVE THE USB DRIVE NOW <<<

  Press Enter to reboot
```

---

## Notes for Dorian

- Edit any screen above to match what you want to see
- Add/remove steps, change wording, change layout
- Specify colors per line if you want (e.g. "this line in yellow")
- I can add a spinner animation on the active step
- Box-drawing, progress bars, anything bash can render is fair game
- Once you're happy with the designs I'll implement them exactly
