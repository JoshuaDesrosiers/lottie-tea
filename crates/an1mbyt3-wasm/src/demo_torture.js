// Ragdoll Torture — a crash-test dummy, a lab full of hazards, and tools.
//
// (The Rust builder prepends FLOOR_Y, HALF_W, HALF_H, the HUD layout
// TOOL_X0 / TOOL_STEP / RESET_X / RESET_W, and the blood pool sizes.)
// Tools: 1 GRAB (hold and fling a limb), 2 PUNCH, 3 BOMB, 4 ANVIL,
// 5 CANNON (fires from the left wall at the cursor). R or the RESET
// button: start over; N or +DUMMY: another dummy (random colour); C: clear props. PAIN comes from impacts (a sudden
// change in a limb's velocity) and from touching the spikes or the saw.
// Battered limbs break off (`ref.breakJoint()`) and bleed.
// (Note: the script runtime injects helpers named lerp/clamp/random/...,
// so this script's own helpers use other names.)

const g = ctx.globals.torture || (ctx.globals.torture = {})
const keys = ctx.keysDown
const dt = Math.min(ctx.deltaSeconds || 1 / 60, 1 / 20)
const T = ctx.time || 0
const mx = ctx.mouseX, my = ctx.mouseY

if (!g.refs) { g.refs = {}; g.keyWas = {} }
const ref = (id) => {
  let r = g.refs[id]
  if (r === undefined) { r = getCompByID(id); g.refs[id] = r }
  return r
}
function setText(id, text) { const r = ref(id); if (r && r.textContent !== text) r.textContent = text }
function pressed(k) {
  const down = keys.has(k)
  const was = g.keyWas[k]
  g.keyWas[k] = down
  return down && !was
}
const rand = (a, b) => a + Math.random() * (b - a)
const pick = (arr) => arr[Math.floor(Math.random() * arr.length)]
const clip = (v, a, b) => Math.max(a, Math.min(b, v))
const mixColor = (a, b, t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, 1]

const TOOLS = ['grab', 'punch', 'bomb', 'anvil', 'cannon']
const BONES = ['Hips', 'Chest', 'Head', 'ThighL', 'ShinL', 'ThighR', 'ShinR', 'UpperArmL', 'ForearmL', 'UpperArmR', 'ForearmR']
const YELLOW = [1, 0.8, 0.18, 1]
const BRUISE = [0.55, 0.22, 0.45, 1]     // a cartoon bruise: purple
const MAX_PROPS = 24
/** At most this many dummies at once (the oldest goes first). */
const MAX_DUMMIES = 6
const CANNON = { x: -HALF_W + 22, y: 40 }
const OUCH = ['OUCH!', 'OOF!', 'BONK!', 'CRUNCH!', 'YOWCH!', 'ARGH!', 'WHAM!', 'OW OW OW']
const SNAP = ['SNAP!', 'RIP!', 'POP!', 'CRACK!', "THAT'S OFF"]
/** Where each bone's joint sits in its PARENT bone's space (the builder's
 * numbers) — the stump that bleeds once the bone breaks off. */
const JOINT_AT = {
  Chest: ['Hips', 16, 0], Head: ['Chest', 22, 0],
  ThighL: ['Hips', 0, 5], ThighR: ['Hips', 0, -5], ShinL: ['ThighL', 19, 0], ShinR: ['ThighR', 19, 0],
  UpperArmL: ['Chest', 19, 9], UpperArmR: ['Chest', 19, -9], ForearmL: ['UpperArmL', 15, 0], ForearmR: ['UpperArmR', 15, 0],
}
/** A limb breaks off once it's this battered (0..1) and takes a big hit. */
const BREAK_HURT = 0.8

// ------------------------------------------------------------- dummy --
/** A bright random colour (any hue, fairly saturated) for a new dummy. */
function randomColor() {
  const h = Math.random() * 6, sat = rand(0.55, 0.9), val = rand(0.8, 1)
  const i = Math.floor(h), f = h - i
  const p = val * (1 - sat), q = val * (1 - sat * f), t = val * (1 - sat * (1 - f))
  const rgb = [[val, t, p], [q, val, p], [p, val, t], [p, q, val], [t, p, val], [val, p, q]][i % 6]
  return [rgb[0], rgb[1], rgb[2], 1]
}

/** Spawns a dummy painted `color` (the prefab is yellow; the script tints
 * every skin part). Each part keeps a link back to its dummy. */
function addDummy(color, x) {
  const root = ctx.spawn('Dummy', x === undefined ? rand(-140, 20) : x, 40)
  if (!root) return null
  const d = {
    root, color, parts: null,
    face: root.part('Face'), eyeL: root.part('EyeL'), eyeR: root.part('EyeR'), mouth: root.part('Mouth'),
    age: 0, wince: 0,
  }
  d.parts = BONES.map((name) => ({ name, dummy: d, ref: root.part(name), skin: root.part(name + 'Skin'), last: null, hurt: 0, flash: 0, broken: false, bleedT: 0, sawT: 0 }))
  g.dummies.push(d)
  while (g.dummies.length > MAX_DUMMIES) removeDummy(g.dummies[0])
  return d
}

function removeDummy(d) {
  d.root.destroy()
  g.dummies = g.dummies.filter((o) => o !== d)
  if (g.grab && g.grab.dummy === d) g.grab = null
  for (const key in g.touching) if (g.touching[key].part && g.touching[key].part.dummy === d) delete g.touching[key]
}

/** Every limb of every dummy. */
function allParts() {
  const list = []
  for (const d of g.dummies) for (const p of d.parts) list.push(p)
  return list
}

function partOf(r) {
  if (!r) return null
  return allParts().find((p) => p.ref.name === r.name && p.ref.opaqueId === r.opaqueId) || null
}

function hurt(p, amount, x, y, word) {
  if (!p || amount <= 0) return
  g.pain += amount
  const wasBattered = p.hurt >= BREAK_HURT
  p.hurt = Math.min(1, p.hurt + amount / 120)
  p.flash = 0.15
  p.dummy.wince = Math.max(p.dummy.wince, Math.min(1.2, 0.3 + amount / 30))
  if (word) popup(word, x, y, amount > 25 ? [1, 0.35, 0.3, 1] : [1, 0.9, 0.6, 1])
  if (amount > 5) bleed(x, y, Math.min(45, Math.round(amount * 1.6)))
  // An already-battered limb that takes another big hit comes off.
  if (wasBattered && amount >= 16) breakPart(p)
}

/** Snaps a bone's hinge: it (and whatever hangs off it) comes loose. */
function breakPart(p) {
  if (!p || p.broken || !JOINT_AT[p.name]) return
  p.broken = true
  p.bleedT = 3
  p.ref.breakJoint()
  if (g.grab === p) g.grab = null
  g.pain += 40
  g.shake = Math.max(g.shake, 2)
  bleed(p.ref.worldX, p.ref.worldY, 70)
  popup(pick(SNAP), p.ref.worldX, p.ref.worldY + 6, [1, 0.25, 0.2, 1])
  p.dummy.wince = 1.5
}

/** The stump a broken bone left on its parent (world point), or null. */
function stumpOf(p) {
  const j = JOINT_AT[p.name]
  const parent = j && p.dummy.parts.find((q) => q.name === j[0])
  return parent ? parent.ref.worldPoint(j[1], j[2]) : null
}

// ---------------------------------------------------------------- blood --
/** A burst of `count` droplets at (x, y), plus a splat on the floor below. */
function bleed(x, y, count) {
  if (!isFinite(x) || !isFinite(y)) return
  const k = g.bloodNext = ((g.bloodNext || 0) + 1) % BLOOD_EMITTERS
  const em = ref('blood' + k)
  em.x = x; em.y = y
  em.emit(count)
  if (count >= 8) splat(x + rand(-18, 18), count)
}

/** A blood decal on the floor (they stay until RESET). */
function splat(x, size) {
  if (Math.abs(x) > HALF_W - 14) return
  const k = g.splatNext = ((g.splatNext || 0) + 1) % BLOOD_SPLATS
  const r = ref('splat' + k)
  r.visible = true
  r.x = Math.round(x); r.y = FLOOR_Y + 1
  const w = clip(0.6 + size / 25, 0.6, 2.6) * rand(0.8, 1.3)
  r.scaleX = w; r.scaleY = rand(0.8, 1.2)
}

/** RESET: one fresh (yellow) dummy, no props, no blood, PAIN back to zero. */
function resetAll() {
  for (const p of g.props) p.ref.destroy()
  g.props = []
  for (const d of g.dummies.slice()) removeDummy(d)
  for (let k = 0; k < BLOOD_SPLATS; k++) { const r = ref('splat' + k); if (r.visible) r.visible = false }
  g.pops = []
  g.touching = {}
  g.grab = null
  g.pain = 0
  addDummy(YELLOW, -70)
  popup('FRESH DUMMY', -60, 40, [0.6, 1, 0.7, 1])
}

/** +DUMMY: another one, in a random colour, dropped in from above. */
function spawnExtra() {
  const d = addDummy(randomColor())
  if (d) popup('ANOTHER ONE', d.root.x, 60, d.color)
}

/** The dummy part nearest (x, y) within `range`. */
function nearestPart(x, y, range) {
  let best = null, bestD = range
  for (const p of allParts()) {
    const d = Math.hypot(p.ref.worldX - x, p.ref.worldY - y)
    if (d < bestD) { best = p; bestD = d }
  }
  return best
}

/** Every moving thing a blast or punch can shove: limbs and props. */
function bodies() {
  const list = allParts().map((p) => ({ ref: p.ref, part: p }))
  for (const pr of g.props) list.push({ ref: pr.ref, part: null })
  return list
}

// ------------------------------------------------------------- effects --
function popup(text, x, y, color) {
  const k = g.popNext = ((g.popNext || 0) + 1) % 10
  g.pops[k] = { text, x: x + rand(-6, 6), y: y + 8, t: 0.9, color }
}

function boom(x, y, radius) {
  const k = g.boomNext = ((g.boomNext || 0) + 1) % 6
  g.booms[k] = { x, y, t: 0, radius }
  g.shake = Math.max(g.shake, radius / 25)
}

// --------------------------------------------------------------- props --
function addProp(kind, x, y) {
  const r = ctx.spawn(kind, x, y)
  if (!r) return null
  const prop = { kind, ref: r, t: 0, fuse: kind === 'Bomb' ? 1.6 : 0, spark: kind === 'Bomb' ? r.part('Spark') : null }
  g.props.push(prop)
  while (g.props.length > MAX_PROPS) g.props.shift().ref.destroy()
  return prop
}

function explode(x, y) {
  const R = 150
  boom(x, y, R)
  for (const b of bodies()) {
    const bx = b.ref.worldX, by = b.ref.worldY
    const d = Math.hypot(bx - x, by - y)
    if (d > R) continue
    const f = 1 - d / R
    const nx = (bx - x) / (d || 1), ny = (by - y) / (d || 1)
    const v = b.ref.velocity || { x: 0, y: 0 }
    b.ref.velocity = { x: v.x + nx * 1100 * f, y: v.y + (ny * 1100 + 250) * f }
    b.ref.angularVelocity = rand(-20, 20) * f
    if (b.part) {
      hurt(b.part, 45 * f, bx, by, f > 0.5 && b.part.name === 'Chest' ? 'KABOOM!' : null)
      // Point-blank: limbs get blown clean off.
      if (f > 0.7 && b.part.hurt > 0.4 && Math.random() < 0.6) breakPart(b.part)
    }
  }
}

// ---------------------------------------------------------------- init --
if (!g.init) {
  g.init = true
  g.tool = 0
  g.props = []
  g.pops = []
  g.booms = []
  g.shake = 0
  g.best = ctx.storage.get('tortureBest') || 0
  g.crusher = { phase: 'up', t: 1.5 }
  g.dummies = []
  g.touching = {}
  g.pain = 0
  for (const k of ['1', '2', '3', '4', '5', 'r', 'c', 'n', 'mouse']) g.keyWas[k] = keys.has(k)
  g.splatNext = 0; g.bloodNext = 0
  addDummy(YELLOW, -70)
}

// ----------------------------------------------------------------- input --
for (let k = 0; k < 5; k++) if (pressed(String(k + 1))) g.tool = k
if (pressed('r')) resetAll()
if (pressed('n')) spawnExtra()
if (pressed('c')) { for (const p of g.props) p.ref.destroy(); g.props = [] }

const mouseDown = keys.has('mouse')
const click = mouseDown && !g.keyWas.mouse
g.keyWas.mouse = mouseDown
const cam = ref('camera')
const onHud = my - cam.y > HALF_H - 20
if (click && onHud) {
  // The tool buttons along the top bar, then RESET.
  const hx = mx - cam.x
  for (let k = 0; k < 5; k++) if (Math.abs(hx - (TOOL_X0 + k * TOOL_STEP)) < TOOL_STEP / 2 - 2) g.tool = k
  if (Math.abs(hx - ADD_X) < ADD_W / 2) spawnExtra()
  if (Math.abs(hx - RESET_X) < RESET_W / 2) resetAll()
} else if (click) {
  const tool = TOOLS[g.tool]
  if (tool === 'grab') {
    const p = nearestPart(mx, my, 40)
    if (p) g.grab = p
  } else if (tool === 'punch') {
    const p = nearestPart(mx, my, 45)
    popup('POW!', mx, my, [1, 1, 1, 1])
    if (p) {
      // Shove away from the cursor, hardest on the part you hit.
      const dx = p.ref.worldX - mx, dy = p.ref.worldY - my
      const d = Math.hypot(dx, dy) || 1
      for (const q of p.dummy.parts) {
        const share = q === p ? 1 : 0.35
        const v = q.ref.velocity || { x: 0, y: 0 }
        q.ref.velocity = { x: v.x + (dx / d) * 650 * share, y: v.y + ((dy / d) * 650 + 120) * share }
      }
      hurt(p, 14, p.ref.worldX, p.ref.worldY, p.name === 'Head' ? 'RIGHT IN THE FACE' : null)
      g.shake = Math.max(g.shake, 1.5)
    }
  } else if (tool === 'bomb') {
    addProp('Bomb', mx, my)
  } else if (tool === 'anvil') {
    const a = addProp('Anvil', mx, Math.max(my, 60))
    if (a) a.ref.velocity = { x: 0, y: -200 }
  } else if (tool === 'cannon') {
    const ang = Math.atan2(my - CANNON.y, mx - CANNON.x)
    const ball = addProp('Ball', CANNON.x + Math.cos(ang) * 26, CANNON.y + Math.sin(ang) * 26)
    if (ball) ball.ref.velocity = { x: Math.cos(ang) * 800, y: Math.sin(ang) * 800 }
    g.recoil = 1
    boom(CANNON.x + Math.cos(ang) * 28, CANNON.y + Math.sin(ang) * 28, 24)
  }
}
if (!mouseDown) g.grab = null

// GRAB: a spring pulls the held limb toward the cursor (so it drags the
// rest of the body along by its joints); let go to fling.
const line = ref('grabLine')
if (g.grab) {
  const p = g.grab.ref
  const dx = mx - p.worldX, dy = my - p.worldY
  p.velocity = { x: clip(dx * 14, -1400, 1400), y: clip(dy * 14, -1400, 1400) }
  const len = Math.hypot(dx, dy)
  // Pull too far for too long and the limb tears off.
  g.stretchT = len > 90 ? (g.stretchT || 0) + dt : 0
  if (g.stretchT > 0.45 && JOINT_AT[g.grab.name]) { const torn = g.grab; breakPart(torn); g.grab = torn; g.stretchT = 0 }
  line.visible = len > 2
  line.x = (mx + p.worldX) / 2; line.y = (my + p.worldY) / 2
  line.scaleX = len; line.rotation = (Math.atan2(dy, dx) * 180) / Math.PI
} else line.visible = false

// ------------------------------------------------------------- hazards --
// The saw spins; the crusher slams on a cycle (both kinematic: the
// physics world follows whatever transform the script gives them).
const saw = ref('saw')
saw.rotation = (saw.rotation - 900 * dt) % 360
const crusher = ref('crusher'), cr = g.crusher
const TOP = 60, BOTTOM = FLOOR_Y + 16
cr.t -= dt
if (cr.phase === 'up' && cr.t <= 0) { cr.phase = 'warn'; cr.t = 0.5 }
else if (cr.phase === 'warn') { crusher.x = 60 + Math.sin(T * 60) * 1.2; if (cr.t <= 0) { cr.phase = 'slam'; crusher.x = 60 } }
else if (cr.phase === 'slam') {
  crusher.y = Math.max(BOTTOM, crusher.y - 700 * dt)
  if (crusher.y <= BOTTOM) { cr.phase = 'hold'; cr.t = 0.6; g.shake = Math.max(g.shake, 2.5); boom(60, BOTTOM - 15, 30) }
} else if (cr.phase === 'hold' && cr.t <= 0) cr.phase = 'rise'
else if (cr.phase === 'rise') {
  crusher.y = Math.min(TOP, crusher.y + 90 * dt)
  if (crusher.y >= TOP) { cr.phase = 'up'; cr.t = rand(1.5, 3) }
}

// Contacts: what's touching the spikes / saw / pad right now.
for (const c of ctx.collisions) {
  const names = [c.a && c.a.name, c.b && c.b.name]
  const other = names[0] === 'Spikes' || names[0] === 'Saw' || names[0] === 'Bounce Pad' || names[0] === 'Crusher' ? 0 : names[1] === 'Spikes' || names[1] === 'Saw' || names[1] === 'Bounce Pad' || names[1] === 'Crusher' ? 1 : -1
  if (other < 0) continue
  const hazard = names[other]
  const bodyRef = other === 0 ? c.b : c.a
  const p = partOf(bodyRef)
  const key = hazard + ':' + (bodyRef && bodyRef.opaqueId)
  if (c.started) g.touching[key] = { hazard, ref: bodyRef, part: p }
  else delete g.touching[key]
  if (!c.started || !bodyRef) continue
  if (hazard === 'Spikes' && p) hurt(p, 10, p.ref.worldX, p.ref.worldY, 'STABBY!')
  if (hazard === 'Saw' && p) hurt(p, 12, p.ref.worldX, p.ref.worldY, 'BZZZT!')
  if (hazard === 'Crusher' && p && cr.phase === 'slam') {
    hurt(p, 30, p.ref.worldX, p.ref.worldY, 'SQUISH!')
    if (p.hurt > 0.6) breakPart(p)
  }
}
for (const key in g.touching) {
  const t = g.touching[key]
  if (t.hazard === 'Bounce Pad') {
    // Launch whatever lands on the pad.
    const v = t.ref.velocity
    if (v && v.y < 400) t.ref.velocity = { x: v.x, y: 620 }
    if (t.part && Math.random() < 0.15) popup('BOING', t.ref.worldX, t.ref.worldY, [0.5, 1, 0.7, 1])
  } else if (t.part) {
    // Spikes and the saw keep hurting while you're on them; the saw flings.
    t.part.hurt = Math.min(1, t.part.hurt + dt * 0.3)
    if (t.hazard !== 'Crusher') g.pain += (t.hazard === 'Saw' ? 30 : 12) * dt
    if (t.hazard !== 'Crusher' && Math.random() < dt * (t.hazard === 'Saw' ? 14 : 4)) bleed(t.ref.worldX, t.ref.worldY, t.hazard === 'Saw' ? 10 : 5)
    if (t.hazard === 'Saw') {
      t.part.sawT += dt
      if (t.part.sawT > 1.1) breakPart(t.part)
      const s = ref('saw')
      const dx = t.ref.worldX - s.worldX, dy = t.ref.worldY - s.worldY
      const v = t.ref.velocity || { x: 0, y: 0 }
      // The blade's surface moves clockwise: tangent (dy, -dx).
      t.ref.velocity = { x: v.x + dy * 6 * dt * 10, y: v.y - dx * 6 * dt * 10 + 40 * dt * 10 }
      if (Math.random() < dt * 8) popup(pick(['BZZT', 'GRRR', 'EEEE']), t.ref.worldX, t.ref.worldY, [1, 0.85, 0.4, 1])
    }
  }
}

// ------------------------------------------------------- the dummies --
for (const d of g.dummies.slice()) {
  d.age += dt
  for (const p of d.parts) {
    const v = p.ref.velocity
    if (!v) continue
    // Impact = how hard the velocity changed this frame (ignoring the
    // limb you're dragging, and the first moments after a spawn).
    if (p.last && d.age > 0.3 && p !== g.grab) {
      const dv = Math.hypot(v.x - p.last.x, v.y - p.last.y)
      if (dv > 260) hurt(p, (dv - 260) * 0.06, p.ref.worldX, p.ref.worldY, dv > 520 ? pick(OUCH) : null)
    }
    p.last = { x: v.x, y: v.y }
    p.flash = Math.max(0, p.flash - dt)
    const base = mixColor(d.color, BRUISE, p.hurt * 0.85)
    if (p.skin) p.skin.fillColor = p.flash > 0 ? [1, 1, 1, 1] : base
    if (p.name === 'Head' && d.face) d.face.fillColor = p.flash > 0 ? [1, 1, 1, 1] : base
  }
  // Broken-off limbs bleed from both ends for a few seconds.
  for (const p of d.parts) {
    if (!p.broken || p.bleedT <= 0) continue
    p.bleedT -= dt
    p.bleedTick = (p.bleedTick || 0) - dt
    if (p.bleedTick > 0) continue
    p.bleedTick = 0.1
    const n = Math.max(2, Math.round(p.bleedT * 3))
    bleed(p.ref.worldX, p.ref.worldY, n)
    const stump = stumpOf(p)
    if (stump) bleed(stump.x, stump.y, n)
  }
  // The face: squinting eyes and an open "O" mouth while it hurts.
  d.wince = Math.max(0, d.wince - dt)
  const w = clip(d.wince, 0, 1)
  for (const eye of [d.eyeL, d.eyeR]) if (eye) { eye.scaleX = 1 - w * 0.75; eye.scaleY = 1 + w * 0.6 }
  if (d.mouth) { d.mouth.scaleX = 1 + w * 1.8; d.mouth.scaleY = 1 - w * 0.2 }
  // Fell out of the world somehow? Gone (and if it was the last, a new one).
  const hips = d.parts[0].ref
  if (Math.abs(hips.worldX) > HALF_W + 60 || hips.worldY < FLOOR_Y - 80) {
    removeDummy(d)
    if (!g.dummies.length) addDummy(YELLOW, -70)
  }
}

// --------------------------------------------------------------- props --
for (let k = g.props.length - 1; k >= 0; k--) {
  const p = g.props[k]
  p.t += dt
  if (p.kind === 'Bomb') {
    p.fuse -= dt
    if (p.spark) { p.spark.visible = Math.sin(T * 40) > -0.3; p.spark.scaleX = p.spark.scaleY = 0.8 + Math.random() * 0.6 }
    if (p.fuse < 0.5) p.ref.fillColor = Math.sin(T * 50) > 0 ? [1, 0.3, 0.2, 1] : [0.12, 0.12, 0.15, 1]
    if (p.fuse <= 0) {
      explode(p.ref.worldX, p.ref.worldY)
      p.ref.destroy()
      g.props.splice(k, 1)
    }
  } else if (p.kind === 'Ball' && p.t > 8) { p.ref.destroy(); g.props.splice(k, 1) }
}

// ----------------------------------------------------------------- draw --
for (let k = 0; k < 10; k++) {
  const p = g.pops[k], r = ref('pop' + k)
  if (!p || p.t <= 0) { if (r.visible) r.visible = false; continue }
  p.t -= dt
  p.y += 30 * dt
  r.visible = true
  r.x = Math.round(p.x); r.y = Math.round(p.y)
  if (r.textContent !== p.text) r.textContent = p.text
  r.fillColor = [p.color[0], p.color[1], p.color[2], clip(p.t * 2, 0, 1)]
  const s = p.t > 0.75 ? 1 + (p.t - 0.75) * 4 : 1
  r.scaleX = r.scaleY = s
}
for (let k = 0; k < 6; k++) {
  const b = g.booms[k], r = ref('boom' + k)
  if (!b || b.t >= 0.4) { if (r.visible) r.visible = false; continue }
  b.t += dt
  const f = b.t / 0.4
  r.visible = true
  r.x = b.x; r.y = b.y
  r.scaleX = r.scaleY = (b.radius / 10) * (0.3 + f * 0.9)
  r.fillColor = [1, 0.75 - f * 0.4, 0.2, (1 - f) * 0.6]
}
// The cannon aims at the cursor (with a little recoil kick).
g.recoil = Math.max(0, (g.recoil || 0) - dt * 5)
const barrel = ref('cannon')
barrel.rotation = clip((Math.atan2(my - CANNON.y, mx - CANNON.x) * 180) / Math.PI, -80, 80)
barrel.x = CANNON.x - g.recoil * 4
// Camera shake.
g.shake = Math.max(0, g.shake - dt * 8)
cam.x = g.shake > 0 ? rand(-g.shake, g.shake) : 0
cam.y = g.shake > 0 ? rand(-g.shake, g.shake) : 0

// HUD.
for (let k = 0; k < 5; k++) ref('tool' + k).fillColor = k === g.tool ? [0.75, 0.25, 0.25, 1] : [0.16, 0.16, 0.2, 1]
ref('addBtn').fillColor = onHud && Math.abs(mx - cam.x - ADD_X) < ADD_W / 2 ? [0.4, 0.42, 0.75, 1] : [0.25, 0.27, 0.55, 1]
ref('resetBtn').fillColor = onHud && Math.abs(mx - cam.x - RESET_X) < RESET_W / 2 ? [0.3, 0.65, 0.42, 1] : [0.2, 0.45, 0.3, 1]
const pain = Math.floor(g.pain)
setText('pain', 'PAIN ' + pain)
if (pain > g.best) { g.best = pain; g.bestDirty = true }
if (g.bestDirty && (!g.saveT || T - g.saveT > 1)) { ctx.storage.set('tortureBest', g.best); g.bestDirty = false; g.saveT = T }
setText('best', 'BEST ' + g.best)
const TOOL_HINT = ['DRAG A LIMB, LET GO TO FLING', 'CLICK NEAR THE DUMMY', 'CLICK TO DROP A BOMB', 'CLICK TO DROP AN ANVIL', 'CLICK TO FIRE THE CANNON']
setText('status', TOOL_HINT[g.tool])
