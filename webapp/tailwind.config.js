/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        // KaleidoSwap dark palette — same vocabulary the legacy webapp used.
        ink: {
          900: '#1E1F28',
          800: '#202027',
          700: '#252632',
          600: '#2E303C',
          500: '#3A3C4C',
          400: '#4A4C5E',
        },
        mosaic: {
          cyan: '#8FD5EA',
          violet: '#9365FF',
          rose: '#ff5d8f',
          amber: '#ffb44a',
          mint: '#15E99A',
        },
      },
      fontFamily: {
        mono: ['"JetBrains Mono"', 'ui-monospace', 'monospace'],
        sans: ['Satoshi', 'Inter', 'ui-sans-serif', 'system-ui', 'sans-serif'],
      },
      boxShadow: {
        glow: '0 0 32px -8px rgba(143, 213, 234, 0.3)',
      },
    },
  },
  plugins: [],
};
