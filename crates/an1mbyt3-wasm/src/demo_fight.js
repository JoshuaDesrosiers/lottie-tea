// Fatal Clash — a Mortal Kombat-style duel between two VRM avatars (see
// `demo_fight.rs`). Runs every frame as the scene's Code program.
//
// Two fighting styles, each its own stance, move list and animation:
//   POPI — VIPER (Muay Thai): bouncing high guard; jab › cross ›
//     roundhouse, teep (→K), knee (→P), low kick (↓K), uppercut (↓P),
//     flying knee (air).
//   SHADOW POPI — WRAITH (Kung Fu): low wide stance, open palms; palm ›
//     double palm › spinning hook kick (←K too), crescent kick (K), axe
//     kick (→K), spinning backfist (→P), spinning sweep (↓K), rising palm
//     (↓P), dive kick (air).
//
// Animation: every move is keyframes of named poses (humanoid bone
// rotations, VRM 1.0 axes — she faces +Z, her left is +X — plus a hips
// offset), with root motion (forward steps) and root yaw (spins). The
// *rig* on top is dynamic: every bone chases its keyframed target through
// a damped spring (so strikes snap, overshoot and settle), hits kick the
// springs (heads whip back, bodies fold), the head tracks the opponent,
// and the stance breathes / bounces / sways. VRM 0.x avatars face −Z, so
// their X / Z angles flip.
//
// Hits: a move connects when the opponent is within its reach (in front)
// during its active frames — high blows whiff over a crouch, lows need a
// crouching block. Knockdowns and KOs hand the avatar to the ragdoll.
//
// Prefix constants (from Rust): W, H, ARENA, DECALS, BLOOD, SPARKS, BRAZIERS.
const g = ctx.globals
if (!g.R) g.R = {}
const ref = (id) => g.R[id] || (g.R[id] = getCompByID(id))
const keys = ctx.keysDown
const rdt = Math.min(ctx.deltaSeconds || 1 / 60, 1 / 20)

const GRAVITY = 24
const WALK = 2.4
const BACK_WALK = 1.8
const JUMP_V = 8.2
const AIR_V = 3.2
const HP = 100
const ROUND_TIME = 60
/** Bodies never get closer than this (m). */
const PUSH = 0.62
const NAMES = ['POPI', 'SHADOW POPI']

const rnd = (a, b) => a + Math.random() * (b - a)
const chance = (p) => Math.random() < p
const mixf = (a, b, t) => a + (b - a) * t
const clampf = (v, a, b) => Math.max(a, Math.min(b, v))
const easeIO = (t) => t * t * (3 - 2 * t)

// ------------------------------------------------------------- poses --
const FIST = {}
for (const side of ['left', 'right']) {
  const s = side === 'left' ? 1 : -1
  for (const f of ['Index', 'Middle', 'Ring', 'Little']) for (const j of ['Proximal', 'Intermediate', 'Distal']) FIST[side + f + j] = [0, 0, -80 * s]
  FIST[side + 'ThumbProximal'] = [0, -35 * s, 0]
  FIST[side + 'ThumbDistal'] = [0, -45 * s, 0]
}
/** Open, slightly cupped hands (kung fu palms). */
const PALM = {}
for (const side of ['left', 'right']) {
  const s = side === 'left' ? 1 : -1
  for (const f of ['Index', 'Middle', 'Ring', 'Little']) {
    PALM[side + f + 'Proximal'] = [0, 0, -8 * s]
    PALM[side + f + 'Intermediate'] = [0, 0, -12 * s]
  }
  PALM[side + 'ThumbProximal'] = [0, -15 * s, 0]
}
/** A pose: bone → [x, y, z] degrees, and the hips offset (m). */
const P = (bones, hips = [0, 0, 0], hands = FIST) => ({ bones: { ...hands, ...bones }, hips })
const with_ = (base, bones, hips) => ({ bones: { ...base.bones, ...bones }, hips: hips || base.hips })

// VIPER (Muay Thai) ------------------------------------------------------
const V_ARMS = { leftUpperArm: [0, -55, -55], leftLowerArm: [0, -125, 0], leftHand: [0, 0, -10], rightUpperArm: [0, 42, 60], rightLowerArm: [0, 122, 0], rightHand: [0, 0, 10] }
const V_LEGS = { leftUpperLeg: [-24, 0, 6], leftLowerLeg: [32, 0, 0], leftFoot: [-10, 0, 0], rightUpperLeg: [16, 0, -8], rightLowerLeg: [28, 0, 0], rightFoot: [-16, 0, 0] }
const V_CROUCH = { leftUpperLeg: [-96, 0, 10], leftLowerLeg: [128, 0, 0], leftFoot: [-28, 0, 0], rightUpperLeg: [-70, 0, -10], rightLowerLeg: [132, 0, 0], rightFoot: [-46, 0, 0] }
const vStance = P({ hips: [0, -28, 0], spine: [8, 12, 0], chest: [2, 6, 0], head: [-4, 12, 0], ...V_ARMS, ...V_LEGS }, [0, -0.06, 0])
const VIPER = {
  stance: vStance,
  crouch: P({ hips: [0, -20, 0], spine: [26, 8, 0], head: [-14, 8, 0], ...V_ARMS, ...V_CROUCH }, [0, -0.4, 0.04]),
  block: with_(vStance, { spine: [16, 18, 0], head: [16, 10, 0], leftUpperArm: [0, -78, -40], leftLowerArm: [0, -140, 0], rightUpperArm: [0, 70, 44], rightLowerArm: [0, 140, 0] }, [0, -0.1, -0.03]),
  cblock: P({ hips: [0, -20, 0], spine: [32, 8, 0], head: [10, 0, 0], leftUpperArm: [0, -78, -40], leftLowerArm: [0, -140, 0], rightUpperArm: [0, 70, 44], rightLowerArm: [0, 140, 0], ...V_CROUCH }, [0, -0.42, 0]),
  jump: P({ spine: [12, 0, 0], ...V_ARMS, leftUpperLeg: [-75, 0, 8], leftLowerLeg: [108, 0, 0], rightUpperLeg: [-38, 0, -8], rightLowerLeg: [96, 0, 0] }),
  jab: with_(vStance, { hips: [0, -14, 0], spine: [8, 28, 0], chest: [0, 12, 0], head: [-4, -4, 0], leftUpperArm: [0, -91, -4], leftLowerArm: [0, -4, 0], leftHand: [0, 0, 0] }, [0, -0.07, 0.06]),
  cross: with_(vStance, { hips: [0, 24, 0], spine: [10, 34, 0], chest: [0, 14, 0], head: [-4, -30, 0], rightUpperArm: [0, 92, 3], rightLowerArm: [0, 4, 0], rightHand: [0, 0, 0], leftUpperArm: [0, -40, -62], leftLowerArm: [0, -128, 0], rightUpperLeg: [12, 18, -6], rightFoot: [-30, 0, 0] }, [0, -0.08, 0.09]),
  rhChamber: with_(vStance, { hips: [0, 40, 0], spine: [-4, 18, 10], head: [0, -26, 0], rightUpperLeg: [-72, 0, -48], rightLowerLeg: [112, 0, 0], rightUpperArm: [0, -30, 72], rightLowerArm: [0, 40, 0], leftUpperLeg: [-4, 0, 6], leftLowerLeg: [18, 0, 0] }, [0, -0.04, 0]),
  roundhouse: with_(vStance, { hips: [0, 78, 0], spine: [-12, 22, 20], chest: [0, 8, 8], head: [6, -50, -10], rightUpperLeg: [-38, 0, -96], rightLowerLeg: [6, 0, 0], rightFoot: [-20, 0, 0], rightUpperArm: [0, -55, 70], rightLowerArm: [0, 20, 0], leftUpperArm: [0, -60, -50], leftUpperLeg: [0, 0, 9], leftLowerLeg: [10, 0, 0] }, [0, 0.02, -0.03]),
  teepChamber: with_(vStance, { hips: [0, -10, 0], spine: [-6, 10, 0], leftUpperLeg: [-102, 0, 0], leftLowerLeg: [124, 0, 0], rightUpperLeg: [4, 0, -6], rightLowerLeg: [18, 0, 0] }, [0, -0.02, 0]),
  teep: with_(vStance, { hips: [0, -10, 0], spine: [-16, 10, 0], head: [8, 6, 0], leftUpperLeg: [-92, 0, 0], leftLowerLeg: [2, 0, 0], leftFoot: [-38, 0, 0], rightUpperLeg: [6, 0, -6], rightLowerLeg: [14, 0, 0], leftUpperArm: [0, -30, -60] }, [0, 0, -0.05]),
  knee: P({ hips: [0, 6, 0], spine: [14, 4, 0], head: [12, 0, 0], leftUpperArm: [0, -76, -36], leftLowerArm: [0, -62, 0], rightUpperArm: [0, 76, 36], rightLowerArm: [0, 62, 0], rightUpperLeg: [-120, 0, -4], rightLowerLeg: [142, 0, 0], rightFoot: [-30, 0, 0], leftUpperLeg: [-4, 0, 6], leftLowerLeg: [10, 0, 0], leftFoot: [10, 0, 0] }, [0, 0.05, 0.12]),
  lowChamber: with_(vStance, { hips: [0, 30, 0], spine: [10, 14, 0], rightUpperLeg: [-22, 0, -28], rightLowerLeg: [62, 0, 0] }),
  lowKick: with_(vStance, { hips: [0, 52, 0], spine: [16, 16, 8], head: [0, -36, 0], rightUpperLeg: [-30, 0, -58], rightLowerLeg: [8, 0, 0], leftUpperLeg: [-10, 0, 6], leftLowerLeg: [30, 0, 0], rightUpperArm: [0, -40, 70] }, [0, -0.1, 0]),
  upWind: P({ hips: [0, -30, 0], spine: [28, 18, 0], head: [-16, 10, 0], ...V_ARMS, rightUpperArm: [0, -18, 72], rightLowerArm: [0, 128, 0], ...V_CROUCH }, [0, -0.34, 0.04]),
  uppercut: P({ hips: [0, 30, 0], spine: [-16, 26, 0], chest: [-6, 10, 0], head: [-20, -12, 0], leftUpperArm: [0, -32, -56], leftLowerArm: [0, -112, 0], rightUpperArm: [0, 46, -74], rightLowerArm: [0, 28, -32], leftUpperLeg: [-14, 0, 6], leftLowerLeg: [12, 0, 0], rightUpperLeg: [10, 0, -6], rightLowerLeg: [8, 0, 0], rightFoot: [-30, 0, 0] }, [0, 0.06, 0.05]),
  flyKnee: P({ spine: [10, 0, 0], head: [10, 0, 0], leftUpperArm: [0, -76, -36], leftLowerArm: [0, -62, 0], rightUpperArm: [0, 76, 36], rightLowerArm: [0, 62, 0], rightUpperLeg: [-125, 0, 0], rightLowerLeg: [140, 0, 0], leftUpperLeg: [-20, 0, 6], leftLowerLeg: [60, 0, 0] }),
}

// WRAITH (Kung Fu) -------------------------------------------------------
const W_LEGS = { leftUpperLeg: [-26, 0, 22], leftLowerLeg: [48, 0, 0], leftFoot: [-16, 0, 0], rightUpperLeg: [12, 0, -24], rightLowerLeg: [44, 0, 0], rightFoot: [-12, 0, 0] }
const W_CROUCH = { leftUpperLeg: [-100, 0, 22], leftLowerLeg: [136, 0, 0], leftFoot: [-30, 0, 0], rightUpperLeg: [-60, 0, -24], rightLowerLeg: [136, 0, 0], rightFoot: [-46, 0, 0] }
const W_ARMS = { leftUpperArm: [0, -70, -28], leftLowerArm: [0, -45, 0], leftHand: [0, 0, 62], rightUpperArm: [0, 50, 56], rightLowerArm: [0, 126, 0], rightHand: [0, 0, -32] }
const wStance = P({ hips: [0, -44, 0], spine: [6, 22, 0], chest: [0, 10, 0], head: [-4, 22, 0], ...W_ARMS, ...W_LEGS }, [0, -0.17, 0], PALM)
const WRAITH = {
  stance: wStance,
  crouch: P({ hips: [0, -40, 0], spine: [24, 16, 0], head: [-12, 16, 0], ...W_ARMS, ...W_CROUCH }, [0, -0.45, 0.02], PALM),
  block: with_(wStance, { spine: [14, 22, 0], head: [10, 16, 0], leftUpperArm: [0, -82, -34], leftLowerArm: [0, -118, 0], leftHand: [0, 0, 40], rightUpperArm: [0, 82, 34], rightLowerArm: [0, 118, 0], rightHand: [0, 0, -40] }, [0, -0.2, -0.03]),
  cblock: P({ hips: [0, -40, 0], spine: [32, 16, 0], head: [10, 10, 0], leftUpperArm: [0, -82, -34], leftLowerArm: [0, -118, 0], rightUpperArm: [0, 82, 34], rightLowerArm: [0, 118, 0], ...W_CROUCH }, [0, -0.46, 0], PALM),
  jump: P({ spine: [16, 0, 0], ...W_ARMS, leftUpperLeg: [-90, 0, 10], leftLowerLeg: [125, 0, 0], rightUpperLeg: [-70, 0, -10], rightLowerLeg: [125, 0, 0] }, [0, 0, 0], PALM),
  palm: with_(wStance, { hips: [0, -26, 0], spine: [6, 38, 0], chest: [0, 14, 0], head: [-4, 4, 0], leftUpperArm: [0, -93, -2], leftLowerArm: [0, -2, 0], leftHand: [0, 0, 78] }, [0, -0.16, 0.07]),
  dpWind: with_(wStance, { hips: [0, 0, 0], spine: [-4, 0, 0], leftUpperArm: [0, 25, -55], leftLowerArm: [0, -128, 0], rightUpperArm: [0, -25, 55], rightLowerArm: [0, 128, 0] }, [0, -0.18, -0.04]),
  dpalm: with_(wStance, { hips: [0, 0, 0], spine: [12, 0, 0], head: [-6, 0, 0], leftUpperArm: [0, -86, -12], leftLowerArm: [0, -6, 0], leftHand: [0, 0, 78], rightUpperArm: [0, 86, 12], rightLowerArm: [0, 6, 0], rightHand: [0, 0, -78], leftUpperLeg: [-36, 0, 18], leftLowerLeg: [40, 0, 0], rightUpperLeg: [24, 0, -18], rightLowerLeg: [20, 0, 0] }, [0, -0.15, 0.13]),
  hookChamber: with_(wStance, { hips: [0, -20, 0], spine: [0, 10, 10], rightUpperLeg: [-52, 0, -42], rightLowerLeg: [92, 0, 0], leftUpperLeg: [-6, 0, 8], leftLowerLeg: [24, 0, 0] }, [0, -0.06, 0]),
  hook: P({ hips: [0, -10, 0], spine: [-6, 10, 24], head: [4, 0, -14], leftUpperArm: [0, 10, -40], leftLowerArm: [0, -40, 0], rightUpperArm: [0, -10, 40], rightLowerArm: [0, 40, 0], rightUpperLeg: [-14, 0, -98], rightLowerLeg: [0, 0, 0], rightFoot: [-10, 0, 0], leftUpperLeg: [0, 0, 10], leftLowerLeg: [12, 0, 0] }, [0, 0.03, 0], PALM),
  cres1: with_(wStance, { hips: [0, -10, 0], spine: [-8, 10, 0], rightUpperLeg: [-74, 0, -60], rightLowerLeg: [4, 0, 0], leftUpperLeg: [-6, 0, 10], leftLowerLeg: [16, 0, 0] }, [0, -0.06, 0]),
  cres2: with_(wStance, { hips: [0, 0, 0], spine: [-16, 4, 0], head: [12, 0, 0], rightUpperLeg: [-128, 0, -6], rightLowerLeg: [2, 0, 0], leftUpperLeg: [-4, 0, 10], leftLowerLeg: [10, 0, 0] }, [0, -0.02, -0.04]),
  cres3: with_(wStance, { hips: [0, 10, 0], spine: [-6, -6, 0], rightUpperLeg: [-70, 0, 42], rightLowerLeg: [6, 0, 0], leftUpperLeg: [-6, 0, 10], leftLowerLeg: [18, 0, 0] }, [0, -0.06, 0]),
  axeUp: with_(wStance, { hips: [0, -10, 0], spine: [-22, 6, 0], head: [16, 0, 0], leftUpperLeg: [-158, 0, 0], leftLowerLeg: [0, 0, 0], leftFoot: [-30, 0, 0], rightUpperLeg: [2, 0, -10], rightLowerLeg: [10, 0, 0], leftUpperArm: [0, 10, -60], rightUpperArm: [0, -10, 60] }, [0, 0.02, -0.06]),
  axeDown: with_(wStance, { hips: [0, -10, 0], spine: [28, 6, 0], head: [-10, 0, 0], leftUpperLeg: [-48, 0, 0], leftLowerLeg: [4, 0, 0], rightUpperLeg: [-10, 0, -10], rightLowerLeg: [38, 0, 0] }, [0, -0.14, 0.08]),
  backfist: P({ hips: [0, 0, 0], spine: [4, 0, -12], head: [-4, 0, 10], rightUpperArm: [0, 8, 0], rightLowerArm: [0, 0, 0], rightHand: [0, 0, 0], leftUpperArm: [0, -40, -66], leftLowerArm: [0, -120, 0], ...W_LEGS }, [0, -0.12, 0], FIST),
  sweep: P({ hips: [0, -10, 0], spine: [42, 10, 0], head: [-26, 0, 0], leftUpperArm: [0, -30, -82], leftLowerArm: [0, -10, 0], rightUpperArm: [0, 30, 82], rightLowerArm: [0, 10, 0], leftUpperLeg: [-112, 0, 22], leftLowerLeg: [142, 0, 0], leftFoot: [-30, 0, 0], rightUpperLeg: [-82, 0, -76], rightLowerLeg: [2, 0, 0], rightFoot: [-10, 0, 0] }, [0, -0.56, 0.02], PALM),
  rpWind: P({ hips: [0, -40, 0], spine: [30, 20, 0], head: [-14, 10, 0], ...W_ARMS, rightUpperArm: [0, 10, 76], rightLowerArm: [0, 20, 0], rightHand: [0, 0, -70], ...W_CROUCH }, [0, -0.4, 0.03], PALM),
  risingPalm: P({ hips: [0, 10, 0], spine: [-14, 24, 0], head: [-20, -10, 0], leftUpperArm: [0, -40, -60], leftLowerArm: [0, -110, 0], rightUpperArm: [0, 30, -82], rightLowerArm: [0, 0, -8], rightHand: [0, 0, -72], ...W_LEGS }, [0, 0.04, 0.05], PALM),
  dive: P({ spine: [16, 0, 0], head: [-8, 0, 0], ...W_ARMS, rightUpperLeg: [-44, 0, 0], rightLowerLeg: [0, 0, 0], rightFoot: [-40, 0, 0], leftUpperLeg: [-96, 0, 6], leftLowerLeg: [128, 0, 0] }, [0, 0, 0], PALM),
}

// Shared reactions ---------------------------------------------------------
const COMMON = {
  hitHigh: P({ spine: [-20, 12, 0], chest: [-12, 0, 0], head: [-30, -14, 8], leftUpperArm: [0, 25, -45], leftLowerArm: [0, -40, 0], rightUpperArm: [0, -20, 50], rightLowerArm: [0, 30, 0], leftUpperLeg: [-10, 0, 6], leftLowerLeg: [25, 0, 0], rightUpperLeg: [16, 0, -6], rightLowerLeg: [22, 0, 0] }, [0, -0.04, -0.08]),
  hitBody: P({ spine: [36, 0, 0], chest: [14, 0, 0], head: [14, 0, 0], leftUpperArm: [0, -50, -60], leftLowerArm: [0, -100, 0], rightUpperArm: [0, 50, 60], rightLowerArm: [0, 100, 0], leftUpperLeg: [-16, 0, 6], leftLowerLeg: [30, 0, 0], rightUpperLeg: [-6, 0, -6], rightLowerLeg: [30, 0, 0] }, [0, -0.12, -0.06]),
  hitLow: P({ spine: [10, 0, 10], head: [6, 0, 6], leftUpperArm: [0, -10, -60], rightUpperArm: [0, 10, 70], leftUpperLeg: [-30, 0, 10], leftLowerLeg: [60, 0, 0], rightUpperLeg: [20, 0, -6], rightLowerLeg: [10, 0, 0] }, [0, -0.15, 0]),
  dizzy: P({ spine: [14, 0, 6], head: [26, 0, 12], leftUpperArm: [0, 4, -76], leftLowerArm: [0, -14, 0], rightUpperArm: [0, -4, 76], rightLowerArm: [0, 14, 0], leftUpperLeg: [-10, 0, 4], leftLowerLeg: [20, 0, 0], rightUpperLeg: [-4, 0, -4], rightLowerLeg: [16, 0, 0] }, [0, -0.06, 0], PALM),
  victory: P({ spine: [-6, 0, 0], head: [-14, 0, 0], rightUpperArm: [0, 10, -80], rightLowerArm: [0, 0, -25], leftUpperArm: [0, -45, -62], leftLowerArm: [0, -110, 0], leftUpperLeg: [0, 0, 6], rightUpperLeg: [0, 0, -6] }, [0, 0, 0]),
}

// -------------------------------------------------------------- moves --
// keys: [time, pose name]; hit: [from, to] (s) — once, if the opponent is
// within `reach` (m) in front; root: forward metres by time; yaw: extra
// body turn by time (spins); chain: what the next press turns into.
const VIPER_MOVES = {
  jab: { keys: [[0, 'stance'], [0.06, 'jab'], [0.15, 'jab'], [0.3, 'stance']], hit: [0.04, 0.14], reach: 0.95, level: 'high', dmg: 4, stun: 0.26, kb: 1.2, root: [[0, 0], [0.08, 0.1]], chain: { P: 'cross', K: 'roundhouse' }, from: 0.08, name: 'JAB', snd: 'hitLight', whoosh: 'whoosh' },
  cross: { keys: [[0, 'jab'], [0.08, 'cross'], [0.18, 'cross'], [0.36, 'stance']], hit: [0.06, 0.16], reach: 1.05, level: 'high', dmg: 6, stun: 0.3, kb: 1.6, root: [[0, 0], [0.1, 0.14]], chain: { K: 'roundhouse' }, from: 0.1, name: 'CROSS', snd: 'hitMid', whoosh: 'whoosh' },
  roundhouse: { keys: [[0, 'stance'], [0.12, 'rhChamber'], [0.22, 'roundhouse'], [0.34, 'roundhouse'], [0.6, 'stance']], hit: [0.18, 0.3], reach: 1.35, level: 'mid', dmg: 10, stun: 0.42, kb: 2.8, root: [[0, 0], [0.22, 0.12]], name: 'ROUNDHOUSE', snd: 'hitHeavy', whoosh: 'whooshBig' },
  teep: { keys: [[0, 'stance'], [0.1, 'teepChamber'], [0.18, 'teep'], [0.28, 'teep'], [0.48, 'stance']], hit: [0.15, 0.26], reach: 1.25, level: 'mid', dmg: 7, stun: 0.36, kb: 4, name: 'TEEP', snd: 'hitMid', whoosh: 'whoosh' },
  knee: { keys: [[0, 'stance'], [0.16, 'knee'], [0.3, 'knee'], [0.5, 'stance']], hit: [0.12, 0.26], reach: 0.85, level: 'mid', dmg: 11, stun: 0.48, kb: 1.6, root: [[0, 0], [0.16, 0.35]], name: 'KNEE STRIKE', snd: 'hitHeavy', whoosh: 'whoosh' },
  lowKick: { keys: [[0, 'stance'], [0.1, 'lowChamber'], [0.18, 'lowKick'], [0.3, 'lowKick'], [0.48, 'stance']], hit: [0.14, 0.26], reach: 1.15, level: 'low', dmg: 6, stun: 0.34, kb: 1.2, name: 'LOW KICK', snd: 'hitMid', whoosh: 'whoosh' },
  uppercut: { keys: [[0, 'crouch'], [0.12, 'upWind'], [0.2, 'uppercut'], [0.34, 'uppercut'], [0.64, 'stance']], hit: [0.15, 0.27], reach: 0.95, level: 'mid', dmg: 13, launch: [2.8, 7], root: [[0, 0], [0.2, 0.18]], name: 'UPPERCUT', snd: 'hitHeavy', whoosh: 'whooshBig' },
  air: { keys: [[0, 'jump'], [0.08, 'flyKnee']], hit: [0.05, 9], reach: 0.9, level: 'mid', dmg: 9, stun: 0.42, kb: 2.6, air: true, name: 'FLYING KNEE', snd: 'hitHeavy', whoosh: 'whooshBig' },
}
const WRAITH_MOVES = {
  jab: { keys: [[0, 'stance'], [0.07, 'palm'], [0.16, 'palm'], [0.3, 'stance']], hit: [0.05, 0.15], reach: 1.0, level: 'high', dmg: 4, stun: 0.26, kb: 1.4, root: [[0, 0], [0.07, 0.12]], chain: { P: 'dpalm', K: 'hook' }, from: 0.08, name: 'PALM STRIKE', snd: 'hitLight', whoosh: 'whoosh' },
  dpalm: { keys: [[0, 'palm'], [0.06, 'dpWind'], [0.14, 'dpalm'], [0.26, 'dpalm'], [0.44, 'stance']], hit: [0.1, 0.22], reach: 1.1, level: 'mid', dmg: 7, stun: 0.4, kb: 3.4, root: [[0, 0], [0.14, 0.3]], chain: { K: 'hook' }, from: 0.14, name: 'TWIN PALMS', snd: 'hitMid', whoosh: 'whooshBig' },
  hook: { keys: [[0, 'stance'], [0.08, 'hookChamber'], [0.18, 'hook'], [0.38, 'hook'], [0.58, 'stance']], hit: [0.22, 0.36], reach: 1.4, level: 'high', dmg: 10, stun: 0.45, kb: 3, yaw: [[0, 0], [0.08, 30], [0.4, 360], [0.58, 360]], name: 'SPIN HOOK KICK', snd: 'hitHeavy', whoosh: 'whooshBig' },
  kick: { keys: [[0, 'stance'], [0.1, 'cres1'], [0.2, 'cres2'], [0.3, 'cres3'], [0.52, 'stance']], hit: [0.14, 0.3], reach: 1.3, level: 'high', dmg: 8, stun: 0.38, kb: 2.2, name: 'CRESCENT KICK', snd: 'hitMid', whoosh: 'whooshBig' },
  axe: { keys: [[0, 'stance'], [0.2, 'axeUp'], [0.3, 'axeDown'], [0.42, 'axeDown'], [0.64, 'stance']], hit: [0.25, 0.35], reach: 1.15, level: 'mid', dmg: 11, stun: 0.5, kb: 1.5, root: [[0, 0], [0.3, 0.2]], name: 'AXE KICK', snd: 'hitHeavy', whoosh: 'whooshBig' },
  backfist: { keys: [[0, 'stance'], [0.06, 'backfist'], [0.3, 'backfist'], [0.46, 'stance']], hit: [0.18, 0.3], reach: 1.2, level: 'high', dmg: 8, stun: 0.36, kb: 2, yaw: [[0, 0], [0.04, 20], [0.32, 360], [0.46, 360]], root: [[0, 0], [0.3, 0.2]], name: 'SPINNING BACKFIST', snd: 'hitMid', whoosh: 'whooshBig' },
  sweep: { keys: [[0, 'crouch'], [0.08, 'sweep'], [0.44, 'sweep'], [0.64, 'stance']], hit: [0.14, 0.42], reach: 1.45, level: 'low', dmg: 7, launch: [1.2, 2.2], yaw: [[0, 0], [0.08, 30], [0.44, 360], [0.64, 360]], name: 'SPINNING SWEEP', snd: 'hitMid', whoosh: 'whooshBig' },
  uppercut: { keys: [[0, 'crouch'], [0.12, 'rpWind'], [0.2, 'risingPalm'], [0.34, 'risingPalm'], [0.62, 'stance']], hit: [0.15, 0.27], reach: 0.95, level: 'mid', dmg: 12, launch: [2.6, 6.6], root: [[0, 0], [0.2, 0.16]], name: 'RISING PALM', snd: 'hitHeavy', whoosh: 'whooshBig' },
  air: { keys: [[0, 'jump'], [0.08, 'dive']], hit: [0.05, 9], reach: 1.0, level: 'mid', dmg: 9, stun: 0.42, kb: 2.6, air: true, dive: true, name: 'DIVE KICK', snd: 'hitHeavy', whoosh: 'whooshBig' },
}
// Input → move, per style: base presses, forward / back / down variants.
const STYLES = [
  { name: 'VIPER', poses: VIPER, moves: VIPER_MOVES, map: { P: 'jab', K: 'roundhouse', FP: 'knee', FK: 'teep', BK: 'teep', DP: 'uppercut', DK: 'lowKick' }, spring: { k: 360, z: 0.5 }, bounce: 1, cpu: ['jab', 'jab', 'roundhouse', 'teep', 'knee', 'lowKick', 'uppercut'] },
  { name: 'WRAITH', poses: WRAITH, moves: WRAITH_MOVES, map: { P: 'jab', K: 'kick', FP: 'backfist', FK: 'axe', BK: 'hook', DP: 'uppercut', DK: 'sweep' }, spring: { k: 260, z: 0.62 }, bounce: 0, cpu: ['jab', 'jab', 'kick', 'axe', 'backfist', 'hook', 'sweep', 'uppercut'] },
]

/** A move's keyframed pose at time t (blended between its keys). */
function sampleMove(style, m, t) {
  const k = m.keys
  let i = 0
  while (i < k.length - 1 && t >= k[i + 1][0]) i++
  const a = k[i]
  const b = k[Math.min(i + 1, k.length - 1)]
  const span = b[0] - a[0]
  const u = span > 0 ? easeIO(clampf((t - a[0]) / span, 0, 1)) : 1
  return blendPose(poseOf(style, a[1]), poseOf(style, b[1]), u)
}
function poseOf(style, name) {
  return style.poses[name] || COMMON[name] || style.poses.stance
}
function blendPose(a, b, u) {
  const bones = {}
  for (const n of new Set([...Object.keys(a.bones), ...Object.keys(b.bones)])) {
    const x = a.bones[n] || [0, 0, 0]
    const y = b.bones[n] || [0, 0, 0]
    bones[n] = [mixf(x[0], y[0], u), mixf(x[1], y[1], u), mixf(x[2], y[2], u)]
  }
  return { bones, hips: [mixf(a.hips[0], b.hips[0], u), mixf(a.hips[1], b.hips[1], u), mixf(a.hips[2], b.hips[2], u)] }
}
/** A [time, value] curve at t. */
function curve(c, t) {
  if (!c) return 0
  if (t <= c[0][0]) return c[0][1]
  for (let i = 0; i < c.length - 1; i++) {
    if (t <= c[i + 1][0]) {
      const u = (t - c[i][0]) / Math.max(1e-6, c[i + 1][0] - c[i][0])
      return mixf(c[i][1], c[i + 1][1], easeIO(u))
    }
  }
  return c[c.length - 1][1]
}
/** A walk cycle on the stance (phase in radians). */
function walkPose(style, phase, amount) {
  const base = style.poses.stance
  const p = { bones: { ...base.bones }, hips: [...base.hips] }
  const s = Math.sin(phase) * amount
  const c = Math.cos(phase) * amount
  const add = (b, x) => { const v = p.bones[b] || [0, 0, 0]; p.bones[b] = [v[0] + x, v[1], v[2]] }
  add('leftUpperLeg', -24 * s)
  add('rightUpperLeg', 24 * s)
  add('leftLowerLeg', 32 * Math.max(0, c))
  add('rightLowerLeg', 32 * Math.max(0, -c))
  p.hips[1] += -0.025 * Math.abs(c)
  return p
}

// ------------------------------------------------------------- setup --
function newFighter(i) {
  return {
    i, id: 'f' + (i + 1), style: STYLES[i], x: i === 0 ? -1.6 : 1.6, y: 0, vx: 0, vy: 0, dir: i === 0 ? 1 : -1,
    hp: HP, ghost: HP, wins: 0, state: 'idle', st: 0, move: null, moveT: 0, hitDone: false, chainNext: null,
    stun: 0, kb: 0, combo: 0, comboT: 0, walkPh: 0, rig: {}, hips: { r: [0, 0, 0], v: [0, 0, 0] }, buf: [], down: 0,
    prev: {}, ai: { t: 0, plan: null, hold: 0 }, yaw: 0, callT: 0, call: '',
  }
}

if (!g.init) {
  g.init = true
  g.t = 0
  g.phase = 'menu'
  g.cpu = true
  g.diff = 'normal'
  g.F = [newFighter(0), newFighter(1)]
  g.round = 1
  g.timer = ROUND_TIME
  g.banner = null
  g.shake = 0
  g.flash = 0
  g.slow = 0
  g.decal = 0
  g.bloodI = 0
  g.req = null
  ctx.setEnvironment3({ fog: [0.035, 0.03, 0.065], fogDensity: 0.03, ambient: [0.26, 0.24, 0.36] })
}
const F = g.F

/** VRM 0.x avatars face −Z: their X / Z angles flip. */
function flipOf(f) {
  if (f.flip === undefined || f.flip === null) {
    const info = ref(f.id).modelInfo3
    if (info) f.flip = info.forwardZ < 0
  }
  return !!f.flip
}

function resetRound() {
  for (const f of F) {
    const keep = { wins: f.wins, flip: f.flip }
    Object.assign(f, newFighter(f.i), keep)
    const r = ref(f.id)
    if (r.isRagdoll3) r.ragdoll3(false)
    r.clearBones3()
  }
  for (let i = 0; i < DECALS; i++) ref('bd' + i).visible = false
  g.timer = ROUND_TIME
  g.slow = 0
}

function startMatch(cpu) {
  g.cpu = cpu
  g.lastFatal = false
  for (const f of F) f.wins = 0
  g.round = 1
  beginRound()
  if (!g.drums) g.drums = ctx.playSound('drums', { loop: true, volume: 0.3 })
}

function beginRound() {
  resetRound()
  g.phase = 'intro'
  g.phaseT = 0
  showBanner(g.round === 3 ? 'FINAL ROUND' : 'ROUND ' + g.round, '', 1.3)
  ctx.playSound('gong', { volume: 0.7 })
}

function showBanner(big, small, time, red) {
  g.banner = { big, small, t: time, red: !!red }
}

// --------------------------------------------------------------- input --
function humanCmd(f) {
  const map = f.i === 0
    ? { left: ['a'], right: ['d'], up: ['w'], down: ['s'], punch: ['j'], kick: ['k'], block: ['l'] }
    : { left: ['left'], right: ['right'], up: ['up'], down: ['down'], punch: ['1', ','], kick: ['2', '.'], block: ['3', '/'] }
  const now = {}
  for (const k of Object.keys(map)) now[k] = map[k].some((n) => keys.has(n))
  const edge = (k) => now[k] && !f.prev[k]
  const fwdKey = f.dir > 0 ? 'right' : 'left'
  const backKey = f.dir > 0 ? 'left' : 'right'
  const push = (k) => { f.buf.push({ k, t: g.t }); if (f.buf.length > 8) f.buf.shift() }
  if (edge(fwdKey)) push('F')
  if (edge(backKey)) push('B')
  if (edge('down')) push('D')
  if (edge('punch')) push('P')
  if (edge('kick')) push('K')
  // (Edges are read before `prev` moves on to this frame.)
  const pressedP = edge('punch')
  const pressedK = edge('kick')
  const jumped = edge('up')
  f.prev = now
  const cmd = { move: (now[fwdKey] ? 1 : 0) - (now[backKey] ? 1 : 0), jump: jumped, crouch: now.down, block: now.block, press: null, dirKey: '' }
  cmd.dirKey = now.down ? 'D' : now[fwdKey] ? 'F' : now[backKey] ? 'B' : ''
  if (pressedP) cmd.press = 'P'
  if (pressedK) cmd.press = buffered(f, ['D', 'F', 'D', 'K'], 1.4) ? 'FATAL' : 'K'
  return cmd
}

function buffered(f, seq, window) {
  const tail = f.buf.slice(-seq.length)
  if (tail.length < seq.length) return false
  return tail.every((b, i) => b.k === seq[i]) && g.t - tail[0].t <= window
}

/** The CPU: closes in, picks from its style's moves, blocks / punishes. */
function cpuCmd(me, op) {
  const skill = { easy: { think: 0.36, block: 0.2, aggro: 0.55 }, normal: { think: 0.22, block: 0.45, aggro: 0.8 }, hard: { think: 0.12, block: 0.75, aggro: 1 } }[g.diff]
  const cmd = { move: 0, jump: false, crouch: false, block: false, press: null, moveName: null }
  const d = Math.abs(op.x - me.x)
  const ai = me.ai
  ai.t -= rdt
  if (g.phase === 'finish' && op.state === 'dizzy') {
    if (d > 1.4) cmd.move = 1
    else if (ai.t <= 0) { cmd.press = chance(0.6) ? 'FATAL' : 'K'; ai.t = 1.2 }
    return cmd
  }
  const threat = op.state === 'attack' && op.move && d < (op.move.reach || 1) + 0.6
  if (threat && (ai.plan === 'block' || (ai.plan !== 'attack' && chance(skill.block * rdt * 16)))) {
    ai.plan = 'block'
    ai.hold = 0.32
    cmd.block = true
    cmd.crouch = op.move.level === 'low'
    return cmd
  }
  if (ai.plan === 'block' && (ai.hold -= rdt) > 0) {
    cmd.block = true
    return cmd
  }
  if (ai.t > 0 && ai.plan === 'walk') {
    cmd.move = ai.dir
    return cmd
  }
  if (ai.plan === 'chain' && me.state === 'attack' && me.move && me.moveT > (me.move.from || 0.1)) {
    cmd.press = ai.chainPress
    ai.plan = null
    return cmd
  }
  if (ai.t > 0) return cmd
  ai.t = skill.think * rnd(0.7, 1.4)
  ai.plan = null
  if (d > 1.5) {
    if (chance(0.1 * skill.aggro) && d < 3.2) { cmd.jump = true; cmd.move = 1; ai.airAt = g.t + 0.28 }
    else { ai.plan = 'walk'; ai.dir = chance(0.88) ? 1 : -1; cmd.move = ai.dir }
  } else if (chance(skill.aggro)) {
    ai.plan = 'attack'
    cmd.moveName = me.style.cpu[Math.floor(Math.random() * me.style.cpu.length)]
    if (cmd.moveName === 'jab' && chance(0.55)) {
      ai.plan = 'chain'
      ai.chainPress = chance(0.5) ? 'P' : 'K'
    }
  } else {
    ai.plan = 'walk'
    ai.dir = -1
    cmd.move = -1
    ai.t = 0.35
  }
  if (ai.airAt && g.t > ai.airAt && me.state === 'air') { cmd.press = 'K'; ai.airAt = 0 }
  return cmd
}

// ------------------------------------------------------------ fighting --
const grounded = (f) => f.y <= 0.001
const knocked = (f) => f.state === 'down' || f.state === 'ko'

function setState(f, s) {
  if (f.state !== s) {
    f.state = s
    f.st = 0
  }
}

function startMove(f, name) {
  const m = f.style.moves[name]
  if (!m) return
  f.move = m
  f.moveName = name
  f.moveT = 0
  f.hitDone = false
  f.rootPrev = 0
  f.chainNext = null
  setState(f, 'attack')
  if (m.dive) { f.vy = Math.min(f.vy, -5.5); f.vx = f.dir * 6.5 }
  if (m.whoosh) ctx.playSound(m.whoosh, { volume: 0.32, pitch: rnd(0.9, 1.15) })
}

/** Which move a press makes, from the style's input map. */
function moveFor(f, cmd) {
  const map = f.style.map
  if (!cmd.press) return null
  return map[(cmd.dirKey || '') + cmd.press] || map[cmd.press]
}

function updateFighter(f, op, cmd, dt) {
  f.st += dt
  f.comboT -= dt
  if (f.comboT <= 0) f.combo = 0
  f.callT -= dt
  if (grounded(f) && f.state !== 'attack' && !knocked(f)) f.dir = op.x >= f.x ? 1 : -1

  switch (f.state) {
    case 'idle': case 'walk': case 'crouch': case 'block': case 'cblock': {
      if (!cmd) break
      if (cmd.press === 'FATAL' && g.phase === 'finish' && op.state === 'dizzy' && Math.abs(op.x - f.x) < 2.3) {
        fatality(f, op)
        break
      }
      const name = cmd.moveName || (cmd.press === 'FATAL' ? f.style.map.K : moveFor(f, cmd))
      if (name) {
        startMove(f, name)
        break
      }
      if (cmd.jump) {
        f.vy = JUMP_V
        f.vx = cmd.move * f.dir * AIR_V
        f.y = 0.001
        setState(f, 'air')
        break
      }
      if (cmd.block) setState(f, cmd.crouch ? 'cblock' : 'block')
      else if (cmd.crouch) setState(f, 'crouch')
      else if (cmd.move) {
        setState(f, 'walk')
        f.x += cmd.move * f.dir * (cmd.move > 0 ? WALK : BACK_WALK) * dt
        f.walkPh += dt * (cmd.move > 0 ? 9 : -8)
      } else setState(f, 'idle')
      break
    }
    case 'air': {
      if (cmd && cmd.press) startMove(f, 'air')
      break
    }
    case 'attack': {
      const m = f.move
      f.moveT += dt
      // Root motion: forward steps along the move.
      const root = curve(m.root, f.moveT)
      f.x += (root - (f.rootPrev || 0)) * f.dir
      f.rootPrev = root
      f.yaw = curve(m.yaw, f.moveT)
      // Chains: a press inside the window queues the follow-up.
      if (cmd && cmd.press && m.chain && f.moveT >= (m.from || 0.1)) {
        const next = m.chain[cmd.press === 'FATAL' ? 'K' : cmd.press]
        if (next) f.chainNext = next
      }
      const end = m.keys[m.keys.length - 1][0]
      if (f.chainNext && f.moveT >= (m.from || 0.1) && (f.hitDone || f.moveT > m.hit[1])) {
        const next = f.chainNext
        f.yaw = 0
        startMove(f, next)
        break
      }
      if (!m.air && f.moveT >= end) {
        f.move = null
        f.yaw = 0
        setState(f, 'idle')
      }
      break
    }
    case 'hit': case 'bstun': {
      f.stun -= dt
      if (f.stun <= 0) setState(f, 'idle')
      break
    }
    case 'down': {
      f.down -= dt
      const hips = ref(f.id).bone3('hips')
      if (hips) f.ragX = hips.x
      if (f.down <= 0 && f.hp > 0) {
        ref(f.id).ragdoll3(false)
        if (f.ragX !== undefined) f.x = clampf(f.ragX, -ARENA, ARENA)
        f.rig = {}
        setState(f, 'getup')
      }
      break
    }
    case 'getup': {
      if (f.st > 0.5) setState(f, 'idle')
      break
    }
    case 'ko': {
      const hips = ref(f.id).bone3('hips')
      if (hips) f.ragX = hips.x
      break
    }
  }

  // Jumps, dives and falls.
  if (f.state === 'air' || (f.state === 'attack' && f.move?.air) || f.y > 0) {
    f.vy -= GRAVITY * dt
    f.y += f.vy * dt
    f.x += f.vx * dt
    if (f.y <= 0) {
      f.y = 0
      f.vy = 0
      f.vx = 0
      f.hips.v[1] -= 1.2 // landing squash
      if (f.state === 'air' || (f.state === 'attack' && f.move?.air)) {
        f.move = null
        setState(f, 'idle')
      }
    }
  }
  if (f.state === 'hit' || f.state === 'bstun') {
    f.x += f.kb * dt
    f.kb *= Math.pow(0.015, dt)
  }
  if (!knocked(f)) f.x = clampf(f.x, -ARENA, ARENA)
}

/** Did `a`'s live move reach `d`? Then it lands (or is blocked). */
function checkHits(a, d) {
  if (a.state !== 'attack' || !a.move || a.hitDone) return
  const m = a.move
  if (a.moveT < m.hit[0] || a.moveT > m.hit[1]) return
  if (knocked(d) || d.state === 'getup') return
  const ahead = (d.x - a.x) * a.dir
  if (ahead < -0.2 || ahead > m.reach + 0.25) return
  if (Math.abs(d.y - a.y) > 1.1) return
  // High blows sail over a crouch.
  if (m.level === 'high' && (d.state === 'crouch' || d.state === 'cblock')) return
  a.hitDone = true
  const h = m.level === 'high' ? 1.42 : m.level === 'low' ? 0.42 : 1.05
  const at = { x: a.x + a.dir * Math.min(Math.max(ahead - 0.15, 0.3), m.reach), y: d.y + h, z: rnd(-0.05, 0.05) }
  land(a, d, m, at)
}

function land(a, d, m, at) {
  const blocking = (d.state === 'block' && m.level !== 'low') || (d.state === 'cblock' && m.level !== 'high') || (d.state === 'bstun' && d.blocking)
  const facing = Math.sign(a.x - d.x) === d.dir
  if (blocking && facing) {
    d.hp = Math.max(1, d.hp - m.dmg * 0.1)
    d.stun = 0.16
    d.kb = a.dir * 1.6
    d.blocking = true
    setState(d, 'bstun')
    kick(d, 'block')
    spark(at)
    ctx.playSound('block', { volume: 0.55, pitch: rnd(0.9, 1.1) })
    return
  }
  d.blocking = false
  a.combo = d.state === 'hit' ? a.combo + 1 : 1
  a.comboT = 0.9
  a.call = m.name
  a.callT = 0.9
  const scale = Math.max(0.45, 1 - (a.combo - 1) * 0.12)
  d.hp = Math.max(0, d.hp - m.dmg * scale)
  bleed(at, m.dmg, a.dir)
  g.shake = Math.max(g.shake, 0.05 + m.dmg * 0.012)
  g.hitStop = 0.045 + m.dmg * 0.004
  ctx.playSound(m.snd || 'hitMid', { volume: 0.75, pitch: rnd(0.9, 1.1) })
  if (g.phase === 'finish' && d.state === 'dizzy') {
    knockOut(a, d, [a.dir * 3, 4])
    return
  }
  if (d.hp <= 0 && g.phase === 'fight') {
    roundKO(a, d, m)
    return
  }
  if (m.launch || d.y > 0) {
    const [vx, vy] = m.launch || [2.4, 3]
    knockDown(d, [a.dir * vx, vy], 1.4)
    return
  }
  d.stun = m.stun || 0.3
  d.kb = a.dir * (m.kb || 1.5)
  d.reaction = m.level === 'low' ? 'hitLow' : m.level === 'high' ? 'hitHigh' : 'hitBody'
  setState(d, 'hit')
  kick(d, d.reaction)
}

/** Hit recoil: kicks the rig's springs (head whips, torso folds). */
function kick(f, kind) {
  const add = (b, v) => {
    const r = f.rig[b] || (f.rig[b] = { r: [0, 0, 0], v: [0, 0, 0] })
    r.v[0] += v[0]
    r.v[1] += v[1]
    r.v[2] += v[2]
  }
  const s = Math.random() < 0.5 ? -1 : 1
  if (kind === 'hitHigh') { add('head', [-900, 300 * s, 200 * s]); add('spine', [-350, 0, 0]); add('chest', [-250, 0, 0]) }
  else if (kind === 'hitBody') { add('spine', [700, 0, 0]); add('chest', [400, 0, 0]); add('head', [400, 0, 0]) }
  else if (kind === 'hitLow') { add('leftUpperLeg', [-500, 0, 0]); add('spine', [200, 0, 300 * s]) }
  else { add('leftLowerArm', [0, -300, 0]); add('rightLowerArm', [0, 300, 0]); add('head', [250, 0, 0]) }
  f.hips.v[2] -= 1.5
}

function knockDown(f, v, time) {
  f.down = time
  f.ragX = f.x
  setState(f, 'down')
  ref(f.id).ragdoll3(true, { velocity: [v[0], v[1], 0] })
  f.fallSound = 0.35
}

function roundKO(a, d, m) {
  if (a.wins + 1 >= 2) {
    g.phase = 'finish'
    g.phaseT = 0
    g.winner = a.i
    d.hp = 0
    setState(d, 'dizzy')
    d.x = clampf(d.x + a.dir * 0.6, -ARENA, ARENA)
    showBanner('FINISH HER', '', 2.2, true)
    ctx.playSound('boom', { volume: 0.5, pitch: 1.4 })
    return
  }
  knockOut(a, d, m.launch ? [a.dir * m.launch[0], m.launch[1]] : [a.dir * 3.5, 4.5])
}

function knockOut(a, d, v) {
  d.hp = 0
  setState(d, 'ko')
  ref(d.id).ragdoll3(true, { velocity: [v[0], v[1], 0] })
  d.fallSound = 0.35
  a.wins++
  g.winner = a.i
  g.phase = 'ko'
  g.phaseT = 0
  g.slow = 1.1
  g.flash = 0.6
  ctx.playSound('boom', { volume: 0.8 })
  showBanner(NAMES[a.i] + ' WINS', a.hp >= HP - 0.01 ? 'FLAWLESS VICTORY' : '', 2.6)
}

function fatality(a, d) {
  g.phase = 'fatality'
  g.phaseT = 0
  g.winner = a.i
  g.lastFatal = true
  a.wins++
  startMove(a, a.style === STYLES[0] ? 'roundhouse' : 'hook')
  setState(d, 'ko')
  ref(d.id).ragdoll3(true, { velocity: [a.dir * 7, 12, 0] })
  g.fatal = { d, n: 0 }
  g.slow = 2.2
  g.flash = 1
  g.flashWhite = true
  g.shake = 0.5
  ctx.playSound('boom', { volume: 1, pitch: 0.7 })
  ctx.playSound('hitHeavy', { volume: 1, pitch: 0.6 })
  showBanner('FATALITY', '', 3.4, true)
}

// ------------------------------------------------------------- effects --
/** Blood: a dark droplet spray thrown along the blow, a puff of mist, and
 * splatter on the stone below — a cluster of small smears trailing away
 * from the hit. */
function bleed(at, dmg, dir) {
  const i = g.bloodI++ % BLOOD
  const e = ref('blood' + i)
  e.position3 = { x: at.x + dir * 0.05, y: at.y, z: at.z }
  e.burst3(Math.round(20 + dmg * 7))
  const mist = ref('mist' + i)
  mist.position3 = { x: at.x, y: at.y, z: at.z }
  mist.burst3(Math.round(4 + dmg * 1.2))
  const drops = dmg >= 9 ? 5 : 3
  for (let k = 0; k < drops; k++) {
    const s = ref('bd' + (g.decal++ % DECALS))
    s.visible = true
    const along = dir * (0.15 + k * rnd(0.12, 0.3))
    s.position3 = { x: at.x + along + rnd(-0.1, 0.1), y: -0.055 + (g.decal % 9) * 0.0004, z: rnd(-0.45, 0.45) }
    const big = k === 0 ? rnd(0.18, 0.32) : rnd(0.04, 0.14)
    s.scale3 = { x: big * (1 + k * 0.35), y: 1, z: big * rnd(0.35, 0.8) }
    s.rotation3 = { x: 0, y: dir > 0 ? rnd(-20, 20) : rnd(160, 200), z: 0 }
  }
}

function spark(at) {
  const e = ref('spark' + ((g.sparkI = (g.sparkI || 0) + 1) % SPARKS))
  e.position3 = { x: at.x, y: at.y, z: at.z }
  e.burst3(22)
}

// ----------------------------------------------------------------- rig --
/** What she should look like right now (before the springs). */
function targetPose(f) {
  const st = f.style
  if (g.debugPose && g.debugPose[f.i]) return poseOf(st, g.debugPose[f.i])
  switch (f.state) {
    case 'walk': return walkPose(st, f.walkPh, 1)
    case 'crouch': return st.poses.crouch
    case 'block': return st.poses.block
    case 'bstun': return f.blocking ? st.poses.block : COMMON.hitHigh
    case 'cblock': return st.poses.cblock
    case 'air': return st.poses.jump
    case 'attack': return sampleMove(st, f.move, f.moveT)
    case 'hit': return COMMON[f.reaction] || COMMON.hitHigh
    case 'dizzy': return COMMON.dizzy
    case 'getup': return f.st < 0.25 ? st.poses.crouch : st.poses.stance
    case 'win': return COMMON.victory
    default: return st.poses.stance
  }
}

/** The springs chase the target; extra life goes on top; bones written. */
function applyRig(f, op, dt) {
  const r = ref(f.id)
  if (knocked(f)) return
  const target = targetPose(f)
  const sp = f.style.spring
  // Snappier during strikes and recoils.
  const kMul = f.state === 'attack' ? 1.6 : f.state === 'hit' ? 1.2 : 1
  const k = sp.k * kMul
  const c = 2 * Math.sqrt(k) * sp.z
  const flip = flipOf(f)
  const steps = dt > 1 / 90 ? 2 : 1
  const h = dt / steps
  // Track the opponent: the head tips toward her when she's up high.
  const look = clampf((op.y - f.y) * 12, -15, 15)
  const t = g.t
  const breathe = Math.sin(t * 3 + f.i * 2)
  for (const b of new Set([...Object.keys(f.rig), ...Object.keys(target.bones)])) {
    const s = f.rig[b] || (f.rig[b] = { r: [0, 0, 0], v: [0, 0, 0] })
    const goal = target.bones[b] || [0, 0, 0]
    for (let n = 0; n < steps; n++) {
      for (let i = 0; i < 3; i++) {
        const a = k * (goal[i] - s.r[i]) - c * s.v[i]
        s.v[i] += a * h
        s.r[i] += s.v[i] * h
      }
    }
    let x = s.r[0]
    const y = s.r[1]
    let z = s.r[2]
    if (b === 'head' && f.state !== 'attack') x -= look
    if (b === 'chest') x += breathe * 1.2
    if (f.style.bounce === 0 && b === 'spine' && (f.state === 'idle' || f.state === 'walk')) z += Math.sin(t * 1.6 + f.i) * 3
    if (f.state === 'dizzy' && (b === 'spine' || b === 'head')) z += Math.sin(t * 2.2) * 9
    r.setBone3(b, flip ? [-x, y, -z] : [x, y, z])
  }
  // The hips offset: a spring too (landings squash, hits shove).
  const hs = f.hips
  for (let n = 0; n < steps; n++) {
    for (let i = 0; i < 3; i++) {
      const a = 260 * (target.hips[i] - hs.r[i]) - 2 * Math.sqrt(260) * 0.55 * hs.v[i]
      hs.v[i] += a * h
      hs.r[i] += hs.v[i] * h
    }
  }
  let bob = 0
  if (f.state === 'idle') bob = f.style.bounce ? Math.abs(Math.sin(t * 5.2 + f.i)) * 0.028 - 0.014 : breathe * 0.008
  const hx = hs.r[0]
  const hy = hs.r[1] + bob
  const hz = hs.r[2]
  r.setBoneOffset3('hips', flip ? [-hx, hy, -hz] : [hx, hy, hz])
}

/** Places her (feet at x, y), turned to face `dir` a little toward the
 * camera, plus any spin the move adds. */
function placeFighter(f) {
  const r = ref(f.id)
  if (knocked(f)) return
  const flip = flipOf(f)
  r.position3 = { x: f.x, y: f.y, z: 0 }
  r.rotation3 = { x: 0, y: f.dir * (flip ? -110 : 70) + f.dir * f.yaw, z: 0 }
}

// --------------------------------------------------------------- frame --
g.t += rdt
if (g.req) {
  const req = g.req
  g.req = null
  if (req === 'p1') startMatch(true)
  if (req === 'p2') startMatch(false)
  if (req.startsWith('d:')) g.diff = req.slice(2)
}
if (g.hitStop > 0) g.hitStop -= rdt
const timeScale = g.hitStop > 0 ? 0.06 : g.slow > 0 ? 0.3 : 1
g.slow = Math.max(0, g.slow - rdt)
const dt = rdt * timeScale
g.phaseT = (g.phaseT || 0) + rdt

const [f1, f2] = F
const fighting = g.phase === 'fight' || g.phase === 'finish'
let c1 = null
let c2 = null
if (g.phase === 'menu') {
  for (const f of F) if (f.state !== 'idle') setState(f, 'idle')
} else if (fighting) {
  c1 = humanCmd(f1)
  c2 = g.cpu ? cpuCmd(f2, f1) : humanCmd(f2)
  if (g.phase === 'finish') {
    if (g.winner !== 0) c1 = null
    if (g.winner !== 1) c2 = null
  }
} else {
  f1.prev = {}
  f2.prev = {}
}
if (g.phase !== 'menu') {
  updateFighter(f1, f2, c1, dt)
  updateFighter(f2, f1, c2, dt)
  if (fighting) {
    checkHits(f1, f2)
    checkHits(f2, f1)
  }
  const gap = f2.x - f1.x
  if (Math.abs(gap) < PUSH && !knocked(f1) && !knocked(f2) && Math.abs(f1.y - f2.y) < 1) {
    const s = gap >= 0 ? 1 : -1
    const fix = (PUSH - Math.abs(gap)) / 2
    f1.x -= s * fix
    f2.x += s * fix
  }
}
for (const f of F) {
  if (f.fallSound > 0) {
    f.fallSound -= rdt
    if (f.fallSound <= 0) ctx.playSound('fall', { volume: 0.7 })
  }
  applyRig(f, F[1 - f.i], dt)
  placeFighter(f)
}

// Fatality: blood keeps coming while she flies.
if (g.phase === 'fatality' && g.fatal) {
  const fa = g.fatal
  fa.cool = (fa.cool || 0) - rdt
  if (fa.n < 7 && fa.cool <= 0) {
    fa.cool = 0.16
    fa.n++
    const r = ref(fa.d.id)
    const p = r.bone3(['chest', 'head', 'hips', 'leftUpperArm', 'rightUpperLeg'][fa.n % 5])
    if (p) bleed(p, 14, F[g.winner].dir)
    if (fa.n === 2) r.ragdollHit3('head', [F[g.winner].dir * 40, 70, 0])
    if (fa.n === 4) ctx.playSound('hitHeavy', { volume: 0.9, pitch: 0.75 })
  }
}

// --------------------------------------------------------------- phases --
if (g.phase === 'intro') {
  if (g.phaseT > 1.35 && !g.saidFight) {
    g.saidFight = true
    showBanner('FIGHT', '', 0.9)
    ctx.playSound('hitHeavy', { volume: 0.5, pitch: 0.7 })
  }
  if (g.phaseT > 2.0) {
    g.phase = 'fight'
    g.phaseT = 0
    g.saidFight = false
  }
} else if (g.phase === 'fight') {
  g.timer -= rdt
  if (g.timer <= 0) {
    g.timer = 0
    const loser = f1.hp <= f2.hp ? f1 : f2
    const winner = loser === f1 ? f2 : f1
    knockOut(winner, loser, [winner.dir * 1.5, 2])
    showBanner('TIME', NAMES[winner.i] + ' WINS', 2.6)
  }
} else if (g.phase === 'finish') {
  const loser = F[1 - g.winner]
  if (g.phaseT > 7 && loser.state === 'dizzy') knockOut(F[g.winner], loser, [F[g.winner].dir, 1.5])
} else if (g.phase === 'ko' || g.phase === 'fatality') {
  const wait = g.phase === 'fatality' ? 4.2 : 3.3
  const w = F[g.winner]
  if (g.phaseT > 1.4 && w.state !== 'win' && grounded(w) && w.state !== 'attack') setState(w, 'win')
  if (g.phaseT > wait) {
    if (w.wins >= 2) {
      g.phase = 'over'
      g.phaseT = 0
      showBanner(NAMES[g.winner] + ' WINS', g.lastFatal ? '' : 'THE MATCH', 3)
    } else {
      g.round++
      beginRound()
    }
  }
} else if (g.phase === 'over') {
  if (g.phaseT > 3.6) {
    g.phase = 'menu'
    resetRound()
    if (g.drums) { g.drums.stop?.(); g.drums = null }
  }
}

// --------------------------------------------------------------- camera --
{
  const xs = F.map((f) => knocked(f) && f.ragX !== undefined ? f.ragX : f.x)
  const mid = (xs[0] + xs[1]) / 2
  const sep = Math.abs(xs[0] - xs[1])
  let dist = clampf(4.4 + sep * 0.62, 4.8, 9.6)
  let y = 1.3 + sep * 0.04
  if (g.phase === 'menu') { dist = 6.2 + Math.sin(g.t * 0.3) * 0.4; y = 1.45 }
  if (g.phase === 'fatality') { dist = mixf(dist, 5.2, 0.6); y = 1.6 }
  g.cam = g.cam || { x: mid, y, z: dist }
  const k = 1 - Math.exp(-rdt * 4)
  g.cam.x = mixf(g.cam.x, clampf(mid, -ARENA + 2, ARENA - 2), k)
  g.cam.y = mixf(g.cam.y, y, k)
  g.cam.z = mixf(g.cam.z, dist, k)
  g.shake = Math.max(0, g.shake - rdt * 1.4)
  const sh = g.shake * g.shake * 2
  ref('cam').position3 = { x: g.cam.x + rnd(-sh, sh), y: g.cam.y + rnd(-sh, sh), z: g.cam.z }
  ref('cam').rotation3 = { x: -5 - sep * 0.15, y: 0, z: rnd(-sh, sh) * 8 }
}

for (let i = 0; i < BRAZIERS; i++) {
  const s = 1 + Math.sin(g.t * 13 + i * 2) * 0.08 + Math.sin(g.t * 23 + i) * 0.05
  ref('flame' + i).scale3 = { x: 1, y: 0.25 * s, z: 1 }
}

// ------------------------------------------------------------------ HUD --
g.flash = Math.max(0, g.flash - rdt * 1.6)
if (g.banner) g.banner.t -= rdt
const hud = ref('hud').dom
if (hud) {
  const $ = (id) => hud.getElementById(id)
  const inGame = g.phase !== 'menu'
  const bars = $('bars')
  const barsCls = 'top' + (inGame ? '' : ' off')
  if (bars.className !== barsCls) bars.className = barsCls
  for (const f of F) {
    const n = f.i + 1
    f.ghost = Math.max(f.hp, f.ghost - rdt * 16)
    $('h' + n).style.width = f.hp + '%'
    const lifeCls = 'life' + (f.hp < 30 ? ' low' : '')
    if ($('h' + n).className !== lifeCls) $('h' + n).className = lifeCls
    $('g' + n).style.width = Math.min(100, f.ghost) + '%'
    const wins = '<i></i>'.repeat(f.wins)
    if ($('w' + n).innerHTML !== wins) $('w' + n).innerHTML = wins
    const combo = $('c' + n)
    const show = f.combo >= 2 && f.comboT > 0
    const comboCls = 'combo ' + (n === 1 ? 'l' : 'r') + (show ? ' on' : '')
    if (combo.className !== comboCls) combo.className = comboCls
    if (show) {
      const text = f.combo + ' HITS<small>' + (f.combo >= 4 ? 'BRUTAL' : 'COMBO') + '</small>'
      if (combo.innerHTML !== text) combo.innerHTML = text
    }
    const call = $('m' + n)
    const callCls = 'callout ' + (n === 1 ? 'l' : 'r') + (f.callT > 0 ? ' on' : '')
    if (call.className !== callCls) call.className = callCls
    if (call.textContent !== f.call) call.textContent = f.call
  }
  const t = Math.ceil(g.timer)
  if ($('timer').textContent !== String(t)) $('timer').textContent = String(t)
  const timerCls = 'num' + (t <= 10 && g.phase === 'fight' ? ' low' : '')
  if ($('timer').className !== timerCls) $('timer').className = timerCls
  const roundText = g.round === 3 ? 'FINAL ROUND' : 'ROUND ' + g.round
  if ($('rnd').textContent !== roundText) $('rnd').textContent = roundText
  const b = $('banner')
  const showB = g.banner && g.banner.t > 0
  const bCls = 'banner' + (showB ? ' on' : '') + (g.banner?.red ? ' red' : '')
  if (b.className !== bCls) b.className = bCls
  if (g.banner) {
    if ($('bannerBig').textContent !== g.banner.big) $('bannerBig').textContent = g.banner.big
    if ($('bannerSmall').textContent !== g.banner.small) $('bannerSmall').textContent = g.banner.small
  }
  const fl = $('flash')
  const flashOn = g.flash > 0.3
  const flCls = 'flash' + (flashOn ? ' on' : '') + (g.flashWhite && flashOn ? ' white' : '')
  if (fl.className !== flCls) fl.className = flCls
  if (!flashOn) g.flashWhite = false
  let hint = ''
  if (g.phase === 'finish' && (g.winner === 0 || !g.cpu)) hint = '↓  →  ↓  +  KICK'
  if ($('hint').textContent !== hint) $('hint').textContent = hint
}

const menuNode = ref('menu')
const menuOn = g.phase === 'menu'
if (menuNode.visible !== menuOn) menuNode.visible = menuOn
const menu = menuNode.dom
if (menu && menuOn) {
  const $ = (id) => menu.getElementById(id)
  if (g.menuDom !== menu) {
    g.menuDom = menu
    $('p1').addEventListener('click', () => { g.req = 'p1' })
    $('p2').addEventListener('click', () => { g.req = 'p2' })
    for (const d of ['Easy', 'Normal', 'Hard']) $('d' + d).addEventListener('click', () => { g.req = 'd:' + d.toLowerCase() })
  }
  for (const d of ['Easy', 'Normal', 'Hard']) {
    const cls = 'd' + (g.diff === d.toLowerCase() ? ' on' : '')
    if ($('d' + d).className !== cls) $('d' + d).className = cls
  }
  if (keys.has('enter')) g.req = 'p1'
}
