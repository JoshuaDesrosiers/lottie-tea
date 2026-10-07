// ===================================================================
//  SUNSET TACTICS — a top-down 5v5 tactical shooter.
//
//  Two scenes, one Code program each: Select (agent pick) and Match.
//  Rust prepends the map (GRID: `.` floor, `#` wall, `c` crate, `a`/`b`
//  plantable site floor), its named points (P) and agent colors.
//
//  Vision: your agent carries a shadow-casting cone light; walls, crates,
//  smokes and Sage walls are shadow casters. Enemies only show while you
//  have line of sight to them (or recon reveals them).
//
//  Your team (agents 0-4, you are 0) and the enemy team (5-9). Teams swap
//  attack / defense after 4 rounds. First to 5 wins.
// ===================================================================

const S = ctx.globals.tactics || (ctx.globals.tactics = {
  agent: 'jett',
  record: Object.assign({ wins: 0, losses: 0 }, ctx.storage.get('tactics') || {}),
  // AI settings (Select screen), saved between sessions.
})
// (Set separately so a `tactics` global left over from before settings
// existed still gets them.)
if (!S.settings) S.settings = Object.assign({ difficulty: 'normal', enemyAcc: 100, allyAcc: 100, reaction: 100, view: '3d', sens: 45 }, ctx.storage.get('tacticsSettings') || {})
const g = ctx.globals.tacScene || (ctx.globals.tacScene = {})
const keys = ctx.keysDown
const dt = Math.min(ctx.deltaSeconds, 1 / 20)
const T = performance.now() / 1000

if (!g.refs) { g.refs = {}; g.keyWas = {} }
const ref = (id) => {
  let r = g.refs[id]
  if (r === undefined) { r = getCompByID(id); g.refs[id] = r }
  return r
}
function setText(id, text) { const r = ref(id); if (r && r.textContent !== text) r.textContent = text }
function show(id, on) { const r = ref(id); if (r && r.visible !== on) r.visible = on }
/** A key pressed this frame. */
function pressed(k) {
  const down = keys.has(k)
  const was = g.keyWas[k]
  g.keyWas[k] = down
  return down && !was
}
const rand = (a, b) => a + Math.random() * (b - a)
const pick = (arr) => arr[Math.floor(Math.random() * arr.length)]
const dist = (ax, ay, bx, by) => Math.hypot(ax - bx, ay - by)
const clip = (v, a, b) => Math.max(a, Math.min(b, v))
const angleTo = (ax, ay, bx, by) => Math.atan2(by - ay, bx - ax)
const wrap = (a) => Math.atan2(Math.sin(a), Math.cos(a))

const KIT = {
  sage: {
    name: 'SAGE', role: 'SENTINEL', ultCost: 6,
    C: { name: 'BARRIER ORB', n: 1, desc: 'C  BARRIER: A WALL THAT BLOCKS PATHS' },
    Q: { name: 'SLOW ORB', n: 2, desc: 'Q  SLOW ORB: A FIELD THAT SLOWS' },
    E: { name: 'HEALING ORB', n: 1, desc: 'E  HEAL: SELF OR AN ALLY, 60 HP' },
    X: { name: 'RESURRECTION', desc: 'X  RESURRECT A DEAD ALLY' },
  },
  reyna: {
    name: 'REYNA', role: 'DUELIST', ultCost: 5,
    C: { name: 'LEER', n: 2, desc: 'C  LEER: AN EYE THAT NEARSIGHTS' },
    Q: { name: 'DEVOUR', n: 2, desc: 'Q  DEVOUR: AFTER A KILL, HEAL' },
    E: { name: 'DISMISS', n: 2, desc: 'E  DISMISS: AFTER A KILL, VANISH' },
    X: { name: 'EMPRESS', desc: 'X  EMPRESS: FASTER FIRE, FREE HEALS' },
  },
  sova: {
    name: 'SOVA', role: 'INITIATOR', ultCost: 6,
    C: { name: 'OWL DRONE', n: 1, desc: 'C  OWL DRONE: SCOUT AND REVEAL' },
    Q: { name: 'SHOCK BOLT', n: 2, desc: 'Q  SHOCK BOLT: BOUNCES, BLASTS' },
    E: { name: 'RECON BOLT', n: 1, desc: 'E  RECON BOLT: REVEALS IN SIGHT' },
    X: { name: "HUNTER'S FURY", desc: 'X  HUNTERS FURY: 3 WALL-PIERCING BEAMS' },
  },
  kj: {
    name: 'KILLJOY', role: 'SENTINEL', ultCost: 6,
    C: { name: 'NANOSWARM', n: 2, desc: 'C  NANOSWARM: A HIDDEN DAMAGE TRAP' },
    Q: { name: 'ALARMBOT', n: 1, desc: 'Q  ALARMBOT: REVEALS + VULNERABLE' },
    E: { name: 'TURRET', n: 1, desc: 'E  TURRET: SHOOTS ENEMIES IN FRONT' },
    X: { name: 'LOCKDOWN', desc: 'X  LOCKDOWN: DETAINS ENEMIES IN RANGE' },
  },
  jett: {
    name: 'JETT', role: 'DUELIST', ultCost: 5,
    C: { name: 'CLOUDBURST', n: 2, desc: 'C  CLOUDBURST: A SMOKE AT YOUR AIM' },
    Q: { name: 'UPDRAFT', n: 2, desc: 'Q  UPDRAFT: A BURST OF SPEED' },
    E: { name: 'TAILWIND', n: 1, desc: 'E  TAILWIND: A QUICK DASH' },
    X: { name: 'BLADE STORM', desc: 'X  BLADE STORM: DEADLY THROWN KNIVES' },
  },
}
const AGENT_IDS = ['sage', 'reyna', 'sova', 'kj', 'jett']

// ------------------------------------------------- bot personalities --
// Every bot gets an archetype plus jittered traits each match, so two Jetts
// never play alike: how fast and accurate they are, how hard they push,
// how long they hold, how much utility they throw, whether they chase,
// trade, lurk or sit on odd angles.
const CALLSIGNS = ['BLITZ', 'NOVA', 'ECHO', 'RIFT', 'ZEPHYR', 'KITE', 'ONYX', 'PIXEL', 'RUNE', 'SABLE', 'TANGO', 'VOLT', 'WREN', 'HALO', 'JINX', 'MOTH', 'QUILL', 'DUSK', 'FROST', 'EMBER']
const ARCHETYPES = {
  ENTRY: { aggression: 0.9, patience: 0.2, util: 0.6, chase: 0.7, peek: 0.8 },
  LURKER: { aggression: 0.5, patience: 0.9, util: 0.4, chase: 0.4, peek: 0.3, lurk: true },
  ANCHOR: { aggression: 0.2, patience: 0.9, util: 0.8, chase: 0.1, peek: 0.2 },
  SUPPORT: { aggression: 0.4, patience: 0.6, util: 0.95, chase: 0.3, peek: 0.4, trade: true },
  HUNTER: { aggression: 1.0, patience: 0.1, util: 0.5, chase: 1.0, peek: 0.9 },
  RAT: { aggression: 0.3, patience: 1.0, util: 0.6, chase: 0.2, peek: 0.1, offAngle: true },
}
// AI difficulty presets (Select screen). They scale the enemy bots; your
// teammates always play at Normal so a harder game isn't also a worse team.
//   skill  — the range each bot's skill is rolled from
//   react  — reaction time multiplier (lower = faster)
//   spread — aim spread multiplier (lower = more accurate)
//   head   — headshot chance multiplier
//   aim    — turn / tracking speed multiplier
//   util   — how eagerly they use abilities
const DIFFICULTY = {
  easy: { skill: [0.1, 0.4], react: 1.6, spread: 1.6, head: 0.5, aim: 0.7, util: 0.6, hint: 'SLOW REACTIONS, SHAKY AIM' },
  normal: { skill: [0.3, 0.85], react: 1.0, spread: 1.0, head: 1.0, aim: 1.0, util: 1.0, hint: 'A FAIR FIGHT' },
  hard: { skill: [0.6, 0.95], react: 0.8, spread: 0.75, head: 1.4, aim: 1.25, util: 1.15, hint: 'SHARP AIM, FAST REACTIONS' },
  insane: { skill: [0.88, 1.0], react: 0.6, spread: 0.5, head: 2.0, aim: 1.6, util: 1.3, hint: 'PRO LOBBY - GOOD LUCK' },
}
const DIFFICULTY_IDS = ['easy', 'normal', 'hard', 'insane']

/** A bot's personality for `team` (MY team = 0), with the AI settings applied. */
function makeTraits(team) {
  const set = S.settings
  const diff = DIFFICULTY[team === 0 ? 'normal' : set.difficulty] || DIFFICULTY.normal
  const accuracy = (team === 0 ? set.allyAcc : set.enemyAcc) / 100
  const reactScale = (100 / set.reaction) * diff.react
  const arch = pick(Object.keys(ARCHETYPES))
  const base = ARCHETYPES[arch]
  const j = (v) => clip(v + rand(-0.15, 0.15), 0, 1)
  const skill = rand(diff.skill[0], diff.skill[1])
  return {
    arch, skill,
    aggression: j(base.aggression), patience: j(base.patience), util: clip(j(base.util) * diff.util, 0, 1), chase: j(base.chase), peek: j(base.peek),
    lurk: !!base.lurk, trade: !!base.trade, offAngle: !!base.offAngle,
    // Skill: reaction window (s), turn speed (rad/s), spray spread, headshots
    // — then the difficulty preset and the accuracy / reaction settings.
    reaction: [(0.4 - 0.18 * skill) * reactScale, (0.56 - 0.22 * skill) * reactScale],
    aimSpeed: (5 + 8 * skill) * diff.aim * Math.sqrt(set.reaction / 100),
    spreadMul: ((1.7 - 0.9 * skill) * diff.spread) / Math.max(0.25, accuracy),
    head: clip((0.06 + 0.16 * skill) * diff.head * accuracy, 0, 0.6),
    rotate: rand(0.2, 1), walker: Math.random() < 0.5, lookSpeed: rand(0.4, 1.4),
  }
}

//@program Select
// Agent select: click a card to preview its kit; set the AI difficulty and
// bot accuracy / reaction; LOCK IN to play.
if (!g.pick) g.pick = S.agent || 'jett'
const set = S.settings
const over = (x, y, w, h) => Math.abs(ctx.mouseX - x) < w / 2 && Math.abs(ctx.mouseY - y) < h / 2
for (let i = 0; i < 5; i++) {
  const id = AGENT_IDS[i]
  const x = -140 + i * 70, y = 54
  const hover = over(x, y, 54, 44)
  const chosen = g.pick === id
  ref('card' + i).fillColor = chosen ? [0.35, 0.12, 0.16, 1] : hover ? [0.2, 0.18, 0.24, 1] : [0.14, 0.12, 0.17, 1]
  const sc = chosen ? 1 + Math.sin(T * 5) * 0.05 : 1
  ref('portrait' + i).scaleX = sc; ref('portrait' + i).scaleY = sc
  ref('portrait' + i).rotation = chosen ? (T * 90) % 360 : 0
  if (hover && ctx.tap) g.pick = id
}
const kit = KIT[g.pick]
setText('pickName', kit.name + ' - ' + kit.role)
;['C', 'Q', 'E', 'X'].forEach((k, i) => setText('abil' + i, kit[k].desc))
setText('record', S.record.wins + 'W ' + S.record.losses + 'L')
// Difficulty buttons.
let changed = false
DIFFICULTY_IDS.forEach((d, k) => {
  const x = -40 + k * 62, y = -42
  const on = set.difficulty === d
  const hover = over(x, y, 58, 12)
  ref('diff' + k + 'Btn').fillColor = on ? [[0.2, 0.5, 0.3, 1], [0.25, 0.4, 0.6, 1], [0.6, 0.35, 0.15, 1], [0.7, 0.15, 0.2, 1]][k] : hover ? [0.24, 0.22, 0.28, 1] : [0.16, 0.14, 0.2, 1]
  if (hover && ctx.tap && !on) { set.difficulty = d; changed = true }
})
// Customization rows: - / + in steps of 25%, 25%..200%.
// [key, label, step, min, max, neutral] — MOUSE SENS is the first-person
// look speed (100% = the original, fast speed; 45% is the default).
const SETTINGS = [['enemyAcc', 'ENEMY ACCURACY', 25, 25, 200, 100], ['allyAcc', 'ALLY ACCURACY', 25, 25, 200, 100], ['reaction', 'BOT REACTION', 25, 25, 200, 100], ['sens', 'MOUSE SENS', 5, 5, 200, 45]]
if (set.sens === undefined) set.sens = 45
SETTINGS.forEach(([key, , step, lo, hi, neutral], k) => {
  const y = -55 - k * 10
  for (const [part, x, delta] of [['Minus', 40, -step], ['Plus', 120, step]]) {
    const hover = over(x, y, 12, 9)
    ref('set' + k + part).fillColor = hover ? [0.3, 0.28, 0.36, 1] : [0.16, 0.14, 0.2, 1]
    if (hover && ctx.tap) { set[key] = clip(set[key] + delta, lo, hi); changed = true }
  }
  setText('set' + k + 'Value', set[key] + '%')
  ref('set' + k + 'Value').fillColor = set[key] > neutral ? [1, 0.55, 0.45, 1] : set[key] < neutral ? [0.55, 0.85, 1, 1] : [1, 0.9, 0.55, 1]
})
if (changed) ctx.storage.set('tacticsSettings', set)
const d = DIFFICULTY[set.difficulty]
setText('diffHint', d.hint + (set.enemyAcc !== 100 || set.allyAcc !== 100 || set.reaction !== 100 ? '  (CUSTOM)' : ''))
// View toggle: top-down vs raycast first-person.
const viewOver = over(-150, -100, 110, 13)
ref('viewBtn').fillColor = viewOver ? [0.3, 0.28, 0.36, 1] : set.view === '3d' ? [0.25, 0.4, 0.6, 1] : [0.16, 0.14, 0.2, 1]
setText('viewLabel', set.view === '3d' ? 'VIEW: 3D' : 'VIEW: TOP-DOWN')
if (viewOver && ctx.tap) { set.view = set.view === '3d' ? 'top' : '3d'; ctx.storage.set('tacticsSettings', set) }
const lockOver = over(150, -100, 110, 13)
ref('lockBtn').fillColor = lockOver ? [1, 0.35, 0.4, 1] : [0.85, 0.25, 0.3, 1]
if ((lockOver && ctx.tap) || keys.has('enter')) {
  S.agent = g.pick
  ctx.globals.tacScene = {}
  ctx.loadScene('Tactics Match')
}

//@program Match
// ------------------------------------------------------------- map --
const GH = GW
const cellX = (x) => Math.floor((x - ORIGIN_X) / CELL)
const cellY = (y) => Math.floor((ORIGIN_Y - y) / CELL)
const centerX = (cx) => ORIGIN_X + cx * CELL + CELL / 2
const centerY = (cy) => ORIGIN_Y - cy * CELL - CELL / 2
const cellAt = (cx, cy) => (cx < 0 || cy < 0 || cx >= GW || cy >= GH ? '#' : GRID[cy * GW + cx])
/** Solid for movement / bullets / sight: walls, crates, Sage walls. */
function solidCell(cx, cy) {
  const c = cellAt(cx, cy)
  if (c === '#' || c === 'c') return true
  return g.dyn ? g.dyn[cy * GW + cx] > 0 : false
}
const solidAt = (x, y) => solidCell(cellX(x), cellY(y))
const siteAt = (x, y) => { const c = cellAt(cellX(x), cellY(y)); return c === 'a' ? 'A' : c === 'b' ? 'B' : null }

/** Is the straight line from (x0,y0) to (x1,y1) blocked (walls; smokes)? */
function blocked(x0, y0, x1, y1, smokes = true) {
  const d = dist(x0, y0, x1, y1)
  const n = Math.ceil(d / 3)
  for (let k = 1; k < n; k++) {
    const t = k / n
    if (solidAt(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t)) return true
  }
  if (smokes) for (const s of g.ents) if (s.kind === 'smoke' && segCircle(x0, y0, x1, y1, s.x, s.y, s.r)) return true
  return false
}
function segCircle(x0, y0, x1, y1, cx, cy, r) {
  const dx = x1 - x0, dy = y1 - y0
  const l2 = dx * dx + dy * dy || 1
  const t = clip(((cx - x0) * dx + (cy - y0) * dy) / l2, 0, 1)
  return dist(x0 + dx * t, y0 + dy * t, cx, cy) < r
}

// --------------------------------------------------- pathfinding --
const N = GW * GH
const walkable = (i) => { const c = GRID[i]; return (c === '.' || c === 'a' || c === 'b') && !(g.dyn && g.dyn[i] > 0) }
function astar(sx, sy, tx, ty) {
  const start = cellY(sy) * GW + cellX(sx), goal = cellY(ty) * GW + cellX(tx)
  if (!walkable(goal)) return null
  if (!g.gCost) { g.gCost = new Float32Array(N); g.from = new Int32Array(N); g.stamp = new Int32Array(N); g.closed = new Int32Array(N); g.searchId = 0 }
  const id = ++g.searchId
  const heap = [], hf = []
  const push = (i, f) => {
    heap.push(i); hf.push(f)
    let k = heap.length - 1
    while (k > 0) { const p = (k - 1) >> 1; if (hf[p] <= hf[k]) break; [heap[p], heap[k]] = [heap[k], heap[p]]; [hf[p], hf[k]] = [hf[k], hf[p]]; k = p }
  }
  const pop = () => {
    const top = heap[0]
    const li = heap.pop(), lf = hf.pop()
    if (heap.length) {
      heap[0] = li; hf[0] = lf
      let k = 0
      for (;;) {
        const l = 2 * k + 1, r = l + 1
        let m = k
        if (l < heap.length && hf[l] < hf[m]) m = l
        if (r < heap.length && hf[r] < hf[m]) m = r
        if (m === k) break
        ;[heap[m], heap[k]] = [heap[k], heap[m]]; [hf[m], hf[k]] = [hf[k], hf[m]]; k = m
      }
    }
    return top
  }
  const gx = goal % GW, gy = (goal / GW) | 0
  const h = (i) => { const dx = Math.abs(i % GW - gx), dy = Math.abs(((i / GW) | 0) - gy); return dx + dy + (1.4142 - 2) * Math.min(dx, dy) }
  g.stamp[start] = id; g.gCost[start] = 0; g.from[start] = -1
  push(start, h(start))
  let expanded = 0
  while (heap.length && expanded < 7000) {
    const cur = pop()
    if (g.closed[cur] === id) continue
    g.closed[cur] = id
    expanded++
    if (cur === goal) break
    const cx = cur % GW, cy = (cur / GW) | 0
    for (const [dx, dy, cost] of DIRS) {
      const nx = cx + dx, ny = cy + dy
      if (nx < 0 || ny < 0 || nx >= GW || ny >= GH) continue
      const ni = ny * GW + nx
      if (!walkable(ni) || g.closed[ni] === id) continue
      if (dx && dy && (!walkable(cy * GW + nx) || !walkable(ny * GW + cx))) continue
      const ng = g.gCost[cur] + cost
      if (g.stamp[ni] !== id || ng < g.gCost[ni]) { g.stamp[ni] = id; g.gCost[ni] = ng; g.from[ni] = cur; push(ni, ng + h(ni)) }
    }
  }
  if (g.closed[goal] !== id) return null
  const path = []
  for (let i = goal; i !== -1 && i !== start; i = g.from[i]) path.push(i)
  path.reverse()
  return path.map((i) => ({ x: centerX(i % GW), y: centerY((i / GW) | 0) }))
}
const DIRS = [[1, 0, 1], [-1, 0, 1], [0, 1, 1], [0, -1, 1], [1, 1, 1.4142], [1, -1, 1.4142], [-1, 1, 1.4142], [-1, -1, 1.4142]]

// ------------------------------------------------------ the teams --
const MY = 0, ENEMY = 1
const isAttacker = (a) => a.team === g.attackers
const enemiesOf = (a) => g.agents.filter((o) => o.team !== a.team && o.alive)
const alliesOf = (a) => g.agents.filter((o) => o.team === a.team && o.alive && o !== a)

function makeAgent(i, kind, callsign) {
  return {
    i, kind, kit: KIT[kind], team: i < 5 ? MY : ENEMY, isPlayer: i === 0,
    // (You get traits too, but they only apply on autopilot.)
    traits: makeTraits(i < 5 ? 0 : 1), callsign: i === 0 ? 'YOU' : callsign,
    x: 0, y: 0, angle: 0, aim: 0, hp: 100, armor: 50, alive: true, ult: 0,
    root: ref('ag' + i), body: ref('ag' + i + 'Body'), xa: ref('ag' + i + 'Xa'), xb: ref('ag' + i + 'Xb'), hpBar: ref('ag' + i + 'Hp'), hpBack: ref('ag' + i + 'HpB'), mark: ref('ag' + i + 'Mark'),
    charges: {}, mag: 25, ammo: 25, reloadT: 0, fireCd: 0, recoil: 0,
    slowT: 0, detainT: 0, nearT: 0, vulnT: 0, dismissT: 0, updraftT: 0, empressT: 0, healLeft: 0, soulT: 0, knives: 0, knifeCd: 0, dash: null,
    path: null, goal: null, pathT: 0, think: Math.random(), target: null, reactT: 0, seenT: 0, utilCd: rand(1, 3), lastSeen: null, moving: false,
  }
}

function resetAgentForRound(a, at) {
  Object.assign(a, {
    x: at.x, y: at.y, hp: 100, armor: 50, alive: true, mag: 25, ammo: 25, reloadT: 0, fireCd: 0, recoil: 0,
    slowT: 0, detainT: 0, nearT: 0, vulnT: 0, dismissT: 0, updraftT: 0, empressT: 0, healLeft: 0, soulT: 0, knives: 0, dash: null,
    path: null, goal: null, target: null, reactT: 0, utilCd: rand(1, 3), task: null, lastSeen: null, deadAt: null, resT: 0,
    cleared: false, setupDone: false, goalFailT: 0, huntUntil: 0, huntPos: null, retreatUntil: 0, peekT: 0, peekNext: T + rand(1, 4),
    lurking: false, pushUntil: 0, pushSpot: null, spotOff: { x: rand(-20, 20), y: rand(-16, 16) }, readyT: rand(0, 3),
  })
  for (const k of ['C', 'Q', 'E']) a.charges[k] = a.kit[k].n
  a.angle = a.aim = isAttacker(a) ? Math.PI / 2 : -Math.PI / 2
}

// ----------------------------------------------------- round flow --
const ROUND_TIME = 100, BUY_TIME = 7, SPIKE_TIME = 40, PLANT_TIME = 3.5, DEFUSE_TIME = 6, WIN_ROUNDS = 5

function spawnPoints(team) {
  const base = team === g.attackers ? P.atkSpawn : P.defSpawn
  const pts = []
  for (let k = 0; k < 5; k++) pts.push({ x: base.x + (k - 2) * 14, y: base.y + (k % 2) * 10 })
  return pts
}

function startRound() {
  g.round++
  if (g.round === 5) { g.attackers = 1 - g.attackers; feed('SIDES SWITCHED') }
  g.phase = 'buy'
  g.phaseT = BUY_TIME
  g.time = ROUND_TIME
  for (const e of g.ents) despawnEnt(e)
  g.ents = []
  g.dyn = new Int16Array(N)
  g.reveal = new Float32Array(10)
  g.known = [[], []]
  for (const team of [MY, ENEMY]) {
    const pts = spawnPoints(team)
    g.agents.filter((a) => a.team === team).forEach((a, k) => resetAgentForRound(a, pts[k]))
  }
  const attackers = g.agents.filter((a) => a.team === g.attackers)
  const defenders = g.agents.filter((a) => a.team !== g.attackers)
  planAttack(attackers)
  const carrier = attackers.find((a) => a.isPlayer) || pick(attackers.filter((a) => !a.lurking)) || attackers[0]
  g.spike = { state: 'carried', carrier, x: 0, y: 0, timer: 0, prog: 0, site: null, defuser: null }
  planDefense(defenders)
  message('ROUND ' + g.round, isAttacker(g.agents[0]) ? 'ATTACK - PLANT THE SPIKE' : 'DEFEND - STOP THE PLANT', 2.5)
  // Your team calls its plan in the feed (the enemy's stays hidden).
  g.log = (g.log || []).concat(['R' + g.round + ' ' + g.plan.label + ' vs ' + g.setupLabel])
  if (g.attackers === MY) feed('PLAN: ' + g.plan.label, true)
  else feed('SETUP: ' + g.setupLabel, true)
}

/** Weighted pick from [[value, weight], ...]. */
function weighted(options) {
  const total = options.reduce((n, o) => n + o[1], 0)
  let r = Math.random() * total
  for (const [v, w] of options) { r -= w; if (r <= 0) return v }
  return options[0][0]
}

// Attack routes per site: where to gather, and which entrance to take.
const ROUTES = {
  A: { main: { stage: P.stageA, entry: P.entryA }, lobby: { stage: P.stageA3, entry: P.entryA }, elbow: { stage: P.stageA2, entry: P.entryA2 }, mid: { stage: P.stageMid, entry: P.chokeA2 } },
  B: { main: { stage: P.stageB, entry: P.entryB }, low: { stage: P.stageB2, entry: P.entryB }, market: { stage: P.stageMid, entry: P.entryB2 } },
}
/** This round's attack: a site, a strategy, and a route per attacker. */
function planAttack(attackers) {
  const site = Math.random() < 0.5 ? 'A' : 'B'
  const other = site === 'A' ? 'B' : 'A'
  const strategy = weighted([['default', 40], ['rush', 14], ['fake', 16], ['split', 20], ['slow', 10]])
  const plan = { site, show: site, strategy, executing: false, executeAt: rand(14, 26) }
  if (strategy === 'rush') plan.executeAt = 0.5
  if (strategy === 'slow') plan.executeAt = rand(38, 55)
  if (strategy === 'fake') { plan.show = other; plan.fakeAt = rand(9, 15); plan.executeAt = plan.fakeAt + rand(7, 11) }
  plan.label = { default: 'DEFAULT, HIT ', rush: 'RUSH ', fake: 'FAKE ' + other + ', HIT ', split: 'SPLIT ', slow: 'SLOW ' }[strategy] + site
  g.plan = plan
  // Routes: per site, so a fake can switch everyone over.
  const bots = attackers.filter((a) => !a.isPlayer)
  bots.forEach((a, k) => {
    a.route = {}
    for (const s of ['A', 'B']) {
      const names = Object.keys(ROUTES[s])
      let route = pick(names.filter((n) => n !== 'mid' && n !== 'market'))
      if (strategy === 'split' && k % 2 === 1) route = s === 'A' ? 'mid' : 'market'
      if (strategy === 'rush') route = 'main'
      a.route[s] = route
    }
    // Lurkers (and slow rounds) send one player to flank the rotation path.
    a.lurking = (a.traits.lurk && Math.random() < 0.7) || (strategy === 'slow' && k === bots.length - 1)
    if (strategy === 'rush') a.lurking = false
    a.spotOff = { x: rand(-22, 22), y: rand(-18, 18) }
    a.readyT = rand(0, 3) * a.traits.patience
  })
}

const HOLDS = {
  A: [{ p: P.holdA1, face: P.entryA }, { p: P.holdA2, face: P.stageA2 }, { p: P.holdA3, face: P.entryA }, { p: P.holdA4, face: P.stageA }],
  B: [{ p: P.holdB1, face: P.entryB }, { p: P.holdB2, face: P.entryB2 }, { p: P.holdB3, face: P.entryB }, { p: P.holdB4, face: P.stageB }],
  Mid: [{ p: P.holdMid, face: P.courtyard }, { p: P.holdMid2, face: P.pushMid }],
}
const PUSH = { A: P.pushA, B: P.pushB, Mid: P.pushMid }
/** This round's defense: a setup, a hold per defender, early pushers. */
function planDefense(defenders) {
  const setup = weighted([['standard', 45], ['stackA', 15], ['stackB', 15], ['aggro', 25]])
  const counts = { standard: ['A', 'A', 'B', 'B', 'Mid'], stackA: ['A', 'A', 'A', 'B', 'Mid'], stackB: ['A', 'B', 'B', 'B', 'Mid'], aggro: ['A', 'A', 'B', 'B', 'Mid'] }[setup]
  g.setupLabel = { standard: 'STANDARD 2-1-2', stackA: 'STACK A', stackB: 'STACK B', aggro: 'AGGRESSIVE' }[setup]
  // The most aggressive defender takes mid; the rest fill the sites.
  const order = defenders.slice().sort((p, q) => (q.traits ? q.traits.aggression : 0.5) - (p.traits ? p.traits.aggression : 0.5))
  const slots = counts.slice()
  const used = new Set()
  for (const a of order) {
    const home = slots.includes('Mid') && order.indexOf(a) === 0 ? 'Mid' : slots.find((s) => s !== 'Mid') || slots[0]
    slots.splice(slots.indexOf(home), 1)
    a.home = home
    // Rats like the off-angles (the later holds); everyone else picks freely.
    let options = HOLDS[home].filter((h) => !used.has(h))
    if (a.traits && a.traits.offAngle && options.length > 1) options = options.slice(1)
    const hold = pick(options) || HOLDS[home][0]
    used.add(hold)
    a.hold = hold.p
    a.face = hold.face
    a.holdHome = hold
    // Early info: aggressive defenders push out, then fall back.
    const pushy = a.traits && (setup === 'aggro' ? a.traits.aggression > 0.45 : a.traits.aggression > 0.8) && Math.random() < 0.65
    a.pushSpot = pushy ? PUSH[home] : null
    a.pushUntil = pushy ? rand(10, 22) : 0
  }
}

function endRound(winner, why) {
  if (g.phase === 'over') return
  g.phase = 'over'
  g.phaseT = 4
  g.score[winner]++
  g.lastWhy = why
  message(winner === MY ? 'ROUND WON' : 'ROUND LOST', why, 3.5)
  for (const a of g.agents) if (a.alive) a.ult = Math.min(a.kit.ultCost, a.ult + (a.team === winner ? 0 : 1))
}

function checkRoundEnd() {
  if (g.phase !== 'live' && g.phase !== 'planted') return
  const alive = (team) => g.agents.some((a) => a.team === team && a.alive)
  const atk = g.attackers, def = 1 - g.attackers
  if (!alive(def)) endRound(atk, 'DEFENDERS ELIMINATED')
  else if (!alive(atk) && g.spike.state !== 'planted') endRound(def, 'ATTACKERS ELIMINATED')
}

// --------------------------------------------------------- combat --
function damage(t, amount, src, label) {
  if (!t.alive || t.dismissT > 0) return
  if (t.vulnT > 0) amount *= 2
  const absorbed = Math.min(t.armor, amount * 0.66)
  t.armor -= absorbed
  t.hp -= amount - absorbed
  t.hitT = 0.12
  if (t.isPlayer) g.hitFlash = 0.18
  if (src && t.team !== src.team && !t.isPlayer) { t.lastSeen = { x: src.x, y: src.y, t: T }; if (!t.target) t.angle = angleTo(t.x, t.y, src.x, src.y) }
  if (t.hp <= 0) kill(t, src, label)
}

function kill(t, src, label) {
  t.alive = false
  t.hp = 0
  t.deadAt = { x: t.x, y: t.y }
  t.ult = Math.min(t.kit.ultCost, t.ult + 1)
  if (g.spike.state === 'carried' && g.spike.carrier === t) { g.spike.state = 'dropped'; g.spike.x = t.x; g.spike.y = t.y; g.spike.carrier = null }
  if (src && src !== t) {
    src.ult = Math.min(src.kit.ultCost, src.ult + 1)
    if (src.kind === 'reyna') { src.soulT = 3; if (src.empressT > 0) { src.healLeft += 50 } }
    if (src.kind === 'jett' && src.knives > 0) src.knives = 5
  }
  feed((src ? src.kit.name : '?') + ' > ' + t.kit.name + (label ? ' ' + label : ''), src && src.team === MY)
  if (t.isPlayer) message('YOU DIED', 'SPECTATING A TEAMMATE', 2)
  checkRoundEnd()
}

/** Hitscan: the first agent (or device / wall) along a ray. */
/** Hitscan: the first agent (or device / wall) along a ray. With `aim3`
 * (`{ eye, pitch }`: eye height in m, pitch in radians — first person),
 * the shot also has a height: it climbs / drops `tan(pitch)` per meter, so
 * it can sail over a crate or over someone's head, and it reports whether
 * it hit the head. */
function castRay(a, angle, range, aim3) {
  const dx = Math.cos(angle), dy = Math.sin(angle)
  const heightAt = (t) => (aim3 ? aim3.eye + Math.tan(aim3.pitch) * t * M : 1.15)
  let wallT = range
  for (let t = 4; t < range; t += 2) {
    const x = a.x + dx * t, y = a.y + dy * t
    if (!solidAt(x, y)) continue
    // Crates are only 1.3 m tall — a shot above that clears them.
    if (aim3 && cellAt(cellX(x), cellY(y)) === 'c' && heightAt(t) > 1.3) continue
    wallT = t
    break
  }
  // Too high / low (into the sky or the floor): stops where it lands.
  if (aim3 && aim3.pitch < -0.02) wallT = Math.min(wallT, (aim3.eye / Math.tan(-aim3.pitch)) / M)
  let hit = null, hitT = wallT, head = false
  for (const o of g.agents) {
    if (!o.alive || o.team === a.team || o.dismissT > 0) continue
    const ox = o.x - a.x, oy = o.y - a.y
    const along = ox * dx + oy * dy
    if (along <= 0 || along >= hitT) continue
    const perp = Math.abs(ox * dy - oy * dx)
    if (perp >= 4) continue
    if (aim3) {
      // A standing agent is ~1.75 m tall; the head is the top ~0.35 m.
      const h = heightAt(along)
      if (h < 0 || h > 1.8) continue
      head = h > 1.42
    }
    hit = o
    hitT = along
  }
  let device = null
  for (const e of g.ents) {
    if (!e.hp || e.team === a.team) continue
    const ox = e.x - a.x, oy = e.y - a.y
    const along = ox * dx + oy * dy
    if (along <= 0 || along >= hitT) continue
    if (Math.abs(ox * dy - oy * dx) < 4) { device = e; hit = null; hitT = along }
  }
  // A Sage wall segment in the way takes the hit.
  const cx = cellX(a.x + dx * hitT), cy = cellY(a.y + dy * hitT)
  const wallCell = !hit && !device && g.dyn[cy * GW + cx] > 0 ? cy * GW + cx : -1
  return { hit, device, head, t: hitT, h: heightAt(hitT), x: a.x + dx * hitT, y: a.y + dy * hitT, wallCell }
}

function fire(a) {
  if (!a.alive || a.detainT > 0 || a.dismissT > 0 || a.fireCd > 0 || a.reloadT > 0 || g.phase === 'over') return
  const knife = a.knives > 0
  if (!knife && a.ammo <= 0) { reload(a); return }
  const rate = knife ? 0.32 : a.empressT > 0 ? 0.075 : 0.1
  a.fireCd = rate
  const bot = !a.isPlayer || S.autopilot
  // You, in first person: the shot goes where the crosshair is (yaw AND
  // pitch); standing still is accurate, running much less so.
  const fps = !bot && fps3d()
  let spread = knife ? 0.004 : (fps ? 0.004 : 0.012) + a.recoil + (a.moving ? (fps ? 0.04 : 0.07) : 0) + (a.updraftT > 0 ? 0.12 : 0)
  if (bot) spread *= a.traits.spreadMul
  if (!knife) { a.ammo--; a.recoil = Math.min(0.09, a.recoil + 0.012) }
  else { a.knives--; }
  const angle = a.angle + rand(-spread, spread)
  a.lastShot = T   // (3D: shoot pose, muzzle flash, recoil kick)
  const pitch = fps ? ((g.pitch3 || 0) * Math.PI) / 180 + rand(-spread, spread) * 0.6 : 0
  const r = castRay(a, angle, 420, fps ? { eye: EYE_H, pitch } : null)
  if (fps) {
    // From the view rifle's muzzle (right of and below your eye) to the hit.
    const c = Math.cos(a.angle), sn = Math.sin(a.angle)
    tracer(a.x + c * 6 + sn * 1.6, a.y + sn * 6 - c * 1.6, r.x, r.y, [1, 0.95, 0.7, 0.85], EYE_H - 0.15, r.h)
  } else {
    tracer(a.x + Math.cos(angle) * 6, a.y + Math.sin(angle) * 6, r.x, r.y, knife ? [0.8, 0.95, 1, 1] : [1, 0.95, 0.7, 0.85])
  }
  if (r.hit) {
    const head = !knife && (fps ? r.head : Math.random() < (bot ? a.traits.head : 0.16))
    damage(r.hit, knife ? 50 : head ? 105 : 38, a, head ? '(HEAD)' : knife ? '(KNIFE)' : '')
  } else if (r.device) {
    r.device.hp -= knife ? 50 : 38
    if (r.device.hp <= 0) despawnEnt(r.device, true)
  } else if (r.wallCell >= 0) {
    hitWall(r.wallCell, 38)
  }
  if (!knife && a.ammo <= 0) reload(a)
}
function reload(a) { if (a.reloadT <= 0 && a.ammo < a.mag) a.reloadT = 2.0 }

// ---------------------------------------------------------- vision --
// Vision rules — the SAME for you and every bot: a 104 degree cone out to
// VISION px (matching your light cone and your screen), plus a short
// all-round sense, both blocked by walls and smokes.
const VISION = 190, FOV = 1.82, NEAR_SENSE = 28
const VISION_3D = 520   // px (65 m)
/** Can `a` see `o`? Cone + range, line of sight, smokes, nearsight. */
function canSee(a, o, fov = FOV) {
  if (!o.alive) return false
  const d = dist(a.x, a.y, o.x, o.y)
  // First person sees as far as the eye can (walls / smokes still block);
  // the top-down view keeps its screen-sized range. Same for bots.
  const range = a.nearT > 0 ? 48 : fps3d() ? VISION_3D : VISION
  if (d > range) return false
  if (d > NEAR_SENSE) {
    const diff = Math.abs(wrap(angleTo(a.x, a.y, o.x, o.y) - a.angle))
    if (diff > fov / 2) return false
  }
  return !blocked(a.x, a.y, o.x, o.y)
}

function revealTeam(enemyTeam, x, y, radius, seconds) {
  for (const o of g.agents) {
    if (o.team === enemyTeam && o.alive && dist(x, y, o.x, o.y) < radius && !blocked(x, y, o.x, o.y, true)) {
      g.reveal[o.i] = Math.max(g.reveal[o.i], seconds)
      note(1 - enemyTeam, o)
    }
  }
}
/** Shared team knowledge: last known enemy positions (the minimap). */
function note(team, o) {
  const list = g.known[team]
  const k = list.find((e) => e.i === o.i)
  if (k) { k.x = o.x; k.y = o.y; k.t = T } else list.push({ i: o.i, x: o.x, y: o.y, t: T })
}

// ---------------------------------------------------- ability ents --
const POOLS = { smoke: 6, wall: 12, slow: 4, swarm: 4, ring: 4, proj: 8, device: 4, beam: 3, tracer: 30 }
function take(kind) {
  g.used = g.used || {}
  const used = g.used[kind] || (g.used[kind] = new Array(POOLS[kind]).fill(false))
  for (let k = 0; k < used.length; k++) if (!used[k]) { used[k] = true; const r = ref(kind + k); r.visible = true; return { k, r } }
  return null
}
function release(kind, k) { if (g.used && g.used[kind]) g.used[kind][k] = false; const r = ref(kind + k); if (r) { r.visible = false; r.x = 100000 } }

function spawnEnt(e) { g.ents.push(e); return e }
function despawnEnt(e, destroyed) {
  if (e.dead) return
  e.dead = true
  if (e.slots) for (const [kind, k] of e.slots) release(kind, k)
  if (e.cells) for (const c of e.cells) g.dyn[c] = 0
  if (destroyed) ringFx(e.x, e.y, 14, [1, 0.6, 0.3])
}

function ringFx(x, y, radius, color) {
  const s = take('ring')
  if (!s) return
  spawnEnt({ kind: 'ringfx', x, y, life: 0.35, age: 0, radius, color, slots: [['ring', s.k]] })
  s.r.x = x; s.r.y = y
}

function tracer(x0, y0, x1, y1, color, h0 = 1.15, h1 = 1.15) {
  const s = take('tracer')
  if (!s) return
  // (3D: the tracer's end heights, for the 3D twin.)
  ;(g.tracerH || (g.tracerH = []))[s.k] = { x0, y0, h0, x1, y1, h1 }
  const len = dist(x0, y0, x1, y1)
  s.r.x = (x0 + x1) / 2; s.r.y = (y0 + y1) / 2
  s.r.rotation = (angleTo(x0, y0, x1, y1) * 180) / Math.PI
  s.r.scaleX = Math.max(1, len); s.r.scaleY = 1
  s.r.fillColor = color
  spawnEnt({ kind: 'tracer', x: x0, y: y0, life: 0.06, age: 0, slots: [['tracer', s.k]] })
}

/** A throwable's landing spot: up to `range` along `angle`, short of walls. */
function throwTo(a, tx, ty, range) {
  const ang = angleTo(a.x, a.y, tx, ty)
  const want = Math.min(range, dist(a.x, a.y, tx, ty))
  let d = 0
  for (; d < want; d += 3) if (solidAt(a.x + Math.cos(ang) * (d + 3), a.y + Math.sin(ang) * (d + 3))) break
  return { x: a.x + Math.cos(ang) * d, y: a.y + Math.sin(ang) * d, ang }
}

/** Uses ability `slot` (C/Q/E/X) aimed at (tx, ty). Returns true if used. */
function cast(a, slot, tx, ty) {
  if (!a.alive || a.detainT > 0 || g.phase === 'over') return false
  if (slot === 'X') { if (a.ult < a.kit.ultCost) return false } else if (!(a.charges[slot] > 0)) return false
  const ok = (CAST[a.kind][slot] || (() => false))(a, tx, ty)
  if (!ok) return false
  g.stats = g.stats || {}
  g.stats[a.kind + ':' + slot] = (g.stats[a.kind + ':' + slot] || 0) + 1
  if (slot === 'X') a.ult = 0
  else a.charges[slot]--
  return true
}

const enemyTeam = (a) => 1 - a.team

const CAST = {
  sage: {
    C(a, tx, ty) {
      // A 5-segment wall across the aim line, a little ahead.
      const ang = angleTo(a.x, a.y, tx, ty)
      const cx = a.x + Math.cos(ang) * 22, cy = a.y + Math.sin(ang) * 22
      const px = -Math.sin(ang), py = Math.cos(ang)
      const cells = [], slots = []
      for (let k = -2; k <= 2; k++) {
        const x = cx + px * k * CELL, y = cy + py * k * CELL
        const ci = cellY(y) * GW + cellX(x)
        if (!walkable(ci)) continue
        if (g.agents.some((o) => o.alive && cellY(o.y) * GW + cellX(o.x) === ci)) continue
        const s = take('wall')
        if (!s) break
        s.r.x = centerX(ci % GW); s.r.y = centerY((ci / GW) | 0)
        g.dyn[ci] = 250
        cells.push(ci); slots.push(['wall', s.k])
      }
      if (!cells.length) return false
      spawnEnt({ kind: 'barrier', team: a.team, x: cx, y: cy, life: 28, age: 0, cells, slots })
      return true
    },
    Q(a, tx, ty) {
      const p = throwTo(a, tx, ty, 150)
      const s = take('slow')
      if (!s) return false
      s.r.x = p.x; s.r.y = p.y
      spawnEnt({ kind: 'slow', team: a.team, x: p.x, y: p.y, r: 30, life: 6, age: 0, slots: [['slow', s.k]] })
      return true
    },
    E(a, tx, ty) {
      // Heal the most hurt ally near the aim (or yourself).
      const target = alliesOf(a).filter((o) => dist(o.x, o.y, tx, ty) < 30 && dist(o.x, o.y, a.x, a.y) < 60).sort((p, q) => p.hp - q.hp)[0] || a
      if (target.hp >= 100) return false
      target.healLeft += 60
      ringFx(target.x, target.y, 10, [0.4, 1, 0.8])
      return true
    },
    X(a) {
      const dead = g.agents.filter((o) => o.team === a.team && !o.alive && o.deadAt && dist(o.deadAt.x, o.deadAt.y, a.x, a.y) < 60).sort((p, q) => dist(p.deadAt.x, p.deadAt.y, a.x, a.y) - dist(q.deadAt.x, q.deadAt.y, a.x, a.y))[0]
      if (!dead) return false
      dead.resT = 1.2
      ringFx(dead.deadAt.x, dead.deadAt.y, 18, [0.4, 1, 0.9])
      return true
    },
  },
  reyna: {
    C(a, tx, ty) {
      const p = throwTo(a, tx, ty, 110)
      const s = take('proj')
      if (!s) return false
      s.r.fillColor = [0.75, 0.3, 1, 1]
      s.r.x = a.x; s.r.y = a.y
      spawnEnt({ kind: 'leer', team: a.team, x: a.x, y: a.y, tx: p.x, ty: p.y, life: 2.6, age: 0, hp: 40, slots: [['proj', s.k]] })
      return true
    },
    Q(a) {
      if (a.soulT <= 0 || a.hp >= 100) return false
      a.healLeft += 60
      a.soulT = 0
      ringFx(a.x, a.y, 10, [0.8, 0.3, 1])
      return true
    },
    E(a) {
      if (a.soulT <= 0) return false
      a.dismissT = 2
      a.soulT = 0
      return true
    },
    X(a) { a.empressT = 30; ringFx(a.x, a.y, 20, [0.8, 0.3, 1]); return true },
  },
  sova: {
    C(a, tx, ty) {
      const s = take('proj')
      if (!s) return false
      s.r.fillColor = [0.4, 0.7, 1, 1]
      const ang = angleTo(a.x, a.y, tx, ty)
      spawnEnt({ kind: 'drone', team: a.team, owner: a, x: a.x, y: a.y, ang, speed: 120, travel: 170, life: 3.5, age: 0, slots: [['proj', s.k]] })
      return true
    },
    Q(a, tx, ty) {
      const s = take('proj')
      if (!s) return false
      s.r.fillColor = [0.5, 0.85, 1, 1]
      const ang = angleTo(a.x, a.y, tx, ty)
      spawnEnt({ kind: 'shock', team: a.team, owner: a, x: a.x, y: a.y, vx: Math.cos(ang) * 230, vy: Math.sin(ang) * 230, bounces: 1, life: 0.9, age: 0, slots: [['proj', s.k]] })
      return true
    },
    E(a, tx, ty) {
      const s = take('proj')
      if (!s) return false
      s.r.fillColor = [0.3, 0.6, 1, 1]
      const ang = angleTo(a.x, a.y, tx, ty)
      spawnEnt({ kind: 'recon', team: a.team, x: a.x, y: a.y, vx: Math.cos(ang) * 280, vy: Math.sin(ang) * 280, stuck: false, life: 4, age: 0, pulses: 0, slots: [['proj', s.k]] })
      return true
    },
    X(a) { spawnEnt({ kind: 'fury', team: a.team, owner: a, x: a.x, y: a.y, life: 2.3, age: 0, shots: 0 }); return true },
  },
  kj: {
    C(a, tx, ty) {
      const p = throwTo(a, tx, ty, 110)
      const s = take('device')
      if (!s) return false
      s.r.x = p.x; s.r.y = p.y
      s.r.fillColor = [1, 0.75, 0.1, 1]
      s.r.scaleX = 0.6; s.r.scaleY = 0.6
      spawnEnt({ kind: 'nanoswarm', team: a.team, x: p.x, y: p.y, life: 999, age: 0, armed: true, slots: [['device', s.k]] })
      return true
    },
    Q(a, tx, ty) {
      const p = throwTo(a, tx, ty, 40)
      const s = take('device')
      if (!s) return false
      s.r.x = p.x; s.r.y = p.y; s.r.fillColor = [1, 0.95, 0.4, 1]; s.r.scaleX = 0.8; s.r.scaleY = 0.8
      spawnEnt({ kind: 'alarm', team: a.team, x: p.x, y: p.y, life: 999, age: 0, hp: 40, slots: [['device', s.k]] })
      return true
    },
    E(a, tx, ty) {
      const p = throwTo(a, tx, ty, 30)
      const s = take('device')
      if (!s) return false
      s.r.x = p.x; s.r.y = p.y; s.r.fillColor = [1, 0.86, 0.2, 1]; s.r.scaleX = 1.2; s.r.scaleY = 1.2
      spawnEnt({ kind: 'turret', team: a.team, x: p.x, y: p.y, facing: p.ang, life: 999, age: 0, hp: 120, cd: 0, slots: [['device', s.k]] })
      return true
    },
    X(a) {
      const s = take('device'), ring = take('ring')
      if (!s || !ring) return false
      s.r.x = a.x; s.r.y = a.y; s.r.fillColor = [1, 0.95, 0.6, 1]; s.r.scaleX = 1.4; s.r.scaleY = 1.4
      ring.r.x = a.x; ring.r.y = a.y
      spawnEnt({ kind: 'lockdown', team: a.team, x: a.x, y: a.y, life: 9, age: 0, hp: 150, slots: [['device', s.k], ['ring', ring.k]] })
      return true
    },
  },
  jett: {
    C(a, tx, ty) {
      const p = throwTo(a, tx, ty, 170)
      const s = take('smoke')
      if (!s) return false
      s.r.x = p.x; s.r.y = p.y
      spawnEnt({ kind: 'smoke', team: a.team, x: p.x, y: p.y, r: 24, life: 7, age: 0, slots: [['smoke', s.k]] })
      return true
    },
    Q(a) { a.updraftT = 0.7; ringFx(a.x, a.y, 8, [0.8, 0.95, 1]); return true },
    E(a, tx, ty) {
      let dx = 0, dy = 0
      if (a.moveX || a.moveY) { dx = a.moveX; dy = a.moveY } else { dx = Math.cos(a.angle); dy = Math.sin(a.angle) }
      const len = Math.hypot(dx, dy) || 1
      a.dash = { vx: (dx / len) * 520, vy: (dy / len) * 520, t: 0.13 }
      return true
    },
    X(a) { a.knives = 5; ringFx(a.x, a.y, 14, [0.8, 0.95, 1]); return true },
  },
}

function hitWall(cell, amount) {
  g.dyn[cell] -= amount
  if (g.dyn[cell] > 0) return
  g.dyn[cell] = 0
  for (const e of g.ents) if (e.kind === 'barrier' && e.cells.includes(cell)) {
    const k = e.cells.indexOf(cell)
    release(e.slots[k][0], e.slots[k][1])
    e.cells.splice(k, 1); e.slots.splice(k, 1)
    if (!e.cells.length) e.dead = true
  }
}

/** Per-frame update of every ability object. */
function stepEnts() {
  for (const e of g.ents) {
    if (e.dead) continue
    e.age += dt
    const s = e.slots && e.slots[0] ? ref(e.slots[0][0] + e.slots[0][1]) : null
    switch (e.kind) {
      case 'tracer': if (e.age >= e.life) despawnEnt(e); break
      case 'ringfx': {
        const k = e.age / e.life
        const r = ref('ring' + e.slots[0][1])
        const d = Math.max(2, e.radius * 2 * k)
        r.scaleX = d / 2; r.scaleY = d / 2
        if (e.age >= e.life) despawnEnt(e)
        break
      }
      case 'smoke': {
        const k = Math.min(1, e.age / 0.3) * Math.min(1, (e.life - e.age) / 0.5)
        s.scaleX = s.scaleY = Math.max(0.05, k)
        e.r = 24 * k
        if (e.age >= e.life) despawnEnt(e)
        break
      }
      case 'barrier': if (e.age >= e.life) despawnEnt(e); break
      case 'slow':
        for (const o of g.agents) if (o.alive && o.team !== e.team && dist(o.x, o.y, e.x, e.y) < e.r) o.slowT = 0.3
        if (e.age >= e.life) despawnEnt(e)
        break
      case 'leer': {
        const k = Math.min(1, e.age / 0.5)
        e.x += (e.tx - e.x) * Math.min(1, dt * 6); e.y += (e.ty - e.y) * Math.min(1, dt * 6)
        s.x = e.x; s.y = e.y
        s.scaleX = s.scaleY = 1 + Math.sin(T * 10) * 0.2
        if (k >= 1) for (const o of g.agents) if (o.alive && o.team !== e.team && canSee(o, { x: e.x, y: e.y, alive: true }, 1.6)) o.nearT = 0.6
        if (e.age >= e.life) despawnEnt(e)
        break
      }
      case 'drone': {
        if (e.travel > 0) {
          const step = e.speed * dt
          const nx = e.x + Math.cos(e.ang) * step, ny = e.y + Math.sin(e.ang) * step
          if (solidAt(nx, ny)) e.travel = 0; else { e.x = nx; e.y = ny; e.travel -= step }
        }
        s.x = e.x; s.y = e.y
        revealTeam(1 - e.team, e.x, e.y, 70, 0.4)
        if (e.age >= e.life) despawnEnt(e)
        break
      }
      case 'shock': {
        let nx = e.x + e.vx * dt, ny = e.y + e.vy * dt
        if (solidAt(nx, ny)) {
          if (e.bounces > 0) {
            e.bounces--
            if (solidAt(nx, e.y)) e.vx = -e.vx
            if (solidAt(e.x, ny)) e.vy = -e.vy
            e.vx *= 0.7; e.vy *= 0.7
            nx = e.x; ny = e.y
          } else { e.age = e.life }
        }
        e.x = nx; e.y = ny
        s.x = e.x; s.y = e.y
        if (e.age >= e.life) {
          ringFx(e.x, e.y, 26, [0.5, 0.85, 1])
          for (const o of g.agents) if (o.alive && o.team !== e.team && dist(o.x, o.y, e.x, e.y) < 26 && !blocked(e.x, e.y, o.x, o.y, false)) damage(o, 75, e.owner, '(SHOCK)')
          despawnEnt(e)
        }
        break
      }
      case 'recon': {
        if (!e.stuck) {
          const nx = e.x + e.vx * dt, ny = e.y + e.vy * dt
          if (solidAt(nx, ny) || e.age > 1.4) { e.stuck = true; e.age = 0 } else { e.x = nx; e.y = ny }
        } else {
          const pulses = e.age > 0.4 ? (e.age > 1.9 ? 2 : 1) : 0
          if (pulses > e.pulses) { e.pulses = pulses; ringFx(e.x, e.y, 100, [0.4, 0.7, 1]); revealTeam(1 - e.team, e.x, e.y, 110, 2.5) }
          if (e.age > 2.6) despawnEnt(e)
        }
        s.x = e.x; s.y = e.y
        break
      }
      case 'fury': {
        const due = Math.floor(e.age / 0.75)
        if (due > e.shots && e.shots < 3 && e.owner.alive) {
          e.shots++
          const o = e.owner, ang = o.angle
          const b = take('beam')
          if (b) {
            b.r.x = o.x + Math.cos(ang) * 210; b.r.y = o.y + Math.sin(ang) * 210
            b.r.rotation = (ang * 180) / Math.PI; b.r.scaleX = 420
            spawnEnt({ kind: 'beamfx', x: o.x, y: o.y, life: 0.35, age: 0, slots: [['beam', b.k]] })
          }
          for (const t of g.agents) {
            if (!t.alive || t.team === o.team) continue
            const ox = t.x - o.x, oy = t.y - o.y
            const along = ox * Math.cos(ang) + oy * Math.sin(ang)
            if (along > 0 && along < 420 && Math.abs(ox * Math.sin(ang) - oy * Math.cos(ang)) < 6) { damage(t, 80, o, '(FURY)'); g.reveal[t.i] = 2; note(o.team, t) }
          }
        }
        if (e.age >= e.life) despawnEnt(e)
        break
      }
      case 'beamfx': {
        const r = ref('beam' + e.slots[0][1])
        r.fillColor = [0.4, 0.75, 1, 0.8 * (1 - e.age / e.life)]
        if (e.age >= e.life) despawnEnt(e)
        break
      }
      case 'nanoswarm': {
        // Hidden from enemies until it goes off.
        s.visible = e.team === MY || !e.armed
        if (e.armed && g.agents.some((o) => o.alive && o.team !== e.team && dist(o.x, o.y, e.x, e.y) < 24)) {
          e.armed = false
          e.age = 0
          const sw = take('swarm')
          if (sw) { sw.r.x = e.x; sw.r.y = e.y; e.slots.push(['swarm', sw.k]) }
        }
        if (!e.armed) {
          for (const o of g.agents) if (o.alive && o.team !== e.team && dist(o.x, o.y, e.x, e.y) < 28) damage(o, 38 * dt, null, '(SWARM)')
          if (e.age > 4) despawnEnt(e)
        }
        break
      }
      case 'alarm': {
        const hit = g.agents.find((o) => o.alive && o.team !== e.team && dist(o.x, o.y, e.x, e.y) < 46 && !blocked(e.x, e.y, o.x, o.y))
        if (hit) { hit.vulnT = 4; g.reveal[hit.i] = 4; note(e.team, hit); ringFx(e.x, e.y, 46, [1, 0.9, 0.3]); despawnEnt(e) }
        break
      }
      case 'turret': {
        e.cd -= dt
        const t = g.agents
          .filter((o) => o.alive && o.team !== e.team && o.dismissT <= 0 && dist(o.x, o.y, e.x, e.y) < 170 && Math.abs(wrap(angleTo(e.x, e.y, o.x, o.y) - e.facing)) < 0.9 && !blocked(e.x, e.y, o.x, o.y))
          .sort((p, q) => dist(p.x, p.y, e.x, e.y) - dist(q.x, q.y, e.x, e.y))[0]
        if (t && e.cd <= 0) {
          e.cd = 0.22
          tracer(e.x, e.y, t.x + rand(-3, 3), t.y + rand(-3, 3), [1, 0.85, 0.3, 0.8])
          if (Math.random() < 0.75) damage(t, 7, null, '(TURRET)')
          note(e.team, t)
        }
        break
      }
      case 'lockdown': {
        const k = e.age / e.life
        const r = ref('ring' + e.slots[1][1])
        r.scaleX = r.scaleY = Math.max(1, 130 * k)
        if (e.age >= e.life) {
          for (const o of g.agents) if (o.alive && o.team !== e.team && dist(o.x, o.y, e.x, e.y) < 130) { o.detainT = 6; if (o.isPlayer) message('DETAINED', 'LOCKDOWN', 2) }
          ringFx(e.x, e.y, 130, [1, 0.95, 0.6])
          despawnEnt(e)
        }
        break
      }
    }
  }
  g.ents = g.ents.filter((e) => !e.dead)
}

// --------------------------------------------------------- movement --
function moveAgent(a, vx, vy) {
  const r = 3.2
  const free = (x, y) => !solidAt(x - r, y - r) && !solidAt(x + r, y - r) && !solidAt(x - r, y + r) && !solidAt(x + r, y + r)
  const nx = a.x + vx * dt
  if (free(nx, a.y)) a.x = nx
  const ny = a.y + vy * dt
  if (free(a.x, ny)) a.y = ny
  // During the buy phase, attackers stay behind their spawn barrier.
  if (g.phase === 'buy' && isAttacker(a)) {
    const spawn = P.atkSpawn
    if (spawn.y < 0 ? a.y > spawn.y + 70 : a.y < spawn.y - 70) a.y = spawn.y + (spawn.y < 0 ? 70 : -70)
  }
}

function speedOf(a, walking) {
  if (a.detainT > 0) return 0
  let s = walking ? 46 : 74
  if (a.slowT > 0 && a.updraftT <= 0) s *= 0.5
  if (a.updraftT > 0) s *= 1.7
  if (a.dismissT > 0) s *= 1.6
  return s
}

/** Common per-frame agent upkeep. */
function stepAgent(a) {
  for (const k of ['slowT', 'detainT', 'nearT', 'vulnT', 'dismissT', 'updraftT', 'empressT', 'soulT', 'fireCd']) a[k] = Math.max(0, a[k] - dt)
  a.recoil = Math.max(0, a.recoil - dt * 0.12)
  if (a.reloadT > 0) { a.reloadT -= dt; if (a.reloadT <= 0) a.ammo = a.mag }
  if (a.healLeft > 0) {
    const h = Math.min(a.healLeft, 20 * dt)
    a.healLeft -= h
    if (a.hp < 100) a.hp = Math.min(100, a.hp + h); else a.armor = Math.min(50, a.armor + h)
  }
  if (a.dash) {
    moveAgent(a, a.dash.vx, a.dash.vy)
    a.dash.t -= dt
    if (a.dash.t <= 0) a.dash = null
  }
  if (a.resT > 0) {
    a.resT -= dt
    if (a.resT <= 0 && !a.alive && a.deadAt) { a.alive = true; a.hp = 100; a.armor = 0; a.x = a.deadAt.x; a.y = a.deadAt.y; feed(a.kit.name + ' RESURRECTED', a.team === MY) }
  }
}

// ------------------------------------------------------- the spike --
function stepSpike() {
  const sp = g.spike
  if (sp.state === 'dropped') {
    // An attacker walking over it picks it up.
    const picker = g.agents.find((a) => a.alive && isAttacker(a) && dist(a.x, a.y, sp.x, sp.y) < 8)
    if (picker) { sp.state = 'carried'; sp.carrier = picker; if (picker.isPlayer) message('', 'YOU HAVE THE SPIKE', 1.5) }
  }
  if (sp.state === 'carried' && sp.carrier) { sp.x = sp.carrier.x; sp.y = sp.carrier.y }
  if (sp.state === 'planted') {
    sp.timer -= dt
    if (sp.timer <= 0) { sp.state = 'exploded'; ringFx(sp.x, sp.y, 120, [1, 0.4, 0.2]); g.hitFlash = 0.6; endRound(g.attackers, 'SPIKE DETONATED') }
  }
}
function plantProgress(a, holding) {
  const sp = g.spike
  if (!holding || sp.state !== 'carried' || sp.carrier !== a || !siteAt(a.x, a.y) || a.moving) { if (sp.carrier === a) sp.prog = 0; return }
  sp.prog += dt
  if (sp.prog >= PLANT_TIME) {
    sp.state = 'planted'; sp.carrier = null; sp.timer = SPIKE_TIME; sp.site = siteAt(a.x, a.y); sp.x = a.x; sp.y = a.y; sp.prog = 0
    g.phase = 'planted'
    a.ult = Math.min(a.kit.ultCost, a.ult + 1)
    message('SPIKE PLANTED', 'SITE ' + sp.site, 2.2)
    for (const team of [MY, ENEMY]) g.known[team].length = 0
  }
}
function defuseProgress(a, holding) {
  const sp = g.spike
  if (sp.state !== 'planted' || isAttacker(a)) return
  if (!holding || dist(a.x, a.y, sp.x, sp.y) > 12 || a.moving) { if (sp.defuser === a) { sp.defuser = null; sp.prog = sp.prog >= DEFUSE_TIME / 2 ? DEFUSE_TIME / 2 : 0 } return }
  sp.defuser = a
  sp.prog += dt
  if (sp.prog >= DEFUSE_TIME) { sp.state = 'defused'; a.ult = Math.min(a.kit.ultCost, a.ult + 1); endRound(a.team, 'SPIKE DEFUSED') }
}

// ------------------------------------------------------- the player --
function playerControl(a) {
  let mx = ctx.mouseX, my = ctx.mouseY
  let dx = 0, dy = 0
  if (fps3d()) {
    // First person: there's no pointer lock, so the mouse steers like a
    // joystick — the further it sits left/right of centre, the faster you
    // turn (dead zone in the middle). The arrow keys turn too.
    // With pointer lock (click the game once), it's real mouse-look instead.
    if (!ctx.pointerLocked && ctx.requestPointerLock) ctx.requestPointerLock()
    const sx = ctx.mouseX - ref('camera').x
    // Mouse-look speed scales with the MOUSE SENS setting (45% default).
    const sens = (S.settings.sens || 45) / 100
    let turn = ctx.pointerLocked ? -(ctx.mouseDX || 0) * 0.0045 * sens / Math.max(dt, 1e-3)
      : Math.abs(sx) > 18 ? -Math.sign(sx) * ((Math.abs(sx) - 18) / 180) * 4.2 : 0
    if (keys.has('left')) turn += 2.8
    if (keys.has('right')) turn -= 2.8
    a.angle = a.aim = wrap(a.angle + turn * dt)
    if (ctx.pointerLocked) g.pitch3 = clip((g.pitch3 || 0) - (ctx.mouseDY || 0) * 0.258 * sens, -70, 70)
    // WASD: forward/back along the facing, strafe across it.
    const f = (keys.has('w') || keys.has('up') ? 1 : 0) - (keys.has('s') || keys.has('down') ? 1 : 0)
    const st = (keys.has('d') ? 1 : 0) - (keys.has('a') ? 1 : 0)
    const c = Math.cos(a.angle), sn = Math.sin(a.angle)
    dx = c * f + sn * st
    dy = sn * f - c * st
    // Abilities aim at whatever's straight ahead.
    const hit = rayPoint(a.x, a.y, a.angle, 160)
    mx = hit.x; my = hit.y
  } else {
    a.angle = a.aim = angleTo(a.x, a.y, mx, my)
    if (keys.has('a') || keys.has('left')) dx -= 1
    if (keys.has('d') || keys.has('right')) dx += 1
    if (keys.has('w') || keys.has('up')) dy += 1
    if (keys.has('s') || keys.has('down')) dy -= 1
  }
  const len = Math.hypot(dx, dy) || 1
  a.moveX = dx; a.moveY = dy
  const holdingF = keys.has('f')
  const busy = holdingF && ((g.spike.state === 'carried' && g.spike.carrier === a && siteAt(a.x, a.y)) || (g.spike.state === 'planted' && !isAttacker(a) && dist(a.x, a.y, g.spike.x, g.spike.y) < 12))
  const sp = busy ? 0 : speedOf(a, keys.has('shift'))
  a.moving = (dx !== 0 || dy !== 0) && sp > 0
  if (a.moving) moveAgent(a, (dx / len) * sp, (dy / len) * sp)
  if (keys.has('mouse')) fire(a)
  if (pressed('r')) reload(a)
  for (const k of ['c', 'q', 'e', 'x']) if (pressed(k)) cast(a, k.toUpperCase(), mx, my)
  plantProgress(a, holdingF)
  defuseProgress(a, holdingF)
}

// ---------------------------------------------------------- the bots --
/** The nearest walkable spot to (x, y) (goals near walls snap onto floor). */
function nearestWalkable(x, y) {
  const cx = cellX(x), cy = cellY(y)
  for (let r = 0; r <= 4; r++) for (let dy = -r; dy <= r; dy++) for (let dx = -r; dx <= r; dx++) {
    if (Math.max(Math.abs(dx), Math.abs(dy)) !== r) continue
    const nx = cx + dx, ny = cy + dy
    if (nx >= 0 && ny >= 0 && nx < GW && ny < GH && walkable(ny * GW + nx)) return { x: centerX(nx), y: centerY(ny) }
  }
  return { x, y }
}
function goTo(a, x, y) {
  if (a.goal && dist(a.goal.x, a.goal.y, x, y) < 6 && (a.path || a.goalFailT > T)) return
  const spot = nearestWalkable(x, y)
  a.goal = { x: spot.x, y: spot.y }
  a.path = astar(a.x, a.y, spot.x, spot.y)
  a.pathI = 0
  // Unreachable right now (a Sage wall?): don't re-search every frame.
  if (!a.path) a.goalFailT = T + 1
}
function followPath(a, walking) {
  if (!a.path || a.pathI >= a.path.length) { a.moving = false; return true }
  // Skip waypoints we can see past.
  while (a.pathI + 1 < a.path.length && a.pathI < a.path.length - 1 && !blocked(a.x, a.y, a.path[a.pathI + 1].x, a.path[a.pathI + 1].y, false) && dist(a.x, a.y, a.path[a.pathI + 1].x, a.path[a.pathI + 1].y) < 60) a.pathI++
  const w = a.path[a.pathI]
  const d = dist(a.x, a.y, w.x, w.y)
  if (d < 3) { a.pathI++; return a.pathI >= a.path.length }
  const sp = speedOf(a, walking)
  const vx = ((w.x - a.x) / d) * sp, vy = ((w.y - a.y) / d) * sp
  a.moveX = vx; a.moveY = vy
  const before = { x: a.x, y: a.y }
  moveAgent(a, vx, vy)
  a.moving = sp > 0
  if (!a.target) a.angle = turnToward(a.angle, Math.atan2(vy, vx), 7 * dt)
  if (dist(a.x, a.y, before.x, before.y) < sp * dt * 0.2) { a.stuck = (a.stuck || 0) + dt; if (a.stuck > 0.6) { a.stuck = 0; a.path = astar(a.x, a.y, a.goal.x, a.goal.y); a.pathI = 0 } }
  return false
}
function turnToward(cur, want, maxStep) {
  const d = wrap(want - cur)
  return cur + clip(d, -maxStep, maxStep)
}

/** The bot's current objective point. */
function objective(a) {
  const sp = g.spike
  const tr = a.traits
  const elapsed = ROUND_TIME - g.time
  // The spike carrier is on plant duty: no chasing, no backing off, no lurking.
  const carrying = sp.state === 'carried' && sp.carrier === a
  if (carrying) a.lurking = false
  // Personal overrides: chasing a fresh lead, or backing off when hurt.
  if (!carrying && a.retreatUntil > T && a.retreatTo) return { x: a.retreatTo.x, y: a.retreatTo.y, walk: false }
  // A chase ends on arrival, and never trumps a loose or planted spike.
  if (a.huntPos && dist(a.x, a.y, a.huntPos.x, a.huntPos.y) < 10) a.huntUntil = 0
  if (!carrying && a.huntUntil > T && a.huntPos && sp.state === 'carried') return { x: a.huntPos.x, y: a.huntPos.y, walk: tr.patience > 0.6 }
  if (isAttacker(a)) {
    if (sp.state === 'dropped') {
      // The nearest attacker grabs it; the rest keep playing.
      const nearest = alliesOf(a).concat([a]).filter((o) => o.alive && !o.isPlayer).sort((p, q) => dist(p.x, p.y, sp.x, sp.y) - dist(q.x, q.y, sp.x, sp.y))[0]
      if (nearest === a) return { x: sp.x, y: sp.y, walk: false }
    }
    if (sp.state === 'planted') {
      // Post-plant: each picks their own cover angle around the spike.
      const off = a.i * 1.3 + a.spotOff.x * 0.05
      const r = 26 + tr.patience * 22
      return { x: sp.x + Math.cos(off) * r, y: sp.y + Math.sin(off) * r, walk: true, guard: true }
    }
    const site = g.plan.show
    const route = ROUTES[site][(a.route && a.route[site]) || 'main']
    // Lurkers hold a flank spot on the other side of the map until the plant.
    if (a.lurking) {
      const lurk = g.plan.site === 'A' ? P.lurkA : P.lurkB
      return { x: lurk.x, y: lurk.y, walk: true, hold: true }
    }
    if (!g.plan.executing) return { x: route.stage.x + a.spotOff.x * 0.5, y: route.stage.y + a.spotOff.y * 0.5, walk: tr.walker }
    // Late in the round: forget the plan, the carrier goes straight to plant.
    if (carrying && g.time < 30) { const real = g.plan.site; return { x: real === 'A' ? P.plantA.x : P.plantB.x, y: real === 'A' ? P.plantA.y : P.plantB.y, walk: false, plant: true } }
    // Patient players give the entry a beat before following.
    if (elapsed < g.plan.executeAt + a.readyT && sp.carrier !== a) return { x: route.stage.x, y: route.stage.y, walk: true }
    const real = g.plan.site
    if (sp.carrier === a && site === real) return { x: real === 'A' ? P.plantA.x : P.plantB.x, y: real === 'A' ? P.plantA.y : P.plantB.y, walk: false, plant: true }
    // Supports trade: stay a step behind the teammate closest to the site.
    if (tr.trade) {
      const center = site === 'A' ? P.siteA : P.siteB
      const lead = alliesOf(a).filter((o) => !o.isPlayer || true).sort((p, q) => dist(p.x, p.y, center.x, center.y) - dist(q.x, q.y, center.x, center.y))[0]
      if (lead && dist(lead.x, lead.y, center.x, center.y) < dist(a.x, a.y, center.x, center.y) - 20) {
        const back = angleTo(center.x, center.y, lead.x, lead.y)
        return { x: lead.x + Math.cos(back) * 16, y: lead.y + Math.sin(back) * 16, walk: false }
      }
    }
    const entry = route.entry
    const center = site === 'A' ? P.siteA : P.siteB
    if (dist(a.x, a.y, entry.x, entry.y) < 20) a.cleared = true
    if (a.cleared) return { x: center.x + a.spotOff.x, y: center.y + a.spotOff.y, walk: false }
    // Walkers go quiet on the last stretch, unless it's a rush.
    const quiet = tr.walker && g.plan.strategy !== 'rush' && dist(a.x, a.y, entry.x, entry.y) < 110
    return { x: entry.x, y: entry.y, walk: quiet }
  }
  // Defenders.
  if (sp.state === 'planted') {
    // The nearest defender defuses; the others cover from around the spike.
    const nearest = alliesOf(a).concat([a]).filter((o) => o.alive).sort((p, q) => dist(p.x, p.y, sp.x, sp.y) - dist(q.x, q.y, sp.x, sp.y))[0]
    if (nearest === a) return { x: sp.x, y: sp.y, walk: false, defuse: true }
    const off = a.i * 1.7
    return { x: sp.x + Math.cos(off) * 40, y: sp.y + Math.sin(off) * 40, walk: false, guard: true }
  }
  if (a.pushSpot && elapsed < a.pushUntil && g.phase === 'live') return { x: a.pushSpot.x, y: a.pushSpot.y, walk: tr.patience > 0.5, hold: true, push: true }
  return { x: a.hold.x, y: a.hold.y, walk: true, hold: true }
}

function botThink(a) {
  // Targets: what I can see, else what my team knows.
  const visible = enemiesOf(a).filter((o) => o.dismissT <= 0 && canSee(a, o))
  for (const o of visible) note(a.team, o)
  const best = visible.sort((p, q) => dist(a.x, a.y, p.x, p.y) - dist(a.x, a.y, q.x, q.y))[0] || null
  const tr = a.traits
  if (best && best !== a.target) {
    a.target = best
    a.reactT = rand(tr.reaction[0], tr.reaction[1]) * (a.nearT > 0 ? 2 : 1)
    // Callouts from your teammates: "BLITZ: 2 ENEMIES B MAIN".
    if (a.team === MY && T - (g.calloutT || 0) > 2.2 && !g.known[MY].some((k) => k.i === best.i && T - k.t < 4)) {
      g.calloutT = T
      const area = CALLOUT_AREAS.reduce((p, q) => (dist(q.x, q.y, best.x, best.y) < dist(p.x, p.y, best.x, best.y) ? q : p))
      const count = visible.length
      feed(a.callsign + ': ' + (count > 1 ? count + ' ENEMIES ' : 'ENEMY ') + area.name, true)
    }
  }
  if (!best && a.target) {
    // Lost them: hunters chase where they were, the hurt back off.
    const lost = a.target
    if (Math.random() < tr.chase && a.hp > 40 && T > (a.nextHuntT || 0)) {
      a.huntUntil = T + rand(1.5, 3.5) * (0.5 + tr.aggression)
      a.huntPos = { x: lost.x, y: lost.y }
      a.nextHuntT = T + rand(5, 9)
    }
    a.target = null
  }
  if (best && a.hp < 38 && tr.aggression < 0.55 && dist(a.x, a.y, best.x, best.y) < 90 && Math.random() < 0.4) {
    const away = angleTo(best.x, best.y, a.x, a.y)
    a.retreatTo = nearestWalkable(a.x + Math.cos(away) * 50, a.y + Math.sin(away) * 50)
    a.retreatUntil = T + 1.4
  }
  // Rotations: defenders shift toward a site once attackers commit there.
  if (!isAttacker(a) && g.spike.state !== 'planted') {
    const known = g.known[a.team].filter((k) => T - k.t < 6)
    const nearA = known.filter((k) => dist(k.x, k.y, P.siteA.x, P.siteA.y) < 120).length
    const nearB = known.filter((k) => dist(k.x, k.y, P.siteB.x, P.siteB.y) < 120).length
    // Eager rotators move on a single sighting; careful ones wait for two.
    const need = tr.rotate > 0.65 ? 1 : 2
    const rotateTo = (site) => { const h = HOLDS[site][a.i % HOLDS[site].length]; a.hold = h.p; a.face = h.face; a.pushSpot = null }
    if (a.home !== 'A' && nearA >= need && nearA > nearB && a.rotated !== 'A') { a.rotated = 'A'; rotateTo('A') }
    if (a.home !== 'B' && nearB >= need && nearB > nearA && a.rotated !== 'B') { a.rotated = 'B'; rotateTo('B') }
  }
  botUtility(a, visible)
}

function botUtility(a, visible) {
  a.utilCd -= 1 / 8
  if (a.utilCd > 0) return
  // Utility-shy bots often just... don't.
  if (Math.random() > 0.15 + a.traits.util * 0.85) { a.utilCd = rand(0.6, 2); return }
  const used = (slot, x, y) => { if (cast(a, slot, x, y)) { a.utilCd = rand(1.5, 3); return true } return false }
  const atk = isAttacker(a)
  const site = g.plan.show
  const sp = g.spike
  const siteP = site === 'A' ? P.siteA : P.siteB
  const chokes = site === 'A' ? [P.chokeA1, P.chokeA2] : [P.chokeB1, P.chokeB2]
  const nearSite = (pt, r) => dist(a.x, a.y, pt.x, pt.y) < r
  const target = visible[0]
  const known = g.known[a.team].filter((k) => T - k.t < 3)
  const lowHp = a.hp < 45
  switch (a.kind) {
    case 'jett':
      if (atk && (g.plan.executing || g.plan.faking) && nearSite(siteP, 170) && sp.state !== 'planted') { for (const c of chokes) if (used('C', c.x, c.y)) return }
      if (!atk && target && dist(a.x, a.y, target.x, target.y) > 60 && a.charges.C > 1) { if (used('C', (a.x + target.x) / 2, (a.y + target.y) / 2)) return }
      if (target && a.ult >= a.kit.ultCost && a.knives <= 0) { if (used('X')) return }
      if (lowHp && target) { if (used('E', a.x - (target.x - a.x), a.y - (target.y - a.y))) return }
      if (atk && g.plan.executing && nearSite(siteP, 90)) used('Q')
      break
    case 'sage':
      // Defense: wall off the site's main entrance once in position.
      if (!atk && !a.setupDone && a.home !== 'Mid' && a.hold && dist(a.x, a.y, a.hold.x, a.hold.y) < 16) {
        const entry = a.home === 'A' ? P.entryA : P.entryB
        a.setupDone = true
        if (used('C', entry.x, entry.y)) return
      }
      if (target && dist(a.x, a.y, target.x, target.y) > 45) { if (used('Q', target.x, target.y)) return }
      if (!target) {
        const hurt = alliesOf(a).filter((o) => o.hp < 55 && dist(o.x, o.y, a.x, a.y) < 50).sort((p, q) => p.hp - q.hp)[0]
        if (a.hp < 60 && used('E', a.x, a.y)) return
        if (hurt && used('E', hurt.x, hurt.y)) return
        if (a.ult >= a.kit.ultCost && used('X')) return
      }
      break
    case 'sova':
      if (atk && !g.plan.executing && g.phase === 'live' && a.goal && dist(a.x, a.y, a.goal.x, a.goal.y) < 30) { if (used('E', siteP.x, siteP.y)) return; if (used('C', siteP.x, siteP.y)) return }
      if (!atk && g.phase === 'live' && ROUND_TIME - g.time > 12 && a.hold && dist(a.x, a.y, a.hold.x, a.hold.y) < 20) { const e = a.home === 'A' ? P.entryA : P.entryB; if (a.home !== 'Mid' && used('E', e.x, e.y)) return }
      {
        // Shock Bolt: a revealed / recently spotted enemy, or one in view
        // at range (the blast catches people around corners too).
        const r = known.find((k) => dist(a.x, a.y, k.x, k.y) < 200 && dist(a.x, a.y, k.x, k.y) > 40) || (target && dist(a.x, a.y, target.x, target.y) > 60 ? target : null)
        if (r && used('Q', r.x, r.y)) return
        // Hunter's Fury: anyone revealed, or a known enemy within range.
        const fury = g.agents.find((o) => o.alive && o.team !== a.team && g.reveal[o.i] > 0) || known.find((k) => dist(a.x, a.y, k.x, k.y) < 380)
        if (fury && a.ult >= a.kit.ultCost) { a.angle = angleTo(a.x, a.y, fury.x, fury.y); if (used('X')) return }
      }
      break
    case 'kj':
      // Defense: set the site up once in position — turret on the
      // entrance, alarmbot on the approach, a nanoswarm on the plant spot.
      if (!atk && !a.setupDone && a.home !== 'Mid' && a.hold && dist(a.x, a.y, a.hold.x, a.hold.y) < 16) {
        const entry = a.home === 'A' ? P.entryA : P.entryB
        const plant = a.home === 'A' ? P.plantA : P.plantB
        if (cast(a, 'E', entry.x, entry.y) | cast(a, 'Q', (a.x + entry.x) / 2, (a.y + entry.y) / 2) | cast(a, 'C', plant.x, plant.y)) { a.setupDone = true; a.utilCd = 1; return }
        a.setupDone = true
      }
      if (atk && sp.state === 'planted' && dist(a.x, a.y, sp.x, sp.y) < 60) { if (used('C', sp.x, sp.y)) return; if (used('E', sp.x, sp.y)) return }
      if (a.ult >= a.kit.ultCost) {
        const crowd = enemiesOf(a).filter((o) => dist(o.x, o.y, a.x, a.y) < 120).length
        const known2 = known.filter((k) => dist(k.x, k.y, a.x, a.y) < 120).length
        if (crowd + known2 >= 2 || (atk && sp.state === 'planted' && dist(a.x, a.y, sp.x, sp.y) < 50)) { if (used('X')) return }
      }
      break
    case 'reyna':
      if (a.soulT > 0 && a.hp < 75 && used('Q')) return
      if (a.soulT > 0 && a.hp < 35 && used('E')) return
      if (!target) {
        const k = known.find((kk) => dist(a.x, a.y, kk.x, kk.y) < 140)
        if (k && used('C', k.x, k.y)) return
        if (atk && g.plan.executing && nearSite(siteP, 120) && used('C', siteP.x, siteP.y)) return
      } else if (a.ult >= a.kit.ultCost && used('X')) return
      break
  }
}

function botControl(a) {
  if (a.detainT > 0) { a.moving = false; return }
  a.think -= dt
  if (a.think <= 0) { a.think = 1 / 8; botThink(a) }
  const t = a.target
  // Already fighting: keep tracking a little past the vision cone (like
  // you turning your mouse), not all the way around.
  if (t && t.alive && canSee(a, t, 2.4)) {
    // Engage: turn, react, then stop and shoot.
    a.angle = turnToward(a.angle, angleTo(a.x, a.y, t.x, t.y) + Math.sin(T * 3 + a.i) * 0.02 * (2 - a.traits.skill), a.traits.aimSpeed * dt)
    a.reactT -= dt
    a.moving = false
    if (a.reactT <= 0 && Math.abs(wrap(angleTo(a.x, a.y, t.x, t.y) - a.angle)) < 0.08) fire(a)
    return
  }
  if (a.ammo < 8 && a.reloadT <= 0) reload(a)
  const obj = objective(a)
  goTo(a, obj.x, obj.y)
  const arrived = followPath(a, obj.walk)
  // Holding: face where the enemy comes from, scanning a personal arc, and
  // (peekers) now and then shoulder-peek out and back.
  if (arrived) {
    a.moving = false
    const tr = a.traits
    const sweep = 0.25 + (1 - tr.patience) * 0.7
    if (obj.hold) {
      const face = obj.push ? (isAttacker(a) ? P.courtyard : P.atkSpawn) : a.face || (a.lurking ? P.courtyard : P.courtyard)
      a.angle = turnToward(a.angle, angleTo(a.x, a.y, face.x, face.y) + Math.sin(T * tr.lookSpeed + a.i) * sweep, 3 * dt)
    } else if (obj.guard) {
      a.angle = turnToward(a.angle, angleTo(a.x, a.y, g.spike.x, g.spike.y) + Math.PI + Math.sin(T * tr.lookSpeed * 0.8 + a.i) * (0.6 + sweep), 3 * dt)
    }
    if (T > a.peekNext && (obj.hold || obj.guard)) {
      a.peekNext = T + rand(2, 6)
      if (Math.random() < tr.peek) a.peekT = rand(0.25, 0.5) * (Math.random() < 0.5 ? -1 : 1)
    }
    if (a.peekT) {
      const side = a.angle + Math.PI / 2 * Math.sign(a.peekT)
      moveAgent(a, Math.cos(side) * 52, Math.sin(side) * 52)
      a.moving = true
      a.peekT -= dt * Math.sign(a.peekT)
      if (Math.abs(a.peekT) < 0.02) a.peekT = 0
    }
  }
  // Last known enemy info: look toward it.
  const k = g.known[a.team].filter((kk) => T - kk.t < 2).sort((p, q) => dist(a.x, a.y, p.x, p.y) - dist(a.x, a.y, q.x, q.y))[0]
  if (k && dist(a.x, a.y, k.x, k.y) < 160) a.angle = turnToward(a.angle, angleTo(a.x, a.y, k.x, k.y), 5 * dt)
  plantProgress(a, !!obj.plant && arrived)
  defuseProgress(a, !!obj.defuse && arrived && !enemiesOf(a).some((o) => canSee(a, o, 6)))
}

// ----------------------------------------------------- HUD & feed --
function message(main, sub, time) { setText('msg', main); setText('sub', sub || ''); g.msgT = time }
function feed(text, mine) {
  g.feed.unshift({ text, t: 5, mine })
  g.feed = g.feed.slice(0, 4)
}

// ------------------------------------------------------ the 3D view --
// Toggle with V (or on the Select screen). The match simulation is the same
// either way (it's 2D, on the Sunset grid); the 3D view draws it through the
// scene's 3D twin (`demo_tactics_3d.rs`): walls extruded from the grid,
// agents as figures, abilities mirrored from their 2D pools — in first
// person, from whoever you're watching. 1 cell (8 px) = 1 m, and 2D (x, y)
// maps to 3D (x, ?, -y) / 8.
const fps3d = () => S.settings.view === '3d'
const RC_FAR = 420                                    // ray range (px) for aiming through the grid
const M = 1 / CELL                                    // px -> meters
const EYE_H = 1.55                                    // eye height (m)

/** First wall hit along a ray (DDA through the grid): distance + which face. */
function rcRay(ox, oy, ang) {
  // Grid space: x right, y DOWN (cell rows), in cells.
  const px = (ox - ORIGIN_X) / CELL, py = (ORIGIN_Y - oy) / CELL
  const dx = Math.cos(ang), dy = -Math.sin(ang)
  let mx = Math.floor(px), my = Math.floor(py)
  const ddx = dx === 0 ? 1e9 : Math.abs(1 / dx), ddy = dy === 0 ? 1e9 : Math.abs(1 / dy)
  const stx = dx < 0 ? -1 : 1, sty = dy < 0 ? -1 : 1
  let sdx = (dx < 0 ? px - mx : mx + 1 - px) * ddx
  let sdy = (dy < 0 ? py - my : my + 1 - py) * ddy
  const maxCells = RC_FAR / CELL
  for (let n = 0; n < 200; n++) {
    let side
    if (sdx < sdy) { sdx += ddx; mx += stx; side = 0 } else { sdy += ddy; my += sty; side = 1 }
    const d = side === 0 ? sdx - ddx : sdy - ddy
    if (d > maxCells) return null
    if (solidCell(mx, my)) {
      const c = cellAt(mx, my)
      return { d: d * CELL, side, kind: c === '#' || c === 'c' ? c : 'dyn' }
    }
  }
  return null
}

/** The point straight ahead, `range` away or at the first wall. */
function rayPoint(x, y, ang, range) {
  const hit = rcRay(x, y, ang)
  const d = hit ? Math.min(range, Math.max(0, hit.d - 4)) : range
  return { x: x + Math.cos(ang) * d, y: y + Math.sin(ang) * d }
}

/** 2D `(x, y)` px at height `h` m -> a 3D position. */
const at3 = (x, y, h) => ({ x: x * M, y: h, z: -y * M })
/** A 2D facing angle (radians, y-up) -> a Node3D yaw (degrees; forward = -Z). */
const yaw3 = (angle) => (angle * 180) / Math.PI - 90

/** Show / hide a node only when it changes (each write is an engine call). */
function vis(r, on) {
  if (r && r.visible !== on) r.visible = on
}

/** Mirrors pooled 2D ability nodes (`smoke3`, `tracer12`…) onto their 3D twins. */
function mirrorPool(name, count, place) {
  for (let k = 0; k < count; k++) {
    const a = ref(name + k), b = ref(name + '3d' + k)
    if (!a || !b) continue
    const on = a.visible
    vis(b, on)
    if (on) place(a, b, k)
  }
}

function drawWorld3D() {
  const v = g.viewer
  // The top-down world is hidden; the 3D twin and the HUD remain.
  show('world2d', false)
  show('globalLight2d', false)
  show('rcCross', v === g.agents[0] && v.alive)

  // Camera: your eyes (or the teammate you're spectating).
  const cam = ref('cam3d')
  const eye = at3(v.x, v.y, v.alive ? EYE_H : 3.0)
  cam.position3 = eye
  cam.rotation3 = { x: v === g.agents[0] ? g.pitch3 || 0 : -6, y: yaw3(v.angle), z: 0 }

  // Agents: animated Kenney characters where they stand.
  for (const a of g.agents) {
    const root = ref('ag3d' + a.i)
    // You don't see yourself; enemies when nothing blocks the line of sight
    // (walls and smokes do — the 3D view itself handles what's on screen).
    const seen = a.team === MY || !a.alive || g.reveal[a.i] > 0 || (dist(v.x, v.y, a.x, a.y) < VISION_3D && !blocked(v.x, v.y, a.x, a.y))
    const on = a !== v && seen && (a.alive || !!a.deadAt)
    vis(root, on)
    if (!on) continue
    const px = a.alive ? a.x : a.deadAt.x, py = a.alive ? a.y : a.deadAt.y
    root.position3 = at3(px, py, 0)
    root.rotation3 = { x: 0, y: yaw3(a.angle), z: 0 }
    const model = ref('ag3d' + a.i + 'Model')
    // Walk while moving (bots: by how far they went since last frame),
    // the rifle up — in the shoot pose right after a shot; dead = fall.
    const moved = a.last3 ? dist(a.last3.x, a.last3.y, a.x, a.y) : 0
    a.last3 = { x: a.x, y: a.y }
    const moving = a.alive && (a.isPlayer && !S.autopilot ? a.moving : moved > 0.05)
    if (!a.alive) model.playAnimation3('die', { loop: false })
    else model.playAnimation3(moving ? 'walk' : 'idle', { overlay: T - (a.lastShot || -9) < 0.15 ? 'holding-right-shoot' : 'holding-right' })
    vis(ref('ag3d' + a.i + 'Gun'), a.alive)
    // The head marker: bigger the farther away, so it stays readable.
    const mark = ref('ag3d' + a.i + 'Mark')
    vis(mark, a.alive)
    if (a.alive) {
      const s3 = clip(dist(v.x, v.y, a.x, a.y) * M / 14, 1, 3.5)
      mark.scale3 = { x: s3, y: s3, z: s3 }
      mark.position3 = { x: 0, y: 2.0 + 0.1 * s3, z: 0 }
    }
    // Hit flash (red tint); fading out while Reyna's Dismiss is up.
    const want = !a.alive ? [0.7, 0.7, 0.72, 1] : a.hitT > 0 ? [1, 0.55, 0.55, 1] : a.dismissT > 0 ? [1, 1, 1, 0.3] : [1, 1, 1, 1]
    const key = want.join(',')
    if (a.colorKey !== key) { model.fillColor = want; a.colorKey = key }
  }

  // First person: your rifle (kicks back on each shot) and the muzzle flash.
  const me = g.agents[0]
  const firstPerson = v === me && me.alive
  const since = T - (me.lastShot || -9)
  vis(ref('viewGun'), firstPerson)
  if (firstPerson) {
    const kick = Math.max(0, 1 - since / 0.08) * 0.05
    const bob = me.moving ? Math.sin(T * 9) * 0.012 : 0
    ref('viewGun').position3 = { x: 0.22, y: -0.2 + bob + kick * 0.3, z: -0.5 + kick }
  }
  vis(ref('muzzleFlash'), firstPerson && since < 0.045)

  // Abilities, mirrored from the 2D pools.
  mirrorPool('smoke', 6, (a, b) => { b.position3 = at3(a.x, a.y, 1.6); b.scale3 = { x: 3.1, y: 2.6, z: 3.1 } })
  mirrorPool('wall', 12, (a, b) => { b.position3 = at3(a.x, a.y, 1.6) })
  for (const name of ['slow', 'swarm']) {
    mirrorPool(name, 4, (a, b) => {
      b.position3 = at3(a.x, a.y, 0.03)
      const r = (a.scaleX || 1) * 30 * M
      b.scale3 = { x: r, y: 1, z: r }
    })
  }
  mirrorPool('proj', 8, (a, b) => { b.position3 = at3(a.x, a.y, 1.2) })
  mirrorPool('device', 4, (a, b) => { b.position3 = at3(a.x, a.y, 0.25); b.rotation3 = { x: 0, y: T * 90, z: 0 } })
  mirrorPool('tracer', 30, (a, b, k) => {
    // A thin bar from muzzle to impact (tilted when the shot climbs / drops).
    const t = (g.tracerH && g.tracerH[k]) || { x0: a.x, y0: a.y, h0: 1.15, x1: a.x, y1: a.y, h1: 1.15 }
    const p0 = at3(t.x0, t.y0, t.h0), p1 = at3(t.x1, t.y1, t.h1)
    const dx = p1.x - p0.x, dy = p1.y - p0.y, dz = p1.z - p0.z
    const flat = Math.hypot(dx, dz)
    b.position3 = { x: (p0.x + p1.x) / 2, y: (p0.y + p1.y) / 2, z: (p0.z + p1.z) / 2 }
    b.rotation3 = { x: 0, y: (Math.atan2(-dz, dx) * 180) / Math.PI, z: (Math.atan2(dy, flat) * 180) / Math.PI }
    b.scale3 = { x: Math.max(0.1, Math.hypot(flat, dy)), y: 0.03, z: 0.03 }
  })
  const sp = g.spike, spike = ref('spike3d')
  const showSpike = sp.state === 'dropped' || sp.state === 'planted'
  vis(spike, showSpike)
  if (showSpike) {
    spike.position3 = at3(sp.x, sp.y, 0.25)
    const blink = sp.state === 'planted' && Math.sin(T * (sp.timer < 10 ? 18 : 6)) > 0
    spike.fillColor = blink ? [1, 0.9, 0.5, 1] : [1, 0.35, 0.2, 1]
  }
}

/** Back to the top-down view: the 2D world shows, the 3D twin hides. */
function hideWorld3D() {
  show('world2d', true)
  show('globalLight2d', true)
  show('rcCross', false)
}

function drawAgents() {
  const me = g.agents[0]
  const viewer = me.alive ? me : g.agents.find((a) => a.team === MY && a.alive) || me
  g.viewer = viewer
  for (const a of g.agents) {
    const enemy = a.team !== MY
    let visible = true
    if (enemy && a.alive) {
      // You see what you see — plus whatever a teammate is looking at
      // right now (the bots share spots too; this is your minimap).
      const seen = canSee(viewer, a)
      const spotted = !seen && g.agents.some((m) => m.team === MY && m.alive && m !== viewer && canSee(m, a))
      if (seen) note(MY, a)  // (bots share their own sightings themselves)
      visible = seen || spotted || g.reveal[a.i] > 0
      a.spotted = !seen && visible
    }
    if (enemy && !a.alive) visible = !!a.deadAt
    a.root.visible = visible
    if (!visible) continue
    a.root.x = Math.round(a.alive ? a.x : a.deadAt ? a.deadAt.x : a.x)
    a.root.y = Math.round(a.alive ? a.y : a.deadAt ? a.deadAt.y : a.y)
    const color = AGENT_COLORS[a.kind]
    const tint = !a.alive ? [0.25, 0.25, 0.27, 1] : a.hitT > 0 ? [1, 0.5, 0.5, 1] : a.dismissT > 0 ? [color[0], color[1], color[2], 0.35] : color
    a.hitT = Math.max(0, (a.hitT || 0) - dt)
    a.body.fillColor = tint
    a.body.rotation = (a.angle * 180) / Math.PI
    // Dead: the body is swapped for an X in the team colour.
    a.body.visible = a.alive
    a.xa.visible = a.xb.visible = !a.alive
    if (!a.alive) { a.xa.rotation = 45; a.xb.rotation = -45 }
    const frac = clip(a.hp / 100, 0, 1)
    a.hpBar.scaleX = Math.max(0.01, frac); a.hpBar.x = -5 * (1 - frac)
    a.hpBar.visible = a.alive; a.hpBack.visible = a.alive
    // Marker: spike carrier (orange), revealed (red pulse), detained (white).
    const carrier = g.spike.state === 'carried' && g.spike.carrier === a
    const rev = enemy && (g.reveal[a.i] > 0 || a.spotted)
    a.mark.visible = a.alive && (carrier || rev || a.detainT > 0 || a.vulnT > 0)
    if (a.mark.visible) a.mark.fillColor = carrier ? [1, 0.55, 0.2, 1] : a.detainT > 0 ? [1, 1, 1, 1] : [1, 0.25 + Math.sin(T * 12) * 0.2, 0.25, 1]
  }
}

function drawHud() {
  const me = g.agents[0]
  const v = g.viewer
  setText('score', g.score[MY] + '  -  ' + g.score[ENEMY])
  const atkMine = g.attackers === MY
  setText('atkLabel', atkMine ? 'ATK' : 'DEF'); ref('atkLabel').fillColor = [0.4, 1, 0.65, 1]
  setText('defLabel', atkMine ? 'DEF' : 'ATK')
  for (let k = 0; k < 5; k++) {
    ref('pipA' + k).fillColor = k < g.score[MY] ? [0.3, 1, 0.6, 1] : [0.2, 0.3, 0.25, 1]
    ref('pipD' + k).fillColor = k < g.score[ENEMY] ? [1, 0.3, 0.3, 1] : [0.3, 0.2, 0.2, 1]
  }
  const t = g.phase === 'planted' ? g.spike.timer : g.phase === 'buy' ? g.phaseT : g.time
  const secs = Math.max(0, Math.ceil(t))
  setText('timer', (g.phase === 'buy' ? 'BUY ' : g.phase === 'planted' ? 'SPIKE ' : '') + Math.floor(secs / 60) + ':' + String(secs % 60).padStart(2, '0'))
  ref('timer').fillColor = g.phase === 'planted' ? [1, 0.45 + Math.sin(T * 8) * 0.2, 0.3, 1] : [1, 0.9, 0.7, 1]
  setText('hpText', String(Math.ceil(v.hp)))
  setText('armorText', String(Math.ceil(v.armor)))
  setText('agentText', v.kit.name + (v !== me ? ' (SPEC)' : ''))
  ;['C', 'Q', 'E', 'X'].forEach((k, i) => {
    const n = k === 'X' ? v.ult + '/' + v.kit.ultCost : String(v.charges[k] || 0)
    setText('slotN' + i, k === 'X' ? '' : n)
    const ready = k === 'X' ? v.ult >= v.kit.ultCost : (v.charges[k] || 0) > 0
    ref('slot' + i).fillColor = ready ? (k === 'X' ? [0.5, 0.4, 0.1, 1] : [0.2, 0.35, 0.3, 1]) : [0.12, 0.12, 0.14, 1]
    if (k === 'X') setText('slotKey' + i, ready ? 'X!' : v.ult + '/' + v.kit.ultCost)
  })
  setText('ammoText', v.knives > 0 ? 'KNIVES ' + v.knives : v.reloadT > 0 ? 'RELOAD' : v.ammo + '/' + v.mag)
  // Spike status and plant / defuse bar.
  const sp = g.spike
  setText('spikeText', sp.state === 'planted' ? 'SPIKE PLANTED ' + sp.site : sp.state === 'dropped' ? 'SPIKE DROPPED' : sp.carrier && sp.carrier.isPlayer ? 'YOU HAVE THE SPIKE' : '')
  const prog = sp.state === 'planted' ? (sp.defuser ? sp.prog / DEFUSE_TIME : 0) : sp.carrier ? sp.prog / PLANT_TIME : 0
  show('progBack', prog > 0); show('progFill', prog > 0)
  ref('progFill').scaleX = Math.max(0.01, prog); ref('progFill').x = -40 * (1 - prog)
  setText('progText', prog > 0 ? (sp.state === 'planted' ? 'DEFUSING' : 'PLANTING') : '')
  // Help line.
  let help = ''
  if (me.alive && sp.state === 'carried' && sp.carrier === me && siteAt(me.x, me.y)) help = 'HOLD F TO PLANT'
  else if (me.alive && sp.state === 'planted' && !isAttacker(me) && dist(me.x, me.y, sp.x, sp.y) < 12) help = 'HOLD F TO DEFUSE'
  else if (g.round === 1 && g.phase === 'buy') help = fps3d() ? 'CLICK TO LOCK MOUSE  WASD MOVE  C Q E X ABILITIES  V TOP-DOWN' : 'WASD MOVE  CLICK SHOOT  C Q E X ABILITIES  V 3D VIEW'
  setText('help', help)
  // Feed.
  for (let k = 0; k < 4; k++) {
    const f = g.feed[k]
    if (f) f.t -= dt
    setText('feed' + k, f && f.t > 0 ? f.text : '')
    if (f) ref('feed' + k).fillColor = f.mine ? [0.5, 1, 0.7, 1] : [1, 0.55, 0.55, 1]
  }
  if (g.msgT > 0) { g.msgT -= dt; if (g.msgT <= 0) message('', '', 0) }
  // Overlays: nearsight (Leer), hit / detonation flash.
  show('nearsight', v.nearT > 0)
  g.hitFlash = Math.max(0, (g.hitFlash || 0) - dt)
  const fl = ref('flash')
  fl.visible = g.hitFlash > 0 || v.detainT > 0
  if (fl.visible) fl.fillColor = v.detainT > 0 ? [0.85, 0.9, 1, 0.22] : [1, 0.2, 0.2, Math.min(0.35, g.hitFlash)]
}

function drawView() {
  const v = g.viewer
  // Vision lights + camera follow the agent you're seeing through.
  const cone = ref('visionCone'), aura = ref('visionAura')
  cone.x = v.x; cone.y = v.y; cone.rotation = (v.angle * 180) / Math.PI - 90
  aura.x = v.x; aura.y = v.y
  cone.lightRadius = v.nearT > 0 ? 50 : 230
  const cam = ref('camera')
  // Look-ahead toward your aim, so the screen covers your whole sight line.
  // (No look-ahead in the 3D view: the mouse's offset from the camera is
  // what steers you there, so the camera must hold still on you.)
  const look = fps3d() ? { x: 0, y: 0 } : v === g.agents[0] ? { x: clip((ctx.mouseX - v.x) * 0.45, -70, 70), y: clip((ctx.mouseY - v.y) * 0.45, -70, 70) } : { x: Math.cos(v.angle) * 50, y: Math.sin(v.angle) * 50 }
  g.camX += (v.x + look.x - g.camX) * Math.min(1, dt * 8)
  g.camY += (v.y + look.y - g.camY) * Math.min(1, dt * 8)
  cam.x = Math.round(g.camX); cam.y = Math.round(g.camY)
  const cross = ref('cross')
  cross.x = Math.round(ctx.mouseX); cross.y = Math.round(ctx.mouseY)
  cross.visible = g.agents[0].alive
  // The spike.
  const sp = g.spike
  const sRef = ref('spike'), sLight = ref('spikeLight')
  const showSpike = sp.state === 'dropped' || sp.state === 'planted'
  sRef.visible = showSpike; sLight.visible = sp.state === 'planted'
  if (showSpike) { sRef.x = Math.round(sp.x); sRef.y = Math.round(sp.y) }
  if (sp.state === 'planted') { sLight.x = sp.x; sLight.y = sp.y; sLight.lightIntensity = 0.8 + Math.sin(T * (sp.timer < 10 ? 18 : 6)) * 0.6 }
}

// ----------------------------------------------------------- setup --
if (!g.init) {
  g.init = true
  // Every match starts in first person (a saved 'top' from older versions
  // doesn't stick); V still flips to the top-down tactical map and back.
  S.settings.view = '3d'
  g.agents = []
  // Your team: you + the four other agents; the enemy: all five.
  const mine = [S.agent].concat(AGENT_IDS.filter((k) => k !== S.agent))
  const names = CALLSIGNS.slice().sort(() => Math.random() - 0.5)
  for (let i = 0; i < 10; i++) g.agents.push(makeAgent(i, i < 5 ? mine[i] : AGENT_IDS[i - 5], names[i]))
  g.score = [0, 0]
  g.round = 0
  g.attackers = MY
  g.ents = []
  g.feed = []
  g.camX = P.atkSpawn.x; g.camY = P.atkSpawn.y
  for (const k of ['c', 'q', 'e', 'x', 'r']) g.keyWas[k] = keys.has(k)
  startRound()
}

// ------------------------------------------------------- the frame --
if (g.phase === 'matchover') {
  if (ctx.tap || keys.has('enter')) {
    if (g.score[MY] >= WIN_ROUNDS) S.record.wins++; else S.record.losses++
    ctx.storage.set('tactics', S.record)
    ctx.globals.tacScene = {}
    ctx.loadScene('Tactics Select')
  }
} else {
  if (g.phase === 'buy') { g.phaseT -= dt; if (g.phaseT <= 0) { g.phase = 'live'; message('', 'GO', 0.8) } }
  else if (g.phase === 'live') {
    g.time -= dt
    if (g.time <= 0) endRound(1 - g.attackers, 'TIME EXPIRED')
    // A fake: after showing utility at one site, everyone swings to the real one.
    if (g.plan.fakeAt && g.plan.show !== g.plan.site && ROUND_TIME - g.time > g.plan.fakeAt) {
      g.plan.show = g.plan.site
      for (const a of g.agents) a.cleared = false
      if (g.attackers === MY) feed('ROTATE TO ' + g.plan.site, true)
    }
    // A fake shows utility at the fake site before swinging.
    if (g.plan.fakeAt && g.plan.show !== g.plan.site && ROUND_TIME - g.time > g.plan.fakeAt - 4) g.plan.faking = true
    // Attackers execute on the plan's clock, or once all the bots have staged.
    const staged = g.agents.filter((a) => a.alive && isAttacker(a) && !a.isPlayer && !a.lurking).every((a) => a.goal && dist(a.x, a.y, a.goal.x, a.goal.y) < 24)
    const elapsedNow = ROUND_TIME - g.time
    const earlyOk = (g.plan.strategy !== 'slow' && g.plan.strategy !== 'fake' && elapsedNow > 8) || g.time < 40
    if (!g.plan.executing && (elapsedNow > g.plan.executeAt || (staged && earlyOk))) { g.plan.executing = true; if (g.attackers === MY) feed('GO ' + g.plan.site + '!', true) }
  } else if (g.phase === 'over') {
    g.phaseT -= dt
    if (g.phaseT <= 0) {
      if (g.score[MY] >= WIN_ROUNDS || g.score[ENEMY] >= WIN_ROUNDS) {
        g.phase = 'matchover'
        message(g.score[MY] >= WIN_ROUNDS ? 'VICTORY' : 'DEFEAT', g.score[MY] + ' - ' + g.score[ENEMY] + '   CLICK TO CONTINUE', 999)
      } else startRound()
    }
  }
  for (let i = 0; i < 10; i++) g.reveal[i] = Math.max(0, g.reveal[i] - dt)
  for (const a of g.agents) {
    stepAgent(a)
    if (!a.alive) continue
    // (`autopilot`: the bot brain plays your agent too — spectate a full bot match.)
    if (a.isPlayer && !S.autopilot) playerControl(a)
    else botControl(a)
  }
  stepEnts()
  stepSpike()
  checkRoundEnd()
}
if (pressed('v')) { S.settings.view = fps3d() ? 'top' : '3d'; ctx.storage.set('tacticsSettings', S.settings) }
drawAgents()
drawView()
if (fps3d()) {
  drawWorld3D()
  g.was3d = true
} else if (g.was3d !== false) {
  hideWorld3D()
  g.was3d = false
}
drawHud()
