//! Totem pole — a Nordic prop. A carved god-pole: a stack of blocky carved
//! faces in alternating wood tones, each with a jutting brow, nose, and
//! cold-glinting deep-set eyes, banded with paint and topped by a horned
//! head. Raised at the edge of the steading. The faces are carved on the
//! shore-facing (-Z) front so they read to the camera.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, glow, id_quat, prim, quat_x, quat_y, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{SHIELD_RED, WOOD_DARK, WOOD_WARM, cloth, timber};

/// Cold glint worked into the carved eyes.
const EYE_GLOW: [f32; 3] = [0.45, 0.66, 0.95];

pub struct TotemPole;

impl CatalogueEntry for TotemPole {
    fn slug(&self) -> &'static str {
        "totem_pole"
    }
    fn name(&self) -> &'static str {
        "Totem Pole"
    }
    fn description(&self) -> &'static str {
        "Carved god-pole of stacked faces topped with a horned head."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Nordic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::NORDIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Carve a face (jutting brow, nose, deep-set glinting eyes, mouth slit) onto
/// the -Z front of a figure tier centred at height `y`, half-width `hw`.
fn carve_face(prims: &mut Vec<Generator>, y: f32, hw: f32, mat: SovereignMaterialSettings) {
    let zf = -(hw + 0.03);
    // Brow ridge.
    prims.push(prim(
        solid(cuboid_tapered([hw * 1.5, 0.12, 0.12], 0.0, mat.clone())),
        [0.0, y + 0.18, zf],
        id_quat(),
    ));
    // Jutting nose pointing forward (-Z).
    prims.push(prim(
        solid(cone(0.09, 0.34, 6, mat)),
        [0.0, y + 0.02, zf - 0.06],
        quat_x(-std::f32::consts::FRAC_PI_2),
    ));
    // Deep-set glinting eyes.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.13, 0.11, 0.05], 0.0, glow(EYE_GLOW, 1.6)),
            [sx * 0.2, y + 0.08, zf],
            id_quat(),
        ));
    }
    // Mouth slit.
    prims.push(prim(
        solid(cuboid_tapered(
            [hw * 1.1, 0.08, 0.06],
            0.0,
            timber(WOOD_DARK),
        )),
        [0.0, y - 0.26, zf],
        id_quat(),
    ));
}

fn build_tree() -> Generator {
    // Buried-post base (root).
    let mut prims = vec![prim(
        solid(cuboid_tapered([0.8, 0.5, 0.8], 0.0, timber(WOOD_DARK))),
        [0.0, 0.25, 0.0],
        id_quat(),
    )];

    // Stacked carved figures, alternating tone and a slight twist.
    let segs = [
        (0.96_f32, 1.0_f32, WOOD_WARM, 0.0_f32),
        (0.86, 0.95, WOOD_DARK, 0.22),
        (0.92, 1.0, WOOD_WARM, -0.18),
        (0.8, 0.9, WOOD_DARK, 0.14),
    ];
    let mut y = 0.5;
    for (w, h, tone, yaw) in segs {
        prims.push(prim(
            solid(cuboid_tapered([w, h, w], 0.0, timber(tone))),
            [0.0, y + h * 0.5, 0.0],
            quat_y(yaw),
        ));
        // Carve a face on the upper part of each tier (twist is small, so
        // the front stays roughly -Z facing).
        carve_face(&mut prims, y + h * 0.55, w * 0.5, timber(tone));
        y += h;
    }

    // A painted band around one figure.
    prims.push(prim(
        cuboid_tapered([0.94, 0.2, 0.94], 0.0, cloth(SHIELD_RED, WOOD_DARK)),
        [0.0, 1.95, 0.0],
        id_quat(),
    ));

    // Horned head on top.
    prims.push(prim(
        solid(cuboid_tapered([0.86, 0.78, 0.74], 0.12, timber(WOOD_WARM))),
        [0.0, y + 0.39, 0.0],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cone(0.11, 0.55, 6, timber(WOOD_DARK))),
            [sx * 0.38, y + 0.78, 0.0],
            quat_z(-sx * 0.6),
        ));
    }
    // Cold-glinting carved eyes on the horned head's -Z front.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.14, 0.12, 0.06], 0.0, glow(EYE_GLOW, 1.8)),
            [sx * 0.2, y + 0.46, -0.4],
            id_quat(),
        ));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&TotemPole.build(""), "totem_pole");
    }
}
