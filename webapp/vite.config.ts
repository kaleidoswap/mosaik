import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// During `npm run dev` we proxy /api → mosaik-api (default http://127.0.0.1:8080)
// so the SPA can run on :5173 without CORS gymnastics. In production nginx
// (services/mosaik-webapp Dockerfile) does the same in front of the static build.
const API_TARGET = process.env.VITE_API_URL ?? 'http://127.0.0.1:8080';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    host: true,
    proxy: {
      '/api': { target: API_TARGET, changeOrigin: true },
      '/healthz': { target: API_TARGET, changeOrigin: true },
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
    target: 'es2022',
  },
});
