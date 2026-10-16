// Builds the website embed (`src/embed`) into `public/embed/`:
// - `an1mbyt3-embed.js`  — `createPlayer`, `<an1-player>` (vanilla JS)
// - `an1mbyt3-react.js`  — `<An1Player>` (imports `react` from the host app)
// - `an1mbyt3-core.js`   — the engine glue + player both share
// "Export → Website embed" copies these next to the game's `.teab` and the
// engine `.wasm`. Like the standalone player, the `.wasm` is never bundled:
// the page loads it (by default from next to the `.teab`).
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { rmSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

const outDir = 'public/embed'

export default defineConfig({
  define: { 'process.env': '{}' },
  plugins: [
    react(),
    {
      name: 'no-default-wasm-url',
      enforce: 'pre',
      transform(code, id) {
        if (!id.split('\\').join('/').endsWith('wasm-pkg/an1mbyt3_wasm.js')) return null
        return code.replace(/new URL\('an1mbyt3_wasm_bg\.wasm', import\.meta\.url\)/g, "'an1mbyt3_wasm_bg.wasm'")
      },
    },
    {
      name: 'drop-emitted-wasm',
      closeBundle() {
        for (const dir of [outDir, join(outDir, 'assets')]) {
          let files: string[] = []
          try {
            files = readdirSync(dir)
          } catch {
            continue
          }
          for (const file of files) if (file.endsWith('.wasm')) rmSync(join(dir, file))
        }
      },
    },
  ],
  build: {
    outDir,
    emptyOutDir: true,
    copyPublicDir: false,
    assetsInlineLimit: 0,
    lib: {
      entry: { 'an1mbyt3-embed': 'src/embed/index.ts', 'an1mbyt3-react': 'src/embed/react.tsx', 'an1mbyt3-lite': 'src/lite/index.ts', 'an1mbyt3-lite-react': 'src/lite/react.tsx' },
      formats: ['es'],
    },
    rollupOptions: {
      external: ['react', 'react/jsx-runtime', 'react-dom'],
      output: {
        entryFileNames: '[name].js',
        chunkFileNames: 'an1mbyt3-[name].js',
      },
    },
  },
})
