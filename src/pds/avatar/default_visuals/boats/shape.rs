//! The shape vocabulary every finless boat is drawn in (#1372, #1373): a
//! sweep that pre-divides its path by its own node scale, a thin line, a
//! rounded panel, a turned part, and the crowned deck run over a hull's
//! stations. The runabout drew them first; the scow draws the same parts, so
//! they live beside the [`HullProfile`] rather than inside one type.
//!
//! The sloop keeps her own (`sloop/hull.rs`): she was built before these
//! and her dumps are pinned byte for byte.

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;

use super::super::common::{bevel, cuboid, id_quat, lathe, prim, quat_xyzw, spine, with_cut};
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
