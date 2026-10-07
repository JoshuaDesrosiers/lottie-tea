// `tsc` only compiles `.ts` files — it never copies `src/wasm-pkg`'s
// `.wasm`/`.js`/`.d.ts` output into `dist/`, but `dist/animation.js`'s
// compiled `import ... from './wasm-pkg/an1mbyt3_wasm.js'` is a relative
// path resolved from wherever *that file* ends up, so `dist/wasm-pkg` has to
// actually exist too. Run after `tsc -b` in this package's `build` script.
import { cpSync, existsSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const packageRoot = dirname(dirname(fileURLToPath(import.meta.url)))
const source = join(packageRoot, 'src', 'wasm-pkg')
const destination = join(packageRoot, 'dist', 'wasm-pkg')

if (!existsSync(source)) {
  console.error(`copy-wasm-pkg: ${source} doesn't exist — run "npm run build:wasm" first`)
  process.exit(1)
}

cpSync(source, destination, { recursive: true })
console.log(`copy-wasm-pkg: copied ${source} -> ${destination}`)
