import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig(({ mode }) => ({
  plugins: [react()],
  server: {
    // HOPWIRE_PROXY_TARGET (env or .env.local) points dev at a non-default engine.
    proxy: { '/v1': loadEnv(mode, '.', 'HOPWIRE_').HOPWIRE_PROXY_TARGET || 'http://localhost:8080' },
  },
}))
