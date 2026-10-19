#!/usr/bin/env node
/**
 * Builds the **shared runtime** ("support binary") — one versioned folder
 * any number of websites can load the full engine from, plus a zip of it
 * for people who'd rather self-host:
 *
 * ```
 * public/runtime/
 *   <version>/                       → https://toastycreate.web.app/runtime/<version>/
 *     an1mbyt3-embed.js               createPlayer, setRuntimeUrl, <an1-player>
 *     an1mbyt3-react.js               <An1Player> (imports `react` — for bundlers)
 *     an1mbyt3-engine.js              the engine glue both share
 *     an1mbyt3-lite*.js, …-runtime.js Toasty Lite (small vector animations)
 *     an1mbyt3-embed.d.ts / -react.d.ts  types
 *     an1mbyt3_wasm_bg.wasm           the engine
 *     runtime.json                    version, build time, file sizes + sha256
 *     README.md                       what this is and how to use it
 *   toasty-runtime-<version>.zip     the folder above, zipped (one top folder)
 *   latest.json                      { version, folder, zip } of this build
 * ```
 *
 * Run after the embed build (`npm run build:player` does: player → embed →
 * this). Inputs: `public/embed/*.js` (from `vite.embed.config.ts`) and the
 * engine `src/wasm-pkg/an1mbyt3_wasm_bg.wasm` (built by wasm-pack; this
 * script never rebuilds it). The version comes from
 * `src/embed/runtimeVersion.ts` (`RUNTIME_VERSION`). Only that version's
 * folder and zip are rewritten — older version folders are left alone, so
 * pages pinned to them keep working.
 *
 * No dependencies: the zip is written by hand below (PKZIP local headers +
 * central directory, DEFLATE via Node's built-in zlib — the 20+ MB wasm
 * shrinks to roughly a third).
 */
import { createHash } from 'node:crypto'
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join, relative } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { deflateRawSync } from 'node:zlib'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const embedDir = join(root, 'public/embed')
const wasmFile = join(root, 'src/wasm-pkg/an1mbyt3_wasm_bg.wasm')
const runtimeRoot = join(root, 'public/runtime')

// ---- version (single source of truth: src/embed/runtimeVersion.ts) ----
const versionSource = readFileSync(join(root, 'src/embed/runtimeVersion.ts'), 'utf8')
const versionMatch = versionSource.match(/export const RUNTIME_VERSION = '([^']+)'/)
if (!versionMatch) throw new Error("build-runtime: couldn't find `export const RUNTIME_VERSION = '…'` in src/embed/runtimeVersion.ts")
const version = versionMatch[1]
if (!/^[\w.-]+$/.test(version)) throw new Error(`build-runtime: RUNTIME_VERSION "${version}" must be a plain folder name`)
const HOSTED = `https://toastycreate.web.app/runtime/${version}/`

// ---- inputs ----
if (!existsSync(wasmFile)) throw new Error('build-runtime: src/wasm-pkg/an1mbyt3_wasm_bg.wasm is missing — build the wasm crate first')
const bundles = existsSync(embedDir) ? readdirSync(embedDir).filter((f) => f.endsWith('.js')) : []
for (const required of ['an1mbyt3-embed.js', 'an1mbyt3-react.js']) {
  if (!bundles.includes(required)) throw new Error(`build-runtime: public/embed/${required} is missing — run \`npm run build:embed\` first`)
}

// ---- the folder ----
const outDir = join(runtimeRoot, version)
rmSync(outDir, { recursive: true, force: true })
mkdirSync(outDir, { recursive: true })
for (const file of bundles) copyFileSync(join(embedDir, file), join(outDir, file))
copyFileSync(wasmFile, join(outDir, 'an1mbyt3_wasm_bg.wasm'))

// Types: the same hand-written declarations "Export → Website embed"
// ships (`src/embed/types.ts`, import-free so Node ≥ 22.18 can load it
// with its built-in type stripping). Skipped with a note on older Node.
try {
  const types = await import(pathToFileURL(join(root, 'src/embed/types.ts')).href)
  writeFileSync(join(outDir, 'an1mbyt3-embed.d.ts'), types.EMBED_DTS)
  writeFileSync(join(outDir, 'an1mbyt3-react.d.ts'), types.REACT_DTS)
} catch (err) {
  console.warn(`build-runtime: skipped .d.ts files (this Node can't import TypeScript: ${err instanceof Error ? err.message : err})`)
}

writeFileSync(join(outDir, 'README.md'), readme())

const listed = readdirSync(outDir).filter((f) => f !== 'runtime.json').sort()
const manifest = {
  name: 'toasty-runtime',
  version,
  hosted: HOSTED,
  builtAt: new Date().toISOString(),
  files: Object.fromEntries(
    listed.map((f) => {
      const bytes = readFileSync(join(outDir, f))
      return [f, { size: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') }]
    }),
  ),
}
writeFileSync(join(outDir, 'runtime.json'), JSON.stringify(manifest, null, 2) + '\n')

// ---- the zip ----
const zipName = `toasty-runtime-${version}.zip`
const top = `toasty-runtime-${version}`
const entries = readdirSync(outDir)
  .sort()
  .map((f) => ({ name: `${top}/${f}`, data: readFileSync(join(outDir, f)), mtime: statSync(join(outDir, f)).mtime }))
const zip = writeZip(entries)
writeFileSync(join(runtimeRoot, zipName), zip)
writeFileSync(join(runtimeRoot, 'latest.json'), JSON.stringify({ version, folder: `/runtime/${version}/`, zip: `/runtime/${zipName}`, hosted: HOSTED }, null, 2) + '\n')

const mb = (n) => `${(n / 1024 / 1024).toFixed(1)} MB`
console.log(`build-runtime: ${relative(root, outDir)} (${listed.length + 1} files) + ${relative(root, join(runtimeRoot, zipName))} (${mb(zip.length)})`)

// ---------------------------------------------------------------------------

/**
 * A minimal PKZIP writer: one local header + data per entry, then the
 * central directory and the end record. DEFLATE (method 8) when it helps,
 * else stored (method 0). No zip64 — fine below 4 GB, which this is far
 * from. Every unzip tool (Explorer, macOS Archive Utility, `unzip`)
 * reads it.
 */
function writeZip(files) {
  const chunks = []
  const central = []
  let offset = 0
  for (const file of files) {
    const name = Buffer.from(file.name, 'utf8')
    const crc = crc32(file.data)
    const deflated = deflateRawSync(file.data, { level: 9 })
    const useDeflate = deflated.length < file.data.length
    const body = useDeflate ? deflated : file.data
    const method = useDeflate ? 8 : 0
    const { time, date } = dosDateTime(file.mtime)

    const local = Buffer.alloc(30)
    local.writeUInt32LE(0x04034b50, 0) // local file header signature
    local.writeUInt16LE(20, 4) // version needed (2.0)
    local.writeUInt16LE(0x0800, 6) // flags: UTF-8 names
    local.writeUInt16LE(method, 8)
    local.writeUInt16LE(time, 10)
    local.writeUInt16LE(date, 12)
    local.writeUInt32LE(crc, 14)
    local.writeUInt32LE(body.length, 18) // compressed size
    local.writeUInt32LE(file.data.length, 22) // uncompressed size
    local.writeUInt16LE(name.length, 26)
    local.writeUInt16LE(0, 28) // extra length
    chunks.push(local, name, body)

    const entry = Buffer.alloc(46)
    entry.writeUInt32LE(0x02014b50, 0) // central directory signature
    entry.writeUInt16LE(20, 4) // version made by
    entry.writeUInt16LE(20, 6) // version needed
    entry.writeUInt16LE(0x0800, 8)
    entry.writeUInt16LE(method, 10)
    entry.writeUInt16LE(time, 12)
    entry.writeUInt16LE(date, 14)
    entry.writeUInt32LE(crc, 16)
    entry.writeUInt32LE(body.length, 20)
    entry.writeUInt32LE(file.data.length, 24)
    entry.writeUInt16LE(name.length, 28)
    entry.writeUInt16LE(0, 30) // extra
    entry.writeUInt16LE(0, 32) // comment
    entry.writeUInt16LE(0, 34) // disk number
    entry.writeUInt16LE(0, 36) // internal attributes
    entry.writeUInt32LE(0, 38) // external attributes
    entry.writeUInt32LE(offset, 42) // local header offset
    central.push(entry, name)

    offset += local.length + name.length + body.length
  }
  const centralSize = central.reduce((n, b) => n + b.length, 0)
  const end = Buffer.alloc(22)
  end.writeUInt32LE(0x06054b50, 0) // end of central directory signature
  end.writeUInt16LE(0, 4)
  end.writeUInt16LE(0, 6)
  end.writeUInt16LE(files.length, 8)
  end.writeUInt16LE(files.length, 10)
  end.writeUInt32LE(centralSize, 12)
  end.writeUInt32LE(offset, 16)
  end.writeUInt16LE(0, 20)
  return Buffer.concat([...chunks, ...central, end])
}

/** MS-DOS time / date fields (2-second resolution, local time). */
function dosDateTime(d) {
  const year = Math.max(1980, d.getFullYear())
  return {
    time: (d.getHours() << 11) | (d.getMinutes() << 5) | Math.floor(d.getSeconds() / 2),
    date: ((year - 1980) << 9) | ((d.getMonth() + 1) << 5) | d.getDate(),
  }
}

/** CRC-32 (IEEE 802.3), as zip wants it. (The table hangs off the
 * function: top-level `let`s below the main code would still be in their
 * temporal dead zone when it runs.) */
function crc32(bytes) {
  let crcTable = crc32.table
  if (!crcTable) {
    crcTable = crc32.table = new Uint32Array(256)
    for (let n = 0; n < 256; n++) {
      let c = n
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
      crcTable[n] = c >>> 0
    }
  }
  let crc = 0xffffffff
  for (let i = 0; i < bytes.length; i++) crc = crcTable[(crc ^ bytes[i]) & 0xff] ^ (crc >>> 8)
  return (crc ^ 0xffffffff) >>> 0
}

function readme() {
  return `# Toasty shared runtime ${version}

The full Toasty (an1mbyt3) game engine for websites, as one folder that any
number of pages and games can share. Games exported with
**Export → Game → Website embed (shared runtime)** load the engine from
here instead of carrying their own 20+ MB copy.

## Use the hosted copy (nothing to install)

\`\`\`html
<div id="game" style="width:100%;height:480px"></div>
<script type="module">
  const runtime = '${HOSTED}'
  const { createPlayer } = await import(runtime + 'an1mbyt3-embed.js')
  await createPlayer(document.getElementById('game'), { game: './game.teab', runtimeUrl: runtime })
</script>
\`\`\`

Or as a tag (any number per page):

\`\`\`html
<script type="module">
  const { defineAn1PlayerElement } = await import('${HOSTED}an1mbyt3-embed.js')
  defineAn1PlayerElement()
</script>
<an1-player game="./hero.teab" runtime="${HOSTED}" style="height:400px"></an1-player>
<an1-player game="./footer.teab" runtime="${HOSTED}" style="height:200px"></an1-player>
\`\`\`

## Self-host it

1. Unzip \`toasty-runtime-${version}.zip\` and put the
   \`toasty-runtime-${version}/\` folder anywhere your site serves static
   files, e.g. \`/vendor/toasty-runtime-${version}/\`.
2. Point the pages at it instead of the hosted URL:
   \`runtimeUrl: '/vendor/toasty-runtime-${version}/'\` (and import
   \`an1mbyt3-embed.js\` from the same folder).
3. Serve \`.wasm\` as \`application/wasm\` (most hosts already do). If the
   runtime is on a different origin than the page, it must send
   \`Access-Control-Allow-Origin\` (module scripts and the wasm fetch are
   CORS requests).

## What's in here

| file | what |
| --- | --- |
| \`an1mbyt3-embed.js\` | \`createPlayer\`, \`setRuntimeUrl\`, \`<an1-player>\` — plain JS, no dependencies |
| \`an1mbyt3-react.js\` | \`<An1Player>\` — imports \`react\`, so use it through a bundler (copy it into your app), not from a URL |
| \`an1mbyt3-engine.js\` | engine glue shared by the two above |
| \`an1mbyt3_wasm_bg.wasm\` | the engine itself (downloaded once per page, shared by every player on it) |
| \`an1mbyt3-lite*.js\`, \`an1mbyt3-runtime.js\` | Toasty Lite, the tiny vector-animation runtime |
| \`*.d.ts\` | TypeScript declarations |
| \`runtime.json\` | version, build time, sizes and SHA-256 of every file |

## Versions

This folder is version **${version}**. Pages keep loading the version in
their URL, so a new runtime never changes a page that didn't opt in. A game
should be played with the runtime version it was exported for (or newer,
when the release notes say it's compatible).

## Many players on one page

Every player is independent (own canvas, game, data and events); the
engine is downloaded and compiled once. Players scrolled out of view pause
(\`pauseWhenOffscreen: false\` / the \`run-offscreen\` attribute keeps one
running). Each full-engine player owns a GPU context, and browsers limit
those per page (Chrome: ~16 WebGL contexts), so for dozens of small
animations use Toasty Lite instead.
`
}
