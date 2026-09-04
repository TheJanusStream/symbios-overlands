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
//! Per-domain impls live in the sibling modules — [`transform`],
//! [`material`], [`terrain`], [`water`], [`sign`], [`particles`],
//! [`primitive`], and [`contact_effects`] — with [`common`] holding
//! shared clamp helpers (NaN-finite checks, allow-list filters) and
//! [`limits`] holding every numeric bound as a `pub const` so callers
//! and tests can share the envelope. The [`GeneratorKind`] variants
//! with inline fields go through the [`sanitize_kind`] dispatcher
//! defined here, since they don't have separate parameter structs to
//! hang the trait off.

mod audio;
pub use audio::MAX_INSTRUMENT_ID_BYTES;
mod common;
mod contact_effects;
pub mod limits;
mod material;
pub mod names;
mod particles;
mod primitive;
mod sign;
pub(crate) use sign::is_fetchable_endpoint;
mod terrain;
mod transform;
mod water;

use crate::pds::generator::{Generator, GeneratorKind};
use crate::pds::types::truncate_on_char_boundary;

/// Authored-rotation fixpoint for the avatar part builders (see
/// `common::unit_quat_fixpoint`).
pub(crate) use common::unit_quat_fixpoint;
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
fn sanitize_generator_node(
    node: &mut Generator,
    depth: u32,
    count: &mut u32,
    is_root: bool,
    max_dim: f32,
) {
    *count += 1;
    node.transform.sanitize();
    // Forward to the asset-class sanitiser — caps embedded patch /
    // sequence JSON length and any Referenced URL/DID/CID strings on
    // the node's spatial-audio source.
    node.audio.sanitize();

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

    sanitize_kind_with(&mut node.kind, max_dim);

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
        sanitize_generator_node(child, depth + 1, count, false, max_dim);
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
    sanitize_kind_with(kind, limits::MAX_PRIM_DIM_M);
}

/// [`sanitize_kind`] with an explicit per-dimension ceiling (#1221 f327) —
/// room content passes [`limits::MAX_PRIM_DIM_M`], an avatar the tighter
/// [`limits::MAX_AVATAR_PRIM_DIM_M`].
pub fn sanitize_kind_with(kind: &mut GeneratorKind, max_dim: f32) {
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
            round_meshes,
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
            // Round-mesh ids follow the same discipline as material slots:
            // an id longer than the upstream identifier cap could never
            // match an emitted `I("...")`, and the list is deduped and
            // capped so a hostile record cannot blow the budget with
            // near-duplicate entries.
            round_meshes
                .retain(|id| !id.is_empty() && id.len() <= limits::MAX_SHAPE_ROOT_RULE_BYTES);
            round_meshes.sort();
            round_meshes.dedup();
            round_meshes.truncate(limits::MAX_SHAPE_MATERIAL_SLOTS);
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
        GeneratorKind::Gateway { size } => {
            for axis in size.0.iter_mut() {
                *axis = common::clamp_finite(*axis, 0.25, 50.0, 2.5);
            }
        }
        crate::for_each_primitive!(pattern {}) => sanitize_primitive(kind, max_dim),
        GeneratorKind::Water { surface } => sanitize_water(surface),
        GeneratorKind::RoadNetwork(config) => sanitize_road(config),
        GeneratorKind::Sign {
            source,
            size,
            uv_repeat,
            uv_offset,
            material,
            alpha_mode,
            ..
        } => sanitize_sign(source, size, uv_repeat, uv_offset, material, alpha_mode),
        GeneratorKind::ParticleSystem(params) => params.sanitize(),
        GeneratorKind::Unknown => {}
    }
}

/// Clamp a [`crate::pds::generator::RoadConfig`] to finite, sane ranges so a hostile or malformed
/// record can't feed the road builder a NaN extent or a million-metre skirt.
/// The child-of-Terrain placement constraint is enforced structurally — only a
/// Terrain child's road config is ever read (see [`crate::terrain`]).
fn sanitize_road(c: &mut crate::pds::generator::RoadConfig) {
    use common::clamp_finite;
    c.district_half_extent.0 = clamp_finite(c.district_half_extent.0, 10.0, 512.0, 170.0);
    for axis in c.center.0.iter_mut() {
        *axis = clamp_finite(*axis, -512.0, 512.0, 0.0);
    }
    c.major_spacing.0 = clamp_finite(c.major_spacing.0, 10.0, 500.0, 95.0);
    c.minor_spacing.0 = clamp_finite(c.minor_spacing.0, 8.0, 400.0, 55.0);
    c.major_half_width.0 = clamp_finite(c.major_half_width.0, 0.5, 20.0, 3.5);
    c.minor_half_width.0 = clamp_finite(c.minor_half_width.0, 0.5, 20.0, 2.0);
    c.curb_height.0 = clamp_finite(c.curb_height.0, 0.0, 2.0, 0.18);
    c.curb_top_width.0 = clamp_finite(c.curb_top_width.0, 0.0, 5.0, 0.22);
    c.chamfer_width.0 = clamp_finite(c.chamfer_width.0, 0.0, 5.0, 0.4);
    c.skirt_depth.0 = clamp_finite(c.skirt_depth.0, 0.5, 50.0, 5.0);
    // Appearance overrides (#891): colours to unit range, strength bounded so
    // a hostile record can't bloom the whole frame white.
    for color in [
        &mut c.appearance.deck_color,
        &mut c.appearance.structure_color,
        &mut c.appearance.neon_color,
    ]
    .into_iter()
    .flatten()
    {
        for ch in color.0.iter_mut() {
            *ch = clamp_finite(*ch, 0.0, 1.0, 0.5);
        }
    }
    if let Some(r) = &mut c.appearance.deck_roughness {
        r.0 = clamp_finite(r.0, 0.0, 1.0, 0.22);
    }
    if let Some(s) = &mut c.appearance.neon_strength {
        s.0 = clamp_finite(s.0, 0.0, 20.0, 2.5);
    }
    // Lot knobs (#892): density unit-range, scales positive with
    // min ≤ max (swap-fixed), theme label bounded like other free text.
    c.lots.density.0 = clamp_finite(c.lots.density.0, 0.0, 1.0, 1.0);
    c.lots.scale_min.0 = clamp_finite(c.lots.scale_min.0, 0.1, 5.0, 0.5);
    c.lots.scale_max.0 = clamp_finite(c.lots.scale_max.0, 0.1, 5.0, 2.0);
    if c.lots.scale_min.0 > c.lots.scale_max.0 {
        std::mem::swap(&mut c.lots.scale_min, &mut c.lots.scale_max);
    }
    truncate_on_char_boundary(
        &mut c.lots.theme_override,
        limits::MAX_LOT_THEME_OVERRIDE_BYTES,
    );
    // Furniture (#893): spacing floor keeps a hostile record from planting
    // a prop every half-metre down every street.
    c.furniture.spacing.0 = clamp_finite(c.furniture.spacing.0, 8.0, 200.0, 30.0);
}

/// Clamp a whole [`Generator`] tree (root + descendants) in place. Shared
/// by [`crate::pds::room::RoomRecord::sanitize`] and
/// [`crate::pds::inventory::InventoryRecord::sanitize`] so the per-variant
/// bounds — and the depth / total-node budgets — stay identical between
/// the room recipe and the inventory stash.
pub fn sanitize_generator(generator: &mut Generator) {
    let mut count: u32 = 0;
    sanitize_generator_node(generator, 0, &mut count, true, limits::MAX_PRIM_DIM_M);
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
    let mut count: u32 = 0;
    // A body is worn into other people's rooms, so its size is a thing it
    // can do TO them (#1221 f327) — hence the tighter per-dimension cap and
    // the bound on accumulated scale that room content does not carry.
    sanitize_generator_node(
        generator,
        0,
        &mut count,
        true,
        limits::MAX_AVATAR_PRIM_DIM_M,
    );
    enforce_avatar_kinds(generator);
    clamp_accumulated_scale(generator, 1.0);
}

/// Hold the product of scales along every root-to-leaf path under
/// [`limits::MAX_AVATAR_SCALE_PRODUCT`] (#1221 f327).
///
/// Clamping the node rather than rejecting the record, like the rest of
/// this module: an oversized body degrades to a large-but-bounded one that
/// still round-trips, rather than vanishing with no explanation to its
/// wearer. Clamping an ANCESTOR bounds everything under it, because that is
/// how the composition worked in the first place — which is why this walks
/// top-down and carries the product forward.
fn clamp_accumulated_scale(node: &mut Generator, carried: f32) {
    let s = node.transform.scale.0;
    let local = s[0].abs().max(s[1].abs()).max(s[2].abs());
    let mut here = carried * local;
    if here > limits::MAX_AVATAR_SCALE_PRODUCT {
        // Scale this node down by exactly the overage, preserving its
        // per-axis proportions — a body clamped to a cube would be a
        // stranger defect than the one being fixed.
        let shrink = limits::MAX_AVATAR_SCALE_PRODUCT / here;
        node.transform.scale =
            crate::pds::types::Fp3([s[0] * shrink, s[1] * shrink, s[2] * shrink]);
        here = limits::MAX_AVATAR_SCALE_PRODUCT;
    }
    for child in node.children.iter_mut() {
        clamp_accumulated_scale(child, here);
    }
}

/// The largest product of scales along any root-to-leaf path in `node`.
///
/// Measurement, not policy: scales compose multiplicatively down the
/// hierarchy and nothing bounded the product, so a body's world-space size
/// was `per-node scale ^ depth` — up to `1000 ^ 16` at the sanitiser's own
/// depth limit (#1221 f327). This is what says how much headroom a cap on
/// that product actually has over the bodies the app ships.
pub fn accumulated_scale(node: &Generator) -> f32 {
    fn walk(node: &Generator, carried: f32) -> f32 {
        let s = node.transform.scale.0;
        let here = carried * s[0].abs().max(s[1].abs()).max(s[2].abs());
        node.children
            .iter()
            .map(|child| walk(child, here))
            .fold(here, f32::max)
    }
    walk(node, 1.0)
}

fn enforce_avatar_kinds(node: &mut Generator) {
    // Gateway shares Portal's exclusion rationale: an avatar carrying a
    // travel zone into someone else's space is the same abuse shape.
    if matches!(
        &node.kind,
        GeneratorKind::Terrain(_)
            | GeneratorKind::Water { .. }
            | GeneratorKind::Portal { .. }
            | GeneratorKind::Gateway { .. }
    ) {
        node.kind = GeneratorKind::default_cuboid();
    }
    for child in node.children.iter_mut() {
        enforce_avatar_kinds(child);
    }
}

#[cfg(test)]
mod avatar_extent_tests {
    use super::*;

    /// Every generator body and part this build can produce, for the guard
    /// below and for anyone re-tuning the caps.
    fn shipped_avatar_trees() -> Vec<Generator> {
        let mut out = Vec::new();
        for seed in 0..400u64 {
            let (body, _) = crate::pds::avatar::default_visuals::build_for_seed(seed);
            if let Some(visuals) = body.visuals() {
                out.push(visuals.clone());
            }
        }
        for seed in [0u64, 1, 7, 42, 1337] {
            let ctx = crate::pds::avatar::parts::PartCtx::for_seed(seed);
            out.extend(crate::pds::avatar::parts::entries().map(|part| part.build(&ctx)));
        }
        out
    }

    fn sanitised_with(tree: &Generator, max_dim: f32) -> Generator {
        let mut out = tree.clone();
        let mut count: u32 = 0;
        sanitize_generator_node(&mut out, 0, &mut count, true, max_dim);
        out
    }

    /// #1221 f327. The caps are chosen from MEASUREMENT, and this is the
    /// measurement: no body or part this build ships is changed by them.
    ///
    /// A silent deformation of every existing avatar would be a stranger
    /// defect than the one being fixed, and nothing at build time can see
    /// it. If this fails, content has grown past the cap — raise the cap
    /// deliberately, do not lower the content.
    #[test]
    fn shipped_avatars_are_unchanged_by_the_avatar_caps() {
        let trees = shipped_avatar_trees();
        assert!(trees.len() > 100, "the corpus is the point of this test");
        for tree in &trees {
            assert_eq!(
                sanitised_with(tree, limits::MAX_AVATAR_PRIM_DIM_M),
                sanitised_with(tree, limits::MAX_PRIM_DIM_M),
                "the avatar dimension cap deformed a body this build ships"
            );
            let mut avatar = tree.clone();
            sanitize_avatar_visuals(&mut avatar);
            let mut room = tree.clone();
            sanitize_generator(&mut room);
            enforce_avatar_kinds(&mut room);
            assert_eq!(
                avatar, room,
                "the accumulated-scale clamp moved a body this build ships"
            );
        }
    }

    /// The headline record (#1221 f327): a 100 m cuboid at scale 1000 is a
    /// 100 km cube centred on the wearer, and it filled every guest's view
    /// with flat colour — while the only remedy, Mute, could not be aimed
    /// because no body carries a name.
    #[test]
    fn a_hostile_avatar_record_is_bounded_on_both_factors() {
        let mut hostile = Generator {
            kind: GeneratorKind::Cuboid {
                size: crate::pds::types::Fp3([100.0, 100.0, 100.0]),
                common: Default::default(),
            },
            ..Generator::default()
        };
        hostile.transform.scale = crate::pds::types::Fp3([1000.0, 1000.0, 1000.0]);

        sanitize_avatar_visuals(&mut hostile);

        assert!(
            accumulated_scale(&hostile) <= limits::MAX_AVATAR_SCALE_PRODUCT + 1e-3,
            "accumulated scale {} exceeds the cap",
            accumulated_scale(&hostile),
        );
        let GeneratorKind::Cuboid { size, .. } = &hostile.kind else {
            panic!("the kind should survive — the sanitiser clamps, it does not reject");
        };
        for axis in size.0 {
            assert!(axis <= limits::MAX_AVATAR_PRIM_DIM_M, "dimension {axis}");
        }
    }

    /// The exponent is the mechanism: scales compose down the hierarchy and
    /// nothing bounded the product, so sixteen nested nodes at a permitted
    /// per-node scale were `4^16` — over four billion — not 4.
    #[test]
    fn nested_scales_cannot_multiply_past_the_cap() {
        fn nest(depth: u32) -> Generator {
            let mut node = Generator::default();
            node.transform.scale = crate::pds::types::Fp3([4.0, 4.0, 4.0]);
            if depth > 0 {
                node.children.push(nest(depth - 1));
            }
            node
        }
        let mut tower = nest(limits::MAX_GENERATOR_DEPTH);
        assert!(
            accumulated_scale(&tower) > 1.0e6,
            "the fixture has to be genuinely unbounded to prove anything"
        );

        sanitize_avatar_visuals(&mut tower);

        assert!(
            accumulated_scale(&tower) <= limits::MAX_AVATAR_SCALE_PRODUCT + 1e-3,
            "accumulated scale {} exceeds the cap",
            accumulated_scale(&tower),
        );
    }

    /// A clamp must not turn a body into a cube: the overage is taken out
    /// of all three axes equally, so proportions survive.
    #[test]
    fn clamping_preserves_per_axis_proportions() {
        let mut node = Generator::default();
        node.transform.scale = crate::pds::types::Fp3([100.0, 50.0, 25.0]);
        sanitize_avatar_visuals(&mut node);
        let s = node.transform.scale.0;
        assert!((s[0] / s[1] - 2.0).abs() < 1e-3, "{s:?}");
        assert!((s[1] / s[2] - 2.0).abs() < 1e-3, "{s:?}");
    }

    /// ROOM content is untouched: a 100 m primitive is legitimate in a
    /// world you choose to enter, and the wire fixtures depend on it.
    #[test]
    fn the_room_path_keeps_its_own_ceiling() {
        let mut room = Generator {
            kind: GeneratorKind::Cuboid {
                size: crate::pds::types::Fp3([100.0, 100.0, 100.0]),
                common: Default::default(),
            },
            ..Generator::default()
        };
        sanitize_generator(&mut room);
        let GeneratorKind::Cuboid { size, .. } = &room.kind else {
            panic!("kind survives");
        };
        assert_eq!(size.0, [100.0, 100.0, 100.0]);
    }
}
