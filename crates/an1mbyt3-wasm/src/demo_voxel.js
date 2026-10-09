// Voxel Craft — a Minetest / Minecraft-style block sandbox (see
// `demo_voxel.rs`). This script owns the world: it generates 16 × H × 16
// chunks from a seed as the player moves, keeps the authoritative copy of
// every loaded chunk (`g.chunks`) for collision, aiming and mobs, and
// mirrors it to the engine's streamed block world (`ref.setVoxelChunk` to
// load, `ref.unloadVoxelChunk` to drop, `ref.setVoxel` per edit), which
// meshes and draws it.
//
// Prefix constants (from Rust): H (world height), DEBRIS / MOBS (pool
// sizes), ATLAS_* + TILE (the block texture atlas, base64 RGBA, for HUD
// icons) and BLOCKS (every block type by id: name + top / side / bottom
// tiles). W / D are unused (the world has no edge).
const ref = (id) => getCompByID(id)
const keys = ctx.keysDown
const dt = Math.min(ctx.deltaSeconds || 1 / 60, 1 / 20)
const AIR = 0
const ID = {}
BLOCKS.forEach((b, i) => { ID[b.name] = i })
const WATER = ID.water, BEDROCK = ID.bedrock
/** Chunk width (x and z), matching the engine's streamed columns. */
const CH = 16
/** Chunks are loaded within this many chunks of the player (a circle) and
 * dropped beyond UNLOAD_RADIUS. */
const LOAD_RADIUS = 8, UNLOAD_RADIUS = 10
/** Milliseconds of chunk generation allowed per frame. */
const GEN_BUDGET_MS = 7
/** The sea level: everything below it that isn't ground is water. */
const SEA = 30
/** Player box: half width, height, eye height. */
const HW = 0.3, PH = 1.75, EYE = 1.62
const REACH = 6, PUNCH_REACH = 3.6
const MAX_HP = 20
/** Mob box and stats. */
const MOB_HW = 0.3, MOB_H = 1.8, MOB_HP = 12, MOB_SPEED = 2.6, MOB_DAMAGE = 3, MAX_MOBS = 7
const SKY = [0.55, 0.74, 0.96]
const clampv = (v, lo, hi) => Math.max(lo, Math.min(hi, v))
/** A chunk's map key (chunk coordinates within ±32768). */
const ck = (cx, cz) => (cx + 32768) * 65536 + (cz + 32768)
/** A block's index inside its chunk. */
const li = (lx, y, lz) => lx + CH * (lz + CH * y)

// ------------------------------------------------------------- noise --
// Integer hash → [0, 1), then smooth value noise and a few octaves of it.
function hash3(x, y, z, s) {
  let h = Math.imul(x | 0, 374761393) ^ Math.imul(y | 0, 668265263) ^ Math.imul(z | 0, 1274126177) ^ Math.imul(s | 0, 1442695041)
  h = Math.imul(h ^ (h >>> 13), 1274126177)
  h ^= h >>> 16
  return (h >>> 0) / 4294967296
}
function noise2(x, z, s) {
  const xi = Math.floor(x), zi = Math.floor(z)
  const xf = x - xi, zf = z - zi
  const u = xf * xf * (3 - 2 * xf), v = zf * zf * (3 - 2 * zf)
  const a = hash3(xi, 0, zi, s), b = hash3(xi + 1, 0, zi, s), c = hash3(xi, 0, zi + 1, s), d = hash3(xi + 1, 0, zi + 1, s)
  return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}
// (Written out flat — no closures — since cave carving calls it for
// thousands of blocks per chunk.)
function noise3(x, y, z, s) {
  const xi = Math.floor(x), yi = Math.floor(y), zi = Math.floor(z)
  const xf = x - xi, yf = y - yi, zf = z - zi
  const u = xf * xf * (3 - 2 * xf), v = yf * yf * (3 - 2 * yf), w = zf * zf * (3 - 2 * zf)
  const a0 = hash3(xi, yi, zi, s), a1 = hash3(xi + 1, yi, zi, s), a2 = hash3(xi, yi + 1, zi, s), a3 = hash3(xi + 1, yi + 1, zi, s)
  const b0 = hash3(xi, yi, zi + 1, s), b1 = hash3(xi + 1, yi, zi + 1, s), b2 = hash3(xi, yi + 1, zi + 1, s), b3 = hash3(xi + 1, yi + 1, zi + 1, s)
  const a = a0 + (a1 - a0) * u + (a2 + (a3 - a2) * u - a0 - (a1 - a0) * u) * v
  const b = b0 + (b1 - b0) * u + (b2 + (b3 - b2) * u - b0 - (b1 - b0) * u) * v
  return a + (b - a) * w
}
function fbm2(x, z, s) {
  let sum = 0, amp = 0.5, norm = 0
  for (let o = 0; o < 4; o++) {
    sum += noise2(x, z, s + o * 97) * amp
    norm += amp
    x *= 2; z *= 2; amp *= 0.5
  }
  return sum / norm
}

// --------------------------------------------------------- world gen --
// Everything is a pure function of the seed and world coordinates, so any
// chunk can be generated on its own, in any order, and comes out the same
// every time (edits are replayed on top).

/** Terrain height at a column: continents vs oceans at a large scale,
 * hills on top, and mountains rising out of the inland. */
function heightAt(x, z, seed) {
  const cont = fbm2(x / 260, z / 260, seed)
  const base = SEA + (cont - 0.5) * 64
  const hills = (fbm2(x / 44, z / 44, seed + 5) - 0.5) * 16
  const inland = clampv((cont - 0.47) * 6, 0, 1)
  const m = Math.max(0, noise2(x / 120, z / 120, seed + 23) - 0.48) / 0.52
  return clampv(Math.floor(base + hills + m * m * 46 * inland), 4, H - 12)
}
function snowLine(x, z, seed) {
  return 56 + Math.floor(hash3(x, 0, z, seed + 5) * 3)
}
/** A tree's trunk height if one grows at this column, else 0. */
function treeAt(x, z, seed, h) {
  if (h <= SEA + 1 || h >= snowLine(x, z, seed) - 2) return 0
  const forest = noise2(x / 30, z / 30, seed + 71)
  if (hash3(x, 0, z, seed + 72) > (forest > 0.56 ? 0.06 : 0.005)) return 0
  return 4 + Math.floor(hash3(x, 1, z, seed + 73) * 3)
}
/** Trilinear sample of a per-chunk field stored on a 4-block lattice
 * (`nx × nx × ny` points, x fastest, then z, then y). */
function latticeAt(f, nx, ny, lx, y, lz) {
  const fx = lx / 4, fy = y / 4, fz = lz / 4
  const i = Math.min(Math.floor(fx), nx - 2), j = Math.min(Math.floor(fy), ny - 2), k = Math.min(Math.floor(fz), nx - 2)
  const u = fx - i, v = fy - j, w = fz - k
  const n = i + nx * (k + nx * j), dz = nx, dy = nx * nx
  const c00 = f[n] + (f[n + 1] - f[n]) * u, c01 = f[n + dz] + (f[n + dz + 1] - f[n + dz]) * u
  const c10 = f[n + dy] + (f[n + dy + 1] - f[n + dy]) * u, c11 = f[n + dy + dz] + (f[n + dy + dz + 1] - f[n + dy + dz]) * u
  const c0 = c00 + (c01 - c00) * w, c1 = c10 + (c11 - c10) * w
  return c0 + (c1 - c0) * v
}
/** Generates chunk (cx, cz): strata, sea, caves, ores, then any trees
 * whose canopies reach into it (from up to 2 blocks outside). */
function genChunk(cx, cz, seed) {
  const b = new Uint8Array(CH * H * CH)
  const x0 = cx * CH, z0 = cz * CH
  // Column heights for the chunk plus a 2-block margin (trees next door).
  const M = 2, SPAN = CH + 2 * M
  const heights = new Int16Array(SPAN * SPAN)
  for (let z = 0; z < SPAN; z++) for (let x = 0; x < SPAN; x++) heights[x + SPAN * z] = heightAt(x0 + x - M, z0 + z - M, seed)
  // The cave field, sampled on a 4-block lattice and blended between
  // (caves are ~10+ blocks across, so this looks the same and costs a
  // fraction): how far each of the two cave noises is from its middle.
  const S = 4, NX = CH / S + 1, NY = Math.ceil(H / S) + 1
  const caveA = new Float32Array(NX * NX * NY), caveB = new Float32Array(NX * NX * NY)
  for (let j = 0; j < NY; j++) {
    for (let k = 0; k < NX; k++) {
      for (let i = 0; i < NX; i++) {
        const x = x0 + i * S, y = j * S, z = z0 + k * S, n = i + NX * (k + NX * j)
        caveA[n] = Math.abs(noise3(x / 15, y / 9, z / 15, seed + 41) - 0.5)
        caveB[n] = Math.abs(noise3(x / 15, y / 9, z / 15, seed + 53) - 0.5)
      }
    }
  }
  for (let lz = 0; lz < CH; lz++) {
    for (let lx = 0; lx < CH; lx++) {
      const x = x0 + lx, z = z0 + lz
      const h = heights[lx + M + SPAN * (lz + M)]
      const beach = h <= SEA + 1
      const snowy = h >= snowLine(x, z, seed)
      const gravelBed = noise2(x / 9, z / 9, seed + 31) > 0.66
      for (let y = 0; y <= h; y++) {
        let id
        if (y === 0 || (y === 1 && hash3(x, y, z, seed + 1) < 0.5)) id = BEDROCK
        else if (y < h - 3) id = ID.stone
        else if (y < h) id = beach ? ID.sand : ID.dirt
        else if (h < SEA) id = gravelBed ? ID.gravel : ID.sand
        else if (beach) id = ID.sand
        else if (snowy) id = ID.snow
        else id = ID.grass
        b[li(lx, y, lz)] = id
      }
      for (let y = h + 1; y <= SEA; y++) b[li(lx, y, lz)] = WATER
      // Caves where two 3D noises are both near their middle (long winding
      // tunnels); allowed to open on dry land, sealed under water.
      const top = h > SEA + 2 ? h : h - 4
      for (let y = 2; y <= top; y++) {
        if (latticeAt(caveA, NX, NY, lx, y, lz) < 0.06 && latticeAt(caveB, NX, NY, lx, y, lz) < 0.09) b[lx + CH * (lz + CH * y)] = AIR
      }
      // Ores: 2×2×2 clusters, rarer and deeper the more valuable.
      for (let y = 1; y < h - 3; y++) {
        const i = li(lx, y, lz)
        if (b[i] !== ID.stone) continue
        const r = hash3(x >> 1, y >> 1, z >> 1, seed + 61)
        if (r >= 0.05 || hash3(x, y, z, seed + 62) > 0.6) continue
        if (r < 0.03) b[i] = ID['coal ore']
        else if (y < 40 && r < 0.042) b[i] = ID['iron ore']
        else if (y < 24 && r < 0.047) b[i] = ID['gold ore']
        else if (y < 15 && r < 0.05) b[i] = ID['diamond ore']
      }
    }
  }
  // Trees (including neighbours' canopies that spill into this chunk).
  const put = (x, y, z, id, onlyAir) => {
    const lx = x - x0, lz = z - z0
    if (lx < 0 || lz < 0 || lx >= CH || lz >= CH || y < 0 || y >= H) return
    const i = li(lx, y, lz)
    if (!onlyAir || b[i] === AIR) b[i] = id
  }
  for (let z = z0 - M; z < z0 + CH + M; z++) {
    for (let x = x0 - M; x < x0 + CH + M; x++) {
      const h = heights[x - x0 + M + SPAN * (z - z0 + M)]
      const trunk = treeAt(x, z, seed, h)
      if (!trunk) continue
      const top = h + trunk
      for (let ly = top - 2; ly <= top + 1; ly++) {
        const r = ly >= top ? 1 : 2
        for (let dz = -r; dz <= r; dz++) {
          for (let dx = -r; dx <= r; dx++) {
            if (Math.abs(dx) === r && Math.abs(dz) === r && (ly >= top || hash3(x + dx, ly, z + dz, seed + 74) < 0.5)) continue
            put(x + dx, ly, z + dz, ID.leaves, true)
          }
        }
      }
      for (let y = h + 1; y < top; y++) put(x, y, z, ID.log, false)
      put(x, h, z, ID.dirt, false)
    }
  }
  return b
}

/** Where to stand: the dry, open (no trees within 3) column nearest the
 * origin. */
function spawnPoint(seed) {
  for (let r = 0; r < 400; r += 2) {
    const n = Math.max(1, r * 4)
    for (let k = 0; k < n; k++) {
      const a = (k / n) * Math.PI * 2
      const x = Math.round(Math.cos(a) * r), z = Math.round(Math.sin(a) * r)
      const h = heightAt(x, z, seed)
      if (h <= SEA + 1 || h >= snowLine(x, z, seed)) continue
      // No tree here or close enough for its canopy to box you in.
      let clear = true
      for (let dz = -3; dz <= 3 && clear; dz++) for (let dx = -3; dx <= 3 && clear; dx++) if (treeAt(x + dx, z + dz, seed, heightAt(x + dx, z + dz, seed))) clear = false
      if (clear) return { x: x + 0.5, y: h + 1, z: z + 0.5 }
    }
  }
  return { x: 0.5, y: H - 2, z: 0.5 }
}

// ---------------------------------------------------- block icons --
// The hotbar / inventory icons: each block drawn as a little isometric
// cube from its atlas tiles (top lit, the two sides shaded), once.
function buildIcons() {
  const bin = atob(ATLAS_B64)
  const px = new Uint8ClampedArray(bin.length)
  for (let i = 0; i < bin.length; i++) px[i] = bin.charCodeAt(i)
  const atlas = document.createElement('canvas')
  atlas.width = ATLAS_W; atlas.height = ATLAS_H
  atlas.getContext('2d').putImageData(new ImageData(px, ATLAS_W, ATLAS_H), 0, 0)
  const S = 32, C = 0.866 * S
  return BLOCKS.map((b, id) => {
    if (id === AIR) return ''
    const c = document.createElement('canvas')
    c.width = Math.ceil(2 * C); c.height = 2 * S
    const x = c.getContext('2d')
    x.imageSmoothingEnabled = false
    const k = S / TILE
    const face = (tile, m, shade) => {
      x.setTransform(m[0] * k, m[1] * k, m[2] * k, m[3] * k, m[4], m[5])
      x.globalCompositeOperation = 'source-over'
      x.drawImage(atlas, (tile % ATLAS_COLS) * TILE, Math.floor(tile / ATLAS_COLS) * TILE, TILE, TILE, 0, 0, TILE, TILE)
      if (shade > 0) {
        x.globalCompositeOperation = 'source-atop'
        x.fillStyle = `rgba(0,0,0,${shade})`
        x.fillRect(0, 0, TILE, TILE)
      }
    }
    face(b.top, [0.866, 0.5, -0.866, 0.5, C, 0], 0)
    face(b.side, [0.866, 0.5, 0, 1, 0, S / 2], 0.22)
    face(b.side, [0.866, -0.5, 0, 1, C, S], 0.4)
    return c.toDataURL()
  })
}

// ------------------------------------------------------------ state --
function landSeed() {
  // A seed whose origin is on land, so the first look around isn't the
  // open sea.
  for (let tries = 0; tries < 40; tries++) {
    const seed = Math.floor(Math.random() * 1e6)
    if (heightAt(0, 0, seed) > SEA + 2) return seed
  }
  return 1234
}
let g = ctx.globals.vc
if (!g) {
  const saved = ctx.storage.get('voxelcraft2') || {}
  const seed = saved.seed ?? landSeed()
  const spawn = spawnPoint(seed)
  const at = saved.player || spawn
  g = ctx.globals.vc = {
    seed, edits: saved.edits || {}, chunks: new Map(), spawn, missing: [], missingAt: -1,
    pos: { x: at.x, y: at.y, z: at.z }, vel: { x: 0, y: 0, z: 0 },
    yaw: saved.yaw ?? 0, pitch: saved.pitch ?? -10, onGround: false, fly: !!saved.fly,
    hp: saved.hp ?? MAX_HP, dead: false, hurtT: 0, regenT: 0,
    slot: 0, hotbar: saved.hotbar || ['grass', 'dirt', 'stone', 'cobblestone', 'planks', 'log', 'leaves', 'glass', 'bricks'].map((n) => ID[n]),
    breakT: 0, placeT: 0, inv: false, dirty: false, saveT: 0, toastT: 0, toast: '',
    swing: 0, bob: 0, stepT: 0, heldId: -1, heldBase: null, under: null, prev: {}, fps: 60, debugT: 0, unloadT: 0,
    debris: Array.from({ length: DEBRIS }, () => ({ life: 0, x: 0, y: 0, z: 0, vx: 0, vy: 0, vz: 0 })), nextDebris: 0,
    mobs: Array.from({ length: MOBS }, () => ({ on: false, x: 0, y: 0, z: 0, vx: 0, vy: 0, vz: 0, yaw: 0, hp: MOB_HP, hurtT: 0, attackT: 0, onGround: false, walk: 0, wanderT: 0, wanderYaw: null, groanT: 3, flashing: false })),
    spawnT: 8, peaceful: !!saved.peaceful, kills: saved.kills || 0,
    icons: null, hudDom: null, invDom: null, target: null,
  }
}
const pressed = (k) => keys.has(k) && !g.prev[k]

// -------------------------------------------------- chunk streaming --
function loadChunk(cx, cz) {
  const b = genChunk(cx, cz, g.seed)
  const edits = g.edits[ck(cx, cz)]
  if (edits) for (const [i, id] of Object.entries(edits)) b[Number(i)] = id
  g.chunks.set(ck(cx, cz), b)
  ref('world').setVoxelChunk(cx, cz, b)
}
function unloadChunk(key) {
  g.chunks.delete(key)
  ref('world').unloadVoxelChunk(Math.floor(key / 65536) - 32768, (key % 65536) - 32768)
}
/** Generates missing chunks around the player, nearest first, within the
 * frame's time budget (always at least one). */
function streamChunks() {
  const pcx = Math.floor(g.pos.x / CH), pcz = Math.floor(g.pos.z / CH)
  if (g.missingAt !== ck(pcx, pcz)) {
    g.missingAt = ck(pcx, pcz)
    g.missing = []
    for (let dz = -LOAD_RADIUS; dz <= LOAD_RADIUS; dz++) {
      for (let dx = -LOAD_RADIUS; dx <= LOAD_RADIUS; dx++) {
        const d = dx * dx + dz * dz
        if (d <= LOAD_RADIUS * LOAD_RADIUS && !g.chunks.has(ck(pcx + dx, pcz + dz))) g.missing.push([d, pcx + dx, pcz + dz])
      }
    }
    // Nearest last, so `pop()` takes it.
    g.missing.sort((a, b) => b[0] - a[0])
  }
  const start = performance.now()
  while (g.missing.length) {
    const [, cx, cz] = g.missing.pop()
    if (!g.chunks.has(ck(cx, cz))) loadChunk(cx, cz)
    if (performance.now() - start > GEN_BUDGET_MS) break
  }
  // Every so often, drop chunks far behind.
  g.unloadT -= dt
  if (g.unloadT <= 0) {
    g.unloadT = 1
    for (const key of [...g.chunks.keys()]) {
      const cx = Math.floor(key / 65536) - 32768, cz = (key % 65536) - 32768
      if (Math.max(Math.abs(cx - pcx), Math.abs(cz - pcz)) > UNLOAD_RADIUS) unloadChunk(key)
    }
  }
}

// -------------------------------------------------------- the world --
/** The block at a world position; `-1` where the chunk isn't loaded. */
function blockAt(x, y, z) {
  if (y < 0 || y >= H) return AIR
  const cx = Math.floor(x / CH), cz = Math.floor(z / CH)
  const c = g.chunks.get(ck(cx, cz))
  if (!c) return -1
  return c[li(x - cx * CH, y, z - cz * CH)]
}
/** Blocks movement: anything but air and water. The floor is solid, and
 * so is unloaded world (nothing falls into the void). */
function solid(x, y, z) {
  if (y < 0) return true
  const b = blockAt(x, y, z)
  return b !== AIR && b !== WATER
}
function setBlock(x, y, z, id) {
  if (y < 0 || y >= H) return
  const cx = Math.floor(x / CH), cz = Math.floor(z / CH)
  const key = ck(cx, cz)
  const c = g.chunks.get(key)
  if (!c) return
  const i = li(x - cx * CH, y, z - cz * CH)
  c[i] = id
  ;(g.edits[key] || (g.edits[key] = {}))[i] = id
  g.dirty = true
  ref('world').setVoxel(x, y, z, id)
}
/** Water next to a freshly dug hole runs in (a little flood fill, at or
 * below the sea line). */
function flood(x, y, z) {
  const queue = [[x, y, z]]
  let budget = 48
  while (queue.length && budget > 0) {
    const [cx, cy, cz] = queue.shift()
    if (blockAt(cx, cy, cz) !== AIR || cy > SEA) continue
    const wet = [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1], [0, 1, 0]].some(([dx, dy, dz]) => blockAt(cx + dx, cy + dy, cz + dz) === WATER)
    if (!wet) continue
    setBlock(cx, cy, cz, WATER)
    budget--
    for (const [dx, dy, dz] of [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1], [0, -1, 0]]) queue.push([cx + dx, cy + dy, cz + dz])
  }
}

// ------------------------------------------------ bodies + collision --
// Shared by the player and mobs: an axis-aligned box `hw` wide (half),
// `h` tall, standing at `p` (feet centre).
function collides(p, hw, h) {
  const x0 = Math.floor(p.x - hw), x1 = Math.floor(p.x + hw)
  const y0 = Math.floor(p.y), y1 = Math.floor(p.y + h)
  const z0 = Math.floor(p.z - hw), z1 = Math.floor(p.z + hw)
  for (let y = y0; y <= y1; y++) for (let z = z0; z <= z1; z++) for (let x = x0; x <= x1; x++) if (solid(x, y, z)) return true
  return false
}
/** Moves `p` along one axis in small steps; on hitting a block, snaps
 * flush against it. Returns whether it hit. */
function moveAxis(p, axis, d, hw, h) {
  const steps = Math.max(1, Math.ceil(Math.abs(d) / 0.3))
  for (let i = 0; i < steps; i++) {
    const before = p[axis]
    p[axis] += d / steps
    if (!collides(p, hw, h)) continue
    if (axis === 'y') p.y = d < 0 ? Math.floor(p.y) + 1 : Math.floor(p.y + h) - h - 0.001
    else if (d > 0) p[axis] = Math.floor(p[axis] + hw) - hw - 0.001
    else p[axis] = Math.floor(p[axis] - hw) + 1 + hw + 0.001
    if (collides(p, hw, h)) p[axis] = before
    return true
  }
  return false
}
/** The block the eye ray hits first (water and air are passed through),
 * with the face it entered by — a voxel DDA walk. */
function raycast(o, d, max) {
  let x = Math.floor(o.x), y = Math.floor(o.y), z = Math.floor(o.z)
  const sx = Math.sign(d.x), sy = Math.sign(d.y), sz = Math.sign(d.z)
  const inv = (v) => (v !== 0 ? 1 / Math.abs(v) : Infinity)
  const dX = inv(d.x), dY = inv(d.y), dZ = inv(d.z)
  let tX = d.x !== 0 ? (sx > 0 ? x + 1 - o.x : o.x - x) * dX : Infinity
  let tY = d.y !== 0 ? (sy > 0 ? y + 1 - o.y : o.y - y) * dY : Infinity
  let tZ = d.z !== 0 ? (sz > 0 ? z + 1 - o.z : o.z - z) * dZ : Infinity
  let nx = 0, ny = 0, nz = 0, t = 0
  while (t <= max) {
    const b = blockAt(x, y, z)
    if (b === -1) return null
    if (b !== AIR && b !== WATER) return { x, y, z, nx, ny, nz, block: b, t }
    if (tX < tY && tX < tZ) { x += sx; t = tX; tX += dX; nx = -sx; ny = 0; nz = 0 }
    else if (tY < tZ) { y += sy; t = tY; tY += dY; nx = 0; ny = -sy; nz = 0 }
    else { z += sz; t = tZ; tZ += dZ; nx = 0; ny = 0; nz = -sz }
  }
  return null
}
/** Ray vs a mob's box (slab test): the distance along the ray, or null. */
function rayBox(o, d, m) {
  const min = [m.x - MOB_HW, m.y, m.z - MOB_HW], max = [m.x + MOB_HW, m.y + MOB_H, m.z + MOB_HW]
  const oo = [o.x, o.y, o.z], dd = [d.x, d.y, d.z]
  let t0 = 0, t1 = Infinity
  for (let a = 0; a < 3; a++) {
    if (Math.abs(dd[a]) < 1e-9) {
      if (oo[a] < min[a] || oo[a] > max[a]) return null
      continue
    }
    let ta = (min[a] - oo[a]) / dd[a], tb = (max[a] - oo[a]) / dd[a]
    if (ta > tb) [ta, tb] = [tb, ta]
    t0 = Math.max(t0, ta); t1 = Math.min(t1, tb)
    if (t0 > t1) return null
  }
  return t0
}

// ------------------------------------------------------------ input --
// The pointer lock comes with the next click on the canvas (browsers need
// a real gesture); a click that only grabs the mouse doesn't dig.
if (!g.inv && !ctx.pointerLocked && ctx.requestPointerLock) ctx.requestPointerLock()
const playing = ctx.pointerLocked && !g.inv && !g.dead
if (pressed('e') && !g.dead) {
  g.inv = !g.inv
  if (g.inv && ctx.exitPointerLock) ctx.exitPointerLock()
}
if (pressed('escape') && g.inv) g.inv = false
if (pressed('f')) { g.fly = !g.fly; g.vel.y = 0; showToast(g.fly ? 'FLY MODE' : 'WALK MODE') }
if (pressed('p')) {
  g.peaceful = !g.peaceful
  if (g.peaceful) for (const m of g.mobs) m.on = false
  showToast(g.peaceful ? 'PEACEFUL — NO ZOMBIES' : 'ZOMBIES ON')
  g.dirty = true
}
if (pressed('n')) {
  // A fresh world (new seed, no edits).
  for (const key of [...g.chunks.keys()]) unloadChunk(key)
  g.seed = landSeed()
  g.edits = {}
  g.spawn = spawnPoint(g.seed)
  g.pos = { ...g.spawn }; g.vel = { x: 0, y: 0, z: 0 }
  g.missingAt = -1
  for (const m of g.mobs) m.on = false
  g.dirty = true
  showToast('NEW WORLD  ·  SEED ' + g.seed)
}
for (let k = 1; k <= 9; k++) if (pressed(String(k))) g.slot = k - 1
if (ctx.wheelY && !g.inv) g.slot = (g.slot + (ctx.wheelY > 0 ? 1 : -1) + 9) % 9
if (ctx.pointerLocked && !g.dead) {
  g.yaw -= (ctx.mouseDX || 0) * 0.14
  g.pitch = clampv(g.pitch - (ctx.mouseDY || 0) * 0.14, -89.5, 89.5)
}
const yawR = (g.yaw * Math.PI) / 180, pitchR = (g.pitch * Math.PI) / 180
const look = { x: -Math.sin(yawR) * Math.cos(pitchR), y: Math.sin(pitchR), z: -Math.cos(yawR) * Math.cos(pitchR) }

streamChunks()
// Wait for the ground under you before simulating anything.
const ready = [[0, 0], [1, 0], [-1, 0], [0, 1], [0, -1]].every(([dx, dz]) => g.chunks.has(ck(Math.floor(g.pos.x / CH) + dx, Math.floor(g.pos.z / CH) + dz)))

// -------------------------------------------------------- movement --
const feet = blockAt(Math.floor(g.pos.x), Math.floor(g.pos.y + 0.4), Math.floor(g.pos.z))
const inWater = feet === WATER
const headUnder = blockAt(Math.floor(g.pos.x), Math.floor(g.pos.y + EYE + 0.05), Math.floor(g.pos.z)) === WATER
let mx = 0, mz = 0
if (playing) {
  if (keys.has('w')) { mx -= Math.sin(yawR); mz -= Math.cos(yawR) }
  if (keys.has('s')) { mx += Math.sin(yawR); mz += Math.cos(yawR) }
  if (keys.has('a')) { mx -= Math.cos(yawR); mz += Math.sin(yawR) }
  if (keys.has('d')) { mx += Math.cos(yawR); mz -= Math.sin(yawR) }
}
const len = Math.hypot(mx, mz) || 1
const sprint = keys.has('control')
const sneak = keys.has('shift') && !g.fly
const speed = g.fly ? (sprint ? 24 : 11) : inWater ? 2.8 : sneak ? 1.6 : sprint ? 6.4 : 4.3
const accel = g.fly || g.onGround || inWater ? 16 : 5
if (ready) {
  g.vel.x += ((mx / len) * speed - g.vel.x) * Math.min(1, accel * dt)
  g.vel.z += ((mz / len) * speed - g.vel.z) * Math.min(1, accel * dt)
  const jump = playing && keys.has('space')
  if (g.fly) {
    const vy = jump ? speed : playing && keys.has('shift') ? -speed : 0
    g.vel.y += (vy - g.vel.y) * Math.min(1, 12 * dt)
  } else if (inWater) {
    g.vel.y = Math.max(g.vel.y - 10 * dt, -3)
    if (jump) g.vel.y = 4.2
  } else {
    g.vel.y = Math.max(g.vel.y - 27 * dt, -48)
    if (jump && g.onGround) { g.vel.y = 8.4; g.onGround = false }
  }
  // Sneaking holds you on the edge of a drop, like Minetest.
  const wasOnGround = g.onGround
  const sx0 = g.pos.x, sz0 = g.pos.z
  if (moveAxis(g.pos, 'x', g.vel.x * dt, HW, PH)) g.vel.x = 0
  if (moveAxis(g.pos, 'z', g.vel.z * dt, HW, PH)) g.vel.z = 0
  if (sneak && wasOnGround && !collides({ x: g.pos.x, y: g.pos.y - 0.6, z: g.pos.z }, HW, PH)) { g.pos.x = sx0; g.pos.z = sz0 }
  const fallSpeed = -g.vel.y
  const hitY = moveAxis(g.pos, 'y', g.vel.y * dt, HW, PH)
  g.onGround = hitY && g.vel.y < 0
  if (hitY) {
    // Fall damage past a ~4 block drop (none into water or while flying).
    if (g.vel.y < 0 && fallSpeed > 15 && !g.fly && !inWater) {
      hurt(Math.floor((fallSpeed - 13) / 2.2), null)
      ctx.playTone({ freq: 120, to: 50, duration: 0.12, type: 'noise', volume: 0.2 })
    }
    g.vel.y = 0
  }
}

// Footsteps.
const horiz = Math.hypot(g.vel.x, g.vel.z)
if (g.onGround && horiz > 1) {
  g.bob += horiz * dt
  g.stepT -= dt * horiz
  if (g.stepT <= 0) {
    g.stepT = 2.2
    const under = blockAt(Math.floor(g.pos.x), Math.floor(g.pos.y - 0.1), Math.floor(g.pos.z))
    const soft = under === ID.grass || under === ID.dirt || under === ID.sand || under === ID.leaves || under === ID.snow || under === ID.gravel
    ctx.playTone({ freq: soft ? 90 : 160, to: soft ? 60 : 110, duration: 0.05, type: 'noise', volume: 0.05 })
  }
}

// ----------------------------------------------------------- health --
function hurt(amount, from) {
  if (amount <= 0 || g.dead) return
  g.hp = Math.max(0, g.hp - amount)
  g.hurtT = 0.35
  g.regenT = 6
  g.dirty = true
  if (from) {
    // Knocked away from whatever hit you.
    const dx = g.pos.x - from.x, dz = g.pos.z - from.z, d = Math.hypot(dx, dz) || 1
    g.vel.x += (dx / d) * 7; g.vel.z += (dz / d) * 7; g.vel.y = Math.max(g.vel.y, 5)
  }
  ctx.playTone({ freq: 300, to: 140, duration: 0.15, type: 'square', volume: 0.14 })
  if (g.hp <= 0) {
    g.dead = true
    if (ctx.exitPointerLock) ctx.exitPointerLock()
    ctx.playTone({ freq: 220, to: 55, duration: 0.8, type: 'sawtooth', volume: 0.15 })
  }
}
g.hurtT -= dt
g.regenT -= dt
if (!g.dead && g.hp < MAX_HP && g.regenT <= 0) { g.hp++; g.regenT = 3; g.dirty = true }
if (g.dead && pressed('mouse')) {
  // Respawn at the world's spawn, zombies cleared.
  g.dead = false
  g.hp = MAX_HP
  g.pos = { ...g.spawn }; g.vel = { x: 0, y: 0, z: 0 }
  for (const m of g.mobs) m.on = false
  g.leftOk = false; g.rightOk = false
}

// --------------------------------------------------- dig and build --
const camEye = { x: g.pos.x, y: g.pos.y + EYE, z: g.pos.z }
const hit = ready ? raycast(camEye, look, REACH) : null
g.target = hit
g.breakT -= dt; g.placeT -= dt
// A press only acts if it started while playing (the click that grabs the
// mouse, or closes the inventory, doesn't count).
if (pressed('mouseleft')) g.leftOk = playing
if (pressed('mouseright')) g.rightOk = playing
// Punching a zombie in reach (and in front of any block) beats digging.
let punched = false
if (playing && g.leftOk && pressed('mouseleft')) {
  let best = null, bestT = Math.min(PUNCH_REACH, hit ? hit.t : Infinity)
  for (const m of g.mobs) {
    if (!m.on) continue
    const t = rayBox(camEye, look, m)
    if (t !== null && t < bestT) { best = m; bestT = t }
  }
  if (best) {
    punched = true
    best.hp -= 4
    best.hurtT = 0.3
    best.vx += look.x * 7; best.vz += look.z * 7; best.vy = 4.5
    ctx.playTone({ freq: 180, to: 90, duration: 0.08, type: 'square', volume: 0.16 })
    if (best.hp <= 0) {
      best.on = false
      g.kills++
      g.dirty = true
      spawnDebris(Math.floor(best.x), Math.floor(best.y + 0.6), Math.floor(best.z), ID.leaves)
      ctx.playTone({ freq: 140, to: 40, duration: 0.35, type: 'sawtooth', volume: 0.14 })
    }
    g.swing = 1
    g.breakT = 0.3
  }
}
if (!punched && playing && g.leftOk && hit && keys.has('mouseleft') && g.breakT <= 0) {
  g.breakT = 0.24
  if (hit.block !== BEDROCK) {
    setBlock(hit.x, hit.y, hit.z, AIR)
    flood(hit.x, hit.y, hit.z)
    spawnDebris(hit.x, hit.y, hit.z, hit.block)
    ctx.playTone({ freq: 240, to: 70, duration: 0.09, type: 'noise', volume: 0.16 })
  }
  g.swing = 1
}
if (!keys.has('mouseleft')) g.breakT = 0
if (playing && g.rightOk && hit && keys.has('mouseright') && g.placeT <= 0) {
  g.placeT = 0.24
  const px = hit.x + hit.nx, py = hit.y + hit.ny, pz = hit.z + hit.nz
  const there = blockAt(px, py, pz)
  const id = g.hotbar[g.slot]
  const overlaps = (x, y, z, hw, h) => px + 1 > x - hw && px < x + hw && py + 1 > y && py < y + h && pz + 1 > z - hw && pz < z + hw
  const blocked = id !== WATER && (overlaps(g.pos.x, g.pos.y, g.pos.z, HW, PH) || g.mobs.some((m) => m.on && overlaps(m.x, m.y, m.z, MOB_HW, MOB_H)))
  if ((there === AIR || there === WATER) && py >= 0 && py < H && !blocked) {
    setBlock(px, py, pz, id)
    ctx.playTone({ freq: 320, to: 180, duration: 0.07, type: 'triangle', volume: 0.14 })
    g.swing = 1
  }
}
if (!keys.has('mouseright')) g.placeT = 0

function spawnDebris(x, y, z, block) {
  for (let k = 0; k < 6; k++) {
    const i = g.nextDebris
    g.nextDebris = (i + 1) % DEBRIS
    const d = g.debris[i]
    Object.assign(d, { life: 0.7 + Math.random() * 0.3, x: x + 0.3 + Math.random() * 0.4, y: y + 0.3 + Math.random() * 0.4, z: z + 0.3 + Math.random() * 0.4, vx: (Math.random() - 0.5) * 4, vy: 2 + Math.random() * 3, vz: (Math.random() - 0.5) * 4 })
    const r = ref('debris' + i)
    r.setVoxels([block])
    r.visible = true
  }
}
for (let i = 0; i < DEBRIS; i++) {
  const d = g.debris[i]
  if (d.life <= 0) continue
  d.life -= dt
  const r = ref('debris' + i)
  if (d.life <= 0) { r.visible = false; continue }
  d.vy -= 22 * dt
  const nx = d.x + d.vx * dt, ny = d.y + d.vy * dt, nz = d.z + d.vz * dt
  if (solid(Math.floor(nx), Math.floor(ny), Math.floor(nz))) { d.vx *= 0.4; d.vz *= 0.4; d.vy = 0 } else { d.x = nx; d.y = ny; d.z = nz }
  const s = 0.16 * Math.min(1, d.life * 2.5)
  r.scale3 = { x: s, y: s, z: s }
  r.position3 = { x: d.x - s / 2, y: d.y - s / 2, z: d.z - s / 2 }
}

// ---------------------------------------------------------- zombies --
// Spawn on open ground 16–34 blocks away (never in water or on leaves),
// chase you when close, wander otherwise; hit when in reach; jump up
// single-block steps; despawn when far.
function surfaceAt(x, z) {
  for (let y = H - 2; y > 1; y--) {
    const b = blockAt(x, y, z)
    if (b === -1) return null
    if (b === AIR) continue
    if (b === WATER || b === ID.leaves) return null
    return blockAt(x, y + 1, z) === AIR && blockAt(x, y + 2, z) === AIR ? y + 1 : null
  }
  return null
}
g.spawnT -= dt
if (ready && !g.peaceful && !g.dead && g.spawnT <= 0) {
  g.spawnT = 3.5
  const free = g.mobs.find((m) => !m.on)
  if (free && g.mobs.filter((m) => m.on).length < MAX_MOBS) {
    for (let tries = 0; tries < 6; tries++) {
      const a = Math.random() * Math.PI * 2, r = 16 + Math.random() * 18
      const x = Math.floor(g.pos.x + Math.cos(a) * r), z = Math.floor(g.pos.z + Math.sin(a) * r)
      const y = surfaceAt(x, z)
      if (y === null) continue
      Object.assign(free, { on: true, x: x + 0.5, y, z: z + 0.5, vx: 0, vy: 0, vz: 0, hp: MOB_HP, hurtT: 0, attackT: 1, wanderT: 0, groanT: 2 + Math.random() * 4 })
      break
    }
  }
}
for (let i = 0; i < MOBS; i++) {
  const m = g.mobs[i]
  const root = ref('mob' + i)
  if (!m.on) {
    if (root.visible) root.visible = false
    continue
  }
  const dx = g.pos.x - m.x, dz = g.pos.z - m.z
  const dist = Math.hypot(dx, dz)
  if (dist > 80 || !g.chunks.has(ck(Math.floor(m.x / CH), Math.floor(m.z / CH)))) { m.on = false; root.visible = false; continue }
  // Brain: chase within 22 blocks, else amble about.
  let wantX = 0, wantZ = 0
  if (!g.dead && dist < 22) {
    m.yaw = Math.atan2(-dx, -dz)
    if (dist > 0.9) { wantX = (dx / dist) * MOB_SPEED; wantZ = (dz / dist) * MOB_SPEED }
  } else {
    m.wanderT -= dt
    if (m.wanderT <= 0) { m.wanderT = 2 + Math.random() * 4; m.wanderYaw = Math.random() < 0.4 ? null : Math.random() * Math.PI * 2 }
    if (m.wanderYaw !== null) { m.yaw = m.wanderYaw; wantX = -Math.sin(m.yaw) * 1.1; wantZ = -Math.cos(m.yaw) * 1.1 }
  }
  const inW = blockAt(Math.floor(m.x), Math.floor(m.y + 0.4), Math.floor(m.z)) === WATER
  const k = Math.min(1, (m.onGround ? 10 : 2) * dt)
  m.vx += (wantX - m.vx) * k
  m.vz += (wantZ - m.vz) * k
  m.vy = inW ? Math.min(m.vy + 12 * dt, 2.5) : Math.max(m.vy - 27 * dt, -48)
  const p = { x: m.x, y: m.y, z: m.z }
  const blockedX = moveAxis(p, 'x', m.vx * dt, MOB_HW, MOB_H)
  const blockedZ = moveAxis(p, 'z', m.vz * dt, MOB_HW, MOB_H)
  const hitY = moveAxis(p, 'y', m.vy * dt, MOB_HW, MOB_H)
  m.onGround = hitY && m.vy < 0
  if (hitY) m.vy = 0
  if ((blockedX || blockedZ) && m.onGround && (wantX || wantZ)) m.vy = 8.4
  m.x = p.x; m.y = p.y; m.z = p.z
  // Attack.
  m.attackT -= dt
  const dy = g.pos.y - m.y
  if (!g.dead && dist < 1.25 && Math.abs(dy) < 1.6 && m.attackT <= 0) {
    m.attackT = 1
    hurt(MOB_DAMAGE, m)
  }
  m.groanT -= dt
  if (m.groanT <= 0 && dist < 24) {
    m.groanT = 4 + Math.random() * 6
    ctx.playTone({ freq: 95 + Math.random() * 30, to: 70, duration: 0.5, type: 'sawtooth', volume: 0.06 * (1 - dist / 24) })
  }
  // Pose: walk cycle on the legs, arms bob forward, a red flash when hurt.
  const moving = Math.hypot(m.vx, m.vz)
  m.walk += moving * dt * 2.4
  const swing = Math.sin(m.walk) * Math.min(1, moving / 1.5) * 40
  root.visible = true
  root.position3 = { x: m.x, y: m.y, z: m.z }
  root.rotation3 = { x: 0, y: (m.yaw * 180) / Math.PI, z: 0 }
  ref('mob' + i + 'legL').rotation3 = { x: swing, y: 0, z: 0 }
  ref('mob' + i + 'legR').rotation3 = { x: -swing, y: 0, z: 0 }
  ref('mob' + i + 'armL').rotation3 = { x: 90 + swing * 0.25, y: 0, z: 0 }
  ref('mob' + i + 'armR').rotation3 = { x: 90 - swing * 0.25, y: 0, z: 0 }
  m.hurtT -= dt
  const flash = m.hurtT > 0
  if (m.flashing !== flash) {
    m.flashing = flash
    const tint = (c) => (flash ? [1, 0.25, 0.25, 1] : c)
    ref('mob' + i + 'head').fillColor = tint([0.33, 0.58, 0.33, 1])
    ref('mob' + i + 'body').fillColor = tint([0.13, 0.52, 0.58, 1])
    for (const limb of ['armL', 'armR']) ref('mob' + i + limb + 'Mesh').fillColor = tint([0.33, 0.58, 0.33, 1])
    for (const limb of ['legL', 'legR']) ref('mob' + i + limb + 'Mesh').fillColor = tint([0.24, 0.24, 0.55, 1])
  }
}

// ----------------------------------------------------------- camera --
const cam = ref('cam3d')
cam.position3 = camEye
cam.rotation3 = { x: g.pitch, y: g.yaw, z: g.hurtT > 0 ? Math.sin(g.hurtT * 40) * 2 : 0 }
ref('sel').position3 = hit ? { x: hit.x, y: hit.y, z: hit.z } : { x: 0, y: -100, z: 0 }
// The held block: swaps with the slot, bobs with your stride, swings on use.
const held = ref('held')
if (!g.heldBase) g.heldBase = held.position3
const heldId = g.hotbar[g.slot]
if (g.heldId !== heldId) { g.heldId = heldId; held.setVoxels([heldId]) }
g.swing = Math.max(0, g.swing - dt * 5)
const sw = Math.sin(g.swing * Math.PI)
const bobY = Math.abs(Math.sin(g.bob * 1.4)) * 0.03, bobX = Math.sin(g.bob * 0.7) * 0.02
held.position3 = { x: g.heldBase.x + bobX - sw * 0.06, y: g.heldBase.y - bobY - sw * 0.1, z: g.heldBase.z - sw * 0.12 }
// Under water: a dense blue fog; otherwise the sky fog hides the edge of
// the loaded world.
if (g.under !== headUnder) {
  g.under = headUnder
  ctx.setEnvironment3(headUnder ? { fog: [0.08, 0.22, 0.45], fogDensity: 0.12, ambient: [0.3, 0.4, 0.6] } : { fog: SKY, fogDensity: 0.0115, ambient: [0.42, 0.47, 0.56] })
}

// -------------------------------------------------------------- HUD --
function showToast(text) { g.toast = text; g.toastT = 1.6 }
g.toastT -= dt
g.fps += (1 / Math.max(ctx.deltaSeconds || 1 / 60, 1e-3) - g.fps) * 0.05
const hud = ref('hud').dom
if (hud) {
  const $ = (id) => hud.getElementById(id)
  if (!g.icons) g.icons = buildIcons()
  if (g.hudDom !== hud) {
    g.hudDom = hud
    g.hudKey = ''
    g.heartsKey = ''
    $('bar').innerHTML = Array.from({ length: 9 }, (_, k) => `<div class="slot"><img alt=""><span class="n">${k + 1}</span></div>`).join('')
    $('hearts').innerHTML = '<i class="h"></i>'.repeat(MAX_HP / 2)
    const style = document.createElement('style')
    style.textContent = '.water{position:absolute;inset:0;background:rgb(30 70 160 / 0.35);display:none}.water.on{display:block}'
    hud.appendChild(style)
    const root = hud.querySelector('.hud')
    const water = document.createElement('div')
    water.className = 'water'; water.id = 'water'
    root.prepend(water)
    const hurtEl = document.createElement('div')
    hurtEl.className = 'hurt'; hurtEl.id = 'hurt'
    root.prepend(hurtEl)
  }
  const key = g.slot + '|' + g.hotbar.join(',')
  if (g.hudKey !== key) {
    if (g.hudKey && g.hudKey.split('|')[0] !== String(g.slot)) showToast(BLOCKS[g.hotbar[g.slot]].name.toUpperCase())
    g.hudKey = key
    ;[...$('bar').children].forEach((el, k) => {
      el.classList.toggle('on', k === g.slot)
      const img = el.firstElementChild
      if (img.dataset.id !== String(g.hotbar[k])) { img.dataset.id = g.hotbar[k]; img.src = g.icons[g.hotbar[k]] }
    })
  }
  const heartsKey = String(g.hp)
  if (g.heartsKey !== heartsKey) {
    const lost = g.heartsKey !== '' && Number(g.heartsKey) > g.hp
    g.heartsKey = heartsKey
    ;[...$('hearts').children].forEach((el, k) => { el.className = 'h' + (g.hp >= (k + 1) * 2 ? ' full' : g.hp === k * 2 + 1 ? ' half' : '') })
    const hearts = $('hearts')
    hearts.classList.remove('blink')
    if (lost) { void hearts.offsetWidth; hearts.classList.add('blink') }
  }
  $('hearts').style.display = g.fly ? 'none' : ''
  $('hurt').classList.toggle('on', g.hurtT > 0)
  $('water').classList.toggle('on', headUnder)
  $('dead').classList.toggle('on', g.dead)
  const toast = $('toast')
  if (toast.textContent !== g.toast) toast.textContent = g.toast
  toast.classList.toggle('off', g.toastT <= 0)
  const hint = g.inv || g.dead ? '' : !ready ? 'GENERATING WORLD…' : ctx.pointerLocked ? '' : 'CLICK TO PLAY\nWASD move  ·  SPACE jump  ·  CTRL sprint  ·  SHIFT sneak  ·  F fly\nLEFT CLICK dig / punch  ·  RIGHT CLICK build  ·  1–9 / WHEEL pick\nE inventory  ·  P peaceful  ·  N new world'
  if ($('hint').textContent !== hint) $('hint').textContent = hint
  g.debugT -= dt
  if (g.debugT <= 0) {
    g.debugT = 0.25
    const facing = ['N', 'W', 'S', 'E'][Math.round((((g.yaw % 360) + 360) % 360) / 90) % 4]
    const zombies = g.mobs.filter((m) => m.on).length
    $('debug').textContent = `VOXEL CRAFT  ·  ${Math.round(g.fps)} FPS\nXYZ  ${g.pos.x.toFixed(1)} / ${g.pos.y.toFixed(1)} / ${g.pos.z.toFixed(1)}  ·  FACING ${facing}\n${hit ? 'LOOKING AT  ' + BLOCKS[hit.block].name.toUpperCase() : 'LOOKING AT  -'}\nCHUNKS ${g.chunks.size}  ·  ${g.peaceful ? 'PEACEFUL' : 'ZOMBIES ' + zombies}  ·  KILLS ${g.kills}\n${g.fly ? 'FLYING' : inWater ? 'SWIMMING' : 'WALKING'}  ·  SEED ${g.seed}`
  }
}
// The creative inventory: a grid of every block; click one to put it in
// the selected slot.
const invNode = ref('inventory')
if (invNode.visible !== g.inv) invNode.visible = g.inv
const inv = invNode.dom
if (inv && g.icons && g.invDom !== inv) {
  g.invDom = inv
  inv.getElementById('grid').innerHTML = BLOCKS.map((b, id) => (id === AIR ? '' : `<div class="cell" data-id="${id}"><img src="${g.icons[id]}" alt=""><span class="tip">${b.name}</span></div>`)).join('')
  inv.addEventListener('click', (e) => {
    const cell = e.target.closest('.cell')
    if (!cell) return
    g.hotbar[g.slot] = Number(cell.dataset.id)
    g.dirty = true
    ctx.playTone({ freq: 600, to: 800, duration: 0.05, type: 'sine', volume: 0.1 })
  })
}

// ------------------------------------------------------------- save --
// Edits (not the world: it regenerates from the seed), where you stand,
// health and hotbar, every few seconds when something changed.
g.saveT -= dt
if (g.saveT <= 0) {
  g.saveT = 3
  if (g.dirty || Math.random() < 0.2) {
    g.dirty = false
    ctx.storage.set('voxelcraft2', { seed: g.seed, edits: g.edits, player: g.dead ? null : { x: g.pos.x, y: g.pos.y, z: g.pos.z }, yaw: g.yaw, pitch: g.pitch, fly: g.fly, hotbar: g.hotbar, hp: g.dead ? MAX_HP : g.hp, peaceful: g.peaceful, kills: g.kills })
  }
}
for (const k of ['mouse', 'mouseleft', 'mouseright', 'e', 'f', 'n', 'p', 'escape', '1', '2', '3', '4', '5', '6', '7', '8', '9']) g.prev[k] = keys.has(k)
