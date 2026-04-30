//! Record sanitisation: clamp every numeric field a malicious peer might
//! inflate to crash the engine or exhaust host RAM. The limits mirror the
//! ranges the World Editor UI already exposes, so a hand-crafted record
//! cannot trigger behaviour the owner couldn't have requested via the
//! normal interface.
//!
//! Each path that accepts a `RoomRecord`/`AvatarRecord`/`InventoryRecord`
//! from the network calls its `sanitize()` method before handing the record
//! to the world compiler; those impls live alongside the record types and
//! delegate into the per-domain helpers defined here.
//!
//! The shared [`Sanitize`] trait factors out the "this type knows how to
//! clamp itself in place" capability so callers can write
//! `material.sanitize()` rather than `sanitize_material_settings(material)`.
//! Per-domain impls live in the sibling modules ([`transform`],
//! [`material`], [`terrain`], [`water`], [`sign`]); the [`GeneratorKind`]
//! variants with inline fields go through the [`sanitize_kind`] dispatcher
//! defined here, since they don't have separate parameter structs to hang
//! the trait off.

mod common;
pub mod limits;
mod material;
mod particles;
mod primitive;
mod sign;
mod terrain;
mod transform;
mod water;

use crate::pds::generator::{Generator, GeneratorKind};
use crate::pds::types::truncate_on_char_boundary;

use particles::sanitize_particles;
use primitive::sanitize_primitive;
use sign::sanitize_sign;
use water::sanitize_water;

/// In-place numeric clamp for a record-bearing type. Implementors live
/// in the sibling modules of this folder, keyed to the type they
/// sanitise — `TransformData`, `SovereignMaterialSettings`,
/// `SovereignTextureConfig`, `SovereignTerrainConfig`, `WaterSurface`,
/// `SignSource`. The `GeneratorKind` open union goes through
/// [`sanitize_kind`] instead because its variants carry inline fields
/// rather than separate parameter structs.
pub(crate) trait Sanitize {
    fn sanitize(&mut self);
}

/// Recursively clamp a [`Generator`] tree. Beyond the depth and total-node
/// budgets (see [`limits::MAX_GENERATOR_DEPTH`] and
/// [`limits::MAX_GENERATOR_NODES`]), each node's transform and kind are
/// clamped so a malicious record can't pass NaN/negative scales to Bevy's
/// primitive mesh constructors or the Avian collider builders.
///
/// **Strict positional rules.**
///
/// * **Terrain is root-only.** The terrain plugin owns the world's
///   heightmap; allowing a Terrain in a child slot would either spawn a
///   second heightfield collider (Avian forbids that) or be silently
///   ignored. A non-root Terrain is overwritten with a default cuboid.
///   *A Terrain root MAY have children* — that's the "region blueprint"
///   shape, where the terrain root anchors a tree of L-systems / portals /
///   props that travel together.
/// * **Water is child-only.** Every Water volume must inherit a parent
///   (typically a Terrain ancestor) so its world-space surface is
///   well-defined. A root Water is overwritten with a default cuboid —
///   `RoomRecord::default_for_did` puts water inside the terrain root, and
///   inventory-saved water should always be a child of the region it
///   belongs to. Water itself is a leaf (its `children` list is cleared).
fn sanitize_generator_node(node: &mut Generator, depth: u32, count: &mut u32, is_root: bool) {
    *count += 1;
    node.transform.sanitize();

    if !is_root && matches!(&node.kind, GeneratorKind::Terrain(_)) {
        // Terrain at non-root: not a valid position. Overwrite rather than
        // reject so the node still round-trips and the owner can fix it.
        node.kind = GeneratorKind::default_cuboid();
    }
    if is_root && matches!(&node.kind, GeneratorKind::Water { .. }) {
        // Water at the root of a named generator: not a valid position.
        // Water needs an ancestor whose transform anchors the volume.
        node.kind = GeneratorKind::default_cuboid();
    }

    sanitize_kind(&mut node.kind);

    // Water is a leaf — `spawn_water_volume` does not consume children, so
    // strip authored children to keep the editor and spawner in sync.
    if matches!(&node.kind, GeneratorKind::Water { .. }) {
        node.children.clear();
        return;
    }

    if depth >= limits::MAX_GENERATOR_DEPTH || *count >= limits::MAX_GENERATOR_NODES {
        node.children.clear();
        return;
    }
    // Drop the tail children whose recursion budget we couldn't afford so
    // the survivor count matches the spawn budget exactly.
    let mut visited = 0usize;
    for (i, child) in node.children.iter_mut().enumerate() {
        if *count >= limits::MAX_GENERATOR_NODES {
            break;
        }
        sanitize_generator_node(child, depth + 1, count, false);
        visited = i + 1;
    }
    if visited < node.children.len() {
        node.children.truncate(visited);
    }
}

/// Clamp the variant-specific payload of a [`GeneratorKind`] in place. Does
/// not touch the wrapping [`Generator`]'s transform or children — those are
/// handled by [`sanitize_generator_node`] which calls this on every node.
pub fn sanitize_kind(kind: &mut GeneratorKind) {
    match kind {
        GeneratorKind::Terrain(cfg) => cfg.sanitize(),
        GeneratorKind::LSystem {
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
                settings.sanitize();
            }
        }
        GeneratorKind::Shape {
            grammar_source,
            root_rule,
            footprint,
            materials,
            ..
        } => {
            truncate_on_char_boundary(grammar_source, limits::MAX_SHAPE_SOURCE_BYTES);
            truncate_on_char_boundary(root_rule, limits::MAX_SHAPE_ROOT_RULE_BYTES);
            // Clamp each footprint axis to a finite, non-negative range. Y is
            // allowed to be 0.0 because most grammars `Extrude` from a flat
            // 2-D plot; the others must stay positive so the interpreter's
            // split / repeat math doesn't divide by zero.
            footprint.0[0] =
                common::clamp_finite(footprint.0[0], 0.001, limits::MAX_SHAPE_FOOTPRINT, 10.0);
            footprint.0[1] =
                common::clamp_finite(footprint.0[1], 0.0, limits::MAX_SHAPE_FOOTPRINT, 0.0);
            footprint.0[2] =
                common::clamp_finite(footprint.0[2], 0.001, limits::MAX_SHAPE_FOOTPRINT, 10.0);
            // Cap the slot count first so the per-slot sanitiser doesn't
            // walk an attacker-supplied million-entry map. Slot keys above
            // the upstream identifier cap are dropped — they could never
            // match an emitted `Mat("...")` anyway.
            if materials.len() > limits::MAX_SHAPE_MATERIAL_SLOTS {
                let mut keys: Vec<String> = materials.keys().cloned().collect();
                keys.sort();
                for k in keys.into_iter().skip(limits::MAX_SHAPE_MATERIAL_SLOTS) {
                    materials.remove(&k);
                }
            }
            materials.retain(|k, _| k.len() <= limits::MAX_SHAPE_ROOT_RULE_BYTES);
            for settings in materials.values_mut() {
                settings.sanitize();
            }
        }
        GeneratorKind::Portal {
            target_did,
            target_pos,
        } => {
            truncate_on_char_boundary(target_did, 256);
            target_pos.0[0] = target_pos.0[0].clamp(-10_000.0, 10_000.0);
            target_pos.0[1] = target_pos.0[1].clamp(-1_000.0, 10_000.0);
            target_pos.0[2] = target_pos.0[2].clamp(-10_000.0, 10_000.0);
        }
        GeneratorKind::Cuboid { .. }
        | GeneratorKind::Sphere { .. }
        | GeneratorKind::Cylinder { .. }
        | GeneratorKind::Capsule { .. }
        | GeneratorKind::Cone { .. }
        | GeneratorKind::Torus { .. }
        | GeneratorKind::Plane { .. }
        | GeneratorKind::Tetrahedron { .. } => sanitize_primitive(kind),
        GeneratorKind::Water {
            level_offset,
            surface,
        } => sanitize_water(level_offset, surface),
        GeneratorKind::Sign {
            source,
            size,
            uv_repeat,
            uv_offset,
            material,
            alpha_mode,
            ..
        } => sanitize_sign(source, size, uv_repeat, uv_offset, material, alpha_mode),
        GeneratorKind::ParticleSystem {
            emitter_shape,
            rate_per_second,
            burst_count,
            max_particles,
            duration,
            lifetime_min,
            lifetime_max,
            speed_min,
            speed_max,
            gravity_multiplier,
            acceleration,
            linear_drag,
            start_size,
            end_size,
            start_color,
            end_color,
            inherit_velocity,
            bounce,
            friction,
            texture,
            texture_atlas,
            frame_mode,
            ..
        } => sanitize_particles(
            emitter_shape,
            rate_per_second,
            burst_count,
            max_particles,
            duration,
            lifetime_min,
            lifetime_max,
            speed_min,
            speed_max,
            gravity_multiplier,
            acceleration,
            linear_drag,
            start_size,
            end_size,
            start_color,
            end_color,
            inherit_velocity,
            bounce,
            friction,
            texture,
            texture_atlas,
            frame_mode,
        ),
        GeneratorKind::Unknown => {}
    }
}

/// Clamp a whole [`Generator`] tree (root + descendants) in place. Shared
/// by [`crate::pds::room::RoomRecord::sanitize`] and
/// [`crate::pds::inventory::InventoryRecord::sanitize`] so the per-variant
/// bounds — and the depth / total-node budgets — stay identical between
/// the room recipe and the inventory stash.
pub fn sanitize_generator(generator: &mut Generator) {
    let mut count: u32 = 0;
    sanitize_generator_node(generator, 0, &mut count, true);
}

/// Avatar-specific sanitiser. Reuses [`sanitize_generator_node`]'s
/// depth, total-node, and per-kind clamps, then walks the tree and
/// rewrites every kind that is forbidden inside an avatar's visual
/// subtree (Terrain, Water, Portal) into a default cuboid.
///
/// Terrain / Water / Portal are excluded by design. Terrain owns the
/// world heightmap; allowing it inside an avatar would either spawn a
/// second heightfield collider (Avian forbids) or be silently ignored.
/// Water needs an ancestor whose transform anchors the volume in world
/// space — meaningless on a vehicle. Portal would let an avatar carry
/// a moving travel target into another peer's space, which is both
/// abusive (drag a stranger through your portal) and confusing (the
/// portal moves with the player).
///
/// Primitives + LSystem + Shape all round-trip; the avatar spawn path
/// (`world_builder::avatar_spawn::spawn_avatar_visuals_subtree`)
/// reuses the same dispatcher as the room compiler with the room-only
/// behaviours (RoomEntity, PrimMarker, per-prim colliders) suppressed.
pub fn sanitize_avatar_visuals(generator: &mut Generator) {
    sanitize_generator(generator);
    enforce_avatar_kinds(generator);
}

fn enforce_avatar_kinds(node: &mut Generator) {
    if matches!(
        &node.kind,
        GeneratorKind::Terrain(_) | GeneratorKind::Water { .. } | GeneratorKind::Portal { .. }
    ) {
        node.kind = GeneratorKind::default_cuboid();
    }
    for child in node.children.iter_mut() {
        enforce_avatar_kinds(child);
    }
}
