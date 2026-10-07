// Builds the standalone game player (`src/player/main.ts`) into one ES
// module, `public/player/an1mbyt3-player.js`, which "Export Game" inlines
// into the exported `.html` (see `src/game/exportGame.ts`). The engine's
// `.wasm` is NOT bundled here — every export embeds the *current* engine
// separately (so the player can never carry a stale one): the glue's
// `new URL('…_bg.wasm', import.meta.url)` default is stripped before Vite
// can inline it, and any stray emitted copy is deleted after the build.
import { defineConfig } from 'vite'
import { rmSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

const outDir = 'public/player'

export default defineConfig({
  define: { 'process.env': '{}' },
  build: {
    outDir,
    emptyOutDir: true,
    copyPublicDir: false,
    lib: {
      entry: 'src/player/main.ts',
      formats: ['es'],
      fileName: () => 'an1mbyt3-player.js',
    },
    assetsInlineLimit: 0,
  },
  plugins: [
    {
      name: 'no-default-wasm-url',
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
})
