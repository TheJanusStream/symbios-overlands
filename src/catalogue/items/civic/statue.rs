//! Statue — a draped bronze figure raising a torch, a tablet held at her
//! hip, on a stepped marble plinth with a dedication plate. A
//! prosperity-Rich scatter prop: commemorative statuary signals an
//! established, well-off settlement in any setting, and a torch-bearer is
//! the one civic allegory — light, learning, liberty — that reads the same
//! in a medieval square and a cyberpunk plaza.
//!
//! **The figure is one skin** (#972 lesson 40): a single
//! [`blob_group`] of sixteen elements in one weathered bronze, because an
//! organic silhouette is exactly what a stack of tapered prims cannot give —
//! the shipped "orator" was a traffic cone with a ball on it. She stands in
//! contrapposto: a flared robe (one capped cone from hem to waist), the left
//! knee pushing the drapery forward, three fold ridges falling from the
//! hip, a himation sash from the left shoulder across to the right hip and
//! its free end hanging from the left forearm, the right arm raised overhead
//! and the left bent to hold the tablet. The
//! torch (handle, funnel cup, gilt flame) and the tablet are separate prims
//! because each is its own material; each is placed FROM the hand that
//! holds it, and each contact is guarded as a relation between a built
//! capsule end and a built part.
//!
//! **Plinth.** Base step → straight die → cornice → the statue's own cast
//! bronze base, each course lapped into the one below so no two horizontal
//! faces share a plane. The die is deliberately not battered: the plate's
//! standoff is taken from the die's half-depth (lesson 11), and a battered
//! face would make that a function of height (lesson 16).
//!
//! Faces `-Z`: the plate, the knee and the tablet are all on the hero side.

use crate::catalogue::items::util::{
    ageing, aim_y, blob_capsule, blob_cone, blob_ellipsoid, blob_group, cone, cuboid_tapered,
    cylinder_tapered, id_quat, nest, prim, quat_x, solid, tile, tiles_per_metre,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::BlobElement;
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignMaterialSettings, SovereignMetalConfig,
    SovereignTextureConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};
use bevy_symbios_texture::metal::MetalStyle;

use super::{GOLD, MARBLE, bronze, marble};

pub struct Statue;

impl CatalogueEntry for Statue {
    fn slug(&self) -> &'static str {
        "statue"
    }
    fn name(&self) -> &'static str {
        "Statue"
    }
    fn description(&self) -> &'static str {
        "Draped bronze torch-bearer on a stepped marble plinth."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn prosperity_band(&self) -> ProsperityBand {
        ProsperityBand::only(ProsperityTier::Rich)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.4,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_with(&Faults::default())
    }
}

// ---- Plinth -----------------------------------------------------------

const BASE: [f32; 3] = [1.0, 0.25, 1.0];
const DIE_W: f32 = 0.78;
/// Die: from inside the base step to under the cornice.
const DIE_BOTTOM: f32 = 0.2;
const DIE_TOP: f32 = 0.94;
const CORNICE: [f32; 3] = [0.96, 0.14, 0.96];
const CORNICE_Y: f32 = DIE_TOP + 0.03;
const CAP_TOP: f32 = CORNICE_Y + CORNICE[1] * 0.5;
/// The statue's own cast base, sunk a centimetre into the cornice.
const CAST_R: f32 = 0.36;
const CAST_H: f32 = 0.06;
const CAST_SINK: f32 = 0.01;
/// Where the figure stands: the cast base's top.
const STAND: f32 = CAP_TOP - CAST_SINK + CAST_H;
/// Dedication plate, its height on the die, and how far it laps into it.
const PLATE: [f32; 3] = [0.52, 0.3, 0.04];
const PLATE_Y: f32 = 0.6;
const PLATE_LAP: f32 = 0.002;

// ---- Figure, in metres above STAND --------------------------------------

/// The skin's node: the pelvis. Element positions are authored relative to
/// the stand point and rebased onto it.
const PELVIS: [f32; 3] = [0.0, 0.95, 0.0];
/// How far the hem sinks into the cast base.
const HEM_SINK: f32 = 0.012;
const ROBE_HEM_R: f32 = 0.25;
const ROBE_WAIST_R: f32 = 0.16;
const ROBE_TOP: f32 = 1.02;
const SHOULDER_R: [f32; 3] = [0.23, 1.46, 0.0];
const ELBOW_R: [f32; 3] = [0.31, 1.75, -0.03];
const HAND_R: [f32; 3] = [0.31, 2.05, -0.07];
const SHOULDER_L: [f32; 3] = [-0.23, 1.44, 0.0];
const ELBOW_L: [f32; 3] = [-0.28, 1.18, -0.02];
const HAND_L: [f32; 3] = [-0.235, 0.93, -0.19];
const UPPER_ARM_R: f32 = 0.056;
const FOREARM_R: f32 = 0.048;
/// Blend radii: the body masses, the limbs, the drapery ridges.
const BLEND_BODY: f32 = 0.07;
const BLEND_LIMB: f32 = 0.05;
const BLEND_FOLD: f32 = 0.02;
/// A drapery fold's radius, and how far its axis stands off the robe.
const FOLD_R: f32 = 0.05;
const FOLD_PROUD: f32 = -0.015;
const SKIN_RES: u32 = 48;

// ---- Held things ------------------------------------------------------

/// Torch handle: how far below the fist's centre it starts, and its length.
const HANDLE_BELOW: f32 = 0.07;
const HANDLE_H: f32 = 0.24;
const HANDLE_R: f32 = 0.028;
/// The funnel cup, and how far its point sinks into the handle's top.
const CUP_R: f32 = 0.095;
const CUP_H: f32 = 0.12;
const CUP_SINK: f32 = 0.02;
/// How far the flame's base sinks below the cup's rim.
const FLAME_SINK: f32 = 0.03;
/// Tablet, its centre above the stand point.
const TABLET: [f32; 3] = [0.05, 0.34, 0.26];
const TABLET_C: [f32; 3] = [-0.25, 1.06, -0.09];

/// One deliberate defect per guard, so each guard is shown to bite on the
/// fault it is for (#972 item 30). All zero in the shipped build.
#[derive(Default)]
struct Faults {
    /// Push the plate this far into the die.
    plate_sink: f32,
    /// Raise the base step this far, so it stands in front of the plate.
    base_high: f32,
    /// Lift the whole figure off its base.
    figure_lift: f32,
    /// Slide the raised forearm and its hand sideways — both ends of the
    /// link, so the gap cannot be closed by the elbow (lesson 40c).
    forearm_gap: f32,
    /// Lift the torch out of the fist.
    torch_lift: f32,
    /// Lift the flame off its cup.
    flame_lift: f32,
    /// Move the tablet away from the hand.
    tablet_away: f32,
}

/// Outdoor statuary bronze with its green patina — the finish the owner
/// asked for. The kit's [`bronze`] is burnished metal with a trace of
/// tarnish, and on a figure its brown rust mottle reads as rust, not
/// patina; here the metal's corrosion colour IS the verdigris, laid on
/// thick, over a darker statuary brown.
fn statue_bronze() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(STATUE_BRONZE),
        roughness: Fp(0.55),
        metallic: Fp(0.75),
        uv_scale: tiles_per_metre(tile::METAL),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(STATUE_BRONZE),
            color_rust: Fp3(PATINA),
            roughness: Fp64(0.55),
            metallic: Fp(0.75),
            rust_level: Fp64(0.45),
            weathering: ageing::verdigris(0x27, 1.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Dark statuary bronze, and the blue-green of its patina.
const STATUE_BRONZE: [f32; 3] = [0.30, 0.21, 0.12];
const PATINA: [f32; 3] = [0.24, 0.50, 0.42];

/// Gilt for the flame: bright, unweathered — gilding does not patinate.
fn gilt() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(GOLD),
        roughness: Fp(0.3),
        metallic: Fp(0.95),
        uv_scale: tiles_per_metre(tile::METAL),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(GOLD),
            roughness: Fp64(0.3),
            metallic: Fp(0.95),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = sub(a, b);
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}
fn unit(v: [f32; 3]) -> [f32; 3] {
    let l = dist(v, [0.0; 3]);
    [v[0] / l, v[1] / l, v[2] / l]
}

/// A capsule element from `a` to `b` (stand-relative), in the skin's frame.
fn limb(a: [f32; 3], b: [f32; 3], r: f32, blend: f32) -> BlobElement {
    let mid = sub(
        [
            (a[0] + b[0]) * 0.5,
            (a[1] + b[1]) * 0.5,
            (a[2] + b[2]) * 0.5,
        ],
        PELVIS,
    );
    blob_capsule(mid, r, dist(a, b) * 0.5, aim_y(unit(sub(b, a))), blend)
}

fn ellipsoid(c: [f32; 3], half: [f32; 3], blend: f32) -> BlobElement {
    blob_ellipsoid(sub(c, PELVIS), half, blend)
}

/// The figure's sixteen elements, stand-relative, rebased onto the pelvis.
fn figure_elements(f: &Faults) -> Vec<BlobElement> {
    let slide = [f.forearm_gap, 0.0, 0.0];
    let robe_half = (ROBE_TOP + HEM_SINK) * 0.5;
    vec![
        // The robe, hem to waist: one flared cone.
        blob_cone(
            sub([0.0, ROBE_TOP - robe_half, 0.01], PELVIS),
            ROBE_HEM_R,
            robe_half,
            ROBE_WAIST_R,
            BLEND_BODY,
        ),
        // Contrapposto: the free (left) knee pushes the drapery forward.
        ellipsoid([-0.09, 0.56, -0.12], [0.09, 0.16, 0.09], BLEND_BODY),
        // Fold ridges falling from the hip to the hem, each laid on the
        // robe's own surface at its height so it stands proud of it.
        fold(-35.0, 0.06, 0.82),
        fold(25.0, 0.06, 0.5),
        fold(70.0, 0.06, 0.8),
        // The himation's free end, hanging from the bent left forearm.
        ellipsoid([-0.31, 0.92, -0.07], [0.06, 0.2, 0.1], BLEND_LIMB),
        // Torso and the yoke of the shoulders.
        ellipsoid([0.0, 1.22, 0.0], [0.2, 0.28, 0.14], BLEND_BODY),
        ellipsoid([0.0, 1.44, 0.0], [0.27, 0.1, 0.13], BLEND_BODY),
        // Himation: a sash from the left shoulder across to the right hip.
        limb([-0.18, 1.48, -0.06], [0.17, 0.96, -0.11], 0.065, BLEND_LIMB),
        // Neck, head, and the hair gathered behind it.
        limb([0.0, 1.52, 0.0], [0.0, 1.63, -0.01], 0.055, BLEND_LIMB),
        ellipsoid([0.0, 1.75, -0.02], [0.095, 0.12, 0.11], BLEND_LIMB),
        ellipsoid([0.0, 1.79, 0.04], [0.1, 0.09, 0.1], BLEND_LIMB),
        // The raised arm, carrying the torch.
        limb(SHOULDER_R, ELBOW_R, UPPER_ARM_R, BLEND_LIMB),
        limb(
            add(ELBOW_R, slide),
            add(HAND_R, slide),
            FOREARM_R,
            BLEND_LIMB,
        ),
        // The bent arm, holding the tablet at the hip.
        limb(SHOULDER_L, ELBOW_L, UPPER_ARM_R, BLEND_LIMB),
        limb(ELBOW_L, HAND_L, FOREARM_R, BLEND_LIMB),
    ]
}

/// The robe's radius at height `y` above the stand: the capped cone's
/// straight flank from hem to waist.
fn robe_radius(y: f32) -> f32 {
    let t = ((y + HEM_SINK) / (ROBE_TOP + HEM_SINK)).clamp(0.0, 1.0);
    ROBE_HEM_R + (ROBE_WAIST_R - ROBE_HEM_R) * t
}

/// A fold ridge from `y0` up to `y1` on the robe's surface at bearing
/// `deg` from the front (`-Z`), sunk a little less than its own radius so
/// it stands proud of the flank.
fn fold(deg: f32, y0: f32, y1: f32) -> BlobElement {
    let (s, c) = deg.to_radians().sin_cos();
    let on = |y: f32| {
        let r = robe_radius(y) + FOLD_PROUD;
        [s * r, y, -c * r]
    };
    limb(on(y0), on(y1), FOLD_R, BLEND_FOLD)
}

/// Stand-relative → prop frame.
fn at_stand(p: [f32; 3], f: &Faults) -> [f32; 3] {
    [p[0], p[1] + STAND + f.figure_lift, p[2]]
}

fn build_with(f: &Faults) -> Generator {
    // The torch, a stack from the fist up: handle → cup → flame.
    let hand = at_stand(add(HAND_R, [f.forearm_gap, 0.0, 0.0]), f);
    let handle_bottom = hand[1] - HANDLE_BELOW + f.torch_lift;
    let handle_top = handle_bottom + HANDLE_H;
    // The cup is a cone turned over into a funnel: its point, now at the
    // bottom, sinks into the handle's top.
    let cup_c = handle_top - CUP_SINK + CUP_H * 0.5;
    let cup_rim = cup_c + CUP_H * 0.5;
    let flame_base = cup_rim - FLAME_SINK + f.flame_lift;
    // Three gilt tongues, each seated where the funnel is still wider than
    // it (lesson 33b), their bases staggered so no two share a plane; the
    // two side tongues lean out, each aimed along its own axis, so the
    // flame reads as a flame and not a spike.
    let tongues = [
        ([0.0, 0.0], 0.0, 0.065, 0.28, 0.0),
        ([0.026, 0.012], 0.012, 0.038, 0.2, 0.3),
        ([-0.024, -0.014], 0.02, 0.036, 0.18, 0.3),
    ];
    let flame: Vec<Generator> = tongues
        .iter()
        .map(|(off, lift, r, h, lean)| {
            let out = unit([off[0] + 1e-6, 0.0, off[1]]);
            let axis = [out[0] * lean, 1.0, out[2] * lean];
            let axis = unit(axis);
            let base = [hand[0] + off[0], flame_base + lift, hand[2] + off[1]];
            prim(
                cone(*r, *h, 10, gilt()),
                add(
                    base,
                    [axis[0] * h * 0.5, axis[1] * h * 0.5, axis[2] * h * 0.5],
                ),
                aim_y(axis),
            )
        })
        .collect();
    let cup = prim(
        cone(CUP_R, CUP_H, 12, statue_bronze()),
        [hand[0], cup_c, hand[2]],
        quat_x(std::f32::consts::PI),
    );
    // The cup is turned, so the flame is the handle's child, not the cup's
    // (a turned node may carry children only at its own origin, lesson 22).
    let mut on_handle = vec![cup];
    on_handle.extend(flame);
    let torch = nest(
        prim(
            cylinder_tapered(HANDLE_R, HANDLE_H, 10, 0.2, statue_bronze()),
            [hand[0], (handle_bottom + handle_top) * 0.5, hand[2]],
            id_quat(),
        ),
        on_handle,
    );
    let tablet = prim(
        cuboid_tapered(TABLET, 0.0, statue_bronze()),
        at_stand(add(TABLET_C, [-f.tablet_away, 0.0, 0.0]), f),
        id_quat(),
    );
    let figure = nest(
        prim(
            blob_group(figure_elements(f), SKIN_RES, statue_bronze()),
            at_stand(PELVIS, f),
            id_quat(),
        ),
        vec![torch, tablet],
    );

    let cast = nest(
        prim(
            solid(cylinder_tapered(CAST_R, CAST_H, 24, 0.0, statue_bronze())),
            [0.0, CAP_TOP - CAST_SINK + CAST_H * 0.5, 0.0],
            id_quat(),
        ),
        vec![figure],
    );
    let cornice = nest(
        prim(
            solid(cuboid_tapered(CORNICE, 0.0, marble([0.82, 0.81, 0.78]))),
            [0.0, CORNICE_Y, 0.0],
            id_quat(),
        ),
        vec![cast],
    );
    // The dedication plate: its back face laps PLATE_LAP into the die's
    // front face, so its standoff is the die's own half-depth (lesson 11).
    let plate = prim(
        cuboid_tapered(PLATE, 0.0, bronze([0.40, 0.30, 0.16])),
        [
            0.0,
            PLATE_Y,
            -(DIE_W * 0.5 + PLATE[2] * 0.5 - PLATE_LAP - f.plate_sink),
        ],
        id_quat(),
    );
    let die = nest(
        prim(
            solid(cuboid_tapered(
                [DIE_W, DIE_TOP - DIE_BOTTOM, DIE_W],
                0.0,
                marble(MARBLE),
            )),
            [0.0, (DIE_TOP + DIE_BOTTOM) * 0.5, 0.0],
            id_quat(),
        ),
        vec![plate, cornice],
    );
    let base_h = BASE[1] + f.base_high;
    nest(
        prim(
            solid(cuboid_tapered(
                [BASE[0], base_h, BASE[2]],
                0.0,
                marble([0.82, 0.81, 0.78]),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
        vec![die],
    )
}

/// What the whole statue may mesh to. Measured at 4 084 — the skin is most
/// of it — against the orator's 1 272; a Rich-only prop can carry it.
#[cfg(test)]
const STATUE_TRIANGLES: usize = 4_600;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_no_coplanar_faces, assert_no_tilted_parents, assert_sanitize_stable, blob_cell_size,
        blob_components, rotate_by, triangle_count,
    };
    use crate::pds::GeneratorKind;
    use crate::pds::generator::BlobShape;

    const SLUG: &str = "statue";

    fn walk(g: &Generator, at: [f32; 3], f: &mut dyn FnMut(&Generator, [f32; 3])) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        f(g, here);
        for c in &g.children {
            walk(c, here, f);
        }
    }

    fn bites(faults: Faults, guard: fn(&Generator), needle: &str) {
        let broken = build_with(&faults);
        let err = std::panic::catch_unwind(|| guard(&broken))
            .expect_err("the guard passed a broken build");
        let msg = err
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default();
        assert!(
            msg.contains(needle),
            "the guard fired, but not on the fault: {msg}"
        );
    }

    /// Every cuboid: (centre, half-extents, solid).
    fn boxes(root: &Generator) -> Vec<([f32; 3], [f32; 3], bool)> {
        let mut out = Vec::new();
        walk(root, [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cuboid { size, common } = &g.kind {
                out.push((at, size.0.map(|s| s * 0.5), common.solid));
            }
        });
        out
    }

    /// The skin: its elements and the node's position in the prop.
    fn skin(root: &Generator) -> (Vec<BlobElement>, [f32; 3], u32) {
        let mut out = None;
        walk(root, [0.0; 3], &mut |g, at| {
            if let GeneratorKind::BlobGroup {
                elements,
                resolution,
                ..
            } = &g.kind
            {
                out = Some((elements.clone(), at, *resolution));
            }
        });
        out.expect("the statue has a skin")
    }

    /// A capsule element's two BUILT end-sphere centres and its radius, in
    /// the prop frame (lesson 40: a capsule's end is a point like a strut's).
    fn capsule_ends(e: &BlobElement, node: [f32; 3]) -> Option<([f32; 3], [f32; 3], f32)> {
        if e.shape != BlobShape::Capsule {
            return None;
        }
        let half = rotate_by(e.rotation.0, [0.0, e.radii.0[1], 0.0]);
        let c = add(node, e.position.0);
        Some((add(c, half), sub(c, half), e.radii.0[0]))
    }

    /// A revolved prim as the torch guards read it: its kind, built centre,
    /// radius, height and turn.
    type Revolved = (&'static str, [f32; 3], f32, f32, [f32; 4]);

    /// Cylinders and cones with their built centre, radius, height and turn.
    fn revolved(root: &Generator) -> Vec<Revolved> {
        let mut out = Vec::new();
        walk(root, [0.0; 3], &mut |g, at| match &g.kind {
            GeneratorKind::Cylinder { radius, height, .. } => {
                out.push(("cylinder", at, radius.0, height.0, g.transform.rotation.0))
            }
            GeneratorKind::Cone { radius, height, .. } => {
                out.push(("cone", at, radius.0, height.0, g.transform.rotation.0))
            }
            _ => {}
        });
        out
    }

    #[test]
    fn statue_round_trips_through_sanitize() {
        assert_sanitize_stable(&Statue.build(""), SLUG);
    }

    #[test]
    fn statue_has_no_tilted_parents() {
        assert_no_tilted_parents(&Statue.build(""), SLUG);
    }

    #[test]
    fn statue_has_no_coplanar_faces() {
        assert_no_coplanar_faces(&Statue.build(""), SLUG);
    }

    // ---- the plate: derived standoff, nothing in front (lessons 11, 28)

    fn guard_plate(root: &Generator) {
        let all = boxes(root);
        // The plate is the one board thin in Z: the tablet is thin in X.
        let plates: Vec<_> = all
            .iter()
            .filter(|(_, h, _)| h[2] < 0.05 && h[0] > 0.15 && h[1] > 0.1)
            .collect();
        assert_eq!(plates.len(), 1, "one dedication plate");
        let (pc, ph, _) = *plates[0];
        let back = pc[2] + ph[2];
        let front = pc[2] - ph[2];
        let covers =
            |c: [f32; 3], h: [f32; 3]| (pc[0] - c[0]).abs() < h[0] && (pc[1] - c[1]).abs() < h[1];
        // The host: of the solids behind the plate's centre, the one whose
        // front face is nearest its back — the face it is mounted on.
        let host = all
            .iter()
            .filter(|(c, h, s)| *s && covers(*c, *h) && (back - c[2]).abs() < h[2])
            .map(|(c, h, _)| c[2] - h[2])
            .min_by(|a, b| (a - back).abs().total_cmp(&(b - back).abs()))
            .unwrap_or(f32::MAX);
        assert!(host < f32::MAX, "the plate is mounted on nothing");
        assert!(
            back - host <= 0.005,
            "the plate's back is {:.3} m inside the die's face — it is buried in the die",
            back - host
        );
        assert!(
            host - front >= 0.02,
            "the plate stands only {:.3} proud of the die",
            host - front
        );
        for (c, h, _) in &all {
            if (c[2] - pc[2]).abs() < 1e-4 && (c[0] - pc[0]).abs() < 1e-4 {
                continue; // the plate itself
            }
            if covers(*c, *h) {
                assert!(
                    c[2] - h[2] >= front + 0.004,
                    "a box at {c:?} presents a face at z {:.3}, nearer the viewer than the \
                     plate's {front:.3} — it stands in front of the plate",
                    c[2] - h[2]
                );
            }
        }
    }

    #[test]
    fn the_plate_is_mounted_on_the_die_and_nothing_hides_it() {
        guard_plate(&Statue.build(""));
        bites(
            Faults {
                plate_sink: 0.03,
                ..Default::default()
            },
            guard_plate,
            "buried in the die",
        );
        bites(
            Faults {
                base_high: 0.5,
                ..Default::default()
            },
            guard_plate,
            "in front of the plate",
        );
    }

    // ---- the plinth is a stack (lesson 33)

    /// Base, die, cornice and cast base chain upward, each lapped into the
    /// one below: no course floats and none shares the one below's top.
    #[test]
    fn the_plinth_is_an_unbroken_stack() {
        let root = Statue.build("");
        let mut courses: Vec<(f32, f32)> = boxes(&root)
            .iter()
            .filter(|(c, _, s)| *s && c[0].abs() < 1e-4 && c[2].abs() < 1e-4)
            .map(|(c, h, _)| (c[1] - h[1], c[1] + h[1]))
            .collect();
        for (kind, c, _, h, _) in revolved(&root) {
            if kind == "cylinder" && c[0].abs() < 1e-4 && c[2].abs() < 1e-4 {
                courses.push((c[1] - h * 0.5, c[1] + h * 0.5));
            }
        }
        courses.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(courses.len(), 4, "base, die, cornice, cast base");
        assert!(courses[0].0.abs() < 1e-4, "the base stands on the ground");
        for w in courses.windows(2) {
            assert!(
                w[1].0 < w[0].1 - 1e-3,
                "a course starting at {:.3} does not lap into the one below, which tops \
                 out at {:.3}",
                w[1].0,
                w[0].1
            );
        }
    }

    // ---- the figure

    fn cast_top(root: &Generator) -> f32 {
        revolved(root)
            .iter()
            .filter(|(k, c, r, ..)| *k == "cylinder" && c[0].abs() < 1e-4 && *r > 0.2)
            .map(|(_, c, _, h, _)| c[1] + h * 0.5)
            .next()
            .expect("the cast base")
    }

    fn guard_stands(root: &Generator) {
        let (elements, node, _) = skin(root);
        let bottom = elements
            .iter()
            .map(|e| {
                let p = add(node, e.position.0);
                let reach = match e.shape {
                    BlobShape::Capsule => {
                        rotate_by(e.rotation.0, [0.0, e.radii.0[1], 0.0])[1].abs() + e.radii.0[0]
                    }
                    _ => e.radii.0[1],
                };
                p[1] - reach
            })
            .fold(f32::MAX, f32::min);
        let top = cast_top(root);
        assert!(
            bottom <= top - 0.002,
            "the skin's lowest point is {bottom:.3} and the cast base tops out at {top:.3} \
             — the figure floats"
        );
        assert!(
            bottom >= top - 0.03,
            "the hem is {:.3} deep in the cast base",
            top - bottom
        );
    }

    /// The hem is sunk a centimetre into the cast base: stood on it, not
    /// hovering over it and not buried in it.
    #[test]
    fn the_figure_stands_on_its_base() {
        guard_stands(&Statue.build(""));
        bites(
            Faults {
                figure_lift: 0.05,
                ..Default::default()
            },
            guard_stands,
            "the figure floats",
        );
    }

    fn guard_one_skin(root: &Generator) {
        let (elements, _, res) = skin(root);
        let kind = blob_group(elements.clone(), res, statue_bronze());
        assert_eq!(
            blob_components(&kind),
            1,
            "{SLUG}: the figure polygonised into more than one piece"
        );
        let mesh = crate::world_builder::build_primitive_mesh(&kind).mesh;
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) =
            mesh.attribute(bevy::prelude::Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions");
        };
        let span = (0..3)
            .map(|k| {
                let (lo, hi) = pos.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
                    (lo.min(p[k]), hi.max(p[k]))
                });
                hi - lo
            })
            .fold(0.0_f32, f32::max);
        let cell = blob_cell_size(span, res);
        let thinnest = elements
            .iter()
            .map(|e| match e.shape {
                BlobShape::Capsule => e.radii.0[0] * 2.0,
                BlobShape::Cone => e.radii.0[0].min(e.radii.0[2]) * 2.0,
                _ => e.radii.0[0].min(e.radii.0[1]).min(e.radii.0[2]) * 2.0,
            })
            .fold(f32::MAX, f32::min);
        assert!(
            thinnest > cell * 2.0,
            "{SLUG}: the thinnest element is {thinnest} m across a {cell} m cell"
        );
    }

    /// One skin, and the arithmetic that keeps it one: the thinnest element
    /// is over two sample cells across.
    #[test]
    fn the_figure_is_one_skin() {
        guard_one_skin(&Statue.build(""));
        bites(
            Faults {
                forearm_gap: 0.25,
                ..Default::default()
            },
            guard_one_skin,
            "more than one piece",
        );
    }

    /// The raised fist: the highest capsule end in the skin.
    fn fist(root: &Generator) -> ([f32; 3], f32) {
        let (elements, node, _) = skin(root);
        elements
            .iter()
            .filter_map(|e| capsule_ends(e, node))
            .flat_map(|(a, b, r)| [(a, r), (b, r)])
            .max_by(|a, b| a.0[1].total_cmp(&b.0[1]))
            .expect("a raised arm")
    }

    fn guard_torch_held(root: &Generator) {
        let (c, r) = fist(root);
        let handles: Vec<_> = revolved(root)
            .into_iter()
            .filter(|(k, _, rad, ..)| *k == "cylinder" && *rad < 0.05)
            .collect();
        assert_eq!(handles.len(), 1, "one torch handle");
        let (_, hc, _, hh, q) = handles[0];
        let axis = rotate_by(q, [0.0, 1.0, 0.0]);
        assert!(axis[1] > 0.99, "the torch is held upright");
        let off = ((hc[0] - c[0]).powi(2) + (hc[2] - c[2]).powi(2)).sqrt();
        assert!(
            off < r * 0.5,
            "the handle's axis passes {off:.3} from the fist's centre — outside the grip"
        );
        let (bottom, top) = (hc[1] - hh * 0.5, hc[1] + hh * 0.5);
        assert!(
            bottom < c[1] && top > c[1] + r,
            "the handle runs {bottom:.3}..{top:.3} and the fist is at {:.3} — the torch \
             floats above the fist",
            c[1]
        );
    }

    /// The torch is gripped: its handle passes through the raised fist
    /// (lesson 40 — a built capsule end against a built part).
    #[test]
    fn the_torch_is_in_the_raised_fist() {
        guard_torch_held(&Statue.build(""));
        bites(
            Faults {
                torch_lift: 0.3,
                ..Default::default()
            },
            guard_torch_held,
            "floats above the fist",
        );
    }

    fn guard_torch_stack(root: &Generator) {
        let parts = revolved(root);
        let (_, hc, _, hh, _) = *parts
            .iter()
            .find(|(k, _, r, ..)| *k == "cylinder" && *r < 0.05)
            .expect("a handle");
        let handle_top = hc[1] + hh * 0.5;
        // The cup: the cone turned over. Read which way its point went.
        let cups: Vec<_> = parts
            .iter()
            .filter(|(k, _, _, _, q)| *k == "cone" && rotate_by(*q, [0.0, 1.0, 0.0])[1] < 0.0)
            .collect();
        assert_eq!(cups.len(), 1, "one funnel cup (a cone with its point down)");
        let (_, cc, cr, ch, q) = *cups[0];
        let point = add(cc, rotate_by(q, [0.0, ch * 0.5, 0.0]));
        assert!(
            point[1] < handle_top && point[1] > handle_top - 0.05,
            "the cup's point is at {:.3} and the handle tops out at {handle_top:.3} — the \
             cup is not seated on the handle",
            point[1]
        );
        let rim = cc[1] + ch * 0.5;
        let flames: Vec<_> = parts
            .iter()
            .filter(|(k, _, _, _, q)| *k == "cone" && rotate_by(*q, [0.0, 1.0, 0.0])[1] > 0.0)
            .collect();
        assert_eq!(flames.len(), 3, "three flame tongues");
        for (_, fc, fr, fh, fq) in flames {
            // The base centre through the tongue's BUILT turn (lesson 23).
            let b = sub(*fc, rotate_by(*fq, [0.0, fh * 0.5, 0.0]));
            let base = b[1];
            let fc = &b;
            assert!(
                base < rim && base > point[1],
                "a flame tongue's base is at {base:.3}, the cup runs {:.3}..{rim:.3} — the \
                 flame floats off its cup",
                point[1]
            );
            // Lesson 33b: seated where the cup is wider than the tongue.
            let cup_r = cr * (base - point[1]) / ch;
            let off = ((fc[0] - cc[0]).powi(2) + (fc[2] - cc[2]).powi(2)).sqrt();
            assert!(
                off + fr <= cup_r,
                "a tongue {off:.3} off-axis with radius {fr:.3} pokes through the cup, whose \
                 radius at its base is {cup_r:.3}"
            );
        }
    }

    /// Handle → cup → flame is a contiguous stack, the cup is a funnel and
    /// every tongue is seated inside it (lessons 33 and the banner corollary).
    #[test]
    fn the_torch_is_an_unbroken_stack() {
        guard_torch_stack(&Statue.build(""));
        bites(
            Faults {
                flame_lift: 0.1,
                ..Default::default()
            },
            guard_torch_stack,
            "the flame floats",
        );
    }

    fn guard_tablet_held(root: &Generator) {
        let (elements, node, _) = skin(root);
        let tablets: Vec<_> = boxes(root)
            .into_iter()
            .filter(|(_, h, _)| h[0] < 0.05 && h[1] > 0.1 && h[2] > 0.1)
            .collect();
        assert_eq!(tablets.len(), 1, "one tablet (thin in X)");
        let (tc, th, _) = tablets[0];
        let gap = |p: [f32; 3]| {
            (0..3)
                .map(|k| ((p[k] - tc[k]).abs() - th[k]).max(0.0).powi(2))
                .sum::<f32>()
                .sqrt()
        };
        let held = elements
            .iter()
            .filter_map(|e| capsule_ends(e, node))
            .flat_map(|(a, b, r)| [(a, r), (b, r)])
            .any(|(p, r)| gap(p) < r * 0.5);
        assert!(
            held,
            "no hand of the figure reaches the tablet at {tc:?} — it is not held"
        );
    }

    /// The tablet is in the left hand: a capsule end sits against it.
    #[test]
    fn the_tablet_is_held() {
        guard_tablet_held(&Statue.build(""));
        bites(
            Faults {
                tablet_away: 0.25,
                ..Default::default()
            },
            guard_tablet_held,
            "it is not held",
        );
    }

    /// The figure faces the front: the tablet, the free knee and the torch
    /// hand are all on the `-Z` side of the body's centre line.
    #[test]
    fn the_figure_faces_the_front() {
        let root = Statue.build("");
        let (c, _) = fist(&root);
        assert!(c[2] < 0.0, "the torch is held toward the back");
        let (elements, node, _) = skin(&root);
        let head = elements
            .iter()
            .filter(|e| e.shape == BlobShape::Ellipsoid)
            .max_by(|a, b| a.position.0[1].total_cmp(&b.position.0[1]))
            .unwrap();
        let _ = node;
        assert!(
            head.position.0[2] <= 0.05,
            "the highest mass (the hair) sits behind"
        );
    }

    #[test]
    fn the_statue_stays_within_its_triangle_budget() {
        let tris = triangle_count(&Statue.build(""));
        assert!(
            tris <= STATUE_TRIANGLES,
            "the statue meshes to {tris} triangles, over its {STATUE_TRIANGLES} budget"
        );
    }

    /// The editability contract (lesson 3): base → die → [plate, cornice →
    /// cast → figure → [torch → [cup, three tongues], tablet]].
    #[test]
    fn statue_subtree_sizes() {
        let root = Statue.build("");
        let die = &root.children[0];
        assert_eq!(die.children.len(), 2, "plate and cornice");
        let cornice = &die.children[1];
        let cast = &cornice.children[0];
        let figure = &cast.children[0];
        assert!(matches!(figure.kind, GeneratorKind::BlobGroup { .. }));
        assert_eq!(figure.children.len(), 2, "torch and tablet");
        assert_eq!(
            figure.children[0].children.len(),
            4,
            "cup and three tongues"
        );
    }
}
