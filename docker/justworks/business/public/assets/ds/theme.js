/*
 * Theme controller.
 *
 * Three preferences: "system" (default), "light", "dark". The choice is
 * stored in localStorage and applied as `data-theme` on <html>; "system"
 * removes the attribute so the `prefers-color-scheme` block in tokens.css
 * takes over. A small inline snippet in each page's <head> applies the stored
 * value before first paint, so this module only has to own the control.
 */

export const THEME_STORAGE_KEY = "jw-theme";
const THEMES = ["system", "light", "dark"];

/* Must match --jw-bg in tokens.css for each theme. */
const BROWSER_CHROME = { light: "#fafafa", dark: "#0b100f" };

const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");
const listeners = new Set();

function readStored() {
  try {
    const stored = localStorage.getItem(THEME_STORAGE_KEY);
    return THEMES.includes(stored) ? stored : "system";
  } catch {
    /* Private mode or blocked storage: fall back to the system preference. */
    return "system";
  }
}

/** The preference the user chose ("system" | "light" | "dark"). */
export function getPreference() {
  return readStored();
}

/** The theme actually on screen ("light" | "dark"), resolving "system". */
export function getResolvedTheme(preference = readStored()) {
  if (preference !== "system") return preference;
  return darkQuery.matches ? "dark" : "light";
}

export function applyTheme(preference) {
  const next = THEMES.includes(preference) ? preference : "system";
  const root = document.documentElement;

  if (next === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", next);

  const meta = document.querySelector('meta[name="theme-color"]');
  if (meta) meta.setAttribute("content", BROWSER_CHROME[getResolvedTheme(next)]);

  try {
    localStorage.setItem(THEME_STORAGE_KEY, next);
  } catch {
    /* Not fatal: the theme still applies for this page view. */
  }

  for (const listener of listeners) listener(next, getResolvedTheme(next));
  return next;
}

/** Subscribe to preference changes. Returns an unsubscribe function. */
export function onThemeChange(listener) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/* Keep the browser chrome correct when the OS flips while set to "system". */
darkQuery.addEventListener("change", () => {
  if (readStored() === "system") applyTheme("system");
});

const ICONS = {
  system: '<path d="M3 4.5h12v8H3z"/><path d="M6.5 15.5h5"/>',
  light: '<circle cx="9" cy="9" r="3.4"/><path d="M9 1.6v1.8M9 14.6v1.8M1.6 9h1.8M14.6 9h1.8M3.8 3.8l1.3 1.3M12.9 12.9l1.3 1.3M14.2 3.8l-1.3 1.3M5.1 12.9l-1.3 1.3"/>',
  dark: '<path d="M14.5 10.4A6 6 0 0 1 7.6 3.5a6 6 0 1 0 6.9 6.9Z"/>'
};

const LABELS = { system: "Match system theme", light: "Light theme", dark: "Dark theme" };

/**
 * Renders the segmented theme control into `container` and keeps its pressed
 * state in sync with the current preference.
 */
export function mountThemeToggle(container) {
  if (!container) return null;

  container.classList.add("theme-toggle");
  container.setAttribute("role", "group");
  container.setAttribute("aria-label", "Color theme");
  container.innerHTML = THEMES.map((theme) => `
    <button type="button" data-theme-option="${theme}" title="${LABELS[theme]}" aria-label="${LABELS[theme]}">
      <svg viewBox="0 0 18 18" aria-hidden="true">${ICONS[theme]}</svg>
    </button>
  `).join("");

  const buttons = [...container.querySelectorAll("[data-theme-option]")];

  function sync(preference = readStored()) {
    for (const button of buttons) {
      button.setAttribute("aria-pressed", String(button.dataset.themeOption === preference));
    }
  }

  container.addEventListener("click", (event) => {
    const button = event.target.closest("[data-theme-option]");
    if (button) applyTheme(button.dataset.themeOption);
  });

  onThemeChange(sync);
  sync();
  return { sync };
}

/**
 * The snippet that must run before first paint. Kept here so the inline copy
 * in each page's <head> has a single documented source; if you change it,
 * update index.html and design/index.html to match.
 */
export const PREPAINT_SNIPPET = `(function(){try{var t=localStorage.getItem("${THEME_STORAGE_KEY}");if(t==="light"||t==="dark")document.documentElement.setAttribute("data-theme",t)}catch(e){}})()`;
