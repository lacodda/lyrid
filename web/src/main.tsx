import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { overlayScrollbars } from 'dowel-ui'

import { App } from '@/App'
import { initLanguage } from '@/lib/language'
import '@/i18n'
import '@/styles.css'

const root = document.getElementById('root')
if (!root) {
  throw new Error('#root is missing from index.html')
}

// Before the first paint, not in an effect: a page that renders in English and
// switches to Russian a frame later is worse than one that waits.
initLanguage()

// A phone or a Mac draws its own overlay bar in place of the theme's 10px
// strip; a browser on Windows cannot ask for one and keeps the strip.
overlayScrollbars()

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
