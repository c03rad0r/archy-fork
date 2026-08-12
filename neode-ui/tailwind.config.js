/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{vue,js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      fontFamily: {
        sans: ['Avenir Next', 'system-ui', 'sans-serif'],
        mono: ['Courier New', 'monospace'],
        archipelago: ['Montserrat', 'sans-serif'],
      },
      backdropBlur: {
        'glass': '18px',
        'glass-strong': '24px',
      },
      colors: {
        'glass-dark': 'rgba(0, 0, 0, 0.35)',
        'glass-darker': 'rgba(0, 0, 0, 0.6)',
        'glass-border': 'rgba(255, 255, 255, 0.18)',
        'glass-highlight': 'rgba(255, 255, 255, 0.22)',
      },
      boxShadow: {
        'glass': '0 8px 24px rgba(0, 0, 0, 0.45)',
        'glass-sm': '0 6px 18px rgba(0, 0, 0, 0.35)',
        'glass-inset': 'inset 0 1px 0 rgba(255, 255, 255, 0.22)',
      },
      spacing: {
        // 4px grid system
        '1': '4px',
        '2': '8px',
        '3': '12px',
        '4': '16px',
        '5': '20px',
        '6': '24px',
        '7': '28px',
        '8': '32px',
      },
    },
  },
  plugins: [],
}

