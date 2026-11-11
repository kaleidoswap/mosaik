import kaleidoPreset from 'kaleido-ui/tailwind';
/** @type {import('tailwindcss').Config} */
export default {
  presets: [kaleidoPreset],
  content: ['./index.html', './src/**/*.{ts,tsx}', './node_modules/kaleido-ui/dist/web/**/*.js', './node_modules/kaleido-ui/dist/*.js'],
  theme: {
    extend: {
      colors: {
        // kaleido-ui design tokens (`--app-*` channels, see styles.css) under the same utility names
        // the desktop app uses: bg-surface-raised, text-content-secondary, border-border-subtle, ...
        surface: {
          base: 'rgb(var(--app-surface-base) / <alpha-value>)',
          raised: 'rgb(var(--app-surface-raised) / <alpha-value>)',
          overlay: 'rgb(var(--app-surface-overlay) / <alpha-value>)',
          elevated: 'rgb(var(--app-surface-elevated) / <alpha-value>)',
          high: 'rgb(var(--app-surface-high) / <alpha-value>)',
          card: 'rgb(255 255 255 / <alpha-value>)',
          inset: 'rgb(0 0 0 / <alpha-value>)',
        },
        content: {
          primary: 'rgb(var(--app-content-primary) / <alpha-value>)',
          secondary: 'rgb(var(--app-content-secondary) / <alpha-value>)',
          tertiary: 'rgb(var(--app-content-tertiary) / <alpha-value>)',
          inverse: 'rgb(var(--app-content-inverse) / <alpha-value>)',
        },
        border: {
          subtle: 'rgb(var(--app-border-subtle) / <alpha-value>)',
          default: 'rgb(var(--app-border-default) / <alpha-value>)',
          strong: 'rgb(var(--app-border-strong) / <alpha-value>)',
        },
        primary: {
          DEFAULT: 'rgb(var(--app-primary) / <alpha-value>)',
          emphasis: 'rgb(var(--app-primary-emphasis) / <alpha-value>)',
          foreground: 'rgb(var(--app-primary-foreground) / <alpha-value>)',
        },
        secondary: {
          DEFAULT: 'rgb(var(--app-secondary) / <alpha-value>)',
          emphasis: 'rgb(var(--app-secondary-emphasis) / <alpha-value>)',
        },
        status: {
          success: 'rgb(var(--app-status-success) / <alpha-value>)',
          danger: 'rgb(var(--app-status-danger) / <alpha-value>)',
          warning: 'rgb(var(--app-status-warning) / <alpha-value>)',
          info: 'rgb(var(--app-status-info) / <alpha-value>)',
          'success-subtle': 'rgb(var(--app-status-success) / 0.15)',
          'danger-subtle': 'rgb(var(--app-status-danger) / 0.15)',
          'warning-subtle': 'rgb(var(--app-status-warning) / 0.15)',
          'info-subtle': 'rgb(var(--app-status-info) / 0.15)',
        },
        divider: 'rgb(var(--app-divider) / <alpha-value>)',
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
        mono: ['"Geist Mono"', '"JetBrains Mono"', 'ui-monospace', 'monospace'],
        sans: ['Satoshi', 'Inter', 'ui-sans-serif', 'system-ui', 'sans-serif'],
      },
      fontSize: {
        mini: ['9px', '12px'],
        xxs: ['10px', '14px'],
        tiny: ['11px', '16px'],
        caption: ['13px', '18px'],
        body: ['15px', '22px'],
        subhead: ['17px', '24px'],
        title: ['20px', '28px'],
        headline: ['28px', '34px'],
      },
      keyframes: {
        'fade-in': { from: { opacity: '0' }, to: { opacity: '1' } },
        'slide-up': { from: { opacity: '0', transform: 'translateY(10px)' }, to: { opacity: '1', transform: 'translateY(0)' } },
      },
      animation: {
        'fade-in': 'fade-in 0.2s ease-out',
        'slide-up': 'slide-up 0.25s ease-out',
      },
      boxShadow: {
        popover: '0 18px 55px rgba(0, 0, 0, 0.65)',
        glow: '0 0 32px -8px rgba(143, 213, 234, 0.3)',
      },
    },
  },
  plugins: [],
};
