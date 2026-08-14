/** Shape of `GET /health` as served by the axum backend. */
export interface Health {
  status: 'ok' | 'degraded'
  version: string
  database: 'ok' | 'unavailable'
}

/**
 * Same-origin fetch: in development Vite proxies these paths to the API, and
 * in production the SPA is served by the same server.
 *
 * A degraded backend answers 503 with a valid body, so the status code alone
 * does not decide whether the payload is usable.
 */
export async function fetchHealth(signal?: AbortSignal): Promise<Health> {
  const response = await fetch('/health', { signal })
  const body: unknown = await response.json()
  if (!isHealth(body)) {
    throw new Error('unexpected /health payload')
  }
  return body
}

function isHealth(value: unknown): value is Health {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as Record<string, unknown>
  return typeof candidate.status === 'string' && typeof candidate.version === 'string' && typeof candidate.database === 'string'
}
