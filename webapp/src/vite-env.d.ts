/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_API_URL?: string;
  readonly VITE_RGB_RELAY_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
