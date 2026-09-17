//! The `--reference-figure` mannequin: a 1.75 m human-scale ruler built from
//! plain primitives, so a vehicle can be judged against a body.
//!
//! `--avatar` refuses a rigged humanoid seed - every one of them is a skinned
//! `symbios-avatar` build with no [`Generator`] tree to walk - so before this
//! there was no tool shot of a vehicle beside a person at all, and the whole
//! scale question of the vehicle redesign (#1359) was argued from memory.
//! This is not a body: it is a scale rule with arms, drawn in one flat grey
//! so nothing about it competes with the subject standing next to it.
//!
//! The tree's own origin is the **pelvis**, not the feet. A generator tree
//! has no empty-group node kind, so a root at the feet would have to be a
//! hidden hub prim - and every child would inherit its offset anyway
//! (`project_prop_authoring_transform`). Rooting the visible pelvis at the
//! origin costs no geometry and keeps every limb's offset readable as "so
//! far above/below the hips"; the play view stands the figure by resting its
//! bounds on the ground, which is exactly right for a thing that has feet.

use crate::pds::avatar::default_visuals::common::{
    cuboid, cylinder, id_quat, prim, quat_xyzw, quat_z, sphere,
};
use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::{Fp, Fp3};

/// How tall the figure stands, crown to sole (m). The number the shot is
/// read against: a 2.8 m sloop is meant to be about one and a half of these.
pub(super) const HEIGHT: f32 = 1.75;

/// Height of the pelvis above the soles (m). The tree's own origin, and the
/// datum every other constant here is measured *down from* rather than up
/// to: all of them are read as heights above the ground, which is how a
/// figure is described, and the builder subtracts this once.
const PELVIS_Y: f32 = 1.00;

/// Head radius and the height of its centre (m): the crown is the top of the
/// figure, so these two set [`HEIGHT`].
const HEAD_R: f32 = 0.12;
const HEAD_Y: f32 = 1.63;

/// Foot block height and the height of its centre (m): the soles are the
/// bottom of the figure, so these two put it on the ground.
const FOOT_H: f32 = 0.08;
const FOOT_Y: f32 = 0.04;

/// Half the distance between the leg centres, and between the arm centres (m).
const LEG_X: f32 = 0.10;
const ARM_X: f32 = 0.225;

/// The mannequin's one material: matte mid-grey, no texture. Flat on
/// purpose - a reference figure that read as a *character* would draw the eye
/// away from the craft it is standing beside.
fn skin() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.62, 0.60, 0.58]),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        ..Default::default()
    }
}

/// The 1.75 m mannequin, facing local -Z - the same way an assembled vehicle
/// faces once its travel yaw is applied, so a line-up presents every subject
/// the same side.
pub(super) fn reference_figure() -> Generator {
    let m = skin();
    // Root: the pelvis, at the tree's origin. Everything below is placed by
    // its height above the soles and lowered onto that datum.
    let mut root = prim(
        cuboid([0.30, 0.18, 0.20], m.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    let mut part = |kind, x: f32, y: f32, z: f32| {
        root.children
            .push(prim(kind, [x, y - PELVIS_Y, z], id_quat()))
    };

    for side in [-1.0f32, 1.0] {
        // Legs: shin and thigh as plain cylinders, a block for the foot with
        // the toe forward (-Z).
        let x = side * LEG_X;
        part(cuboid([0.10, FOOT_H, 0.26], m.clone()), x, FOOT_Y, -0.04);
        part(cylinder(0.058, 0.40, 12, m.clone()), x, 0.28, 0.0);
        part(cylinder(0.072, 0.44, 12, m.clone()), x, 0.70, 0.0);
        // Arms hanging at the sides, fingertips at mid-thigh.
        let x = side * ARM_X;
        part(cylinder(0.050, 0.30, 10, m.clone()), x, 1.29, 0.0);
        part(cylinder(0.044, 0.30, 10, m.clone()), x, 0.99, 0.0);
        part(cuboid([0.075, 0.16, 0.05], m.clone()), x, 0.76, 0.0);
    }
    part(cuboid([0.36, 0.38, 0.21], m.clone()), 0.0, 1.28, 0.0);
    part(cylinder(0.055, 0.10, 10, m.clone()), 0.0, 1.50, 0.0);
    part(sphere(HEAD_R, 16, m.clone()), 0.0, HEAD_Y, 0.0);
    // Shoulders: one cylinder laid across the chest on its side.
    root.children.push(prim(
        cylinder(0.075, 0.40, 12, m),
        [0.0, 1.44 - PELVIS_Y, 0.0],
        quat_xyzw(quat_z(std::f32::consts::FRAC_PI_2)),
    ));
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The figure is exactly 1.75 m from sole to crown, and its soles are on
    /// the ground plane when its bounds are rested there. Both are claims the
    /// picture makes - the figure is the shot's only ruler - and both are a
    /// sum of four constants, so they are worth pinning by arithmetic rather
    /// than by reading the mesh back.
    #[test]
    fn the_reference_figure_is_one_point_seven_five_metres_tall() {
        assert_eq!(HEAD_Y + HEAD_R, HEIGHT, "crown");
        assert_eq!(FOOT_Y - FOOT_H * 0.5, 0.0, "soles");
    }

    /// Plain primitives only (#1360's own words): nothing here may need the
    /// blob mesher, an L-system derivation or a texture bake, because the
    /// figure is spawned in every play-view shot and must cost nothing.
    #[test]
    fn the_reference_figure_is_built_from_plain_primitives() {
        use crate::pds::generator::GeneratorKind;
        fn walk(g: &Generator, seen: &mut usize) {
            assert!(
                matches!(
                    g.kind,
                    GeneratorKind::Cuboid { .. }
                        | GeneratorKind::Cylinder { .. }
                        | GeneratorKind::Sphere { .. }
                ),
                "the mannequin grew a {:?} node",
                std::mem::discriminant(&g.kind)
            );
            assert!(
                g.kind.material().is_none_or(|m| {
                    m.texture == crate::pds::texture::SovereignTextureConfig::None
                }),
                "the mannequin grew a textured material"
            );
            *seen += 1;
            for child in &g.children {
                walk(child, seen);
            }
        }
        let mut seen = 0;
        walk(&reference_figure(), &mut seen);
        assert_eq!(
            seen, 17,
            "pelvis + 6 per side + torso, neck, head, shoulders"
        );
    }
}
