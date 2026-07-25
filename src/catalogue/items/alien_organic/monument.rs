//! Owner Spore Bloom — the Alien-Organic identity monument (#975).
//!
//! A grown thing rather than a built one: a chitinous stalk rises from a
//! fleshy mound and opens into a membrane bract, and the room owner's likeness
//! is held in the bract the way a spore print is — with biolume veins running
//! up the stalk and two sacs pulsing at its foot.
//!
//! The theme's monument is the one that most has to survive a blank panel,
//! because "an empty membrane" is a perfectly good thing for a hive to grow.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, glow, id_quat, nest, pfp_panel, prim, prim_scaled, quat_z,
    solid, sphere, superellipsoid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BIOLUME_CYAN, CHITIN_DARK, CHITIN_GREEN, FLESH_RED, MEMBRANE_TEAL, SAC_GLOW, chitin, flesh,
    membrane,
};

const PANEL: f32 = 1.8;
const PANEL_Y: f32 = 3.2;

pub struct AlienOrganicMonument;

impl CatalogueEntry for AlienOrganicMonument {
    fn slug(&self) -> &'static str {
        "alien_organic_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Spore Bloom"
    }
    fn description(&self) -> &'static str {
        "Chitinous stalk opening into a membrane bract that holds the room owner's likeness."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienOrganic]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.6,
            min_spawn_dist: 8.0,
        }
    }
    fn build(&self, local_did: &str) -> Generator {
        build_tree(local_did)
    }
}

fn build_tree(did: &str) -> Generator {
    // Fleshy mound — the root, and flat-based, so nothing above inherits a
    // tilt from it.
    let mound = prim_scaled(
        solid(superellipsoid([1.7, 0.55, 1.2], 0.7, 0.7, flesh(FLESH_RED))),
        [0.0, 0.4, 0.0],
        id_quat(),
        [1.0, 1.0, 1.0],
    );
    let stalk = prim(
        solid(cylinder_tapered(0.42, 3.4, 14, 0.35, chitin(CHITIN_DARK))),
        [0.0, 2.3, 0.1],
        id_quat(),
    );

    nest(
        mound,
        vec![nest(stalk, bract(did)), sac(-1.15, 0.34), sac(1.05, 0.28)],
    )
}

/// The bract: a chitin cup, the membrane the likeness prints on, its backing,
/// and the veins that light the stalk.
fn bract(did: &str) -> Vec<Generator> {
    let z = -0.36;
    let rim = 0.15;
    let mut out = vec![
        // Membrane backing — the panel is single-sided, and a bract is a
        // closed sheath from behind.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.34, PANEL + 0.34, 0.12],
                0.3,
                membrane(MEMBRANE_TEAL),
            )),
            [0.0, PANEL_Y, z + 0.09],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, z]),
        // The cup the bract opens from, sitting under the panel.
        prim(
            solid(cylinder_tapered(0.85, 0.7, 14, -0.5, chitin(CHITIN_GREEN))),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.32, z + 0.42],
            id_quat(),
        ),
    ];
    // Chitin rim around the bract — four ribs, leaning outward like a calyx
    // rather than sitting square, which is what stops it reading as a frame.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [rim, PANEL + rim * 2.0, 0.16],
                0.4,
                chitin(CHITIN_GREEN),
            )),
            [sx * (PANEL + rim) * 0.5, PANEL_Y, z - 0.03],
            quat_z(sx * 0.09),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [PANEL, rim, 0.16],
                0.4,
                chitin(CHITIN_GREEN),
            )),
            [0.0, PANEL_Y + sy * (PANEL + rim) * 0.5, z - 0.03],
            id_quat(),
        ));
    }
    // Biolume veins up the stalk. The likeness is unlit and reads on its own;
    // these are what make the chitin read as alive after dark.
    for (i, sx) in [-1.0_f32, 1.0].iter().enumerate() {
        out.push(prim(
            solid(cylinder_tapered(
                0.06,
                2.2 - i as f32 * 0.4,
                8,
                0.3,
                glow(BIOLUME_CYAN, 1.5),
            )),
            [sx * 0.3, 1.5, z + 0.62],
            quat_z(sx * 0.06),
        ));
    }
    out
}

/// A pulsing sac at the mound's edge — the theme's signature, and the reason
/// the monument reads as grown rather than assembled.
fn sac(x: f32, r: f32) -> Generator {
    let body = prim_scaled(
        sphere(r, 6, flesh(FLESH_RED)),
        [x, 0.62, -0.62],
        id_quat(),
        [1.0, 0.82, 1.0],
    );
    nest(
        body,
        vec![prim(
            solid(sphere(r * 0.45, 6, glow(SAC_GLOW, 1.4))),
            [x, 0.62 + r * 0.5, -0.62],
            id_quat(),
        )],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{assert_owner_panel, assert_sanitize_stable};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(
            &AlienOrganicMonument.build("did:plc:test"),
            "alien_organic_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&AlienOrganicMonument, "did:plc:test");
    }
}
