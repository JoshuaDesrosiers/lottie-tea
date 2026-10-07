// ===================================================================
//  DUNGEON ROYALE — the home screen and the game over screen.
//  Each is its own scene with its own one-state Code program; the
//  shared helpers come first, then one `//@program` section per screen.
//
//  Scenes hand over with `ctx.loadScene(name)`. The player's progress
//  travels in `ctx.globals.royale` and is saved with `ctx.storage`
//  (localStorage): mode, card levels, banked gold, trophies, best wave,
//  and the last run's result (written by the game).
//  Leaving a scene restores its designed pose, so each screen clears its
//  own cached refs (`ctx.globals.menu = {}`) on the way out.
// ===================================================================

const g = ctx.globals.menu || (ctx.globals.menu = {})
function loadRoyale() {
  const saved = ctx.storage.get('royale')
  return Object.assign(
    { wave: 0, gold: 0, best: 0, bank: 0, trophies: 0, wins: 0, losses: 0, levels: {}, mode: 'infinite', result: '' },
    saved && typeof saved === 'object' ? saved : {},
  )
}
function saveRoyale() { ctx.storage.set('royale', royale) }
const royale = ctx.globals.royale || (ctx.globals.royale = loadRoyale())
if (!royale.levels) royale.levels = {}
const keys = ctx.keysDown
const T = performance.now() / 1000

if (!g.refs) {
  g.refs = {}
  g.imgs = {}
  for (const im of ctx.images) if (g.imgs[im.name] === undefined) g.imgs[im.name] = im.id
  g.keyWas = true  // ignore a key still held from the previous screen
}

const ref = (id) => {
  let r = g.refs[id]
  if (r === undefined) { r = getCompByID(id); g.refs[id] = r }
  return r
}
const unitFrame = (row, f) => g.imgs['unit ' + (row * 4 + f)]
const itemFrame = (row, f) => g.imgs['item ' + (row * 4 + f)]

/** Torch flames flicker and animate (their lights breathe with them). */
function torches() {
  for (let i = 0; i < 4; i++) {
    const r = ref('torch' + i)
    if (!r) continue
    r.image = itemFrame(1, Math.floor(T * 8 + i) % 4)
    r.lightIntensity = 1.25 + Math.sin(T * 13 + i * 1.7) * 0.08 + Math.sin(T * 7.3 + i) * 0.05
  }
}

/** Idle-animate a row of units (ids `prefix0..n`) from the sprite rows. */
function idle(prefix, rows, hop) {
  rows.forEach((row, i) => {
    const r = ref(prefix + i)
    if (!r) return
    if (g[prefix + 'y' + i] === undefined) g[prefix + 'y' + i] = r.y
    r.image = unitFrame(row, Math.floor(T * 6 + i) % 4)
    r.y = g[prefix + 'y' + i] + (hop ? Math.round(Math.abs(Math.sin(T * 6 + i * 1.3)) * 3) : 0)
  })
}

/** A key pressed this frame (not held over from before). */
function keyPressed(...names) {
  const down = names.some((k) => keys.has(k))
  const fresh = down && !g.keyWas
  g.keyWas = down
  return fresh
}

/** Sets a text node only when it changes (text re-shapes are not free). */
function setText(id, text) {
  const r = ref(id)
  if (r && r.textContent !== text) r.textContent = text
}

/** A plate button: hover lifts and brightens it; returns true when tapped. */
function button(id, x, y, w, h, color) {
  const over = Math.abs(ctx.mouseX - x) < w / 2 && Math.abs(ctx.mouseY - y) < h / 2
  const k = over ? 1.35 : 1
  const btn = ref(id + 'Btn')
  btn.fillColor = [color[0] * k, color[1] * k, color[2] * k, 1]
  btn.scaleX = over ? 1.04 : 1
  btn.scaleY = over ? 1.08 : 1
  return over && ctx.tap
}

/** The battle scene for a mode. */
const battleScene = (mode) => (mode === 'arena' ? 'Arena Royale' : 'Dungeon Royale')

function go(scene) {
  ctx.globals.menu = {}
  if (scene === 'Dungeon Royale') ctx.globals.dungeon = {}  // a fresh run
  if (scene === 'Arena Royale') ctx.globals.arena = {}
  ctx.loadScene(scene)
}

//@program Home
// Title screen: pick ROYALE (2-minute castle battle) or INFINITE (endless
// waves), or visit the UPGRADES shop. Enter replays the last mode.
torches()
idle('ally', [0, 3, 1], false)
idle('foe', [6, 7, 9], false)
for (let i = 0; i < 2; i++) ref('banner' + i).image = itemFrame(4, Math.floor(T * 6 + i * 2) % 4)

const pickMode = (mode) => { royale.mode = mode; saveRoyale(); go(battleScene(mode)) }
if (button('royale', 0, -92, 160, 24, [0.42, 0.2, 0.22])) pickMode('royale')
else if (button('arena', 0, -122, 160, 24, [0.22, 0.38, 0.3])) pickMode('arena')
else if (button('infinite', 0, -152, 160, 24, [0.2, 0.3, 0.45])) pickMode('infinite')
else if (button('upgrades', 0, -182, 160, 24, [0.36, 0.28, 0.12])) go('Royale Upgrades')
else if (keyPressed('space', 'enter')) pickMode(royale.mode || 'royale')

setText('bank', 'GOLD ' + (royale.bank || 0))
const arena = ctx.storage.get('arena') || {}
setText('best', 'TROPHIES ' + (royale.trophies || 0) + '  ARENA ' + (arena.trophies || 0) + '  WAVE ' + (royale.best || 0))
const glow = 0.55 + 0.45 * (Math.sin(T * 4) * 0.5 + 0.5)
ref('play').fillColor = [1, 0.95, 0.8, glow]

//@program Upgrades
// The upgrade shop: every card with its level and bonus; spend banked gold
// to level one up (+12% power per level, up to LV 10). Saved instantly.
torches()
const DECK = ['knight', 'archers', 'captain', 'fireball', 'cleric', 'spikes', 'brazier', 'rogue']
const MAX_LEVEL = 10
const upgradeCost = (lv) => 10 * lv * (lv + 1)
if (!g.pop) { g.pop = new Array(8).fill(0); g.statusT = 0 }
const dtm = ctx.deltaSeconds
for (let i = 0; i < 8; i++) {
  const key = DECK[i]
  const lv = royale.levels[key] || 1
  const cost = upgradeCost(lv)
  const x = i % 2 === 0 ? -70 : 70, y = 128 - Math.floor(i / 2) * 74
  const maxed = lv >= MAX_LEVEL
  const afford = !maxed && (royale.bank || 0) >= cost
  setText('lv' + i, 'LV ' + lv)
  setText('bonus' + i, lv > 1 ? '+' + Math.round((lv - 1) * 12) + '% POWER' : 'BASE POWER')
  setText('up' + i + 'Label', maxed ? 'MAX' : 'UP ' + cost + 'G')
  ref('up' + i + 'Label').fillColor = afford ? [1, 0.95, 0.85, 1] : [0.6, 0.55, 0.6, 1]
  const color = maxed ? [0.25, 0.25, 0.3] : afford ? [0.2, 0.42, 0.22] : [0.22, 0.18, 0.24]
  // The level-up pop on the card art.
  g.pop[i] = Math.max(0, g.pop[i] - dtm)
  const card = ref('card' + i)
  const s = 1 + g.pop[i] * 1.2
  card.scaleX = s; card.scaleY = s
  if (button('up' + i, x + 18, y - 22, 76, 16, color)) {
    if (maxed) { g.status = DECK[i].toUpperCase() + ' IS MAXED'; g.statusT = 1.5 }
    else if (afford) {
      royale.bank -= cost
      royale.levels[key] = lv + 1
      saveRoyale()
      g.pop[i] = 0.25
      g.status = key.toUpperCase() + ' NOW LV ' + (lv + 1)
      g.statusT = 1.8
    } else { g.status = 'NEED ' + (cost - (royale.bank || 0)) + ' MORE GOLD'; g.statusT = 1.5 }
  }
}
setText('bank', 'GOLD ' + (royale.bank || 0))
g.statusT = Math.max(0, g.statusT - dtm)
setText('play', g.statusT > 0 ? g.status : 'WIN MATCHES TO EARN GOLD')
if (button('back', 0, -186, 150, 26, [0.3, 0.2, 0.42]) || keys.has('escape')) go('Royale Home')

//@program GameOver
// The match is over: the result, the run's numbers, then Play Again (same
// mode) or back to Menu.
torches()
const won = royale.result === 'win'
const isRoyale = royale.mode === 'royale' || royale.mode === 'arena'
// Victory: your army celebrates around a standing keep.
idle('foe', won ? [0, 3, 1] : [6, 8, 7], true)
if (won) { ref('keep').rotation = 0; ref('keep').fillColor = [1, 1, 1, 1] }
const [line1, line2] = isRoyale
  ? won ? ['VICTORY', royale.mode === 'arena' ? 'KING TOWER TAKEN' : 'CASTLE TAKEN'] : royale.result === 'draw' ? ['DRAW', 'TIME UP'] : ['DEFEAT', royale.mode === 'arena' ? 'OUTPLAYED BY THE AI' : 'KEEP FALLEN']
  : ['THE KEEP', 'HAS FALLEN']
for (const [id, text] of [['title1', line1], ['title2', line2]]) {
  setText(id, text)
  setText(id + 'Shadow', text)
  ref(id).fillColor = won ? [1, 0.86, 0.5, 1] : [1, 0.5, 0.35, 1]
}
const stats = royale.mode === 'arena'
  ? [['statWave', 'CROWNS ' + (royale.crowns || '0 - 0')], ['statGold', 'GOLD +' + royale.gold], ['statBest', 'ARENA TROPHIES ' + ((ctx.storage.get('arena') || {}).trophies || 0)]]
  : isRoyale
  ? [['statWave', won ? 'TROPHIES +30' : royale.result === 'draw' ? 'TROPHIES +0' : 'TROPHIES -15'], ['statGold', 'GOLD +' + royale.gold], ['statBest', 'TROPHIES ' + royale.trophies]]
  : [['statWave', 'WAVE ' + royale.wave], ['statGold', 'GOLD +' + royale.gold], ['statBest', 'BEST ' + royale.best]]
for (const [id, text] of stats) setText(id, text)

if (button('again', 0, -112, 150, 28, [0.2, 0.42, 0.22])) go(battleScene(royale.mode))
else if (button('menu', 0, -150, 150, 28, [0.3, 0.2, 0.42])) go('Royale Home')
else if (keyPressed('enter', 'space')) go(battleScene(royale.mode))
else if (keys.has('escape') || keys.has('m')) go('Royale Home')
