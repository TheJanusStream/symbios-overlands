//! Shared primitive vocabulary for the avatar assemblers and parts.
//!
//! Pure geometry plumbing: axis-rotation quaternion helpers, primitive-kind
//! constructors (torture triple zeroed), and the assembler placement
//! helpers ([`offset`] / [`offset_rot`]). Material
//! *finish* lives in [`crate::seeded_defaults::MaterialKit`]; both the
//! assemblers ([`super`]) and the part catalogue
//! ([`crate::pds::avatar::parts`]) build from this bin so geometry plumbing
//! lives in exactly one place.

use crate::pds::PrimCommon;
use crate::pds::generator::{Generator, GeneratorKind, LathePoint, SpinePoint};
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::{Fp, Fp2, Fp3, Fp4, TransformData};

// ---------------------------------------------------------------------------
// Quaternion helpers
// ---------------------------------------------------------------------------

/// Rotation around X as a normalised `[x, y, z, w]` quaternion - points a
/// cone apex (local +Y) along ±Z, e.g. a forward-pointing prow ram.
pub(crate) fn quat_x(angle_rad: f32) -> [f32; 4] {
    let half = angle_rad * 0.5;
    [half.sin(), 0.0, 0.0, half.cos()]
}

/// Rotation around Y as a normalised `[x, y, z, w]` quaternion - yaws a part
/// in plan view, e.g. the root flip that turns a chassis to face −Z.
pub(crate) fn quat_y(angle_rad: f32) -> [f32; 4] {
    let half = angle_rad * 0.5;
    [0.0, half.sin(), 0.0, half.cos()]
}

/// Rotation around Z - lays wheel cylinders onto their axle and rolls hair
/// tufts / stabiliser fins off vertical.
pub(crate) fn quat_z(angle_rad: f32) -> [f32; 4] {
    let half = angle_rad * 0.5;
    [0.0, 0.0, half.sin(), half.cos()]
}

/// Hamilton product of two `[x, y, z, w]` quaternions: the rotation that
/// applies `b` first and then `a`. Used to compose two axis rotations into
/// one transform, e.g. laying a wheel ring flat on its axle.
pub(crate) fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

pub(crate) fn quat_xyzw(q: [f32; 4]) -> Fp4 {
    Fp4(q)
}

/// Identity rotation for transforms that don't turn their child.
pub(crate) fn id_quat() -> Fp4 {
    Fp4([0.0, 0.0, 0.0, 1.0])
}

// ---------------------------------------------------------------------------
// Node assembly
// ---------------------------------------------------------------------------

/// Wrap a [`GeneratorKind`] into a childless [`Generator`] node at
/// `translation` with `rotation`. Children are pushed onto the returned node
/// by the caller where needed.
pub(crate) fn prim(kind: GeneratorKind, translation: [f32; 3], rotation: Fp4) -> Generator {
    Generator {
        kind,
        transform: TransformData {
            translation: Fp3(translation),
            rotation,
            scale: Fp3([1.0, 1.0, 1.0]),
        },
        children: Vec::new(),
        audio: crate::pds::SovereignAudioConfig::None,
    }
}

// ---------------------------------------------------------------------------
// Primitive-kind constructors (torture triple zeroed)
// ---------------------------------------------------------------------------

pub(crate) fn cuboid(size: [f32; 3], material: SovereignMaterialSettings) -> GeneratorKind {
    GeneratorKind::Cuboid {
        size: Fp3(size),
        common: PrimCommon::with_material(material),
    }
}

pub(crate) fn sphere(
    radius: f32,
    resolution: u32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Sphere {
        radius: Fp(radius),
        resolution,
        common: PrimCommon::with_material(material),
    }
}

pub(crate) fn cylinder(
    radius: f32,
    height: f32,
    resolution: u32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Cylinder {
        radius: Fp(radius),
        height: Fp(height),
        resolution,
        common: PrimCommon::with_material(material),
    }
}

// The smooth-blend SDF vocabulary (`blob_group` and its elements) lived here
// until #1363. It existed for the boat hull, which was a blob iso-surface
// nobody could predict - which is exactly why rails floated off it and every
// mount needed an embed fudge factor - and the redesign's owner decision 4 is
// that machines are built from swept and turned shapes, never blobs, because a
// blob reads as organic. Nothing left in this crate assembles one, so it is
// gone rather than kept warm; git history has it if a genuinely organic avatar
// part ever wants it back.

pub(crate) fn spine(
    points: &[([f32; 3], f32)],
    resolution: u32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Spine {
        points: points
            .iter()
            .map(|(p, r)| SpinePoint {
                position: Fp3(*p),
                radius: Fp(*r),
            })
            .collect(),
        resolution,
        samples_per_segment: 8,
        common: PrimCommon::with_material(material),
    }
}

/// Profile-revolve prim (#689): `points` are `(radius, height)` silhouette
/// stations bottom-to-top; `smooth` splines them.
#[allow(dead_code)]
pub(crate) fn lathe(
    points: &[(f32, f32)],
    resolution: u32,
    smooth: bool,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Lathe {
        points: points
            .iter()
            .map(|(r, h)| LathePoint {
                radius: Fp(*r),
                height: Fp(*h),
            })
            .collect(),
        resolution,
        smooth,
        common: PrimCommon::with_material(material),
    }
}

pub(crate) fn cone(
    radius: f32,
    height: f32,
    resolution: u32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Cone {
        radius: Fp(radius),
        height: Fp(height),
        resolution,
        common: PrimCommon::with_material(material),
    }
}

/// Barr superellipsoid (#687): a rounded box that morphs from a hard box
/// (`exponent → 0.2`) through a soft pillow to a true ellipsoid (`1.0`) toward
/// a pinched octahedron (`2.5`). `exponent_ns` shapes the vertical (latitude)
/// profile, `exponent_ew` the horizontal (longitude) cross-section - so a car
/// body panel wants ~0.5 (rounded edges, near-flat faces). The soft-surface
/// counterpart of [`cuboid`] where a sheared box reads too hard.
pub(crate) fn superellipsoid(
    half_extents: [f32; 3],
    exponent_ns: f32,
    exponent_ew: f32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Superellipsoid {
        half_extents: Fp3(half_extents),
        common: PrimCommon::with_material(material),
        exponent_ns: Fp(exponent_ns),
        exponent_ew: Fp(exponent_ew),
        latitudes: 16,
        longitudes: 24,
    }
}

pub(crate) fn torus(
    minor_radius: f32,
    major_radius: f32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Torus {
        minor_radius: Fp(minor_radius),
        major_radius: Fp(major_radius),
        minor_resolution: 12,
        major_resolution: 24,
        common: PrimCommon::with_material(material),
    }
}

/// A helical tube - spring / screw / spiral (`Helix` prim, #527). `radius` is
/// the helix radius, `tube` the wire thickness, `pitch` the vertical rise per
/// full turn, `turns` the revolution count. Laid along +Y; rotate to lay it
/// along the travel axis for a screw propeller.
pub(crate) fn helix(
    radius: f32,
    tube: f32,
    pitch: f32,
    turns: f32,
    resolution: u32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Helix {
        radius: Fp(radius),
        tube_radius: Fp(tube),
        pitch: Fp(pitch),
        turns: Fp(turns),
        resolution,
        common: PrimCommon::with_material(material),
    }
}

/// Stamp a torture triple onto a parametric primitive kind for organic
/// shaping. Semantics live in `crate::world_builder::prim`: `twist` is
/// radians of Y-rotation across the height, `taper` scales X/Z toward the
/// top (`0.5` → half-width crown, negative flares outward), `bend` displaces
/// the top quadratically on world X/Z. The scalar `new_taper` sets a uniform
/// (X == Z) taper; author per-axis taper or an S-bend by building
/// [`TortureParams`](crate::pds::TortureParams) directly. Non-primitive
/// kinds pass through.
pub(crate) fn with_torture(
    mut kind: GeneratorKind,
    new_twist: f32,
    new_taper: f32,
    new_bend: [f32; 3],
) -> GeneratorKind {
    if let Some(t) = kind.torture_mut() {
        t.twist = Fp(new_twist);
        t.taper = Fp2([new_taper, new_taper]);
        t.bend = Fp3(new_bend);
    }
    kind
}

/// Per-axis shaping for box bodies - [`with_torture`]'s sibling for when the
/// X and Z taper differ (a cabin greenhouse that narrows more across than
/// fore-aft) or a top-shear lean is wanted. `taper` scales `[x, z]` toward the
/// top (`1 - taper·t`, so `0.0` = straight, positive draws the top in,
/// negative flares it); `bend` is the quadratic top displacement; `shear`
/// slides the top linearly in `[x, z]` (a parallelepiped, edges stay straight).
/// On an 8-corner cuboid this turns the box into a frustum / wedge / leaning
/// prism - the cheapest de-blocking deform. Non-primitive kinds pass through.
pub(crate) fn with_shape(
    mut kind: GeneratorKind,
    taper: [f32; 2],
    bend: [f32; 3],
    shear: [f32; 2],
) -> GeneratorKind {
    if let Some(t) = kind.torture_mut() {
        t.taper = Fp2(taper);
        t.bend = Fp3(bend);
        t.shear = Fp2(shear);
    }
    kind
}

/// Stamp the SL-style topology cuts onto a swept primitive (Sphere / Cylinder /
/// Cone / Torus / Tube): `path_cut` (`[begin, end]` kept angular fraction),
/// `profile_cut` (`[begin, end]` kept latitude band - domes / bowls), and
/// `hollow` (bore fraction). Non-swept kinds pass through unchanged. Honoured
/// by the unified sweep mesher in `crate::world_builder::prim`.
pub(crate) fn with_cut(
    mut kind: GeneratorKind,
    path_cut: [f32; 2],
    profile_cut: [f32; 2],
    hollow: f32,
) -> GeneratorKind {
    if let Some(t) = kind.torture_mut() {
        t.path_cut = Fp2(path_cut);
        t.profile_cut = Fp2(profile_cut);
        t.hollow = Fp(hollow);
    }
    kind
}

// ---------------------------------------------------------------------------
// Assembler placement
// ---------------------------------------------------------------------------

/// Offset a built part to a joint anchor by adding the anchor to the part
/// root's intrinsic translation (which carries the part's own offset from its
/// attachment pivot - e.g. an arm hanging below the shoulder). Rotation and
/// scale on the part root are preserved.
pub(crate) fn offset(mut part: Generator, anchor: [f32; 3]) -> Generator {
    let t = part.transform.translation.0;
    part.transform.translation = Fp3([anchor[0] + t[0], anchor[1] + t[1], anchor[2] + t[2]]);
    part
}

/// [`offset`] plus a rotation set on the part root - for slots the assembler
/// orients (a wheel laid on its axle, an airship fin). Parts build at identity
/// rotation in their local frame, so setting it here is safe.
pub(crate) fn offset_rot(part: Generator, anchor: [f32; 3], rotation: Fp4) -> Generator {
    let mut p = offset(part, anchor);
    p.transform.rotation = rotation;
    p
}
