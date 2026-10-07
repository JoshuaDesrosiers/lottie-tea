// ===================================================================
//  CROWN MATCH — a royal match-3. Three scenes, each one Code program:
//  Home, Levels (the kingdom map) and Game (the board). Shared library
//  first; Rust appends one `//@program` section per scene.
//
//  ctx.globals.match3  — this play session (the level picked, the save)
//  ctx.globals.m3scene — the current screen's cached refs (cleared on
//                        every scene switch: leaving restores the pose)
//  ctx.storage 'crownmatch' — progress: unlocked level, best stars per
//                        level, coins. Survives restarts.
// ===================================================================

const S = ctx.globals.match3 || (ctx.globals.match3 = {})
const g = ctx.globals.m3scene || (ctx.globals.m3scene = {})
const save = S.save || (S.save = Object.assign({ unlocked: 1, stars: {}, best: {}, coins: 0 }, ctx.storage.get('crownmatch') || {}))
function persist() { ctx.storage.set('crownmatch', save) }
const T = performance.now() / 1000
const dt = Math.min(ctx.deltaSeconds, 1 / 20)
const keys = ctx.keysDown
const LEVEL_COUNT = 20

if (!g.refs) { g.refs = {}; g.mouseWas = true }
const ref = (id) => {
  let r = g.refs[id]
  if (r === undefined) { r = getCompByID(id); g.refs[id] = r }
  return r
}
function setText(id, text) { const r = ref(id); if (r && r.textContent !== text) r.textContent = text }
function show(id, on) { const r = ref(id); if (r && r.visible !== on) r.visible = on }

/** A plate button: grows on hover, returns true when tapped. */
function button(id, x, y, w, h, enabled = true) {
  const btn = ref(id + 'Btn')
  if (!btn) return false
  const over = enabled && Math.abs(ctx.mouseX - x) < w / 2 && Math.abs(ctx.mouseY - y) < h / 2
  const s = over ? (keys.has('mouse') ? 0.95 : 1.06) : 1
  if (btn.scaleX !== s) { btn.scaleX = s; btn.scaleY = s }
  return over && ctx.tap
}

function go(scene) {
  ctx.globals.m3scene = {}
  ctx.loadScene(scene)
}

const GEM_COLORS = [
  [0.93, 0.18, 0.3, 1], [0.18, 0.5, 1, 1], [0.18, 0.82, 0.36, 1],
  [1, 0.8, 0.16, 1], [0.7, 0.34, 0.98, 1], [1, 0.5, 0.12, 1], [1, 1, 1, 1],
]
const totalStars = () => Object.values(save.stars).reduce((a, b) => a + b, 0)

/** Deterministic random numbers, so each level always has the same layout. */
function rng(seed) {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = a
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

/** Level `n` (1-based): moves, goals, frost layout, holes, star scores. */
function levelDef(n) {
  const r = rng(n * 7919 + 13)
  const colors = n <= 3 ? 5 : 6
  const order = [0, 1, 2, 3, 4, 5].slice(0, colors).sort(() => r() - 0.5)
  const goals = []
  const goalCount = n < 4 ? 1 : n < 11 ? 2 : 3
  for (let k = 0; k < goalCount; k++) goals.push({ t: order[k], n: 10 + Math.floor(n * 1.3) + Math.floor(r() * 5) })
  // Frost: every other level from 5, in one of three patterns.
  const frost = new Array(64).fill(0)
  if (n >= 5 && n % 2 === 1) {
    const pattern = Math.floor(n / 2) % 3
    for (let rr = 0; rr < 8; rr++) for (let c = 0; c < 8; c++) {
      const on = pattern === 0 ? rr >= 2 && rr <= 5 && c >= 2 && c <= 5 : pattern === 1 ? rr === 3 || rr === 4 || rr === 6 : (rr + c) % 3 === 0 && rr > 1
      if (on) frost[rr * 8 + c] = n >= 11 ? 2 : 1
    }
    const total = frost.reduce((a, b) => a + (b > 0 ? 1 : 0), 0)
    if (goals.length === 3) goals.pop()
    goals.push({ t: 'frost', n: total })
  }
  // Holes: every 5th level gets a shaped board.
  const holes = new Array(64).fill(false)
  if (n % 5 === 0) for (const [rr, c] of [[0, 0], [0, 1], [1, 0], [0, 7], [0, 6], [1, 7], [7, 0], [7, 7], [3, 3], [3, 4], [4, 3], [4, 4]]) holes[rr * 8 + c] = true
  const moves = Math.max(15, 27 - Math.floor(n * 0.55)) + (frost.some((f) => f) ? 5 : 0)
  const base = 6000 + n * 700
  return { n, colors, goals, frost, holes, moves, stars: [Math.round(base * 0.45), Math.round(base * 0.75), base] }
}

// ===================================================================
//  Scenes
// ===================================================================

//@program Home
// Title screen: a bobbing crown, gems drifting up the screen, PLAY (your
// next level) or the LEVEL MAP.
if (!g.drift) {
  g.drift = []
  for (let i = 0; i < 14; i++) g.drift.push({ x: -160 + ((i * 53) % 320), y: -300 + ((i * 97) % 600), s: 18 + (i % 4) * 7, spin: (i % 2 ? 1 : -1) * (20 + i * 3) })
}
for (let i = 0; i < g.drift.length; i++) {
  const d = g.drift[i]
  d.y += d.s * dt
  if (d.y > 340) { d.y = -340; d.x = -160 + Math.random() * 320 }
  const r = ref('drift' + i)
  r.x = Math.round(d.x + Math.sin(T * 0.8 + i) * 12)
  r.y = Math.round(d.y)
  r.rotation = (T * d.spin) % 360
  r.fillColor = [GEM_COLORS[i % 6][0], GEM_COLORS[i % 6][1], GEM_COLORS[i % 6][2], 0.35]
}
const crown = ref('crown')
crown.y = Math.round(168 + Math.sin(T * 2) * 5)
crown.rotation = Math.sin(T * 1.3) * 3
const next = Math.min(LEVEL_COUNT, save.unlocked)
setText('levelHint', save.unlocked > LEVEL_COUNT ? 'ALL LEVELS CLEARED!' : 'LEVEL ' + next)
setText('coinsText', String(save.coins))
setText('starsText', String(totalStars()))
const pulse = 1 + Math.sin(T * 4) * 0.04
ref('playLabel').scaleX = pulse; ref('playLabel').scaleY = pulse
if (button('play', 0, -90, 200, 64)) { S.level = next; go('Crown Match Game') }
else if (button('map', 0, -196, 160, 40)) go('Crown Match Levels')
else if (keys.has('enter') || keys.has('space')) { S.level = next; go('Crown Match Game') }

//@program Levels
// The kingdom map: 20 levels along a winding path, bottom to top. Gold =
// done (with its best stars), green = your next level, grey = locked.
const spot = (i) => {
  const row = Math.floor(i / 4)
  const col = row % 2 === 0 ? i % 4 : 3 - (i % 4)
  return { x: -114 + col * 76, y: -206 + row * 94 }
}
for (let i = 0; i < LEVEL_COUNT; i++) {
  const n = i + 1
  const stars = save.stars[n] || 0
  const unlocked = n <= save.unlocked
  const current = n === Math.min(LEVEL_COUNT, save.unlocked) && !(save.stars[n] > 0)
  const node = ref('lvl' + i)
  node.fillColor = !unlocked ? [0.32, 0.32, 0.4, 1] : current ? [0.3, 0.85, 0.35, 1] : [1, 0.74, 0.22, 1]
  const s = current ? 1 + Math.sin(T * 5) * 0.08 : 1
  node.scaleX = s; node.scaleY = s
  ref('lvlNum' + i).fillColor = unlocked ? [1, 1, 1, 1] : [0.6, 0.6, 0.66, 1]
  for (let k = 0; k < 3; k++) ref('lvlStar' + i + '_' + k).fillColor = k < stars ? [1, 0.85, 0.2, 1] : [0.3, 0.3, 0.35, 1]
  if (i < LEVEL_COUNT - 1) for (let k = 1; k < 4; k++) ref('dot' + i + '_' + k).fillColor = n + 1 <= save.unlocked ? [1, 0.88, 0.45, 0.9] : [1, 0.95, 0.8, 0.25]
  const p = spot(i)
  if (ctx.tap && Math.hypot(ctx.mouseX - p.x, ctx.mouseY - p.y) < 26) {
    if (unlocked) { S.level = n; go('Crown Match Game') }
    else { g.lockedT = 1.4; g.lockedN = n }
  }
}
g.lockedT = Math.max(0, (g.lockedT || 0) - dt)
setText('hint', g.lockedT > 0 ? 'LEVEL ' + g.lockedN + ' IS LOCKED - BEAT ' + (g.lockedN - 1) + ' FIRST' : 'TAP A LEVEL TO PLAY')
setText('starTotal', totalStars() + ' / ' + LEVEL_COUNT * 3)
if (button('back', -118, 284, 92, 34) || keys.has('escape')) go('Crown Match Home')

//@program Game
// The board. Drag a gem onto a neighbour (or tap one, then the other) to
// swap; only swaps that make a match stick. Matches clear, gems fall,
// new ones drop in, cascades chain. Specials:
//   4 in a row  -> ROCKET (clears its row / column)
//   L or T      -> TNT    (clears a 5x5 area)
//   5 in a row  -> LIGHT BALL (swap it with a gem: clears that color)
// Swapping two specials together combines them.
const N = 8, CELL = 40, BOARD_Y = -36
const lx = (c) => -140 + c * CELL
const ly = (r) => 140 - r * CELL

// ---------------------------------------------------------- setup --
function lookOf(p) {
  // Shape, color and special markers for a piece.
  const v = g.vis[p.v]
  const t = p.sp === 'light' ? 6 : p.t
  if (v.t !== t) { v.t = t; v.ref.path = g.tpl[t]; v.ref.fillColor = GEM_COLORS[t] }
  const h = p.sp === 'h', vv = p.sp === 'v', bomb = p.sp === 'bomb'
  if (v.h !== h) { v.h = h; v.H.visible = h }
  if (v.vv !== vv) { v.vv = vv; v.V.visible = vv }
  if (v.bomb !== bomb) { v.bomb = bomb; v.R.visible = bomb }
}

function newPiece(t, r, c, startY) {
  const v = g.free.pop()
  const p = { t, sp: null, v, r, c, x: lx(c), y: startY, vy: 0, scale: 1, pop: 0 }
  const vis = g.vis[v]
  vis.ref.visible = true
  vis.lastX = vis.lastY = vis.lastS = null
  lookOf(p)
  return p
}

function freePiece(p) {
  const vis = g.vis[p.v]
  vis.ref.visible = false
  g.free.push(p.v)
}

// (Rows may not exist yet while the board is first being filled.)
const at = (r, c) => (r >= 0 && c >= 0 && r < N && c < N && !g.holes[r * N + c] && g.grid[r] ? g.grid[r][c] : null)
const playable = (r, c) => r >= 0 && c >= 0 && r < N && c < N && !g.holes[r * N + c]

/** Would type `t` at (r, c) complete a run of 3 (for fills and moves)? */
function makesRun(r, c, t) {
  const same = (rr, cc) => { const p = at(rr, cc); return p && p.t === t && p.sp !== 'light' }
  let h = 1, v = 1
  for (let k = c - 1; same(r, k); k--) h++
  for (let k = c + 1; same(r, k); k++) h++
  for (let k = r - 1; same(k, c); k--) v++
  for (let k = r + 1; same(k, c); k++) v++
  return h >= 3 || v >= 3
}

function startLevel() {
  const def = levelDef(S.level || 1)
  g.def = def
  g.holes = def.holes
  g.frost = def.frost.slice()
  g.goals = def.goals.map((x) => ({ ...x, left: x.n }))
  g.moves = def.moves
  g.score = 0
  g.combo = 0
  g.phase = 'intro'
  g.timer = 0
  g.idleT = 0
  g.selected = null
  g.press = null
  for (const p of g.pieces || []) freePiece(p)
  g.pieces = []
  g.grid = []
  for (let r = 0; r < N; r++) {
    g.grid.push(new Array(N).fill(null))
    for (let c = 0; c < N; c++) {
      ref('cell' + (r * N + c)).visible = !g.holes[r * N + c]
      if (g.holes[r * N + c]) continue
      // Avoid starting matches.
      let t, tries = 0
      do { t = Math.floor(Math.random() * def.colors); tries++ } while (makesRun(r, c, t) && tries < 20)
      const p = newPiece(t, r, c, ly(r) + 360 + r * 14)
      g.grid[r][c] = p
      g.pieces.push(p)
    }
  }
  if (!hasMove()) shuffle()
  for (let i = 0; i < N * N; i++) refreshFrost(i)
  setText('levelText', 'LEVEL ' + def.n)
  for (let k = 0; k < 3; k++) {
    const goal = g.goals[k]
    show('goalSlot' + k, !!goal); show('goalIcon' + k, !!goal); show('goalText' + k, !!goal)
    if (!goal) continue
    const icon = ref('goalIcon' + k)
    if (goal.t === 'frost') { icon.path = g.tplFrost; icon.fillColor = [0.62, 0.86, 1, 1] } else { icon.path = g.tpl[goal.t]; icon.fillColor = GEM_COLORS[goal.t] }
  }
  // Star markers along the score bar.
  for (let k = 0; k < 3; k++) ref('barStar' + k).x = Math.round(-150 + 300 * (def.stars[k] / def.stars[2]))
  for (const id of ['dim', 'panel', 'resultTitle', 'resultSub', 'resultStar0', 'resultStar1', 'resultStar2', 'nextBtn', 'nextLabel', 'map2Btn', 'map2Label']) show(id, false)
  banner('LEVEL ' + def.n, goalSummary(), 1.6)
}

function goalSummary() {
  const names = ['RUBIES', 'SAPPHIRES', 'EMERALDS', 'TOPAZ', 'AMETHYSTS', 'AMBERS']
  return g.goals.map((x) => (x.t === 'frost' ? 'BREAK ' + x.n + ' FROST' : 'COLLECT ' + x.n + ' ' + names[x.t])).join('  +  ')
}

function banner(main, sub, time) { setText('banner', main); setText('bannerSub', sub || ''); g.bannerT = time }

function refreshFrost(i) {
  const f = g.frost[i]
  const r = ref('frost' + i)
  r.visible = f > 0
  if (f > 0) r.fillColor = f > 1 ? [0.5, 0.78, 1, 0.75] : [0.62, 0.86, 1, 0.45]
}

if (!g.init) {
  g.init = true
  g.tpl = [0, 1, 2, 3, 4, 5, 6].map((t) => ref('tpl' + t).path)
  g.tplFrost = ref('frost0').path
  g.vis = []
  for (let i = 0; i < N * N; i++) {
    g.vis.push({ ref: ref('gem' + i), H: ref('gem' + i + 'H'), V: ref('gem' + i + 'V'), R: ref('gem' + i + 'R'), t: i % 6, h: false, vv: false, bomb: false })
    ref('gem' + i).visible = false
  }
  g.free = []
  for (let i = N * N - 1; i >= 0; i--) g.free.push(i)
  g.floats = []
  for (let k = 0; k < 6; k++) g.floats.push({ ref: ref('float' + k), t: 0 })
  g.floatNext = 0
  g.pops = [0, 1, 2, 3, 4, 5, 6].map((t) => ref('pop' + t))
  startLevel()
}

// --------------------------------------------------------- matching --

/** Every run of 3+ same-colored gems: [{ cells: [[r, c]...], dir }]. */
function findRuns() {
  const runs = []
  const key = (p) => (p && p.sp !== 'light' ? p.t : -1)
  for (let r = 0; r < N; r++) {
    let c = 0
    while (c < N) {
      const t = key(at(r, c))
      let e = c + 1
      while (e < N && t >= 0 && key(at(r, e)) === t) e++
      if (t >= 0 && e - c >= 3) runs.push({ dir: 'h', cells: Array.from({ length: e - c }, (_, k) => [r, c + k]) })
      c = e
    }
  }
  for (let c = 0; c < N; c++) {
    let r = 0
    while (r < N) {
      const t = key(at(r, c))
      let e = r + 1
      while (e < N && t >= 0 && key(at(e, c)) === t) e++
      if (t >= 0 && e - r >= 3) runs.push({ dir: 'v', cells: Array.from({ length: e - r }, (_, k) => [r + k, c]) })
      r = e
    }
  }
  return runs
}

function hasMove() {
  for (let r = 0; r < N; r++) for (let c = 0; c < N; c++) {
    const p = at(r, c)
    if (!p) continue
    if (p.sp === 'light') return true
    for (const [dr, dc] of [[0, 1], [1, 0]]) {
      const q = at(r + dr, c + dc)
      if (!q) continue
      if (p.sp && q.sp) return true
      // Try the swap.
      g.grid[r][c] = q; g.grid[r + dr][c + dc] = p
      const ok = makesRun(r, c, q.t) || makesRun(r + dr, c + dc, p.t)
      g.grid[r][c] = p; g.grid[r + dr][c + dc] = q
      if (ok) return [[r, c], [r + dr, c + dc]]
    }
  }
  return false
}

function shuffle() {
  const pieces = g.pieces.filter((p) => !p.sp)
  for (let tries = 0; tries < 60; tries++) {
    const types = pieces.map((p) => p.t).sort(() => Math.random() - 0.5)
    pieces.forEach((p, i) => { p.t = types[i] })
    if (!findRuns().length && hasMove()) break
  }
  for (const p of pieces) lookOf(p)
}

// -------------------------------------------------------- clearing --

/**
 * Clears `cells` (a Set of "r,c"), detonating any specials caught in it
 * (which add their own cells), skipping cells in `keep` (where a new
 * special is being born). Counts goals, frost and score; starts the pop.
 */
function clearCells(cells, keep) {
  const queue = [...cells]
  const done = new Set()
  let cleared = 0, cx = 0, cy = 0
  while (queue.length) {
    const k = queue.shift()
    if (done.has(k)) continue
    done.add(k)
    const [r, c] = k.split(',').map(Number)
    if (!playable(r, c)) continue
    const p = g.grid[r][c]
    // Frost under the cell breaks (one layer per hit).
    const i = r * N + c
    if (g.frost[i] > 0) {
      g.frost[i]--
      refreshFrost(i)
      if (g.frost[i] === 0) bumpGoal('frost')
    }
    if (!p || p.pop > 0 || (keep && keep.has(k))) continue
    // A special in the blast goes off too.
    if (p.sp === 'h') for (let cc = 0; cc < N; cc++) queue.push(r + ',' + cc)
    if (p.sp === 'v') for (let rr = 0; rr < N; rr++) queue.push(rr + ',' + c)
    if (p.sp === 'bomb') for (let dr = -2; dr <= 2; dr++) for (let dc = -2; dc <= 2; dc++) queue.push((r + dr) + ',' + (c + dc))
    if (p.sp === 'light') {
      const colors = g.pieces.filter((q) => !q.sp && q.pop === 0).map((q) => q.t)
      const t = colors.length ? colors[Math.floor(Math.random() * colors.length)] : 0
      for (const q of g.pieces) if (q.t === t && !q.sp) queue.push(q.r + ',' + q.c)
    }
    if (p.sp) { g.score += 200; blast(p) }
    p.pop = 0.2
    if (p.sp !== 'light') bumpGoal(p.t)
    cleared++
    cx += p.x; cy += p.y
    const pop = g.pops[p.sp === 'light' ? 6 : p.t]
    pop.x = Math.round(p.x); pop.y = Math.round(p.y + BOARD_Y)
    pop.emit(7)
  }
  if (cleared) {
    const points = Math.round(cleared * 60 * (1 + g.combo * 0.5))
    g.score += points
    floatText('+' + points, cx / cleared, cy / cleared + BOARD_Y)
  }
  return cleared
}

function bumpGoal(t) {
  for (const goal of g.goals) if (goal.t === t && goal.left > 0) { goal.left--; break }
}

function blast(p) {
  const b = ref('blast')
  b.x = Math.round(p.x); b.y = Math.round(p.y + BOARD_Y)
  b.emit(30)
}

function floatText(text, x, y) {
  const f = g.floats[g.floatNext++ % g.floats.length]
  f.t = 0.9; f.x = x; f.y = y
  f.ref.textContent = text
}

/** Resolves the board after a swap or a fall: matches -> specials + clears. */
function resolve(swapped) {
  const runs = findRuns()
  if (!runs.length) return false
  const clear = new Set(), keep = new Set()
  const inRun = new Map()
  for (const run of runs) for (const [r, c] of run.cells) {
    clear.add(r + ',' + c)
    inRun.set(r + ',' + c, (inRun.get(r + ',' + c) || []).concat([run]))
  }
  // Specials are born where the player swapped (if part of the run), else
  // in the run's middle.
  const birthCell = (run) => {
    for (const s of swapped || []) if (run.cells.some(([r, c]) => r === s[0] && c === s[1])) return s
    return run.cells[Math.floor(run.cells.length / 2)]
  }
  const born = []
  // L / T: a cell in both a horizontal and a vertical run -> TNT.
  for (const [k, list] of inRun) {
    if (list.some((x) => x.dir === 'h') && list.some((x) => x.dir === 'v')) { const [r, c] = k.split(',').map(Number); born.push({ r, c, sp: 'bomb' }); list.forEach((x) => { x.used = true }) }
  }
  for (const run of runs) {
    if (run.used) continue
    if (run.cells.length >= 5) { const [r, c] = birthCell(run); born.push({ r, c, sp: 'light' }) }
    else if (run.cells.length === 4) { const [r, c] = birthCell(run); born.push({ r, c, sp: run.dir === 'h' ? 'v' : 'h' }) }
  }
  for (const b of born) {
    const k = b.r + ',' + b.c
    if (keep.has(k)) continue
    const p = g.grid[b.r][b.c]
    if (!p || p.sp) continue
    keep.add(k)
    bumpGoal(p.t)
    p.sp = b.sp
    lookOf(p)
    p.scale = 1.5
  }
  clearCells(clear, keep)
  return true
}

/** Swapping two specials (or a Light Ball with anything) combines them. */
function combo(a, b) {
  const cells = new Set()
  const addRow = (r) => { for (let c = 0; c < N; c++) cells.add(r + ',' + c) }
  const addCol = (c) => { for (let r = 0; r < N; r++) cells.add(r + ',' + c) }
  if (a.sp === 'light' || b.sp === 'light') {
    const ball = a.sp === 'light' ? a : b, other = ball === a ? b : a
    cells.add(ball.r + ',' + ball.c)
    if (other.sp === 'light') { for (const p of g.pieces) cells.add(p.r + ',' + p.c) }
    else {
      for (const p of g.pieces) if (p.t === other.t && !p.sp) {
        // Ball + special: every gem of that color becomes that special.
        if (other.sp) { p.sp = other.sp; lookOf(p) }
        cells.add(p.r + ',' + p.c)
      }
      cells.add(other.r + ',' + other.c)
    }
    ball.sp = null  // spent (don't fire a random color as well)
    g.score += 500
  } else {
    const kinds = [a.sp, b.sp].sort().join('+')
    if (kinds === 'bomb+bomb') { for (let dr = -3; dr <= 3; dr++) for (let dc = -3; dc <= 3; dc++) cells.add((b.r + dr) + ',' + (b.c + dc)) }
    else if (kinds.includes('bomb')) { for (let d = -1; d <= 1; d++) { addRow(b.r + d); addCol(b.c + d) } }
    else { addRow(b.r); addCol(b.c) }
    a.sp = b.sp = null
    cells.add(a.r + ',' + a.c); cells.add(b.r + ',' + b.c)
    g.score += 400
  }
  blast(b)
  g.shake = 0.3
  clearCells(cells, null)
}

/** Removes popped pieces, drops the rest, refills from the top. */
function gravity() {
  for (const p of g.pieces) if (p.pop > 0) { g.grid[p.r][p.c] = null; freePiece(p) }
  g.pieces = g.pieces.filter((p) => p.pop === 0)
  for (let c = 0; c < N; c++) {
    let write = N - 1
    for (let r = N - 1; r >= 0; r--) {
      if (!playable(r, c)) continue
      while (write >= 0 && !playable(write, c)) write--
      const p = g.grid[r][c]
      if (!p) continue
      if (write !== r) { g.grid[write][c] = p; g.grid[r][c] = null; p.r = write }
      write--
      while (write >= 0 && !playable(write, c)) write--
    }
    let spawned = 0
    for (let r = N - 1; r >= 0; r--) {
      if (!playable(r, c) || g.grid[r][c]) continue
      spawned++
      const p = newPiece(Math.floor(Math.random() * g.def.colors), r, c, ly(0) + spawned * CELL + 10)
      g.grid[r][c] = p
      g.pieces.push(p)
    }
  }
}

const goalsDone = () => g.goals.every((x) => x.left <= 0)
const settled = () => g.pieces.every((p) => Math.abs(p.y - ly(p.r)) < 0.5 && Math.abs(p.x - lx(p.c)) < 0.5)

// ------------------------------------------------------------ input --

function cellAt(mx, my) {
  const c = Math.round((mx + 140) / CELL), r = Math.round((140 - (my - BOARD_Y)) / CELL)
  return playable(r, c) && Math.abs(mx - lx(c)) < CELL / 2 && Math.abs(my - BOARD_Y - ly(r)) < CELL / 2 ? [r, c] : null
}

function trySwap(a, b) {
  const p = at(a[0], a[1]), q = at(b[0], b[1])
  if (!p || !q || Math.abs(a[0] - b[0]) + Math.abs(a[1] - b[1]) !== 1) return
  g.selected = null
  g.grid[a[0]][a[1]] = q; g.grid[b[0]][b[1]] = p
  ;[p.r, p.c, q.r, q.c] = [q.r, q.c, p.r, p.c]
  g.swap = { p, q, a, b, from: [[lx(a[1]), ly(a[0])], [lx(b[1]), ly(b[0])]] }
  g.phase = 'swap'
  g.timer = 0
}

function input() {
  const down = keys.has('mouse')
  const pressed = down && !g.mouseWas
  const released = !down && g.mouseWas
  g.mouseWas = down
  if (g.phase !== 'idle') { g.press = null; return }
  if (pressed) {
    const cell = cellAt(ctx.mouseX, ctx.mouseY)
    g.press = cell ? { cell, x: ctx.mouseX, y: ctx.mouseY, moved: false } : null
  }
  if (g.press && down && !g.press.moved) {
    const dx = ctx.mouseX - g.press.x, dy = ctx.mouseY - g.press.y
    if (Math.hypot(dx, dy) > 14) {
      g.press.moved = true
      const [r, c] = g.press.cell
      const target = Math.abs(dx) > Math.abs(dy) ? [r, c + Math.sign(dx)] : [r - Math.sign(dy), c]
      trySwap([r, c], target)
    }
  }
  if (released && g.press && !g.press.moved) {
    const cell = g.press.cell
    if (g.selected && Math.abs(g.selected[0] - cell[0]) + Math.abs(g.selected[1] - cell[1]) === 1) trySwap(g.selected, cell)
    else g.selected = g.selected && g.selected[0] === cell[0] && g.selected[1] === cell[1] ? null : cell
  }
  if (released) g.press = null
}

// --------------------------------------------------- the game loop --

input()
g.timer += dt
if (g.phase === 'intro' && settled() && g.timer > 1.2) { g.phase = 'idle'; g.idleT = 0 }

if (g.phase === 'swap') {
  const k = Math.min(1, g.timer / 0.16)
  const { p, q, from } = g.swap
  p.x = from[0][0] + (from[1][0] - from[0][0]) * k; p.y = from[0][1] + (from[1][1] - from[0][1]) * k
  q.x = from[1][0] + (from[0][0] - from[1][0]) * k; q.y = from[1][1] + (from[0][1] - from[1][1]) * k
  if (k >= 1) {
    g.combo = 0
    const special = p.sp === 'light' || q.sp === 'light' || (p.sp && q.sp)
    if (special) { g.moves--; combo(p, q); g.phase = 'clear'; g.timer = 0 }
    else if (resolve([[p.r, p.c], [q.r, q.c]])) { g.moves--; g.phase = 'clear'; g.timer = 0 }
    else {
      // No match: swap back.
      const { a, b } = g.swap
      g.grid[a[0]][a[1]] = p; g.grid[b[0]][b[1]] = q
      ;[p.r, p.c, q.r, q.c] = [a[0], a[1], b[0], b[1]]
      g.swap.from = [[lx(b[1]), ly(b[0])], [lx(a[1]), ly(a[0])]]
      g.phase = 'unswap'
      g.timer = 0
    }
  }
} else if (g.phase === 'unswap') {
  const k = Math.min(1, g.timer / 0.16)
  const { p, q, from } = g.swap
  p.x = from[0][0] + (from[1][0] - from[0][0]) * k; p.y = from[0][1] + (from[1][1] - from[0][1]) * k
  q.x = from[1][0] + (from[0][0] - from[1][0]) * k; q.y = from[1][1] + (from[0][1] - from[1][1]) * k
  if (k >= 1) g.phase = 'idle'
} else if (g.phase === 'clear') {
  if (g.timer >= 0.2) { gravity(); g.phase = 'fall'; g.timer = 0 }
} else if (g.phase === 'fall') {
  if (settled()) {
    g.combo++
    if (resolve(null)) {
      g.phase = 'clear'
      g.timer = 0
      if (g.combo >= 2) banner(['', '', 'GOOD!', 'GREAT!', 'AMAZING!', 'ROYAL!'][Math.min(5, g.combo)] || 'ROYAL!', '', 0.8)
    } else g.phase = 'settle'
  }
} else if (g.phase === 'settle') {
  g.phase = 'idle'
  g.idleT = 0
  if (goalsDone()) { g.phase = 'won'; g.timer = 0; banner('LEVEL COMPLETE!', g.moves > 0 ? 'BONUS: ' + g.moves + ' MOVES LEFT' : '', 1.6) }
  else if (g.moves <= 0) { g.phase = 'lost'; g.timer = 0; banner('OUT OF MOVES', '', 1.2) }
  else if (!hasMove()) { shuffle(); banner('SHUFFLE!', 'NO MOVES LEFT', 1.0) }
}

// Win: leftover moves pay out, then the result panel.
if (g.phase === 'won') {
  if (g.moves > 0 && g.timer > 0.25) {
    g.timer = 0
    g.moves--
    g.score += 250
    const p = g.pieces[Math.floor(Math.random() * g.pieces.length)]
    if (p) { blast(p); floatText('+250', p.x, p.y + BOARD_Y) }
  } else if (g.moves <= 0 && g.timer > 0.8 && !g.result) finish(true)
}
if (g.phase === 'lost' && g.timer > 1.0 && !g.result) finish(false)

function finish(won) {
  const def = g.def
  const stars = won ? Math.max(1, def.stars.filter((s) => g.score >= s).length) : 0
  g.result = { won, stars, shown: 0, t: 0 }
  if (won) {
    save.stars[def.n] = Math.max(save.stars[def.n] || 0, stars)
    save.best[def.n] = Math.max(save.best[def.n] || 0, g.score)
    save.unlocked = Math.max(save.unlocked, def.n + 1)
    save.coins += 20 + stars * 10
    persist()
    const conf = ref('confetti')
    conf.x = 0; conf.y = 120
    conf.emit(120)
  }
  for (const id of ['dim', 'panel', 'resultTitle', 'resultSub', 'resultStar0', 'resultStar1', 'resultStar2', 'nextBtn', 'nextLabel', 'map2Btn', 'map2Label']) show(id, true)
  setText('resultTitle', won ? 'VICTORY!' : 'SO CLOSE!')
  setText('resultSub', won ? 'SCORE ' + g.score + '   +' + (20 + stars * 10) + ' COINS' : 'OUT OF MOVES')
  setText('nextLabel', won ? (def.n < LEVEL_COUNT ? 'NEXT LEVEL' : 'FINISH') : 'TRY AGAIN')
  for (let k = 0; k < 3; k++) ref('resultStar' + k).fillColor = [0.3, 0.28, 0.4, 1]
}

// Result panel: stars pop in one by one; buttons.
if (g.result) {
  const res = g.result
  res.t += dt
  const panelScale = Math.min(1, res.t / 0.25)
  ref('panel').scaleX = 0.6 + panelScale * 0.4; ref('panel').scaleY = 0.6 + panelScale * 0.4
  while (res.shown < res.stars && res.t > 0.4 + res.shown * 0.35) {
    const star = ref('resultStar' + res.shown)
    star.fillColor = [1, 0.85, 0.2, 1]
    const pop = g.pops[3]
    pop.x = star.x; pop.y = star.y
    pop.emit(16)
    res.shown++
  }
  for (let k = 0; k < 3; k++) {
    const s = k < res.shown ? 1 + Math.max(0, 0.4 - (res.t - 0.4 - k * 0.35)) : 0.85
    ref('resultStar' + k).scaleX = s; ref('resultStar' + k).scaleY = s
  }
  if (res.t > 0.5) {
    if (button('next', 0, -62, 170, 42)) {
      if (res.won && g.def.n < LEVEL_COUNT) S.level = g.def.n + 1
      if (res.won && g.def.n >= LEVEL_COUNT) go('Crown Match Levels')
      else { g.result = null; startLevel() }
    } else if (button('map2', 0, -114, 170, 42)) go('Crown Match Levels')
  }
}

// ---------------------------------------------------------- visuals --

for (const p of g.pieces) {
  const tx = lx(p.c), ty = ly(p.r)
  if (g.phase !== 'swap' && g.phase !== 'unswap') {
    // Fall with gravity and a little bounce; slide sideways if needed.
    if (p.y > ty + 0.01) {
      p.vy += 1600 * dt
      p.y = Math.max(ty, p.y - p.vy * dt)
      if (p.y === ty) { p.vy = 0; p.scale = 0.9 }
    } else { p.y = ty; p.vy = 0 }
    p.x += (tx - p.x) * Math.min(1, dt * 18)
  }
  if (p.pop > 0) { p.pop = Math.max(0.0001, p.pop - dt); p.scale = Math.max(0.05, p.pop / 0.2) * 1.2 }
  else p.scale += (1 - p.scale) * Math.min(1, dt * 10)
  const vis = g.vis[p.v]
  const rx = Math.round(p.x * 2) / 2, ry = Math.round(p.y * 2) / 2
  if (vis.lastX !== rx || vis.lastY !== ry) { vis.ref.x = rx; vis.ref.y = ry; vis.lastX = rx; vis.lastY = ry }
  // Hint wiggle / selection pulse.
  let s = p.scale
  const sel = g.selected && g.selected[0] === p.r && g.selected[1] === p.c
  if (sel) s *= 1.08 + Math.sin(T * 10) * 0.04
  if (g.hint && g.hint.some(([r, c]) => r === p.r && c === p.c)) s *= 1 + Math.abs(Math.sin(T * 7)) * 0.12
  const rs = Math.round(s * 50) / 50
  if (vis.lastS !== rs) { vis.ref.scaleX = rs; vis.ref.scaleY = rs; vis.lastS = rs }
  if (p.sp === 'light') {
    vis.ref.rotation = (T * 120) % 360
    const h = T * 2
    vis.ref.fillColor = [0.6 + 0.4 * Math.sin(h), 0.6 + 0.4 * Math.sin(h + 2.1), 0.6 + 0.4 * Math.sin(h + 4.2), 1]
  }
}

// Selection ring.
const selRef = ref('sel')
selRef.visible = !!g.selected && g.phase === 'idle'
if (g.selected) { selRef.x = lx(g.selected[1]); selRef.y = ly(g.selected[0]) }

// Hint after a few idle seconds.
if (g.phase === 'idle') {
  g.idleT += dt
  if (g.idleT > 5 && !g.hint) { const m = hasMove(); g.hint = Array.isArray(m) ? m : null }
} else { g.idleT = 0; g.hint = null }

// HUD.
setText('movesText', String(Math.max(0, g.moves)))
ref('movesBadge').fillColor = g.moves <= 3 && g.phase !== 'won' ? [1, 0.55 + Math.sin(T * 8) * 0.25, 0.55, 1] : [1, 1, 1, 1]
for (let k = 0; k < g.goals.length; k++) {
  const goal = g.goals[k]
  setText('goalText' + k, goal.left > 0 ? String(goal.left) : 'DONE')
  ref('goalText' + k).fillColor = goal.left > 0 ? [1, 1, 1, 1] : [0.5, 1, 0.55, 1]
}
setText('scoreText', String(g.score))
const frac = Math.min(1, g.score / g.def.stars[2])
const fill = ref('scoreFill')
fill.scaleX = Math.max(0.01, frac)
fill.x = Math.round(-148 + 148 * frac)
for (let k = 0; k < 3; k++) ref('barStar' + k).fillColor = g.score >= g.def.stars[k] ? [1, 0.85, 0.2, 1] : [0.35, 0.32, 0.45, 1]
for (const f of g.floats) {
  if (f.t <= 0) continue
  f.t -= dt
  f.y += 40 * dt
  f.ref.x = Math.round(f.x); f.ref.y = Math.round(f.y)
  if (f.t <= 0) { f.ref.textContent = ''; f.ref.y = 10000 }
}
if (g.bannerT > 0) { g.bannerT -= dt; if (g.bannerT <= 0) banner('', '', 0) }
const shake = g.shake > 0 ? (g.shake -= dt, g.shake) : 0
const board = ref('board')
board.x = shake > 0 ? Math.round((Math.random() - 0.5) * 8 * shake / 0.3) : 0
setText('tip', g.phase === 'won' ? 'WELL DONE!' : g.combo >= 2 ? 'COMBO x' + g.combo : 'DRAG A GEM TO SWAP')
if (!g.result && (button('quit', -128, -264, 84, 34) || keys.has('escape'))) go('Crown Match Levels')
