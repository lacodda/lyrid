import { useEffect, useState } from 'react'

import { fetchHealth, type Health } from '@/api'

/** What the shell knows about the server behind it. */
type Probe = { state: 'checking' } | { state: 'reached'; health: Health } | { state: 'unreachable' }

export function App() {
  const probe = useHealth()

  return (
    <div className="shell">
      <header className="shell__header">
        <img className="shell__mark" src="/mark.svg" alt="" />
        <div>
          <h1 className="shell__title">lyrid</h1>
          <p className="shell__tagline">a music universe</p>
        </div>
      </header>

      <section className="panel">
        <h2 className="panel__title">The sky is being built</h2>
        <p className="panel__body">
          Every artist a star, every genre a nebula — one canonical map for everyone, with a personal fog of war over it. This is the foundation release:
          the rails stand, the server answers, and the universe itself arrives with the data pipelines.
        </p>

        <dl className="status">
          <div className="status__row">
            <dt className="status__key">client</dt>
            <dd className="status__value status__value--ok">v{__APP_VERSION__}</dd>
          </div>
          <div className="status__row">
            <dt className="status__key">api</dt>
            <dd className={`status__value ${apiClass(probe)}`}>{apiText(probe)}</dd>
          </div>
          <div className="status__row">
            <dt className="status__key">database</dt>
            <dd className={`status__value ${databaseClass(probe)}`}>{databaseText(probe)}</dd>
          </div>
        </dl>
      </section>
    </div>
  )
}

/** Probes the API once on mount: the shell should show whether it has a server. */
function useHealth(): Probe {
  const [probe, setProbe] = useState<Probe>({ state: 'checking' })

  useEffect(() => {
    const controller = new AbortController()

    fetchHealth(controller.signal)
      .then((health) => {
        setProbe({ state: 'reached', health })
      })
      .catch((error: unknown) => {
        // An aborted request is the StrictMode double-mount, not a failure.
        if (!(error instanceof DOMException && error.name === 'AbortError')) {
          setProbe({ state: 'unreachable' })
        }
      })

    return () => {
      controller.abort()
    }
  }, [])

  return probe
}

function apiText(probe: Probe): string {
  switch (probe.state) {
    case 'checking':
      return 'connecting…'
    case 'reached':
      return `v${probe.health.version}`
    case 'unreachable':
      return 'not running'
  }
}

function apiClass(probe: Probe): string {
  switch (probe.state) {
    case 'checking':
      return 'status__value--idle'
    case 'reached':
      return 'status__value--ok'
    case 'unreachable':
      return 'status__value--warn'
  }
}

function databaseText(probe: Probe): string {
  switch (probe.state) {
    case 'checking':
      return '…'
    case 'reached':
      return probe.health.database
    case 'unreachable':
      return 'unknown'
  }
}

function databaseClass(probe: Probe): string {
  if (probe.state === 'reached') {
    return probe.health.database === 'ok' ? 'status__value--ok' : 'status__value--warn'
  }
  return probe.state === 'checking' ? 'status__value--idle' : 'status__value--warn'
}
