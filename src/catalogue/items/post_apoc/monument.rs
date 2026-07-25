//! Owner Scrap Shrine — the Post-Apocalyptic identity monument (#975).
//!
//! Somebody welded this: a rusted plate frame on a broken concrete footing,
//! the room owner's picture behind a sheet-steel surround with a salvaged work
//! light clamped over it, rebar stubs sticking out of the base and a stack of
//! tyres wedged against one side.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, glow, id_quat, nest, pfp_panel, prim, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, CORRUGATED_RUST, RUST_BROWN, SIGNAL_RED, STEEL_GREY, WORKLIGHT, concrete,
    rebar_stubs, rubble_chunks, rusted, sheet, tyre_stack,
};

const PANEL: f32 = 1.8;
const PANEL_Y: f32 = 3.15;

pub struct PostApocMonument;

impl CatalogueEntry for PostApocMonument {
    fn slug(&self) -> &'static str {
        "post_apoc_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Scrap Shrine"
    }
    fn description(&self) -> &'static str {
        "Welded scrap frame on broken concrete, holding the room owner's picture."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::PostApoc]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.7,
            min_spawn_dist: 8.0,
        }
    }
    fn build(&self, local_did: &str) -> Generator {
        build_tree(local_did)
    }
}

fn build_tree(did: &str) -> Generator {
    // Broken slab — the root, and flat, so the canted posts above spin
    // nothing.
    let footing = prim(
        solid(cuboid_tapered(
            [3.4, 0.36, 1.7],
            0.1,
            concrete(CONCRETE_GREY),
        )),
        [0.0, 0.18, 0.0],
        id_quat(),
    );

    let mut parts = Vec::new();
    for (sx, lean) in [(-1.0_f32, 0.05_f32), (1.0, -0.03)] {
        parts.push(post(sx * 1.32, lean));
    }
    parts.push(nest(header(), board(did)));
    parts.extend(rubble_chunks([-1.5, 0.36, -0.5], 0.9, 0.16, 5));
    parts.extend(rebar_stubs([1.3, 0.36, -0.55], 0.6, 4));
    parts.extend(tyre_stack([1.75, 0.36, 0.45], 0.4));
    nest(footing, parts)
}

/// A scavenged post, canted a little because nothing here was set plumb. The
/// lean lives on the post, which carries only its own foot plate.
fn post(x: f32, lean: f32) -> Generator {
    let shaft = prim(
        solid(cuboid_tapered([0.24, 4.4, 0.24], 0.04, rusted(RUST_BROWN))),
        [x, 2.56, 0.0],
        quat_z(lean),
    );
    nest(
        shaft,
        vec![prim(
            solid(cuboid_tapered([0.42, 0.1, 0.42], 0.0, sheet(STEEL_GREY))),
            [x, 0.42, 0.0],
            id_quat(),
        )],
    )
}

/// The welded header, and everything hanging off it.
fn header() -> Generator {
    prim(
        solid(cuboid_tapered(
            [3.2, 0.24, 0.26],
            0.0,
            rusted(CORRUGATED_RUST),
        )),
        [0.0, 4.6, 0.0],
        id_quat(),
    )
}

/// The board: a riveted sheet, the picture, a heavy welded surround, a clamped
/// work light and a scrap of red warning tape.
fn board(did: &str) -> Vec<Generator> {
    let z = -0.15;
    let fr = 0.16;
    let mut out = vec![
        // Sheet backing — the panel is single-sided, and this is the plate it
        // was bolted to.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.44, PANEL + 0.44, 0.09],
                0.0,
                sheet(STEEL_GREY),
            )),
            [0.0, PANEL_Y, z + 0.06],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, z]),
        // A strip of warning tape across a corner, the theme's one flash of
        // colour.
        prim(
            solid(cuboid_tapered([1.0, 0.14, 0.05], 0.0, sheet(SIGNAL_RED))),
            [-0.5, PANEL_Y + PANEL * 0.5 - 0.2, z - 0.05],
            quat_z(0.22),
        ),
        // Salvaged work light clamped to the header, aimed down the board. The
        // picture is unlit and reads on its own; this is what makes the rust
        // read at night.
        prim(
            solid(cylinder_tapered(0.2, 0.24, 10, 0.45, rusted(RUST_BROWN))),
            [0.85, 4.36, z - 0.28],
            id_quat(),
        ),
        prim(
            cuboid_tapered([0.2, 0.1, 0.14], 0.0, glow(WORKLIGHT, 2.1)),
            [0.85, 4.28, z - 0.36],
            id_quat(),
        ),
    ];
    // Welded surround — deliberately uneven, because it was cut with an angle
    // grinder by somebody in a hurry.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [fr, PANEL + fr * 2.0, 0.13],
                0.0,
                rusted(RUST_BROWN),
            )),
            [sx * (PANEL + fr) * 0.5, PANEL_Y, z - 0.03],
            quat_z(sx * 0.02),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered([PANEL, fr, 0.13], 0.0, rusted(RUST_BROWN))),
            [0.0, PANEL_Y + sy * (PANEL + fr) * 0.5, z - 0.03],
            id_quat(),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{assert_owner_panel, assert_sanitize_stable};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(
            &PostApocMonument.build("did:plc:test"),
            "post_apoc_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&PostApocMonument, "did:plc:test");
    }
}
