//! The shape vocabulary the land craft are drawn in (#1377, #1374, #1376,
//! #1375): a board, a thin swept line, a turned part (bored and cut, or
//! solid) and a sweep that pre-divides its path by its own node scale. The
//! wagon drew the first four and the roadster the sweep; the dune buggy draws
//! all five, so they live beside the [`BodyPlan`](super::BodyPlan) rather than
//! inside one type, as the boats' own shape vocabulary does (#1373).
//!
//! And the FACETED half of it: a [`plate`], a [`tapered_plate`] and a
//! [`ramp`], which the armoured car is built from and nothing else in the
//! family is. They are here rather than in her own module because what they
//! close is a fact about the whole vocabulary - a swept or turned shape can
//! never be faceted, whatever its resolution - and the rover is next (#1378).
//!
//! And the parts two types now share: the buggy's turned tyre and the rim
//! that stands proud of it, which the cyclecar rolls on too, and the fairing
//! profile a wheel's spat is swept over (#1376).
//!
//! The roadster keeps its own `line` and `turned`, which take their material
//! by value: it was built before these and its dumps are pinned byte for
//! byte.

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;

use super::super::common::{
    bevel, id_quat, lathe, prim, quat_xyzw, spine, wedge, with_cut, with_taper,
};
use super::dim;

/// A board: a Bevel whose corners are rounded in its `[x, z]` footprint, with
/// every dimension floored at [`super::MIN_DIM`] and the corner radius held
/// under half the smaller footprint axis - the sanitiser's clamp, which would
/// otherwise rewrite it and fail the round trip.
pub(super) fn board(
    size: [f32; 3],
    m: &SovereignMaterialSettings,
    at: [f32; 3],
    rotation: [f32; 4],
    radius: f32,
) -> Generator {
    let size = size.map(dim);
    let cap = size[0].min(size[2]) * 0.5 - 1e-4;
    prim(
        bevel(size, radius.min(cap).max(0.0), 2, m.clone()),
        at,
        quat_xyzw(rotation),
    )
}

/// An armour PLATE: a Bevel with a FLAT chamfer - one bevel segment, so the
/// prim IS an octagonal prism - with its faces planar and its edges hard.
///
/// `chamfer` is a FRACTION of the smaller footprint axis, held under half of
/// it (the sanitiser's own clamp, as [`board`] holds its radius). A board is
/// the same prim with two segments, which rounds its vertical edges off; a
/// plate keeps them, and that difference is the whole reason this helper
/// exists: **a swept or turned shape cannot be faceted at all**. A Spine
/// pushes a radial ring normal at every station and a Lathe a radial revolve
/// normal at every profile point, whatever the resolution, so a res-6 hull
/// has a hexagonal silhouette and smooth barrel SHADING - and at 12 m the
/// shading is what reads. Facets come only from Bevel, Wedge and Cuboid
/// (#1375).
pub(super) fn plate(
    size: [f32; 3],
    m: &SovereignMaterialSettings,
    at: [f32; 3],
    rotation: [f32; 4],
    chamfer: f32,
) -> Generator {
    tapered_plate(size, m, at, rotation, chamfer, [0.0; 2], [0.0; 2])
}

/// The same plate cut to a section: `taper` slopes its flanks in toward its
/// roof and `taper_bottom` in toward its floor, per axis `[x, z]`.
///
/// One node is then an armoured hull plate with sloped flanks, or an
/// octagonal turret frustum. `taper` 0.5 over the datum and `taper_bottom`
/// 0.5 under it reproduce exactly the hexagon a res-6 sweep of the same plan
/// would draw, which is what lets every mount read one function -
/// `flank_x(z, y)` - whichever form the hull is drawn in (#1375).
#[allow(clippy::too_many_arguments)]
pub(super) fn tapered_plate(
    size: [f32; 3],
    m: &SovereignMaterialSettings,
    at: [f32; 3],
    rotation: [f32; 4],
    chamfer: f32,
    taper: [f32; 2],
    taper_bottom: [f32; 2],
) -> Generator {
    let size = size.map(dim);
    let smaller = size[0].min(size[2]);
    let radius = (smaller * chamfer).min(smaller * 0.5 - 1e-4).max(0.0);
    prim(
        with_taper(bevel(size, radius, 1, m.clone()), taper, taper_bottom),
        at,
        quat_xyzw(rotation),
    )
}

/// A right-triangular prism in its `size` box, placed and turned: the slope
/// rises from the front-bottom (`+Z`, `-Y`) edge to the back-top (`-Z`, `+Y`)
/// one, so the `-Z` face is the upright one and a wedge placed nose-forward
/// IS a glacis with no rotation at all (#1375).
pub(super) fn ramp(
    size: [f32; 3],
    m: &SovereignMaterialSettings,
    at: [f32; 3],
    rotation: [f32; 4],
) -> Generator {
    prim(wedge(size.map(dim), m.clone()), at, quat_xyzw(rotation))
}

/// A thin swept line - a rail, a spoke, a spring, an axle, a pole.
pub(super) fn line(
    points: &[([f32; 3], f32)],
    resolution: u32,
    m: &SovereignMaterialSettings,
) -> Generator {
    let pts: Vec<([f32; 3], f32)> = points.iter().map(|&(p, r)| (p, dim(r))).collect();
    prim(spine(&pts, resolution, m.clone()), [0.0; 3], id_quat())
}

/// A turned part, optionally bored (`hollow`) and cut to an angular range
/// (`path`).
#[allow(clippy::too_many_arguments)]
pub(super) fn turned(
    profile: &[(f32, f32)],
    resolution: u32,
    smooth: bool,
    m: &SovereignMaterialSettings,
    at: [f32; 3],
    rotation: [f32; 4],
    hollow: f32,
    path: [f32; 2],
) -> Generator {
    prim(
        with_cut(
            lathe(profile, resolution, smooth, m.clone()),
            path,
            [0.0, 1.0],
            hollow,
        ),
        at,
        quat_xyzw(rotation),
    )
}

/// A solid turned part, whole round.
pub(super) fn solid(
    profile: &[(f32, f32)],
    resolution: u32,
    smooth: bool,
    m: &SovereignMaterialSettings,
    at: [f32; 3],
    rotation: [f32; 4],
) -> Generator {
    turned(
        profile,
        resolution,
        smooth,
        m,
        at,
        rotation,
        0.0,
        [0.0, 1.0],
    )
}

/// A swept shape whose section is shaped by its own node scale - a body run, a
/// flattened guard.
///
/// **The path is pre-divided by that scale here, and that is the trap this
/// helper exists to close.** A node scale moves its path as well as its
/// profile, so a station written straight into the path is drawn displaced by
/// exactly the factor that makes the shape the shape. It cost the boat two
/// silent defects before a `debug_assert` caught it (#1363), and the
/// roadster's guards would be the next: they carry a 3.4x scale on x and
/// their arcs are written in true metres off the wheel landmarks.
///
/// `cut` is the swept profile's kept fraction: `[0.5, 1.0]` is the lower half
/// (and its flat cut face is the coaming), `[0.0, 0.5]` the upper half (a
/// deck), `[0.0, 1.0]` the whole barrel. `hollow` bores it.
pub(super) fn sweep(
    points: &[([f32; 3], f32)],
    resolution: u32,
    scale: [f32; 3],
    cut: [f32; 2],
    hollow: f32,
    material: SovereignMaterialSettings,
) -> Generator {
    let path: Vec<([f32; 3], f32)> = points
        .iter()
        .map(|&([x, y, z], r)| ([x / scale[0], y / scale[1], z / scale[2]], dim(r)))
        .collect();
    let mut node = prim(
        with_cut(spine(&path, resolution, material), cut, [0.0, 1.0], hollow),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    node.transform.scale = Fp3(scale);
    node
}

/// A turned tyre of radius `r` and half-width `w` - the dune buggy's fat
/// off-road one (#1374), and the cyclecar's at her narrower width (#1376):
/// squared shoulders and a single crown point at `r`, so the drawn tyre
/// reaches exactly the axle's radius and stands on the ground. Its end caps
/// are solid discs at `0.86 w`.
pub(super) fn tyre_profile(r: f32, w: f32) -> [(f32, f32); 9] {
    let lip = r * 0.58;
    [
        (lip, -w * 0.86),
        (r * 0.80, -w),
        (r * 0.95, -w * 0.93),
        (r * 0.995, -w * 0.55),
        (r, 0.0),
        (r * 0.995, w * 0.55),
        (r * 0.95, w * 0.93),
        (r * 0.80, w),
        (lip, w * 0.86),
    ]
}

/// A wide mag rim standing proud of the tyre's end caps on BOTH faces, at
/// `1.10 w`.
pub(super) fn rim_profile(r: f32, w: f32) -> [(f32, f32); 10] {
    let lip = r * 0.58;
    [
        (0.0, -w * 1.10),
        (lip * 0.30, -w * 1.06),
        (lip * 0.42, -w * 0.92),
        (lip * 1.00, -w * 0.90),
        (lip * 1.03, -w * 0.50),
        (lip * 1.03, w * 0.50),
        (lip * 1.00, w * 0.90),
        (lip * 0.42, w * 0.92),
        (lip * 0.30, w * 1.06),
        (0.0, w * 1.10),
    ]
}

/// How far a fairing's cut plane stands under the axle, and how far its
/// shell clears the tyre's crown, both over the wheel's radius (#1376).
pub(super) const FAIRING_DROP: f32 = 0.30;
pub(super) const FAIRING_CLEAR: f32 = 0.10;

/// A wheel fairing's profile along its wheel, `(dz, radius)` over the wheel's
/// radius, fore to aft of the axle: what a spat is swept over (#1376).
///
/// **Read off the tyre it covers, and that is a render result.** A
/// hand-typed spat and a pair of wheel pants cleared the tyre's crown by 0
/// and 6 % and the tyre showed through them as black arcs. So at every
/// station the radius is the tyre's crown height there, plus the cut plane
/// [`FAIRING_DROP`] under the axle, plus [`FAIRING_CLEAR`]; and the ends
/// close in past the tyre's own reach. Swept as an upper half-pipe on a path
/// [`FAIRING_DROP`] under the axle, it covers the tyre's top and leaves its
/// lower part standing on the ground.
pub(super) fn fairing() -> [(f32, f32); 7] {
    let station = |dz: f32| {
        (
            dz,
            (1.0 - dz * dz).max(0.0).sqrt() + FAIRING_DROP + FAIRING_CLEAR,
        )
    };
    [
        (-1.30, 0.30),
        station(-1.05),
        station(-0.60),
        station(0.0),
        station(0.60),
        station(1.05),
        (1.30, 0.30),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tyre reaches exactly its radius and no further, so with its axle one
    /// radius over the ground it stands ON the ground; and the rim stands
    /// proud of both of the tyre's end caps, or the drum swallows it.
    #[test]
    fn the_tyre_meets_the_ground_and_the_rim_stands_proud() {
        for r in [0.2f32, 0.31, 0.37, 0.5] {
            for w in [r * 0.26, r * 0.38] {
                let tyre = tyre_profile(r, w);
                let widest = tyre.iter().map(|&(x, _)| x).fold(0.0f32, f32::max);
                assert!(
                    (widest - r).abs() < 1e-6,
                    "the tyre is {widest} across a {r} wheel"
                );
                let cap = tyre[0].1.abs().max(tyre[tyre.len() - 1].1.abs());
                let rim = rim_profile(r, w);
                assert!(
                    rim[0].1 < -cap && rim[rim.len() - 1].1 > cap,
                    "the rim is inside the tyre's end caps at {cap}"
                );
            }
        }
    }

    /// A fairing clears the tyre it covers at every station along it: over
    /// its cut plane, its shell stands [`FAIRING_CLEAR`] over the crown of a
    /// unit tyre wherever the tyre reaches, and closes in past it.
    #[test]
    fn a_fairing_clears_its_tyre() {
        let prof = fairing();
        for w in prof.windows(2) {
            assert!(w[0].0 < w[1].0, "the stations run fore and aft in order");
        }
        for &(dz, k) in &prof {
            assert!(k > 0.0, "a fairing station at {dz} has radius {k}");
            let crown = (1.0 - dz * dz).max(0.0).sqrt();
            if crown > 0.0 {
                assert!(
                    (k - FAIRING_DROP - crown - FAIRING_CLEAR).abs() < 1e-6,
                    "at {dz} the shell is not its clearance over the tyre's crown"
                );
            }
        }
        assert!(prof[0].0 < -1.0 && prof[prof.len() - 1].0 > 1.0);
    }
}
