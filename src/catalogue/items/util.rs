//! Shared construction vocabulary for primitive-built catalogue
//! entries (lighthouse, stone circle, ziggurat, observatory).
//!
//! The shape-grammar entries (villa, castle, watchtower, temple)
//! don't need these — their geometry comes from the grammar
//! interpreter. The primitive entries assemble `Generator` trees by
//! hand, and these helpers keep that assembly at the "place a tapered
//! cylinder here" altitude instead of struct-literal plumbing.

use crate::pds::{
    Fp, Fp3, Fp4, Generator, GeneratorKind, SovereignMaterialSettings, TransformData,
};

/// Wrap a kind into a childless node at `translation` / `rotation`.
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

pub(super) fn id_quat() -> Fp4 {
    Fp4([0.0, 0.0, 0.0, 1.0])
}

/// Assemble a flat list of prims, each positioned in the prop's plain
/// ground-relative world frame, into one generator: the first prim becomes
/// the root (keeping its transform), and every other prim is reparented
/// under it with its translation rebased into the root's local frame.
///
/// Spawned generator children inherit the root's transform (Bevy
/// `add_child`), so without this rebase a child authored at world `y = 2`
/// under a root sitting at `y = 0.5` would render at `y = 2.5`. Authoring
/// against this helper lets each prop's geometry read in one consistent
/// world frame instead of threading a per-file offset through every piece.
pub(super) fn assemble(mut prims: Vec<Generator>) -> Generator {
    let mut root = prims.remove(0);
    let [rx, ry, rz] = root.transform.translation.0;
    for mut p in prims {
        let t = &mut p.transform.translation.0;
        t[0] -= rx;
        t[1] -= ry;
        t[2] -= rz;
        root.children.push(p);
    }
    root
}

/// Rotation around X — tilts ramps and dome slits.
pub(super) fn quat_x(angle_rad: f32) -> Fp4 {
    let half = angle_rad * 0.5;
    Fp4([half.sin(), 0.0, 0.0, half.cos()])
}

/// Rotation around Y — yaws monoliths to face the circle centre.
pub(super) fn quat_y(angle_rad: f32) -> Fp4 {
    let half = angle_rad * 0.5;
    Fp4([0.0, half.sin(), 0.0, half.cos()])
}

/// Cuboid with an optional taper (`0.0` = straight, `1.0` = pyramid).
pub(super) fn cuboid_tapered(
    size: [f32; 3],
    taper: f32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Cuboid {
        size: Fp3(size),
        solid: false,
        material,
        twist: Fp(0.0),
        taper: Fp(taper),
        bend: Fp3([0.0, 0.0, 0.0]),
    }
}

pub(super) fn cylinder_tapered(
    radius: f32,
    height: f32,
    resolution: u32,
    taper: f32,
    material: SovereignMaterialSettings,
) -> GeneratorKind {
    GeneratorKind::Cylinder {
        radius: Fp(radius),
        height: Fp(height),
        resolution,
        solid: false,
        material,
        twist: Fp(0.0),
        taper: Fp(taper),
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
        minor_resolution: 10,
        major_resolution: 28,
        solid: false,
        material,
        twist: Fp(0.0),
        taper: Fp(0.0),
        bend: Fp3([0.0, 0.0, 0.0]),
    }
}

/// Mark a primitive kind solid so the spawner attaches its matching
/// collider — structural pieces players can stand on or bump into.
/// Decorative trim (railings, orbs, lamps) stays non-solid.
pub(super) fn solid(mut kind: GeneratorKind) -> GeneratorKind {
    match &mut kind {
        GeneratorKind::Cuboid { solid, .. }
        | GeneratorKind::Sphere { solid, .. }
        | GeneratorKind::Cylinder { solid, .. }
        | GeneratorKind::Capsule { solid, .. }
        | GeneratorKind::Cone { solid, .. }
        | GeneratorKind::Torus { solid, .. } => *solid = true,
        _ => {}
    }
    kind
}

/// Shared foundation material — neutral rough-cut stone that sits
/// under any of the structure palettes.
pub(super) fn foundation_mat() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.45, 0.43, 0.40]),
        roughness: Fp(0.95),
        uv_scale: Fp(2.0),
        texture: crate::pds::SovereignTextureConfig::Rock(
            crate::pds::SovereignRockConfig::default(),
        ),
        ..Default::default()
    }
}

/// Reveal height of a foundation above the entry's ground plane — a
/// visible plinth course rather than a flush slab.
const FOUNDATION_REVEAL: f32 = 0.15;

/// Rectangular buried foundation: a solid stone block whose top sits
/// [`FOUNDATION_REVEAL`] above the entry's y=0 ground plane and which
/// extends `depth` below it, so a terrain-snapped structure on a
/// slope shows plinth instead of daylight under its downhill edge.
/// `center` is the block's XZ centre in the entry's local frame
/// (footprint/2 for the corner-origin shape-grammar entries, the
/// origin for the centred primitive entries).
pub(super) fn foundation_block(
    size_x: f32,
    size_z: f32,
    center: [f32; 2],
    depth: f32,
) -> Generator {
    let height = depth + FOUNDATION_REVEAL;
    prim(
        solid(cuboid_tapered(
            [size_x, height, size_z],
            0.0,
            foundation_mat(),
        )),
        [center[0], FOUNDATION_REVEAL - height * 0.5, center[1]],
        id_quat(),
    )
}

/// Round sibling of [`foundation_block`] for the drum/tower entries,
/// centred on the entry origin.
pub(super) fn foundation_disc(radius: f32, depth: f32) -> Generator {
    let height = depth + FOUNDATION_REVEAL;
    prim(
        solid(cylinder_tapered(radius, height, 24, 0.0, foundation_mat())),
        [0.0, FOUNDATION_REVEAL - height * 0.5, 0.0],
        id_quat(),
    )
}

/// Strong self-lit material — lamps, orbs, finials.
pub(super) fn glow(color: [f32; 3], strength: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        emission_color: Fp3(color),
        emission_strength: Fp(strength),
        roughness: Fp(0.4),
        metallic: Fp(0.1),
        ..Default::default()
    }
}

/// Assert that `sanitize_generator` leaves a primitive-built entry
/// geometrically untouched. Rotations are compared with an epsilon
/// because the sanitiser renormalises every quaternion, which can
/// shift the last ulp of an already-normalised rotation; everything
/// else must be bit-identical.
#[cfg(test)]
pub(super) fn assert_sanitize_stable(built: &Generator, name: &str) {
    fn tree_eq(a: &Generator, b: &Generator, name: &str) {
        assert_eq!(a.kind, b.kind, "{name}: kind rewritten by sanitiser");
        assert_eq!(
            a.transform.translation, b.transform.translation,
            "{name}: translation rewritten"
        );
        assert_eq!(
            a.transform.scale, b.transform.scale,
            "{name}: scale rewritten"
        );
        for i in 0..4 {
            assert!(
                (a.transform.rotation.0[i] - b.transform.rotation.0[i]).abs() < 1e-5,
                "{name}: rotation rewritten beyond renormalisation: {:?} vs {:?}",
                a.transform.rotation,
                b.transform.rotation
            );
        }
        assert_eq!(a.children.len(), b.children.len(), "{name}: child dropped");
        for (ca, cb) in a.children.iter().zip(b.children.iter()) {
            tree_eq(ca, cb, name);
        }
    }
    let mut sanitized = built.clone();
    crate::pds::sanitize_generator(&mut sanitized);
    tree_eq(built, &sanitized, name);
}
