//! Record sanitisation: clamp every numeric field a malicious peer might
//! inflate to crash the engine or exhaust host RAM. The limits mirror the
//! ranges the World Editor UI already exposes, so a hand-crafted record
//! cannot trigger behaviour the owner couldn't have requested via the
//! normal interface.
//!
//! Each path that accepts a `RoomRecord`/`AvatarRecord`/`InventoryRecord`
//! from the network calls its `sanitize()` method before handing the record
//! to the world compiler; those impls live alongside the record types and
//! delegate into the per-variant helpers defined here.

use super::generator::Generator;
use super::prim::PrimNode;
use super::terrain::SovereignTerrainConfig;
use super::texture::{SovereignMaterialSettings, SovereignTextureConfig};
use super::types::{Fp, Fp3, Fp4, TransformData, truncate_on_char_boundary};

/// Maximum values allowed in a `RoomRecord`. Record fields outside these
/// bounds are clamped rather than rejected so slightly exotic records from
/// forward-compatible clients still round-trip, but a weaponised payload
/// cannot force a runaway allocation.
pub mod limits {
    /// Heightmap edge length (cells per side). 2048² ≈ 4M f32 cells ≈ 16 MiB.
    pub const MAX_GRID_SIZE: u32 = 2048;
    /// FBM / noise octaves.
    pub const MAX_OCTAVES: u32 = 32;
    /// Voronoi seed-point count.
    pub const MAX_VORONOI_SEEDS: u32 = 10_000;
    /// Voronoi terrace-level count.
    pub const MAX_VORONOI_TERRACES: u32 = 64;
    /// Hydraulic erosion drop count.
    pub const MAX_EROSION_DROPS: u32 = 500_000;
    /// Thermal erosion iteration count.
    pub const MAX_THERMAL_ITERATIONS: u32 = 500;
    /// Splat texture resolution per side (pixels).
    pub const MAX_TEXTURE_SIZE: u32 = 4096;
    /// Ground / rock generator octaves.
    pub const MAX_GROUND_OCTAVES: u32 = 12;
    pub const MAX_ROCK_OCTAVES: u32 = 16;
    /// Scatter placement count.
    pub const MAX_SCATTER_COUNT: u32 = 100_000;
    /// L-system derivation iterations. 12 is already enough to blow out most
    /// lexical grammars — anything beyond this is almost certainly an attack.
    pub const MAX_LSYSTEM_ITERATIONS: u32 = 12;
    /// L-system source / finalization code length in bytes.
    pub const MAX_LSYSTEM_CODE_BYTES: usize = 16_384;
    /// L-system mesh resolution (stroke segments per twig).
    pub const MAX_LSYSTEM_MESH_RESOLUTION: u32 = 32;
    /// Shape generator floor count.
    pub const MAX_SHAPE_FLOORS: u32 = 64;
    /// Maximum number of `Placement` entries per `RoomRecord`. Clamping
    /// `Scatter.count` alone is not enough — a record with ten-thousand
    /// single-count scatter entries still weaponises `compile_room_record`.
    pub const MAX_PLACEMENTS: usize = 1_024;
    /// Maximum number of generators per `RoomRecord`. Every generator also
    /// materialises per-peer state (L-system material cache, lookup work
    /// in hot loops) so a record with a million generator entries would
    /// still inflate memory and slow every `compile_room_record` pass even
    /// if no placement referenced them.
    pub const MAX_GENERATORS: usize = 256;
    /// Maximum recursion depth for a `Construct` primitive tree. Deep
    /// hierarchies cost an entity + Transform chain per node; 16 is well
    /// past any plausible hand-authored assembly.
    pub const MAX_CONSTRUCT_DEPTH: u32 = 16;
    /// Maximum total node count for a single `Construct` generator. A
    /// malicious record with a million children would otherwise spawn a
    /// million Bevy entities + colliders on every compile pass.
    pub const MAX_CONSTRUCT_NODES: u32 = 1024;
}

/// Recursively clamp a `Construct` primitive tree. Beyond the depth and
/// total-node budgets (see [`limits::MAX_CONSTRUCT_DEPTH`] and
/// [`limits::MAX_CONSTRUCT_NODES`]), each node's transform and material are
/// clamped so a malicious record can't pass NaN/negative scales to Bevy's
/// primitive mesh constructors or the Avian collider builders.
pub(crate) fn sanitize_prim_node(node: &mut PrimNode, depth: u32, count: &mut u32) {
    *count += 1;
    node.shape.sanitize();
    sanitize_prim_transform(&mut node.transform);
    sanitize_material_settings(&mut node.material);

    if depth >= limits::MAX_CONSTRUCT_DEPTH || *count >= limits::MAX_CONSTRUCT_NODES {
        node.children.clear();
        return;
    }
    // Drop the tail children whose recursion budget we couldn't afford so
    // the survivor count matches the spawn budget exactly. We track the
    // loop index directly — the previous attempt to derive it from
    // `*count - MAX_CONSTRUCT_NODES` evaluated to zero on the nominal
    // break path, leaving the unvisited subtrees (with their unsanitized
    // transforms and materials) in place and bypassing every cap below.
    let mut visited = 0usize;
    for (i, child) in node.children.iter_mut().enumerate() {
        if *count >= limits::MAX_CONSTRUCT_NODES {
            break;
        }
        sanitize_prim_node(child, depth + 1, count);
        visited = i + 1;
    }
    if visited < node.children.len() {
        node.children.truncate(visited);
    }
}

/// Clamp a `TransformData` so the downstream Bevy/Avian constructors can't
/// be fed NaN, infinities, or non-positive scales.
pub(crate) fn sanitize_prim_transform(t: &mut TransformData) {
    let finite = |v: f32, default: f32| if v.is_finite() { v } else { default };
    let clamp_pos = |v: f32| {
        if v.is_finite() {
            v.clamp(0.001, 1_000.0)
        } else {
            1.0
        }
    };
    let clamp_offset = |v: f32| {
        if v.is_finite() {
            v.clamp(-10_000.0, 10_000.0)
        } else {
            0.0
        }
    };
    t.translation = Fp3([
        clamp_offset(t.translation.0[0]),
        clamp_offset(t.translation.0[1]),
        clamp_offset(t.translation.0[2]),
    ]);
    let rot = [
        finite(t.rotation.0[0], 0.0),
        finite(t.rotation.0[1], 0.0),
        finite(t.rotation.0[2], 0.0),
        finite(t.rotation.0[3], 1.0),
    ];
    let len_sq = rot[0] * rot[0] + rot[1] * rot[1] + rot[2] * rot[2] + rot[3] * rot[3];
    t.rotation = if len_sq > 1e-6 {
        let inv = len_sq.sqrt().recip();
        Fp4([rot[0] * inv, rot[1] * inv, rot[2] * inv, rot[3] * inv])
    } else {
        Fp4([0.0, 0.0, 0.0, 1.0])
    };
    t.scale = Fp3([
        clamp_pos(t.scale.0[0]),
        clamp_pos(t.scale.0[1]),
        clamp_pos(t.scale.0[2]),
    ]);
}

/// Clamp a `SovereignMaterialSettings` so render/PBR parameters stay in
/// physically sensible ranges. Colour channels are `[0,1]`, roughness and
/// metallic are `[0,1]`, emission strength is capped. Also clamps the
/// embedded [`SovereignTextureConfig`] so octave-style DoS vectors can't
/// ride in via a PBR material.
pub(crate) fn sanitize_material_settings(m: &mut SovereignMaterialSettings) {
    let clamp_unit = |v: f32| {
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        }
    };
    let clamp3 = |c: Fp3| Fp3([clamp_unit(c.0[0]), clamp_unit(c.0[1]), clamp_unit(c.0[2])]);
    m.base_color = clamp3(m.base_color);
    m.emission_color = clamp3(m.emission_color);
    m.emission_strength = Fp(if m.emission_strength.0.is_finite() {
        m.emission_strength.0.clamp(0.0, 1_000.0)
    } else {
        0.0
    });
    m.roughness = Fp(clamp_unit(m.roughness.0));
    m.metallic = Fp(clamp_unit(m.metallic.0));
    m.uv_scale = Fp(if m.uv_scale.0.is_finite() {
        m.uv_scale.0.clamp(0.001, 1_000.0)
    } else {
        1.0
    });
    sanitize_texture_config(&mut m.texture);
}

/// Clamp octave-style fields on a `SovereignTextureConfig` variant so a
/// malicious record cannot tell the procedural texture pipeline to run
/// billions of noise iterations per pixel. Variants without an octave-like
/// parameter are passed through untouched — their cost is bounded by the
/// texture resolution cap in [`limits::MAX_TEXTURE_SIZE`].
pub(crate) fn sanitize_texture_config(cfg: &mut SovereignTextureConfig) {
    match cfg {
        SovereignTextureConfig::Ground(g) => {
            g.macro_octaves = g.macro_octaves.clamp(1, limits::MAX_GROUND_OCTAVES);
            g.micro_octaves = g.micro_octaves.clamp(1, limits::MAX_GROUND_OCTAVES);
        }
        SovereignTextureConfig::Rock(r) => {
            r.octaves = r.octaves.clamp(1, limits::MAX_ROCK_OCTAVES);
        }
        SovereignTextureConfig::Bark(b) => {
            b.octaves = b.octaves.clamp(1, limits::MAX_ROCK_OCTAVES);
        }
        SovereignTextureConfig::Stucco(s) => {
            s.octaves = s.octaves.clamp(1, limits::MAX_ROCK_OCTAVES);
        }
        SovereignTextureConfig::Concrete(c) => {
            c.octaves = c.octaves.clamp(1, limits::MAX_ROCK_OCTAVES);
        }
        SovereignTextureConfig::Marble(m) => {
            m.octaves = m.octaves.clamp(1, limits::MAX_ROCK_OCTAVES);
        }
        _ => {}
    }
}

/// Clamp a single `Generator` variant in place. Shared by
/// [`super::room::RoomRecord::sanitize`] and
/// [`super::inventory::InventoryRecord::sanitize`] so the per-variant
/// bounds stay identical between the room recipe and the inventory stash —
/// an inventory item that was safe on the PDS must stay safe the moment the
/// owner drags it back into their room.
pub fn sanitize_generator(generator: &mut Generator) {
    match generator {
        Generator::Terrain(cfg) => sanitize_terrain_cfg(cfg),
        Generator::LSystem {
            source_code,
            finalization_code,
            iterations,
            mesh_resolution,
            materials,
            ..
        } => {
            truncate_on_char_boundary(source_code, limits::MAX_LSYSTEM_CODE_BYTES);
            truncate_on_char_boundary(finalization_code, limits::MAX_LSYSTEM_CODE_BYTES);
            *iterations = (*iterations).min(limits::MAX_LSYSTEM_ITERATIONS);
            *mesh_resolution = (*mesh_resolution).clamp(3, limits::MAX_LSYSTEM_MESH_RESOLUTION);
            // Without this, a peer could ship a `Bark` slot with
            // `octaves = 4_000_000_000` (or NaN emission) and hang the
            // procedural texture task the moment a scatter lands.
            for settings in materials.values_mut() {
                sanitize_material_settings(settings);
            }
        }
        Generator::Shape { floors, .. } => {
            *floors = (*floors).min(limits::MAX_SHAPE_FLOORS);
        }
        Generator::Portal {
            target_did,
            target_pos,
        } => {
            truncate_on_char_boundary(target_did, 256);
            target_pos.0[0] = target_pos.0[0].clamp(-10_000.0, 10_000.0);
            target_pos.0[1] = target_pos.0[1].clamp(-1_000.0, 10_000.0);
            target_pos.0[2] = target_pos.0[2].clamp(-10_000.0, 10_000.0);
        }
        Generator::Construct { root } => {
            let mut count: u32 = 0;
            sanitize_prim_node(root, 0, &mut count);
        }
        Generator::Water { .. } | Generator::Unknown => {}
    }
}

pub(crate) fn sanitize_terrain_cfg(cfg: &mut SovereignTerrainConfig) {
    cfg.grid_size = cfg.grid_size.clamp(2, limits::MAX_GRID_SIZE);
    cfg.octaves = cfg.octaves.clamp(1, limits::MAX_OCTAVES);
    cfg.voronoi_num_seeds = cfg.voronoi_num_seeds.clamp(1, limits::MAX_VORONOI_SEEDS);
    cfg.voronoi_num_terraces = cfg
        .voronoi_num_terraces
        .clamp(1, limits::MAX_VORONOI_TERRACES);
    cfg.erosion_drops = cfg.erosion_drops.min(limits::MAX_EROSION_DROPS);
    cfg.thermal_iterations = cfg.thermal_iterations.min(limits::MAX_THERMAL_ITERATIONS);
    cfg.material.texture_size = cfg
        .material
        .texture_size
        .clamp(16, limits::MAX_TEXTURE_SIZE);
    // Cap per-variant octave-like fields so a forward-compat peer cannot
    // weaponise texture-size × octave blowups.
    for layer in cfg.material.layers.iter_mut() {
        sanitize_texture_config(layer);
    }
}
