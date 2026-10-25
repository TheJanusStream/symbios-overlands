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
//!
//! On Berlin's ground (#1589) the walk is to open, dry ground: off the
//! water and off the street space the land use leaves, and where the
//! anchor's bearing finds none, to the nearest. Every site reads the
//! ground through an [`AnchorGround`], so they agree there too.

use bevy::math::Vec3;

/// The ground a snapped anchor's walks read: the heights, the room's water,
/// and - on Berlin's ground (#1589) - where its streets run.
#[derive(Clone, Copy)]
pub(crate) struct AnchorGround<'a> {
    pub hm: &'a bevy_symbios_ground::HeightMap,
    /// The room's water, as the compile reads it (`drawn_water`): its sea
    /// and its lakes, each walk asking the level at its own point (#1727).
    pub water: &'a crate::water::bodies::RoomWater,
    /// Berlin's ground, where the region is built from it.
    pub berlin: Option<&'a crate::terrain::geo::GeoGround>,
}

impl<'a> AnchorGround<'a> {
    /// `heightmap`'s ground, under the room's `water`.
    pub(crate) fn new(
        heightmap: &'a crate::terrain::FinishedHeightMap,
        water: &'a crate::water::bodies::RoomWater,
    ) -> Self {
        AnchorGround {
            hm: &heightmap.0,
            water,
            berlin: heightmap.ground(),
        }
    }
}

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

/// How far along its own axis a seeded jetty looks for the water's edge,
/// either way (m), and the step it reads the ground at (#1782): as far as
/// the derive kept its ground clear for (`room::crafts::jetty::WALK`).
const SHORE_REACH: f32 = 50.0;
const SHORE_STEP: f32 = 0.5;
/// How far inland of the water's edge a seeded jetty's anchor stands (m):
/// its deck leaves the land there and runs back onto the bank past it.
const SHORE_SET_BACK: f32 = 1.0;
/// How deep the water under a seeded jetty's head is, at the least, for it
/// to stand (m): its boats float there.
const JETTY_HEAD_DEPTH: f32 = 0.8;
/// How far the walk off water may move a seeded pad or mast before it no
/// longer stands where it was sited clear of the rest (m).
const PLACE_SLIDE: f32 = 4.0;
/// How far the ground may fall away under a seeded pad or mast before its
/// skirt or base no longer reaches it (m): the helipad's skirt, the mast's
/// base (`catalogue::items::crafts`).
const PLACE_REACH: f32 = 3.5;

/// Which way a placement turned by `rotation` looks: its front, -Z, on the
/// ground.
pub(crate) fn front_of(rotation: bevy::math::Quat) -> bevy::math::Vec2 {
    let front = rotation * Vec3::NEG_Z;
    bevy::math::Vec2::new(front.x, front.z)
}

/// Walk a seeded jetty's anchor to the water's edge the ground draws
/// (#1782): the derive sites it on a probe that misreads a shore by metres
/// (by tens of them along a gentle one). Its anchor - where its deck leaves
/// the land - moves along `out`, the way it runs out over the water, to
/// [`SHORE_SET_BACK`] inland of the crossing from dry ground to water
/// nearest it. `false`, the anchor left where it is, where no such crossing
/// lies within [`SHORE_REACH`]. Moves X/Z only: its height is the water's,
/// which its record keeps. Read through [`place_stands`], as the compile,
/// the scatters' keep-outs and the editor's outline all read it.
pub(crate) fn meet_the_shore(
    ground: &AnchorGround<'_>,
    translation: &mut Vec3,
    out: bevy::math::Vec2,
) -> bool {
    let Some(out) = out.try_normalize() else {
        return false;
    };
    let from = bevy::math::Vec2::new(translation.x, translation.z);
    let wet = |s: f32| wet_at(ground, from + out * s);
    // Every crossing in reach, the nearest kept: the first of equals,
    // landward of the anchor before seaward.
    let steps = (SHORE_REACH / SHORE_STEP) as i32;
    let mut nearest: Option<f32> = None;
    let mut dry = !wet(-SHORE_REACH);
    for k in -steps..steps {
        let s = (k + 1) as f32 * SHORE_STEP;
        let here = !wet(s);
        if dry && !here {
            let crossing = s - SHORE_STEP * 0.5;
            if nearest.is_none_or(|kept| crossing.abs() < kept.abs()) {
                nearest = Some(crossing);
            }
        }
        dry = here;
    }
    let Some(crossing) = nearest else {
        return false;
    };
    let at = from + out * (crossing - SHORE_SET_BACK);
    translation.x = at.x;
    translation.z = at.y;
    true
}

/// The ground's height at world `at`, on `ground`'s map.
fn height_at(ground: &AnchorGround<'_>, at: bevy::math::Vec2) -> f32 {
    let hm = ground.hm;
    let extent = (hm.width().saturating_sub(1)) as f32 * hm.scale();
    let half = extent * 0.5;
    hm.get_height_at(
        (at.x + half).clamp(0.0, extent),
        (at.y + half).clamp(0.0, extent),
    )
}

/// How deep the room's water stands over world `at`, or `None` where it
/// has none there.
fn depth_at(ground: &AnchorGround<'_>, at: bevy::math::Vec2) -> Option<f32> {
    ground
        .water
        .level_at(at.x, at.y)
        .map(|level| level - height_at(ground, at))
}

/// Whether world `at` is under the room's water.
fn wet_at(ground: &AnchorGround<'_>, at: bevy::math::Vec2) -> bool {
    depth_at(ground, at).is_some_and(|depth| depth > 0.0)
}

/// The lowest and the highest ground under a disc of `radius` round
/// `(x, z)`: its rim, a ring halfway in and its middle, read where the
/// heightmap draws them.
fn footprint_range(ground: &AnchorGround<'_>, x: f32, z: f32, radius: f32) -> (f32, f32) {
    let middle = height_at(ground, bevy::math::Vec2::new(x, z));
    let (mut low, mut high) = (middle, middle);
    for ring in [0.5, 1.0] {
        for i in 0..RIM_SAMPLES {
            let a = i as f32 * std::f32::consts::TAU / RIM_SAMPLES as f32;
            // libm (#1132), as the snap's rim.
            let at = bevy::math::Vec2::new(
                x + libm::sinf(a) * radius * ring,
                z + libm::cosf(a) * radius * ring,
            );
            let h = height_at(ground, at);
            low = low.min(h);
            high = high.max(h);
        }
    }
    (low, high)
}

/// How far off a track's edge a windsock or a track's tyres stand to be
/// that track's (m): a seeded windsock stands 5 m off its airstrip's edge,
/// the tyres 1.5 m off their track's.
const BESIDE_A_TRACK: f32 = 8.0;

/// Whether `at` stands within [`BESIDE_A_TRACK`] of the edge of one of
/// `tracks` the ground leaves unpainted (`terrain::track_drawn`): a windsock
/// there marks an airstrip that is not drawn.
fn beside_a_bare_track(
    ground: &AnchorGround<'_>,
    at: bevy::math::Vec2,
    tracks: &[crate::pds::SovereignTrack],
) -> bool {
    tracks.iter().any(|track| {
        let line: Vec<bevy::math::Vec2> = track
            .line
            .iter()
            .map(|p| bevy::math::Vec2::from_array(p.0))
            .collect();
        let off = line
            .windows(2)
            .map(|pair| {
                let (a, b) = (pair[0], pair[1]);
                let along = b - a;
                let t = if along.length_squared() > 0.0 {
                    ((at - a).dot(along) / along.length_squared()).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                at.distance(a + along * t)
            })
            .fold(f32::MAX, f32::min);
        off <= track.width.0 * 0.5 + BESIDE_A_TRACK
            && !crate::terrain::track_drawn((ground.hm, ground.water), &line, track.width.0)
    })
}

/// How a seeded world's place for a craft (#1782) stands on the ground the
/// compile draws, which the probe it was sited on misreads: by the shore,
/// in a hollow the sea floods, on a slope.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PlaceStand {
    /// Not a place for a craft: it stands as any placement does.
    Other,
    /// It stands here: a jetty walked to its shore, a snapped place where
    /// its snap puts it.
    At(Vec3),
    /// The ground does not take it: it is not drawn and keeps nothing
    /// clear. Its record keeps it, for its owner to move.
    Not,
}

/// How the placement of `generator_ref` placed by `transform` stands on
/// `ground` (see [`PlaceStand`]): a seeded jetty (`compile::scatter::JETTY`,
/// unsnapped) walked to its shore, standing only where the water under its
/// head is [`JETTY_HEAD_DEPTH`] deep; a seeded pad or mast (built) standing
/// only where the walk off water moves it no more than [`PLACE_SLIDE`] and
/// its ground falls no more than [`PLACE_REACH`] under it; a windsock or a
/// track's tyres standing only on dry ground, beside no track of `tracks`
/// the ground leaves unpainted ([`beside_a_bare_track`]). The compile, the
/// scatters' keep-outs and the editor's outline all read it, so they
/// agree.
pub(crate) fn place_stands(
    ground: &AnchorGround<'_>,
    generator_ref: &str,
    transform: &crate::pds::TransformData,
    snap: bool,
    avoid_water: bool,
    clearance: f32,
    tracks: &[crate::pds::SovereignTrack],
) -> PlaceStand {
    use super::scatter::{JETTY, MAST, PAD, TYRES, WINDSOCK};
    use crate::catalogue::items::crafts::jetty::{HEAD, SEEDED_LENGTH};
    let record = Vec3::from_array(transform.translation.0);
    match generator_ref {
        JETTY if !snap => {
            let out = front_of(bevy::math::Quat::from_array(transform.rotation.0));
            let mut at = record;
            if !meet_the_shore(ground, &mut at, out) {
                return PlaceStand::Not;
            }
            let head = bevy::math::Vec2::new(at.x, at.z)
                + out.normalize_or_zero() * (SEEDED_LENGTH - HEAD[1] * 0.5);
            match depth_at(ground, head) {
                Some(depth) if depth >= JETTY_HEAD_DEPTH => PlaceStand::At(at),
                _ => PlaceStand::Not,
            }
        }
        PAD | MAST if snap && avoid_water => {
            let at = snapped_absolute_anchor(ground, transform, avoid_water, clearance);
            let moved = (at.x - record.x).hypot(at.z - record.z);
            let (low, high) = footprint_range(ground, at.x, at.z, clearance);
            if moved > PLACE_SLIDE || high - low > PLACE_REACH {
                PlaceStand::Not
            } else {
                PlaceStand::At(at)
            }
        }
        WINDSOCK | TYRES if snap => {
            let at = snapped_absolute_anchor(ground, transform, avoid_water, clearance);
            let xz = bevy::math::Vec2::new(at.x, at.z);
            if wet_at(ground, xz) || beside_a_bare_track(ground, xz, tracks) {
                PlaceStand::Not
            } else {
                PlaceStand::At(at)
            }
        }
        _ => PlaceStand::Other,
    }
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

/// Walk a snapped anchor to where the compile stands it: off water - on
/// Berlin's ground, to open dry ground (#1589) - then off over-steep
/// ground (#905), along its bearing through the origin. Moves X/Z only.
/// `clearance` comes from [`relocation_clearance`]; a placement without one
/// is never walked.
pub(super) fn relocate_snapped_anchor(
    ground: &AnchorGround<'_>,
    translation: &mut Vec3,
    clearance: f32,
) {
    let hm = ground.hm;
    let extent = (hm.width().saturating_sub(1)) as f32 * hm.scale();
    let half = extent * 0.5;
    let steep = |translation: &mut Vec3| {
        super::slope::relocate_off_steep_ground(
            hm,
            extent,
            half,
            translation,
            ground.water,
            clearance,
        );
    };
    match ground.berlin {
        Some(berlin) => {
            let open = |translation: &mut Vec3| {
                super::water::relocate_to_open_ground(
                    hm,
                    (extent, half),
                    translation,
                    ground.water,
                    berlin,
                    clearance,
                );
            };
            open(translation);
            steep(translation);
            // The slope walk knows no streets: a spot it moved onto one, or
            // onto water, is walked open again, which it leaves alone where
            // it stands.
            open(translation);
        }
        None => {
            if !ground.water.is_dry() {
                super::water::relocate_above_water(
                    hm,
                    extent,
                    half,
                    translation,
                    ground.water,
                    clearance,
                );
            }
            steep(translation);
        }
    }
}

/// Where the compile draws a SNAPPED `Absolute` placement's anchor: its
/// record's x/z, walked by [`relocate_snapped_anchor`] when it avoids
/// water, on the ground there ([`snapped_ground_y`]) plus its authored Y
/// as an offset. `ground` carries the room's water, as the compile reads
/// it (`drawn_water`: its sea and its lakes, #1760, or in a geodata region
/// whose core has water Berlin's own bodies, #1724), and Berlin's ground
/// where there is one (#1589).
///
/// The editor's reading of the anchor, for the sites in the module docs'
/// "Where it stands" (#1399) - and the Berlin seeder's (#1647), which keeps
/// Berlin's buildings clear of where a seeded gate stands, not of where its
/// record put it.
pub(crate) fn snapped_absolute_anchor(
    ground: &AnchorGround<'_>,
    transform: &crate::pds::TransformData,
    avoid_water: bool,
    avoid_water_clearance: f32,
) -> Vec3 {
    let mut anchor = Vec3::from_array(transform.translation.0);
    let scale_x = transform.scale.0[0];
    if let Some(clearance) = relocation_clearance(avoid_water, avoid_water_clearance, scale_x) {
        relocate_snapped_anchor(ground, &mut anchor, clearance);
    }
    let radius = snap_radius_of(avoid_water, avoid_water_clearance, scale_x);
    anchor.y += snapped_ground_y(ground.hm, anchor.x, anchor.z, radius);
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
    crate::terrain::FinishedHeightMap(hm, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1782: a seeded jetty meets the shore the ground draws along its own
    /// axis - from inland out to it, or back from the water to it - its
    /// anchor SHORE_SET_BACK inland of the water's edge; and stays where it
    /// is where its axis crosses no shore within reach, or the room has no
    /// water.
    #[test]
    fn a_jetty_meets_the_shore_along_its_axis() {
        // The wet ramp's shore: dry east of x = 30, the sea west of it.
        let hm = wet_ramp();
        let sea = crate::water::bodies::RoomWater::sea_alone(Some(0.0));
        let ground = AnchorGround::new(&hm, &sea);
        let west = bevy::math::Vec2::new(-1.0, 0.0);
        for from in [45.0, 20.0] {
            let mut at = Vec3::new(from, 0.6, 7.0);
            assert!(
                meet_the_shore(&ground, &mut at, west),
                "from {from}: a shore"
            );
            assert!(
                (at.x - (30.0 + SHORE_SET_BACK)).abs() <= SHORE_STEP,
                "from {from}: {at}"
            );
            assert_eq!(
                (at.y, at.z),
                (0.6, 7.0),
                "from {from}: X and Z along its axis alone"
            );
        }
        let mut along = Vec3::new(45.0, 0.6, 0.0);
        assert!(!meet_the_shore(
            &ground,
            &mut along,
            bevy::math::Vec2::new(0.0, 1.0)
        ));
        assert_eq!(along, Vec3::new(45.0, 0.6, 0.0), "no shore crossed: left");
        let dry = crate::water::bodies::RoomWater::sea_alone(None);
        let mut inland = Vec3::new(45.0, 0.6, 0.0);
        assert!(!meet_the_shore(
            &AnchorGround::new(&hm, &dry),
            &mut inland,
            west
        ));
        assert_eq!(inland, Vec3::new(45.0, 0.6, 0.0), "no water: left");
    }

    /// #1782: a seeded place for a craft stands only where the ground the
    /// compile draws takes it - a jetty with its shore along it and water
    /// deep enough at its head, a pad dry where it was sited and on ground
    /// its skirt reaches, a windsock out of the water and beside an airstrip
    /// that is painted - and any other placement stands as it always did.
    #[test]
    fn a_place_for_a_craft_stands_where_the_ground_takes_it() {
        use super::super::scatter::{JETTY, PAD, WINDSOCK};
        use crate::pds::{Fp3, Fp4, TransformData};
        let placed = |x: f32, z: f32, yaw: f32| TransformData {
            translation: Fp3([x, 0.6, z]),
            rotation: Fp4([0.0, (yaw * 0.5).sin(), 0.0, (yaw * 0.5).cos()]),
            scale: Fp3([1.0, 1.0, 1.0]),
        };
        // A yaw of a quarter turns the front (-Z) to -X: west, to the sea.
        let west = std::f32::consts::FRAC_PI_2;
        let sea = crate::water::bodies::RoomWater::sea_alone(Some(0.0));
        let ramp = wet_ramp();
        let ground = AnchorGround::new(&ramp, &sea);
        match place_stands(
            &ground,
            JETTY,
            &placed(45.0, 7.0, west),
            false,
            false,
            4.0,
            &[],
        ) {
            PlaceStand::At(at) => assert!((at.x - (30.0 + SHORE_SET_BACK)).abs() <= SHORE_STEP),
            other => panic!("a jetty off a shore stands: {other:?}"),
        }
        let north = placed(45.0, 7.0, 0.0);
        assert_eq!(
            place_stands(&ground, JETTY, &north, false, false, 4.0, &[]),
            PlaceStand::Not
        );
        // A shore shelving 2 cm a metre: 0.3 m of water at the head.
        let gentle = crate::terrain::FinishedHeightMap(map_from(|x, _| 0.02 * (x - 30.0)), None);
        let shallow = AnchorGround::new(&gentle, &sea);
        let jetty = placed(45.0, 7.0, west);
        assert_eq!(
            place_stands(&shallow, JETTY, &jetty, false, false, 4.0, &[]),
            PlaceStand::Not
        );
        // A pad on the dry ramp, 2.8 m of fall under its 7 m: it stands; one
        // the water walk carries off its site does not, nor one on ground
        // falling past its skirt.
        assert!(matches!(
            place_stands(&ground, PAD, &placed(50.0, 0.0, 0.0), true, true, 7.0, &[]),
            PlaceStand::At(_)
        ));
        let wet_pad = placed(25.0, 0.0, 0.0);
        assert_eq!(
            place_stands(&ground, PAD, &wet_pad, true, true, 7.0, &[]),
            PlaceStand::Not
        );
        let steep = crate::terrain::FinishedHeightMap(map_from(|x, _| 0.3 * x + 30.0), None);
        let hillside = AnchorGround::new(&steep, &sea);
        let pad = placed(20.0, 0.0, 0.0);
        assert_eq!(
            place_stands(&hillside, PAD, &pad, true, true, 7.0, &[]),
            PlaceStand::Not
        );
        // A windsock on dry ground stands, one in the water does not.
        assert!(matches!(
            place_stands(
                &ground,
                WINDSOCK,
                &placed(50.0, 0.0, 0.0),
                true,
                false,
                2.0,
                &[]
            ),
            PlaceStand::At(_)
        ));
        let drowned = placed(10.0, 0.0, 0.0);
        assert_eq!(
            place_stands(&ground, WINDSOCK, &drowned, true, false, 2.0, &[]),
            PlaceStand::Not
        );
        // A windsock beside its airstrip stands with it: dry beside a strip
        // the sea floods it is not drawn, beside a strip on dry ground it is.
        let strip = |x: f32| crate::pds::SovereignTrack {
            line: vec![crate::pds::Fp2([x, -30.0]), crate::pds::Fp2([x, 30.0])],
            width: crate::pds::Fp(12.0),
        };
        let flooded = [strip(20.0)];
        let sock = placed(32.0, 0.0, 0.0);
        assert_eq!(
            place_stands(&ground, WINDSOCK, &sock, true, false, 2.0, &flooded),
            PlaceStand::Not
        );
        let dry = [strip(45.0)];
        let sock = placed(56.0, 0.0, 0.0);
        assert!(matches!(
            place_stands(&ground, WINDSOCK, &sock, true, false, 2.0, &dry),
            PlaceStand::At(_)
        ));
        // Anything else is no place for a craft.
        let house = placed(50.0, 0.0, 0.0);
        assert_eq!(
            place_stands(&ground, "landmark", &house, true, true, 8.0, &[]),
            PlaceStand::Other
        );
    }

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
            seed: None,
            params: Default::default(),
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
            seed: None,
            params: Default::default(),
        };
        assert_eq!(snap_footprint_radius(&scaled), Some(12.0));

        // A hand-placed prop has no `avoid_water` marker: point-like.
        let hand = Placement::Absolute {
            generator_ref: "x".into(),
            transform: TransformData::default(),
            snap_to_terrain: true,
            avoid_water: false,
            avoid_water_clearance: Fp(8.0),
            seed: None,
            params: Default::default(),
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
