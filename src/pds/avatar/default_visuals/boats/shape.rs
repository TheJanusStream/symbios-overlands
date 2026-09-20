//! The shape vocabulary every finless boat is drawn in (#1372, #1373,
//! #1370, #1371): a sweep that pre-divides its path by its own node scale, a
//! thin line, a rounded panel, a turned part, the hull's stations as a sweep
//! path, the crowned deck run over them, and the underbody a screw boat
//! hangs under her counter. The runabout drew them first; the scow and the
//! steam tug draw the same parts, so they live beside the [`HullProfile`]
//! rather than inside one type.
//!
//! The junk added a FLATTENED sweep ([`flat_sweep`]): a Spine squashed thin
//! across one axis is a board with a free outline, each station's radius its
//! half-width - her sails, her transom, her bulkhead and her rudder blade are
//! all one - and a [`band`] of one, kept between two of its stations as the
//! mesher samples it, is a panel of a sail that lies on its cloth exactly.
//!
//! The sloop keeps her own (`sloop/hull.rs`): she was built before these
//! and her dumps are pinned byte for byte.

use bevy::math::cubic_splines::{CubicCardinalSpline, CubicGenerator};
use bevy::math::{Vec3, Vec4};

use crate::pds::generator::{Generator, GeneratorKind};
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::{Fp2, Fp3};

use super::super::common::{
    bevel, cuboid, id_quat, lathe, prim, quat_x, quat_xyzw, spine, with_cut,
};
use super::dim;
use super::profile::HullProfile;

/// A swept shape squashed by its own node scale, bored by `hollow`. The path
/// is PRE-DIVIDED by the scale (the sloop's `sweep`, and the trap it closes:
/// a node scale moves the path as well as the profile).
pub(super) fn sweep(
    points: &[([f32; 3], f32)],
    resolution: u32,
    scale: [f32; 3],
    cut: [f32; 2],
    m: &SovereignMaterialSettings,
    hollow: f32,
) -> Generator {
    let path: Vec<([f32; 3], f32)> = points
        .iter()
        .map(|&([x, y, z], r)| ([x / scale[0], y / scale[1], z / scale[2]], dim(r)))
        .collect();
    let mut node = prim(
        with_cut(spine(&path, resolution, m.clone()), cut, [0.0, 1.0], hollow),
        [0.0; 3],
        id_quat(),
    );
    node.transform.scale = Fp3(scale);
    node
}

/// A FLATTENED sweep (#1371): a Spine squashed to `s` across `axis` (0 is
/// x, 2 is z) and placed at `at` by its node's translation.
///
/// The path is given in the root frame, taken about `at` and pre-divided by
/// the scale, as [`sweep`]'s is. The offset rides the node rather than the
/// path: pre-divided by a scale of a hundredth, an offset in the path grows a
/// hundredfold, and the record sanitiser clamps a Spine's positions at 100
/// local units.
/// Every section is perpendicular to the path, so the lens-thin board it
/// draws has a free outline - each station's radius is its half-width - and
/// the connectivity guard, which honours node scale, reads it as the board it
/// is. Never rotate one: a non-uniform scale under a rotation is not a
/// similarity, which that guard asserts. Lay it out where it stands instead.
pub(super) fn flat_sweep(
    points: &[([f32; 3], f32)],
    resolution: u32,
    axis: usize,
    s: f32,
    m: &SovereignMaterialSettings,
    at: [f32; 3],
) -> Generator {
    let mut scale = [1.0; 3];
    scale[axis] = s;
    let path: Vec<([f32; 3], f32)> = points
        .iter()
        .map(|&(p, r)| {
            // A negative half-width is a bug to report, never a value to
            // floor into a hair-thin board (the tug's deck walk, #1370).
            debug_assert!(r > 0.0, "a flattened sweep's half-width is {r}");
            (std::array::from_fn(|k| (p[k] - at[k]) / scale[k]), dim(r))
        })
        .collect();
    let mut node = prim(spine(&path, resolution, m.clone()), at, id_quat());
    node.transform.scale = Fp3(scale);
    node
}

/// A Spine kept only between its stations `i0` and `i1` (#1371): a
/// `profile_cut` band over the sweep it is given.
///
/// The mesher trims a Spine's PATH by `profile_cut`, measured in the arc
/// length it samples and snapped to its samples
/// (`world_builder/prim/sweeps.rs`: bevy's Catmull-Rom over position and
/// radius, mirrored ends, `samples_per_segment` a segment). So each
/// station's fraction is measured exactly that way, on the node's own local
/// path, and each end of the band goes half a sample outside its station so
/// the snap lands ON it. The kept sections are the whole sweep's, so a band
/// of a sail lies on its cloth exactly: a worn junk's replaced panel, and
/// the two halves of a battered one's mainsail either side of the panel she
/// has lost.
pub(super) fn band(mut node: Generator, i0: usize, i1: usize) -> Generator {
    let GeneratorKind::Spine {
        points,
        samples_per_segment,
        ..
    } = &node.kind
    else {
        panic!("a band is cut from a Spine, not a {}", node.kind.kind_tag());
    };
    let per = (*samples_per_segment).clamp(2, 64);
    let ctrl: Vec<Vec4> = points
        .iter()
        .map(|p| Vec3::from(p.position.0).extend(p.radius.0.max(0.005)))
        .collect();
    let n = (ctrl.len() as u32 - 1) * per;
    let curve = CubicCardinalSpline::new_catmull_rom(ctrl)
        .to_curve()
        .expect("a band is cut from a Spine of two stations or more");
    let mut arc = Vec::with_capacity(n as usize + 1);
    let (mut total, mut prev) = (0.0f32, curve.position(0.0).truncate());
    arc.push(total);
    for i in 1..=n {
        let p = curve.position(i as f32 / per as f32).truncate();
        total += (p - prev).length();
        arc.push(total);
        prev = p;
    }
    let total = total.max(1e-5);
    let at = |station: usize| arc[station * per as usize] / total;
    let half = 1.0 / n as f32;
    let cut = [
        (at(i0) - half * 0.5).max(0.0),
        (at(i1) + half * 0.5).min(1.0),
    ];
    if let Some(t) = node.kind.torture_mut() {
        t.profile_cut = Fp2(cut);
    }
    node
}

/// A thin swept line in the root frame - a rail, a frame, a post.
pub(super) fn line(
    points: &[([f32; 3], f32)],
    resolution: u32,
    m: &SovereignMaterialSettings,
) -> Generator {
    let pts: Vec<([f32; 3], f32)> = points.iter().map(|&(p, r)| (p, dim(r))).collect();
    prim(spine(&pts, resolution, m.clone()), [0.0; 3], id_quat())
}

/// A panel: a Bevel rounded in its `[x, z]` footprint, height on y, every
/// dimension floored and the corner radius held under half the smaller
/// footprint axis - the sanitiser's clamp, which would otherwise rewrite it.
pub(super) fn panel(
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

/// A plain box.
pub(super) fn box_at(size: [f32; 3], m: &SovereignMaterialSettings, at: [f32; 3]) -> Generator {
    prim(cuboid(size.map(dim), m.clone()), at, id_quat())
}

/// A turned part, optionally bored.
pub(super) fn turned(
    profile: &[(f32, f32)],
    resolution: u32,
    smooth: bool,
    m: &SovereignMaterialSettings,
    at: [f32; 3],
    rotation: [f32; 4],
    hollow: f32,
) -> Generator {
    prim(
        with_cut(
            lathe(profile, resolution, smooth, m.clone()),
            [0.0, 1.0],
            [0.0, 1.0],
            hollow,
        ),
        at,
        quat_xyzw(rotation),
    )
}

/// No rotation.
pub(super) const UPRIGHT: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// Deck crown, as a section-depth factor on the deck's own sweep.
pub(super) const DECK_CROWN: f32 = 0.16;

/// A re-laid quadrant's arc on a crowned deck: a quarter of the deck's
/// section, to one side of the centreline - where a worn boat's pale new
/// boards go (#1373, #1370).
pub(super) const PATCH_CUT: [f32; 2] = [0.30, 0.46];

/// The hull's stations as a sweep path on a hull centred at `x0`, each
/// radius grown by `grow`; from `from_z` aft of which nothing is drawn, when
/// given.
pub(super) fn hull_path(
    hull: &HullProfile,
    grow: f32,
    x0: f32,
    from_z: Option<f32>,
) -> Vec<([f32; 3], f32)> {
    let st = hull.stations();
    let mut pts: Vec<([f32; 3], f32)> = Vec::with_capacity(st.len() + 1);
    if let Some(z0) = from_z {
        pts.push(([x0, hull.sheer_z(z0), z0], hull.half_beam_at(z0) * grow));
        pts.extend(
            st.iter()
                .filter(|s| s.z > z0 + 1e-6)
                .map(|s| ([x0, s.sheer, s.z], s.half_beam * grow)),
        );
    } else {
        pts.extend(st.iter().map(|s| ([x0, s.sheer, s.z], s.half_beam * grow)));
    }
    pts
}

/// The stations strictly between `z0` and `z1`, bracketed by both ends - a
/// sub-run of the hull that still reads every station on it.
pub(super) fn run_z(hull: &HullProfile, z0: f32, z1: f32) -> Vec<f32> {
    let mut zs = vec![z0];
    zs.extend(
        hull.stations()
            .iter()
            .map(|s| s.z)
            .filter(|&z| z0 < z && z < z1),
    );
    zs.push(z1);
    zs
}

/// A crowned deck over the stations from `z0` to `z1`, on a hull centred at
/// `x0`, set a hair under the sheer so its underside is not coplanar with the
/// shell's cut band.
pub(super) fn deck_run(
    hull: &HullProfile,
    z0: f32,
    z1: f32,
    m: &SovereignMaterialSettings,
    x0: f32,
) -> Generator {
    let pts: Vec<_> = run_z(hull, z0, z1)
        .into_iter()
        .map(|z| {
            (
                [x0, hull.sheer_z(z) - hull.loa * 0.002, z],
                hull.half_beam_at(z) * 0.985,
            )
        })
        .collect();
    sweep(&pts, 14, [1.0, DECK_CROWN, 1.0], [0.0, 0.5], m, 0.0)
}

/// Skeg, and on an inboard boat a propeller and a rudder - on show, because
/// she hovers. The skeg is what the derived draft's skeg allowance is. Every
/// screw boat hangs the same one under her counter - the runabout (#1372)
/// and the steam tug (#1370) - painted in her own `antifoul` and `bronze`.
pub(super) fn underbody(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    antifoul: &SovereignMaterialSettings,
    bronze: &SovereignMaterialSettings,
    x0: f32,
    prop: bool,
) {
    let l = hull.loa;
    let t = hull.transom_z();
    let k_t = hull.keel_at(t + l * 0.02);
    let deep = -hull.draft;
    let skeg = [
        (
            [x0, hull.keel_at(-0.05 * l) + l * 0.004, -0.05 * l],
            l * 0.004,
        ),
        (
            [x0, (k_t + deep) * 0.5 + l * 0.006, t + l * 0.10],
            (k_t - deep) * 0.5,
        ),
        (
            [x0, (k_t + deep) * 0.5 + l * 0.004, t + l * 0.045],
            (k_t - deep) * 0.5,
        ),
    ];
    kids.push(sweep(&skeg, 8, [0.18, 1.0, 1.0], [0.0, 1.0], antifoul, 0.0));
    if !prop {
        return;
    }
    let py = (k_t + deep) * 0.5 - l * 0.004;
    let pz = t + l * 0.030;
    let r = (k_t - deep) * 0.55;
    kids.push(turned(
        &[
            (r * 0.25, -l * 0.010),
            (r, -l * 0.004),
            (r, l * 0.004),
            (r * 0.25, l * 0.010),
        ],
        12,
        false,
        bronze,
        [x0, py, pz],
        quat_x(std::f32::consts::FRAC_PI_2),
        0.0,
    ));
    let rudder = [
        ([x0, k_t + l * 0.010, t + l * 0.006], l * 0.012),
        ([x0, deep + l * 0.004, t + l * 0.010], l * 0.012),
    ];
    kids.push(sweep(&rudder, 8, [0.25, 1.0, 1.0], [0.0, 1.0], bronze, 0.0));
}
