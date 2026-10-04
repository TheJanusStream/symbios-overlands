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
//! Per-domain impls live in the sibling modules - [`transform`],
//! [`material`], [`terrain`], [`water`], [`sign`], [`particles`],
//! [`primitive`], and [`contact_effects`] - with [`common`] holding
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
pub(crate) use sign::{is_fetchable_endpoint, is_fetchable_reference, refusal_reason};
mod terrain;
mod transform;
mod water;

use crate::pds::generator::{Generator, GeneratorKind};
use crate::pds::types::truncate_on_char_boundary;

use primitive::sanitize_primitive;
use sign::sanitize_sign;
use water::sanitize_water;

/// In-place numeric clamp for a record-bearing type. Implementors live
/// in the sibling modules of this folder, keyed to the type they
/// sanitise - `TransformData`, `SovereignMaterialSettings`,
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
///   *A Terrain root MAY have children* - that's the "region blueprint"
///   shape, where the terrain root anchors a tree of L-systems / portals /
///   props that travel together.
/// * **Water is child-only.** Every Water volume must inherit a parent
///   (typically a Terrain ancestor) so its world-space surface is
///   well-defined. A root Water is overwritten with a default cuboid -
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
    // Forward to the asset-class sanitiser - caps embedded patch /
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

    // Water is a leaf - `spawn_water_volume` does not consume children, so
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
/// not touch the wrapping [`Generator`]'s transform or children - those are
/// handled by [`sanitize_generator_node`] which calls this on every node.
pub fn sanitize_kind(kind: &mut GeneratorKind) {
    sanitize_kind_with(kind, limits::MAX_PRIM_DIM_M);
}

/// [`sanitize_kind`] with an explicit per-dimension ceiling (#1221 f327) -
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
            // the upstream identifier cap are dropped - they could never
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
/// The child-of-Terrain placement constraint is enforced structurally - only a
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
    // Lot area (#1555): at least twice symbios-tensor's 50 m2 discard floor,
    // so a split always leaves pieces it keeps, and at most a hectare, past
    // which no block of a real street plan is split at all.
    c.lots.lot_area.0 = clamp_finite(
        c.lots.lot_area.0,
        100.0,
        10_000.0,
        crate::pds::generator::LotSettings::DEFAULT_LOT_AREA,
    );
    // The core (#1555): finite, and on the map - a point past any room's
    // edge ranks nothing it could mean.
    if let Some(core) = &mut c.lots.focus {
        for axis in &mut core.0 {
            *axis = clamp_finite(*axis, -1024.0, 1024.0, 0.0);
        }
    }
    truncate_on_char_boundary(
        &mut c.lots.theme_override,
        limits::MAX_LOT_THEME_OVERRIDE_BYTES,
    );
    // Socio overrides (#1555): an armed one to finite unit range, at the
    // neutral value the lot layer itself reads a non-finite one as; `None`
    // (the room's own scene) is left alone.
    use crate::pds::generator::LotSettings;
    if let Some(p) = &mut c.lots.prosperity {
        p.0 = clamp_finite(p.0, 0.0, 1.0, LotSettings::NEUTRAL_PROSPERITY);
    }
    if let Some(e) = &mut c.lots.escalation {
        e.0 = clamp_finite(e.0, 0.0, 1.0, LotSettings::NEUTRAL_ESCALATION);
    }
    // Furniture (#893): spacing floor keeps a hostile record from planting
    // a prop every half-metre down every street.
    c.furniture.spacing.0 = clamp_finite(c.furniture.spacing.0, 8.0, 200.0, 30.0);
    // The street field (#1556): every value the trace hands symbios-tensor
    // finite and inside what it accepts, and both lists bounded, so a
    // hostile record can neither make the tracer refuse the whole network
    // nor have it sum a thousand fields at every step of every street.
    sanitize_road_field(&mut c.field);
    // The layout revision (#1558): one from a newer client reads as the
    // latest this build derives - its streets and lots are grown by this
    // build's latest tidy, its re-mesh and lot fingerprint key on that, and
    // a save from here writes it back as that revision.
    //
    // The clamp is kept on purpose, though the derivation would not need it
    // (`tidies_layout` is `>= 1`). A record's revision must say what its
    // saved buildings were grown with, so an older client that regrows them
    // writes the revision it grew them by: a silent downgrade of the field,
    // and the consistent one. Passing an unknown revision through instead
    // would save buildings grown by this build's tidy under a number that
    // promises a newer one, and a newer client reading it back would take
    // them for its own.
    c.layout_revision = c
        .layout_revision
        .min(crate::pds::generator::RoadConfig::LATEST_LAYOUT);
}

/// Clamp a road network's street field (#1556) so the trace always accepts
/// it. symbios-tensor's `TensorFieldConfig::validate` refuses a non-finite
/// value, a negative smoothing, terrain weight or strength, and a radius
/// that is not positive, and `generate_roads` refuses a keep-out disc
/// likewise - and a refused field takes the whole network with it, streets,
/// lots and all. Centres stay on any room's map, a grid's bearing is folded
/// into `[0, 180)`, and the lists are cut to their caps. A basis field of a
/// kind from a newer client is kept as read - it cannot be written back
/// (#1111), and the trace ignores it - but it counts toward the cap.
fn sanitize_road_field(field: &mut crate::pds::generator::RoadField) {
    use crate::pds::generator::{RoadBasis, RoadField, RoadKeepOut};
    use common::clamp_finite;
    let clamp = |v: &mut crate::pds::Fp, range: std::ops::RangeInclusive<f32>, fallback: f32| {
        v.0 = clamp_finite(v.0, *range.start(), *range.end(), fallback);
    };
    let clamp_center = |c: &mut crate::pds::Fp2| {
        for axis in &mut c.0 {
            *axis = clamp_finite(
                *axis,
                -RoadField::CENTER_LIMIT_M,
                RoadField::CENTER_LIMIT_M,
                0.0,
            );
        }
    };
    clamp(&mut field.smoothing, RoadField::SMOOTHING_M, 0.0);
    clamp(&mut field.terrain_weight, RoadField::TERRAIN_WEIGHT, 1.0);
    field.basis.truncate(RoadField::MAX_BASIS);
    for basis in &mut field.basis {
        match basis {
            RoadBasis::Ring {
                center,
                radius,
                strength,
            } => {
                clamp_center(center);
                clamp(radius, RoadField::BASIS_RADIUS_M, RoadBasis::DEFAULT_RADIUS);
                clamp(
                    strength,
                    RoadField::BASIS_STRENGTH,
                    RoadBasis::DEFAULT_STRENGTH,
                );
            }
            RoadBasis::Grid {
                center,
                bearing,
                radius,
                strength,
            } => {
                clamp_center(center);
                bearing.0 = RoadBasis::canonical_bearing(bearing.0);
                clamp(radius, RoadField::BASIS_RADIUS_M, RoadBasis::DEFAULT_RADIUS);
                clamp(
                    strength,
                    RoadField::BASIS_STRENGTH,
                    RoadBasis::DEFAULT_STRENGTH,
                );
            }
            RoadBasis::Unknown => {}
        }
    }
    field.keep_out.truncate(RoadField::MAX_KEEP_OUT);
    for disc in &mut field.keep_out {
        clamp_center(&mut disc.center);
        clamp(
            &mut disc.radius,
            RoadField::KEEP_OUT_RADIUS_M,
            RoadKeepOut::DEFAULT_RADIUS,
        );
    }
}

/// Clamp a whole [`Generator`] tree (root + descendants) in place. Shared
/// by [`crate::pds::room::RoomRecord::sanitize`] and
/// [`crate::pds::inventory::InventoryRecord::sanitize`] so the per-variant
/// bounds - and the depth / total-node budgets - stay identical between
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
/// space - meaningless on a vehicle. Portal would let an avatar carry
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
    // can do TO them (#1221 f327) - hence the tighter per-dimension cap and
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
/// how the composition worked in the first place - which is why this walks
/// top-down and carries the product forward.
fn clamp_accumulated_scale(node: &mut Generator, carried: f32) {
    let s = node.transform.scale.0;
    let local = s[0].abs().max(s[1].abs()).max(s[2].abs());
    let mut here = carried * local;
    if here > limits::MAX_AVATAR_SCALE_PRODUCT {
        // Scale this node down by exactly the overage, preserving its
        // per-axis proportions - a body clamped to a cube would be a
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
/// was `per-node scale ^ depth` - up to `1000 ^ 16` at the sanitiser's own
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
mod road_lot_override_tests {
    use super::*;
    use crate::pds::generator::{LotSettings, RoadConfig};
    use crate::pds::types::Fp;

    fn sanitised(prosperity: Option<f32>, escalation: Option<f32>) -> LotSettings {
        let mut road = RoadConfig::default();
        road.lots.prosperity = prosperity.map(Fp);
        road.lots.escalation = escalation.map(Fp);
        let mut generator = Generator {
            kind: GeneratorKind::RoadNetwork(road),
            ..Default::default()
        };
        sanitize_generator(&mut generator);
        let GeneratorKind::RoadNetwork(road) = generator.kind else {
            panic!("the sanitiser keeps the variant");
        };
        road.lots
    }

    /// #1555: an armed socio override is clamped to finite unit range, a
    /// non-finite one to the neutral value the lot layer itself reads it
    /// as, and an unarmed one stays `None`, the room's own scene.
    #[test]
    fn the_lot_overrides_are_clamped_to_finite_unit_range() {
        let high = sanitised(Some(1.7), Some(3.0));
        assert_eq!(high.prosperity, Some(Fp(1.0)));
        assert_eq!(high.escalation, Some(Fp(1.0)));
        let low = sanitised(Some(-0.4), Some(-2.0));
        assert_eq!(low.prosperity, Some(Fp(0.0)));
        assert_eq!(low.escalation, Some(Fp(0.0)));
        let broken = sanitised(Some(f32::NAN), Some(f32::NEG_INFINITY));
        assert_eq!(broken.prosperity, Some(Fp(LotSettings::NEUTRAL_PROSPERITY)));
        assert_eq!(broken.escalation, Some(Fp(LotSettings::NEUTRAL_ESCALATION)));
        let inside = sanitised(Some(0.9), Some(0.25));
        assert_eq!(inside.prosperity, Some(Fp(0.9)));
        assert_eq!(inside.escalation, Some(Fp(0.25)));
        let unarmed = sanitised(None, None);
        assert_eq!(unarmed.prosperity, None);
        assert_eq!(unarmed.escalation, None);
    }

    /// #1555: a lot area is held to a real lot - at least twice
    /// symbios-tensor's 50 m2 discard floor, at most a hectare - and a
    /// non-finite one reads as the default split.
    #[test]
    fn the_lot_area_is_clamped_to_a_real_lot() {
        let area = |v: f32| {
            let mut road = RoadConfig::default();
            road.lots.lot_area = Fp(v);
            let mut generator = Generator {
                kind: GeneratorKind::RoadNetwork(road),
                ..Default::default()
            };
            sanitize_generator(&mut generator);
            let GeneratorKind::RoadNetwork(road) = generator.kind else {
                panic!("the sanitiser keeps the variant");
            };
            road.lots.lot_area.0
        };
        let mut road = RoadConfig::default();
        road.lots.focus = Some(crate::pds::types::Fp2([f32::NAN, 5000.0]));
        let mut generator = Generator {
            kind: GeneratorKind::RoadNetwork(road),
            ..Default::default()
        };
        sanitize_generator(&mut generator);
        let GeneratorKind::RoadNetwork(road) = generator.kind else {
            panic!("the sanitiser keeps the variant");
        };
        assert_eq!(
            road.lots.focus,
            Some(crate::pds::types::Fp2([0.0, 1024.0])),
            "a core is finite and on the map"
        );
        assert_eq!(area(5.0), 100.0);
        assert_eq!(area(1.0e9), 10_000.0);
        assert_eq!(area(f32::NAN), LotSettings::DEFAULT_LOT_AREA);
        assert_eq!(area(2400.0), 2400.0);
    }
}

#[cfg(test)]
mod layout_revision_tests {
    use crate::pds::Generator;
    use crate::pds::generator::{GeneratorKind, RoadConfig};

    /// #1558: a layout revision from a newer client reads as the latest this
    /// build derives; one it knows is kept.
    #[test]
    fn a_newer_layout_revision_reads_as_the_latest_this_build_knows() {
        let sanitised = |layout_revision: u32| {
            let mut generator = Generator::from_kind(GeneratorKind::RoadNetwork(RoadConfig {
                layout_revision,
                ..RoadConfig::default()
            }));
            super::sanitize_generator(&mut generator);
            let GeneratorKind::RoadNetwork(road) = generator.kind else {
                panic!("still a road network");
            };
            road.layout_revision
        };
        assert_eq!(sanitised(0), 0);
        assert_eq!(
            sanitised(RoadConfig::LATEST_LAYOUT),
            RoadConfig::LATEST_LAYOUT
        );
        assert_eq!(
            sanitised(RoadConfig::LATEST_LAYOUT + 1),
            RoadConfig::LATEST_LAYOUT
        );
        assert_eq!(sanitised(u32::MAX), RoadConfig::LATEST_LAYOUT);
    }
}

#[cfg(test)]
mod road_field_tests {
    use super::*;
    use crate::pds::generator::{RoadBasis, RoadConfig, RoadField, RoadKeepOut};
    use crate::pds::types::{Fp, Fp2};

    fn sanitised(field: RoadField) -> RoadField {
        let mut generator = Generator {
            kind: GeneratorKind::RoadNetwork(RoadConfig {
                field,
                ..RoadConfig::default()
            }),
            ..Default::default()
        };
        sanitize_generator(&mut generator);
        let GeneratorKind::RoadNetwork(road) = generator.kind else {
            panic!("the sanitiser keeps the variant");
        };
        road.field
    }

    fn ring(center: [f32; 2], radius: f32, strength: f32) -> RoadBasis {
        RoadBasis::Ring {
            center: Fp2(center),
            radius: Fp(radius),
            strength: Fp(strength),
        }
    }

    fn grid(center: [f32; 2], bearing: f32, radius: f32, strength: f32) -> RoadBasis {
        RoadBasis::Grid {
            center: Fp2(center),
            bearing: Fp(bearing),
            radius: Fp(radius),
            strength: Fp(strength),
        }
    }

    fn disc(center: [f32; 2], radius: f32) -> RoadKeepOut {
        RoadKeepOut {
            center: Fp2(center),
            radius: Fp(radius),
        }
    }

    /// #1556: the street field is held to what the tracer takes - radii
    /// 5-1024 m (discs 2-512 m), strengths 0-10, smoothing 0-100 m, terrain
    /// weight 0-10, centres finite and within 1024 m of the origin, a grid's
    /// bearing folded into `[0, 180)` - with a non-finite value at its
    /// default, at most 8 basis fields and 16 discs, a kind from a newer
    /// client kept as read, and a field already inside its bounds untouched.
    #[test]
    fn the_street_field_is_clamped_to_what_the_tracer_takes() {
        let scalars = |smoothing: f32, terrain_weight: f32| {
            let f = sanitised(RoadField {
                smoothing: Fp(smoothing),
                terrain_weight: Fp(terrain_weight),
                ..RoadField::default()
            });
            (f.smoothing.0, f.terrain_weight.0)
        };
        assert_eq!(scalars(-5.0, -1.0), (0.0, 0.0));
        assert_eq!(scalars(1.0e9, 50.0), (100.0, 10.0));
        assert_eq!(scalars(f32::NAN, f32::INFINITY), (0.0, 1.0));
        assert_eq!(scalars(12.5, 0.25), (12.5, 0.25));

        let mut basis = vec![
            ring([f32::NAN, 5000.0], 0.0, -1.0),
            ring([f32::NEG_INFINITY, -5000.0], 1.0e6, f32::INFINITY),
            grid([12.5, -7.0], -30.0, f32::NAN, 50.0),
            grid([0.0, 0.0], 540.0, 3.0, 2.0),
            grid([0.0, 0.0], 190.0, 120.0, 1.0),
            grid([0.0, 0.0], f32::NAN, 120.0, 1.0),
            grid([0.0, 0.0], -1.0e-8, 120.0, 1.0),
            RoadBasis::Unknown,
        ];
        basis.extend(std::iter::repeat_n(ring([1.0, 2.0], 50.0, 1.0), 100));
        let clean = sanitised(RoadField {
            basis,
            ..RoadField::default()
        });
        assert_eq!(
            clean.basis,
            vec![
                ring([0.0, 1024.0], 5.0, 0.0),
                ring([0.0, -1024.0], 1024.0, RoadBasis::DEFAULT_STRENGTH),
                grid([12.5, -7.0], 150.0, RoadBasis::DEFAULT_RADIUS, 10.0),
                grid([0.0, 0.0], 0.0, 5.0, 2.0),
                grid([0.0, 0.0], 10.0, 120.0, 1.0),
                grid([0.0, 0.0], 0.0, 120.0, 1.0),
                grid([0.0, 0.0], 0.0, 120.0, 1.0),
                RoadBasis::Unknown,
            ],
            "cut to the first {} and each held to its bounds",
            RoadField::MAX_BASIS
        );

        let mut keep_out = vec![
            disc([f32::NAN, 2000.0], 0.0),
            disc([-2000.0, f32::INFINITY], 1.0e6),
            disc([3.0, 4.0], f32::NAN),
            disc([-12.5, 30.0], 45.5),
        ];
        keep_out.extend(std::iter::repeat_n(disc([1.0, 2.0], 10.0), 100));
        let clean = sanitised(RoadField {
            keep_out,
            ..RoadField::default()
        });
        assert_eq!(clean.keep_out.len(), RoadField::MAX_KEEP_OUT);
        assert_eq!(
            clean.keep_out[..4],
            [
                disc([0.0, 1024.0], 2.0),
                disc([-1024.0, 0.0], 512.0),
                disc([3.0, 4.0], RoadKeepOut::DEFAULT_RADIUS),
                disc([-12.5, 30.0], 45.5),
            ]
        );

        let inside = RoadField {
            smoothing: Fp(30.0),
            terrain_weight: Fp(0.5),
            basis: vec![
                ring([90.0, -10.0], 200.0, 1.5),
                grid([5.0, 5.0], 37.5, 80.0, 0.25),
            ],
            keep_out: vec![disc([40.0, 60.0], 25.0)],
        };
        assert_eq!(
            sanitised(inside.clone()),
            inside,
            "a sane field is left alone"
        );
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
    /// it. If this fails, content has grown past the cap - raise the cap
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
    /// with flat colour - while the only remedy, Mute, could not be aimed
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
            panic!("the kind should survive - the sanitiser clamps, it does not reject");
        };
        for axis in size.0 {
            assert!(axis <= limits::MAX_AVATAR_PRIM_DIM_M, "dimension {axis}");
        }
    }

    /// The exponent is the mechanism: scales compose down the hierarchy and
    /// nothing bounded the product, so sixteen nested nodes at a permitted
    /// per-node scale were `4^16` - over four billion - not 4.
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
