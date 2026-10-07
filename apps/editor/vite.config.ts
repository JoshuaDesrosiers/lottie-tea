import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'

// A rebuilt `src/wasm-pkg` (wasm-pack output) must force a *full* page
// reload, never an HMR hot-swap: `App.tsx` creates its one `WasmEditor`
// instance once, at mount, from whatever wasm binary was loaded then. HMR
// swaps in fresh React components but keeps that old instance alive — so
// new UI code reading a field the old binary never serializes (a real
// crash: the Inspector's Camera section calling `toHex` on a
// `backgroundColor` the stale binary didn't send) blows up the whole app.
function wasmFullReload(): Plugin {
  return {
    name: 'wasm-pkg-full-reload',
    handleHotUpdate({ file, server }) {
      if (file.replace(/\\/g, '/').includes('/src/wasm-pkg/')) {
        server.ws.send({ type: 'full-reload' })
        return []
      }
    },
  }
}

// Standard Tauri+Vite dev-server configuration: a fixed, non-conflicting port
// Tauri's shell expects, HMR left visible in the terminal (clearScreen: false)
// so Rust build errors from the Tauri side aren't wiped from view, and the
// `src-tauri` directory excluded from Vite's file watcher since Cargo/Tauri
// already rebuilds on changes there — Vite re-triggering on the same edits
// would just be redundant churn.
const host = process.env.TAURI_DEV_HOST

export default defineConfig({
  plugins: [react(), wasmFullReload()],
  clearScreen: false,
  // The vendored scratch-paint tree (src/vendor/scratch-paint — forked in
  // directly, not npm-installed, so it's built by Vite like the rest of
  // this app rather than pre-bundled by its own original webpack config)
  // reads a bare `process.env.DEBUG` at module scope (a Node/webpack
  // convention gating a dev-only prop-types-validation path) — `process`
  // isn't a real global in a Vite-built browser bundle, so referencing it
  // unset throws a `ReferenceError`. Vite already defines
  // `process.env.NODE_ENV` for library compat; this adds an empty
  // `process.env` object so `.DEBUG` (and any other stray `process.env.X`
  // read anywhere else in the vendored tree) degrades to `undefined`
  // instead of throwing.
  define: {
    'process.env': '{}',
  },
  css: {
    modules: {
      // The vendored scratch-paint tree's CSS uses kebab-case class names
      // (`.editor-container`) but every component reads them camelCased
      // (`styles.editorContainer`) — its original webpack config had
      // css-loader's `exportLocalsConvention: 'camelCase'` set, which Vite
      // doesn't replicate by default (Vite's own default keeps only the
      // literal kebab-case key). Without this, every single CSS Modules
      // class lookup across the whole vendored tree silently evaluates to
      // `undefined` — real, hand-verified symptom: the Paint view rendered
      // with zero layout/styling at all, plain unstyled text top to bottom,
      // despite the actual CSS rules loading and injecting correctly.
      // `'camelCase'` (not `'camelCaseOnly'`) keeps the original kebab-case
      // key available too, in case anything (ours or scratch-paint's own)
      // ever accesses it by bracket notation.
      localsConvention: 'camelCase',
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] },
  },
})
