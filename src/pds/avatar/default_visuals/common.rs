//! Shared builder vocabulary for the per-family default avatar
//! visuals.
//!
//! Every family builder (boat / airship / humanoid / skiff) assembles
//! its silhouette from the same parts bin: PBR material presets,
//! axis-rotation quaternion helpers, primitive-kind constructors with
//! the torture triple zeroed, and the DID-pfp banner. Centralising
//! them keeps each family file focused on *layout* — where the pieces
//! go — rather than on `Generator` struct plumbing.

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::{AlphaModeKind, Generator, GeneratorKind, SignSource};
use crate::pds::texture::{
    SovereignMaterialSettings, SovereignPlankConfig, SovereignTextureConfig,
};
use crate::pds::types::{Fp, Fp2, Fp3, Fp4, TransformData};

// ---------------------------------------------------------------------------
// Material presets
// ---------------------------------------------------------------------------

/// Painted-metal body panel — the workhorse hull/deck material.
pub(super) fn metal_mat(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        metallic: Fp(0.4),
        roughness: Fp(0.45),
        ..Default::default()
    }
}

/// Polished brass — fittings, funnels, ornaments.
pub(super) fn brass_mat(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        metallic: Fp(0.7),
        roughness: Fp(0.35),
        ..Default::default()
    }
}

/// Self-lit jewel — finials, lamps, glowing eyes.
pub(super) fn glow_mat(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        metallic: Fp(0.4),
        roughness: Fp(0.40),
        emission_color: Fp3(color),
        emission_strength: Fp(5.0),
        ..Default::default()
    }
}

/// Matte fabric — clothing, envelope canvas.
pub(super) fn cloth_mat(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        metallic: Fp(0.0),
        roughness: Fp(0.85),
        ..Default::default()
    }
}

/// Skin — slightly softer than cloth so faces catch the sun.
pub(super) fn skin_mat(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        metallic: Fp(0.0),
        roughness: Fp(0.65),
        ..Default::default()
    }
}

/// Smokestack / exhaust funnel — a darkened cut of the avatar's metal
/// tone. The curated hair-colour table includes platinum and blonde
/// entries that read as washed-out white pipes when used raw; scaling
/// toward black keeps the funnel in-palette but unmistakably sooty
/// iron.
pub(super) fn funnel_mat(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([color[0] * 0.45, color[1] * 0.45, color[2] * 0.45]),
        metallic: Fp(0.75),
        roughness: Fp(0.45),
        ..Default::default()
    }
}

/// Tire rubber — the avatar's metal tone crushed nearly to black.
/// Tires must read dark regardless of what the curated hair table
/// rolled; a platinum draw used raw produced cream-white rubber.
pub(super) fn rubber_mat(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([
            0.04 + color[0] * 0.18,
            0.04 + color[1] * 0.18,
            0.04 + color[2] * 0.18,
        ]),
        metallic: Fp(0.0),
        roughness: Fp(0.95),
        ..Default::default()
    }
}

/// Glassy canopy / visor.
pub(super) fn glass_mat(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        metallic: Fp(0.9),
        roughness: Fp(0.08),
        ..Default::default()
    }
}

/// Procedural wood-plank deck. Light/dark plank tones are derived
/// from the accent colour so the deck stays on the avatar's palette
/// while reading as boards instead of paint.
pub(super) fn plank_mat(color: [f32; 3]) -> SovereignMaterialSettings {
    let light = [
        (color[0] * 1.15).min(1.0),
        (color[1] * 1.15).min(1.0),
        (color[2] * 1.15).min(1.0),
    ];
    let dark = [color[0] * 0.55, color[1] * 0.55, color[2] * 0.55];
    SovereignMaterialSettings {
        base_color: Fp3(color),
        metallic: Fp(0.05),
        roughness: Fp(0.7),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3(light),
            color_wood_dark: Fp3(dark),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Quaternion helpers
// ---------------------------------------------------------------------------

/// Normalised `[x, y, z, w]` quaternion from a rotation around X.
/// Lays capsules on their side and points cone apexes along ±Z.
pub(super) fn quat_x(angle_rad: f32) -> [f32; 4] {
    let half = angle_rad * 0.5;
    [half.sin(), 0.0, 0.0, half.cos()]
}

/// Sister of [`quat_x`] around Y — rolls a Sign panel about its own
/// normal so its textured image sits upright after the panel is stood up.
pub(super) fn quat_y(angle_rad: f32) -> [f32; 4] {
    let half = angle_rad * 0.5;
    [0.0, half.sin(), 0.0, half.cos()]
}

/// Sister of [`quat_x`] around Z — stands the pfp Sign plane up in YZ
/// and rolls wheel cylinders onto their rims.
pub(super) fn quat_z(angle_rad: f32) -> [f32; 4] {
    let half = angle_rad * 0.5;
    [0.0, 0.0, half.sin(), half.cos()]
}

/// Hamilton product of two `[x, y, z, w]` quaternions: the rotation that
/// applies `b` first and then `a`. Used to compose the pfp banner's
/// stand-up and image-upright rolls into one transform.
pub(super) fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

pub(super) fn quat_xyzw(q: [f32; 4]) -> Fp4 {
    Fp4(q)
}

/// Identity rotation for transforms that don't turn their child.
pub(super) fn id_quat() -> Fp4 {
    Fp4([0.0, 0.0, 0.0, 1.0])
}

// ---------------------------------------------------------------------------
// Node assembly
// ---------------------------------------------------------------------------

/// Wrap a [`GeneratorKind`] into a childless [`Generator`] node at
/// `translation` with `rotation`. Children are pushed onto the
/// returned node by the caller where needed.
pub(super) fn prim(kind: GeneratorKind, translation: [f32; 3], rotation: Fp4) -> Generator {
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

pub(super) fn cuboid(size: [f32; 3], material: SovereignMaterialSettings) -> GeneratorKind {
    GeneratorKind::Cuboid {
        size: Fp3(size),
        solid: false,
        material,
        twist: Fp(0.0),
        taper: Fp(0.0),
        bend: Fp3([0.0, 0.0, 0.0]),
    }
}

pub(super) fn sphere(
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

pub(super) fn cylinder(
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

pub(super) fn capsule(
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

pub(super) fn cone(
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

pub(super) fn torus(
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

/// Stamp a torture triple onto any parametric primitive kind. The
/// semantics live in `crate::world_builder::prim`: `twist` is radians
/// of Y-rotation across the height, `taper` scales X/Z toward the top
/// (`0.5` → half-width crown, negative flares), `bend` displaces the
/// top quadratically on world X/Z. Non-primitive kinds pass through
/// untouched.
pub(super) fn with_torture(
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

/// Pastel of an accent colour — 65 % white. Used as the pfp banner's
/// base tint so the panel reads as a heraldic flag while the image is
/// still loading (or the account has no avatar) instead of a stark
/// white rectangle. The Sign material *multiplies* its texture by
/// `base_color`, so the mix is kept light: a loaded pfp picks up only
/// a mild dye, never a full recolour.
pub(super) fn pastel(color: [f32; 3]) -> [f32; 3] {
    [
        0.65 + 0.35 * color[0],
        0.65 + 0.35 * color[1],
        0.65 + 0.35 * color[2],
    ]
}

/// Square Sign panel showing the owner's bsky profile picture, flown as
/// a heraldic side banner (face normal ±X) at `translation`. `size` is
/// the side length: the panel is kept **square** so the profile picture
/// is never stretched. `double_sided` renders both faces and `unlit`
/// keeps the pfp legible regardless of sun angle. `tint` is the fallback
/// colour — pass [`pastel`] of an accent so an unloaded banner still
/// belongs to the avatar's palette.
///
/// The Sign mesh is a plane in local XZ (normal +Y) with the image's
/// right→local +X and down→local +Z. Two rolls are baked in:
/// `quat_y(-FRAC_PI_2)` turns the image upright within the panel, and
/// `quat_z(FRAC_PI_2)` (applied last) stands the panel vertical with its
/// normal on ±X. Without the Y roll the picture rides 90° on its side.
pub(super) fn pfp_banner(did: &str, size: f32, translation: [f32; 3], tint: [f32; 3]) -> Generator {
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
