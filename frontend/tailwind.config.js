/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: 'class',
  theme: {
    extend: {
      fontFamily: {
        sans: ['Inter', 'sans-serif'],
        mono: ['JetBrains Mono', 'ui-monospace', 'monospace'],
      },
      colors: {
        dark: {
          950: '#030508',
          900: '#070b12',
          850: '#0b101b',
          800: '#101726',
          700: '#1a2338',
          600: '#27344f',
        },
        brand: {
          50: '#ecfdf5',
          100: '#d1fae5',
          200: '#a7f3d0',
          300: '#6ee7b7',
          400: '#34d399',
          500: '#10b981',
          600: '#059669',
          700: '#047857',
          800: '#065f46',
          900: '#064e3b',
        },
        cyber: {
          cyan: '#06b6d4',
          purple: '#a855f7',
          amber: '#f59e0b',
          rose: '#f43f5e',
          emerald: '#10b981',
        }
      }
    },
  },
  plugins: [],
}
