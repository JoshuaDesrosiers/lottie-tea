//! 3D physics for an1mbyt3 scenes: the `Rigidbody 3D` and `Collider 3D`
//! components (`an1mbyt3_core::physics3d`) on Node3Ds, simulated with
//! Rapier 3D. The same shape as the 2D [`crate::PhysicsWorld`]:
//!
//! - every Node3D with an enabled **Rigidbody 3D** is a Rapier body at the
//!   node's world pose; every **Collider 3D** attaches to the nearest
//!   Rigidbody 3D on its own node or a Node3D ancestor (compound bodies),
//!   or gets a fixed body of its own (static scenery);
//! - per frame: script-moved / kinematic nodes are pushed into Rapier,
//!   fixed 60 Hz substeps run (at most 5 a frame), dynamic bodies' poses are
//!   written back to their nodes (as local transforms, through their Node3D
//!   parents), and contact / trigger events are collected.
//!
//! 3D units are already meters, so nothing is rescaled.

use std::collections::HashMap;

use an1mbyt3_core::{Artboard, Collider3D, Collider3DShape, ModelId, Node3DContent, NodeBehavior, NodeId, NodeKind, Primitive3D, RigidbodyType3D};
use glam::{Mat4, Quat, Vec3};
use rapier3d::prelude::*;

use crate::ContactEvent;

const FIXED_DT: f32 = 1.0 / 60.0;
const MAX_SUBSTEPS: u32 = 5;

/// What a 3D raycast hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit3D {
    pub node: NodeId,
    pub point: Vec3,
    pub normal: Vec3,
    pub distance: f32,
}

struct Body3 {
    handle: RigidBodyHandle,
    body_type: RigidbodyType3D,
    /// The world pose we last wrote / read — a mismatch next frame means a
    /// script moved the node, so it's teleported in Rapier.
    last_pose: (Vec3, Quat),
}

/// One limb of a ragdoll (a VRM avatar's upper arm, shin, chest …): a
/// capsule from its joint `start` to `end` (world space), the bone's world
/// `rotation` when the ragdoll began, and which earlier part it hangs from
/// (by a ball joint at this part's `start`, bending at most
/// `limit_degrees` each way).
#[derive(Debug, Clone, Copy)]
pub struct RagdollPart {
    pub parent: Option<usize>,
    pub start: Vec3,
    pub end: Vec3,
    pub radius: f32,
    pub rotation: Quat,
    pub mass: f32,
    pub limit_degrees: f32,
}

/// A live ragdoll: its parts and their bodies (same order).
struct Ragdoll {
    parts: Vec<RagdollPart>,
    bodies: Vec<RigidBodyHandle>,
}

/// Ragdoll limbs don't collide with each other (their joints hold them
/// together) but do hit everything else.
const RAGDOLL_GROUP: Group = Group::GROUP_32;

pub struct Physics3DWorld {
    bodies: RigidBodySet,
    colliders: ColliderSet,
    pipeline: PhysicsPipeline,
    islands: IslandManager,
    broad_phase: BroadPhaseBvh,
    narrow_phase: NarrowPhase,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    soft_bodies: SoftBodySet,
    ccd: CCDSolver,
    params: IntegrationParameters,
    node_bodies: HashMap<NodeId, Body3>,
    collider_nodes: HashMap<ColliderHandle, (NodeId, bool)>,
    /// Physics nodes + body types the world was built from (rebuild on change).
    signature: Vec<(NodeId, u8)>,
    accumulator: f32,
    events: Vec<ContactEvent>,
    pending_forces: HashMap<NodeId, Vec3>,
    /// Ragdolls by the node (avatar) they belong to.
    ragdolls: HashMap<NodeId, Ragdoll>,
    /// A ground plane at y = 0 for ragdolls in scenes without colliders.
    ragdoll_ground: Option<ColliderHandle>,
}

impl Default for Physics3DWorld {
    fn default() -> Self {
        Self::new()
    }
}

fn rigidbody(artboard: &Artboard, id: NodeId) -> Option<an1mbyt3_core::Rigidbody3D> {
    artboard.node(id)?.behaviors.iter().find_map(|e| match &e.behavior {
        NodeBehavior::Rigidbody3D(rb) if e.enabled => Some(*rb),
        _ => None,
    })
}

fn colliders(artboard: &Artboard, id: NodeId) -> Vec<Collider3D> {
    artboard
        .node(id)
        .map(|n| {
            n.behaviors
                .iter()
                .filter_map(|e| match &e.behavior {
                    NodeBehavior::Collider3D(c) if e.enabled => Some(*c),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Visible, not deleted, and a Node3D.
fn active(artboard: &Artboard, id: NodeId) -> bool {
    if artboard.is_effectively_deleted(id) {
        return false;
    }
    let Some(node) = artboard.node(id) else { return false };
    if !matches!(node.kind, NodeKind::Node3D(_)) {
        return false;
    }
    // Hidden anywhere up the chain = out of the simulation.
    let mut current = Some(id);
    while let Some(c) = current {
        let Some(n) = artboard.node(c) else { break };
        if !n.visible {
            return false;
        }
        current = n.parent;
    }
    true
}

/// A Node3D's world matrix decomposed: (scale, rotation, position).
fn world_pose(artboard: &Artboard, id: NodeId) -> Option<(Vec3, Quat, Vec3)> {
    let m = artboard.world_matrix_3d(id)?;
    Some(m.to_scale_rotation_translation())
}

/// The nearest node at or above `id` with a Rigidbody 3D (Node3D ancestors only).
fn owning_body(artboard: &Artboard, id: NodeId) -> Option<NodeId> {
    let mut current = Some(id);
    while let Some(c) = current {
        let node = artboard.node(c)?;
        if !matches!(node.kind, NodeKind::Node3D(_)) {
            return None;
        }
        if rigidbody(artboard, c).is_some() {
            return Some(c);
        }
        current = node.parent;
    }
    None
}

/// The world matrix of `id`'s Node3D parent chain (identity at the top).
fn parent_matrix(artboard: &Artboard, id: NodeId) -> Mat4 {
    let mut parent = artboard.node(id).and_then(|n| n.parent);
    while let Some(p) = parent {
        let Some(node) = artboard.node(p) else { break };
        if matches!(node.kind, NodeKind::Node3D(_)) {
            return artboard.world_matrix_3d(p).unwrap_or(Mat4::IDENTITY);
        }
        parent = node.parent;
    }
    Mat4::IDENTITY
}

fn depth(artboard: &Artboard, id: NodeId) -> usize {
    let mut d = 0;
    let mut current = artboard.node(id).and_then(|n| n.parent);
    while let Some(c) = current {
        d += 1;
        current = artboard.node(c).and_then(|n| n.parent);
    }
    d
}

/// A collider shape at world `scale`, plus its local centre — `Auto`
/// fitted to the node's mesh (`model_bounds` measures imported models).
fn build_shape(artboard: &Artboard, id: NodeId, collider: &Collider3D, scale: Vec3, model_bounds: &dyn Fn(ModelId) -> Option<(Vec3, Vec3)>) -> Option<(ColliderBuilder, Vec3, f32)> {
    let s = scale.abs().max(Vec3::splat(1e-3));
    let cuboid = |size: Vec3| {
        let h = (size * s * 0.5).max(Vec3::splat(0.005));
        (ColliderBuilder::cuboid(h.x, h.y, h.z), 8.0 * h.x * h.y * h.z)
    };
    let (builder, centre, volume) = match collider.shape {
        Collider3DShape::Box { size } => {
            let (b, v) = cuboid(size);
            (b, collider.offset, v)
        }
        Collider3DShape::Sphere { radius } => {
            let r = (radius * s.max_element()).max(0.005);
            (ColliderBuilder::ball(r), collider.offset, 4.0 / 3.0 * std::f32::consts::PI * r * r * r)
        }
        Collider3DShape::Capsule { radius, height } => {
            let r = (radius * s.x.max(s.z)).max(0.005);
            let half = ((height * s.y) * 0.5 - r).max(0.0);
            (ColliderBuilder::capsule_y(half, r), collider.offset, std::f32::consts::PI * r * r * (2.0 * half + 4.0 / 3.0 * r))
        }
        Collider3DShape::Auto => {
            let NodeKind::Node3D(n) = &artboard.node(id)?.kind else { return None };
            let Node3DContent::Mesh(mesh) = n.content else {
                // Not a mesh (an empty / camera / light): a small box.
                let (b, v) = cuboid(Vec3::splat(0.5));
                return Some((b, collider.offset, v));
            };
            match mesh.primitive {
                Primitive3D::Cube { size } => {
                    let (b, v) = cuboid(size);
                    (b, collider.offset, v)
                }
                Primitive3D::Sphere { radius, .. } => {
                    let r = (radius * s.max_element()).max(0.005);
                    (ColliderBuilder::ball(r), collider.offset, 4.0 / 3.0 * std::f32::consts::PI * r * r * r)
                }
                // A plane: a slab whose top face is the plane (so things rest on it).
                Primitive3D::Plane { size, .. } => {
                    let (b, v) = cuboid(Vec3::new(size, 0.1 / s.y, size));
                    (b, collider.offset - Vec3::Y * (0.05 / s.y), v)
                }
                Primitive3D::Cylinder { radius, height, .. } => {
                    let r = (radius * s.x.max(s.z)).max(0.005);
                    let h = (height * s.y * 0.5).max(0.005);
                    (ColliderBuilder::cylinder(h, r), collider.offset, std::f32::consts::PI * r * r * 2.0 * h)
                }
                Primitive3D::Cone { radius, height, .. } => {
                    let r = (radius * s.x.max(s.z)).max(0.005);
                    let h = (height * s.y * 0.5).max(0.005);
                    (ColliderBuilder::cone(h, r), collider.offset, std::f32::consts::PI * r * r * 2.0 * h / 3.0)
                }
                Primitive3D::Torus { radius, tube, .. } => {
                    let (b, v) = cuboid(Vec3::new(2.0 * (radius + tube), 2.0 * tube, 2.0 * (radius + tube)));
                    (b, collider.offset, v)
                }
                // A block world's collision is the game script's job (its
                // blocks aren't in the document); this box is only its extent.
                Primitive3D::Voxels { width, height, depth } => {
                    let size = Vec3::new(width as f32, height as f32, depth as f32);
                    let (b, v) = cuboid(size);
                    (b, collider.offset + size * 0.5, v)
                }
                // A glass pane's collider is the whole pane (holes are the
                // game script's business — `ctx.glassRaycast3`).
                Primitive3D::Glass { width, height, thickness, .. } => {
                    let (b, v) = cuboid(Vec3::new(width, height, thickness.max(0.01)));
                    (b, collider.offset, v)
                }
                Primitive3D::Model { model } => {
                    let (min, max) = model_bounds(model).unwrap_or((Vec3::splat(-0.5), Vec3::splat(0.5)));
                    let (b, v) = cuboid(max - min);
                    (b, collider.offset + (min + max) * 0.5, v)
                }
            }
        }
    };
    Some((builder, centre * s, volume))
}

impl Physics3DWorld {
    pub fn new() -> Self {
        let mut params = IntegrationParameters::default();
        params.dt = FIXED_DT;
        Self {
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            pipeline: PhysicsPipeline::new(),
            islands: IslandManager::new(),
            broad_phase: BroadPhaseBvh::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            soft_bodies: SoftBodySet::new(),
            ccd: CCDSolver::new(),
            params,
            node_bodies: HashMap::new(),
            collider_nodes: HashMap::new(),
            signature: Vec::new(),
            accumulator: 0.0,
            events: Vec::new(),
            pending_forces: HashMap::new(),
            ragdolls: HashMap::new(),
            ragdoll_ground: None,
        }
    }

    /// Turns `owner`'s avatar into a ragdoll: one dynamic capsule body per
    /// part, ball-jointed to its parent part, all starting with `velocity`
    /// (m/s). Replaces any ragdoll `owner` already had. Scenes with no
    /// collider of their own get a floor at y = 0 so it has something to
    /// land on.
    pub fn add_ragdoll(&mut self, owner: NodeId, parts: &[RagdollPart], velocity: Vec3) {
        self.remove_ragdoll(owner);
        let poses: Vec<(Pose, Vec3, Vec3)> = parts.iter().map(|p| (Pose::from_parts(p.start, p.rotation), velocity, Vec3::ZERO)).collect();
        self.insert_ragdoll(owner, parts.to_vec(), &poses);
    }

    fn insert_ragdoll(&mut self, owner: NodeId, parts: Vec<RagdollPart>, poses: &[(Pose, Vec3, Vec3)]) {
        let has_static = self.colliders.iter().any(|(_, c)| c.parent().is_none_or(|b| self.bodies.get(b).is_some_and(|b| b.is_fixed())));
        if !has_static && self.ragdoll_ground.is_none() {
            // A wide slab whose top face is y = 0.
            let ground = ColliderBuilder::cuboid(500.0, 0.5, 500.0).translation(Vec3::new(0.0, -0.5, 0.0)).friction(0.9).build();
            self.ragdoll_ground = Some(self.colliders.insert(ground));
        }
        let groups = InteractionGroups::new(RAGDOLL_GROUP, Group::ALL ^ RAGDOLL_GROUP, InteractionTestMode::And);
        let mut bodies = Vec::with_capacity(parts.len());
        for (i, part) in parts.iter().enumerate() {
            let (pose, lin, ang) = poses.get(i).copied().unwrap_or((Pose::from_parts(part.start, part.rotation), Vec3::ZERO, Vec3::ZERO));
            let body = RigidBodyBuilder::dynamic().pose(pose).linvel(lin).angvel(ang).linear_damping(0.1).angular_damping(1.2).can_sleep(false).build();
            let handle = self.bodies.insert(body);
            // The capsule in the body's own frame (origin at the joint).
            let tip = part.rotation.inverse() * (part.end - part.start);
            let r = part.radius.max(0.01);
            let length = tip.length();
            let volume = std::f32::consts::PI * r * r * length + 4.0 / 3.0 * std::f32::consts::PI * r * r * r;
            let collider = ColliderBuilder::capsule_from_endpoints(Vec3::ZERO, tip, r)
                .density(part.mass.max(0.05) / volume.max(1e-5))
                .friction(0.8)
                .collision_groups(groups)
                .build();
            self.colliders.insert_with_parent(collider, handle, &mut self.bodies);
            bodies.push(handle);
        }
        for (i, part) in parts.iter().enumerate() {
            let Some(p) = part.parent.filter(|&p| p < i) else { continue };
            let parent = &parts[p];
            // The joint frames line up at the starting pose, so limits
            // measure bending away from it.
            let anchor = parent.rotation.inverse() * (part.start - parent.start);
            let relative = parent.rotation.inverse() * part.rotation;
            let l = part.limit_degrees.to_radians();
            let joint = SphericalJointBuilder::new()
                .local_frame1(Pose::from_parts(anchor, relative))
                .local_frame2(Pose::IDENTITY)
                .limits(JointAxis::AngX, [-l, l])
                .limits(JointAxis::AngY, [-l * 0.6, l * 0.6])
                .limits(JointAxis::AngZ, [-l, l])
                .contacts_enabled(false);
            self.impulse_joints.insert(bodies[p], bodies[i], joint, true);
        }
        self.ragdolls.insert(owner, Ragdoll { parts, bodies });
    }

    /// Ends `owner`'s ragdoll (the avatar goes back to being animated).
    pub fn remove_ragdoll(&mut self, owner: NodeId) {
        let Some(ragdoll) = self.ragdolls.remove(&owner) else { return };
        for handle in ragdoll.bodies {
            self.bodies.remove(handle, &mut self.islands, &mut self.colliders, &mut self.impulse_joints, &mut self.multibody_joints, &mut self.soft_bodies, true);
        }
    }

    pub fn has_ragdoll(&self, owner: NodeId) -> bool {
        self.ragdolls.contains_key(&owner)
    }

    /// Each part's world pose (joint position, rotation), same order as
    /// the parts it was made from.
    pub fn ragdoll_poses(&self, owner: NodeId) -> Option<Vec<(Vec3, Quat)>> {
        let ragdoll = self.ragdolls.get(&owner)?;
        Some(ragdoll.bodies.iter().map(|h| self.bodies.get(*h).map_or((Vec3::ZERO, Quat::IDENTITY), |b| (b.translation(), *b.rotation()))).collect())
    }

    /// A hit on one part of a ragdoll: an impulse (N·s) at world `point`.
    pub fn ragdoll_impulse(&mut self, owner: NodeId, part: usize, impulse: Vec3, point: Option<Vec3>) {
        let Some(handle) = self.ragdolls.get(&owner).and_then(|r| r.bodies.get(part).copied()) else { return };
        if let Some(body) = self.bodies.get_mut(handle) {
            match point {
                Some(p) => body.apply_impulse_at_point(impulse, p, true),
                None => body.apply_impulse(impulse, true),
            }
        }
    }

    /// Does the scene use 3D physics at all? (Cheap: skip everything if not.)
    pub fn scene_has_physics(artboard: &Artboard) -> bool {
        artboard.node_ids().any(|id| {
            artboard
                .node(id)
                .is_some_and(|n| n.behaviors.iter().any(|e| e.enabled && matches!(e.behavior, NodeBehavior::Rigidbody3D(_) | NodeBehavior::Collider3D(_))))
        })
    }

    fn physics_nodes(artboard: &Artboard) -> Vec<NodeId> {
        let mut nodes: Vec<NodeId> = artboard
            .node_ids()
            .filter(|&id| active(artboard, id))
            .filter(|&id| rigidbody(artboard, id).is_some() || !colliders(artboard, id).is_empty())
            .collect();
        nodes.sort();
        nodes
    }

    /// (Re)builds the Rapier world when the set of physics nodes (or a body
    /// type) changed. Surviving bodies keep their velocities.
    pub fn sync(&mut self, artboard: &Artboard, model_bounds: &dyn Fn(ModelId) -> Option<(Vec3, Vec3)>) {
        let nodes = Self::physics_nodes(artboard);
        let signature: Vec<(NodeId, u8)> = nodes.iter().map(|&id| (id, rigidbody(artboard, id).map_or(0, |rb| 1 + rb.body_type as u8))).collect();
        if signature == self.signature {
            return;
        }
        let kept: HashMap<NodeId, (Vector, AngVector)> = self.node_bodies.iter().filter_map(|(id, b)| self.bodies.get(b.handle).map(|body| (*id, (body.linvel(), body.angvel())))).collect();
        let forces = std::mem::take(&mut self.pending_forces);
        // Ragdolls survive a rebuild where they are, moving as they were.
        let ragdolls: Vec<(NodeId, Vec<RagdollPart>, Vec<(Pose, Vec3, Vec3)>)> = self
            .ragdolls
            .drain()
            .map(|(id, r)| {
                let poses = r.bodies.iter().map(|h| self.bodies.get(*h).map_or((Pose::IDENTITY, Vec3::ZERO, Vec3::ZERO), |b| (*b.position(), b.linvel(), b.angvel()))).collect();
                (id, r.parts, poses)
            })
            .collect();
        *self = Self::new();
        self.pending_forces = forces;
        let restore_ragdolls = ragdolls;
        self.signature = signature;

        for &id in &nodes {
            let Some(rb) = rigidbody(artboard, id) else { continue };
            let Some((_, rotation, position)) = world_pose(artboard, id) else { continue };
            let builder = match rb.body_type {
                RigidbodyType3D::Dynamic => RigidBodyBuilder::dynamic(),
                RigidbodyType3D::Kinematic => RigidBodyBuilder::kinematic_position_based(),
                RigidbodyType3D::Static => RigidBodyBuilder::fixed(),
            };
            let mut builder = builder
                .pose(Pose::from_parts(position, rotation))
                .linear_damping(rb.linear_damping.max(0.0))
                .angular_damping(rb.angular_damping.max(0.0))
                .gravity_scale(rb.gravity_scale)
                .ccd_enabled(rb.continuous);
            if rb.freeze_rotation {
                builder = builder.lock_rotations();
            }
            let handle = self.bodies.insert(builder.build());
            if let Some((linvel, angvel)) = kept.get(&id) {
                self.bodies[handle].set_linvel(*linvel, false);
                self.bodies[handle].set_angvel(*angvel, false);
            }
            self.node_bodies.insert(id, Body3 { handle, body_type: rb.body_type, last_pose: (position, rotation) });
        }

        for &id in &nodes {
            let shapes = colliders(artboard, id);
            if shapes.is_empty() {
                continue;
            }
            let owner = owning_body(artboard, id).filter(|o| self.node_bodies.contains_key(o));
            let body_node = match owner {
                Some(o) => o,
                None => {
                    let Some((_, rotation, position)) = world_pose(artboard, id) else { continue };
                    let handle = self.bodies.insert(RigidBodyBuilder::fixed().pose(Pose::from_parts(position, rotation)).build());
                    self.node_bodies.insert(id, Body3 { handle, body_type: RigidbodyType3D::Static, last_pose: (position, rotation) });
                    id
                }
            };
            let body = &self.node_bodies[&body_node];
            let (body_handle, (body_pos, body_rot)) = (body.handle, body.last_pose);
            let Some((scale, rotation, position)) = world_pose(artboard, id) else { continue };
            let mass = owner.and_then(|o| rigidbody(artboard, o)).map_or(1.0, |rb| rb.mass.max(1e-4));
            let built: Vec<(ColliderBuilder, Vec3, f32, Collider3D)> = shapes.iter().filter_map(|c| build_shape(artboard, id, c, scale, model_bounds).map(|(b, centre, v)| (b, centre, v, *c))).collect();
            let total: f32 = built.iter().map(|(_, _, v, _)| *v).sum::<f32>().max(1e-6);
            for (builder, centre, _volume, c) in built {
                // The shape's centre and rotation relative to the body.
                let world_centre = position + rotation * centre;
                let local = body_rot.inverse() * (world_centre - body_pos);
                let local_rot = body_rot.inverse() * rotation;
                let collider = builder
                    .position(Pose::from_parts(local, local_rot))
                    .sensor(c.is_trigger)
                    .friction(c.friction.max(0.0))
                    .restitution(c.restitution.clamp(0.0, 1.0))
                    // Mass spread by volume: density = mass / total volume.
                    .density(mass / total)
                    .active_events(ActiveEvents::COLLISION_EVENTS)
                    .active_collision_types(ActiveCollisionTypes::all())
                    .build();
                let handle = self.colliders.insert_with_parent(collider, body_handle, &mut self.bodies);
                self.collider_nodes.insert(handle, (id, c.is_trigger));
            }
        }

        for (id, parts, poses) in restore_ragdolls {
            self.insert_ragdoll(id, parts, &poses);
        }

        // Prime the broad phase so raycasts work on the first frame.
        let mut prime = self.params;
        prime.dt = 1e-6;
        self.pipeline.step(
            Vector::ZERO,
            &prime,
            &mut self.islands,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.soft_bodies,
            &mut self.ccd,
            &(),
            &(),
        );
    }

    /// Advances by `delta_seconds` and writes dynamic poses back. Call
    /// [`Physics3DWorld::sync`] first.
    pub fn step(&mut self, artboard: &mut Artboard, delta_seconds: f32) {
        // 1. Script-moved nodes / kinematic bodies -> Rapier.
        for (id, info) in self.node_bodies.iter_mut() {
            let Some((_, rotation, position)) = world_pose(artboard, *id) else { continue };
            let Some(body) = self.bodies.get_mut(info.handle) else { continue };
            match info.body_type {
                RigidbodyType3D::Kinematic => {
                    body.set_next_kinematic_translation(position);
                    body.set_next_kinematic_rotation(rotation);
                }
                _ => {
                    let moved = position.distance(info.last_pose.0) > 1e-4 || rotation.angle_between(info.last_pose.1) > 1e-4;
                    if moved {
                        body.set_position(Pose::from_parts(position, rotation), true);
                    }
                }
            }
            info.last_pose = (position, rotation);
        }

        // 2. Fixed substeps.
        for (id, force) in &self.pending_forces {
            if let Some(body) = self.node_bodies.get(id).and_then(|b| self.bodies.get_mut(b.handle)) {
                body.add_force(*force, true);
            }
        }
        self.accumulator = (self.accumulator + delta_seconds).min(FIXED_DT * MAX_SUBSTEPS as f32);
        let gravity = artboard.gravity_3d;
        let (collision_tx, collision_rx) = std::sync::mpsc::channel();
        let (force_tx, _force_rx) = std::sync::mpsc::channel();
        let (tear_tx, _tear_rx) = std::sync::mpsc::channel();
        let collector = ChannelEventCollector::new(collision_tx, force_tx, tear_tx);
        while self.accumulator >= FIXED_DT {
            self.accumulator -= FIXED_DT;
            self.pipeline.step(
                gravity,
                &self.params,
                &mut self.islands,
                &mut self.broad_phase,
                &mut self.narrow_phase,
                &mut self.bodies,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                &mut self.soft_bodies,
                &mut self.ccd,
                &(),
                &collector,
            );
        }
        for (_, body) in self.bodies.iter_mut() {
            body.reset_forces(false);
        }
        self.pending_forces.clear();

        // 3. Dynamic bodies -> nodes (parents first, so children compose
        //    against their parent's new pose).
        let mut order: Vec<NodeId> = self.node_bodies.iter().filter(|(_, b)| b.body_type == RigidbodyType3D::Dynamic).map(|(id, _)| *id).collect();
        order.sort_by_key(|id| depth(artboard, *id));
        for id in order {
            let Some(info) = self.node_bodies.get_mut(&id) else { continue };
            let Some(body) = self.bodies.get(info.handle) else { continue };
            let position = body.translation();
            let rotation = *body.rotation();
            let parent = parent_matrix(artboard, id);
            let (_, parent_rot, _) = parent.to_scale_rotation_translation();
            if let Some(node) = artboard.node_mut(id) {
                if let NodeKind::Node3D(n) = &mut node.kind {
                    n.transform.position = parent.inverse().transform_point3(position);
                    n.transform.set_rotation(parent_rot.inverse() * rotation);
                }
            }
            info.last_pose = (position, rotation);
        }

        // 4. Events.
        for event in collision_rx.try_iter() {
            let (a, b, started) = match event {
                CollisionEvent::Started(a, b, _) => (a, b, true),
                CollisionEvent::Stopped(a, b, _) => (a, b, false),
            };
            let (Some(&(node_a, trigger_a)), Some(&(node_b, trigger_b))) = (self.collider_nodes.get(&a), self.collider_nodes.get(&b)) else { continue };
            self.events.push(ContactEvent { a: node_a, b: node_b, started, trigger: trigger_a || trigger_b });
        }
        const MAX_PENDING_EVENTS: usize = 1024;
        if self.events.len() > MAX_PENDING_EVENTS {
            let excess = self.events.len() - MAX_PENDING_EVENTS;
            self.events.drain(..excess);
        }
    }

    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// Contact / trigger events since the last call.
    pub fn take_events(&mut self) -> Vec<ContactEvent> {
        std::mem::take(&mut self.events)
    }

    fn body_mut(&mut self, node: NodeId) -> Option<&mut RigidBody> {
        let handle = self.node_bodies.get(&node)?.handle;
        self.bodies.get_mut(handle)
    }

    /// Linear velocity, m/s.
    pub fn velocity(&self, node: NodeId) -> Option<Vec3> {
        self.node_bodies.get(&node).and_then(|b| self.bodies.get(b.handle)).map(|b| b.linvel())
    }

    pub fn set_velocity(&mut self, node: NodeId, velocity: Vec3) {
        if let Some(body) = self.body_mut(node) {
            body.set_linvel(velocity, true);
        }
    }

    /// Angular velocity, degrees per second about each world axis.
    pub fn angular_velocity(&self, node: NodeId) -> Option<Vec3> {
        self.node_bodies.get(&node).and_then(|b| self.bodies.get(b.handle)).map(|b| b.angvel() * (180.0 / std::f32::consts::PI))
    }

    pub fn set_angular_velocity(&mut self, node: NodeId, degrees_per_second: Vec3) {
        if let Some(body) = self.body_mut(node) {
            body.set_angvel(degrees_per_second * (std::f32::consts::PI / 180.0), true);
        }
    }

    /// Unity's `AddForce` (ForceMode.Force), newtons, applied over the next frame.
    pub fn add_force(&mut self, node: NodeId, force: Vec3) {
        *self.pending_forces.entry(node).or_insert(Vec3::ZERO) += force;
    }

    /// An instant kick (ForceMode.Impulse): velocity change = impulse / mass.
    pub fn add_impulse(&mut self, node: NodeId, impulse: Vec3) {
        if let Some(body) = self.body_mut(node) {
            body.apply_impulse(impulse, true);
        }
    }

    /// An instant spin (`AddTorque`, ForceMode.Impulse).
    pub fn add_torque_impulse(&mut self, node: NodeId, impulse: Vec3) {
        if let Some(body) = self.body_mut(node) {
            body.apply_torque_impulse(impulse, true);
        }
    }

    /// Unity's `Physics.Raycast`: the first non-trigger collider hit along
    /// `direction` from `origin` within `max_distance` meters, optionally
    /// ignoring `exclude`'s own body.
    pub fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32, exclude: Option<NodeId>) -> Option<RayHit3D> {
        let dir = direction.normalize_or_zero();
        if dir == Vec3::ZERO {
            return None;
        }
        let mut filter = QueryFilter::default().exclude_sensors();
        if let Some(handle) = exclude.and_then(|n| self.node_bodies.get(&n)).map(|b| b.handle) {
            filter = filter.exclude_rigid_body(handle);
        }
        let query = self.broad_phase.as_query_pipeline(self.narrow_phase.query_dispatcher(), &self.bodies, &self.colliders, filter);
        let ray = Ray::new(origin, dir);
        let (handle, hit) = query.cast_ray_and_get_normal(&ray, max_distance, true)?;
        let (node, _) = *self.collider_nodes.get(&handle)?;
        Some(RayHit3D { node, point: origin + dir * hit.time_of_impact, normal: hit.normal, distance: hit.time_of_impact })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use an1mbyt3_core::{Mesh3D, Node, Node3D, NodeBehaviorEntry, Rigidbody3D, Transform3D};

    fn node3d(name: &str, at: Vec3, primitive: Primitive3D, behaviors: Vec<NodeBehavior>) -> Node {
        let mut node = Node::new(name, None, NodeKind::Node3D(Node3D::new(Transform3D::at(at), Node3DContent::Mesh(Mesh3D::new(primitive, [1.0; 4])))));
        for behavior in behaviors {
            node.behaviors.push(NodeBehaviorEntry { behavior, enabled: true });
        }
        node
    }

    #[test]
    fn a_cube_falls_and_rests_on_a_plane() {
        let mut artboard = Artboard::new("3D", 100.0, 100.0);
        artboard.add_node(node3d("Floor", Vec3::ZERO, Primitive3D::plane(), vec![NodeBehavior::Collider3D(Collider3D::default())]));
        let cube = artboard.add_node(node3d("Cube", Vec3::new(0.0, 3.0, 0.0), Primitive3D::cube(), vec![NodeBehavior::Rigidbody3D(Rigidbody3D::default()), NodeBehavior::Collider3D(Collider3D::default())]));
        let mut world = Physics3DWorld::new();
        for _ in 0..240 {
            world.sync(&artboard, &|_| None);
            world.step(&mut artboard, 1.0 / 60.0);
        }
        let NodeKind::Node3D(n) = &artboard.node(cube).unwrap().kind else { unreachable!() };
        // A 1 m cube resting on the plane: its centre half a metre up.
        assert!((n.transform.position.y - 0.5).abs() < 0.05, "y = {}", n.transform.position.y);
        let hit = world.raycast(Vec3::new(0.0, 10.0, 0.0), Vec3::NEG_Y, 20.0, None).expect("ray hits the cube");
        assert_eq!(hit.node, cube);
        assert!((hit.point.y - 1.0).abs() < 0.05, "{hit:?}");
    }

    #[test]
    fn an_impulse_launches_a_ball() {
        let mut artboard = Artboard::new("3D", 100.0, 100.0);
        artboard.gravity_3d = Vec3::ZERO;
        let ball = artboard.add_node(node3d("Ball", Vec3::ZERO, Primitive3D::sphere(), vec![NodeBehavior::Rigidbody3D(Rigidbody3D { mass: 2.0, ..Default::default() }), NodeBehavior::Collider3D(Collider3D::default())]));
        let mut world = Physics3DWorld::new();
        world.sync(&artboard, &|_| None);
        world.add_impulse(ball, Vec3::new(4.0, 0.0, 0.0));
        world.step(&mut artboard, 1.0 / 60.0);
        let v = world.velocity(ball).unwrap();
        assert!((v.x - 2.0).abs() < 0.05, "velocity = impulse / mass: {v}");
    }

    #[test]
    fn a_ragdoll_falls_lands_and_stays_jointed() {
        let mut artboard = an1mbyt3_core::Artboard::new("t", 100.0, 100.0);
        let mut world = Physics3DWorld::new();
        world.sync(&artboard, &|_| None);
        // Torso, with a thigh and a shin hanging below it, held up at 2 m.
        let up = Quat::from_rotation_arc(Vec3::Y, Vec3::NEG_Y);
        let parts = [
            RagdollPart { parent: None, start: Vec3::new(0.0, 2.0, 0.0), end: Vec3::new(0.0, 2.5, 0.0), radius: 0.12, rotation: Quat::IDENTITY, mass: 10.0, limit_degrees: 30.0 },
            RagdollPart { parent: Some(0), start: Vec3::new(0.1, 2.0, 0.0), end: Vec3::new(0.1, 1.55, 0.0), radius: 0.07, rotation: up, mass: 4.0, limit_degrees: 70.0 },
            RagdollPart { parent: Some(1), start: Vec3::new(0.1, 1.55, 0.0), end: Vec3::new(0.1, 1.1, 0.0), radius: 0.06, rotation: up, mass: 3.0, limit_degrees: 70.0 },
        ];
        world.add_ragdoll(NodeId::default(), &parts, Vec3::new(1.0, 0.0, 0.0));
        for _ in 0..240 {
            world.step(&mut artboard, 1.0 / 60.0);
        }
        let poses = world.ragdoll_poses(NodeId::default()).expect("ragdoll");
        for (p, _) in &poses {
            assert!(p.y > -0.05 && p.y < 0.6, "landed on the floor: {p}");
        }
        // The shin still hangs off the thigh's far end.
        let thigh_end = poses[1].0 + poses[1].1 * (up.inverse() * (parts[1].end - parts[1].start));
        assert!(thigh_end.distance(poses[2].0) < 0.05, "joint held: {}", thigh_end.distance(poses[2].0));
        world.remove_ragdoll(NodeId::default());
        assert!(!world.has_ragdoll(NodeId::default()));
    }
}