/**
 * `<LiveHTML>` — a real web page, running: the example's HTML, CSS and
 * JavaScript go into a sandboxed iframe exactly as they'd sit in a site
 * (with `<toasty-lite>` and the Lite runtime loaded the normal way), shown
 * in a browser frame, with the source in tabs underneath — plus an
 * optional React version of the same thing.
 *
 * Example code references the runtime and files as `/toasty/…` (where a
 * reader's site would serve them); inside the frame those resolve to this
 * docs site's copies.
 */
import React, { useMemo, useState } from 'react'
import useBaseUrl from '@docusaurus/useBaseUrl'
import CodeBlock from '@theme/CodeBlock'
import { useColorMode } from '@docusaurus/theme-common'
import styles from './live.module.css'

export interface LiveHTMLProps {
  /** Shown in the fake address bar. */
  url?: string
  height?: number
  html: string
  css?: string
  /** A module script (runs after the HTML). */
  js?: string
  /** The same example as a React component (shown as a tab; not run). */
  react?: string
  /** Which tab is open first. */
  tab?: 'html' | 'css' | 'js' | 'react'
  caption?: React.ReactNode
}

const BASE_CSS = `
  :root { color-scheme: light dark; --ink: #18181b; --muted: #6b6b76; --line: #e7e5e4; --surface: #ffffff; --page: #f7f7f5; --accent: #e8590c; }
  :root[data-theme="dark"] { --ink: #ececef; --muted: #9a9aa6; --line: #2a2a31; --surface: #17171b; --page: #0f0f12; }
  * { box-sizing: border-box; }
  html, body { margin: 0; height: 100%; }
  body { font: 15px/1.5 "Geist", system-ui, -apple-system, "Segoe UI", sans-serif; color: var(--ink); background: var(--page); -webkit-font-smoothing: antialiased; }
  button { font: inherit; color: inherit; }
`

export function LiveHTML({ url = 'example.com', height = 360, html, css = '', js = '', react, tab = 'html', caption }: LiveHTMLProps) {
  const base = useBaseUrl('/')
  const { colorMode } = useColorMode()
  const [open, setOpen] = useState(tab)
  const [nonce, setNonce] = useState(0)
  const srcDoc = useMemo(() => {
    if (typeof window === 'undefined') return ''
    const root = new URL(base, window.location.href).href
    const fix = (s: string) => s.replace(/(["'(])\/toasty\//g, `$1${root}toasty/`)
    return `<!doctype html><html data-theme="${colorMode}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<link rel="preconnect" href="https://fonts.googleapis.com"><link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600;700&display=swap">
<script type="module" src="${root}toasty/an1mbyt3-lite.js"></script>
<style>${BASE_CSS}${fix(css)}</style></head><body>${fix(html)}${js ? `<script type="module">${fix(js)}</script>` : ''}</body></html>`
  }, [base, colorMode, html, css, js, nonce])
  const tabs = [
    ['html', 'HTML', html, 'html'],
    ...(css ? [['css', 'CSS', css, 'css']] : []),
    ...(js ? [['js', 'JavaScript', js, 'js']] : []),
    ...(react ? [['react', 'React', react, 'tsx']] : []),
  ] as [typeof tab, string, string, string][]
  const current = tabs.find((t) => t[0] === open) ?? tabs[0]
  return (
    <figure className={styles.live}>
      <div className={styles.chrome}>
        <span className={styles.dots} aria-hidden="true">
          <i />
          <i />
          <i />
        </span>
        <span className={styles.url}>{url}</span>
        <button type="button" className={styles.reload} onClick={() => setNonce((n) => n + 1)} title="Reload the example">
          Reload
        </button>
      </div>
      <iframe key={nonce} className={styles.frame} style={{ height }} srcDoc={srcDoc} title={`Live example: ${url}`} sandbox="allow-scripts allow-same-origin allow-forms" allow="clipboard-write" loading="lazy" />
      <div className={styles.tabs} role="tablist">
        {tabs.map(([id, label]) => (
          <button key={id} type="button" role="tab" aria-selected={current[0] === id} className={current[0] === id ? styles.tabOn : styles.tab} onClick={() => setOpen(id)}>
            {label}
          </button>
        ))}
      </div>
      <div className={styles.code}>
        <CodeBlock language={current[3]}>{current[2].trim()}</CodeBlock>
      </div>
      {caption && <figcaption className={styles.caption}>{caption}</figcaption>}
    </figure>
  )
}
