//! Unity-style 2D physics for an1mbyt3 scenes: the `Rigidbody 2D` and
//! `Box/Circle/Capsule/Polygon Collider 2D` components
//! (`an1mbyt3_core::physics2d`), simulated with [Rapier](https://rapier.rs).
//!
//! # How a scene maps onto Rapier
//!
//! - Every node with an enabled **Rigidbody 2D** becomes a Rapier body
//!   (Dynamic / Kinematic-position / Fixed), placed at the node's world
//!   pivot (its `translation`, in world space) and world rotation.
//! - Every enabled **collider** attaches to the nearest Rigidbody 2D on its
//!   own node or any ancestor (Unity's compound colliders); a collider with
//!   no Rigidbody above it gets a Fixed body of its own (static scenery).
//! - Collider shapes live in the node's own local (art) space, scaled by the
//!   node's world scale.
//!
//! # Per frame ([`PhysicsWorld::step`])
//!
//! 1. Nodes a script moved since our last write (or kinematic bodies, every
//!    step) are pushed *into* Rapier — so `ref.x = ...` teleports a body.
//! 2. Fixed 60 Hz substeps (Unity's FixedUpdate), at most 5 per frame.
//! 3. Dynamic bodies' poses are written back *out* to their nodes (as the
//!    node's local transform, through its parent).
//! 4. Contact/trigger enter/exit events are collected for scripts.
//!
//! Rapier works best near meter scale, so world units (art pixels) are
//! divided by [`PIXELS_PER_METER`] on the way in and multiplied back out.

pub mod liquid;
pub mod physics3d;

use std::collections::{HashMap, HashSet};

use an1mbyt3_core::ColliderRef;
use an1mbyt3_core::{Artboard, CapsuleDirection2D, CollisionDetection2D, NodeId, PhysicsMaterial2D, RigidbodyType2D};
use glam::{Affine2, Vec2};
use rapier2d::prelude::*;

/// Art pixels per Rapier meter.
pub const PIXELS_PER_METER: f32 = 32.0;
/// Unity's default Fixed Timestep.
const FIXED_DT: f32 = 1.0 / 60.0;
const MAX_SUBSTEPS: u32 = 5;

/// A contact (solid) or trigger (sensor) starting or ending between two
/// nodes' colliders.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactEvent {
    pub a: NodeId,
    pub b: NodeId,
    /// `true` = began touching/overlapping this frame; `false` = stopped.
    pub started: bool,
    /// Either collider is a trigger (`is_trigger`).
    pub trigger: bool,
}

/// What [`PhysicsWorld::raycast`] hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    pub node: NodeId,
    pub point: Vec2,
    pub normal: Vec2,
    pub distance: f32,
}

fn to_rapier(v: Vec2) -> Vector {
    Vector::new(v.x / PIXELS_PER_METER, v.y / PIXELS_PER_METER)
}

fn from_rapier(v: Vector) -> Vec2 {
    Vec2::new(v.x * PIXELS_PER_METER, v.y * PIXELS_PER_METER)
}

/// The parent's world affine (identity for a root node).
fn parent_affine(artboard: &Artboard, node: NodeId) -> Affine2 {
    artboard
        .node(node)
        .and_then(|n| n.parent)
        .and_then(|p| artboard.try_world_transform(p))
        .map(|t| t.to_affine2())
        .unwrap_or(Affine2::IDENTITY)
}

/// A node's world pivot (its own `translation`, carried into world space)
/// and world rotation (radians).
fn world_pose(artboard: &Artboard, node: NodeId) -> Option<(Vec2, f32)> {
    let n = artboard.node(node)?;
    let pivot = parent_affine(artboard, node).transform_point2(n.transform.translation);
    let rotation = artboard.try_world_transform(node)?.rotation;
    Some((pivot, rotation))
}

fn rotate(v: Vec2, angle: f32) -> Vec2 {
    let (s, c) = angle.sin_cos();
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// The nearest node at or above `node` with an enabled Rigidbody 2D.
fn owning_body_node(artboard: &Artboard, node: NodeId) -> Option<NodeId> {
    let mut cursor = Some(node);
    for _ in 0..64 {
        let id = cursor?;
        let n = artboard.node(id)?;
        if n.rigidbody().is_some() {
            return Some(id);
        }
        cursor = n.parent;
    }
    None
}

/// The topmost node at or above `node` that has a Rigidbody 2D — the root
/// of its physics hierarchy (a ragdoll's hips, a chain's first link).
fn topmost_body(artboard: &Artboard, node: NodeId) -> NodeId {
    let mut top = node;
    let mut cursor = artboard.node(node).and_then(|n| n.parent);
    for _ in 0..64 {
        let Some(id) = cursor else { break };
        let Some(n) = artboard.node(id) else { break };
        if n.rigidbody().is_some() {
            top = id;
        }
        cursor = n.parent;
    }
    top
}

/// How many ancestors `node` has.
fn depth(artboard: &Artboard, node: NodeId) -> usize {
    let mut d = 0;
    let mut cursor = artboard.node(node).and_then(|n| n.parent);
    while let Some(id) = cursor {
        d += 1;
        if d > 64 {
            break;
        }
        cursor = artboard.node(id).and_then(|n| n.parent);
    }
    d
}

/// Visible and not deleted, itself and every ancestor.
fn active(artboard: &Artboard, node: NodeId) -> bool {
    if artboard.is_effectively_deleted(node) {
        return false;
    }
    let mut cursor = Some(node);
    for _ in 0..64 {
        let Some(id) = cursor else { return true };
        let Some(n) = artboard.node(id) else { return false };
        if !n.visible {
            return false;
        }
        cursor = n.parent;
    }
    true
}

struct BodyInfo {
    handle: RigidBodyHandle,
    body_type: RigidbodyType2D,
    /// The node pose we last wrote out (or read in) — a mismatch next frame
    /// means a script moved the node, so it's teleported in Rapier.
    last_pose: (Vec2, f32),
}

/// A live simulation of one artboard's physics components.
pub struct PhysicsWorld {
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
    /// Body per node that owns one (Rigidbody 2D nodes, plus a Fixed body
    /// for each static collider node).
    node_bodies: HashMap<NodeId, BodyInfo>,
    /// Which node each collider was authored on, and whether it's a trigger.
    collider_nodes: HashMap<ColliderHandle, (NodeId, bool)>,
    /// The signature the world was built from: every physics node with its
    /// body type and whether it has a hinge — rebuild when any of that
    /// changes (a spawned clone, a deleted node, a ragdoll switching its
    /// limbs from Kinematic to Dynamic). Velocities survive a rebuild.
    signature: Vec<(NodeId, u8)>,
    accumulator: f32,
    events: Vec<ContactEvent>,
    /// Forces requested this frame (px/s² * mass), applied over its substeps.
    pending_forces: HashMap<NodeId, Vec2>,
    /// Running Liquid 2D fluids, per node. Kept across world rebuilds
    /// (a spawned body mustn't reset the water).
    liquids: HashMap<NodeId, liquid::LiquidSim>,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsWorld {
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
            liquids: HashMap::new(),
        }
    }

    /// Every node taking part in physics (a Rigidbody 2D or a collider),
    /// in a stable order — the membership signature.
    fn physics_nodes(artboard: &Artboard) -> Vec<NodeId> {
        let mut nodes: Vec<NodeId> = artboard
            .node_ids()
            .filter(|&id| active(artboard, id))
            .filter(|&id| artboard.node(id).is_some_and(|n| n.rigidbody().is_some() || n.colliders().next().is_some()))
            .collect();
        nodes.sort();
        nodes
    }

    fn signature_of(artboard: &Artboard, nodes: &[NodeId]) -> Vec<(NodeId, u8)> {
        nodes
            .iter()
            .map(|&id| {
                let node = artboard.node(id);
                let body = node.and_then(|n| n.rigidbody()).map_or(0, |rb| 1 + rb.body_type as u8);
                let hinge = node.is_some_and(|n| n.hinge_joint().is_some()) as u8;
                (id, body | (hinge << 4))
            })
            .collect()
    }

    /// (Re)builds the Rapier world if the set of physics nodes changed since
    /// last time. Velocities of bodies that survive a rebuild are kept.
    pub fn sync(&mut self, artboard: &Artboard) {
        let nodes = Self::physics_nodes(artboard);
        let signature = Self::signature_of(artboard, &nodes);
        if signature == self.signature {
            return;
        }
        let kept: HashMap<NodeId, (Vector, f32)> = self
            .node_bodies
            .iter()
            .filter_map(|(id, info)| self.bodies.get(info.handle).map(|b| (*id, (b.linvel(), b.angvel()))))
            .collect();
        let gravity_kept = std::mem::take(&mut self.pending_forces);
        let liquids_kept = std::mem::take(&mut self.liquids);
        *self = Self::new();
        self.pending_forces = gravity_kept;
        self.liquids = liquids_kept;
        self.signature = signature;

        // Bodies: one per Rigidbody 2D node.
        for &id in &nodes {
            let node = artboard.node(id).expect("listed above");
            let Some(rb) = node.rigidbody() else { continue };
            let Some((pivot, rotation)) = world_pose(artboard, id) else { continue };
            let builder = match rb.body_type {
                RigidbodyType2D::Dynamic => RigidBodyBuilder::dynamic(),
                RigidbodyType2D::Kinematic => RigidBodyBuilder::kinematic_position_based(),
                RigidbodyType2D::Static => RigidBodyBuilder::fixed(),
            };
            let mut builder = builder
                .translation(to_rapier(pivot))
                .rotation(rotation)
                .linear_damping(rb.linear_drag.max(0.0))
                .angular_damping(rb.angular_drag.max(0.0))
                .gravity_scale(rb.gravity_scale)
                .ccd_enabled(rb.collision_detection == CollisionDetection2D::Continuous)
                .enabled_translations(!rb.freeze_position_x, !rb.freeze_position_y);
            if rb.freeze_rotation {
                builder = builder.lock_rotations();
            }
            let handle = self.bodies.insert(builder.build());
            if let Some((linvel, angvel)) = kept.get(&id) {
                self.bodies[handle].set_linvel(*linvel, false);
                self.bodies[handle].set_angvel(*angvel, false);
            }
            self.node_bodies.insert(id, BodyInfo { handle, body_type: rb.body_type, last_pose: (pivot, rotation) });
        }

        // Self-collision: bodies in one hierarchy (a ragdoll, a chain) don't
        // collide with each other — their limbs overlap at the joints by
        // design. Each such hierarchy (2+ bodies under one topmost body)
        // gets its own collision group bit; everything else collides with all.
        let mut hierarchy_bodies: HashMap<NodeId, usize> = HashMap::new();
        for &id in self.node_bodies.keys() {
            *hierarchy_bodies.entry(topmost_body(artboard, id)).or_insert(0) += 1;
        }
        let mut group_of_root: HashMap<NodeId, u32> = HashMap::new();
        let mut roots: Vec<NodeId> = hierarchy_bodies.iter().filter(|(_, count)| **count >= 2).map(|(root, _)| *root).collect();
        roots.sort();
        for (i, root) in roots.into_iter().enumerate() {
            group_of_root.insert(root, (i % 31) as u32);
        }

        // Colliders: attached to the owning body, or a fixed body of their own.
        for &id in &nodes {
            let node = artboard.node(id).expect("listed above");
            if node.colliders().next().is_none() {
                continue;
            }
            let owner = owning_body_node(artboard, id).filter(|o| self.node_bodies.contains_key(o));
            let body_node = match owner {
                Some(owner) => owner,
                None => {
                    let Some((pivot, rotation)) = world_pose(artboard, id) else { continue };
                    let handle = self.bodies.insert(RigidBodyBuilder::fixed().translation(to_rapier(pivot)).rotation(rotation).build());
                    self.node_bodies.insert(id, BodyInfo { handle, body_type: RigidbodyType2D::Static, last_pose: (pivot, rotation) });
                    id
                }
            };
            let body_handle = self.node_bodies[&body_node].handle;
            let (body_pivot, body_rotation) = self.node_bodies[&body_node].last_pose;
            let groups = match group_of_root.get(&topmost_body(artboard, body_node)) {
                Some(&bit) => {
                    let own = Group::from_bits_truncate(1 << bit);
                    InteractionGroups::new(own, Group::ALL ^ own, InteractionTestMode::And)
                }
                None => InteractionGroups::all(),
            };
            let Some(world) = artboard.try_world_transform(id) else { continue };
            let affine = world.to_affine2();
            let scale = world.scale.abs();
            let relative_rotation = world.rotation - body_rotation;
            // A node-local point -> its position relative to the body, in the
            // body's own (unrotated) frame, in Rapier units.
            let relative = |local: Vec2| to_rapier(rotate(affine.transform_point2(local) - body_pivot, -body_rotation));

            // Mass: Unity's Rigidbody 2D mass, spread over its colliders by area.
            let mass = owner.and_then(|o| artboard.node(o)).and_then(|n| n.rigidbody()).map(|rb| rb.mass.max(0.0001)).unwrap_or(1.0);
            let colliders: Vec<_> = node.colliders().collect();
            let total_area: f32 = colliders.iter().map(|c| collider_area(c, scale)).sum::<f32>().max(1e-6);

            for collider in colliders {
                let (builder, is_trigger, material) = match collider {
                    ColliderRef::Box(c) => {
                        let half = (c.size * scale * 0.5).max(Vec2::splat(0.01)) / PIXELS_PER_METER;
                        (ColliderBuilder::cuboid(half.x, half.y).translation(relative(c.offset)).rotation(relative_rotation), c.is_trigger, c.material)
                    }
                    ColliderRef::Circle(c) => {
                        let radius = (c.radius * scale.max_element()).max(0.01) / PIXELS_PER_METER;
                        (ColliderBuilder::ball(radius).translation(relative(c.offset)), c.is_trigger, c.material)
                    }
                    ColliderRef::Capsule(c) => {
                        let size = (c.size * scale).max(Vec2::splat(0.02)) / PIXELS_PER_METER;
                        let builder = match c.direction {
                            CapsuleDirection2D::Vertical => {
                                let radius = size.x * 0.5;
                                ColliderBuilder::capsule_y((size.y * 0.5 - radius).max(0.0), radius)
                            }
                            CapsuleDirection2D::Horizontal => {
                                let radius = size.y * 0.5;
                                ColliderBuilder::capsule_x((size.x * 0.5 - radius).max(0.0), radius)
                            }
                        };
                        (builder.translation(relative(c.offset)).rotation(relative_rotation), c.is_trigger, c.material)
                    }
                    ColliderRef::Polygon(c) => {
                        let points: Vec<Vector> = c.points.iter().map(|p| relative(c.offset + *p)).collect();
                        let Some(builder) = ColliderBuilder::convex_hull(&points) else { continue };
                        (builder, c.is_trigger, c.material)
                    }
                };
                let PhysicsMaterial2D { friction, bounciness } = material;
                let area = collider_area(&collider, scale) / (PIXELS_PER_METER * PIXELS_PER_METER);
                let density = mass / (total_area / (PIXELS_PER_METER * PIXELS_PER_METER)) * if area > 0.0 { 1.0 } else { 0.0 };
                let built = builder
                    .sensor(is_trigger)
                    .friction(friction.max(0.0))
                    .restitution(bounciness.clamp(0.0, 1.0))
                    .density(density.max(0.0))
                    .active_events(ActiveEvents::COLLISION_EVENTS)
                    .active_collision_types(ActiveCollisionTypes::all())
                    .collision_groups(groups)
                    .build();
                let handle = self.colliders.insert_with_parent(built, body_handle, &mut self.bodies);
                self.collider_nodes.insert(handle, (id, is_trigger));
            }
        }

        // Joints: Hinge Joint 2D -> a Rapier revolute joint between this
        // node's body (body 2) and the connected / parent body (body 1), or
        // a fixed anchor in the world. Limits are relative to the pose the
        // joint is built in, so the authored pose is the "rest" angle.
        for &id in &nodes {
            let Some(hinge) = artboard.node(id).and_then(|n| n.hinge_joint()).copied() else { continue };
            let Some(own) = owning_body_node(artboard, id).filter(|o| self.node_bodies.contains_key(o)) else { continue };
            let other = hinge
                .connected_body
                .filter(|b| *b != own && self.node_bodies.contains_key(b))
                .or_else(|| {
                    let parent = artboard.node(own).and_then(|n| n.parent)?;
                    owning_body_node(artboard, parent).filter(|o| self.node_bodies.contains_key(o))
                });
            let Some(world) = artboard.try_world_transform(id) else { continue };
            let anchor_world = world.to_affine2().transform_point2(hinge.anchor);
            let (own_pivot, own_rot) = self.node_bodies[&own].last_pose;
            let (other_handle, other_pivot, other_rot) = match other {
                Some(other) => {
                    let info = &self.node_bodies[&other];
                    (info.handle, info.last_pose.0, info.last_pose.1)
                }
                None => {
                    let handle = self.bodies.insert(RigidBodyBuilder::fixed().translation(to_rapier(anchor_world)).build());
                    (handle, anchor_world, 0.0)
                }
            };
            let local_other = to_rapier(rotate(anchor_world - other_pivot, -other_rot));
            let local_own = to_rapier(rotate(anchor_world - own_pivot, -own_rot));
            let mut joint = RevoluteJointBuilder::new().local_anchor1(local_other).local_anchor2(local_own).contacts_enabled(hinge.enable_collision);
            if hinge.use_limits {
                let rest = own_rot - other_rot;
                let (lo, hi) = (hinge.lower_angle.min(hinge.upper_angle).to_radians(), hinge.lower_angle.max(hinge.upper_angle).to_radians());
                joint = joint.limits([rest + lo, rest + hi]);
            }
            if hinge.use_motor {
                joint = joint.motor_velocity(hinge.motor_speed.to_radians(), 1.0).motor_max_force(hinge.max_motor_torque.max(0.0));
            }
            self.impulse_joints.insert(other_handle, self.node_bodies[&own].handle, joint, true);
        }

        // Prime Rapier's broad phase (what raycasts query) with a near-zero
        // step, so queries work on the very first frame after a rebuild.
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

    /// Advances the simulation by `delta_seconds` (fixed substeps) and
    /// writes dynamic bodies' poses back into `artboard`. Call [`sync`]
    /// first.
    pub fn step(&mut self, artboard: &mut Artboard, delta_seconds: f32) {
        // 1. Script-moved nodes / kinematic bodies -> Rapier.
        for (id, info) in self.node_bodies.iter_mut() {
            let Some((pivot, rotation)) = world_pose(artboard, *id) else { continue };
            let Some(body) = self.bodies.get_mut(info.handle) else { continue };
            match info.body_type {
                RigidbodyType2D::Kinematic => {
                    body.set_next_kinematic_translation(to_rapier(pivot));
                    body.set_next_kinematic_rotation(Rotation::from_angle(rotation));
                }
                _ => {
                    let moved = pivot.distance(info.last_pose.0) > 0.01 || (rotation - info.last_pose.1).abs() > 1e-4;
                    if moved {
                        body.set_translation(to_rapier(pivot), true);
                        body.set_rotation(Rotation::from_angle(rotation), true);
                    }
                }
            }
            info.last_pose = (pivot, rotation);
        }

        // 2. Fixed substeps, with this frame's script forces applied on each.
        for (id, force) in &self.pending_forces {
            if let Some(body) = self.node_bodies.get(id).and_then(|i| self.bodies.get_mut(i.handle)) {
                body.add_force(to_rapier(*force), true);
            }
        }
        self.accumulator = (self.accumulator + delta_seconds).min(FIXED_DT * MAX_SUBSTEPS as f32);
        let gravity = to_rapier(artboard.gravity);
        // Liquid 2D nodes: start a fluid for each new one, drop removed ones.
        let liquid_nodes: Vec<(NodeId, an1mbyt3_core::Liquid2D, Vec2)> = artboard
            .node_ids()
            .filter(|id| !artboard.is_effectively_deleted(*id))
            .filter_map(|id| artboard.node(id)?.liquid().map(|l| (id, *l, artboard.world_transform(id).translation)))
            .collect();
        self.liquids.retain(|id, _| liquid_nodes.iter().any(|(n, _, _)| n == id));
        for (id, config, at) in &liquid_nodes {
            self.liquids.entry(*id).or_insert_with(|| liquid::LiquidSim::new(config, *at));
        }
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
            if !liquid_nodes.is_empty() {
                step_liquids(&mut self.liquids, &liquid_nodes, &mut self.bodies, &self.colliders, artboard.gravity, FIXED_DT);
            }
        }
        for body in self.bodies.iter_mut().map(|(_, b)| b) {
            body.reset_forces(false);
        }
        self.pending_forces.clear();

        // 3. Dynamic bodies -> nodes, parents before children: a ragdoll
        // limb's local pose is computed against its parent's *new* pose.
        let mut order: Vec<NodeId> = self.node_bodies.iter().filter(|(_, info)| info.body_type == RigidbodyType2D::Dynamic).map(|(id, _)| *id).collect();
        order.sort_by_key(|id| depth(artboard, *id));
        for id in order {
            let id = &id;
            let Some(info) = self.node_bodies.get_mut(id) else { continue };
            let Some(body) = self.bodies.get(info.handle) else { continue };
            let pivot = from_rapier(body.translation());
            let rotation = body.rotation().angle();
            let parent = parent_affine(artboard, *id);
            let parent_rotation = artboard
                .node(*id)
                .and_then(|n| n.parent)
                .and_then(|p| artboard.try_world_transform(p))
                .map(|t| t.rotation)
                .unwrap_or(0.0);
            if let Some(node) = artboard.node_mut(*id) {
                node.transform.translation = parent.inverse().transform_point2(pivot);
                node.transform.rotation = rotation - parent_rotation;
            }
            info.last_pose = (pivot, rotation);
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
        // Nobody reading events (no script running) mustn't leak memory:
        // keep only the newest batch.
        const MAX_PENDING_EVENTS: usize = 1024;
        if self.events.len() > MAX_PENDING_EVENTS {
            let excess = self.events.len() - MAX_PENDING_EVENTS;
            self.events.drain(..excess);
        }
    }

    /// A Liquid 2D node's running fluid, if Playground has started it.
    pub fn liquid(&self, node: NodeId) -> Option<&liquid::LiquidSim> {
        self.liquids.get(&node)
    }

    /// How many bodies the world currently simulates (profiling).
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// Contact/trigger events since the last call.
    pub fn take_events(&mut self) -> Vec<ContactEvent> {
        std::mem::take(&mut self.events)
    }

    fn body(&self, node: NodeId) -> Option<&RigidBody> {
        self.node_bodies.get(&node).and_then(|info| self.bodies.get(info.handle))
    }

    fn body_mut(&mut self, node: NodeId) -> Option<&mut RigidBody> {
        let handle = self.node_bodies.get(&node)?.handle;
        self.bodies.get_mut(handle)
    }

    /// Linear velocity in world units per second.
    pub fn velocity(&self, node: NodeId) -> Option<Vec2> {
        self.body(node).map(|b| from_rapier(b.linvel()))
    }

    pub fn set_velocity(&mut self, node: NodeId, velocity: Vec2) {
        if let Some(body) = self.body_mut(node) {
            body.set_linvel(to_rapier(velocity), true);
        }
    }

    /// Angular velocity, degrees per second (counter-clockwise).
    pub fn angular_velocity(&self, node: NodeId) -> Option<f32> {
        self.body(node).map(|b| b.angvel().to_degrees())
    }

    pub fn set_angular_velocity(&mut self, node: NodeId, degrees_per_second: f32) {
        if let Some(body) = self.body_mut(node) {
            body.set_angvel(degrees_per_second.to_radians(), true);
        }
    }

    /// Unity's `AddForce` (ForceMode2D.Force): applied over the next frame.
    pub fn add_force(&mut self, node: NodeId, force: Vec2) {
        *self.pending_forces.entry(node).or_insert(Vec2::ZERO) += force;
    }

    /// Unity's `AddForce(..., ForceMode2D.Impulse)`: an instant kick
    /// (velocity change = impulse / mass).
    pub fn add_impulse(&mut self, node: NodeId, impulse: Vec2) {
        if let Some(body) = self.body_mut(node) {
            body.apply_impulse(to_rapier(impulse), true);
        }
    }

    /// Unity's `Physics2D.Raycast`: the first non-trigger collider along
    /// `direction` from `origin` within `max_distance` (world units),
    /// optionally ignoring `exclude`'s own colliders.
    pub fn raycast(&self, origin: Vec2, direction: Vec2, max_distance: f32, exclude: Option<NodeId>) -> Option<RayHit> {
        let dir = direction.normalize_or_zero();
        if dir == Vec2::ZERO {
            return None;
        }
        let excluded_body = exclude.and_then(|n| self.node_bodies.get(&n)).map(|i| i.handle);
        let mut filter = QueryFilter::default().exclude_sensors();
        if let Some(handle) = excluded_body {
            filter = filter.exclude_rigid_body(handle);
        }
        let query = self.broad_phase.as_query_pipeline(self.narrow_phase.query_dispatcher(), &self.bodies, &self.colliders, filter);
        let ray = Ray::new(to_rapier(origin), Vector::new(dir.x, dir.y));
        let (handle, hit) = query.cast_ray_and_get_normal(&ray, max_distance / PIXELS_PER_METER, true)?;
        let (node, _) = *self.collider_nodes.get(&handle)?;
        let distance = hit.time_of_impact * PIXELS_PER_METER;
        Some(RayHit { node, point: origin + dir * distance, normal: Vec2::new(hit.normal.x, hit.normal.y), distance })
    }
}

/// Per-particle mass (Rapier kg) used when the fluid pushes a body.
/// Small on purpose: floating comes from buoyancy below, the per-particle
/// pushes only add the splash and current.
const LIQUID_PARTICLE_MASS: f32 = 0.004;

/// One fixed step of every liquid: fluid relaxation, then collisions with
/// every solid collider (pushing dynamic bodies back), then buoyancy and
/// drag on the bodies it covers. Two substeps per physics step keep fast
/// splashes from tunnelling through thin walls.
fn step_liquids(
    liquids: &mut HashMap<NodeId, liquid::LiquidSim>,
    nodes: &[(NodeId, an1mbyt3_core::Liquid2D, Vec2)],
    bodies: &mut RigidBodySet,
    colliders: &ColliderSet,
    gravity_px: Vec2,
    dt: f32,
) {
    // Every solid collider, with its pixel-space bounds.
    let solids: Vec<(ColliderHandle, Vec2, Vec2)> = colliders
        .iter()
        .filter(|(_, c)| !c.is_sensor())
        .map(|(h, c)| {
            let aabb = c.compute_aabb();
            (h, from_rapier(aabb.mins), from_rapier(aabb.maxs))
        })
        .collect();
    const SUBSTEPS: usize = 2;
    let sub = dt / SUBSTEPS as f32;
    for (node, config, origin) in nodes {
        let Some(sim) = liquids.get_mut(node) else { continue };
        let radius = config.particle_spacing.max(2.0) * 0.5;
        let mut covered: HashMap<ColliderHandle, u32> = HashMap::new();
        for step in 0..SUBSTEPS {
            sim.emit(config, *origin, sub);
            sim.relax(config, gravity_px, sub);
            for i in 0..sim.particles.len() {
                let mut p = sim.particles[i].position;
                for &(handle, min, max) in &solids {
                    if p.x < min.x - radius || p.x > max.x + radius || p.y < min.y - radius || p.y > max.y + radius {
                        continue;
                    }
                    let collider = &colliders[handle];
                    let projection = collider.shape().project_point(collider.position(), to_rapier(p), false);
                    let surface = from_rapier(projection.point);
                    let d = p - surface;
                    let dist = d.length();
                    if !projection.is_inside && dist >= radius {
                        continue;
                    }
                    let mut normal = if dist > 1e-4 { if projection.is_inside { -d / dist } else { d / dist } } else { Vec2::Y };
                    let mut target = surface + normal * radius;
                    // Inside a thin wall, the nearest surface can be the far
                    // side — pushing there would pop the particle through.
                    // If so, send it back the way it came instead.
                    let back = sim.previous(i);
                    if projection.is_inside && (surface - p).dot(back - p) < 0.0 {
                        let away = back - p;
                        normal = if away.length_squared() > 1e-8 { away.normalize() } else { normal };
                        target = back;
                    }
                    let body = collider.parent().and_then(|b| bodies.get(b).map(|body| (b, body)));
                    let surface_v = body.map_or(Vec2::ZERO, |(_, body)| from_rapier(body.velocity_at_point(to_rapier(target))));
                    let change = sim.collide(i, target, normal, surface_v, 0.15, sub);
                    if let Some((handle_b, body)) = body {
                        if body.is_dynamic() {
                            let impulse = -change / PIXELS_PER_METER * LIQUID_PARTICLE_MASS;
                            if let Some(body) = bodies.get_mut(handle_b) {
                                body.apply_impulse_at_point(Vector::new(impulse.x, impulse.y), to_rapier(target), true);
                            }
                            if step == 0 {
                                *covered.entry(handle).or_insert(0) += 1;
                            }
                        }
                    }
                    p = target;
                }
                // Still inside something (squeezed between two touching
                // colliders, or between a body and a wall)? Then no push
                // was safe: put it back where this step started.
                let p = sim.particles[i].position;
                let stuck = solids.iter().any(|&(handle, min, max)| {
                    p.x >= min.x && p.x <= max.x && p.y >= min.y && p.y <= max.y && {
                        let collider = &colliders[handle];
                        collider.shape().contains_point(collider.position(), to_rapier(p))
                    }
                });
                if stuck {
                    let back = sim.previous(i);
                    sim.collide(i, back, Vec2::Y, Vec2::ZERO, 1.0, sub);
                }
            }
            sim.finish(sub);
        }
        // Buoyancy + drag for every dynamic body the fluid touches, from
        // how much of its collider area the touching particles cover.
        let spacing2 = config.particle_spacing.max(2.0).powi(2);
        let g = to_rapier(gravity_px);
        for (handle, count) in covered {
            let collider = &colliders[handle];
            let area = collider.shape().mass_properties(1.0).mass() * PIXELS_PER_METER * PIXELS_PER_METER;
            if area <= 0.0 {
                continue;
            }
            // Particles hug the surface, so each one in contact stands for
            // a little more than its own cell of fluid.
            let submerged = ((count as f32 * spacing2 * 1.4) / area).min(1.0);
            let Some(body) = collider.parent().and_then(|b| bodies.get_mut(b)) else { continue };
            let mass = body.mass();
            // Archimedes: the weight of the liquid displaced (density x the
            // submerged part of the collider's area, in Rapier m²).
            let displaced_area = submerged * area / (PIXELS_PER_METER * PIXELS_PER_METER);
            let lift = -g * config.buoyancy * displaced_area * dt;
            body.apply_impulse(lift, true);
            let drag = (config.drag * submerged * dt).min(0.9);
            let v = body.linvel();
            body.apply_impulse(-v * mass * drag, true);
            let w = body.angvel();
            body.set_angvel(w * (1.0 - drag * 0.6), true);
        }
    }
}

fn collider_area(collider: &ColliderRef<'_>, scale: Vec2) -> f32 {
    match collider {
        ColliderRef::Box(c) => (c.size * scale).abs().element_product(),
        ColliderRef::Circle(c) => std::f32::consts::PI * (c.radius * scale.max_element()).powi(2),
        ColliderRef::Capsule(c) => {
            let size = (c.size * scale).abs();
            let r = size.min_element() * 0.5;
            std::f32::consts::PI * r * r + (size.max_element() - 2.0 * r).max(0.0) * 2.0 * r
        }
        ColliderRef::Polygon(c) => {
            let n = c.points.len();
            let twice: f32 = (0..n).map(|i| c.points[i].perp_dot(c.points[(i + 1) % n])).sum();
            (twice * 0.5).abs() * scale.x.abs() * scale.y.abs()
        }
    }
}

/// Set of nodes in a membership signature (handy for callers diffing).
pub fn node_set(nodes: &[NodeId]) -> HashSet<NodeId> {
    nodes.iter().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use an1mbyt3_core::{BoxCollider2D, CircleCollider2D, Node, NodeBehavior, Rigidbody2D};

    fn scene() -> (Artboard, NodeId, NodeId) {
        let mut artboard = Artboard::new("Physics", 400.0, 300.0);
        let mut ground = Node::with_components(
            "Ground",
            None,
            vec![NodeBehavior::BoxCollider2D(BoxCollider2D { size: Vec2::new(400.0, 20.0), ..Default::default() })],
        );
        ground.transform.translation = Vec2::new(0.0, -100.0);
        let ground_id = artboard.add_node(ground);
        let mut ball = Node::with_components(
            "Ball",
            None,
            vec![NodeBehavior::Rigidbody2D(Rigidbody2D::default()), NodeBehavior::CircleCollider2D(CircleCollider2D { radius: 10.0, ..Default::default() })],
        );
        ball.transform.translation = Vec2::new(0.0, 50.0);
        let ball_id = artboard.add_node(ball);
        (artboard, ground_id, ball_id)
    }

    /// A bone of `length` along +x with a body, a capsule and a hinge at its
    /// joint (origin).
    fn limb(name: &str, parent: Option<NodeId>, length: f32, at: Vec2, limits: Option<(f32, f32)>) -> Node {
        use an1mbyt3_core::{BoneNode, CapsuleCollider2D, CapsuleDirection2D, HingeJoint2D, NodeKind};
        let mut node = Node::new(name, parent, NodeKind::Bone(BoneNode { length }));
        node.transform.translation = at;
        let hinge = HingeJoint2D { use_limits: limits.is_some(), lower_angle: limits.map_or(0.0, |l| l.0), upper_angle: limits.map_or(0.0, |l| l.1), ..Default::default() };
        node.behaviors = vec![
            NodeBehavior::Rigidbody2D(Rigidbody2D::default()),
            NodeBehavior::CapsuleCollider2D(CapsuleCollider2D { offset: Vec2::new(length / 2.0, 0.0), size: Vec2::new(length, 8.0), direction: CapsuleDirection2D::Horizontal, ..Default::default() }),
            NodeBehavior::HingeJoint2D(hinge),
        ]
        .into_iter()
        .map(|behavior| an1mbyt3_core::NodeBehaviorEntry { behavior, enabled: true })
        .collect();
        node
    }

    fn run(world: &mut PhysicsWorld, artboard: &mut Artboard, frames: usize) {
        for _ in 0..frames {
            world.sync(artboard);
            world.step(artboard, 1.0 / 60.0);
        }
    }

    /// A walled tank (static box colliders), water in it, and a crate.
    fn tank(crate_mass: f32, buoyancy: f32) -> (Artboard, NodeId, NodeId) {
        let mut artboard = Artboard::new("Tank", 400.0, 300.0);
        let wall = |name: &str, at: Vec2, size: Vec2| {
            let mut n = Node::with_components(name, None, vec![NodeBehavior::BoxCollider2D(BoxCollider2D { size, ..Default::default() })]);
            n.transform.translation = at;
            n
        };
        artboard.add_node(wall("Floor", Vec2::new(0.0, -10.0), Vec2::new(240.0, 20.0)));
        artboard.add_node(wall("Left", Vec2::new(-110.0, 60.0), Vec2::new(20.0, 160.0)));
        artboard.add_node(wall("Right", Vec2::new(110.0, 60.0), Vec2::new(20.0, 160.0)));
        let liquid = an1mbyt3_core::Liquid2D { size: Vec2::new(190.0, 70.0), buoyancy, ..Default::default() };
        let mut water = Node::with_components("Water", None, vec![NodeBehavior::Liquid2D(liquid)]);
        water.transform.translation = Vec2::new(0.0, 40.0);
        let water_id = artboard.add_node(water);
        let mut crate_node = Node::with_components(
            "Crate",
            None,
            vec![NodeBehavior::Rigidbody2D(Rigidbody2D { mass: crate_mass, ..Default::default() }), NodeBehavior::BoxCollider2D(BoxCollider2D { size: Vec2::new(24.0, 24.0), ..Default::default() })],
        );
        crate_node.transform.translation = Vec2::new(0.0, 140.0);
        let crate_id = artboard.add_node(crate_node);
        (artboard, water_id, crate_id)
    }

    #[test]
    fn water_stays_in_its_tank_and_a_crate_floats_on_it() {
        let (mut artboard, water, crate_id) = tank(1.0, 2.5);
        let mut world = PhysicsWorld::new();
        run(&mut world, &mut artboard, 360);
        let sim = world.liquid(water).expect("liquid started");
        let n = sim.particles.len();
        assert!(n > 100);
        let escaped = sim.particles.iter().filter(|p| p.position.x.abs() > 101.0 || p.position.y < -1.0).count();
        assert!(escaped == 0, "{escaped} of {n} particles left the tank");
        let surface = {
            let mut ys: Vec<f32> = sim.particles.iter().map(|p| p.position.y).collect();
            ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
            ys[ys.len() * 9 / 10]
        };
        let y = artboard.world_transform(crate_id).translation.y;
        eprintln!("DEBUG particles={n} surface~{surface:.1} crate y={y:.1}");
        // Floats: near the surface, clearly off the floor (floor top = 0).
        assert!(y > surface - 18.0 && y < surface + 30.0, "crate at {y}, water surface about {surface}");
    }

    #[test]
    fn a_dense_stone_sinks_where_a_crate_would_float() {
        let (mut artboard, _, crate_id) = tank(5.0, 2.5);
        let mut world = PhysicsWorld::new();
        run(&mut world, &mut artboard, 420);
        let y = artboard.world_transform(crate_id).translation.y;
        eprintln!("DEBUG sinking crate y={y:.1}");
        assert!(y < 30.0, "sank toward the floor: {y}");
    }

    #[test]
    fn a_pinned_pendulum_swings_but_keeps_its_pin() {
        let mut artboard = Artboard::new("Joints", 400.0, 300.0);
        // Horizontal arm pinned to the world at (0, 50): gravity swings it down.
        let arm = artboard.add_node(limb("Arm", None, 60.0, Vec2::new(0.0, 50.0), None));
        let mut world = PhysicsWorld::new();
        run(&mut world, &mut artboard, 90);
        let t = artboard.world_transform(arm);
        assert!(t.translation.distance(Vec2::new(0.0, 50.0)) < 1.5, "pin holds: {:?}", t.translation);
        assert!(t.rotation < -0.5, "arm swung down: {}", t.rotation);
    }

    #[test]
    fn hinge_limits_stop_the_swing() {
        let mut artboard = Artboard::new("Joints", 400.0, 300.0);
        let arm = artboard.add_node(limb("Arm", None, 60.0, Vec2::new(0.0, 50.0), Some((-20.0, 20.0))));
        let mut world = PhysicsWorld::new();
        run(&mut world, &mut artboard, 120);
        let rot = artboard.world_transform(arm).rotation.to_degrees();
        assert!(rot > -24.0 && rot < 1.0, "swing limited to -20 deg: {rot}");
    }

    #[test]
    fn a_two_bone_ragdoll_stays_connected() {
        let mut artboard = Artboard::new("Joints", 400.0, 300.0);
        let upper = artboard.add_node(limb("Upper", None, 40.0, Vec2::new(0.0, 80.0), None));
        // Lower bone sits at the upper bone's tip, hinged to it (its parent).
        let lower = artboard.add_node(limb("Lower", Some(upper), 40.0, Vec2::new(40.0, 0.0), Some((-90.0, 90.0))));
        let mut world = PhysicsWorld::new();
        run(&mut world, &mut artboard, 120);
        let tip = artboard.world_transform(upper).to_affine2().transform_point2(Vec2::new(40.0, 0.0));
        let joint = artboard.world_transform(lower).translation;
        assert!(tip.distance(joint) < 2.0, "elbow stays together: tip {tip:?} vs joint {joint:?}");
        assert!(artboard.world_transform(upper).translation.distance(Vec2::new(0.0, 80.0)) < 1.5, "shoulder pinned to the world");
    }

    #[test]
    fn a_ball_falls_and_comes_to_rest_on_the_ground() {
        let (mut artboard, ground, ball) = scene();
        let mut world = PhysicsWorld::new();
        let mut touched = false;
        for _ in 0..240 {
            world.sync(&artboard);
            world.step(&mut artboard, 1.0 / 60.0);
            touched |= world.take_events().iter().any(|e| e.started && ((e.a == ball && e.b == ground) || (e.a == ground && e.b == ball)));
        }
        let y = artboard.node(ball).unwrap().transform.translation.y;
        // Ground top at -90, ball radius 10 -> resting center near -80.
        assert!((y + 80.0).abs() < 1.5, "ball rests on the ground, got y = {y}");
        assert!(touched, "a contact-started event fired");
    }

    #[test]
    fn script_teleports_and_impulses_reach_the_body() {
        let (mut artboard, _ground, ball) = scene();
        let mut world = PhysicsWorld::new();
        world.sync(&artboard);
        world.step(&mut artboard, 1.0 / 60.0);
        artboard.node_mut(ball).unwrap().transform.translation = Vec2::new(150.0, 100.0);
        world.add_impulse(ball, Vec2::new(0.0, 0.0));
        world.set_velocity(ball, Vec2::new(60.0, 0.0));
        world.step(&mut artboard, 1.0 / 60.0);
        let p = artboard.node(ball).unwrap().transform.translation;
        assert!(p.x > 150.0 && p.x < 153.0, "teleported then moved right, got {p:?}");
    }

    #[test]
    fn raycast_finds_the_ground() {
        let (artboard, ground, _ball) = scene();
        let mut world = PhysicsWorld::new();
        world.sync(&artboard);
        let _ = &artboard;
        let hit = world.raycast(Vec2::new(100.0, 0.0), Vec2::new(0.0, -1.0), 500.0, None).expect("hits the ground");
        assert_eq!(hit.node, ground);
        assert!((hit.distance - 90.0).abs() < 0.5);
    }
}
