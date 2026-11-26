import { fileURLToPath } from 'node:url'
import { defineConfig, loadEnv } from 'vite'
import vue from '@vitejs/plugin-vue'

export default defineConfig(({ mode }) => {
  const gatewayDir = fileURLToPath(new URL('..', import.meta.url))
  const { PORT, APP_PORT } = loadEnv(mode, gatewayDir, ['PORT', 'APP_PORT'])
  return {
    plugins: [vue()],
    server: {
      host: '0.0.0.0',
      port: 5173,
      proxy: {
        '/api': `http://127.0.0.1:${PORT ?? APP_PORT ?? 8080}`,
      },
    },
  }
})
