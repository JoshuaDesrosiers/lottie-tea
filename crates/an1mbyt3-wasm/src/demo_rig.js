// ===================================================================
//  RAGDOLL ARENA — the "Brain" layer's Code program.
//
//  Everything that *isn't* authored data lives here:
//  - player movement + procedural foot stepping (moves the IK foot targets),
//  - punches: fires the Combat layer's `jab` / `cross` triggers (that
//    layer keyframes the hand IK targets), then `guard` to return,
//  - an enemy fighter with a small AI (its hands/feet driven from here),
//  - hits measured from bone positions (`ref.worldPoint`),
//  - knockouts: `ref.ragdoll(true)` hands the whole rig to physics, a
//    launch velocity sends it flying; it gets back up after a moment,
//  - the skinned worm's IK target sway, and the HUD.
// ===================================================================

const g = ctx.globals
const dt = Math.min(ctx.deltaSeconds, 1 / 20)
const keys = ctx.keysDown
const T = performance.now() / 1000

const GROUND = -100
const HIPS_Y = GROUND + 37
const ARENA = 200
const MAX_HP = 100
const BONES = ['hips', 'chest', 'head', 'uarmN', 'larmN', 'uarmF', 'larmF', 'thighN', 'shinN', 'thighF', 'shinF']

const ref = (id) => {
  let r = g.refs[id]
  if (r === undefined) { r = getCompByID(id); g.refs[id] = r }
  return r
}

function makeFighter(prefix, facing) {
  const f = { prefix, facing, hp: MAX_HP, ko: false, koT: 0, launch: null, x: ref(prefix + '_hips').x, vx: 0, hitCd: 0, flash: 0, stagger: 0, recoil: 0, rest: {} }
  for (const b of BONES) {
    const r = ref(prefix + '_' + b)
    f.rest[b] = { x: r.x, y: r.y, rot: r.rotation }
  }
  // Feet: planted positions + an in-flight step.
  f.feet = ['N', 'F'].map((side, i) => ({ side, x: f.x + (i === 0 ? 6 : -6) * facing, y: GROUND + 2, step: null }))
  return f
}

if (!g.init) {
  g.init = true
  g.refs = {}
  g.p = makeFighter('p', 1)
  g.e = makeFighter('e', -1)
  g.punch = null
  g.enemyPunch = null
  g.enemyCd = 1.5
  g.msgT = 0
  g.rWas = false
  g.shake = 0
}

const part = (f, b) => ref(f.prefix + '_' + b)

function message(text, t) {
  ref('msg').textContent = text
  g.msgT = t
}

/** Knockout: the whole rig becomes Dynamic (ragdoll) and is launched. */
function knockOut(f, dir) {
  f.ko = true
  f.koT = 0
  part(f, 'hips').ragdoll(true)
  // The physics world rebuilds next frame; launch right after.
  f.launch = { frames: 2, vx: 260 * dir, vy: 190 }
  g.shake = 0.3
  fx('fxDust', f.x, GROUND + 4, 20)
  if (f === g.p) ctx.setInput('ko', true)
  message(f === g.p ? 'K.O.!' : 'KNOCKOUT!', 2.2)
}

/** Stand back up: Kinematic again and every bone back in its rest pose. */
function getUp(f, x) {
  part(f, 'hips').ragdoll(false)
  for (const b of BONES) {
    const r = part(f, b), rest = f.rest[b]
    r.rotation = rest.rot
    if (b === 'hips') { r.x = x; r.y = HIPS_Y } else { r.x = rest.x; r.y = rest.y }
  }
  f.x = x
  f.vx = 0
  f.hp = MAX_HP
  f.ko = false
  f.launch = null
  for (const [i, foot] of f.feet.entries()) { foot.x = x + (i === 0 ? 6 : -6) * f.facing; foot.y = GROUND + 2; foot.step = null }
  fx('fxDust', x, GROUND + 4, 16)
  if (f === g.p) ctx.setInput('ko', false)
}

function fx(id, x, y, n) {
  const r = ref(id)
  r.x = x; r.y = y
  r.emit(n)
}

/** Damage `target` if `fist` (world point) is touching its chest or head. */
function tryHit(attacker, target, fist, damage) {
  if (target.ko) return false
  const chest = part(target, 'chest').worldPoint(12, 0)
  const head = part(target, 'head').worldPoint(8, 0)
  const near = Math.hypot(fist.x - chest.x, fist.y - chest.y) < 17 || Math.hypot(fist.x - head.x, fist.y - head.y) < 13
  if (!near) return false
  target.hp -= damage
  target.flash = 0.15
  // Knockback: slide back, stagger (no control), lean away from the blow,
  // and shake the camera — harder for heavier punches.
  const power = damage / 10
  target.vx = 170 * power * attacker.facing
  target.stagger = 0.18 + 0.12 * power
  target.recoil = 14 * power * attacker.facing
  g.shake = Math.max(g.shake, 0.06 + 0.06 * power)
  fx('fxHit', fist.x, fist.y, 10 + Math.round(8 * power))
  if (target.hp <= 0) knockOut(target, attacker.facing)
  return true
}

// -------------------------------------------------------------- movement --

/** Moves a standing fighter and keeps its feet under it (IK targets). */
function locomote(f, input) {
  f.hitCd = Math.max(0, f.hitCd - dt)
  f.flash = Math.max(0, f.flash - dt)
  const speed = 75
  if (f.stagger > 0) {
    // Staggered: sliding back from a hit, no control.
    f.stagger -= dt
    f.vx *= Math.max(0, 1 - dt * 5)
  } else {
    f.vx += (input * speed - f.vx) * Math.min(1, dt * 10)
  }
  f.recoil *= Math.exp(-dt * 7)
  // Knockback decays into the ground friction above; keep inside the arena
  // and don't walk through the other fighter while they're standing.
  let x = f.x + f.vx * dt
  const other = f === g.p ? g.e : g.p
  if (!other.ko) {
    const gap = 38
    if (f.facing > 0) x = Math.min(x, other.x - gap)
    else x = Math.max(x, other.x + gap)
  }
  f.x = Math.max(-ARENA, Math.min(ARENA, x))
  const walking = Math.abs(f.vx) > 8
  const hips = part(f, 'hips')
  hips.x = f.x
  hips.y = HIPS_Y + (walking ? -Math.abs(Math.sin(T * 12)) * 2 : Math.sin(T * 2.4) * 0.6) - Math.abs(f.recoil) * 0.15
  // Lean the whole body away from the last hit (the feet stay planted by IK).
  hips.rotation = 90 - f.recoil

  // Procedural stepping: a foot lifts when the hips drift too far from it
  // and lands ahead (in the direction of travel); one foot at a time.
  const stepping = f.feet.some((ft) => ft.step)
  for (const [i, foot] of f.feet.entries()) {
    const home = f.x + (i === 0 ? 6 : -6) * f.facing + f.vx * 0.16
    if (foot.step) {
      foot.step.t += dt / 0.16
      const t = Math.min(1, foot.step.t)
      foot.x = foot.step.from + (foot.step.to - foot.step.from) * t
      foot.y = GROUND + 2 + Math.sin(t * Math.PI) * 7
      if (t >= 1) { foot.step = null; foot.y = GROUND + 2; if (walking) fx('fxDust', foot.x, GROUND + 1, 2) }
    } else if (!stepping && Math.abs(foot.x - home) > 9) {
      foot.step = { from: foot.x, to: home + Math.sign(home - foot.x) * 3, t: 0 }
    }
    const target = ref(f.prefix + '_foot' + foot.side)
    target.x = foot.x
    target.y = foot.y
  }
}

/** A knocked-out fighter: apply the launch, then get up after a while. */
function ragdolled(f, respawnX) {
  if (f.launch && --f.launch.frames <= 0) {
    for (const b of ['hips', 'chest', 'head']) part(f, b).velocity = { x: f.launch.vx, y: f.launch.vy }
    f.launch = null
  }
  f.koT += dt
  f.x = part(f, 'hips').worldX
  if (f.koT > 3.2) getUp(f, respawnX)
}

// ------------------------------------------------------------------ player --

const p = g.p, e = g.e
const pressed = (k) => keys.has(k)
if (!p.ko) {
  const input = (pressed('d') || pressed('right') ? 1 : 0) - (pressed('a') || pressed('left') ? 1 : 0)
  locomote(p, input)
  ctx.setInput('speed', Math.abs(p.vx))

  // Punch: the Combat layer animates the hand IK targets; we time the hit.
  if (g.punch) {
    g.punch.t += dt
    if (!g.punch.hit && g.punch.t > g.punch.peak) {
      const fist = part(p, g.punch.hand === 'N' ? 'larmN' : 'larmF').worldPoint(17, 0)
      g.punch.hit = tryHit(p, e, fist, g.punch.damage) || g.punch.t > g.punch.peak + 0.06
    }
    if (g.punch.t > g.punch.length) { ctx.fireTrigger('guard'); g.punch = null }
  } else if (pressed('j') || pressed('mouse') || ctx.tap) {
    ctx.fireTrigger('jab')
    g.punch = { hand: 'N', t: 0, peak: 0.08, length: 0.28, damage: 9 }
  } else if (pressed('k') || (ctx.swipe && ctx.swipe.dir === 'right')) {
    ctx.fireTrigger('cross')
    g.punch = { hand: 'F', t: 0, peak: 0.11, length: 0.34, damage: 16 }
  }
} else {
  ragdolled(p, -70)
}

// ------------------------------------------------------------------- enemy --

if (!e.ko) {
  // Approach to punching range, back off a little after swinging.
  const gap = e.x - p.x
  let input = 0
  if (!p.ko) {
    if (gap > 48) input = -1
    else if (gap < 40) input = 0.4
  } else if (e.x < 70) input = 0.6
  locomote(e, input)
  e.hitCd = Math.max(0, e.hitCd - dt)

  // Enemy punches: tween its own hand IK target (no state machine needed).
  g.enemyCd -= dt
  const handN = part(e, 'handN')
  const guardN = { x: 10, y: 14 }
  if (!g.enemyPunch && g.enemyCd <= 0 && !p.ko && gap < 52 && e.stagger <= 0) {
    g.enemyPunch = { t: 0, hit: false }
    g.enemyCd = 0.9 + Math.random() * 0.9
  }
  if (g.enemyPunch) {
    const ep = g.enemyPunch
    ep.t += dt
    const k = ep.t < 0.1 ? ep.t / 0.1 : Math.max(0, 1 - (ep.t - 0.16) / 0.14)
    handN.x = guardN.x + (15 - guardN.x) * k
    handN.y = guardN.y + (40 - guardN.y) * k
    if (!ep.hit && ep.t > 0.09) ep.hit = tryHit(e, p, part(e, 'larmN').worldPoint(17, 0), 8) || ep.t > 0.16
    if (ep.t > 0.3) g.enemyPunch = null
  } else {
    handN.x = guardN.x + Math.sin(T * 3) * 1
    handN.y = guardN.y
  }
  const handF = part(e, 'handF')
  handF.x = 12 + Math.sin(T * 3 + 1) * 1
  handF.y = 9
  // A little idle sway on the spine (the enemy isn't on the state machine).
  part(e, 'chest').rotation = Math.sin(T * 2.2) * 3 + (Math.abs(e.vx) > 8 ? 6 : 0)
  part(e, 'head').rotation = Math.sin(T * 2.2 + 0.6) * 4
} else {
  ragdolled(e, 80)
}

// --------------------------------------------------------------- the world --

// The worm reaches for a target drifting in a lazy figure-eight.
const worm = ref('wormTarget')
worm.x = 190 + Math.sin(T * 0.9) * 26
worm.y = GROUND + 74 + Math.sin(T * 1.8) * 10

// Reset everything.
if (pressed('r') && !g.rWas) {
  getUp(p, -70)
  getUp(e, 70)
  message('FIGHT!', 1.2)
}
g.rWas = pressed('r')

// HUD.
for (const f of [p, e]) {
  const bar = ref(f.prefix + '_hp')
  const frac = Math.max(0.001, f.hp / MAX_HP)
  bar.scaleX = frac
  bar.x = (f === p ? -170 : 170) - 50 + 50 * frac
}
if (g.msgT > 0) { g.msgT -= dt; if (g.msgT <= 0) ref('msg').textContent = '' }

// Camera shake on hits / knockouts.
const cam = ref('camera')
if (g.shake > 0) {
  g.shake -= dt
  cam.x = (Math.random() - 0.5) * 10 * g.shake / 0.15
  cam.y = (Math.random() - 0.5) * 10 * g.shake / 0.15
} else if (cam.x !== 0 || cam.y !== 0) { cam.x = 0; cam.y = 0 }
