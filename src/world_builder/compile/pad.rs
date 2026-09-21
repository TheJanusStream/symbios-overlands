//! Footprint-aware ground resolution for seeded structures (#1008).
//!
//! A tree is a point object: sampling the terrain under its trunk places
//! it correctly by construction. A building is not. Resolving its Y from
//! a single sample at the anchor centre leaves the whole footprint
//! tilted around that one point, so on a hillside the uphill wall sinks
//! into the ground by `slope × radius` while the downhill wall lifts off
//! it by the same amount. At the derive-time `BUILD_SLOPE_LIMIT` of 0.28
//! and a typical 8 m clearance that is ±2.2 m of error, against the
//! 0.35 m foundation bite the record authors.
//!
//! This resolves the ground a *footprint* rests on instead: the highest
//! terrain under the building, sampled over its own disc. Upward-only,
//! for the same reason the road deck takes `max` over its lateral
//! samples rather than a mean (`urban::levelling`) - ground
//! that pokes through a floor is a hole in the building, while ground
//! that falls away beneath one is a gap a plinth can close. It is also
//! how the ground is actually prepared: a graded pad sets finished floor
//! level at the high point of the site and fills below it.
//!
//! Applied only to seeded structures - the `avoid_water` opt-in the
//! settlement and lot derivers set - so hand-authored editor placements
//! keep the plain centre sample they were positioned against.
//!
//! # One sampling site
//!
//! Four places resolve a snapped placement's ground: the compile
//! executor, the gizmo's preview, the gizmo's drag commit, and the
//! editor's snap toggle. They must agree exactly - the offset a drag
//! writes into the record is `dragged world Y − ground`, and the compile
//! then renders at `ground + offset`, so any disagreement between the two
//! reads is baked into the offset and *accumulates on every drag*. Read
//! [`snapped_ground_y`] from all four rather than sampling the heightmap
//! directly; it is the reason this module is public.
//!
//! # Where it stands
//!
//! A seeded structure is also MOVED before it snaps: the compile walks it
//! off water and then off over-steep ground ([`relocate_snapped_anchor`])
//! and reads the ground under the walked anchor, while its record keeps
//! the authored x/z - seed 253's kiosk stands 6 m from its record's spot.
//! So the sites that show an object where the world has it, or keep one
//! there, walk it the same way (#1399): the placement outline and the snap
//! toggle through [`snapped_absolute_anchor`], the executor through the
//! walk itself. The drag commit needs neither - it rebases against the
//! pose the drag started from, which is already the walked one (#1398).

use bevy::math::Vec3;

/// Samples around the footprint rim. A heightfield's maximum over a disc
/// lies either at a grid vertex inside it or somewhere on its rim; the
/// vertex sweep below finds the former exactly, and these bound the
/// latter. Twenty-four puts one every 15°, and the residual between them
/// is second-order because the rim maximum is a turning point.
const RIM_SAMPLES: u32 = 24;

/// Ceiling on the vertex sweep's span, in grid cells per side. Every real
/// footprint is far inside this (the widest catalogue clearance is 54 m
/// against a ~2 m cell, so 54 cells per side); it only bounds the loop if
/// a record ever pairs a huge clearance with a fine terrain grid.
const MAX_SPAN_CELLS: usize = 192;

/// The footprint radius a placement's snap resolves against, or `None`
/// for one that resolves at its centre.
///
/// A seeded structure is marked by `avoid_water` - the opt-in the
/// settlement and lot derivers set - and carries its footprint in
/// `avoid_water_clearance`, scaled by the placement's own scale so a 1.2×
/// landmark measures a 1.2× disc. Everything else (scatter bounds, grid
/// anchors, hand-placed props) resolves at a point.
pub fn snap_footprint_radius(placement: &crate::pds::Placement) -> Option<f32> {
    match placement {
        crate::pds::Placement::Absolute {
            transform,
            avoid_water,
            avoid_water_clearance,
            ..
        } => snap_radius_of(*avoid_water, avoid_water_clearance.0, transform.scale.0[0]),
        _ => None,
    }
}

/// [`snap_footprint_radius`] from the loose fields, for callers that hold
/// a destructured placement rather than the enum.
fn snap_radius_of(avoid_water: bool, clearance: f32, scale_x: f32) -> Option<f32> {
    let r = clearance * scale_x.max(0.0);
    (avoid_water && r > 0.0).then_some(r)
}

/// The ground a snapped placement sits on - the single reading every
/// consumer must use (see the module docs).
///
/// `radius` comes from [`snap_footprint_radius`]; `None` gives the plain
/// centre sample a point-like placement wants.
pub fn snapped_ground_y(
    hm: &bevy_symbios_ground::HeightMap,
    x: f32,
    z: f32,
    radius: Option<f32>,
) -> f32 {
    let extent = (hm.width().saturating_sub(1)) as f32 * hm.scale();
    let half = extent * 0.5;
    footprint_height(hm, extent, half, x, z, radius.unwrap_or(0.0))
}

/// The dry disc a snapped placement's relocation must clear, or `None`
/// for one the compile never relocates.
///
/// Only a placement that opts into Avoid Water - the seeded pipeline's
/// marker - is walked, and its disc scales with the placement's own scale,
/// so a 1.2x landmark demands a 1.2x dry disc. Unlike [`snap_radius_of`],
/// zero is an answer: the walk then checks the anchor's centre alone.
pub(super) fn relocation_clearance(avoid_water: bool, clearance: f32, scale_x: f32) -> Option<f32> {
    avoid_water.then_some(clearance * scale_x.max(0.0))
}

/// Walk a snapped anchor to where the compile stands it: off water, then
/// off over-steep ground (#905), along its bearing through the origin.
/// Moves X/Z only. `clearance` comes from [`relocation_clearance`]; a
/// placement without one is never walked.
pub(super) fn relocate_snapped_anchor(
    hm: &bevy_symbios_ground::HeightMap,
    translation: &mut Vec3,
    clearance: f32,
    room_water_y: Option<f32>,
) {
    let extent = (hm.width().saturating_sub(1)) as f32 * hm.scale();
    let half = extent * 0.5;
    if let Some(water_y) = room_water_y {
        super::water::relocate_above_water(hm, extent, half, translation, water_y, clearance);
    }
    super::slope::relocate_off_steep_ground(hm, extent, half, translation, room_water_y, clearance);
}

/// Where the compile draws a SNAPPED `Absolute` placement's anchor: its
/// record's x/z, walked by [`relocate_snapped_anchor`] when it avoids
/// water, on the ground there ([`snapped_ground_y`]) plus its authored Y
/// as an offset. `room_water_y` is the room's water line, as the compile
/// reads it (`room_water_level`).
///
/// The editor's reading of the anchor, for the sites in the module docs'
/// "Where it stands" (#1399).
pub fn snapped_absolute_anchor(
    hm: &bevy_symbios_ground::HeightMap,
    transform: &crate::pds::TransformData,
    avoid_water: bool,
    avoid_water_clearance: f32,
    room_water_y: Option<f32>,
) -> Vec3 {
    let mut anchor = Vec3::from_array(transform.translation.0);
    let scale_x = transform.scale.0[0];
    if let Some(clearance) = relocation_clearance(avoid_water, avoid_water_clearance, scale_x) {
        relocate_snapped_anchor(hm, &mut anchor, clearance, room_water_y);
    }
    let radius = snap_radius_of(avoid_water, avoid_water_clearance, scale_x);
    anchor.y += snapped_ground_y(hm, anchor.x, anchor.z, radius);
    anchor
}

/// Height of the ground a footprint of `radius` centred on `(x, z)` rests
/// on: the highest terrain under the building.
///
/// Falls back to the plain centre sample for a non-positive or
/// non-finite radius, which is also what a point-like placement wants.
pub(super) fn footprint_height(
    hm: &bevy_symbios_ground::HeightMap,
    extent: f32,
    half: f32,
    x: f32,
    z: f32,
    radius: f32,
) -> f32 {
    let sample = |px: f32, pz: f32| {
        hm.get_height_at(
            (px + half).clamp(0.0, extent),
            (pz + half).clamp(0.0, extent),
        )
    };
    let mut highest = sample(x, z);
    // NaN is caught by the finite check before the comparison sees it.
    if !radius.is_finite() || radius <= 0.0 {
        return highest;
    }

    // The rim.
    for i in 0..RIM_SAMPLES {
        let a = i as f32 * std::f32::consts::TAU / RIM_SAMPLES as f32;
        // libm (#1132): the rim samples decide the pad's height, which the
        // placement is then SNAPPED to - so this feeds a value two peers must
        // agree on exactly, not merely approximately.
        highest = highest.max(sample(
            x + libm::sinf(a) * radius,
            z + libm::cosf(a) * radius,
        ));
    }

    // The interior, at the grid vertices themselves. Between its
    // vertices the map is bilinear, which attains no interior maximum of
    // its own - so the vertices inside the disc, plus the rim above, are
    // the whole story. Sampling *at* a vertex makes the bilinear filter
    // return that vertex's value exactly.
    let cell = hm.scale().max(1e-3);
    let vertex = |i: usize| i as f32 * cell - half;
    let index_of = |w: f32| (w + half) / cell;
    let last_x = hm.width().saturating_sub(1);
    let last_z = hm.height().saturating_sub(1);
    let lo = |w: f32, last: usize| (index_of(w).floor().max(0.0) as usize).min(last);
    let hi = |w: f32, last: usize| (index_of(w).ceil().max(0.0) as usize).min(last);
    let (ix0, ix1) = (lo(x - radius, last_x), hi(x + radius, last_x));
    let (iz0, iz1) = (lo(z - radius, last_z), hi(z + radius, last_z));

    // Stride keeps a pathological footprint/grid pairing bounded; it is
    // 1 for every footprint the catalogue actually declares.
    let span = (ix1 - ix0).max(iz1 - iz0) + 1;
    let stride = span.div_ceil(MAX_SPAN_CELLS).max(1);
    let r2 = radius * radius;
    for iz in (iz0..=iz1).step_by(stride) {
        let wz = vertex(iz);
        let dz = wz - z;
        for ix in (ix0..=ix1).step_by(stride) {
            let dx = vertex(ix) - x;
            if dx * dx + dz * dz <= r2 {
                highest = highest.max(hm.get(ix, iz));
            }
        }
    }
    highest
}

/// Ground a seeded anchor is walked across (#1399): 129 x 129 at 1 m
/// (world -64..64), rising 0.2 m per metre of +X through 0 at x = 30 and
/// flat along Z - gentle everywhere against the steep walk's 0.45. Under a
/// water line at 0 it is wet out to x = 33.75, where the walks' 0.75 m
/// freeboard begins. So an anchor recorded at (25, 0) with a 3 m clearance
/// is walked out along its bearing to exactly (37, 0): the probes at 31 m
/// and 19 m are wet, and at 37 m the ring's nearest point (34 m) reads 0.8.
#[cfg(test)]
pub(crate) fn wet_ramp() -> crate::terrain::FinishedHeightMap {
    let mut hm = bevy_symbios_ground::HeightMap::new(129, 129, 1.0);
    for z in 0..129 {
        for x in 0..129 {
            hm.set(x, z, 0.2 * (x as f32 - 64.0 - 30.0));
        }
    }
    crate::terrain::FinishedHeightMap(hm)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 129×129, scale 1 → world [-64, 64].
    fn map_from(f: impl Fn(f32, f32) -> f32) -> bevy_symbios_ground::HeightMap {
        let mut hm = bevy_symbios_ground::HeightMap::new(129, 129, 1.0);
        for z in 0..129 {
            for x in 0..129 {
                hm.set(x, z, f(x as f32 - 64.0, z as f32 - 64.0));
            }
        }
        hm
    }

    const EXTENT: f32 = 128.0;
    const HALF: f32 = 64.0;

    /// Terrain height at a point, in the same world frame the pad uses.
    fn at(hm: &bevy_symbios_ground::HeightMap, x: f32, z: f32) -> f32 {
        hm.get_height_at((x + HALF).clamp(0.0, EXTENT), (z + HALF).clamp(0.0, EXTENT))
    }

    #[test]
    fn flat_ground_resolves_to_the_plain_sample() {
        let hm = map_from(|_, _| 7.5);
        let y = footprint_height(&hm, EXTENT, HALF, 3.0, -11.0, 8.0);
        assert!((y - 7.5).abs() < 1e-4, "{y}");
    }

    #[test]
    fn a_point_placement_keeps_the_centre_sample() {
        // Zero radius must not consult the neighbourhood at all - that is
        // the editor-placement / point-object path.
        let hm = map_from(|x, _| x);
        for radius in [0.0, -3.0, f32::NAN] {
            let y = footprint_height(&hm, EXTENT, HALF, 10.0, 0.0, radius);
            assert!((y - 10.0).abs() < 1e-4, "radius {radius} gave {y}");
        }
    }

    /// The property the whole change exists for: no part of the footprint
    /// is left above the resolved floor, so no wall starts underground.
    #[test]
    fn no_ground_in_the_footprint_rises_above_the_resolved_height() {
        // A slope steeper than BUILD_SLOPE_LIMIT, plus a cross-ridge, so
        // the high point is neither the centre nor a single edge sample.
        let hm = map_from(|x, z| 0.3 * x + 2.0 * (-(z * z) / 50.0).exp());
        let (cx, cz) = (5.0, -4.0);
        let radius = 9.0;
        let floor = footprint_height(&hm, EXTENT, HALF, cx, cz, radius);

        // Dense independent sweep of the disc - not the sample pattern.
        for i in 0..64 {
            for j in 0..16 {
                let a = i as f32 * std::f32::consts::TAU / 64.0;
                let r = radius * (j as f32 / 15.0);
                let ground = at(&hm, cx + a.sin() * r, cz + a.cos() * r);
                assert!(
                    ground <= floor + 0.02,
                    "ground {ground} at r={r} exceeds resolved floor {floor}"
                );
            }
        }
    }

    /// And it is strictly better than what it replaces: on that same
    /// hillside the centre sample leaves the uphill edge buried.
    #[test]
    fn the_centre_sample_it_replaces_would_bury_the_uphill_edge() {
        let hm = map_from(|x, _| 0.3 * x);
        let (cx, cz) = (5.0, 0.0);
        let radius = 9.0;

        let centre_only = at(&hm, cx, cz);
        let uphill = at(&hm, cx + radius, cz);
        assert!(
            uphill - centre_only > 2.0,
            "fixture should bury the uphill edge: {uphill} vs {centre_only}"
        );

        let floor = footprint_height(&hm, EXTENT, HALF, cx, cz, radius);
        assert!(
            floor >= uphill - 0.02,
            "resolved floor {floor} still sits below the uphill edge {uphill}"
        );
    }

    /// Upward-only: the resolved floor never drops below the centre, so a
    /// building can gain a plinth gap but never sink into the hill.
    #[test]
    fn resolution_is_upward_only() {
        let hm = map_from(|x, z| ((x * 0.2).sin() * 3.0) + ((z * 0.15).cos() * 2.0));
        for (cx, cz) in [(0.0, 0.0), (-20.0, 13.0), (31.0, -27.0), (7.0, 44.0)] {
            let centre = at(&hm, cx, cz);
            let floor = footprint_height(&hm, EXTENT, HALF, cx, cz, 8.0);
            assert!(floor >= centre - 1e-4, "floor {floor} < centre {centre}");
        }
    }

    /// A footprint hanging off the map edge stays clamped inside it
    /// rather than sampling out of bounds.
    #[test]
    fn edge_footprints_stay_in_bounds() {
        let hm = map_from(|x, z| 0.1 * (x + z));
        for (cx, cz) in [(-63.0, 0.0), (63.0, 0.0), (0.0, -63.0), (0.0, 63.0)] {
            let y = footprint_height(&hm, EXTENT, HALF, cx, cz, 20.0);
            assert!(y.is_finite(), "({cx}, {cz}) gave {y}");
        }
    }

    /// A seeded placement reads as a footprint, everything else as a
    /// point - the classification the four snap sites share.
    #[test]
    fn seeded_placements_resolve_as_footprints_and_others_as_points() {
        use crate::pds::{Fp, Fp3, Placement, TransformData};

        let seeded = Placement::Absolute {
            generator_ref: "x".into(),
            transform: TransformData::default(),
            snap_to_terrain: true,
            avoid_water: true,
            avoid_water_clearance: Fp(8.0),
        };
        assert_eq!(snap_footprint_radius(&seeded), Some(8.0));

        // Scaled up, the disc scales with it.
        let scaled = Placement::Absolute {
            generator_ref: "x".into(),
            transform: TransformData {
                scale: Fp3([1.5, 1.5, 1.5]),
                ..Default::default()
            },
            snap_to_terrain: true,
            avoid_water: true,
            avoid_water_clearance: Fp(8.0),
        };
        assert_eq!(snap_footprint_radius(&scaled), Some(12.0));

        // A hand-placed prop has no `avoid_water` marker: point-like.
        let hand = Placement::Absolute {
            generator_ref: "x".into(),
            transform: TransformData::default(),
            snap_to_terrain: true,
            avoid_water: false,
            avoid_water_clearance: Fp(8.0),
        };
        assert_eq!(snap_footprint_radius(&hand), None);
    }

    /// The invariant that keeps a drag from ratcheting (#1011): the
    /// ground a drag commit subtracts is the same one the compile adds
    /// back, so re-committing an unmoved placement is a fixpoint.
    ///
    /// Before the four sites shared this reader, the editor sampled the
    /// centre while the compile took the footprint maximum, and the
    /// difference landed in the stored offset on every drag.
    #[test]
    fn resolving_a_seeded_placement_is_a_fixpoint_across_sites() {
        let hm = map_from(|x, z| 0.25 * x + 0.1 * z);
        let (x, z) = (6.0, -3.0);
        let radius = Some(9.0);

        // What the compile renders at, for a stored offset of -0.35.
        let ground = footprint_height(&hm, EXTENT, HALF, x, z, 9.0);
        let world_y = ground + -0.35;

        // What a drag commit stores back, having not moved the placement.
        let offset = world_y - super::snapped_ground_y(&hm, x, z, radius);
        assert!(
            (offset - -0.35).abs() < 1e-4,
            "offset drifted to {offset} - the sites disagree"
        );

        // A centre-sampling commit (the pre-#1011 bug) would drift up.
        let centre_only = at(&hm, x, z);
        let bad_offset = world_y - centre_only;
        assert!(
            bad_offset - -0.35 > 1.0,
            "fixture should expose the drift the shared reader removes"
        );
    }

    /// Deterministic: peers must derive identical worlds from the same
    /// heightmap, so the sample pattern may not depend on anything else.
    #[test]
    fn resolution_is_deterministic() {
        let hm = map_from(|x, z| (x * 0.11).sin() * (z * 0.07).cos() * 5.0);
        let a = footprint_height(&hm, EXTENT, HALF, 12.0, -6.0, 7.0);
        for _ in 0..8 {
            assert_eq!(a, footprint_height(&hm, EXTENT, HALF, 12.0, -6.0, 7.0));
        }
    }
}
