/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        // KaleidoSwap dark palette — same vocabulary the legacy webapp used.
        ink: {
          900: '#0a0a0f',
          800: '#101018',
          700: '#161624',
          600: '#1e1e2e',
          500: '#2a2a3d',
          400: '#3a3a52',
        },
        mosaic: {
          // Cyan-violet covenant accents.
          cyan: '#5ee7df',
          violet: '#a78bfa',
          rose: '#ff5d8f',
          amber: '#ffb44a',
          mint: '#7af0a2',
        },
      },
      fontFamily: {
        mono: ['"JetBrains Mono"', 'ui-monospace', 'monospace'],
        sans: ['Inter', 'ui-sans-serif', 'system-ui', 'sans-serif'],
      },
      boxShadow: {
        glow: '0 0 32px -8px rgba(94, 231, 223, 0.35)',
      },
    },
  },
  plugins: [],
};
