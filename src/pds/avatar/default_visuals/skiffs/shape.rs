//! The shape vocabulary the land craft are drawn in (#1377, #1374): a board,
//! a thin swept line, a turned part (bored and cut, or solid) and a sweep
//! that pre-divides its path by its own node scale. The wagon drew the first
//! four and the roadster the sweep; the dune buggy draws all five, so they
//! live beside the [`BodyPlan`](super::BodyPlan) rather than inside one
//! type, as the boats' own shape vocabulary does (#1373).
//!
//! The roadster keeps its own `line` and `turned`, which take their material
//! by value: it was built before these and its dumps are pinned byte for
//! byte.

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;

use super::super::common::{bevel, id_quat, lathe, prim, quat_xyzw, spine, with_cut};
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
