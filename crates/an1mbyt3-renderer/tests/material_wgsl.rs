//! Shader-graph materials: every node kind's generated WGSL must validate
//! (naga, the front end wgpu itself uses) once linked against the real
//! sprite shader + material glue — no GPU needed.

use an1mbyt3_core::{compile_wgsl, ShaderDomain, ShaderEdgeDef, ShaderGraphDef, ShaderNodeDef, ShaderNodeKind};
use an1mbyt3_renderer::Renderer;
use glam::Vec2;

fn node(id: u32, kind: ShaderNodeKind) -> ShaderNodeDef {
    ShaderNodeDef { id, kind, position: Vec2::ZERO, value: [0.5; 4], defaults: Vec::new() }
}

#[test]
fn starter_material_validates() {
    let wgsl = compile_wgsl(&ShaderGraphDef::starter()).unwrap();
    Renderer::validate_material(&wgsl).unwrap_or_else(|e| panic!("{e}"));
}

/// Each kind alone into the Output, and each kind with every input wired
/// from a Vec4 / Vec2 / Float source (exercising every conversion path).
#[test]
fn every_node_kind_validates_with_every_input_type() {
    for &kind in ShaderNodeKind::ALL {
        if kind == ShaderNodeKind::Output {
            continue;
        }
        for source in [None, Some(ShaderNodeKind::ColorConstant), Some(ShaderNodeKind::Uv), Some(ShaderNodeKind::Time)] {
            let mut nodes = vec![node(1, kind), node(2, ShaderNodeKind::Output)];
            let mut edges = vec![ShaderEdgeDef { from: 1, from_port: 0, to: 2, to_port: 0 }];
            if let Some(src) = source {
                nodes.push(node(3, src));
                for port in 0..kind.inputs().len() as u8 {
                    edges.push(ShaderEdgeDef { from: 3, from_port: 0, to: 1, to_port: port });
                }
            }
            let graph = ShaderGraphDef { domain: ShaderDomain::Canvas2D, nodes, edges };
            let wgsl = compile_wgsl(&graph).unwrap_or_else(|e| panic!("{kind:?} <- {source:?}: {e}"));
            Renderer::validate_material(&wgsl).unwrap_or_else(|e| panic!("{kind:?} <- {source:?}:\n{e}\n{wgsl}"));
            // The same graph must also link against the 3D pass.
            Renderer::validate_material_3d(&wgsl).unwrap_or_else(|e| panic!("3D {kind:?} <- {source:?}:
{e}"));
        }
    }
}

/// The block-world material (`an1mbyt3_geometry::VOXEL_MATERIAL_WGSL`) links
/// against the 3D pass: it reads the atlas, `uniforms.color` and the
/// AO-carrying normal, and cuts out transparent texels with `discard`.
#[test]
fn voxel_material_validates_in_3d() {
    Renderer::validate_material_3d(an1mbyt3_geometry::VOXEL_MATERIAL_WGSL).unwrap_or_else(|e| panic!("voxel material: {e}"));
}
