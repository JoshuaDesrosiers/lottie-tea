//! **Rifle moves** for Sunset Tactics' VRM agents — a small library of
//! humanoid animation clips (aim idle, walk, run, back-pedal, strafes,
//! reload), *generated* rather than captured, and packed as a VRMA file
//! (`retarget::write_vrma`) so the agents play them through the ordinary
//! Humanoid Animator (crossfades, speed, retargeting onto any avatar).
//!
//! How a pose is made: a canonical humanoid in the *normalized* frame
//! (every rest rotation identity, facing +Z, left at +X, hips 1 unit high —
//! see `retarget.rs`). Each keyframe says where the hips are, how the spine
//! twists and leans, where each hand grips the rifle and where each foot
//! stands; two-bone IK places the elbows and knees (poles: elbows down and
//! out, knees forward) and the bones' rotations are read back as local
//! rotations. Retargeting copies rotations, so on a real avatar the hands
//! land close to (not exactly on) the authored points — the rifle itself
//! rides the right hand (a Bone Attachment), and Foot IK re-plants the
//! feet on the actual floor.
//!
//! Units: hips heights (≈ 0.8–0.9 m on an adult avatar).

use an1mbyt3_geometry::retarget::{write_vrma, BoneTrack, HumanoidClip, VrmaBone};
use glam::{EulerRot, Mat3, Quat, Vec3};
use std::f32::consts::{PI, TAU};

/// The canonical skeleton (normalized frame, hips height 1).
pub(crate) const SKELETON: [VrmaBone; 22] = {
    const fn b(bone: &'static str, parent: Option<usize>, x: f32, y: f32, z: f32) -> VrmaBone {
        VrmaBone { bone, parent, offset: Vec3::new(x, y, z) }
    }
    [
        b("hips", None, 0.0, 1.0, 0.0),
        b("spine", Some(0), 0.0, 0.08, 0.0),
        b("chest", Some(1), 0.0, 0.14, 0.0),
        b("upperChest", Some(2), 0.0, 0.13, 0.0),
        b("neck", Some(3), 0.0, 0.15, 0.0),
        b("head", Some(4), 0.0, 0.08, 0.0),
        b("leftShoulder", Some(3), 0.03, 0.11, 0.0),
        b("leftUpperArm", Some(6), 0.1, 0.0, 0.0),
        b("leftLowerArm", Some(7), 0.3, 0.0, 0.0),
        b("leftHand", Some(8), 0.27, 0.0, 0.0),
        b("rightShoulder", Some(3), -0.03, 0.11, 0.0),
        b("rightUpperArm", Some(10), -0.1, 0.0, 0.0),
        b("rightLowerArm", Some(11), -0.3, 0.0, 0.0),
        b("rightHand", Some(12), -0.27, 0.0, 0.0),
        b("leftUpperLeg", Some(0), 0.1, -0.06, 0.0),
        b("leftLowerLeg", Some(14), 0.0, -0.45, 0.0),
        b("leftFoot", Some(15), 0.0, -0.43, 0.0),
        b("leftToes", Some(16), 0.0, -0.05, 0.13),
        b("rightUpperLeg", Some(0), -0.1, -0.06, 0.0),
        b("rightLowerLeg", Some(18), 0.0, -0.45, 0.0),
        b("rightFoot", Some(19), 0.0, -0.43, 0.0),
        b("rightToes", Some(20), 0.0, -0.05, 0.13),
    ]
};

// Bone indices into `SKELETON`.
const HIPS: usize = 0;
const SPINE: usize = 1;
const CHEST: usize = 2;
const UPPER_CHEST: usize = 3;
const NECK: usize = 4;
const HEAD: usize = 5;
/// Per side (left, right): shoulder, upper arm, lower arm, hand.
const ARM: [[usize; 4]; 2] = [[6, 7, 8, 9], [10, 11, 12, 13]];
/// Per side (left, right): upper leg, lower leg, foot, toes.
const LEG: [[usize; 4]; 2] = [[14, 15, 16, 17], [18, 19, 20, 21]];

/// Degrees → a Y-X-Z Euler rotation (yaw, pitch, roll), the same order the
/// Inspector's bone rotations use.
fn euler(yaw: f32, pitch: f32, roll: f32) -> Quat {
    Quat::from_euler(EulerRot::YXZ, yaw.to_radians(), pitch.to_radians(), roll.to_radians())
}

/// The rotation turning a bone's rest frame (main axis `rest_axis`, with a
/// second, roughly perpendicular axis `rest_up`) so that its main axis
/// points along `axis` and its second axis toward `up`.
fn aim(rest_axis: Vec3, rest_up: Vec3, axis: Vec3, up: Vec3) -> Quat {
    let frame = |a: Vec3, u: Vec3| {
        let a = a.normalize();
        let s = a.cross(u).normalize();
        Mat3::from_cols(a, s.cross(a), s)
    };
    Quat::from_mat3(&(frame(axis, up) * frame(rest_axis, rest_up).transpose())).normalize()
}

/// Two-bone IK: where the middle joint goes for a chain rooted at `root`
/// (bone lengths `a`, `b`) reaching for `target`, bending toward `pole`.
/// Returns (middle joint, reached end).
fn two_bone(root: Vec3, a: f32, b: f32, target: Vec3, pole: Vec3) -> (Vec3, Vec3) {
    let to = target - root;
    let d = to.length().clamp((a - b).abs() + 1e-4, (a + b) * 0.999);
    let dir = to.normalize_or(Vec3::NEG_Y);
    let x = (a * a - b * b + d * d) / (2.0 * d);
    let h = (a * a - x * x).max(0.0).sqrt();
    let p = pole - root;
    let side = (p - dir * p.dot(dir)).normalize_or(Vec3::Z);
    (root + dir * x + side * h, root + dir * d)
}

/// One keyframe, as authored.
#[derive(Clone, Copy)]
struct Pose {
    /// Hips offset from rest (hips heights) and the hips' own turn.
    hips: Vec3,
    hips_turn: Quat,
    /// Local turns of spine, chest, upper chest, neck, head.
    spine: [Quat; 5],
    /// Hands: target position in the *aim frame* (origin at the upper
    /// chest, turned only by `aim_pitch`) and world rotation.
    hands: [(Vec3, Quat); 2],
    /// Pitch (degrees) of the aim frame — how much the rifle follows a
    /// forward lean.
    aim_pitch: f32,
    /// Feet: ankle target (hips-height units, world) and world rotation.
    feet: [(Vec3, Quat); 2],
    /// Toe bend (degrees, local) per foot — push-off.
    toes: [f32; 2],
}

/// Rest direction of a limb bone toward its child.
fn rest_dir(child: usize) -> Vec3 {
    SKELETON[child].offset.normalize()
}

/// The pose's local rotations (one per skeleton bone) and hips offset.
fn solve(p: &Pose) -> ([Quat; 22], Vec3) {
    let mut local = [Quat::IDENTITY; 22];
    let mut world = [Quat::IDENTITY; 22];
    let mut pos = [Vec3::ZERO; 22];
    // Forward kinematics for a bone whose local rotation is set.
    let fk = |i: usize, local: &[Quat; 22], world: &mut [Quat; 22], pos: &mut [Vec3; 22]| {
        let b = SKELETON[i];
        match b.parent {
            Some(parent) => {
                pos[i] = pos[parent] + world[parent] * b.offset;
                world[i] = world[parent] * local[i];
            }
            None => {
                pos[i] = b.offset + p.hips;
                world[i] = local[i];
            }
        }
    };
    local[HIPS] = p.hips_turn;
    fk(HIPS, &local, &mut world, &mut pos);
    for (k, &i) in [SPINE, CHEST, UPPER_CHEST, NECK, HEAD].iter().enumerate() {
        local[i] = p.spine[k];
        fk(i, &local, &mut world, &mut pos);
    }
    // Arms: the hands reach their grips in the aim frame.
    let aim_frame = Quat::from_rotation_x(p.aim_pitch.to_radians());
    for side in 0..2 {
        let [shoulder, upper, lower, hand] = ARM[side];
        fk(shoulder, &local, &mut world, &mut pos);
        // The upper arm's joint position doesn't depend on its rotation.
        fk(upper, &local, &mut world, &mut pos);
        let root = pos[upper];
        let (target, hand_rot) = p.hands[side];
        let target = pos[UPPER_CHEST] + aim_frame * target;
        let out = if side == 0 { Vec3::X } else { Vec3::NEG_X };
        let pole = root + out * 0.35 + Vec3::new(0.0, -0.6, -0.15);
        let (a, b) = (SKELETON[lower].offset.length(), SKELETON[hand].offset.length());
        let (elbow, end) = two_bone(root, a, b, target, pole);
        let g_upper = Quat::from_rotation_arc(rest_dir(lower), (elbow - root).normalize());
        let g_lower = Quat::from_rotation_arc(g_upper * rest_dir(hand), (end - elbow).normalize()) * g_upper;
        local[upper] = (world[shoulder].inverse() * g_upper).normalize();
        world[upper] = g_upper;
        local[lower] = (g_upper.inverse() * g_lower).normalize();
        world[lower] = g_lower;
        pos[lower] = elbow;
        local[hand] = (g_lower.inverse() * hand_rot).normalize();
        world[hand] = hand_rot;
        pos[hand] = end;
    }
    // Legs: knees forward over the feet.
    for side in 0..2 {
        let [upper, lower, foot, toes] = LEG[side];
        fk(upper, &local, &mut world, &mut pos);
        let root = pos[upper];
        let (target, foot_rot) = p.feet[side];
        let pole = root + Vec3::new(0.0, -0.25, 0.8);
        let (a, b) = (SKELETON[lower].offset.length(), SKELETON[foot].offset.length());
        let (knee, _) = two_bone(root, a, b, target, pole);
        let g_upper = Quat::from_rotation_arc(rest_dir(lower), (knee - root).normalize());
        let end = two_bone(root, a, b, target, pole).1;
        let g_lower = Quat::from_rotation_arc(g_upper * rest_dir(foot), (end - knee).normalize()) * g_upper;
        local[upper] = (world[HIPS].inverse() * g_upper).normalize();
        local[lower] = (g_upper.inverse() * g_lower).normalize();
        local[foot] = (g_lower.inverse() * foot_rot).normalize();
        local[toes] = Quat::from_rotation_x(p.toes[side].to_radians());
    }
    (local, p.hips)
}

/// The right hand around the pistol grip: knuckles forward (a touch down
/// and inward), palm toward the body's midline. Also the frame the rifle
/// is attached in (see [`rifle_in_hand`]).
pub(crate) fn grip_rotation() -> Quat {
    // Rest: right-hand fingers along −X, palm down (−Y).
    aim(Vec3::NEG_X, Vec3::NEG_Y, Vec3::new(0.12, -0.18, 1.0), Vec3::X)
}

/// The left hand cupping the handguard from below, fingers wrapping up the
/// far side.
fn support_rotation() -> Quat {
    // Rest: left-hand fingers along +X, palm down (−Y); palm turned up.
    aim(Vec3::X, Vec3::NEG_Y, Vec3::new(-0.55, 0.35, 0.75), Vec3::new(-0.35, 1.0, 0.0))
}

/// Where the rifle sits relative to the right hand bone, in the hand's
/// *normalized* frame (meters): (translation, rotation, uniform scale),
/// for the rifle node's local transform under a `rightHand` Bone
/// Attachment. `grip` is the pistol-grip point in the rifle model's own
/// units and `scale` the rifle's scale. The rifle model points down −Z.
pub(crate) fn rifle_in_hand(grip: Vec3, scale: f32) -> (Vec3, Quat) {
    // The rifle's world rotation when the hand grips it: pointing +Z
    // (straight ahead in the normalized frame) — i.e. turned 180°.
    let rifle_world = Quat::from_rotation_y(PI);
    let hand = grip_rotation();
    let rotation = (hand.inverse() * rifle_world).normalize();
    // The palm: a few cm along the fingers, a little toward the palm.
    let palm = Vec3::new(-0.075, -0.025, 0.0);
    (palm - rotation * (grip * scale), rotation)
}

/// The rifle hold shared by every clip: hand grips in the aim frame.
fn hold(reload: f32) -> [(Vec3, Quat); 2] {
    let grip = Vec3::new(-0.07, -0.06, 0.2);
    let guard = Vec3::new(-0.045, -0.035, 0.57);
    // Reload (0..1 through the clip): the support hand drops to the
    // magazine, pulls it, swaps it at the belt, seats it, back to the guard.
    let mag = grip + Vec3::new(0.03, -0.14, 0.13);
    let belt = Vec3::new(0.12, -0.4, 0.12);
    let keys = [(0.0, guard), (0.14, mag), (0.24, mag + Vec3::new(0.0, -0.06, 0.0)), (0.45, belt), (0.62, belt), (0.8, mag - Vec3::new(0.0, 0.04, 0.0)), (0.86, mag), (1.0, guard)];
    let mut left = guard;
    if reload > 0.0 {
        for w in keys.windows(2) {
            let ((t0, a), (t1, b)) = (w[0], w[1]);
            if reload >= t0 && reload <= t1 {
                let u = smooth((reload - t0) / (t1 - t0));
                left = a.lerp(b, u);
            }
        }
    }
    let mut right_rot = grip_rotation();
    if reload > 0.0 {
        // The rifle cants toward the support hand while it works.
        let cant = (reload * PI).sin().powf(0.5) * 28.0;
        right_rot = Quat::from_rotation_z(cant.to_radians()) * right_rot;
    }
    [(left, support_rotation()), (grip, right_rot)]
}

fn smooth(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// Torso twist toward the rifle (left shoulder forward), neck and head
/// turning back to look straight ahead; `lean` pitches it forward.
fn torso(lean: f32, breathe: f32, look_down: f32) -> [Quat; 5] {
    [euler(-10.0, lean * 0.4, 0.0), euler(-11.0, lean * 0.3 + breathe, 0.0), euler(-11.0, lean * 0.3 - breathe * 0.5, 0.0), euler(14.0, -lean * 0.4, 0.0), euler(18.0, -lean * 0.5 + 3.0 + look_down, 0.0)]
}

/// A flat foot turned out a little (left / right).
fn foot_rot(side: usize, pitch: f32) -> Quat {
    euler(if side == 0 { 9.0 } else { -9.0 }, pitch, 0.0)
}

/// The aim stance (feet staggered, knees soft).
fn stance(breathe: f32, reload: f32) -> Pose {
    Pose {
        hips: Vec3::new(0.0, -0.045 + breathe * 0.004, 0.0),
        hips_turn: euler(-6.0, 0.0, 0.0),
        spine: torso(4.0, breathe, reload * 14.0),
        hands: hold(reload),
        aim_pitch: 0.0,
        feet: [(Vec3::new(0.13, 0.06, 0.09), foot_rot(0, 0.0)), (Vec3::new(-0.12, 0.06, -0.11), foot_rot(1, 0.0))],
        toes: [0.0; 2],
    }
}

/// A locomotion cycle's shape.
struct Gait {
    /// Travel direction in the body frame (x left, z forward).
    dir: Vec3,
    /// Foot travel per step (hips heights), stance share of the cycle,
    /// foot lift, hips bob, forward lean (degrees), crouch.
    stride: f32,
    stance: f32,
    lift: f32,
    bob: f32,
    lean: f32,
    crouch: f32,
}

/// The pose at phase `phase` (0..1) of a gait cycle.
fn stride_pose(g: &Gait, phase: f32) -> Pose {
    let mut feet = [(Vec3::ZERO, Quat::IDENTITY); 2];
    let mut toes = [0.0; 2];
    // A sideways gait keeps the feet a bit wider so they never cross.
    let width = 0.11 + g.dir.x.abs() * 0.03;
    for side in 0..2 {
        let p = (phase + side as f32 * 0.5).fract();
        let (along, lift, pitch) = if p < g.stance {
            // Planted, sliding back under the body; the heel rises late.
            let u = p / g.stance;
            let heel = ((u - 0.7) / 0.3).clamp(0.0, 1.0);
            (g.stride * 0.5 * (1.0 - 2.0 * u), heel * 0.035, heel * 22.0)
        } else {
            // Swinging forward.
            let u = (p - g.stance) / (1.0 - g.stance);
            (g.stride * 0.5 * (2.0 * smooth(u) - 1.0), g.lift * (u * PI).sin(), 18.0 * (1.0 - u) - 12.0 * (u * PI).sin().powi(2) * u)
        };
        let base = Vec3::new(if side == 0 { width } else { -width }, 0.06 + lift, 0.0);
        // Back-pedalling: the foot rolls the other way.
        let pitch = if g.dir.z < -0.5 { -pitch * 0.5 } else { pitch };
        feet[side] = (base + g.dir * along, foot_rot(side, pitch));
        toes[side] = if p < g.stance { -pitch * 0.8 } else { 0.0 };
    }
    // Lowest just after each foot lands (twice a cycle).
    let bob = g.bob * (1.0 - (phase * 2.0 * TAU).cos()) * 0.5;
    let sway = 0.018 * (phase * TAU).sin();
    let lean = g.lean * g.dir.z;
    Pose {
        hips: Vec3::new(sway, -0.05 - g.crouch - bob, 0.0) + g.dir * 0.02,
        hips_turn: euler(-6.0 + 7.0 * (phase * TAU).sin() * g.dir.z, lean * 0.5, -3.0 * (phase * TAU).sin()),
        spine: torso(lean * 0.6, 0.0, 0.0),
        hands: hold(0.0),
        aim_pitch: lean * 0.6,
        feet,
        toes,
    }
}

/// Samples `pose(t)` (t in 0..1) into a clip of `duration` seconds.
fn clip(name: &str, duration: f32, keys: usize, pose: impl Fn(f32) -> Pose) -> HumanoidClip {
    let mut tracks: Vec<BoneTrack> = SKELETON.iter().map(|b| BoneTrack { bone: b.bone.into(), times: Vec::new(), rotations: Vec::new(), step: false }).collect();
    let mut hips_times = Vec::new();
    let mut hips = Vec::new();
    for k in 0..=keys {
        let u = k as f32 / keys as f32;
        let (local, offset) = solve(&pose(u));
        for (i, track) in tracks.iter_mut().enumerate() {
            track.times.push(u * duration);
            track.rotations.push(local[i]);
        }
        hips_times.push(u * duration);
        hips.push(offset);
    }
    HumanoidClip { name: name.into(), duration, bones: tracks, hips: Some((hips_times, hips)) }
}

/// Every rifle clip, by name, with the travel speed (hips heights per
/// second) each was designed for — the script scales playback by how fast
/// an agent actually moves.
pub(crate) fn rifle_clips() -> Vec<(HumanoidClip, f32)> {
    let gait = |dir: Vec3, stride: f32, stance: f32, lift: f32, bob: f32, lean: f32, crouch: f32| Gait { dir, stride, stance, lift, bob, lean, crouch };
    let cycle = |name: &str, duration: f32, g: Gait| {
        // Two steps of `stride` per cycle.
        let speed = 2.0 * g.stride / duration;
        (clip(name, duration, 24, move |u| stride_pose(&g, u)), speed)
    };
    vec![
        (clip("Idle", 3.0, 24, |u| stance((u * TAU).sin(), 0.0)), 0.0),
        (clip("Reload", 2.0, 40, |u| stance(0.0, u)), 0.0),
        cycle("Walk", 0.9, gait(Vec3::Z, 0.42, 0.6, 0.08, 0.025, 4.0, 0.06)),
        cycle("Run", 0.62, gait(Vec3::Z, 0.62, 0.42, 0.17, 0.05, 12.0, 0.03)),
        cycle("RunBack", 0.66, gait(Vec3::NEG_Z, 0.46, 0.5, 0.12, 0.035, 0.0, 0.05)),
        cycle("RunLeft", 0.64, gait(Vec3::X, 0.44, 0.5, 0.12, 0.035, 0.0, 0.05)),
        cycle("RunRight", 0.64, gait(Vec3::NEG_X, 0.44, 0.5, 0.12, 0.035, 0.0, 0.05)),
    ]
}

/// The rifle clips packed as one VRMA file (`Rifle Moves.vrma`), plus each
/// clip's name and design speed.
pub(crate) fn rifle_moves_vrma() -> (Vec<u8>, Vec<(String, f32)>) {
    let clips = rifle_clips();
    let speeds = clips.iter().map(|(c, s)| (c.name.clone(), *s)).collect();
    let clips: Vec<HumanoidClip> = clips.into_iter().map(|(c, _)| c).collect();
    (write_vrma(&SKELETON, &clips), speeds)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FK of a solved pose: world position of every bone.
    fn positions(local: &[Quat; 22], hips: Vec3) -> [Vec3; 22] {
        let mut world = [Quat::IDENTITY; 22];
        let mut pos = [Vec3::ZERO; 22];
        for (i, b) in SKELETON.iter().enumerate() {
            match b.parent {
                Some(p) => {
                    pos[i] = pos[p] + world[p] * b.offset;
                    world[i] = world[p] * local[i];
                }
                None => {
                    pos[i] = b.offset + hips;
                    world[i] = local[i];
                }
            }
        }
        pos
    }

    #[test]
    fn stance_reaches_the_rifle_and_the_ground() {
        let pose = stance(0.0, 0.0);
        let (local, hips) = solve(&pose);
        let pos = positions(&local, hips);
        // Feet on their marks.
        for side in 0..2 {
            assert!(pos[LEG[side][2]].distance(pose.feet[side].0) < 0.01, "foot {side} at {}", pos[LEG[side][2]]);
        }
        // Right hand at the grip, in front of the body and below the chin.
        let grip = pos[ARM[1][3]];
        assert!(grip.z > 0.1 && grip.y > 1.1 && grip.y < 1.45, "grip at {grip}");
        // Left hand out on the handguard, ahead of the grip.
        let guard = pos[ARM[0][3]];
        assert!(guard.z > grip.z + 0.25, "guard at {guard}");
    }

    #[test]
    fn run_cycle_loops_and_moves_the_feet() {
        let g = Gait { dir: Vec3::Z, stride: 0.62, stance: 0.42, lift: 0.17, bob: 0.05, lean: 12.0, crouch: 0.03 };
        let a = solve(&stride_pose(&g, 0.0)).0;
        let b = solve(&stride_pose(&g, 1.0)).0;
        for i in 0..22 {
            assert!(a[i].angle_between(b[i]) < 1e-3, "bone {} doesn't loop", SKELETON[i].bone);
        }
        let quarter = solve(&stride_pose(&g, 0.25)).0;
        assert!(a[LEG[0][0]].angle_between(quarter[LEG[0][0]]) > 0.1, "thigh swings");
    }

    #[test]
    fn the_vrma_reads_back() {
        let (bytes, speeds) = rifle_moves_vrma();
        let data = an1mbyt3_geometry::parse_gltf(&bytes, &|_| None).expect("parses");
        let json = an1mbyt3_geometry::vrm::gltf_json(&bytes);
        let clips = an1mbyt3_geometry::retarget::humanoid_clips(&data, json.as_ref()).expect("clips");
        let names: Vec<&str> = clips.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Idle", "Reload", "Walk", "Run", "RunBack", "RunLeft", "RunRight"]);
        assert_eq!(speeds.len(), names.len());
    }
}
