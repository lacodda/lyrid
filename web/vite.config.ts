import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import { fileURLToPath } from 'node:url'
import { writeFileSync } from 'node:fs'
import type { IncomingMessage, ServerResponse } from 'node:http'
// The import attribute is required by Vite's native config loader.
import pkg from './package.json' with { type: 'json' }

/**
 * Catches the renderer prototype's benchmark result and writes it beside the
 * prototype, so a frame-rate measurement can be captured from a script rather
 * than read off the screen.
 */
function benchmarkCollector(): Plugin {
  return {
    name: 'lyrid-benchmark-collector',
    configureServer(server) {
      server.middlewares.use('/__benchmark', (request: IncomingMessage, response: ServerResponse) => {
        const chunks: Buffer[] = []
        request.on('data', (chunk: Buffer) => chunks.push(chunk))
        request.on('end', () => {
          const body = Buffer.concat(chunks).toString('utf8')
          writeFileSync(fileURLToPath(new URL('./prototype/benchmark.json', import.meta.url)), body)
          console.log('[benchmark]', body)
          response.statusCode = 204
          response.end()
        })
      })
    },
  }
}

export default defineConfig({
  plugins: [react(), benchmarkCollector()],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  server: {
    port: 5173,
    // The API runs separately during development; proxying keeps the SPA on
    // same-origin paths, so no CORS handling is needed here or on the server.
    proxy: {
      '/health': 'http://127.0.0.1:8080',
      '/api': 'http://127.0.0.1:8080',
    },
  },
})
