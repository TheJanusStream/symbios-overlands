//! Lighthouse - a tapered banded tower with a glowing, framed lamp room,
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

use crate::catalogue::items::fx::light::{Lamp, lit};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings, SovereignTextureConfig};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use crate::catalogue::items::util::{
    cone, cuboid_tapered, cylinder_tapered, foundation_block, foundation_disc, id_quat, prim,
    quat_y, solid, torus,
};

/// How far the beacon's fire lights down over its own lantern room (m): its
/// gallery and the top of the tower, not the ground some sixteen metres
/// under it, which no light reaches (#1923).
const BEACON_LIGHT_M: f32 = 4.0;

/// How strongly the lamp room's glazing glows: a broad face, so at a
/// strength that holds the beacon's hue rather than blooming white.
const LAMP_ROOM_GLOW: f32 = 1.6;

/// The lamp room's glazing's radius (m), and the mullions round it.
const LAMP_ROOM_R: f32 = 1.2;
const MULLIONS: usize = 8;

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
    fn prosperity_band(&self) -> ProsperityBand {
        ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich)
    }

    fn themes(&self) -> &'static [ThemeArchetype] {
        &[
            ThemeArchetype::AncientClassical,
            ThemeArchetype::CoastalResort,
        ]
    }
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::Shore
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
        // No texture, so `uv_scale` is inert - pinned at 1.0 so it does not
        // read as a stale pre-#936 repeat count.
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

fn build_tree() -> Generator {
    let white = [0.92, 0.90, 0.86];
    let red = [0.72, 0.18, 0.14];
    let iron = [0.20, 0.21, 0.24];
    let lamp_glow = [1.0, 0.85, 0.45];

    // Foundation slab - the root. Slab base sits at the generator
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

    // Lamp room: a drum of lit glazing in an iron frame. No material here
    // is transparent, so a beacon inside a drum is never seen: the glazing
    // itself glows with the beacon's colour (#1927) - an iron drum round a
    // glowing orb read dark beside its own light - and carries the
    // beacon's light, as the part the eye reads as the lamp. Mullions and a
    // sill and head ring frame it, so by day it reads as a glazed room.
    let lamp_h = 1.8;
    let room_mid = rel(tower_top + 0.25 + lamp_h * 0.5);
    root.children.push(lit(
        prim(
            solid(cylinder_tapered(
                LAMP_ROOM_R,
                lamp_h,
                16,
                0.0,
                SovereignMaterialSettings {
                    emission_color: Fp3(lamp_glow),
                    emission_strength: Fp(LAMP_ROOM_GLOW),
                    ..band_mat(lamp_glow)
                },
            )),
            [0.0, room_mid, 0.0],
            id_quat(),
        ),
        Lamp::Fire {
            over: BEACON_LIGHT_M,
        },
    ));
    for k in 0..MULLIONS {
        let a = k as f32 * std::f32::consts::TAU / MULLIONS as f32;
        root.children.push(prim(
            cuboid_tapered([0.07, lamp_h, 0.07], 0.0, band_mat(iron)),
            [
                (LAMP_ROOM_R + 0.01) * a.sin(),
                room_mid,
                (LAMP_ROOM_R + 0.01) * a.cos(),
            ],
            quat_y(a),
        ));
    }
    // Sill and head rings, a hair inside the glazing's ends so no face of
    // theirs lies in one of its caps.
    for dy in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cylinder_tapered(LAMP_ROOM_R + 0.07, 0.12, 16, 0.0, band_mat(iron)),
            [0.0, room_mid + dy * (lamp_h * 0.5 - 0.05), 0.0],
            id_quat(),
        ));
    }

    // Cone roof above the lamp room.
    root.children.push(prim(
        solid(cone(1.4, 1.3, 16, band_mat(red))),
        [0.0, rel(tower_top + 0.25 + lamp_h + 0.65), 0.0],
        id_quat(),
    ));

    // Keeper's hut: small block + pyramid roof. Taper tops out at the
    // sanitiser's 0.99 cap - visually identical to a true apex.
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

    /// Its lamp room's glazing glows and carries the beacon's light.
    #[test]
    fn has_emissive_beacon() {
        fn lit_glow(g: &Generator) -> bool {
            let own = g.light.is_some()
                && g.kind
                    .material()
                    .is_some_and(|m| m.emission_strength.0 > 1.0);
            own || g.children.iter().any(lit_glow)
        }
        assert!(
            lit_glow(&Lighthouse.build("")),
            "lighthouse lost its glowing beacon"
        );
    }
}
