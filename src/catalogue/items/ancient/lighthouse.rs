//! Lighthouse — a tapered banded tower with a glowing lamp room,
//! gallery ring, cone roof, and a keeper's hut at the base. The lamp
//! is strongly emissive, so the structure doubles as a night beacon
//! visible across a coastal or archipelago home region.
//!
//! Primitive-built (no shape grammar): the tower's taper and the round
//! plan come straight from tapered cylinders, which the grammar's
//! box-split vocabulary can't express.
//!
//! Frame convention: the root is a wide foundation slab whose centre
//! sits just above the generator origin, so a terrain-snapped
//! placement puts the slab base at ground level; every child is
//! positioned relative to the slab centre.

use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings, SovereignTextureConfig};
use crate::seeded_defaults::ThemeArchetype;

use crate::catalogue::items::util::{
    cone, cuboid_tapered, cylinder_tapered, foundation_block, foundation_disc, glow, id_quat, prim,
    solid, sphere, torus,
};

pub struct Lighthouse;

impl CatalogueEntry for Lighthouse {
    fn slug(&self) -> &'static str {
        "lighthouse"
    }
    fn name(&self) -> &'static str {
        "Lighthouse"
    }
    fn description(&self) -> &'static str {
        "Tapered banded beacon tower with a glowing lamp room and keeper's hut."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }

    fn themes(&self) -> &'static [ThemeArchetype] {
        &[
            ThemeArchetype::AncientClassical,
            ThemeArchetype::CoastalResort,
        ]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 7.5,
            min_spawn_dist: 45.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn band_mat(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.7),
        metallic: Fp(0.05),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

fn build_tree() -> Generator {
    let white = [0.92, 0.90, 0.86];
    let red = [0.72, 0.18, 0.14];
    let iron = [0.20, 0.21, 0.24];
    let lamp_glow = [1.0, 0.85, 0.45];

    // Foundation slab — the root. Slab base sits at the generator
    // origin (= snapped terrain height); children measure their Y from
    // the slab centre.
    let slab_h = 0.4;
    let mut root = prim(
        solid(cylinder_tapered(3.4, slab_h, 24, 0.05, band_mat(iron))),
        [0.0, slab_h * 0.5, 0.0],
        id_quat(),
    );
    // Ground height → child-frame Y (relative to the slab centre).
    let rel = |ground_y: f32| ground_y - slab_h * 0.5;

    // Buried foundations under the tower disc and the keeper's hut,
    // re-anchored from the entry ground frame into the slab-root frame.
    for mut base in [
        foundation_disc(3.6, 3.0),
        foundation_block(4.6, 5.4, [3.8, 0.0], 3.0),
    ] {
        base.transform.translation.0[1] -= slab_h * 0.5;
        root.children.push(base);
    }

    // Three stacked, tapered drum segments in alternating bands.
    // Radii chain so each segment's crown meets the next one's base:
    // a segment of radius r and taper t ends at r * (1 - t).
    let taper = 0.12;
    let segments = [
        (2.2_f32, 5.5_f32, white),
        (2.2 * (1.0 - taper), 5.0, red),
        (2.2 * (1.0 - taper) * (1.0 - taper), 4.5, white),
    ];
    let mut y = slab_h;
    for (radius, height, color) in segments {
        root.children.push(prim(
            solid(cylinder_tapered(radius, height, 24, taper, band_mat(color))),
            [0.0, rel(y + height * 0.5), 0.0],
            id_quat(),
        ));
        y += height;
    }
    let tower_top = y; // ≈ 15.4 m above ground

    // Gallery: platform disc + railing ring under the lamp room.
    let gallery_r = 2.0;
    root.children.push(prim(
        solid(cylinder_tapered(gallery_r, 0.25, 24, 0.0, band_mat(iron))),
        [0.0, rel(tower_top + 0.125), 0.0],
        id_quat(),
    ));
    root.children.push(prim(
        torus(0.06, gallery_r - 0.05, band_mat(iron)),
        [0.0, rel(tower_top + 1.0), 0.0],
        id_quat(),
    ));

    // Lamp room: iron drum with the beacon orb inside (the orb is
    // oversized so it reads through the drum silhouette at distance).
    let lamp_h = 1.8;
    let mut lamp_room = prim(
        solid(cylinder_tapered(1.2, lamp_h, 16, 0.05, band_mat(iron))),
        [0.0, rel(tower_top + 0.25 + lamp_h * 0.5), 0.0],
        id_quat(),
    );
    lamp_room.children.push(prim(
        sphere(0.85, 3, glow(lamp_glow, 8.0)),
        [0.0, 0.1, 0.0],
        id_quat(),
    ));
    root.children.push(lamp_room);

    // Cone roof above the lamp room.
    root.children.push(prim(
        solid(cone(1.4, 1.3, 16, band_mat(red))),
        [0.0, rel(tower_top + 0.25 + lamp_h + 0.65), 0.0],
        id_quat(),
    ));

    // Keeper's hut: small block + pyramid roof. Taper tops out at the
    // sanitiser's 0.99 cap — visually identical to a true apex.
    let mut hut = prim(
        solid(cuboid_tapered([3.2, 2.6, 4.2], 0.0, band_mat(white))),
        [3.8, rel(slab_h + 1.3), 0.0],
        id_quat(),
    );
    hut.children.push(prim(
        solid(cuboid_tapered([3.6, 1.4, 4.6], 0.99, band_mat(red))),
        [0.0, 2.0, 0.0],
        id_quat(),
    ));
    root.children.push(hut);

    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Lighthouse.build(""), "lighthouse");
    }

    #[test]
    fn has_emissive_beacon() {
        fn any_emissive(g: &Generator) -> bool {
            let own = match &g.kind {
                crate::pds::GeneratorKind::Sphere { material, .. } => {
                    material.emission_strength.0 > 1.0
                }
                _ => false,
            };
            own || g.children.iter().any(any_emissive)
        }
        assert!(
            any_emissive(&Lighthouse.build("")),
            "lighthouse lost its glowing beacon"
        );
    }
}
