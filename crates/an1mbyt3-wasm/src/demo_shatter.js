// Shatterline — a Smash Hit-style rail shooter (see `demo_shatter.rs`).
//
// The camera glides down an endless corridor (world −Z; `g.d` is how far
// along it is, so a thing at distance `d` sits at z = −d). Everything in
// view comes from pools the builder made — arches, wall blocks, glass
// panes, bars, fans, hangers, spinners, doors, crystals, balls — placed
// ahead in *patterns* and recycled once behind. The glass itself is the
// engine's breakable glass: `ctx.glassRaycast3` finds glass still
// standing along a ray (holes let rays through) and `ref.shatter3` breaks
// it where something hit, so this script never models fracture itself.
//
// Rules: start with 40 balls; each throw costs 1; crystals give +3; 10
// crystals in a row (without letting one pass) = multiball (up to 5 balls
// per throw); standing glass the camera flies into costs 10 and resets
// multiball. 0 balls = run over. Every ZONE_LEN metres a checkpoint door
// opens a new zone (palette, speed, obstacle mix); reached zones unlock on
// the menu.
//
// Prefix constants (from Rust): W/H (screen), HALF_WIDTH/HEIGHT (corridor),
// EYE, FOV, pool sizes, pane sizes, SHELL_LEN, ZONES (palettes).
const g = ctx.globals
// Refs are kept across frames (cheaper than rebuilding hundreds a frame);
// everything this script reads back from them (3D transforms) is live.
if (!g.R) g.R = {}
const ref = (id) => g.R[id] || (g.R[id] = getCompByID(id))
const keys = ctx.keysDown
const realDt = Math.min(ctx.deltaSeconds || 1 / 60, 1 / 20)

/** Metres per zone. */
const ZONE_LEN = 400
/** Balls at the start of every run. */
const START_BALLS = 40
const CRASH_COST = 10
const CRYSTAL_GAIN = 3
const STREAK_FOR_MULTI = 10
const MAX_MULTI = 5
/** Throw speed (m/s) and gravity (m/s²). */
const THROW_SPEED = 34
const GRAVITY = 9.8
const BALL_R = 0.13
const CRYSTAL_R = 0.42
/** How far ahead patterns get placed. */
const AHEAD = 110
const TAN_HALF = Math.tan((FOV * Math.PI) / 360)
const ASPECT = W / H
/** Camera collider: rays at these offsets (x, y) from the eye. */
const COLLIDER = [[0, 0], [0.22, 0], [-0.22, 0], [0, 0.2], [0, -0.2]]

const rand = (a, b) => a + Math.random() * (b - a)
const pick = (list) => list[Math.floor(Math.random() * list.length)]
const lerp3 = (a, b, t) => [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]
const norm = (x, y, z) => { const l = Math.hypot(x, y, z) || 1; return [x / l, y / l, z / l] }
const zoneOf = (d) => Math.max(0, Math.floor(d / ZONE_LEN))
const themeOf = (zone) => ZONES[zone % ZONES.length]

// ------------------------------------------------------------- setup --
if (!g.init) {
  g.init = true
  g.t = 0
  g.mode = 'menu' // 'menu' | 'play' | 'over'
  g.save = Object.assign({ unlocked: 0, best: 0 }, ctx.storage.get('shatterline') || {})
  g.free = {
    pb: Array.from({ length: BIG_PANES }, (_, i) => i),
    ps: Array.from({ length: SMALL_PANES }, (_, i) => i),
    sp: Array.from({ length: SPINNERS }, (_, i) => i),
    bar: Array.from({ length: BARS }, (_, i) => i),
    fan: Array.from({ length: FANS }, (_, i) => i),
    hang: Array.from({ length: HANGERS }, (_, i) => i),
    door: Array.from({ length: DOORS }, (_, i) => i),
    cry: Array.from({ length: CRYSTALS }, (_, i) => i),
    deco: Array.from({ length: DECOS }, (_, i) => i),
  }
  g.objs = []
  g.balls = []
  for (let i = 0; i < BALLS; i++) g.balls.push({ i, on: false, x: 0, y: 0, z: 0, vx: 0, vy: 0, vz: 0, age: 0 })
  g.archD = Array.from({ length: ARCHES }, (_, i) => i * ARCH_GAP)
  g.col = JSON.parse(JSON.stringify(ZONES[0]))
  g.colDirty = true
  g.pops = []
  g.popId = 0
  g.wasDown = false
  g.drone = ctx.playSound('drone', { loop: true, volume: 0.3 })
  startRun(0, true)
}

/** Starts a run (or the attract-mode drift behind the menu) at `zone`. */
function startRun(zone, attract) {
  for (const o of g.objs) release(o, true)
  g.objs = []
  for (const b of g.balls) if (b.on) hideBall(b)
  g.d = Math.max(0, zone * ZONE_LEN - 36)
  g.spawnD = g.d + (attract ? 26 : 48)
  g.zone = zoneOf(g.d + 40)
  g.lastZone = zoneOf(g.d)
  g.ammo = START_BALLS
  g.multi = 1
  g.streak = 0
  g.speed = attract ? 5 : 0
  g.invuln = 0
  g.shake = 0
  g.flash = 0
  g.slow = 0
  g.broken = 0
  g.runStart = g.d
  g.overT = 0
  g.attractT = 0
  g.playT = 0
  g.banner = null
  g.archD = Array.from({ length: ARCHES }, (_, i) => Math.floor(g.d / ARCH_GAP) * ARCH_GAP + i * ARCH_GAP)
  g.archDirty = true
  g.col = JSON.parse(JSON.stringify(themeOf(zoneOf(g.d))))
  g.colDirty = true
  if (!attract) {
    g.mode = 'play'
    showBanner(zone === 0 ? 'ZONE 1' : 'CHECKPOINT ' + zone, themeOf(zone).name.toUpperCase())
  }
}

// ------------------------------------------------------------- pools --
function take(pool) {
  const list = g.free[pool]
  return list.length ? list.shift() : -1
}

function giveBack(pool, i) {
  g.free[pool].push(i)
}

/** Ids of the glass nodes an object owns (to reset them). */
function glassOf(o) {
  switch (o.kind) {
    case 'pane': return [o.pool + o.i]
    case 'bar': return ['bar' + o.i]
    case 'fan': return Array.from({ length: BLADES }, (_, b) => 'fan' + o.i + 'b' + b)
    case 'hang': return ['hang' + o.i + 'g']
    case 'door': return ['door' + o.i + 't']
    default: return []
  }
}

/** The node an object shows / hides / moves. */
function rootOf(o) {
  switch (o.kind) {
    case 'pane': return o.pool + o.i + 'r'
    case 'bar': return 'bar' + o.i
    case 'fan': return 'fan' + o.i
    case 'hang': return 'hang' + o.i
    case 'door': return 'door' + o.i
    case 'cry': return 'cry' + o.i
    case 'deco': return 'deco' + o.i
  }
}

const POOL_OF = { bar: 'bar', fan: 'fan', hang: 'hang', door: 'door', cry: 'cry', deco: 'deco' }

function release(o, quiet) {
  const root = ref(rootOf(o))
  if (root) {
    root.visible = false
    root.position3 = { x: 0, y: -50, z: 0 }
  }
  for (const id of glassOf(o)) ref(id)?.resetGlass()
  giveBack(o.kind === 'pane' ? o.pool : POOL_OF[o.kind], o.i)
  if (o.kind === 'cry' && o.alive && !quiet && g.mode === 'play' && g.streak > 0) g.streak = 0
}

function place(o) {
  for (const id of glassOf(o)) ref(id)?.resetGlass()
  const root = ref(rootOf(o))
  root.visible = true
  g.objs.push(o)
  updateObj(o, g.t)
}

// ---------------------------------------------------------- patterns --
// Each places its objects from distance `d` and returns how long it is.
function crystal(d, x, y) {
  const i = take('cry')
  if (i < 0) return
  place({ kind: 'cry', i, d, x, y, alive: true, spin: rand(0, 360) })
}

function crystalsNear(d, count) {
  for (let k = 0; k < count; k++) {
    const side = Math.random() < 0.5 ? -1 : 1
    crystal(d + rand(-2, 4), side * rand(0.9, 3.0), rand(0.8, 4.4))
  }
}

function pane(pool, d, x, y, s, move) {
  const i = take(pool)
  if (i < 0) return false
  place({ kind: 'pane', pool, i, d, x, y, s, move, ph: rand(0, 6.28) })
  return true
}

function deco(d) {
  const i = take('deco')
  if (i < 0) return
  const side = Math.random() < 0.5 ? -1 : 1
  const w = rand(0.4, 1.4), h = rand(0.4, 2.6), depth = rand(0.6, 3)
  place({ kind: 'deco', i, d, x: side * (HALF_WIDTH - w * 0.5), y: Math.random() < 0.5 ? h * 0.5 : rand(1, HEIGHT - h * 0.5), w, h, depth })
}

const PATTERNS = {
  wall(d, k) {
    pane('pb', d, rand(-0.35, 0.35), EYE + rand(-0.15, 0.3), rand(1.0, 1.3))
    if (Math.random() < 0.35) pane('ps', d + 1.5, pick([-2.6, 2.6]), rand(1.2, 3.8), 0.9)
    crystalsNear(d - 6, 1 + (Math.random() < 0.4 ? 1 : 0))
    return 14
  },
  double(d, k) {
    const off = rand(0.3, 0.7) * pick([-1, 1])
    pane('pb', d, off, EYE + 0.1, 1.15)
    pane('pb', d + 6.5, -off, EYE + 0.1, 1.15)
    crystalsNear(d + 2, 2)
    return 20
  },
  bars(d, k) {
    for (let n = 0; n < 7; n++) {
      const i = take('bar')
      if (i < 0) break
      place({ kind: 'bar', i, d: d + (n % 2) * 0.06, x: (n - 3) * 0.55 })
    }
    crystalsNear(d - 5, 1)
    return 13
  },
  grid(d, k) {
    for (const [x, y] of [[-0.8, EYE - 0.8], [0.8, EYE - 0.8], [-0.8, EYE + 0.8], [0.8, EYE + 0.8]]) pane('ps', d, x, y, 1.0)
    crystalsNear(d - 5, 2)
    return 14
  },
  slider(d, k) {
    const w = 1.1 + k * 0.08
    pane('ps', d, 0, EYE, 1.15, { type: 'slide', amp: 1.7, w })
    pane('ps', d + 5, 0, EYE, 1.15, { type: 'slide', amp: -1.7, w })
    crystalsNear(d, 1)
    return 17
  },
  hanger(d, k) {
    for (let n = 0; n < 2; n++) {
      const i = take('hang')
      if (i < 0) break
      place({ kind: 'hang', i, d: d + n * 6, x: 0, amp: 38 + k * 2, w: 1.5 + k * 0.08, ph: n * Math.PI })
    }
    crystalsNear(d + 2, 1)
    return 18
  },
  fan(d, k) {
    const i = take('fan')
    if (i < 0) return PATTERNS.wall(d, k)
    const side = pick([-1, 1])
    place({ kind: 'fan', i, d, x: side * 2.35, y: EYE, w: (55 + k * 6) * -side, ph: rand(0, 90) })
    crystal(d - 4, -side * 2.4, 3.6)
    return 15
  },
  spinner(d, k) {
    pane('sp', d, 0, EYE + 0.15, 1.0, { type: 'spin', w: 70 + k * 8 })
    crystalsNear(d - 4, 1)
    return 13
  },
  breather(d, k) {
    crystalsNear(d + 3, 2 + Math.floor(Math.random() * 2))
    return 16
  },
}

/** Which patterns each zone mixes in (weights). */
function patternsFor(zone) {
  const z = Math.min(zone, 9)
  const w = { wall: 4, breather: z < 3 ? 1.5 : 0.8, bars: z >= 0 ? 1.2 : 0, double: z >= 1 ? 1.5 : 0.5, grid: z >= 1 ? 1.5 : 0, slider: z >= 2 ? 2 : 0, hanger: z >= 3 ? 2 : 0, fan: z >= 4 ? 2 : 0, spinner: z >= 5 ? 2 : 0 }
  if (z >= 6) for (const key of ['slider', 'hanger', 'fan', 'spinner', 'double']) w[key] += 1
  return w
}

function weighted(weights) {
  let total = 0
  for (const v of Object.values(weights)) total += v
  let r = Math.random() * total
  for (const [key, v] of Object.entries(weights)) if ((r -= v) <= 0) return key
  return 'wall'
}

function spawnAhead() {
  while (g.spawnD < g.d + AHEAD) {
    const zone = zoneOf(g.spawnD)
    const gate = (zone + 1) * ZONE_LEN
    // A checkpoint door opens each zone (none before the very first).
    if (g.spawnD + 22 > gate) {
      const i = take('door')
      if (i >= 0) place({ kind: 'door', i, d: gate, open: 0, opening: false, hit: false })
      crystalsNear(gate - 8, 2)
      g.spawnD = gate + 14
      continue
    }
    const k = Math.min(zone, 12)
    const length = PATTERNS[weighted(patternsFor(zone))](g.spawnD, k)
    if (Math.random() < 0.7) deco(g.spawnD + rand(0, length))
    const gap = Math.max(4.5, 11 - k * 0.6) + rand(0, 3)
    g.spawnD += length + gap
  }
}

// ------------------------------------------------------------ update --
/** Moves an object's nodes for time `t`. */
function updateObj(o, t) {
  const z = -o.d
  switch (o.kind) {
    case 'pane': {
      const root = ref(rootOf(o))
      let x = o.x, ry = 0
      if (o.move?.type === 'slide') x = o.x + Math.sin(t * o.move.w + o.ph) * o.move.amp
      if (o.move?.type === 'spin') ry = (t * o.move.w + o.ph * 57) % 360
      root.position3 = { x, y: o.y, z }
      root.rotation3 = { x: 0, y: ry, z: 0 }
      root.scale3 = { x: o.s, y: o.s, z: 1 }
      break
    }
    case 'bar':
      ref(rootOf(o)).position3 = { x: o.x, y: HEIGHT * 0.5, z }
      break
    case 'fan': {
      const root = ref(rootOf(o))
      root.position3 = { x: o.x, y: o.y, z }
      root.rotation3 = { x: 0, y: 0, z: o.ph + t * o.w }
      break
    }
    case 'hang': {
      const root = ref(rootOf(o))
      root.position3 = { x: o.x, y: HEIGHT, z }
      root.rotation3 = { x: 0, y: 0, z: Math.sin(t * o.w + o.ph) * o.amp }
      break
    }
    case 'door': {
      ref(rootOf(o)).position3 = { x: 0, y: 0, z }
      const slide = o.open * HALF_WIDTH * 0.95
      ref('door' + o.i + 'L').position3 = { x: -HALF_WIDTH * 0.5 - slide, y: HEIGHT * 0.5, z: 0 }
      ref('door' + o.i + 'R').position3 = { x: HALF_WIDTH * 0.5 + slide, y: HEIGHT * 0.5, z: 0 }
      break
    }
    case 'cry': {
      const root = ref(rootOf(o))
      root.position3 = { x: o.x, y: o.y + Math.sin(t * 2 + o.spin) * 0.12, z }
      root.rotation3 = { x: 0, y: o.spin + t * 110, z: 0 }
      break
    }
    case 'deco': {
      const root = ref(rootOf(o))
      root.position3 = { x: o.x, y: o.y, z }
      root.scale3 = { x: o.w, y: o.h, z: o.depth }
      break
    }
  }
}

function showBanner(small, big) {
  g.banner = { small, big, t: 3.2 }
}

function pop(text, wx, wy, wz, cls) {
  // Project a world point to HUD pixels (1600 × 900).
  let px = 800, py = 360
  if (wx !== null) {
    const rz = wz - g.camZ
    if (rz < -0.2) {
      px = 800 + ((wx - g.camX) / -rz / (TAN_HALF * ASPECT)) * 800
      py = 450 - ((wy - g.camY) / -rz / TAN_HALF) * 450
    }
  }
  g.pops.push({ id: g.popId++, text, x: px, y: py, cls: cls || '', t: 0.9, made: false })
}

function hideBall(b) {
  b.on = false
  const node = ref('ball' + b.i)
  node.visible = false
  node.position3 = { x: 0, y: -50, z: 0 }
}

/** The sound of a break of `pieces` (the glass itself is the effect). */
function breakFx(x, y, z, pieces, big) {
  const sound = big ? 'shatterBig' : pieces <= 1 ? 'tink' : 'shatter'
  ctx.playSound(sound, { volume: Math.min(1, 0.45 + pieces * 0.04), pitch: rand(0.88, 1.15) })
  g.broken += pieces
}

function openDoorFor(targetName) {
  for (const o of g.objs) {
    if (o.kind === 'door' && 'door' + o.i + 't' === targetName && !o.opening) {
      o.opening = true
      ctx.playSound('door', { volume: 0.7 })
    }
  }
}

/** Glass the camera flew into. */
function crash(hit) {
  breakFx(hit.x, hit.y, hit.z, 20, true)
  ctx.playSound('crash', { volume: 1 })
  // Burst a big hole (and the pieces the collider would hit next).
  hit.ref?.shatter3(hit, [0, 0, -g.speed * 1.6 - 4], { radius: 1.05, floor: 0 })
  for (const [ox, oy] of COLLIDER) hit.ref?.shatter3([g.camX + ox * 2.5, g.camY + oy * 2.5, hit.z], [0, 0, -g.speed - 3], { radius: 0.6, floor: 0 })
  penalty()
}

function penalty() {
  g.ammo = Math.max(0, g.ammo - CRASH_COST)
  g.multi = 1
  g.streak = 0
  g.invuln = 0.45
  g.shake = 0.55
  g.flash = 1
  g.slow = 0.35
  pop('−' + CRASH_COST, null, 0, 0, 'bad')
  if (g.ammo <= 0) gameOver()
}

function gameOver() {
  if (g.mode !== 'play') return
  g.mode = 'over'
  g.overT = 0
  const dist = Math.round(g.d)
  g.lastRun = { dist, zone: zoneOf(g.d), broken: g.broken }
  g.save.best = Math.max(g.save.best || 0, dist)
  ctx.storage.set('shatterline', g.save)
}

function throwBalls(dirX, dirY, dirZ) {
  const ox = g.camX, oy = g.camY - 0.22, oz = g.camZ - 0.35
  // Aim assist: arc onto the glass (or a crystal) under the cursor,
  // allowing for gravity, so a ball lands where you point.
  let tx = ox + dirX * 40, ty = oy + dirY * 40 + 0.22, tz = oz + dirZ * 40
  const hit = ctx.glassRaycast3([g.camX, g.camY, g.camZ], [dirX, dirY, dirZ], 70)
  let best = hit ? hit.distance : Infinity
  if (hit) { tx = hit.x; ty = hit.y; tz = hit.z }
  for (const o of g.objs) {
    if (o.kind !== 'cry' || !o.alive) continue
    const cx = o.x - g.camX, cy = o.y - g.camY, cz = -o.d - g.camZ
    const along = cx * dirX + cy * dirY + cz * dirZ
    if (along <= 1 || along > best) continue
    const off = Math.hypot(cx - dirX * along, cy - dirY * along, cz - dirZ * along)
    if (off < 0.5 + along * 0.025) { best = along; tx = o.x; ty = o.y; tz = -o.d }
  }
  const dx = tx - ox, dy = ty - oy, dz = tz - oz
  const dist = Math.hypot(dx, dy, dz)
  const time = Math.max(0.05, dist / THROW_SPEED)
  const vx = dx / time, vy = dy / time + 0.5 * GRAVITY * time, vz = dz / time
  const count = g.mode === 'play' ? g.multi : 1
  for (let n = 0; n < count; n++) {
    const b = g.balls.find((ball) => !ball.on)
    if (!b) break
    const spread = (n - (count - 1) / 2) * 0.16
    Object.assign(b, { on: true, x: ox + spread, y: oy + Math.abs(spread) * 0.3, z: oz, vx: vx + spread * 1.4, vy, vz, age: 0 })
    ref('ball' + b.i).visible = true
  }
  ctx.playSound('throw', { volume: 0.5, pitch: rand(0.9, 1.1) })
}

function stepBall(b, dt) {
  for (let s = 0; s < 2; s++) {
    const h = dt / 2
    b.vy -= GRAVITY * h
    const speed = Math.hypot(b.vx, b.vy, b.vz)
    const hit = ctx.glassRaycast3([b.x, b.y, b.z], [b.vx, b.vy, b.vz], speed * h + BALL_R)
    if (hit && hit.ref) {
      const r = hit.ref.shatter3(hit, [b.vx, b.vy, b.vz], { radius: 0.34, floor: 0 })
      const pieces = r.broken + r.loose
      if (pieces > 0) {
        breakFx(hit.x, hit.y, hit.z, pieces, r.loose > 5 || pieces > 12)
        if (hit.ref.name.startsWith('door')) openDoorFor(hit.ref.name)
        // Punch through, losing some pace and kicked a little off line.
        b.vx = b.vx * 0.72 + hit.normalX * 0.6 + rand(-0.8, 0.8)
        b.vy = b.vy * 0.72 + rand(-0.4, 0.8)
        b.vz *= 0.72
      }
    }
    b.x += b.vx * h
    b.y += b.vy * h
    const prevZ = b.z
    b.z += b.vz * h
    // Crystals.
    for (const o of g.objs) {
      if (o.kind !== 'cry' || !o.alive) continue
      if (Math.hypot(b.x - o.x, b.y - o.y, b.z + o.d) < BALL_R + CRYSTAL_R) collectCrystal(o)
    }
    // Floor, ceiling, walls.
    if (b.y < BALL_R) {
      b.y = BALL_R
      if (b.vy < 0) b.vy = -b.vy * 0.45
      b.vx *= 0.92
      b.vz *= 0.92
      if (Math.abs(b.vy) > 1.2) ctx.playSound('tink', { volume: Math.min(0.25, Math.abs(b.vy) * 0.04), pitch: rand(0.5, 0.7) })
    }
    if (b.y > HEIGHT - BALL_R && b.vy > 0) b.vy = -b.vy * 0.5
    if (Math.abs(b.x) > HALF_WIDTH - BALL_R) {
      b.x = Math.sign(b.x) * (HALF_WIDTH - BALL_R)
      b.vx = -b.vx * 0.55
    }
    // Closed door leaves bounce balls back.
    for (const o of g.objs) {
      if (o.kind !== 'door') continue
      const plane = -o.d + 0.2
      if (prevZ > plane && b.z <= plane && Math.abs(b.x) > o.open * HALF_WIDTH * 0.95) {
        b.z = plane + 0.01
        b.vz = -b.vz * 0.35
        ctx.playSound('tink', { volume: 0.25, pitch: 0.35 })
      }
    }
  }
  b.age += dt
  if (b.age > 5 || b.z > g.camZ + 2) hideBall(b)
}

function collectCrystal(o) {
  o.alive = false
  const node = ref('cry' + o.i)
  node.visible = false
  if (g.mode !== 'play') return
  g.ammo += CRYSTAL_GAIN
  g.bump = 0.25
  g.streak++
  ctx.playSound('crystal', { volume: 0.75, pitch: 1 + Math.min(g.streak, 10) * 0.035 })
  pop('+' + CRYSTAL_GAIN, o.x, o.y, -o.d, '')
  if (g.streak >= STREAK_FOR_MULTI) {
    g.streak = 0
    if (g.multi < MAX_MULTI) {
      g.multi++
      pop('MULTIBALL ×' + g.multi, null, 0, 0, 'multi')
    }
  }
}

// --------------------------------------------------------- the frame --
const paused = g.paused && g.mode === 'play'
if (keys.has('p') && !g.wasP && g.mode === 'play') g.paused = !g.paused
g.wasP = keys.has('p')
g.slow = Math.max(0, g.slow - realDt)
const dt = paused ? 0 : realDt * (g.slow > 0 ? 0.45 : 1)
g.t += dt
g.playT += dt

// Menu buttons → start a run.
if (g.req !== undefined && g.req !== null) {
  const zone = g.req
  g.req = null
  startRun(zone, false)
}

// Speed: faster every zone (and a gentle ramp within it).
const zoneNow = zoneOf(g.d)
const targetSpeed = g.mode === 'play' ? Math.min(19, 9.5 + zoneNow * 0.8 + ((g.d % ZONE_LEN) / ZONE_LEN) * 0.6) : g.mode === 'over' ? 0 : 5
g.speed += (targetSpeed - g.speed) * Math.min(1, dt * (g.mode === 'over' ? 1.6 : 0.9))
const prevD = g.d
g.d += g.speed * dt

// Checkpoints.
if (zoneNow !== g.lastZone) {
  g.lastZone = zoneNow
  if (g.mode === 'play') {
    showBanner('CHECKPOINT ' + zoneNow, themeOf(zoneNow).name.toUpperCase())
    ctx.playSound('checkpoint', { volume: 0.6 })
    const reachable = Math.min(zoneNow, ZONES.length - 1)
    if (reachable > (g.save.unlocked || 0)) {
      g.save.unlocked = reachable
      ctx.storage.set('shatterline', g.save)
    }
  }
}

// The camera: down the middle with a breath of bob, and shake on a crash.
g.shake = Math.max(0, g.shake - dt)
g.camX = Math.sin(g.t * 0.7) * 0.05 + (g.shake > 0 ? rand(-1, 1) * g.shake * 0.12 : 0)
g.camY = EYE + Math.sin(g.t * 2.2) * 0.035 + (g.shake > 0 ? rand(-1, 1) * g.shake * 0.1 : 0)
g.camZ = -g.d
const cam = ref('cam')
cam.position3 = { x: g.camX, y: g.camY, z: g.camZ }
cam.rotation3 = { x: 0, y: 0, z: Math.sin(g.t * 0.5) * 0.4 + (g.shake > 0 ? rand(-1, 1) * g.shake * 3 : 0) }

// Glass the camera passes through.
g.invuln = Math.max(0, g.invuln - dt)
if (g.mode === 'play' && g.invuln <= 0 && g.d > prevD) {
  for (const [ox, oy] of COLLIDER) {
    const hit = ctx.glassRaycast3([g.camX + ox, g.camY + oy, -prevD + 0.3], [0, 0, -1], g.d - prevD + 0.5)
    if (hit) {
      crash(hit)
      break
    }
  }
}

// Spawn ahead, recycle behind, animate what moves.
spawnAhead()
for (let k = g.objs.length - 1; k >= 0; k--) {
  const o = g.objs[k]
  if (o.d < g.d - 9) {
    release(o, false)
    g.objs.splice(k, 1)
    continue
  }
  if (o.kind === 'door') {
    if (o.opening) o.open = Math.min(1, o.open + dt * 2.6)
    // Flying into a closed door: a crash, and it bursts open.
    if (!o.opening && o.d - g.d < 0.6 && g.mode === 'play') {
      o.opening = true
      o.open = 0.6
      breakFx(g.camX, g.camY, -o.d, 10, true)
      ctx.playSound('crash', { volume: 1 })
      penalty()
    }
  }
  if (o.kind === 'pane' && o.move || o.kind === 'fan' || o.kind === 'hang' || o.kind === 'door' || o.kind === 'cry') updateObj(o, g.t)
}

// Arches (and the lamps on the next few) recycle forward.
for (let i = 0; i < ARCHES; i++) {
  if (g.archD[i] < g.d - 8 || g.archDirty) {
    while (g.archD[i] < g.d - 8) g.archD[i] += ARCHES * ARCH_GAP
    ref('arch' + i).position3 = { x: 0, y: 0, z: -g.archD[i] }
  }
}
g.archDirty = false
const ahead = g.archD.map((d, i) => ({ d, i })).filter((a) => a.d > g.d - 2).sort((a, b) => a.d - b.d)
for (let j = 0; j < LAMPS; j++) {
  const a = ahead[j + 1] || ahead[0]
  ref('lamp' + j).position3 = { x: 0, y: HEIGHT - 1.4, z: -a.d }
}

// The shell rides along (its tiles are world-locked, so they stream by).
const shellZ = -(g.d + SHELL_LEN * 0.5 - 40)
for (const id of ['floor', 'ceil', 'wallL', 'wallR']) {
  const node = ref(id)
  const p = node.position3
  node.position3 = { x: p.x, y: p.y, z: shellZ }
}

// Throwing.
const down = keys.has('mouse') || keys.has('mouseleft')
const space = keys.has('space')
const pressed = (down && !g.wasDown) || (space && !g.wasSpace)
g.wasDown = down
g.wasSpace = space
if (g.mode === 'play' && !paused && pressed && g.playT > 0.3) {
  if (g.ammo > 0) {
    const nx = space && !down ? 0 : ctx.mouseX / (W / 2), ny = space && !down ? 0 : ctx.mouseY / (H / 2)
    const [dx, dy, dz] = norm(nx * TAN_HALF * ASPECT, ny * TAN_HALF, -1)
    g.ammo--
    throwBalls(dx, dy, dz)
  }
}
// Attract mode behind the menu: it plays itself.
if (g.mode === 'menu') {
  g.attractT -= dt
  if (g.attractT <= 0) {
    g.attractT = 0.55
    const target = g.objs.filter((o) => (o.kind === 'pane' || o.kind === 'bar' || o.kind === 'cry') && o.d - g.d > 6 && o.d - g.d < 30).sort((a, b) => a.d - b.d)[0]
    if (target) {
      const tx = target.kind === 'pane' ? target.x + rand(-0.4, 0.4) : target.x
      const ty = target.kind === 'bar' ? EYE + rand(-0.3, 0.3) : (target.y ?? EYE) + rand(-0.3, 0.3)
      throwBalls(...norm(tx - g.camX, ty - g.camY, -target.d - g.camZ))
    }
  }
}
for (const b of g.balls) {
  if (!b.on) continue
  stepBall(b, dt)
  if (b.on) ref('ball' + b.i).position3 = { x: b.x, y: b.y, z: b.z }
}
// Out of balls (and none left in the air that could still win some back).
if (g.mode === 'play' && g.ammo <= 0 && !g.balls.some((b) => b.on)) {
  g.overT += dt
  if (g.overT > 0.9) gameOver()
} else if (g.mode === 'play') g.overT = 0
else if (g.mode === 'over') g.overT += realDt

// --------------------------------------------------------- palette --
const target = themeOf(g.mode === 'menu' ? 0 : zoneOf(g.d + 30))
const k = 1 - Math.exp(-dt * 1.1)
let moved = false
for (const key of ['fog', 'floor', 'wall', 'arch', 'glow', 'glass', 'ambient']) {
  const next = lerp3(g.col[key], target[key], k)
  if (Math.abs(next[0] - g.col[key][0]) + Math.abs(next[1] - g.col[key][1]) + Math.abs(next[2] - g.col[key][2]) > 0.0004) moved = true
  g.col[key] = next
}
ctx.setEnvironment3({ fog: g.col.fog, fogDensity: 0.0135, ambient: g.col.ambient })
g.colT = (g.colT || 0) - realDt
if ((moved || g.colDirty) && g.colT <= 0) {
  g.colT = 0.08
  g.colDirty = false
  const c = g.col
  const rgba = (v, a = 1) => [v[0], v[1], v[2], a]
  ref('floor').fillColor = rgba(c.floor)
  for (const id of ['ceil', 'wallL', 'wallR']) ref(id).fillColor = rgba(c.wall)
  for (let i = 0; i < ARCHES; i++) {
    for (const part of ['L', 'R', 'T']) ref('arch' + i + part).fillColor = rgba(c.arch)
    for (const part of ['G', 'a', 'b']) ref('arch' + i + part).fillColor = rgba(c.glow)
  }
  for (let i = 0; i < DECOS; i++) ref('deco' + i).fillColor = rgba(lerp3(c.arch, c.wall, 0.4))
  const glass = rgba(c.glass, 0.5)
  for (let i = 0; i < BIG_PANES; i++) ref('pb' + i).fillColor = glass
  for (let i = 0; i < SMALL_PANES; i++) ref('ps' + i).fillColor = glass
  for (let i = 0; i < SPINNERS; i++) ref('sp' + i).fillColor = glass
  for (let i = 0; i < BARS; i++) ref('bar' + i).fillColor = glass
  for (let i = 0; i < HANGERS; i++) ref('hang' + i + 'g').fillColor = glass
  for (let f = 0; f < FANS; f++) for (let b = 0; b < BLADES; b++) ref('fan' + f + 'b' + b).fillColor = glass
  // Chrome reflects the zone: its Tint is the floor colour.
  const chrome = rgba(lerp3(c.floor, c.fog, 0.35))
  for (let i = 0; i < BALLS; i++) ref('ball' + i).fillColor = chrome
  for (let i = 0; i < DOORS; i++) for (const s of ['L', 'R']) ref('door' + i + s + 's').fillColor = rgba(c.glow)
}

// ------------------------------------------------------------- HUD --
g.flash = Math.max(0, g.flash - realDt * 2.5)
g.bump = Math.max(0, (g.bump || 0) - realDt)
if (g.banner) g.banner.t -= realDt
const hud = ref('hud').dom
if (hud) {
  const $ = (id) => hud.getElementById(id)
  if (g.hudDom !== hud) {
    g.hudDom = hud
    g.hudKey = ''
    $('streak').innerHTML = '<i></i>'.repeat(STREAK_FOR_MULTI)
  }
  const playing = g.mode === 'play'
  $('root').style.opacity = playing ? '1' : '0'
  const key = [g.ammo, g.streak, g.multi, Math.floor(g.d / 5), g.zone].join('|')
  if (key !== g.hudKey) {
    g.hudKey = key
    $('balls').textContent = String(g.ammo)
    const count = $('balls').parentElement
    count.classList.toggle('low', g.ammo <= 10)
    ;[...$('streak').children].forEach((el, i) => el.classList.toggle('on', i < g.streak))
    $('multi').textContent = g.multi > 1 ? 'MULTIBALL ×' + g.multi : ''
    const zone = zoneOf(g.d)
    $('zoneName').textContent = (zone >= ZONES.length ? 'ENDLESS · ' : 'ZONE ' + (zone + 1) + ' · ') + themeOf(zone).name.toUpperCase()
    $('dist').textContent = Math.round(g.d) + ' M'
  }
  $('balls').parentElement.classList.toggle('bump', g.bump > 0)
  $('flash').classList.toggle('on', g.flash > 0.6)
  const banner = $('banner')
  const showB = !!(g.banner && g.banner.t > 0 && playing)
  banner.classList.toggle('on', showB)
  if (showB && $('bannerBig').textContent !== g.banner.big) {
    $('bannerSmall').textContent = g.banner.small
    $('bannerBig').textContent = g.banner.big
  }
  const hint = paused ? 'PAUSED · P TO RESUME' : playing && g.playT < 6 ? 'CLICK TO THROW · SPACE THROWS STRAIGHT · P PAUSES' : ''
  if ($('hint').textContent !== hint) $('hint').textContent = hint
  // Score pops.
  const pops = $('pops')
  for (const p of g.pops) {
    if (!p.made) {
      p.made = true
      const el = document.createElement('div')
      el.className = 'pop ' + p.cls
      el.id = 'pop' + p.id
      el.textContent = p.text
      el.style.left = p.x + 'px'
      el.style.top = p.y + 'px'
      pops.appendChild(el)
    }
    p.t -= realDt
    if (p.t <= 0) pops.querySelector('#pop' + p.id)?.remove()
  }
  g.pops = g.pops.filter((p) => p.t > 0)
}

// The menu (start screen / run over).
const menuNode = ref('menu')
const menuOn = g.mode !== 'play'
if (menuNode.visible !== menuOn) menuNode.visible = menuOn
const menu = menuNode.dom
if (menu && menuOn) {
  const $ = (id) => menu.getElementById(id)
  // (After a run, the menu waits a beat so you see the last shatter.)
  const showMenu = g.mode !== 'over' || g.overT > 1.2
  $('root').style.opacity = showMenu ? '1' : '0'
  const key = g.mode + '|' + (g.save.unlocked || 0) + '|' + (g.lastRun ? g.lastRun.dist : '')
  if (g.menuDom !== menu || g.menuKey !== key) {
    g.menuDom = menu
    g.menuKey = key
    const unlocked = g.save.unlocked || 0
    const lastZone = g.lastRun ? Math.min(g.lastRun.zone, ZONES.length - 1) : 0
    $('sub').textContent = g.mode === 'over' ? 'OUT OF BALLS' : "Throw chrome. Break glass. Don't hit what's still standing."
    $('stats').textContent = g.mode === 'over' && g.lastRun
      ? `${g.lastRun.dist} M  ·  ZONE ${g.lastRun.zone + 1}  ·  ${g.lastRun.broken} PIECES OF GLASS  ·  BEST ${g.save.best} M`
      : g.save.best ? `BEST ${g.save.best} M` : ''
    const zones = $('zones')
    zones.innerHTML = ''
    ZONES.forEach((z, i) => {
      const button = document.createElement('button')
      button.className = 'z' + (i > unlocked ? ' locked' : '')
      const c = (v) => `rgb(${Math.round(v[0] * 255)} ${Math.round(v[1] * 255)} ${Math.round(v[2] * 255)})`
      button.innerHTML = `<i style="background: linear-gradient(90deg, ${c(z.fog)}, ${c(z.glow)})"></i>${i === 0 ? 'START' : 'ZONE ' + (i + 1)}<small>${z.name}${g.mode === 'over' && i === lastZone && i > 0 ? ' · RETRY' : ''}</small>`
      if (i <= unlocked) button.addEventListener('click', () => { g.req = i })
      zones.appendChild(button)
    })
  }
}
