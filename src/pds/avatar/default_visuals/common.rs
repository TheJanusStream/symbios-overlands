//! Shared primitive vocabulary for the avatar assemblers and parts.
//!
//! Pure geometry plumbing: axis-rotation quaternion helpers, primitive-kind
//! constructors (torture triple zeroed), the DID-pfp banner, and the
//! assembler placement helpers ([`offset`] / [`offset_rot`]). Material
//! *finish* lives in [`crate::seeded_defaults::MaterialKit`]; both the
//! assemblers ([`super`]) and the part catalogue
//! ([`crate::pds::avatar::parts`]) build from this bin so geometry plumbing
//! lives in exactly one place.

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::{AlphaModeKind, Generator, GeneratorKind, SignSource};
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::{Fp, Fp2, Fp3, Fp4, TransformData};

// ---------------------------------------------------------------------------
// Quaternion helpers
// ---------------------------------------------------------------------------

/// Rotation around X as a normalised `[x, y, z, w]` quaternion — points a
/// cone apex (local +Y) along ±Z, e.g. a forward-pointing prow ram.
pub(crate) fn quat_x(angle_rad: f32) -> [f32; 4] {
    let half = angle_rad * 0.5;
    [half.sin(), 0.0, 0.0, half.cos()]
}

/// Rotation around Y as a normalised `[x, y, z, w]` quaternion — rolls a Sign
/// panel about its own normal so its textured image sits upright.
pub(crate) fn quat_y(angle_rad: f32) -> [f32; 4] {
    let half = angle_rad * 0.5;
    [0.0, half.sin(), 0.0, half.cos()]
}

/// Rotation around Z — stands the pfp Sign plane up in YZ and lays wheel
/// cylinders onto their axle.
pub(crate) fn quat_z(angle_rad: f32) -> [f32; 4] {
    let half = angle_rad * 0.5;
    [0.0, 0.0, half.sin(), half.cos()]
}

/// Hamilton product of two `[x, y, z, w]` quaternions: the rotation that
/// applies `b` first and then `a`. Used to compose the pfp banner's
/// stand-up and image-upright rolls into one transform.
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
        solid: false,
        material,
        twist: Fp(0.0),
        taper: Fp(0.0),
        bend: Fp3([0.0, 0.0, 0.0]),
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
        solid: false,
        material,
        twist: Fp(0.0),
        taper: Fp(0.0),
        bend: Fp3([0.0, 0.0, 0.0]),
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
        solid: false,
        material,
        twist: Fp(0.0),
        taper: Fp(0.0),
        bend: Fp3([0.0, 0.0, 0.0]),
    }
}

pub(crate) fn capsule(
    radius: f32,
    length: f32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Capsule {
        radius: Fp(radius),
        length: Fp(length),
        latitudes: 8,
        longitudes: 16,
        solid: false,
        material,
        twist: Fp(0.0),
        taper: Fp(0.0),
        bend: Fp3([0.0, 0.0, 0.0]),
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
        solid: false,
        material,
        twist: Fp(0.0),
        taper: Fp(0.0),
        bend: Fp3([0.0, 0.0, 0.0]),
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
        solid: false,
        material,
        twist: Fp(0.0),
        taper: Fp(0.0),
        bend: Fp3([0.0, 0.0, 0.0]),
    }
}

/// Stamp a torture triple onto a parametric primitive kind for organic
/// shaping. Semantics live in `crate::world_builder::prim`: `twist` is
/// radians of Y-rotation across the height, `taper` scales X/Z toward the
/// top (`0.5` → half-width crown, negative flares outward), `bend` displaces
/// the top quadratically on world X/Z. Non-primitive kinds pass through.
pub(crate) fn with_torture(
    mut kind: GeneratorKind,
    new_twist: f32,
    new_taper: f32,
    new_bend: [f32; 3],
) -> GeneratorKind {
    match &mut kind {
        GeneratorKind::Cuboid {
            twist, taper, bend, ..
        }
        | GeneratorKind::Sphere {
            twist, taper, bend, ..
        }
        | GeneratorKind::Cylinder {
            twist, taper, bend, ..
        }
        | GeneratorKind::Capsule {
            twist, taper, bend, ..
        }
        | GeneratorKind::Cone {
            twist, taper, bend, ..
        }
        | GeneratorKind::Torus {
            twist, taper, bend, ..
        } => {
            *twist = Fp(new_twist);
            *taper = Fp(new_taper);
            *bend = Fp3(new_bend);
        }
        _ => {}
    }
    kind
}

// ---------------------------------------------------------------------------
// Pfp banner
// ---------------------------------------------------------------------------

/// Pastel of an accent colour — 65 % white. Used as the pfp banner's base
/// tint so the panel reads as a heraldic flag while the image is still
/// loading (or the account has no avatar) instead of a stark white
/// rectangle. The Sign material *multiplies* its texture by `base_color`, so
/// the mix is kept light: a loaded pfp picks up only a mild dye.
pub(crate) fn pastel(color: [f32; 3]) -> [f32; 3] {
    [
        0.65 + 0.35 * color[0],
        0.65 + 0.35 * color[1],
        0.65 + 0.35 * color[2],
    ]
}

/// Square Sign panel showing the owner's bsky profile picture, flown as a
/// heraldic side banner (face normal ±X) at `translation`. `size` is the side
/// length: the panel is kept **square** so the profile picture is never
/// stretched. `tint` is the fallback colour — pass [`pastel`] of an accent so
/// an unloaded banner still belongs to the avatar's palette.
///
/// The Sign mesh is a plane in local XZ (normal +Y). Two rolls are baked in:
/// `quat_y(-FRAC_PI_2)` turns the image upright within the panel, and
/// `quat_z(FRAC_PI_2)` (applied last) stands the panel vertical with its
/// normal on ±X. Without the Y roll the picture rides 90° on its side.
pub(crate) fn pfp_banner(did: &str, size: f32, translation: [f32; 3], tint: [f32; 3]) -> Generator {
    prim(
        GeneratorKind::Sign {
            source: SignSource::DidPfp {
                did: did.to_owned(),
            },
            size: Fp2([size, size]),
            uv_repeat: Fp2([1.0, 1.0]),
            uv_offset: Fp2([0.0, 0.0]),
            material: SovereignMaterialSettings {
                base_color: Fp3(tint),
                roughness: Fp(0.6),
                metallic: Fp(0.0),
                ..Default::default()
            },
            double_sided: true,
            alpha_mode: AlphaModeKind::Opaque,
            unlit: true,
        },
        translation,
        quat_xyzw(quat_mul(quat_z(FRAC_PI_2), quat_y(-FRAC_PI_2))),
    )
}

// ---------------------------------------------------------------------------
// Assembler placement
// ---------------------------------------------------------------------------

/// Offset a built part to a joint anchor by adding the anchor to the part
/// root's intrinsic translation (which carries the part's own offset from its
/// attachment pivot — e.g. an arm hanging below the shoulder). Rotation and
/// scale on the part root are preserved.
pub(crate) fn offset(mut part: Generator, anchor: [f32; 3]) -> Generator {
    let t = part.transform.translation.0;
    part.transform.translation = Fp3([anchor[0] + t[0], anchor[1] + t[1], anchor[2] + t[2]]);
    part
}

/// [`offset`] plus a rotation set on the part root — for slots the assembler
/// orients (a wheel laid on its axle, an airship fin). Parts build at identity
/// rotation in their local frame, so setting it here is safe.
pub(crate) fn offset_rot(part: Generator, anchor: [f32; 3], rotation: Fp4) -> Generator {
    let mut p = offset(part, anchor);
    p.transform.rotation = rotation;
    p
}
