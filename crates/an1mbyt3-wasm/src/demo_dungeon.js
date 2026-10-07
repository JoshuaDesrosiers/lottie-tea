// ===================================================================
//  DUNGEON ROYALE — drag cards into the dungeon, defend the keep.
//  (MAP_W/MAP_H/TILE/MAP_TOP/MAP_LEFT/POOL are prepended by Rust.)
//
//  Sections:  data tables · dungeon generation · A* pathfinding ·
//             units & bots · projectiles/effects · cards & elixir ·
//             waves · frame loop
// ===================================================================

// Three scenes (home, game, game over) share `ctx.globals`; the game keeps
// its own state under `.dungeon`, and the run's results under `.royale`.
const g = ctx.globals.dungeon || (ctx.globals.dungeon = {})
// Player progress — card levels, banked gold, trophies, best wave — lives
// in `ctx.storage` (localStorage), so it survives restarts and reloads of
// an exported game. `ctx.globals.royale` is this session's working copy.
function loadRoyale() {
  const saved = ctx.storage.get('royale')
  return Object.assign(
    { wave: 0, gold: 0, best: 0, bank: 0, trophies: 0, wins: 0, losses: 0, levels: {}, mode: 'infinite', result: '' },
    saved && typeof saved === 'object' ? saved : {},
  )
}
function saveRoyale() { ctx.storage.set('royale', royale) }
const royale = ctx.globals.royale || (ctx.globals.royale = loadRoyale())
// Two modes (picked on the home screen):
//  - 'royale'   — Clash Royale rules: 2 minutes to destroy the enemy
//                 castle; 2x elixir in the last minute, 3x in the last 30s.
//  - 'infinite' — endless monster waves; survive as long as you can.
const ROYALE = royale.mode === 'royale'
const dt = Math.min(ctx.deltaSeconds, 1 / 20)
const keys = ctx.keysDown
// A wall clock shared by every layer's program — `T` restarts at 0
// whenever *that layer* changes state, so animation phases and path ages
// use this instead.
const T = performance.now() / 1000

// ------------------------------------------------------------ data --

// Sprite rows in units.png (4 idle frames each).
const ROW = { knight: 0, captain: 1, squire: 2, archer: 3, cleric: 4, rogue: 5, skeleton: 6, reaper: 7, wisp: 8, necro: 9 }

// Every unit type. Allies are deployed from cards; enemies pour out of
// portals. `sight`: how far a bot notices targets. `ranged`: projectile
// kind. Special behaviors are flags read by the bot brain below.
// Health tiers shared by BOTH sides, so a basic monster dies about as fast
// as a basic troop: light ~3 hits, medium ~5, heavy ~8 from a knight.
const HP = { light: 120, medium: 240, heavy: 420 }

const KINDS = {
  knight:   { team: 'ally', row: ROW.knight, hp: HP.heavy, dmg: 55, cd: 1.0, range: 12, speed: 36, sight: 90 },
  captain:  { team: 'ally', row: ROW.captain, hp: HP.heavy, dmg: 60, cd: 1.0, range: 12, speed: 32, sight: 90, light: [1, 0.85, 0.5], leader: true },
  squire:   { team: 'ally', row: ROW.squire, hp: HP.light, dmg: 26, cd: 0.8, range: 12, speed: 44, sight: 70, scale: 0.8, follower: true },
  archer:   { team: 'ally', row: ROW.archer, hp: HP.light, dmg: 30, cd: 0.9, range: 70, speed: 38, sight: 96, ranged: 'arrow' },
  cleric:   { team: 'ally', row: ROW.cleric, hp: HP.medium, dmg: 0, heal: 32, cd: 1.0, range: 46, speed: 38, sight: 110, light: [0.45, 1, 0.55], healer: true },
  rogue:    { team: 'ally', row: ROW.rogue, hp: HP.light, dmg: 40, cd: 0.55, range: 12, speed: 62, sight: 140, assassin: true },
  // Fodder: always exactly 2 arrows (no per-wave toughening).
  skeleton: { team: 'enemy', row: ROW.skeleton, hp: 58, dmg: 24, cd: 1.0, range: 12, speed: 32, sight: 52, gold: 1, noScale: true },
  reaper:   { team: 'enemy', row: ROW.reaper, hp: HP.medium, dmg: 60, cd: 1.5, range: 14, speed: 25, sight: 46, gold: 3 },
  wisp:     { team: 'enemy', row: ROW.wisp, hp: 60, dmg: 16, cd: 0.7, range: 10, speed: 72, sight: 0, flying: true, castleOnly: true, light: [0.4, 0.7, 1], gold: 1 },
  necro:    { team: 'enemy', row: ROW.necro, hp: HP.medium, dmg: 32, cd: 1.5, range: 66, speed: 26, sight: 92, ranged: 'hex', kite: 42, summoner: true, light: [0.75, 0.35, 1], gold: 4 },
  brute:    { team: 'enemy', row: ROW.skeleton, hp: 700, dmg: 100, cd: 1.8, range: 16, speed: 19, sight: 60, siege: true, scale: 1.6, tint: [0.72, 1, 0.68, 1], gold: 6 },
  king:     { team: 'enemy', row: ROW.reaper, hp: 1800, dmg: 160, cd: 2.0, range: 20, speed: 16, sight: 60, siege: true, scale: 2.2, tint: [1, 0.62, 0.62, 1], light: [1, 0.25, 0.25], gold: 25 },
}

// The deck, in cards.png order.
const CARDS = [
  { key: 'knight', cost: 3, type: 'troop', spawn: [['knight', 0, 0]] },
  { key: 'archers', cost: 2, type: 'troop', spawn: [['archer', -7, 0], ['archer', 7, 0]] },
  { key: 'captain', cost: 4, type: 'troop', spawn: [['captain', 0, 0], ['squire', -11, -5], ['squire', 11, -5], ['squire', 0, -11]] },
  { key: 'fireball', cost: 4, type: 'spell', radius: 34, dmg: 340 },
  { key: 'cleric', cost: 3, type: 'troop', spawn: [['cleric', 0, 0]] },
  { key: 'spikes', cost: 2, type: 'build', radius: 14 },
  { key: 'brazier', cost: 3, type: 'build', radius: 92 },
  { key: 'rogue', cost: 2, type: 'troop', spawn: [['rogue', 0, 0]] },
]

// Wave recipe: point cost + first wave it may appear.
const ENEMY_MENU = [
  { kind: 'skeleton', cost: 1, from: 1 },
  { kind: 'wisp', cost: 1, from: 2 },
  { kind: 'reaper', cost: 3, from: 3 },
  { kind: 'necro', cost: 4, from: 4 },
  { kind: 'brute', cost: 6, from: 5 },
]

// Tileset indices (10 columns). The pack's top-left 6x5 block is a room
// template: row 0 = top wall (corners at columns 0/5), rows 1-3 = side
// walls, row 4 = bottom wall. Walls use fixed pieces (no random mixing —
// that's what made the old walls look jumbled); floors are one tile with
// a few seamless cracked variants mixed in.
const T_FLOOR = 23, T_FLOOR_ALT = [22, 6, 16, 26, 17]
const T_FACE = 2, T_FACE_ALT = 3
const T_CAP = 42
const T_SIDE_L = 10, T_SIDE_R = 15
const T_CORNER_TL = 0, T_CORNER_TR = 5, T_CORNER_BL = 40, T_CORNER_BR = 45
const T_DECOR = [68, 77]

const DEPLOY_ROW = 11                 // troops/buildings: rows >= this
// Allies may hold at most this many unit slots, so the shared pool can
// never fill up with your army and starve the monster waves.
const ALLY_CAP = 22
const ELIXIR_MAX = 10

// Card upgrades (bought on the home screen with banked gold): every level
// adds 12% to a card's troops' health and damage, its spell damage, or
// its building's damage and health.
const MAX_LEVEL = 10
const cardLevel = (card) => Math.min(MAX_LEVEL, Math.max(1, (royale.levels && royale.levels[card.key]) || 1))
const cardPower = (card) => 1 + 0.12 * (cardLevel(card) - 1)

// Royale mode.
const MATCH_TIME = 120
const ENEMY_KEEP_HP = 4200
// The bot gets a little more elixir than you (it has no hand to juggle),
// and its troops scale with your trophies — the higher you climb, the
// tougher the arena.
const BOT_ELIXIR_RATE = 1.15
const enemyPower = () => 1.15 + Math.min(0.6, (royale.trophies || 0) / 1000)
// The enemy bot's "deck": what it can deploy and what it costs.
const ENEMY_CARDS = [
  { cost: 2, spawn: ['skeleton', 'skeleton', 'skeleton'] },
  { cost: 2, spawn: ['wisp', 'wisp'] },
  { cost: 3, spawn: ['reaper'] },
  { cost: 4, spawn: ['necro'] },
  { cost: 4, spawn: ['reaper', 'skeleton', 'skeleton'] },
  { cost: 6, spawn: ['brute'] },
  { cost: 5, spawn: ['necro', 'skeleton', 'skeleton'] },
  { cost: 8, spawn: ['king'], late: true },  // only once overtime starts
]
/** Royale elixir multiplier: 1x, then 2x in the last minute, 3x in the last 30s. */
const elixirMult = () => (g.matchT > 60 ? 1 : g.matchT > 30 ? 2 : 3)
const TIPS = [
  'DRAG A CARD ONTO YOUR HALF',
  'SPIKES SLOW MONSTERS DOWN',
  'ARCHERS + KNIGHT = SAFE LANE',
  'FIREBALL CLUMPED MONSTERS',
  'CLERICS HEAL HURT TROOPS',
  'WISPS FLY OVER THE WALLS',
  'ROGUES HUNT CASTERS FIRST',
]
const TRAY_TOP = -128
const CARD_Y = -204
const CARD_X = (i) => -72 + i * 46
const BAR_LEFT = 14 - 100, BAR_W = 200

// ----------------------------------------------------------- helpers --

const rand = (n) => Math.floor(Math.random() * n)
const pick = (arr) => arr[rand(arr.length)]
const clamp01 = (v) => Math.max(0, Math.min(1, v))
const dist = (ax, ay, bx, by) => Math.hypot(ax - bx, ay - by)
const ref = (id) => {
  let r = g.refs[id]
  if (r === undefined) { r = getCompByID(id); g.refs[id] = r }
  return r
}
const img = (name) => g.imgs[name]
const unitFrame = (row, f) => img('unit ' + (row * 4 + f))
const itemFrame = (row, f) => img('item ' + (row * 4 + f))

// Grid <-> world. Cell (cx, cy): cy = 0 is the top row.
const cellX = (x) => Math.floor((x - MAP_LEFT) / TILE)
const cellY = (y) => Math.floor((MAP_TOP - y) / TILE)
const centerX = (cx) => MAP_LEFT + cx * TILE + TILE / 2
const centerY = (cy) => MAP_TOP - cy * TILE - TILE / 2
const inMap = (cx, cy) => cx >= 0 && cy >= 0 && cx < MAP_W && cy < MAP_H
const walkable = (cx, cy) => inMap(cx, cy) && g.grid[cy * MAP_W + cx] === 0
const walkableAt = (x, y) => walkable(cellX(x), cellY(y))

// ------------------------------------------------- dungeon generation --
//
// Rooms-and-corridors: the keep's hall at the bottom, up to 7 random rooms
// above it, each joined to its nearest already-connected room by a 2-wide
// L-shaped corridor (a spanning tree, so everything is reachable), plus a
// couple of extra corridors that create loops — loops are what make A*
// interesting (several routes to the keep, bots pick the shortest).

function carve(x0, y0, w, h) {
  for (let y = y0; y < y0 + h; y++) for (let x = x0; x < x0 + w; x++) {
    if (x > 0 && y > 0 && x < MAP_W - 1 && y < MAP_H - 1) g.grid[y * MAP_W + x] = 0
  }
}

function corridor(a, b) {
  const ax = a.x + (a.w >> 1), ay = a.y + (a.h >> 1)
  const bx = b.x + (b.w >> 1), by = b.y + (b.h >> 1)
  if (Math.random() < 0.5) {
    carve(Math.min(ax, bx), ay, Math.abs(ax - bx) + 2, 2)
    carve(bx, Math.min(ay, by), 2, Math.abs(ay - by) + 2)
  } else {
    carve(ax, Math.min(ay, by), 2, Math.abs(ay - by) + 2)
    carve(Math.min(ax, bx), by, Math.abs(ax - bx) + 2, 2)
  }
}

function roomDist(a, b) {
  return Math.abs(a.x + a.w / 2 - b.x - b.w / 2) + Math.abs(a.y + a.h / 2 - b.y - b.h / 2)
}

/** BFS step-distance from the keep's door to every floor cell. */
function floodFrom(cx, cy) {
  const d = new Int32Array(MAP_W * MAP_H).fill(-1)
  const q = [cy * MAP_W + cx]
  d[q[0]] = 0
  for (let h = 0; h < q.length; h++) {
    const i = q[h], x = i % MAP_W, y = (i / MAP_W) | 0
    for (const [ox, oy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
      const nx = x + ox, ny = y + oy
      if (walkable(nx, ny) && d[ny * MAP_W + nx] < 0) { d[ny * MAP_W + nx] = d[i] + 1; q.push(ny * MAP_W + nx) }
    }
  }
  return d
}

function generateDungeon() {
  g.grid = new Array(MAP_W * MAP_H).fill(1)
  const hall = { x: 4, y: 17, w: 10, h: 6 }
  carve(hall.x, hall.y, hall.w, hall.h)
  const rooms = []
  for (let tries = 0; tries < 300 && rooms.length < 7; tries++) {
    const w = 3 + rand(4), h = 3 + rand(3)
    // Rows 0-1 stay solid wall under the top bar.
    const r = { x: 1 + rand(MAP_W - w - 2), y: 2 + rand(13 - h), w, h }
    const clash = rooms.some((o) => r.x < o.x + o.w + 1 && o.x < r.x + r.w + 1 && r.y < o.y + o.h + 1 && o.y < r.y + r.h + 1)
    if (!clash) { rooms.push(r); carve(r.x, r.y, r.w, r.h) }
  }
  // Spanning tree from the hall outward.
  const linked = [hall], rest = rooms.slice()
  while (rest.length) {
    let best = null
    for (const a of linked) for (const b of rest) {
      const d = roomDist(a, b)
      if (!best || d < best.d) best = { a, b, d }
    }
    corridor(best.a, best.b)
    linked.push(best.b)
    rest.splice(rest.indexOf(best.b), 1)
  }
  // Loops.
  for (let k = 0; k < 2 && rooms.length > 2; k++) corridor(pick(rooms), pick(linked))

  // The keep: a 4x3 block in the hall's lower middle that nobody walks through.
  g.keep = { x: 0, y: centerY(21) - 8, cx0: 7, cx1: 10, cy0: 19, cy1: 21 }
  for (let y = 19; y <= 21; y++) for (let x = 7; x <= 10; x++) g.grid[y * MAP_W + x] = 2
  g.keepGoals = [[7, 18], [8, 18], [9, 18], [10, 18], [6, 19], [6, 20], [11, 19], [11, 20]]

  // Portals: the rooms farthest (by walking distance) from the keep.
  const flood = floodFrom(8, 18)
  g.portals = rooms
    .map((r) => {
      const cx = r.x + (r.w >> 1), cy = r.y + (r.h >> 1)
      return { cx, cy, d: flood[cy * MAP_W + cx] }
    })
    .filter((p) => p.d > 0)
    .sort((a, b) => b.d - a.d)
    .slice(0, POOL.portals)
    .map((p) => ({ x: centerX(p.cx), y: centerY(p.cy), cx: p.cx, cy: p.cy }))
  // Degenerate layout (shouldn't happen): fall back to the top corridor.
  if (g.portals.length === 0) { carve(8, 1, 2, 17); g.portals = [{ x: centerX(8), y: centerY(2), cx: 8, cy: 2 }] }

  // Tiles: floors, then walls dressed by which side the floor is on.
  const tiles = new Array(MAP_W * MAP_H).fill(-1)
  const decor = new Array(MAP_W * MAP_H).fill(-1)
  const isFloor = (x, y) => inMap(x, y) && g.grid[y * MAP_W + x] !== 1
  const faces = []
  for (let y = 0; y < MAP_H; y++) for (let x = 0; x < MAP_W; x++) {
    const i = y * MAP_W + x
    if (isFloor(x, y)) {
      tiles[i] = Math.random() < 0.22 ? pick(T_FLOOR_ALT) : T_FLOOR
      if (g.grid[i] === 0 && y < 16 && Math.random() < 0.025) decor[i] = pick(T_DECOR)
      continue
    }
    const s = isFloor(x, y + 1), n = isFloor(x, y - 1), e = isFloor(x + 1, y), w = isFloor(x - 1, y)
    // A wall with floor below shows its brick face; others are edges seen
    // from above, picked by which side the floor is on (then diagonal-only
    // floor = an outer corner).
    if (s) { tiles[i] = (x * 7 + y * 3) % 5 === 0 ? T_FACE_ALT : T_FACE; faces.push([x, y]) }
    else if (n) tiles[i] = T_CAP
    else if (e) tiles[i] = T_SIDE_L
    else if (w) tiles[i] = T_SIDE_R
    else if (isFloor(x + 1, y + 1)) tiles[i] = T_CORNER_TL
    else if (isFloor(x - 1, y + 1)) tiles[i] = T_CORNER_TR
    else if (isFloor(x + 1, y - 1)) tiles[i] = T_CORNER_BL
    else if (isFloor(x - 1, y - 1)) tiles[i] = T_CORNER_BR
  }
  ref('map').setTiles(tiles)
  ref('decor').setTiles(decor)

  // Torches on wall faces, spread out.
  g.torches = []
  faces.sort(() => Math.random() - 0.5)
  for (const [x, y] of faces) {
    if (g.torches.length >= POOL.torches) break
    if (g.torches.some((t) => Math.abs(t.cx - x) + Math.abs(t.cy - y) < 4)) continue
    g.torches.push({ cx: x, cy: y, phase: Math.random() * 10 })
  }
  for (let i = 0; i < POOL.torches; i++) {
    const r = ref('torch' + i), t = g.torches[i]
    if (t) { r.x = centerX(t.cx); r.y = centerY(t.cy) - 6; r.visible = true } else { r.visible = false; r.x = 100000 }
  }
  for (let i = 0; i < POOL.portals; i++) {
    const r = ref('portal' + i), p = g.portals[i]
    r.visible = false
    if (p) { r.x = p.x; r.y = p.y }
  }
}

// ------------------------------------------------------ A* pathfinding --
//
// 8-way grid A*: octile heuristic, diagonal moves only when both
// orthogonal neighbors are open (no corner cutting), binary min-heap open
// set, and generation-stamped scratch arrays so a search allocates
// nothing. Returns cell indices from (excluding) start to goal, or null.

const N = MAP_W * MAP_H
if (!g.astar) {
  g.astar = { gen: 0, stamp: new Uint32Array(N), closed: new Uint32Array(N), gs: new Float32Array(N), from: new Int32Array(N), heapI: new Int32Array(N * 8), heapF: new Float32Array(N * 8) }
}
const DIRS = [[1, 0, 1], [-1, 0, 1], [0, 1, 1], [0, -1, 1], [1, 1, 1.4142], [1, -1, 1.4142], [-1, 1, 1.4142], [-1, -1, 1.4142]]

function aStar(sx, sy, gx, gy) {
  if (!inMap(sx, sy) || !inMap(gx, gy)) return null
  const A = g.astar
  const gen = ++A.gen
  const start = sy * MAP_W + sx, goal = gy * MAP_W + gx
  const h = (i) => {
    const dx = Math.abs((i % MAP_W) - gx), dy = Math.abs(((i / MAP_W) | 0) - gy)
    return Math.max(dx, dy) + 0.4142 * Math.min(dx, dy)
  }
  let size = 0
  const push = (i, f) => {
    let k = size++
    while (k > 0) {
      const p = (k - 1) >> 1
      if (A.heapF[p] <= f) break
      A.heapI[k] = A.heapI[p]; A.heapF[k] = A.heapF[p]; k = p
    }
    A.heapI[k] = i; A.heapF[k] = f
  }
  const pop = () => {
    const top = A.heapI[0]
    const li = A.heapI[--size], lf = A.heapF[size]
    let k = 0
    for (;;) {
      let c = 2 * k + 1
      if (c >= size) break
      if (c + 1 < size && A.heapF[c + 1] < A.heapF[c]) c++
      if (A.heapF[c] >= lf) break
      A.heapI[k] = A.heapI[c]; A.heapF[k] = A.heapF[c]; k = c
    }
    A.heapI[k] = li; A.heapF[k] = lf
    return top
  }
  A.stamp[start] = gen; A.gs[start] = 0; A.from[start] = -1
  push(start, h(start))
  while (size > 0) {
    const cur = pop()
    if (A.closed[cur] === gen) continue
    A.closed[cur] = gen
    if (cur === goal) {
      const path = []
      for (let i = goal; i !== start; i = A.from[i]) path.push(i)
      return path.reverse()
    }
    const x = cur % MAP_W, y = (cur / MAP_W) | 0
    for (const [ox, oy, cost] of DIRS) {
      const nx = x + ox, ny = y + oy
      if (!walkable(nx, ny)) continue
      if (ox && oy && (!walkable(x + ox, y) || !walkable(x, y + oy))) continue
      const ni = ny * MAP_W + nx
      if (A.closed[ni] === gen) continue
      // A little extra cost on cells crowded with bodies spreads traffic
      // over parallel routes instead of single-file conga lines.
      const gNew = A.gs[cur] + cost + (g.crowd[ni] || 0) * 0.35
      if (A.stamp[ni] !== gen || gNew < A.gs[ni]) {
        A.stamp[ni] = gen; A.gs[ni] = gNew; A.from[ni] = cur
        if (size < A.heapI.length) push(ni, gNew + h(ni))
      }
    }
  }
  return null
}

/** Is the straight segment walkable for a ~6px-wide body? (Lets bots cut
 *  corners smoothly instead of zig-zagging cell to cell.) */
function clearLine(ax, ay, bx, by) {
  const d = dist(ax, ay, bx, by)
  const steps = Math.ceil(d / 4)
  const nx = (by - ay) / (d || 1) * 3, ny = -(bx - ax) / (d || 1) * 3
  for (let s = 1; s <= steps; s++) {
    const t = s / steps, x = ax + (bx - ax) * t, y = ay + (by - ay) * t
    if (!walkableAt(x + nx, y + ny) || !walkableAt(x - nx, y - ny)) return false
  }
  return true
}

/** Nearest open cell to (cx, cy) — so goals inside walls/the keep resolve. */
function openNear(cx, cy) {
  if (walkable(cx, cy)) return [cx, cy]
  for (let r = 1; r < 6; r++) for (let oy = -r; oy <= r; oy++) for (let ox = -r; ox <= r; ox++) {
    if (Math.max(Math.abs(ox), Math.abs(oy)) === r && walkable(cx + ox, cy + oy)) return [cx + ox, cy + oy]
  }
  return null
}

// ------------------------------------------------------------- units --

function takeLight(color, radius, intensity) {
  for (const l of g.lights) if (!l.used) {
    l.used = true
    l.ref.fillColor = [color[0], color[1], color[2], 1]
    l.ref.lightRadius = radius
    l.ref.lightIntensity = intensity
    l.ref.visible = true
    return l
  }
  return null
}

// Monsters get 12% tougher every wave (and hit 6% harder, see act()).
const hpScale = (K) => (K.team === 'enemy' && !K.noScale ? 1 + 0.08 * Math.max(0, (g.wave || 1) - 1) : 1)
const dmgScale = (K) => (K.team === 'enemy' ? 1 + 0.06 * Math.max(0, (g.wave || 1) - 1) : 1)

const allyCount = () => g.units.reduce((n, u) => n + (u.alive && u.team === 'ally' ? 1 : 0), 0)

// Every unit is an instance of the "Unit" prefab (Resources > Prefabs): a
// sprite with an HP bar. Deploying spawns one (`ctx.spawn`), dying destroys
// it (`ref.destroy()`) — edit the prefab and every unit changes with it.
/** `power`: the deploying card's level multiplier (enemies: 1). */
function spawnUnit(kind, x, y, power = 1) {
  if (g.units.length >= POOL.units) return null
  const K = KINDS[kind]
  if (K.team === 'ally' && allyCount() >= ALLY_CAP) return null
  const ref = ctx.spawn('Unit', x, y, g.world)
  if (!ref) return null
  const u = { i: g.unitSerial++, ref, bar: ref.part('HP Back'), fill: ref.part('HP') }
  g.units.push(u)
  Object.assign(u, {
    alive: true, kind, K, team: K.team, hp: K.hp * hpScale(K) * power, maxHp: K.hp * hpScale(K) * power, power, x, y, vx: 0, vy: 0,
    target: null, path: null, pathKey: '', pathT: -9, cd: 0.4 + Math.random() * 0.3, face: 1,
    frame: -1, animT: Math.random(), lunge: 0, hitFlash: 0, landT: 0.25, summonT: 4 + Math.random() * 3,
    leader: null, slot: 0, fleeing: false, light: null, shownHp: -1, stuckT: 0, lastX: x, lastY: y,
  })
  const s = K.scale || 1
  u.ref.scaleX = s; u.ref.scaleY = s
  u.ref.fillColor = K.tint || [1, 1, 1, 1]
  u.ref.visible = true
  u.bar.visible = false; u.fill.visible = false
  u.fill.fillColor = u.team === 'ally' ? [0.45, 0.95, 0.5, 1] : [1, 0.38, 0.38, 1]
  if (K.light) u.light = takeLight(K.light, kind === 'king' ? 70 : 40, 1.0)
  return u
}

function killUnit(u) {
  u.alive = false
  u.ref.destroy()
  // A new array (not a splice): callers are often mid-loop over g.units.
  g.units = g.units.filter((o) => o !== u)
  if (u.light) { u.light.used = false; u.light.ref.visible = false; u.light = null }
  fx('Poof', u.x, u.y + 6, u.K.scale ? 26 : 14)
  if (u.team === 'enemy') {
    dropCoin(u.x, u.y + 8, u.K.gold || 1)
    flash(u.x, u.y + 6, [0.7, 0.4, 1], 0.9, 36)
  }
}

function damage(u, amount, from) {
  if (!u.alive) return
  u.hp -= amount
  u.hitFlash = 0.12
  if (u.isKeep) { fx('Hit', u.x + (Math.random() - 0.5) * 24, u.y + (Math.random() - 0.5) * 12, 5); return }
  fx('Hit', u.x, u.y + 6, 5)
  if (u.isBuilding) return  // stepBuildings retires it at 0 hp
  if (from && !u.target && u.team === 'ally') u.target = from   // retaliate
  if (u.hp <= 0) killUnit(u)
}

/** Everything a bot can target: units plus buildings plus the keep. */
function enemiesOf(team) {
  const out = []
  for (const u of g.units) if (u.alive && u.team !== team) out.push(u)
  if (team === 'enemy') {
    for (const b of g.buildings) if (b.alive && b.kind === 'brazier') out.push(b)
    out.push(g.keepUnit)
  } else if (g.enemyKeep && g.enemyKeep.alive) {
    out.push(g.enemyKeep)  // Royale: troops march on the enemy castle
  }
  return out
}

function nearest(list, x, y, maxD, filter) {
  let best = null, bd = maxD
  for (const t of list) {
    if (filter && !filter(t)) continue
    const d = dist(x, y, t.x, t.y)
    if (d < bd) { bd = d; best = t }
  }
  return best
}

const reach = (t) => (t.isKeep ? 30 : t.isBuilding ? 8 : 5)

/** The bot brain: decide what this unit wants (target + where to stand). */
function think(u) {
  const K = u.K
  const foes = enemiesOf(u.team)
  if (u.target && (!u.target.alive)) u.target = null

  if (u.team === 'enemy') {
    // Necromancers retreat to their portal to recover when badly hurt.
    if (K.summoner) {
      if (u.hp < u.maxHp * 0.3) u.fleeing = true
      if (u.fleeing && u.hp > u.maxHp * 0.75) u.fleeing = false
    }
    if (K.castleOnly) { u.target = g.keepUnit; return }
    if (K.siege) {
      u.target = nearest(foes, u.x, u.y, K.sight * 2, (t) => t.isBuilding || t.isKeep) || g.keepUnit
      return
    }
    // Leash: give up a chase that strays too far, go back to sieging.
    if (u.target && !u.target.isKeep && dist(u.x, u.y, u.target.x, u.target.y) > K.sight * 1.8) u.target = null
    const near = nearest(foes, u.x, u.y, K.sight, (t) => !t.isKeep)
    if (near && (!u.target || u.target.isKeep)) u.target = near
    if (!u.target) u.target = g.keepUnit
    return
  }

  // ---- allies ----
  if (K.healer) { u.target = null; return }  // handled in act()
  if (K.assassin) {
    // Rogues hunt the backline: casters/ranged first, anywhere in sight.
    const prey = nearest(foes, u.x, u.y, K.sight, (t) => t.K && (t.K.ranged || t.K.summoner))
    if (prey) { u.target = prey; return }
  }
  if (K.follower && u.leader && u.leader.alive) {
    // Squires fight what's near their captain, otherwise hold formation.
    const near = nearest(foes, u.leader.x, u.leader.y, K.sight, null)
    u.target = near || null
    return
  }
  if (!u.target || u.target.isKeep || dist(u.x, u.y, u.target.x, u.target.y) > K.sight * 1.5) {
    // Fight whatever is in sight; with nothing near, Royale troops march on
    // the enemy castle (Infinite: hunt the nearest monster anywhere).
    const near = nearest(foes, u.x, u.y, K.sight, (t) => !t.isKeep)
    const castle = g.enemyKeep && g.enemyKeep.alive ? g.enemyKeep : null
    u.target = near || castle || nearest(foes, u.x, u.y, 9999, null)
  }
}

/** Where should this unit walk this frame? Returns [x, y] or null. */
function destination(u) {
  const K = u.K
  if (u.fleeing) {
    const p = nearest(g.portals, u.x, u.y, 9999, null)
    return p ? [p.x, p.y] : null
  }
  if (K.healer) {
    // Follow the most injured ally (else the nearest), staying a step behind.
    let best = null, bestScore = -1
    for (const a of g.units) {
      if (!a.alive || a === u || a.team !== 'ally') continue
      const d = dist(u.x, u.y, a.x, a.y)
      const score = (1 - a.hp / a.maxHp) * 200 - d * 0.5 + 60
      if (score > bestScore) { bestScore = score; best = a }
    }
    u.follow = best
    if (!best) return null
    return dist(u.x, u.y, best.x, best.y) > 22 ? [best.x, best.y] : null
  }
  if (K.follower && u.leader && u.leader.alive && !u.target) {
    // Formation: slots in an arc behind the leader's facing.
    const angle = Math.PI + (u.slot - 1) * 0.9
    const fx0 = u.leader.x + Math.cos(angle) * 14 * u.leader.face
    const fy0 = u.leader.y + Math.sin(angle) * 10 - 4
    return dist(u.x, u.y, fx0, fy0) > 4 ? [fx0, fy0] : null
  }
  const t = u.target
  if (!t) return null
  const d = dist(u.x, u.y, t.x, t.y)
  if (K.kite && !t.isKeep && d < K.kite) {
    // Kite: step directly away from a melee threat if there's room.
    const ax = u.x + (u.x - t.x) / (d || 1) * 16, ay = u.y + (u.y - t.y) / (d || 1) * 16
    if (walkableAt(ax, ay)) return [ax, ay]
  }
  if (d <= K.range + reach(t)) return null
  return [t.x, t.y]
}

/** Path-follow toward (tx, ty) with A* + line-of-sight smoothing. */
function steer(u, tx, ty) {
  const K = u.K
  if (K.flying || clearLine(u.x, u.y, tx, ty)) { u.path = null; return [tx, ty] }
  let gcx = cellX(tx), gcy = cellY(ty)
  // Your keep's walls block its own cells: aim for the open cells around it.
  // (The enemy castle stands on open floor — path straight to it.)
  if (u.target && u.target === g.keepUnit) {
    const goal = g.keepGoals.reduce((a, b) => (dist(u.x, u.y, centerX(a[0]), centerY(a[1])) < dist(u.x, u.y, centerX(b[0]), centerY(b[1])) ? a : b))
    gcx = goal[0]; gcy = goal[1]
  }
  const key = gcx + ',' + gcy
  const stale = !u.path || u.pathKey !== key || T - u.pathT > 1.2 || u.stuckT > 0.6
  if (stale && g.searches < 8) {
    g.searches++
    const open = openNear(gcx, gcy)
    const s = openNear(cellX(u.x), cellY(u.y))
    u.path = open && s ? aStar(s[0], s[1], open[0], open[1]) : null
    u.pathKey = key; u.pathT = T; u.stuckT = 0
  }
  if (!u.path || u.path.length === 0) return [tx, ty]
  // Skip waypoints we can already see past.
  while (u.path.length > 1) {
    const n = u.path[1]
    if (!clearLine(u.x, u.y, centerX(n % MAP_W), centerY((n / MAP_W) | 0))) break
    u.path.shift()
  }
  const w = u.path[0]
  const wx = centerX(w % MAP_W), wy = centerY((w / MAP_W) | 0)
  if (dist(u.x, u.y, wx, wy) < 4) u.path.shift()
  return [wx, wy]
}

function act(u) {
  const K = u.K
  u.cd -= dt
  if (K.healer) {
    if (u.cd <= 0) {
      let best = null, need = 0
      for (const a of g.units) {
        if (!a.alive || a.team !== 'ally' || a.hp >= a.maxHp) continue
        if (dist(u.x, u.y, a.x, a.y) > K.range) continue
        const n = a.maxHp - a.hp
        if (n > need) { need = n; best = a }
      }
      if (best) {
        best.hp = Math.min(best.maxHp, best.hp + K.heal * (u.power || 1))
        fx('Heal', best.x, best.y + 6, 4)
        u.cd = K.cd
        if (u.light) u.light.pulse = 0.4
      }
    }
    return
  }
  const t = u.target
  if (!t || !t.alive) return
  const d = dist(u.x, u.y, t.x, t.y)
  if (d > K.range + reach(t) + 2) return
  u.face = t.x >= u.x ? 1 : -1
  if (u.cd > 0) return
  u.cd = K.cd * (0.9 + Math.random() * 0.2)
  if (K.ranged) {
    shoot(K.ranged, u.x, u.y + 8, t, K.dmg * dmgScale(K) * (u.power || 1), u)
  } else {
    u.lunge = 0.15
    damage(t, K.dmg * dmgScale(K) * (u.power || 1), u)
  }
  if (K.summoner && u.summonT <= 0) {
    u.summonT = 9
    for (let k = 0; k < 2; k++) {
      const s = spawnUnit('skeleton', u.x + (k ? 8 : -8), u.y - 4)
      if (s) fx('Poof', s.x, s.y + 6, 10)
    }
    flash(u.x, u.y + 8, [0.7, 0.3, 1], 1.3, 50)
  }
}

/** Integrate movement: seek + separation, slide along walls. */
function move(u) {
  const K = u.K
  const dest = destination(u)
  let ax = 0, ay = 0
  if (dest) {
    const [wx, wy] = steer(u, dest[0], dest[1])
    const d = dist(u.x, u.y, wx, wy) || 1
    ax = (wx - u.x) / d; ay = (wy - u.y) / d
  }
  // Separation: don't stack — push apart from close bodies.
  for (const o of g.units) {
    if (o === u || !o.alive) continue
    const dx = u.x - o.x, dy = u.y - o.y, dd = dx * dx + dy * dy
    const r = 9 * ((K.scale || 1) + (o.K.scale || 1)) / 2
    if (dd > 0.01 && dd < r * r) {
      const dd1 = Math.sqrt(dd), push = (r - dd1) / r * (o.team === u.team ? 1.2 : 0.6)
      ax += dx / dd1 * push; ay += dy / dd1 * push
    }
  }
  const len = Math.hypot(ax, ay)
  u.slowT = Math.max(0, (u.slowT || 0) - dt)
  const sp = K.speed * (u.fleeing ? 1.25 : 1) * (u.slowT > 0 ? 0.45 : 1)
  if (len > 0.05) {
    const nx = u.x + ax / Math.max(1, len) * sp * dt
    const ny = u.y + ay / Math.max(1, len) * sp * dt
    if (K.flying || walkableAt(nx, u.y)) u.x = nx
    if (K.flying || walkableAt(u.x, ny)) u.y = ny
    if (Math.abs(ax) > 0.2) u.face = ax > 0 ? 1 : -1
  }
  // Stuck detection: wanted to move but barely did → force a re-path.
  const moved = dist(u.x, u.y, u.lastX, u.lastY)
  u.stuckT = dest && moved < sp * dt * 0.2 ? u.stuckT + dt : 0
  u.moving = moved > 0.05
  u.lastX = u.x; u.lastY = u.y
}

function drawUnit(u) {
  const K = u.K
  u.animT += dt * (u.moving ? 9 : 5)
  const f = Math.floor(u.animT) % 4
  if (f !== u.frame) { u.frame = f; u.ref.image = unitFrame(K.row, f) }
  const lunge = u.lunge > 0 ? Math.sin((u.lunge / 0.15) * Math.PI) * 3 * u.face : 0
  u.lunge -= dt
  const bob = u.moving ? Math.abs(Math.sin(u.animT * 1.6)) : 0
  const land = u.landT > 0 ? 1 + u.landT * 1.6 : 1
  u.landT -= dt
  const s = (K.scale || 1) * land
  u.ref.x = Math.round(u.x + lunge)
  u.ref.y = Math.round(u.y + bob + (u.landT > 0 ? u.landT * 30 : 0))
  u.ref.scaleX = s * u.face
  u.ref.scaleY = s
  // Tint: red flash on a hit, icy blue while slowed by spikes.
  const slowed = (u.slowT || 0) > 0
  if (u.hitFlash > 0) u.hitFlash -= dt
  const tintKey = u.hitFlash > 0 ? 'hit' : slowed ? 'slow' : 'base'
  if (tintKey !== u.tintKey) {
    u.tintKey = tintKey
    u.ref.fillColor = tintKey === 'hit' ? [1, 0.45, 0.45, 1] : tintKey === 'slow' ? [0.55, 0.78, 1, 1] : K.tint || [1, 1, 1, 1]
  }
  // HP bar only once hurt. (Children flip with the sprite — the fill
  // drains from both ends so that never shows.)
  const frac = Math.max(0, u.hp / u.maxHp)
  if (frac !== u.shownHp) {
    u.shownHp = frac
    const show = frac < 0.999
    u.bar.visible = show; u.fill.visible = show
    u.fill.scaleX = Math.max(0.01, frac)
  }
  if (u.light) {
    u.light.pulse = Math.max(0, (u.light.pulse || 0) - dt)
    u.light.ref.x = u.ref.x
    u.light.ref.y = u.ref.y + 8
    u.light.ref.lightIntensity = 0.9 + (u.light.pulse || 0) * 1.5 + Math.sin(T * 6 + u.i) * 0.08
  }
}

// --------------------------------------------- projectiles & effects --

// Arrows and bolts are prefabs too ("Arrow", "Bolt" — the bolt carries its
// own Light 2D), spawned per shot and destroyed on impact.
function shoot(kind, x, y, target, dmg, from) {
  const ref = g.projectiles.length < POOL.arrows + POOL.bolts ? ctx.spawn(kind === 'arrow' ? 'Arrow' : 'Bolt', Math.round(x), Math.round(y)) : null
  if (!ref) { damage(target, dmg, from); return }
  const p = { alive: true, kind, x, y, target, dmg, from, speed: kind === 'arrow' ? 190 : 120, ref }
  g.projectiles.push(p)
  if (kind !== 'arrow') {
    const c = kind === 'hex' ? [0.8, 0.4, 1, 1] : [1, 0.62, 0.25, 1]
    p.ref.fillColor = c
  }
}

function stepProjectiles() {
  for (const p of g.projectiles) {
    if (!p.alive) continue
    const t = p.target
    const tx = t.x, ty = t.y + (t.isKeep ? 0 : 7)
    const d = dist(p.x, p.y, tx, ty)
    const step = p.speed * dt
    if (!t.alive || d <= step + 2) {
      if (t.alive) damage(t, p.dmg, p.from)
      p.alive = false; p.ref.destroy()
      if (p.kind !== 'arrow') fx('Hit', tx, ty, 6)
      continue
    }
    p.x += (tx - p.x) / d * step
    p.y += (ty - p.y) / d * step
    p.ref.x = Math.round(p.x); p.ref.y = Math.round(p.y)
    // The arrow sprite points down (-90deg); turn it to face its flight.
    if (p.kind === 'arrow') p.ref.rotation = Math.atan2(ty - p.y, tx - p.x) * 180 / Math.PI + 90
  }
  if (g.projectiles.some((p) => !p.alive)) g.projectiles = g.projectiles.filter((p) => p.alive)
}

/** Fire a pooled one-shot particle burst (round-robin per effect kind). */
function fx(name, x, y, n) {
  const list = g.fx[name]
  if (!list) return
  const r = list[g.fxNext[name]++ % list.length]
  r.x = x; r.y = y
  r.emit(n)
}

/** A short light burst that fades out. */
function flash(x, y, color, intensity, radius) {
  const f = g.flashes[g.flashNext++ % g.flashes.length]
  f.t = 1; f.peak = intensity
  f.ref.fillColor = [color[0], color[1], color[2], 1]
  f.ref.lightRadius = radius
  f.ref.x = x; f.ref.y = y
  f.ref.visible = true
}

function stepFlashes() {
  for (const f of g.flashes) {
    if (f.t <= 0) continue
    f.t -= dt * 2.2
    if (f.t <= 0) { f.ref.visible = false; continue }
    f.ref.lightIntensity = f.peak * f.t * f.t
  }
}

function dropCoin(x, y, amount) {
  g.gold += amount
  const c = g.coins.find((q) => q.t <= 0)
  if (!c) return
  c.t = 0.8; c.x = x; c.y = y
  c.ref.visible = true
  fx('Coin', x, y, 6)
}

function stepCoins() {
  for (const c of g.coins) {
    if (c.t <= 0) continue
    c.t -= dt
    if (c.t <= 0) { c.ref.visible = false; c.ref.x = 100000; continue }
    const k = 0.8 - c.t
    c.ref.x = Math.round(c.x)
    c.ref.y = Math.round(c.y + k * 26 - k * k * 14)
    c.ref.image = itemFrame(0, Math.floor(k * 14) % 4)
    c.ref.fillColor = [1, 1, 1, clamp01(c.t * 3)]
  }
}

// --------------------------------------------- buildings & the keep --

function stepBuildings() {
  for (const b of g.buildings) {
    if (!b.alive) continue
    b.life -= dt
    if (b.kind === 'spikes') {
      b.cd -= dt
      b.anim = Math.max(0, b.anim - dt)
      // Anything walking over the spikes is slowed to a crawl...
      for (const u of g.units) {
        if (u.alive && u.team === 'enemy' && !u.K.flying && dist(u.x, u.y, b.x, b.y) < 16) u.slowT = 0.8
      }
      // ...and stung twice a second.
      if (b.cd <= 0) {
        let hit = false
        for (const u of g.units) {
          if (u.alive && u.team === 'enemy' && !u.K.flying && dist(u.x, u.y, b.x, b.y) < 14) { damage(u, 30 * (b.power || 1), null); hit = true }
        }
        if (hit) { b.anim = 0.3; fx('Hit', b.x, b.y, 4) }
        b.cd = 0.5
      }
      b.ref.image = itemFrame(5, b.anim > 0 ? 0 : 3)
    } else {
      // Brazier: a fire turret that lights up the room around it.
      b.cd -= dt
      if (b.cd <= 0) {
        const t = nearest(enemiesOf('ally'), b.x, b.y, 92, (o) => !o.isKeep)
        if (t) { shoot('fire', b.x, b.y + 12, t, 72 * (b.power || 1), null); b.cd = 0.7 }
      }
      b.ref.image = itemFrame(3, Math.floor(T * 8) % 4)
      b.ref.lightIntensity = 1.1 + Math.sin(T * 17 + b.x) * 0.08 + Math.sin(T * 5.3) * 0.06
      b.ref.lightRadius = 72 + Math.sin(T * 9) * 3
      b.ref.fillColor = b.hitFlash > 0 ? [1, 0.45, 0.45, 1] : [1, 1, 1, clamp01(b.life / 3 + 0.3)]
      b.hitFlash -= dt
    }
    if (b.life <= 0 || b.hp <= 0) {
      b.alive = false; b.ref.visible = false; b.ref.x = 100000
      fx('Poof', b.x, b.y + 6, 14)
    }
  }
}

function stepKeep() {
  const k = g.keepUnit
  // The keep fights back: a volley at the closest monster in range.
  k.cd -= dt
  if (k.cd <= 0) {
    const t = nearest(enemiesOf('ally'), k.x, k.y, 92, null)
    if (t) { shoot('arrow', k.x + (t.x < k.x ? -10 : 10), k.y + 10, t, 40, null); k.cd = 0.7 }
  }
  const frac = Math.max(0, k.hp / k.maxHp)
  if (frac !== k.shown) {
    k.shown = frac
    ref('castleHp').scaleX = Math.max(0.01, frac)
    ref('castleHp').fillColor = frac > 0.5 ? [0.45, 0.95, 0.5, 1] : frac > 0.25 ? [1, 0.8, 0.3, 1] : [1, 0.35, 0.35, 1]
  }
  if (k.hitFlash > 0) { k.hitFlash -= dt; ref('castle').fillColor = k.hitFlash > 0 ? [1, 0.6, 0.6, 1] : [1, 1, 1, 1] }
  const c = ref('castle')
  c.lightIntensity = 1.4 + Math.sin(T * 3) * 0.06 + (k.hitFlash > 0 ? 0.4 : 0)
  for (let i = 0; i < 2; i++) ref('banner' + i).image = itemFrame(4, Math.floor(T * 6 + i * 2) % 4)
}

// ------------------------------------------------ cards & deployment --

function cardImage(cardIndex) { return img('card ' + cardIndex) }

function dealHand() {
  const order = CARDS.map((_, i) => i).sort(() => Math.random() - 0.5)
  g.hand = order.slice(0, 4)
  g.queue = order.slice(4)
  for (let i = 0; i < 4; i++) refreshSlot(i)
}

function refreshSlot(i) {
  ref('card' + i).image = cardImage(g.hand[i])
  ref('cost' + i).textContent = String(CARDS[g.hand[i]].cost)
  ref('nextCard').image = cardImage(g.queue[0])
}

function canDeploy(card, x, y) {
  if (y < TRAY_TOP + 2 || y > MAP_TOP || x < MAP_LEFT || x > -MAP_LEFT) return false
  if (card.type === 'spell') return true
  const cx = cellX(x), cy = cellY(y)
  if (cy < DEPLOY_ROW || !walkable(cx, cy)) return false
  if (card.type === 'build') return !g.buildings.some((b) => b.alive && dist(b.x, b.y, x, y) < 12)
  return allyCount() + card.spawn.length <= ALLY_CAP
}

function deploy(card, x, y) {
  if (card.type === 'spell') {
    // Fireball: lobbed from the keep, explodes on arrival.
    g.fireball = { t: 0, sx: g.keepUnit.x, sy: g.keepUnit.y + 16, x, y, card }
    ref('fireball').visible = true
    return
  }
  if (card.type === 'build') {
    // Snap to the cell center.
    const bx = centerX(cellX(x)), by = centerY(cellY(y)) - 4
    const pool = card.key === 'spikes' ? g.spikes : g.braziers
    const b = pool.find((q) => !q.alive) || pool.reduce((a, q) => (q.life < a.life ? q : a))
    const power = cardPower(card)
    Object.assign(b, { alive: true, kind: card.key, x: bx, y: by, life: card.key === 'spikes' ? 45 : 40, hp: 700 * power, maxHp: 700 * power, power, cd: 0.3, anim: 0, hitFlash: 0, isBuilding: true })
    b.ref.x = bx; b.ref.y = card.key === 'spikes' ? by + 4 : by
    b.ref.visible = true
    fx('Deploy', bx, by + 4, 16)
    flash(bx, by + 6, [1, 0.8, 0.5], 1.0, 46)
    return
  }
  let leader = null
  card.spawn.forEach(([kind, ox, oy], k) => {
    let px = x + ox, py = y + oy
    if (!walkableAt(px, py)) { px = x; py = y }
    const u = spawnUnit(kind, px, py, cardPower(card))
    if (!u) return
    if (KINDS[kind].leader) leader = u
    else if (leader) { u.leader = leader; u.slot = k - 1 }
  })
  fx('Deploy', x, y + 4, 22)
  flash(x, y + 8, [0.6, 0.85, 1], 1.2, 48)
}

function stepFireball() {
  const fb = g.fireball
  if (!fb) return
  fb.t += dt / 0.6
  const t = Math.min(1, fb.t)
  const x = fb.sx + (fb.x - fb.sx) * t
  const y = fb.sy + (fb.y - fb.sy) * t + Math.sin(t * Math.PI) * 40
  const r = ref('fireball')
  r.x = Math.round(x); r.y = Math.round(y)
  r.lightIntensity = 1.3 + Math.sin(T * 40) * 0.15
  fx('Trail', x, y, 3)
  if (t >= 1) {
    r.visible = false; r.x = 100000
    for (const u of g.units) {
      if (!u.alive || u.team !== 'enemy') continue
      const d = dist(u.x, u.y + 6, fb.x, fb.y)
      if (d < fb.card.radius) {
        damage(u, fb.card.dmg * cardPower(fb.card), null)
        // Knockback away from the blast (respecting walls).
        const kx = u.x + (u.x - fb.x) / (d || 1) * 10, ky = u.y + (u.y - fb.y) / (d || 1) * 10
        if (walkableAt(kx, u.y)) u.x = kx
        if (walkableAt(u.x, ky)) u.y = ky
      }
    }
    // Like Clash Royale, a fireball also chips a tower it lands on.
    const ek = g.enemyKeep
    if (ek && ek.alive && dist(ek.x, ek.y, fb.x, fb.y) < fb.card.radius + 22) damage(ek, fb.card.dmg * cardPower(fb.card) * 0.35, null)
    fx('Boom', fb.x, fb.y, 70)
    flash(fb.x, fb.y, [1, 0.6, 0.25], 2.2, 90)
    g.shake = 0.3
    g.fireball = null
  }
}

function stepHand() {
  const mx = ctx.mouseX, my = ctx.mouseY
  const down = keys.has('mouse')
  const pressed = down && !g.mouseWas
  const released = !down && g.mouseWas
  g.mouseWas = down

  // Elixir tops up continuously; the Hand layer's state sets the rate
  // (Royale: the match clock does — 1x, 2x, 3x).
  const mult = ROYALE ? elixirMult() : g.elixirRate || 1
  const rate = mult / 2.8
  g.elixir = Math.min(ELIXIR_MAX, g.elixir + rate * dt)

  if (pressed && (g.phase === 'break' || g.phase === 'battle')) {
    for (let i = 0; i < 4; i++) {
      if (Math.abs(mx - CARD_X(i)) < 19 && Math.abs(my - CARD_Y) < 24) {
        if (g.elixir >= CARDS[g.hand[i]].cost) g.drag = i
        else g.wiggle[i] = 0.35
      }
    }
  }

  const ghost = ref('ghost'), ring = ref('ghostRing'), zone = ref('zone')
  if (g.drag !== null) {
    const card = CARDS[g.hand[g.drag]]
    const over = my > TRAY_TOP
    const ok = canDeploy(card, mx, my)
    zone.visible = card.type !== 'spell'
    // Ghost preview: the unit (or building) itself, translucent.
    if (over && card.type !== 'spell') {
      const kind = card.spawn ? card.spawn[0][0] : null
      ghost.image = kind ? unitFrame(KINDS[kind].row, Math.floor(T * 6) % 4) : itemFrame(card.key === 'spikes' ? 5 : 3, 0)
      ghost.x = Math.round(mx); ghost.y = Math.round(my)
      ghost.fillColor = ok ? [1, 1, 1, 0.7] : [1, 0.4, 0.4, 0.55]
      ghost.visible = true
    } else ghost.visible = false
    if (over) {
      const r = (card.radius || 10) / 12
      ring.x = Math.round(mx); ring.y = Math.round(my + (card.type === 'spell' ? 0 : 4))
      ring.scaleX = r; ring.scaleY = r * (card.type === 'spell' ? 1 : 0.6)
      ring.fillColor = ok ? [0.5, 1, 0.6, 0.12] : [1, 0.3, 0.3, 0.12]
      ring.visible = true
    } else ring.visible = false
    // The held card lifts out of the hand while dragging.
    const cardRef = ref('card' + g.drag)
    cardRef.x = over ? CARD_X(g.drag) : Math.round(mx)
    cardRef.y = over ? CARD_Y + 8 : Math.round(Math.max(my, CARD_Y))
    if (released) {
      if (over && ok && g.elixir >= card.cost) {
        g.elixir -= card.cost
        deploy(card, mx, my)
        const played = g.hand[g.drag]
        g.hand[g.drag] = g.queue.shift()
        g.queue.push(played)
        refreshSlot(g.drag)
        g.dealT[g.drag] = 0.25
      }
      cardRef.x = CARD_X(g.drag); cardRef.y = CARD_Y
      g.drag = null
      ghost.visible = false; ring.visible = false; zone.visible = false
    }
  }

  // Card states: dim when unaffordable, wiggle on a failed grab, pop in
  // when freshly dealt.
  for (let i = 0; i < 4; i++) {
    const c = ref('card' + i)
    const afford = g.elixir >= CARDS[g.hand[i]].cost
    if (i !== g.drag) {
      const w = g.wiggle[i] > 0 ? Math.sin(g.wiggle[i] * 60) * 2 : 0
      g.wiggle[i] = Math.max(0, g.wiggle[i] - dt)
      g.dealT[i] = Math.max(0, g.dealT[i] - dt)
      c.x = Math.round(CARD_X(i) + w)
      c.y = Math.round(CARD_Y - g.dealT[i] * 30)
    }
    const tint = afford ? 1 : 0.42
    if (c.fillColor[0] !== tint) c.fillColor = [tint, tint, tint + (afford ? 0 : 0.08), 1]
  }

  // Elixir bar.
  const frac = g.elixir / ELIXIR_MAX
  const fill = ref('elxFill')
  fill.scaleX = Math.max(0.005, frac)
  fill.x = BAR_LEFT + (BAR_W * frac) / 2
  const shown = String(Math.floor(g.elixir))
  if (ref('elxText').textContent !== shown) ref('elxText').textContent = shown
  if (ROYALE) {
    // Overtime glow: violet at 2x, hot pink at 3x.
    const p = 0.85 + Math.sin(T * (mult === 3 ? 12 : 8)) * 0.15
    fill.fillColor = mult === 1 ? [1, 1, 1, 1] : mult === 2 ? [1, p, 1, 1] : [1, p * 0.7, p, 1]
  }
}

// ---------------------------------------------- Royale: the enemy side --

/** The enemy castle shoots back and shows its health. */
function stepEnemyKeep() {
  const k = g.enemyKeep
  if (!k || !k.alive) return
  k.cd -= dt
  if (k.cd <= 0) {
    const t = nearest(enemiesOf('enemy'), k.x, k.y, 112, (o) => !o.isKeep && !o.isBuilding)
    if (t) { shoot('arrow', k.x + (t.x < k.x ? -10 : 10), k.y + 10, t, 65, null); k.cd = 0.6 }
  }
  const frac = Math.max(0, k.hp / k.maxHp)
  if (frac !== k.shown && k.fill) {
    k.shown = frac
    k.fill.scaleX = Math.max(0.01, frac)
    k.fill.x = -19 * (1 - frac)
  }
  if (k.hitFlash > 0) k.hitFlash -= dt
  k.ref.fillColor = k.hitFlash > 0 ? [1, 0.75, 0.75, 1] : [1, 0.6, 0.6, 1]
  k.ref.lightIntensity = 1.2 + Math.sin(T * 3) * 0.08 + (k.hitFlash > 0 ? 0.5 : 0)
}

/** The enemy "player". Earns elixir (a bit faster than you) and plays it
 *  like a person would:
 *  - DEFEND: your troops near its castle -> answer at once, from the
 *    castle, with the best counter it can afford (swarm a tank, tank a swarm);
 *  - PUNISH: you just spent down (elixir < 3) -> push immediately;
 *  - BUILD UP: otherwise save toward a big push (8+ elixir), then send a
 *    tank with support from the portal nearest your keep.
 *  Small reaction delays keep it beatable. */
function stepEnemyBot() {
  g.enemyElixir = Math.min(ELIXIR_MAX, g.enemyElixir + ((elixirMult() * BOT_ELIXIR_RATE) / 2.8) * dt)
  g.botCd = (g.botCd || 0) - dt
  if (g.botCd > 0) return
  const castle = g.enemyKeep
  const raiders = castle ? g.units.filter((u) => u.alive && u.team === 'ally' && dist(u.x, u.y, castle.x, castle.y) < 130) : []
  const deck = ENEMY_CARDS.filter((c) => !c.late || g.matchT <= 60)
  const affordable = deck.filter((c) => c.cost <= g.enemyElixir)
  let card = null, at = null
  if (raiders.length && affordable.length) {
    // Counter-pick: lots of small troops -> splash/tank; one big one -> swarm.
    const heavy = raiders.some((u) => u.maxHp >= 400)
    const score = (c) => {
      const kinds = c.spawn.map((k) => KINDS[k])
      const swarm = c.spawn.length >= 3, tank = kinds.some((K) => K.hp >= 600), ranged = kinds.some((K) => K.ranged)
      return (heavy && swarm ? 3 : 0) + (!heavy && raiders.length >= 3 && (tank || ranged) ? 3 : 0) + (ranged ? 1 : 0) + c.cost * 0.3
    }
    card = affordable.sort((a, b) => score(b) - score(a))[0]
    at = g.portals[0]
  } else if (affordable.length && (g.elixir < 3 || g.enemyElixir >= 8)) {
    // Push: the strongest thing it can afford, from the portal nearest your keep.
    card = affordable.sort((a, b) => b.cost - a.cost)[0]
    at = g.portals.reduce((a, b) => (dist(a.x, a.y, g.keepUnit.x, g.keepUnit.y) < dist(b.x, b.y, g.keepUnit.x, g.keepUnit.y) ? a : b))
  }
  if (!card) { g.botCd = 0.3; return }
  const n = card.spawn.length
  card.spawn.forEach((kind, k) => {
    const u = spawnUnit(kind, at.x + (k - (n - 1) / 2) * 9, at.y - 4, enemyPower())
    if (u) u.landT = 0
  })
  fx('Poof', at.x, at.y + 4, 12)
  flash(at.x, at.y, [0.7, 0.35, 1], 1.4, 44)
  if (card.spawn[0] === 'king') message('THE BONE KING', 'JOINS THE BATTLE', 2)
  g.enemyElixir -= card.cost
  g.botCd = raiders.length ? 0.6 : 1.2 + Math.random()  // reaction time
}

/** The match clock: elixir multiplier callouts, the last-10s countdown,
 *  and time-up (the side with more castle left wins). */
function stepMatch() {
  const before = elixirMult()
  const tick = Math.ceil(g.matchT)
  g.matchT = Math.max(0, g.matchT - dt)
  const after = elixirMult()
  if (after !== before) message(after + 'X ELIXIR!', after === 3 ? 'FINAL 30 SECONDS' : 'LAST MINUTE', 2)
  else if (g.matchT > 0 && g.matchT <= 10 && Math.ceil(g.matchT) !== tick) message(String(Math.ceil(g.matchT)), '', 0.7)
  stepEnemyBot()
  if (g.matchT <= 0 && !g.sentFallen) {
    const mine = g.keepUnit.hp / g.keepUnit.maxHp, theirs = g.enemyKeep.hp / g.enemyKeep.maxHp
    g.result = mine > theirs + 0.001 ? 'win' : theirs > mine + 0.001 ? 'lose' : 'draw'
    g.timeUp = true
    g.sentFallen = true
    ctx.setInput('fallen', true)
  }
}


// ------------------------------------------------- flow helpers & setup --

function buildWave(n) {
  const list = []
  if (n % 5 === 0) list.push('king')
  let budget = 3 + Math.floor(n * 3.2)
  const menu = ENEMY_MENU.filter((e) => n >= e.from)
  while (budget > 0) {
    const e = pick(menu)
    if (e.cost > budget && budget < 6) { list.push('skeleton'); budget -= 1; continue }
    list.push(e.kind); budget -= e.cost
  }
  return list.sort(() => Math.random() - 0.5)
}

function message(main, sub, time) {
  ref('msg').textContent = main
  ref('subMsg').textContent = sub || ''
  g.msgT = time || 0
}

const activePortals = () => Math.min(g.portals.length, 1 + Math.floor((g.wave - 1) / 3))

/** Fresh run: new dungeon, full keep, new hand. (Title state's entry.) */
function restart() {
  for (const u of g.units) { u.alive = false; u.ref.destroy(); if (u.light) { u.light.used = false; u.light.ref.visible = false; u.light = null } }
  g.units = []
  for (const p of g.projectiles) { p.alive = false; p.ref.destroy() }
  g.projectiles = []
  for (const b of g.buildings) { b.alive = false; b.ref.visible = false; b.ref.x = 100000 }
  generateDungeon()
  if (g.enemyKeep) { g.enemyKeep.ref.destroy(); g.enemyKeep = null }
  g.result = ''
  g.timeUp = false
  g.matchT = MATCH_TIME
  g.enemyElixir = 7
  g.botCd = 2
  if (ROYALE) {
    // The enemy castle (a prefab) stands in the room farthest from your keep.
    const p = g.portals[0]
    const castle = ctx.spawn('Castle', p.x, p.y - 14, g.world)
    if (castle) {
      castle.scaleX = 0.8; castle.scaleY = 0.8
      g.enemyKeep = { isKeep: true, alive: true, team: 'enemy', x: p.x, y: p.y + 4, hp: ENEMY_KEEP_HP, maxHp: ENEMY_KEEP_HP, cd: 1, hitFlash: 0, shown: -1, K: null, ref: castle, fill: castle.part('HP') }
    }
  }
  // The keep's aim/hit point is its center, 24px above its base.
  g.keepUnit = { isKeep: true, alive: true, team: 'ally', x: 0, y: g.keep.y + 24, hp: 3000, maxHp: 3000, cd: 0, hitFlash: 0, shown: -1, K: null }
  ref('castle').x = 0; ref('castle').y = g.keep.y; ref('castle').fillColor = [1, 1, 1, 1]
  g.elixir = 6
  g.gold = 0
  g.wave = 1
  g.fireball = null
  g.drag = null
  g.spawnQueue = []
  dealHand()
}

/** Push game numbers into the state machine (only when they change) so
 *  its transitions — Elixir x2, Danger, the banner alarm — can react. */
function syncInputs() {
  const hp = Math.round(Math.max(0, g.keepUnit.hp / g.keepUnit.maxHp) * 100) / 100
  if (hp !== g.sentHp) { g.sentHp = hp; ctx.setInput('keepHp', hp) }
  if (g.wave !== g.sentWave) { g.sentWave = g.wave; ctx.setInput('wave', g.wave) }
}

/** True on the first frame a layer spends in its current state. */
function entered() {
  const key = ctx.layer || 'flow'
  if (g.cur[key] === ctx.state) return false
  g.cur[key] = ctx.state
  return true
}

if (!g.init) {
  g.init = true
  g.refs = {}
  g.imgs = {}
  g.cur = {}
  for (const im of ctx.images) if (g.imgs[im.name] === undefined) g.imgs[im.name] = im.id
  g.crowd = new Uint8Array(N)
  g.units = []
  // Units / arrows / bolts are spawned from prefabs on demand (spawnUnit, shoot).
  g.unitSerial = 0
  g.world = ref('world')
  g.projectiles = []
  g.lights = []; for (let i = 0; i < POOL.lights; i++) g.lights.push({ used: false, ref: ref('ul' + i) })
  g.flashes = []; for (let i = 0; i < POOL.flashes; i++) g.flashes.push({ t: 0, ref: ref('fl' + i) })
  g.flashNext = 0
  g.coins = []; for (let i = 0; i < POOL.coins; i++) g.coins.push({ t: 0, ref: ref('coin' + i) })
  g.spikes = []; for (let i = 0; i < POOL.spikes; i++) g.spikes.push({ alive: false, life: 0, ref: ref('spike' + i) })
  g.braziers = []; for (let i = 0; i < POOL.braziers; i++) g.braziers.push({ alive: false, life: 0, ref: ref('braz' + i) })
  g.buildings = g.spikes.concat(g.braziers)
  g.fx = {
    Hit: [0, 1, 2, 3].map((i) => ref('fxHit' + i)),
    Poof: [0, 1, 2].map((i) => ref('fxPoof' + i)),
    Deploy: [0, 1].map((i) => ref('fxDeploy' + i)),
    Heal: [0, 1].map((i) => ref('fxHeal' + i)),
    Boom: [ref('fxBoom')],
    Trail: [ref('fxTrail')],
    Coin: [ref('fxCoin')],
  }
  g.fxNext = { Hit: 0, Poof: 0, Deploy: 0, Heal: 0, Boom: 0, Trail: 0, Coin: 0 }
  g.wiggle = [0, 0, 0, 0]
  g.dealT = [0, 0, 0, 0]
  g.mouseWas = keys.has('mouse')
  g.shake = 0
  g.elixirRate = 1
  g.phase = 'intro'
  ctx.setInput('fallen', false)
  restart()
}

/** Bots + projectiles + buildings + spells: one simulation step. */
function simulate() {
  g.searches = 0
  g.crowd.fill(0)
  for (const u of g.units) if (u.alive) { const c = cellY(u.y) * MAP_W + cellX(u.x); if (c >= 0 && c < N) g.crowd[c] = Math.min(255, g.crowd[c] + 1) }
  for (const u of g.units) {
    if (!u.alive) continue
    if (u.summonT !== undefined) u.summonT -= dt
    if (u.fleeing) u.hp = Math.min(u.maxHp, u.hp + 14 * dt)
    // Bots re-think a few times a second (staggered), act every frame.
    if (((T * 6 + u.i * 0.37) | 0) !== u.thinkTick) { u.thinkTick = (T * 6 + u.i * 0.37) | 0; think(u) }
    if (u.landT > 0) continue  // still dropping in
    move(u)
    act(u)
  }
  stepProjectiles()
  stepBuildings()
  stepKeep()
  stepEnemyKeep()
  stepFireball()
  if (g.keepUnit.hp <= 0 && g.phase !== 'lost' && !g.sentFallen) { g.result = 'lose'; g.sentFallen = true; ctx.setInput('fallen', true) }
  if (g.enemyKeep && g.enemyKeep.hp <= 0 && g.phase !== 'lost' && !g.sentFallen) { g.result = 'win'; g.sentFallen = true; ctx.setInput('fallen', true) }
}

function drawWorld() {
  for (const u of g.units) if (u.alive) drawUnit(u)
  stepFlashes()
  stepCoins()
}

/** Torches, portals, HUD labels, screen shake. `danger`: keep nearly gone. */
function ambience(danger) {
  for (let i = 0; i < g.torches.length; i++) {
    const r = ref('torch' + i), t = g.torches[i]
    const f = Math.floor(T * (danger ? 12 : 8) + t.phase) % 4
    if (f !== t.frame) { t.frame = f; r.image = itemFrame(1, f) }
    const flicker = danger ? 0.18 : 0.08
    r.lightIntensity = 1.3 + Math.sin(T * 13 + t.phase) * flicker + Math.sin(T * 7.1 + t.phase * 2) * 0.06
  }
  const portals = activePortals()
  for (let i = 0; i < portals && i < POOL.portals; i++) {
    ref('portal' + i).lightIntensity = (g.phase === 'battle' ? 1.2 : 0.8) + Math.sin(T * 4 + i) * 0.2
  }
  if (g.msgT > 0) { g.msgT -= dt; if (g.msgT <= 0) message('', '', 0) }
  const foes = g.units.reduce((n, u) => n + (u.alive && u.team === 'enemy' ? 1 : 0), 0) + (g.spawnQueue ? g.spawnQueue.length : 0)
  const foesLabel = ROYALE
    ? (g.enemyKeep ? Math.ceil(Math.max(0, g.enemyKeep.hp / g.enemyKeep.maxHp) * 100) + '%' : '-')
    : g.phase === 'battle' ? String(foes) : '-'
  if (ref('foesText').textContent !== foesLabel) ref('foesText').textContent = foesLabel
  const tip = TIPS[Math.floor(T / 6) % TIPS.length]
  if (ref('tipText').textContent !== tip) ref('tipText').textContent = tip
  for (let i = 0; i < 2; i++) ref('trayCandle' + i).image = itemFrame(3, Math.floor(T * 7 + i * 2) % 4)
  const secs = Math.ceil(g.matchT || 0)
  const label = ROYALE ? Math.floor(secs / 60) + ':' + String(secs % 60).padStart(2, '0') : 'WAVE ' + g.wave
  if (ref('waveText').textContent !== label) ref('waveText').textContent = label
  const gold = String(g.gold)
  if (ref('goldText').textContent !== gold) ref('goldText').textContent = gold
  const cam = ref('camera')
  if (g.shake > 0) {
    g.shake -= dt
    cam.x = Math.round((Math.random() - 0.5) * 6 * g.shake / 0.3)
    cam.y = Math.round((Math.random() - 0.5) * 6 * g.shake / 0.3)
  } else if (cam.x !== 0 || cam.y !== 0) { cam.x = 0; cam.y = 0 }
}

// ===================================================================
//  Everything above is the shared library; each section below is one
//  state's Code program (Rust prepends the library to each).
// ===================================================================

//@program Title
// Game Flow layer — fresh dungeon, title card, then `begin`.
if (entered()) {
  g.phase = 'intro'
  if (g.started) restart()
  g.started = true
  g.sentFallen = false
  ctx.setInput('fallen', false)
  for (let i = 0; i < POOL.portals; i++) ref('portal' + i).visible = false
  if (ROYALE) message('ROYALE', 'DESTROY THE ENEMY CASTLE', 1.8)
  else message('GET READY', 'DEFEND THE KEEP', 1.8)
  g.flowFired = false
}
if (ctx.time > 1.8 && !g.flowFired) { g.flowFired = true; ctx.fireTrigger('begin') }
syncInputs()

//@program Wave Break
// Game Flow layer — breather between waves, then `go`.
if (entered()) {
  g.phase = 'break'
  for (let i = 0; i < POOL.portals; i++) ref('portal' + i).visible = false
  if (ROYALE) message('FIGHT!', '2:00 ON THE CLOCK', 2.4)
  else if (g.wave === 1) message('WAVE 1', 'DRAG CARDS TO DEFEND', 2.4)
  else message('WAVE ' + g.wave, g.wave === 6 ? '2X ELIXIR!' : 'CLEARED  +2 ELIXIR', 2.8)
  g.flowFired = false
}
if (ctx.time > 3 && !g.flowFired) { g.flowFired = true; ctx.fireTrigger('go') }
syncInputs()

//@program Battle
// Game Flow layer — portals open and the wave pours out; `cleared` when
// the last monster falls. (Defeat is a Bool transition on `fallen`.)
if (entered()) {
  g.phase = 'battle'
  g.spawnQueue = ROYALE ? [] : buildWave(g.wave)
  g.spawnT = 0
  g.flowFired = false
  for (let i = 0; i < POOL.portals; i++) ref('portal' + i).visible = ROYALE ? i < g.portals.length : i < activePortals()
}
// Royale: one long battle against the clock and the enemy bot.
if (ROYALE) stepMatch()
g.spawnT -= dt
if (!ROYALE && g.spawnT <= 0 && g.spawnQueue.length) {
  const p = g.portals[rand(activePortals())]
  const kind = g.spawnQueue.shift()
  const u = spawnUnit(kind, p.x + (Math.random() - 0.5) * 8, p.y - 4)
  if (u) { u.landT = 0; fx('Poof', p.x, p.y + 4, 12); flash(p.x, p.y, [0.7, 0.35, 1], 1.4, 44) }
  else g.spawnQueue.push(kind)  // pool busy: try again shortly
  if (kind === 'king' && u) message('THE BONE KING', 'HAS AWOKEN', 2.5)
  g.spawnT = Math.max(0.45, 1.1 - g.wave * 0.05)
}
if (!ROYALE && !g.flowFired && !g.spawnQueue.length && !g.units.some((u) => u.alive && u.team === 'enemy')) {
  g.flowFired = true
  g.wave++
  g.elixir = Math.min(ELIXIR_MAX, g.elixir + 2)
  ctx.fireTrigger('cleared')
}
syncInputs()

//@program Defeat
// Game Flow layer — the keep fell: explode it, record the run, and after a
// beat hand over to the Game Over scene (its own scene + state machine).
if (entered()) {
  g.phase = 'lost'
  const result = g.result || 'lose'
  // Whichever castle fell (none on a time-up draw) goes up in flames.
  const fallen = result === 'win' ? g.enemyKeep : result === 'lose' ? g.keepUnit : null
  if (fallen && (!g.timeUp || fallen.hp <= 0)) {
    fallen.alive = false
    if (fallen.ref) fallen.ref.visible = false
    fx('Boom', fallen.x, fallen.y, 90)
    flash(fallen.x, fallen.y, [1, 0.4, 0.2], 2.5, 120)
    g.shake = 0.5
  }
  if (result === 'win') message('VICTORY!', g.timeUp ? 'TIME UP - YOU HELD MORE' : 'CASTLE DESTROYED', 0)
  else if (result === 'draw') message('DRAW', 'TIME UP', 0)
  else message(ROYALE ? 'DEFEAT' : 'KEEP FALLEN', g.timeUp ? 'TIME UP' : '', 0)
  // Record the run and bank the gold (saved: progress persists).
  const bonus = ROYALE ? (result === 'win' ? 40 : result === 'draw' ? 15 : 5) : 0
  royale.result = result
  royale.wave = g.wave
  royale.gold = g.gold + bonus
  royale.bank = (royale.bank || 0) + royale.gold
  if (ROYALE) {
    royale.trophies = Math.max(0, (royale.trophies || 0) + (result === 'win' ? 30 : result === 'lose' ? -15 : 0))
    if (result === 'win') royale.wins = (royale.wins || 0) + 1
    if (result === 'lose') royale.losses = (royale.losses || 0) + 1
  } else {
    royale.best = Math.max(royale.best || 0, g.wave)
  }
  saveRoyale()
  g.flowFired = false
}
if (ctx.time > 2.2 && !g.flowFired) {
  g.flowFired = true
  // Leaving restores this scene's designed pose, so cached refs would go
  // stale: the next visit starts from a clean slate.
  ctx.globals.dungeon = {}
  ctx.loadScene('Royale Game Over')
}

//@program Running
// Simulation layer — the bots think, move, fight.
entered()
simulate()
drawWorld()

//@program Frozen
// Simulation layer — paused while the keep lies in ruins.
entered()
drawWorld()

//@program Elixir x1
// Hand layer — cards, drag & drop, normal elixir.
if (entered()) { g.elixirRate = 1; ref('elxFill').fillColor = [1, 1, 1, 1] }
stepHand()

//@program Elixir x2
// Hand layer — overtime: double elixir and a pulsing bar.
if (entered()) g.elixirRate = 2
stepHand()
const pulse = 0.85 + Math.sin(T * 8) * 0.15
ref('elxFill').fillColor = [1, pulse, 1, 1]

//@program Calm
// Ambience layer — steady torches.
entered()
ambience(false)

//@program Danger
// Ambience layer — keep below 35%: frantic torches, the keep's hearth
// pulses an alarm, a one-time warning.
if (entered()) message('', 'THE KEEP IS FALLING!', 2)
ambience(true)
ref('castle').lightIntensity = 1.2 + (Math.sin(T * 10) * 0.5 + 0.5) * 0.9
