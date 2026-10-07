// ===================================================================
//  ARENA ROYALE — a Clash Royale match against an AI opponent.
//
//  Two halves split by a chasm, two bridges, three towers a side (two
//  Princess towers on the lanes, the King tower behind). Knock out towers
//  for crowns; the King tower is worth the match. 2:00 on the clock — 2x
//  elixir in the last minute, 3x in the last 30 seconds. At time up the
//  side with more crowns wins (then more tower health; else a draw).
//
//  The opponent is a *utility AI*: every decision it scores each card it
//  can afford against every move it could make with it (defend a lane,
//  punish a spent opponent, build a push, finish a weak tower, blast a
//  clump with a spell) and plays the best — or saves elixir when nothing
//  scores well enough. It counts your elixir, picks counters (swarms for
//  tanks, splash for swarms), places defenders where its towers help, and
//  follows its tanks with support. Its intent is shown along the bottom.
//
//  Part of Dungeon Royale (Home -> ARENA VS AI): shares its card levels and
//  gold bank (upgrade in the shop), and ends on its Game Over screen.
//  Every state below is one Code program; Rust prepends this library.
// ===================================================================

const g = ctx.globals.arena || (ctx.globals.arena = {})
function loadJson(key, fallback) {
  const v = ctx.storage.get(key)
  return Object.assign(fallback, v && typeof v === 'object' ? v : {})
}
// Shared with Dungeon Royale: card levels + gold bank. Arena: its own trophies.
const royale = ctx.globals.royale || (ctx.globals.royale = loadJson('royale', { bank: 0, levels: {}, trophies: 0, best: 0, mode: 'royale' }))
const arenaSave = ctx.globals.arenaSave || (ctx.globals.arenaSave = loadJson('arena', { trophies: 0, wins: 0, losses: 0, draws: 0 }))
const dt = Math.min(ctx.deltaSeconds, 1 / 20)
const keys = ctx.keysDown
const T = performance.now() / 1000

// ------------------------------------------------------------ data --

const ROW = { knight: 0, captain: 1, squire: 2, archer: 3, cleric: 4, rogue: 5, skeleton: 6, reaper: 7, wisp: 8, necro: 9 }
const KINDS = {
  // Your troops.
  knight:   { team: 'you', row: ROW.knight, hp: 520, dmg: 70, cd: 1.1, range: 12, speed: 34, sight: 80 },
  captain:  { team: 'you', row: ROW.captain, hp: 640, dmg: 80, cd: 1.1, range: 12, speed: 30, sight: 80, light: [1, 0.85, 0.5], leader: true },
  squire:   { team: 'you', row: ROW.squire, hp: 150, dmg: 32, cd: 0.8, range: 12, speed: 42, sight: 70, scale: 0.8, follower: true },
  archer:   { team: 'you', row: ROW.archer, hp: 160, dmg: 36, cd: 0.9, range: 72, speed: 36, sight: 90, ranged: 'arrow' },
  cleric:   { team: 'you', row: ROW.cleric, hp: 260, dmg: 0, heal: 40, cd: 1.0, range: 46, speed: 36, sight: 110, light: [0.45, 1, 0.55], healer: true },
  rogue:    { team: 'you', row: ROW.rogue, hp: 200, dmg: 55, cd: 0.6, range: 12, speed: 60, sight: 130, assassin: true },
  // The AI's monsters.
  skeleton: { team: 'ai', row: ROW.skeleton, hp: 70, dmg: 30, cd: 1.0, range: 12, speed: 40, sight: 70 },
  reaper:   { team: 'ai', row: ROW.reaper, hp: 420, dmg: 110, cd: 1.5, range: 14, speed: 30, sight: 70 },
  wisp:     { team: 'ai', row: ROW.wisp, hp: 90, dmg: 22, cd: 0.7, range: 10, speed: 70, sight: 0, flying: true, towersOnly: true, light: [0.4, 0.7, 1] },
  necro:    { team: 'ai', row: ROW.necro, hp: 300, dmg: 44, cd: 1.4, range: 70, speed: 28, sight: 92, ranged: 'hex', light: [0.75, 0.35, 1] },
  brute:    { team: 'ai', row: ROW.skeleton, hp: 1500, dmg: 110, cd: 1.6, range: 16, speed: 20, sight: 60, towersOnly: true, scale: 1.6, tint: [0.72, 1, 0.68, 1] },
  king:     { team: 'ai', row: ROW.reaper, hp: 2600, dmg: 170, cd: 1.8, range: 20, speed: 16, sight: 70, towersOnly: true, scale: 2.0, tint: [1, 0.62, 0.62, 1], light: [1, 0.25, 0.25] },
}

// Your deck — cards.png order (levels come from the Dungeon Royale shop).
const CARDS = [
  { key: 'knight', cost: 3, type: 'troop', spawn: [['knight', 0, 0]] },
  { key: 'archers', cost: 3, type: 'troop', spawn: [['archer', -7, 0], ['archer', 7, 0]] },
  { key: 'captain', cost: 5, type: 'troop', spawn: [['captain', 0, 0], ['squire', -11, -5], ['squire', 11, -5], ['squire', 0, -11]] },
  { key: 'fireball', cost: 4, type: 'spell', radius: 32, dmg: 360 },
  { key: 'cleric', cost: 3, type: 'troop', spawn: [['cleric', 0, 0]] },
  { key: 'spikes', cost: 2, type: 'build', radius: 14 },
  { key: 'brazier', cost: 4, type: 'build', radius: 92 },
  { key: 'rogue', cost: 2, type: 'troop', spawn: [['rogue', 0, 0]] },
]
const cardLevel = (card) => Math.min(10, Math.max(1, (royale.levels && royale.levels[card.key]) || 1))
const cardPower = (card) => 1 + 0.12 * (cardLevel(card) - 1)

// The AI's deck. `role` drives its counter-picking.
const AI_DECK = [
  { name: 'SKELETONS', cost: 3, spawn: ['skeleton', 'skeleton', 'skeleton', 'skeleton'], role: 'swarm' },
  { name: 'WISPS', cost: 2, spawn: ['wisp', 'wisp'], role: 'chip' },
  { name: 'REAPER', cost: 4, spawn: ['reaper'], role: 'dps' },
  { name: 'NECROMANCER', cost: 4, spawn: ['necro'], role: 'ranged' },
  { name: 'BRUTE', cost: 6, spawn: ['brute'], role: 'tank' },
  { name: 'HEX STORM', cost: 4, spell: true, radius: 34, dmg: 300, role: 'spell' },
  { name: 'BONE KING', cost: 8, spawn: ['king'], role: 'tank', late: true },
]

const MATCH_TIME = 120
const ELIXIR_MAX = 10
const RIVER_Y = MAP_TOP - 12 * TILE   // the chasm's center line (y = 64)
const BRIDGE_X = 80                   // bridges at x = -80 and +80
const DEPLOY_MAX_Y = RIVER_Y - 18     // you deploy below the chasm
const TRAY_TOP = -128
const CARD_Y = -204
const CARD_X = (i) => -72 + i * 46
const BAR_LEFT = 14 - 100, BAR_W = 200
const TOWERS = [
  // Your side (bottom) and the AI's (top) — mirrored across the chasm.
  { team: 'you', kind: 'king', x: 0, y: RIVER_Y - 136, lane: 0 },
  { team: 'you', kind: 'princess', x: -BRIDGE_X, y: RIVER_Y - 104, lane: -1 },
  { team: 'you', kind: 'princess', x: BRIDGE_X, y: RIVER_Y - 104, lane: 1 },
  { team: 'ai', kind: 'king', x: 0, y: RIVER_Y + 136, lane: 0 },
  { team: 'ai', kind: 'princess', x: -BRIDGE_X, y: RIVER_Y + 104, lane: -1 },
  { team: 'ai', kind: 'princess', x: BRIDGE_X, y: RIVER_Y + 104, lane: 1 },
]
const TOWER_STATS = { king: { hp: 2600, dmg: 70, cd: 1.0, range: 112, scale: 0.85 }, princess: { hp: 1500, dmg: 52, cd: 0.8, range: 104, scale: 0.6 } }

// Tileset indices (same pack as Dungeon Royale).
const T_FLOOR = 23, T_FLOOR_ALT = [22, 6, 16, 26, 17], T_BRIDGE = 26
const T_FACE = 2, T_FACE_ALT = 3, T_CAP = 42, T_SIDE_L = 10, T_SIDE_R = 15
const T_CORNER_TL = 0, T_CORNER_TR = 5, T_CORNER_BL = 40, T_CORNER_BR = 45

// ----------------------------------------------------------- helpers --

const rand = (n) => Math.floor(Math.random() * n)
const pick = (arr) => arr[rand(arr.length)]
const dist = (ax, ay, bx, by) => Math.hypot(ax - bx, ay - by)
const ref = (id) => {
  let r = g.refs[id]
  if (r === undefined) { r = getCompByID(id); g.refs[id] = r }
  return r
}
const img = (name) => g.imgs[name]
const unitFrame = (row, f) => img('unit ' + (row * 4 + f))
const itemFrame = (row, f) => img('item ' + (row * 4 + f))
const cellX = (x) => Math.floor((x - MAP_LEFT) / TILE)
const cellY = (y) => Math.floor((MAP_TOP - y) / TILE)
const centerX = (cx) => MAP_LEFT + cx * TILE + TILE / 2
const centerY = (cy) => MAP_TOP - cy * TILE - TILE / 2
const inMap = (cx, cy) => cx >= 0 && cy >= 0 && cx < MAP_W && cy < MAP_H
const walkable = (cx, cy) => inMap(cx, cy) && g.grid[cy * MAP_W + cx] === 0
const walkableAt = (x, y) => walkable(cellX(x), cellY(y))
const sideOf = (y) => (y > RIVER_Y ? 'ai' : 'you')
const foeTeam = (team) => (team === 'you' ? 'ai' : 'you')
function setText(id, text) { const r = ref(id); if (r && r.textContent !== text) r.textContent = text }
function message(main, sub, time) { setText('msg', main); setText('subMsg', sub || ''); g.msgT = time || 0 }

// ------------------------------------------------------- the arena --

function buildArena() {
  // 0 floor, 1 wall, 3 chasm (walls dressed as a gap), 2 tower.
  g.grid = new Array(MAP_W * MAP_H).fill(0)
  for (let y = 0; y < MAP_H; y++) for (let x = 0; x < MAP_W; x++) {
    const edge = x === 0 || x === MAP_W - 1 || y <= 1 || y === MAP_H - 1
    const chasm = (y === 11 || y === 12) && !(x === 3 || x === 4 || x === 13 || x === 14)
    if (edge) g.grid[y * MAP_W + x] = 1
    else if (chasm) g.grid[y * MAP_W + x] = 3
  }
  const tiles = new Array(MAP_W * MAP_H).fill(-1)
  const decor = new Array(MAP_W * MAP_H).fill(-1)
  const isFloor = (x, y) => inMap(x, y) && g.grid[y * MAP_W + x] === 0
  const faces = []
  for (let y = 0; y < MAP_H; y++) for (let x = 0; x < MAP_W; x++) {
    const i = y * MAP_W + x
    if (isFloor(x, y)) {
      const bridge = y === 11 || y === 12
      tiles[i] = bridge ? T_BRIDGE : (x * 7 + y * 13) % 9 === 0 ? pick(T_FLOOR_ALT) : T_FLOOR
      continue
    }
    const s = isFloor(x, y + 1), n = isFloor(x, y - 1), e = isFloor(x + 1, y), w = isFloor(x - 1, y)
    if (s) { tiles[i] = (x * 7 + y * 3) % 5 === 0 ? T_FACE_ALT : T_FACE; if (g.grid[i] === 1) faces.push([x, y]) }
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
  // Torches on the side walls, evenly spaced, and at the chasm's ends.
  g.torches = []
  for (const cy of [3, 7, 16, 20]) for (const cx of [0, MAP_W - 1]) g.torches.push({ x: centerX(cx) + (cx === 0 ? 10 : -10), y: centerY(cy), phase: Math.random() * 10 })
  for (const cx of [2, 15]) g.torches.push({ x: centerX(cx), y: centerY(11) + 2, phase: Math.random() * 10 })
  for (let i = 0; i < POOL.torches; i++) {
    const r = ref('torch' + i), t = g.torches[i]
    if (t) { r.x = Math.round(t.x); r.y = Math.round(t.y); r.visible = true } else { r.visible = false; r.x = 100000 }
  }
  // The Dungeon keep and its portals aren't part of this arena.
  for (const id of ['castle', 'castleHp', 'castleHpBack', 'banner0', 'banner1']) ref(id).visible = false
  ref('castle').x = 100000
  for (let i = 0; i < POOL.portals; i++) ref('portal' + i).visible = false
}

function spawnTowers() {
  for (const t of g.towers || []) if (t.ref) t.ref.destroy()
  g.towers = TOWERS.map((def) => {
    const S = TOWER_STATS[def.kind]
    const r = ctx.spawn('Castle', def.x, def.y - 14, g.world)
    if (r) { r.scaleX = S.scale; r.scaleY = S.scale }
    const fill = r && r.part('HP')
    if (fill) fill.fillColor = def.team === 'you' ? [0.45, 0.95, 0.5, 1] : [1, 0.38, 0.38, 1]
    // Towers block the floor cells under their footprint.
    const w = def.kind === 'king' ? 14 : 10
    const seen = new Set()
    const cells = []
    for (let px = def.x - w + 1; px < def.x + w; px += 6) for (let py = def.y - 12; py < def.y + 12; py += 6) {
      const cx = cellX(px), cy = cellY(py), key = cy * MAP_W + cx
      if (seen.has(key) || !inMap(cx, cy) || g.grid[key] !== 0) continue
      seen.add(key); cells.push([cx, cy]); g.grid[key] = 2
    }
    return {
      ...def, isTower: true, alive: true, hp: S.hp, maxHp: S.hp, dmg: S.dmg, cdMax: S.cd, cd: 1, range: S.range,
      active: def.kind !== 'king', hitFlash: 0, shown: -1, ref: r, fill, cells,
      // Aim point: the tower's middle.
      y: def.y + 10,
    }
  })
}

const towersOf = (team) => g.towers.filter((t) => t.alive && t.team === team)
const kingOf = (team) => g.towers.find((t) => t.team === team && t.kind === 'king')
const crowns = (team) => g.towers.filter((t) => t.team === foeTeam(team) && !t.alive).reduce((n, t) => n + (t.kind === 'king' ? 3 : 1), 0)
const towerHealth = (team) => g.towers.filter((t) => t.team === team).reduce((n, t) => n + Math.max(0, t.hp) / t.maxHp, 0)

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

/** Spawns a unit from the "Unit" prefab. `power`: card level / AI strength. */
function spawnUnit(kind, x, y, power = 1) {
  if (g.units.length >= POOL.units) return null
  const K = KINDS[kind]
  const ref0 = ctx.spawn('Unit', x, y, g.world)
  if (!ref0) return null
  const u = {
    i: g.serial++, ref: ref0, bar: ref0.part('HP Back'), fill: ref0.part('HP'),
    alive: true, kind, K, team: K.team, hp: K.hp * power, maxHp: K.hp * power, power, x, y,
    target: null, cd: 0.4 + Math.random() * 0.3, face: K.team === 'you' ? 1 : -1, frame: -1, animT: Math.random(),
    lunge: 0, hitFlash: 0, landT: 0.25, leader: null, slot: 0, light: null, shownHp: -1, lastX: x, lastY: y, slowT: 0,
  }
  g.units.push(u)
  const s = K.scale || 1
  ref0.scaleX = s; ref0.scaleY = s
  ref0.fillColor = K.tint || [1, 1, 1, 1]
  u.bar.visible = false; u.fill.visible = false
  u.fill.fillColor = u.team === 'you' ? [0.45, 0.95, 0.5, 1] : [1, 0.38, 0.38, 1]
  if (K.light) u.light = takeLight(K.light, kind === 'king' ? 70 : 40, 1.0)
  return u
}

function killUnit(u) {
  u.alive = false
  u.ref.destroy()
  g.units = g.units.filter((o) => o !== u)
  if (u.light) { u.light.used = false; u.light.ref.visible = false; u.light = null }
  fx('Poof', u.x, u.y + 6, u.K.scale ? 26 : 14)
}

function damage(t, amount, from) {
  if (!t.alive) return
  t.hp -= amount
  t.hitFlash = 0.12
  if (t.isTower) {
    if (t.kind === 'king') t.active = true  // a struck King tower wakes up
    fx('Hit', t.x + (Math.random() - 0.5) * 18, t.y + (Math.random() - 0.5) * 10, 5)
    if (t.hp <= 0) destroyTower(t)
    return
  }
  fx('Hit', t.x, t.y + 6, 5)
  if (t.isBuilding) return
  if (from && !t.target && !t.K.towersOnly) t.target = from
  if (t.hp <= 0) killUnit(t)
}

function destroyTower(t) {
  t.alive = false
  t.hp = 0
  if (t.ref) t.ref.visible = false
  for (const [cx, cy] of t.cells) if (inMap(cx, cy)) g.grid[cy * MAP_W + cx] = 0
  fx('Boom', t.x, t.y, 80)
  flash(t.x, t.y, [1, 0.5, 0.25], 2.4, 110)
  g.shake = 0.45
  // Losing a Princess tower wakes that side's King.
  const king = kingOf(t.team)
  if (king) king.active = true
  if (t.kind === 'king') return
  message(t.team === 'ai' ? 'CROWN!' : 'TOWER LOST', t.team === 'ai' ? 'YOU TOOK A TOWER' : 'DEFEND YOUR KING', 1.6)
}

/** Everything a `team` unit may attack: enemy units, buildings, towers. */
function targetsFor(team) {
  const out = []
  for (const u of g.units) if (u.alive && u.team !== team) out.push(u)
  if (team === 'ai') for (const b of g.buildings) if (b.alive) out.push(b)
  for (const t of g.towers) if (t.alive && t.team !== team) out.push(t)
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

const reach = (t) => (t.isTower ? (t.kind === 'king' ? 22 : 16) : t.isBuilding ? 8 : 5)

/** The tower a unit marches on: its lane's Princess, else the King. */
function laneTower(u) {
  const foe = foeTeam(u.team)
  const lane = u.x < 0 ? -1 : 1
  const princess = g.towers.find((t) => t.alive && t.team === foe && t.lane === lane)
  return princess || g.towers.find((t) => t.alive && t.team === foe && t.kind === 'king') || nearest(towersOf(foe), u.x, u.y, 9999, null)
}

function think(u) {
  const K = u.K
  if (u.target && !u.target.alive) u.target = null
  if (K.healer) { u.target = null; return }
  const foes = targetsFor(u.team)
  if (K.towersOnly) { u.target = (u.target && u.target.isTower && u.target.alive) ? u.target : laneTower(u); return }
  if (K.follower && u.leader && u.leader.alive) {
    u.target = nearest(foes, u.leader.x, u.leader.y, K.sight, (t) => !t.isTower) || u.leader.target
    return
  }
  if (K.assassin) {
    const prey = nearest(foes, u.x, u.y, K.sight, (t) => t.K && t.K.ranged)
    if (prey) { u.target = prey; return }
  }
  // Fight what's near (a unit beats a tower); otherwise march on the lane.
  const near = nearest(foes, u.x, u.y, K.sight, (t) => !t.isTower)
  if (near) { u.target = near; return }
  const tower = nearest(foes, u.x, u.y, K.range + 40, (t) => t.isTower)
  u.target = tower || laneTower(u)
}

/** Where to walk: bridges first when the target is across the chasm. */
function waypoint(u, tx, ty) {
  if (u.K.flying) return [tx, ty]
  const mine = sideOf(u.y), theirs = sideOf(ty)
  const onBridge = Math.abs(Math.abs(u.x) - BRIDGE_X) < 16 && Math.abs(u.y - RIVER_Y) < 24
  if (mine === theirs) return [tx, ty]
  // Use the bridge on the target's lane, unless the other is much closer.
  let bx = tx < 0 ? -BRIDGE_X : BRIDGE_X
  if (Math.abs(u.x - -bx) + 40 < Math.abs(u.x - bx)) bx = -bx
  const dir = mine === 'you' ? 1 : -1
  if (onBridge && Math.abs(u.x - bx) < 16) return [bx, RIVER_Y + dir * 30]
  return [bx, RIVER_Y - dir * 22]
}

function destination(u) {
  const K = u.K
  if (K.healer) {
    let best = null, bestScore = -1
    for (const a of g.units) {
      if (!a.alive || a === u || a.team !== u.team) continue
      const score = (1 - a.hp / a.maxHp) * 200 - dist(u.x, u.y, a.x, a.y) * 0.5 + 60
      if (score > bestScore) { bestScore = score; best = a }
    }
    if (!best) return null
    return dist(u.x, u.y, best.x, best.y) > 22 ? [best.x, best.y] : null
  }
  if (K.follower && u.leader && u.leader.alive && !(u.target && !u.target.isTower && dist(u.x, u.y, u.target.x, u.target.y) < K.sight)) {
    const angle = Math.PI + (u.slot - 1) * 0.9
    const fx0 = u.leader.x + Math.cos(angle) * 14 * u.leader.face, fy0 = u.leader.y + Math.sin(angle) * 10 - 4
    return dist(u.x, u.y, fx0, fy0) > 4 ? [fx0, fy0] : null
  }
  const t = u.target
  if (!t) return null
  if (dist(u.x, u.y, t.x, t.y) <= K.range + reach(t)) return null
  return [t.x, t.y]
}

function move(u) {
  const K = u.K
  const dest = destination(u)
  let ax = 0, ay = 0
  if (dest) {
    const [wx, wy] = waypoint(u, dest[0], dest[1])
    const d = dist(u.x, u.y, wx, wy) || 1
    ax = (wx - u.x) / d; ay = (wy - u.y) / d
  }
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
  u.slowT = Math.max(0, u.slowT - dt)
  const sp = K.speed * (u.slowT > 0 ? 0.45 : 1)
  if (len > 0.05) {
    const nx = u.x + ax / Math.max(1, len) * sp * dt
    const ny = u.y + ay / Math.max(1, len) * sp * dt
    if (K.flying || walkableAt(nx, u.y)) u.x = nx
    if (K.flying || walkableAt(u.x, ny)) u.y = ny
    if (Math.abs(ax) > 0.2) u.face = ax > 0 ? 1 : -1
  }
  u.moving = dist(u.x, u.y, u.lastX, u.lastY) > 0.05
  u.lastX = u.x; u.lastY = u.y
}

function act(u) {
  const K = u.K
  u.cd -= dt
  if (K.healer) {
    if (u.cd > 0) return
    let best = null, need = 0
    for (const a of g.units) {
      if (!a.alive || a.team !== u.team || a.hp >= a.maxHp || dist(u.x, u.y, a.x, a.y) > K.range) continue
      if (a.maxHp - a.hp > need) { need = a.maxHp - a.hp; best = a }
    }
    if (best) { best.hp = Math.min(best.maxHp, best.hp + K.heal * u.power); fx('Heal', best.x, best.y + 6, 4); u.cd = K.cd }
    return
  }
  const t = u.target
  if (!t || !t.alive) return
  if (dist(u.x, u.y, t.x, t.y) > K.range + reach(t) + 2) return
  u.face = t.x >= u.x ? 1 : -1
  if (u.cd > 0) return
  u.cd = K.cd * (0.9 + Math.random() * 0.2)
  const dmg = K.dmg * u.power
  if (K.ranged) shoot(K.ranged, u.x, u.y + 8, t, dmg, u)
  else { u.lunge = 0.15; damage(t, dmg, u) }
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
  if (u.hitFlash > 0) u.hitFlash -= dt
  const tintKey = u.hitFlash > 0 ? 'hit' : u.slowT > 0 ? 'slow' : 'base'
  if (tintKey !== u.tintKey) {
    u.tintKey = tintKey
    u.ref.fillColor = tintKey === 'hit' ? [1, 0.45, 0.45, 1] : tintKey === 'slow' ? [0.55, 0.78, 1, 1] : K.tint || [1, 1, 1, 1]
  }
  const frac = Math.max(0, u.hp / u.maxHp)
  if (frac !== u.shownHp) {
    u.shownHp = frac
    const show = frac < 0.999
    u.bar.visible = show; u.fill.visible = show
    u.fill.scaleX = Math.max(0.01, frac)
  }
  if (u.light) {
    u.light.ref.x = u.ref.x
    u.light.ref.y = u.ref.y + 8
    u.light.ref.lightIntensity = 0.9 + Math.sin(T * 6 + u.i) * 0.08
  }
}

// ------------------------------------------- projectiles, fx, towers --

function shoot(kind, x, y, target, dmg, from) {
  const ref0 = g.projectiles.length < 40 ? ctx.spawn(kind === 'arrow' ? 'Arrow' : 'Bolt', Math.round(x), Math.round(y)) : null
  if (!ref0) { damage(target, dmg, from); return }
  g.projectiles.push({ alive: true, kind, x, y, target, dmg, from, speed: kind === 'arrow' ? 200 : 125, ref: ref0 })
  if (kind !== 'arrow') ref0.fillColor = kind === 'hex' ? [0.8, 0.4, 1, 1] : [1, 0.62, 0.25, 1]
}

function stepProjectiles() {
  for (const p of g.projectiles) {
    const t = p.target
    const tx = t.x, ty = t.y + (t.isTower ? 0 : 7)
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
    if (p.kind === 'arrow') p.ref.rotation = Math.atan2(ty - p.y, tx - p.x) * 180 / Math.PI + 90
  }
  if (g.projectiles.some((p) => !p.alive)) g.projectiles = g.projectiles.filter((p) => p.alive)
}

function fx(name, x, y, n) {
  const pool = g.fx[name]
  const r = pool[g.fxNext[name] % pool.length]
  g.fxNext[name]++
  r.x = Math.round(x); r.y = Math.round(y)
  r.emit(n)
}

function flash(x, y, color, intensity, radius) {
  const f = g.flashes[g.flashNext++ % g.flashes.length]
  f.t = 0.35
  f.ref.x = Math.round(x); f.ref.y = Math.round(y)
  f.ref.fillColor = [color[0], color[1], color[2], 1]
  f.ref.lightRadius = radius
  f.ref.lightIntensity = intensity
  f.ref.visible = true
  f.peak = intensity
}

function stepFlashes() {
  for (const f of g.flashes) {
    if (f.t <= 0) continue
    f.t -= dt
    f.ref.lightIntensity = Math.max(0, f.peak * (f.t / 0.35))
    if (f.t <= 0) f.ref.visible = false
  }
}

function stepTowers() {
  for (const t of g.towers) {
    if (!t.alive) continue
    t.cd -= dt
    if (t.active && t.cd <= 0) {
      const target = nearest(targetsFor(t.team), t.x, t.y, t.range, (o) => !o.isTower && !o.isBuilding)
      if (target) { shoot('arrow', t.x + (target.x < t.x ? -8 : 8), t.y + 8, target, t.dmg, null); t.cd = t.cdMax }
    }
    const frac = Math.max(0, t.hp / t.maxHp)
    if (frac !== t.shown && t.fill) {
      t.shown = frac
      // Drains from the left: shrink, and shift so the left edge stays put.
      t.fill.scaleX = Math.max(0.01, frac)
      t.fill.x = -19 * (1 - frac)
      t.fill.fillColor = t.team === 'you' ? [0.45, 0.95, 0.5, 1] : [1, 0.38, 0.38, 1]
    }
    if (t.hitFlash > 0) t.hitFlash -= dt
    if (t.ref) {
      const base = t.team === 'ai' ? [1, 0.62, 0.62, 1] : [1, 1, 1, 1]
      t.ref.fillColor = t.hitFlash > 0 ? [1, 0.55, 0.55, 1] : t.active ? base : [base[0] * 0.75, base[1] * 0.75, base[2] * 0.75, 1]
      t.ref.lightIntensity = (t.active ? 1.2 : 0.6) + Math.sin(T * 3 + t.x) * 0.06
    }
  }
}

function stepBuildings() {
  for (const b of g.buildings) {
    if (!b.alive) continue
    b.life -= dt
    if (b.kind === 'spikes') {
      b.cd -= dt
      for (const u of g.units) if (u.alive && u.team === 'ai' && !u.K.flying && dist(u.x, u.y, b.x, b.y) < 16) u.slowT = 0.8
      if (b.cd <= 0) {
        for (const u of g.units) if (u.alive && u.team === 'ai' && !u.K.flying && dist(u.x, u.y, b.x, b.y) < 14) damage(u, 30 * b.power, null)
        b.cd = 0.5
      }
      b.ref.image = itemFrame(5, Math.floor(T * 4) % 2 ? 0 : 3)
    } else {
      b.cd -= dt
      if (b.cd <= 0) {
        const t = nearest(targetsFor('you'), b.x, b.y, 92, (o) => !o.isTower)
        if (t) { shoot('fire', b.x, b.y + 12, t, 72 * b.power, null); b.cd = 0.7 }
      }
      b.ref.image = itemFrame(3, Math.floor(T * 8) % 4)
      b.ref.lightIntensity = 1.1 + Math.sin(T * 17 + b.x) * 0.08
    }
    if (b.life <= 0 || b.hp <= 0) { b.alive = false; b.ref.visible = false; b.ref.x = 100000; fx('Poof', b.x, b.y + 6, 14) }
  }
}

/** A thrown spell (yours: the fireball; the AI's: a hex storm). */
function stepSpells() {
  for (const sp of g.spells) {
    sp.t += dt / 0.6
    const k = Math.min(1, sp.t)
    const x = sp.sx + (sp.x - sp.sx) * k, y = sp.sy + (sp.y - sp.sy) * k + Math.sin(k * Math.PI) * 40
    sp.ref.x = Math.round(x); sp.ref.y = Math.round(y)
    fx('Trail', x, y, 2)
    if (k < 1) continue
    sp.done = true
    for (const u of g.units) if (u.alive && u.team !== sp.team && dist(u.x, u.y + 6, sp.x, sp.y) < sp.radius) damage(u, sp.dmg, null)
    for (const t of g.towers) if (t.alive && t.team !== sp.team && dist(t.x, t.y, sp.x, sp.y) < sp.radius + 18) damage(t, sp.dmg * 0.35, null)
    fx('Boom', sp.x, sp.y, 60)
    flash(sp.x, sp.y, sp.team === 'you' ? [1, 0.6, 0.25] : [0.75, 0.35, 1], 2.2, 90)
    g.shake = 0.3
    if (sp.own) { sp.ref.visible = false; sp.ref.x = 100000 } else sp.ref.destroy()
  }
  g.spells = g.spells.filter((s) => !s.done)
}

function castSpell(team, x, y, radius, dmg) {
  const king = kingOf(team)
  let r0
  if (team === 'you') { r0 = ref('fireball'); r0.visible = true }
  else { r0 = ctx.spawn('Bolt', king.x, king.y); if (r0) { r0.scaleX = 2; r0.scaleY = 2; r0.fillColor = [0.8, 0.4, 1, 1] } }
  if (!r0) return
  g.spells.push({ team, t: 0, sx: king.x, sy: king.y + 16, x, y, radius, dmg, ref: r0, own: team === 'you' })
}

// ------------------------------------------------- the AI opponent --
//
// Utility AI: list every (card, move) it could make right now, score each
// one from the board, play the best if it beats the "save elixir" bar.
// Re-decides every ~0.3 s, and waits a human-ish reaction time before
// acting on a new threat.

const aiPower = () => 1.08 + Math.min(0.5, (arenaSave.trophies || 0) / 1200)

function aiThink() {
  const ai = g.ai
  ai.cd -= dt
  if (ai.cd > 0) return
  ai.cd = 0.3

  // Read the board.
  const invaders = g.units.filter((u) => u.alive && u.team === 'you' && u.y > RIVER_Y - 30)
  const myArmy = g.units.filter((u) => u.alive && u.team === 'ai')
  const laneThreat = { '-1': 0, '1': 0 }
  for (const u of invaders) laneThreat[u.x < 0 ? -1 : 1] += u.hp / 250 + u.K.dmg / u.K.cd / 40
  const threatLane = laneThreat['-1'] >= laneThreat['1'] ? -1 : 1
  const threat = laneThreat[threatLane]
  const playerLow = g.elixir < 3
  const deck = AI_DECK.filter((c) => !c.late || g.matchT <= 60)

  let best = null
  const consider = (card, score, x, y, intent) => {
    if (card.cost > ai.elixir) return
    if (!best || score > best.score) best = { card, score, x, y, intent }
  }

  for (const card of deck) {
    // 1) SPELL: a clump of your troops (or one at a tower) worth the elixir.
    if (card.spell) {
      let bestHit = 0, bx = 0, by = 0
      for (const u of g.units) {
        if (!u.alive || u.team !== 'you') continue
        let value = 0
        for (const o of g.units) if (o.alive && o.team === 'you' && dist(o.x, o.y, u.x, u.y) < card.radius) value += Math.min(o.hp, card.dmg) / 100
        if (value > bestHit) { bestHit = value; bx = u.x; by = u.y }
      }
      // Lead the target a little toward where it's walking.
      if (bestHit > 0) consider(card, bestHit * 1.2 - card.cost * 0.6, bx, by + 6, 'HEX STORM ON A CLUMP')
      // Finisher: a nearly dead tower of yours.
      for (const t of towersOf('you')) if (t.hp < card.dmg * 0.35 + 5) consider(card, 12, t.x, t.y, 'FINISHING A TOWER')
      continue
    }
    const kinds = card.spawn.map((k) => KINDS[k])
    // 2) DEFEND: something is on our half — counter it where towers help.
    if (threat > 0) {
      const heavy = invaders.some((u) => u.maxHp >= 500), many = invaders.length >= 3, ranged = invaders.some((u) => u.K.ranged)
      let counter = 1
      if (heavy && card.role === 'swarm') counter = 2.2         // swarm a tank
      if (heavy && card.role === 'dps') counter = 2.0           // big hitter on a tank
      if (many && card.role === 'ranged') counter = 1.9         // splash a swarm
      if (many && card.role === 'tank') counter = 1.5
      if (ranged && card.role === 'chip') counter = 1.4
      if (card.role === 'tank' && !many) counter = 0.6          // don't waste a tank on defense
      const near = invaders.reduce((a, u) => (Math.abs(u.x - threatLane * BRIDGE_X) < Math.abs(a.x - threatLane * BRIDGE_X) ? u : a), invaders[0])
      // In front of our tower in that lane, between it and the attacker.
      const tower = g.towers.find((t) => t.alive && t.team === 'ai' && t.lane === threatLane) || kingOf('ai')
      const x = (tower.x * 2 + near.x) / 3, y = Math.max(RIVER_Y + 24, Math.min(tower.y - 18, near.y + 30))
      const urgency = threat * (near.y < RIVER_Y + 70 ? 1 : 1.6)   // deeper = more urgent
      consider(card, urgency * counter - card.cost * 0.25 + (kinds.some((K) => K.ranged) ? 0.3 : 0), x, y, 'DEFENDING ' + (threatLane < 0 ? 'LEFT' : 'RIGHT'))
    }
    // 3) PUSH: build an attack — behind the King for a slow, stacked push,
    //    or straight at the bridge when you're out of elixir.
    const weakLane = (() => {
      const l = g.towers.find((t) => t.team === 'you' && t.lane === -1), r = g.towers.find((t) => t.team === 'you' && t.lane === 1)
      const lh = l && l.alive ? l.hp : 0, rh = r && r.alive ? r.hp : 0
      return lh <= rh ? -1 : 1
    })()
    const tankUp = myArmy.find((u) => u.K.towersOnly && u.y < RIVER_Y + 40)   // a tank already crossing
    if (card.role === 'tank' && ai.elixir >= Math.min(10, card.cost + 2) && !tankUp) {
      consider(card, 2.2 + (playerLow ? 1.5 : 0), weakLane * BRIDGE_X * 0.6, RIVER_Y + 128, 'BUILDING A PUSH ' + (weakLane < 0 ? 'LEFT' : 'RIGHT'))
    }
    if (tankUp && card.role !== 'tank') {
      // Support behind the tank.
      consider(card, 2.4 + (card.role === 'ranged' ? 0.6 : 0), tankUp.x, Math.max(RIVER_Y + 26, tankUp.y + 26), 'SUPPORTING THE PUSH')
    }
    if (playerLow && (card.role === 'dps' || card.role === 'swarm' || card.role === 'chip')) {
      consider(card, 2.0, weakLane * BRIDGE_X, RIVER_Y + 26, 'PUNISHING - YOU ARE LOW')
    }
    // Elixir would overflow: play something cheap rather than waste it.
    if (ai.elixir >= ELIXIR_MAX - 0.3) consider(card, 1.2 - card.cost * 0.1, weakLane * BRIDGE_X, RIVER_Y + 60, 'SPENDING SPARE ELIXIR')
  }

  const SAVE_BAR = 1.0
  if (!best || best.score < SAVE_BAR) {
    ai.intent = 'SAVING ELIXIR ' + Math.floor(ai.elixir) + '/10'
    ai.pending = null
    return
  }
  // A new kind of decision waits a reaction time; the same one goes now.
  if (!ai.pending || ai.pending.intent !== best.intent) { ai.pending = best; ai.reactT = 0.35 + Math.random() * 0.4; ai.intent = best.intent; return }
  ai.reactT -= 0.3
  if (ai.reactT > 0) return
  aiPlay(best)
  ai.pending = null
  ai.cd = 0.8
}

function aiPlay(choice) {
  const card = choice.card
  g.ai.elixir -= card.cost
  g.ai.intent = choice.intent + ': ' + card.name
  if (card.spell) { castSpell('ai', choice.x, choice.y, card.radius, card.dmg * aiPower()); return }
  // Clamp onto the AI's own half, on open floor.
  let x = Math.max(MAP_LEFT + 24, Math.min(-MAP_LEFT - 24, choice.x)), y = Math.max(RIVER_Y + 24, choice.y)
  if (!walkableAt(x, y)) { x = choice.x < 0 ? -BRIDGE_X : BRIDGE_X; y = RIVER_Y + 40 }
  const n = card.spawn.length
  card.spawn.forEach((kind, k) => {
    const a = (k / n) * Math.PI * 2
    const u = spawnUnit(kind, x + (n > 1 ? Math.cos(a) * 8 : 0), y + (n > 1 ? Math.sin(a) * 6 : 0), aiPower())
    if (u) u.landT = 0.25
  })
  fx('Deploy', x, y + 4, 18)
  flash(x, y + 8, [0.75, 0.35, 1], 1.1, 44)
}

// ------------------------------------------------- your hand & cards --

function dealHand() {
  const order = CARDS.map((_, i) => i).sort(() => Math.random() - 0.5)
  g.hand = order.slice(0, 4)
  g.queue = order.slice(4)
  for (let i = 0; i < 4; i++) refreshSlot(i)
}

function refreshSlot(i) {
  ref('card' + i).image = img('card ' + g.hand[i])
  setText('cost' + i, String(CARDS[g.hand[i]].cost))
  ref('nextCard').image = img('card ' + g.queue[0])
}

function canDeploy(card, x, y) {
  if (y < TRAY_TOP + 2 || y > MAP_TOP || x < MAP_LEFT || x > -MAP_LEFT) return false
  if (card.type === 'spell') return true
  if (y > DEPLOY_MAX_Y || !walkableAt(x, y)) return false
  if (card.type === 'build') return !g.buildings.some((b) => b.alive && dist(b.x, b.y, x, y) < 12)
  return true
}

function deploy(card, x, y) {
  const power = cardPower(card)
  if (card.type === 'spell') { castSpell('you', x, y, card.radius, card.dmg * power); return }
  if (card.type === 'build') {
    const bx = centerX(cellX(x)), by = centerY(cellY(y)) - 4
    const pool = card.key === 'spikes' ? g.spikes : g.braziers
    const b = pool.find((q) => !q.alive) || pool.reduce((a, q) => (q.life < a.life ? q : a))
    Object.assign(b, { alive: true, kind: card.key, x: bx, y: by, life: card.key === 'spikes' ? 45 : 40, hp: 700 * power, maxHp: 700 * power, power, cd: 0.3, hitFlash: 0, isBuilding: true })
    b.ref.x = bx; b.ref.y = card.key === 'spikes' ? by + 4 : by
    b.ref.visible = true
    fx('Deploy', bx, by + 4, 16)
    return
  }
  let leader = null
  card.spawn.forEach(([kind, ox, oy], k) => {
    let px = x + ox, py = y + oy
    if (!walkableAt(px, py)) { px = x; py = y }
    const u = spawnUnit(kind, px, py, power)
    if (!u) return
    if (KINDS[kind].leader) leader = u
    else if (leader) { u.leader = leader; u.slot = k - 1 }
  })
  fx('Deploy', x, y + 4, 22)
  flash(x, y + 8, [0.6, 0.85, 1], 1.2, 48)
}

const elixirMult = () => (g.matchT > 60 ? 1 : g.matchT > 30 ? 2 : 3)

function stepHand() {
  const mx = ctx.mouseX, my = ctx.mouseY
  const down = keys.has('mouse')
  const pressed = down && !g.mouseWas
  const released = !down && g.mouseWas
  g.mouseWas = down
  const live = g.phase === 'battle'
  if (live) g.elixir = Math.min(ELIXIR_MAX, g.elixir + (elixirMult() / 2.8) * dt)

  if (pressed && live) {
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
    if (over && card.type !== 'spell') {
      const kind = card.spawn ? card.spawn[0][0] : null
      ghost.image = kind ? unitFrame(KINDS[kind].row, Math.floor(T * 6) % 4) : itemFrame(card.key === 'spikes' ? 5 : 3, 0)
      ghost.x = Math.round(mx); ghost.y = Math.round(my)
      ghost.fillColor = ok ? [1, 1, 1, 0.7] : [1, 0.4, 0.4, 0.55]
      ghost.visible = true
    } else ghost.visible = false
    if (over) {
      const r = (card.radius || 10) / 12
      ring.x = Math.round(mx); ring.y = Math.round(my)
      ring.scaleX = r; ring.scaleY = r * (card.type === 'spell' ? 1 : 0.6)
      ring.fillColor = ok ? [0.5, 1, 0.6, 0.12] : [1, 0.3, 0.3, 0.12]
      ring.visible = true
    } else ring.visible = false
    const cardRef = ref('card' + g.drag)
    cardRef.x = over ? CARD_X(g.drag) : Math.round(mx)
    cardRef.y = over ? CARD_Y + 8 : Math.round(Math.max(my, CARD_Y))
    if (released) {
      if (over && ok && g.elixir >= card.cost && live) {
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
    c.fillColor = [tint, tint, tint + (afford ? 0 : 0.08), 1]
  }
  const frac = g.elixir / ELIXIR_MAX
  const fill = ref('elxFill')
  fill.scaleX = Math.max(0.005, frac)
  fill.x = BAR_LEFT + (BAR_W * frac) / 2
  const mult = elixirMult()
  const p = 0.85 + Math.sin(T * (mult === 3 ? 12 : 8)) * 0.15
  fill.fillColor = mult === 1 ? [1, 1, 1, 1] : mult === 2 ? [1, p, 1, 1] : [1, p * 0.7, p, 1]
  setText('elxText', String(Math.floor(g.elixir)))
}

// ------------------------------------------------- flow & setup --

function restart() {
  for (const u of g.units) { u.ref.destroy(); if (u.light) { u.light.used = false; u.light.ref.visible = false } }
  g.units = []
  for (const p of g.projectiles) p.ref.destroy()
  g.projectiles = []
  for (const s of g.spells) { if (s.own) s.ref.visible = false; else s.ref.destroy() }
  g.spells = []
  for (const b of g.buildings) { b.alive = false; b.ref.visible = false; b.ref.x = 100000 }
  buildArena()
  spawnTowers()
  g.matchT = MATCH_TIME
  g.elixir = 5
  g.ai = { elixir: 5, cd: 2.5, intent: 'SIZING YOU UP', pending: null, reactT: 0 }
  g.drag = null
  g.result = ''
  g.sentOver = false
  dealHand()
}

function syncInputs() {
  const king = kingOf('you')
  const hp = Math.round(Math.max(0, king.hp / king.maxHp) * 100) / 100
  if (hp !== g.sentHp) { g.sentHp = hp; ctx.setInput('keepHp', hp) }
}

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
  g.units = []; g.projectiles = []; g.spells = []; g.towers = []
  g.serial = 0
  g.world = ref('world')
  g.lights = []; for (let i = 0; i < POOL.lights; i++) g.lights.push({ used: false, ref: ref('ul' + i) })
  g.flashes = []; for (let i = 0; i < POOL.flashes; i++) g.flashes.push({ t: 0, ref: ref('fl' + i) })
  g.flashNext = 0
  g.spikes = []; for (let i = 0; i < POOL.spikes; i++) g.spikes.push({ alive: false, life: 0, ref: ref('spike' + i) })
  g.braziers = []; for (let i = 0; i < POOL.braziers; i++) g.braziers.push({ alive: false, life: 0, ref: ref('braz' + i) })
  g.buildings = g.spikes.concat(g.braziers)
  g.fx = {
    Hit: [0, 1, 2, 3].map((i) => ref('fxHit' + i)), Poof: [0, 1, 2].map((i) => ref('fxPoof' + i)), Deploy: [0, 1].map((i) => ref('fxDeploy' + i)),
    Heal: [0, 1].map((i) => ref('fxHeal' + i)), Boom: [ref('fxBoom')], Trail: [ref('fxTrail')],
  }
  g.fxNext = { Hit: 0, Poof: 0, Deploy: 0, Heal: 0, Boom: 0, Trail: 0 }
  g.wiggle = [0, 0, 0, 0]
  g.dealT = [0, 0, 0, 0]
  g.mouseWas = keys.has('mouse')
  g.shake = 0
  g.phase = 'intro'
  ctx.setInput('over', false)
  restart()
}

function simulate() {
  for (const u of g.units) {
    if (!u.alive) continue
    if (((T * 6 + u.i * 0.37) | 0) !== u.thinkTick) { u.thinkTick = (T * 6 + u.i * 0.37) | 0; think(u) }
    if (u.landT > 0) continue
    move(u)
    act(u)
  }
  stepProjectiles()
  stepBuildings()
  stepTowers()
  stepSpells()
}

function drawWorld() {
  for (const u of g.units) if (u.alive) drawUnit(u)
  stepFlashes()
}

/** Torches, HUD labels (clock, crowns, AI intent), screen shake. */
function ambience(danger) {
  for (let i = 0; i < g.torches.length && i < POOL.torches; i++) {
    const r = ref('torch' + i), t = g.torches[i]
    r.image = itemFrame(1, Math.floor(T * (danger ? 12 : 8) + t.phase) % 4)
    r.lightIntensity = 1.3 + Math.sin(T * 13 + t.phase) * (danger ? 0.18 : 0.08)
  }
  if (g.msgT > 0) { g.msgT -= dt; if (g.msgT <= 0) message('', '', 0) }
  const secs = Math.ceil(g.matchT)
  setText('waveText', Math.floor(secs / 60) + ':' + String(secs % 60).padStart(2, '0'))
  setText('foesText', crowns('you') + ' - ' + crowns('ai'))
  setText('goldText', String(arenaSave.trophies || 0))
  setText('tipText', 'AI: ' + (g.ai.intent || '...'))
  for (let i = 0; i < 2; i++) ref('trayCandle' + i).image = itemFrame(3, Math.floor(T * 7 + i * 2) % 4)
  const cam = ref('camera')
  if (g.shake > 0) {
    g.shake -= dt
    cam.x = Math.round((Math.random() - 0.5) * 6 * g.shake / 0.3)
    cam.y = Math.round((Math.random() - 0.5) * 6 * g.shake / 0.3)
  } else if (cam.x !== 0 || cam.y !== 0) { cam.x = 0; cam.y = 0 }
}

/** End conditions: a King falls (3 crowns), or time runs out. */
function checkOver() {
  if (g.sentOver) return
  const youKing = kingOf('you'), aiKing = kingOf('ai')
  let result = ''
  if (!aiKing.alive) result = 'win'
  else if (!youKing.alive) result = 'lose'
  else if (g.matchT <= 0) {
    const a = crowns('you'), b = crowns('ai')
    if (a !== b) result = a > b ? 'win' : 'lose'
    else {
      const ha = towerHealth('you'), hb = towerHealth('ai')
      result = Math.abs(ha - hb) < 0.01 ? 'draw' : ha > hb ? 'win' : 'lose'
    }
  }
  if (!result) return
  g.result = result
  g.sentOver = true
  ctx.setInput('over', true)
}

// ===================================================================
//  One Code program per state (Rust prepends the library above).
// ===================================================================

//@program Intro
// Flow — fresh arena, a countdown, then `begin`.
if (entered()) {
  g.phase = 'intro'
  if (g.started) restart()
  g.started = true
  ctx.setInput('over', false)
  message('ARENA ROYALE', 'TAKE THE KING TOWER', 1.6)
  g.flowFired = false
}
if (ctx.time > 1.6 && ctx.time < 2.4) message('3', '', 0)
else if (ctx.time >= 2.4 && ctx.time < 3.0) message('2', '', 0)
else if (ctx.time >= 3.0 && ctx.time < 3.6) message('1', '', 0)
if (ctx.time > 3.6 && !g.flowFired) { g.flowFired = true; message('FIGHT!', '', 0.9); ctx.fireTrigger('begin') }
syncInputs()

//@program Battle
// Flow — the match clock (2x elixir in the last minute, 3x in the last
// 30 s), the AI's turn, and the end check.
if (entered()) { g.phase = 'battle'; g.lastMult = 1 }
const before = elixirMult()
const tick = Math.ceil(g.matchT)
g.matchT = Math.max(0, g.matchT - dt)
if (elixirMult() !== before) message(elixirMult() + 'X ELIXIR!', elixirMult() === 3 ? 'FINAL 30 SECONDS' : 'LAST MINUTE', 2)
else if (g.matchT > 0 && g.matchT <= 10 && Math.ceil(g.matchT) !== tick) message(String(Math.ceil(g.matchT)), '', 0.7)
g.ai.elixir = Math.min(ELIXIR_MAX, g.ai.elixir + (elixirMult() / 2.8) * dt)
aiThink()
checkOver()
syncInputs()

//@program Finish
// Flow — the result, trophies and gold saved (ctx.storage); tap to rematch.
if (entered()) {
  g.phase = 'over'
  const r = g.result
  const a = crowns('you'), b = crowns('ai')
  message(r === 'win' ? 'VICTORY!' : r === 'draw' ? 'DRAW' : 'DEFEAT', a + ' CROWNS TO ' + b, 0)
  const trophies = r === 'win' ? 30 : r === 'lose' ? -20 : 0
  const gold = r === 'win' ? 50 : r === 'draw' ? 20 : 10
  arenaSave.trophies = Math.max(0, (arenaSave.trophies || 0) + trophies)
  if (r === 'win') arenaSave.wins = (arenaSave.wins || 0) + 1
  else if (r === 'lose') arenaSave.losses = (arenaSave.losses || 0) + 1
  else arenaSave.draws = (arenaSave.draws || 0) + 1
  ctx.storage.set('arena', arenaSave)
  // Gold goes into the bank shared with Dungeon Royale's upgrade shop.
  royale.bank = (royale.bank || 0) + gold
  ctx.storage.set('royale', royale)
  g.ai.intent = r === 'win' ? 'GG - WELL PLAYED' : 'GG'
  // For the Game Over screen.
  royale.mode = 'arena'
  royale.result = r
  royale.gold = gold
  royale.crowns = a + ' - ' + b
  ctx.storage.set('royale', royale)
  g.flowFired = false
}
if (ctx.time > 2.4 && !g.flowFired) {
  g.flowFired = true
  // Leaving restores the scene's designed pose: start clean next visit.
  ctx.globals.arena = {}
  ctx.loadScene('Royale Game Over')
}

//@program Running
// Simulation layer — everyone thinks, moves, fights.
entered()
simulate()
drawWorld()

//@program Frozen
// Simulation layer — the match is over; the arena holds still.
entered()
drawWorld()

//@program Cards
// Hand layer — your elixir, cards, drag & drop.
entered()
stepHand()

//@program Calm
// Ambience layer — steady torches.
entered()
ambience(false)

//@program Danger
// Ambience layer — your King tower is in trouble.
if (entered()) message('', 'YOUR KING IS UNDER ATTACK!', 2)
ambience(true)
