// Voxel Craft — a Minetest / Minecraft-style block sandbox (see
// `demo_voxel.rs`). This script owns the world: it generates the blocks
// from a seed, keeps the authoritative copy (`g.blocks`) for collision and
// aiming, and mirrors every change to the engine's block world
// (`ref.setVoxels` once, `ref.setVoxel` per edit), which meshes and draws it.
//
// Prefix constants (from Rust): W, H, D (world size), DEBRIS (pool size),
// ATLAS_* + TILE (the block texture atlas, base64 RGBA, for HUD icons) and
// BLOCKS (every block type by id: name + top / side / bottom tiles).
const ref = (id) => getCompByID(id)
const keys = ctx.keysDown
const dt = Math.min(ctx.deltaSeconds || 1 / 60, 1 / 20)
const AIR = 0
const ID = {}
BLOCKS.forEach((b, i) => { ID[b.name] = i })
const WATER = ID.water, BEDROCK = ID.bedrock
/** The sea level: everything below it that isn't ground is water. */
const SEA = 24
/** Player box: half width, height, eye height. */
const HW = 0.3, PH = 1.75, EYE = 1.62
const REACH = 6
const SKY = [0.55, 0.74, 0.96]
const idx = (x, y, z) => x + W * (z + D * y)
const clampv = (v, lo, hi) => Math.max(lo, Math.min(hi, v))

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
function noise3(x, y, z, s) {
  const xi = Math.floor(x), yi = Math.floor(y), zi = Math.floor(z)
  const xf = x - xi, yf = y - yi, zf = z - zi
  const u = xf * xf * (3 - 2 * xf), v = yf * yf * (3 - 2 * yf), w = zf * zf * (3 - 2 * zf)
  const l = (a, b, t) => a + (b - a) * t
  const at = (dx, dy, dz) => hash3(xi + dx, yi + dy, zi + dz, s)
  return l(
    l(l(at(0, 0, 0), at(1, 0, 0), u), l(at(0, 1, 0), at(1, 1, 0), u), v),
    l(l(at(0, 0, 1), at(1, 0, 1), u), l(at(0, 1, 1), at(1, 1, 1), u), v),
    w,
  )
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
// Heights from layered noise (base terrain + small hills + rare
// mountains), then strata (bedrock, stone, dirt, grass / sand / snow), the
// sea, caves (where two 3D noises are both near their middle: long
// winding tunnels), ore clusters and trees.
function generate(seed) {
  const b = new Uint8Array(W * H * D)
  const heights = new Int16Array(W * D)
  for (let z = 0; z < D; z++) {
    for (let x = 0; x < W; x++) {
      const base = fbm2(x / 56, z / 56, seed)
      const hills = noise2(x / 18, z / 18, seed + 11)
      const m = Math.max(0, noise2(x / 70, z / 70, seed + 23) - 0.45) / 0.55
      // (`landSeed` checks the same formula, minus the mountains.)
      const h = clampv(Math.floor(21 + (base - 0.5) * 46 + hills * 4 + m * m * 30), 4, H - 8)
      heights[x + W * z] = h
      const beach = h <= SEA + 1
      const snowy = h >= 45 + Math.floor(hash3(x, 0, z, seed + 5) * 3)
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
        b[idx(x, y, z)] = id
      }
      for (let y = h + 1; y <= SEA; y++) b[idx(x, y, z)] = WATER
    }
  }
  // Caves: allowed to break the surface on dry land (cave mouths), kept
  // sealed under the sea.
  for (let z = 0; z < D; z++) {
    for (let x = 0; x < W; x++) {
      const h = heights[x + W * z]
      const top = h > SEA + 2 ? h : h - 4
      for (let y = 2; y <= top; y++) {
        const a = noise3(x / 15, y / 9, z / 15, seed + 41)
        if (Math.abs(a - 0.5) > 0.06) continue
        const c = noise3(x / 15, y / 9, z / 15, seed + 53)
        if (Math.abs(c - 0.5) < 0.09) b[idx(x, y, z)] = AIR
      }
    }
  }
  // Ores: 2×2×2 clusters picked by a coarse hash, partly filled; rarer and
  // deeper the more valuable.
  for (let y = 1; y < H; y++) {
    for (let z = 0; z < D; z++) {
      for (let x = 0; x < W; x++) {
        const i = idx(x, y, z)
        if (b[i] !== ID.stone) continue
        const r = hash3(x >> 1, y >> 1, z >> 1, seed + 61)
        if (hash3(x, y, z, seed + 62) > 0.6) continue
        if (r < 0.03) b[i] = ID['coal ore']
        else if (y < 34 && r < 0.042) b[i] = ID['iron ore']
        else if (y < 20 && r < 0.047) b[i] = ID['gold ore']
        else if (y < 13 && r < 0.05) b[i] = ID['diamond ore']
      }
    }
  }
  // Trees on grass, denser in forest patches.
  for (let z = 3; z < D - 3; z++) {
    for (let x = 3; x < W - 3; x++) {
      const h = heights[x + W * z]
      if (b[idx(x, h, z)] !== ID.grass || b[idx(x, h + 1, z)] !== AIR) continue
      const forest = noise2(x / 26, z / 26, seed + 71)
      if (hash3(x, 0, z, seed + 72) > (forest > 0.55 ? 0.05 : 0.006)) continue
      const trunk = 4 + Math.floor(hash3(x, 1, z, seed + 73) * 3)
      if (h + trunk + 2 >= H) continue
      const top = h + trunk
      for (let ly = top - 2; ly <= top + 1; ly++) {
        const r = ly >= top ? 1 : 2
        for (let dz = -r; dz <= r; dz++) {
          for (let dx = -r; dx <= r; dx++) {
            if (Math.abs(dx) === r && Math.abs(dz) === r && (ly >= top || hash3(x + dx, ly, z + dz, seed + 74) < 0.5)) continue
            const i = idx(x + dx, ly, z + dz)
            if (b[i] === AIR) b[i] = ID.leaves
          }
        }
      }
      for (let y = h + 1; y < top; y++) b[idx(x, y, z)] = ID.log
      b[idx(x, h, z)] = ID.dirt
    }
  }
  return { blocks: b, heights }
}

/** Where to stand: the dry, open column nearest the middle of the world. */
function spawnPoint(world) {
  for (let r = 0; r < W / 2; r++) {
    for (let k = 0; k < 8 * Math.max(1, r); k++) {
      const a = (k / (8 * Math.max(1, r))) * Math.PI * 2
      const x = Math.floor(W / 2 + Math.cos(a) * r), z = Math.floor(D / 2 + Math.sin(a) * r)
      const h = world.heights[x + W * z]
      if (h > SEA && world.blocks[idx(x, h + 1, z)] === AIR && world.blocks[idx(x, h + 2, z)] === AIR) return { x: x + 0.5, y: h + 1, z: z + 0.5, land: true }
    }
  }
  return { x: W / 2, y: H - 2, z: D / 2, land: false }
}

/** A random seed whose world has dry land near the middle (some seeds
 * are all ocean). */
function landSeed() {
  for (let tries = 0; ; tries++) {
    const seed = Math.floor(Math.random() * 1e6)
    // A cheap check of the height noise alone, without generating the world.
    let dry = 0
    for (let z = D / 4; z < (3 * D) / 4; z += 4) {
      for (let x = W / 4; x < (3 * W) / 4; x += 4) {
        const h = Math.floor(21 + (fbm2(x / 56, z / 56, seed) - 0.5) * 46 + noise2(x / 18, z / 18, seed + 11) * 4)
        if (h > SEA + 1) dry++
      }
    }
    if (dry > 60 || tries > 30) return seed
  }
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
function setupWorld(seed, edits) {
  const world = generate(seed)
  for (const [i, id] of Object.entries(edits)) world.blocks[Number(i)] = id
  ref('world').setVoxels(world.blocks)
  return world
}
let g = ctx.globals.vc
if (!g) {
  const saved = ctx.storage.get('voxelcraft') || {}
  const seed = saved.seed ?? landSeed()
  const edits = saved.edits || {}
  const world = setupWorld(seed, edits)
  const spawn = saved.player || spawnPoint(world)
  g = ctx.globals.vc = {
    seed, edits, blocks: world.blocks, heights: world.heights,
    pos: { x: spawn.x, y: spawn.y, z: spawn.z }, vel: { x: 0, y: 0, z: 0 },
    yaw: saved.yaw ?? 0, pitch: saved.pitch ?? -10, onGround: false, fly: !!saved.fly,
    slot: 0, hotbar: saved.hotbar || ['grass', 'dirt', 'stone', 'cobblestone', 'planks', 'log', 'leaves', 'glass', 'bricks'].map((n) => ID[n]),
    breakT: 0, placeT: 0, inv: false, dirty: false, saveT: 0, toastT: 0, toast: '',
    swing: 0, bob: 0, stepT: 0, heldId: -1, heldBase: null, under: null, prev: {}, fps: 60, debugT: 0,
    debris: Array.from({ length: DEBRIS }, () => ({ life: 0, x: 0, y: 0, z: 0, vx: 0, vy: 0, vz: 0 })), nextDebris: 0,
    icons: null, hudDom: null, invDom: null, target: null,
  }
}
const pressed = (k) => keys.has(k) && !g.prev[k]

// -------------------------------------------------------- the world --
function blockAt(x, y, z) {
  if (x < 0 || y < 0 || z < 0 || x >= W || y >= H || z >= D) return AIR
  return g.blocks[idx(x, y, z)]
}
/** Blocks the player: anything but air and water; the world's sides and
 * floor are walls. */
function solid(x, y, z) {
  if (y < 0) return true
  if (y >= H) return false
  if (x < 0 || z < 0 || x >= W || z >= D) return true
  const b = g.blocks[idx(x, y, z)]
  return b !== AIR && b !== WATER
}
function setBlock(x, y, z, id) {
  if (x < 0 || y < 0 || z < 0 || x >= W || y >= H || z >= D) return
  const i = idx(x, y, z)
  g.blocks[i] = id
  g.edits[i] = id
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

// ------------------------------------------------------- the player --
function collides(p) {
  const x0 = Math.floor(p.x - HW), x1 = Math.floor(p.x + HW)
  const y0 = Math.floor(p.y), y1 = Math.floor(p.y + PH)
  const z0 = Math.floor(p.z - HW), z1 = Math.floor(p.z + HW)
  for (let y = y0; y <= y1; y++) for (let z = z0; z <= z1; z++) for (let x = x0; x <= x1; x++) if (solid(x, y, z)) return true
  return false
}
/** Moves along one axis in small steps; on hitting a block, snaps flush
 * against it. Returns whether it hit. */
function moveAxis(axis, d) {
  const steps = Math.max(1, Math.ceil(Math.abs(d) / 0.3))
  for (let i = 0; i < steps; i++) {
    const before = g.pos[axis]
    g.pos[axis] += d / steps
    if (!collides(g.pos)) continue
    const p = g.pos
    if (axis === 'y') p.y = d < 0 ? Math.floor(p.y) + 1 : Math.floor(p.y + PH) - PH - 0.001
    else if (d > 0) p[axis] = Math.floor(p[axis] + HW) - HW - 0.001
    else p[axis] = Math.floor(p[axis] - HW) + 1 + HW + 0.001
    if (collides(p)) p[axis] = before
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
    if (b !== AIR && b !== WATER) return { x, y, z, nx, ny, nz, block: b }
    if (tX < tY && tX < tZ) { x += sx; t = tX; tX += dX; nx = -sx; ny = 0; nz = 0 }
    else if (tY < tZ) { y += sy; t = tY; tY += dY; nx = 0; ny = -sy; nz = 0 }
    else { z += sz; t = tZ; tZ += dZ; nx = 0; ny = 0; nz = -sz }
  }
  return null
}

// ------------------------------------------------------------ input --
// The pointer lock comes with the next click on the canvas (browsers need
// a real gesture); a click that only grabs the mouse doesn't dig.
if (!g.inv && !ctx.pointerLocked && ctx.requestPointerLock) ctx.requestPointerLock()
const playing = ctx.pointerLocked && !g.inv
if (pressed('e')) {
  g.inv = !g.inv
  if (g.inv && ctx.exitPointerLock) ctx.exitPointerLock()
}
if (pressed('escape') && g.inv) g.inv = false
if (pressed('f')) { g.fly = !g.fly; g.vel.y = 0; showToast(g.fly ? 'FLY MODE' : 'WALK MODE') }
if (pressed('n')) {
  // A fresh world (new seed, no edits).
  g.seed = landSeed()
  g.edits = {}
  const world = setupWorld(g.seed, g.edits)
  g.blocks = world.blocks; g.heights = world.heights
  const s = spawnPoint(world)
  g.pos = { x: s.x, y: s.y, z: s.z }; g.vel = { x: 0, y: 0, z: 0 }
  g.dirty = true
  showToast('NEW WORLD  ·  SEED ' + g.seed)
}
for (let k = 1; k <= 9; k++) if (pressed(String(k))) g.slot = k - 1
if (ctx.wheelY && !g.inv) g.slot = (g.slot + (ctx.wheelY > 0 ? 1 : -1) + 9) % 9
if (ctx.pointerLocked) {
  g.yaw -= (ctx.mouseDX || 0) * 0.14
  g.pitch = clampv(g.pitch - (ctx.mouseDY || 0) * 0.14, -89.5, 89.5)
}
const yawR = (g.yaw * Math.PI) / 180, pitchR = (g.pitch * Math.PI) / 180
const look = { x: -Math.sin(yawR) * Math.cos(pitchR), y: Math.sin(pitchR), z: -Math.cos(yawR) * Math.cos(pitchR) }

// -------------------------------------------------------- movement --
const feet = blockAt(Math.floor(g.pos.x), Math.floor(g.pos.y + 0.4), Math.floor(g.pos.z))
const inWater = feet === WATER
const eye = { x: g.pos.x, y: g.pos.y + EYE, z: g.pos.z }
const headUnder = blockAt(Math.floor(eye.x), Math.floor(eye.y + 0.05), Math.floor(eye.z)) === WATER
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
moveAxis('x', g.vel.x * dt) && (g.vel.x = 0)
moveAxis('z', g.vel.z * dt) && (g.vel.z = 0)
if (sneak && wasOnGround && !g.fly) {
  const probe = { x: g.pos.x, y: g.pos.y - 0.6, z: g.pos.z }
  if (!collides(probe)) { g.pos.x = sx0; g.pos.z = sz0 }
}
const hitY = moveAxis('y', g.vel.y * dt)
g.onGround = hitY && g.vel.y < 0
if (hitY) {
  if (g.vel.y < -16) ctx.playTone({ freq: 120, to: 50, duration: 0.12, type: 'noise', volume: 0.18 })
  g.vel.y = 0
}
// Fell out somehow: back to the surface.
if (g.pos.y < -10) { const s = spawnPoint(g); g.pos = { x: s.x, y: s.y, z: s.z }; g.vel.y = 0 }

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

// --------------------------------------------------- dig and build --
const camEye = { x: g.pos.x, y: g.pos.y + EYE, z: g.pos.z }
const hit = raycast(camEye, look, REACH)
g.target = hit
g.breakT -= dt; g.placeT -= dt
// A press only digs / builds if it started while playing (the click that
// grabs the mouse, or closes the inventory, doesn't count).
if (pressed('mouseleft')) g.leftOk = playing
if (pressed('mouseright')) g.rightOk = playing
if (playing && g.leftOk && hit && keys.has('mouseleft') && g.breakT <= 0) {
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
  const overlapsPlayer = id !== WATER &&
    px + 1 > g.pos.x - HW && px < g.pos.x + HW && py + 1 > g.pos.y && py < g.pos.y + PH && pz + 1 > g.pos.z - HW && pz < g.pos.z + HW
  if ((there === AIR || there === WATER) && py >= 0 && py < H && !overlapsPlayer) {
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

// ----------------------------------------------------------- camera --
const cam = ref('cam3d')
cam.position3 = camEye
cam.rotation3 = { x: g.pitch, y: g.yaw, z: 0 }
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
// Under water: a dense blue fog; otherwise the sky fog hides the world's edge.
if (g.under !== headUnder) {
  g.under = headUnder
  ctx.setEnvironment3(headUnder ? { fog: [0.08, 0.22, 0.45], fogDensity: 0.12, ambient: [0.3, 0.4, 0.6] } : { fog: SKY, fogDensity: 0.0072, ambient: [0.42, 0.47, 0.56] })
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
    $('bar').innerHTML = Array.from({ length: 9 }, (_, k) => `<div class="slot"><img alt=""><span class="n">${k + 1}</span></div>`).join('')
    const style = document.createElement('style')
    style.textContent = '.water{position:absolute;inset:0;background:rgb(30 70 160 / 0.35);display:none}.water.on{display:block}'
    hud.appendChild(style)
    const water = document.createElement('div')
    water.className = 'water'; water.id = 'water'
    hud.querySelector('.hud').prepend(water)
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
  $('water').classList.toggle('on', headUnder)
  const toast = $('toast')
  if (toast.textContent !== g.toast) toast.textContent = g.toast
  toast.classList.toggle('off', g.toastT <= 0)
  const hint = g.inv ? '' : ctx.pointerLocked ? '' : 'CLICK TO PLAY\nWASD move  ·  SPACE jump  ·  CTRL sprint  ·  SHIFT sneak  ·  F fly\nLEFT CLICK dig  ·  RIGHT CLICK build  ·  1–9 / WHEEL pick  ·  E inventory  ·  N new world'
  if ($('hint').textContent !== hint) $('hint').textContent = hint
  g.debugT -= dt
  if (g.debugT <= 0) {
    g.debugT = 0.25
    const facing = ['N', 'W', 'S', 'E'][Math.round((((g.yaw % 360) + 360) % 360) / 90) % 4]
    $('debug').textContent = `VOXEL CRAFT  ·  ${Math.round(g.fps)} FPS\nXYZ  ${g.pos.x.toFixed(1)} / ${g.pos.y.toFixed(1)} / ${g.pos.z.toFixed(1)}  ·  FACING ${facing}\n${hit ? 'LOOKING AT  ' + BLOCKS[hit.block].name.toUpperCase() : 'LOOKING AT  -'}\n${g.fly ? 'FLYING' : inWater ? 'SWIMMING' : 'WALKING'}  ·  SEED ${g.seed}`
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
// Edits (not the whole world: it regenerates from the seed), where you
// stand and your hotbar, every few seconds when something changed.
g.saveT -= dt
if (g.saveT <= 0) {
  g.saveT = 3
  if (g.dirty || Math.random() < 0.2) {
    g.dirty = false
    ctx.storage.set('voxelcraft', { seed: g.seed, edits: g.edits, player: { x: g.pos.x, y: g.pos.y, z: g.pos.z }, yaw: g.yaw, pitch: g.pitch, fly: g.fly, hotbar: g.hotbar })
  }
}
for (const k of ['mouseleft', 'mouseright', 'e', 'f', 'n', 'escape', '1', '2', '3', '4', '5', '6', '7', '8', '9']) g.prev[k] = keys.has(k)
