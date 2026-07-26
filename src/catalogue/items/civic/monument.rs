//! Owner Stele — the cross-theme fallback identity monument (#975), and the
//! reference every bespoke themed monument is built against.
//!
//! Every seeded room stands one of these beside its social gateway, turned to
//! face the arrival landing: a stepped stone plinth carrying a stele whose
//! bronze frame holds the room owner's profile picture on a square panel. It
//! is the first thing a visitor sees, and it answers the first question they
//! have — whose room is this.
//!
//! `themes()` is left empty on purpose. The seeded wiring reaches a bespoke
//! monument first via `entries_for(theme, Monument)`; this one is the
//! `by_slug("civic_monument")` fallback behind it, so a future theme that
//! ships without a monument still gets one.
//!
//! # The three things every monument in this family has to get right
//!
//! 1. **The panel is [`pfp_panel`] and nothing else.** Square, `uv_scale` 1.0,
//!    unlit, single-sided. See that helper for why each of those is not
//!    negotiable.
//! 2. **It has to read finished with the panel blank.** The image arrives over
//!    the network or not at all — a room owner with no picture leaves it at
//!    its tint forever, and the headless render tool never fetches one. So the
//!    *frame* carries the design: if the monument only works once a face
//!    appears in it, it does not work.
//! 3. **A backing plate behind the panel.** The panel is single-sided, so
//!    without one the monument is see-through from behind. The plate is also
//!    what the portrait reads as being fixed *to*.

use crate::catalogue::items::util::{cuboid_tapered, glow, id_quat, nest, pfp_panel, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{bronze, marble, stone};

/// Panel side in metres. Monument scale: readable from the landing, which is
/// the far side of the gate's forecourt.
const PANEL: f32 = 1.8;
/// Height of the panel's centre above the ground.
const PANEL_Y: f32 = 3.3;
/// Frame and lettering bronze.
const BRONZE: [f32; 3] = [0.52, 0.40, 0.20];
/// Plinth and stele stone.
const STONE: [f32; 3] = [0.62, 0.60, 0.56];
const PALE: [f32; 3] = [0.78, 0.76, 0.72];
/// Lamp flame — deep-saturated amber at low strength, so it reads as a colour
/// under bloom instead of washing to a white blank.
const FLAME: [f32; 3] = [1.0, 0.66, 0.28];

pub struct CivicMonument;

impl CatalogueEntry for CivicMonument {
    fn slug(&self) -> &'static str {
        "civic_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Stele"
    }
    fn description(&self) -> &'static str {
        "Stepped stone stele with a bronze-framed portrait of the room's owner."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    // themes() stays empty: this is the cross-theme fallback, reached by slug
    // only when a theme ships without its own monument.
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[]
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
    // Stepped plinth: root at the bottom, each step standing on the one below.
    let step0 = prim(
        solid(cuboid_tapered([3.4, 0.34, 2.4], 0.0, stone(STONE))),
        [0.0, 0.17, 0.0],
        id_quat(),
    );
    let step1 = prim(
        solid(cuboid_tapered([2.8, 0.3, 1.9], 0.0, stone(STONE))),
        [0.0, 0.49, 0.0],
        id_quat(),
    );

    // The stele the portrait is set into, and the coping that caps it.
    let stele = prim(
        solid(cuboid_tapered([2.3, 4.1, 0.66], 0.03, marble(PALE))),
        [0.0, 2.69, 0.0],
        id_quat(),
    );

    nest(
        step0,
        vec![nest(
            step1,
            vec![
                nest(stele, portrait(did)),
                // Two flame bowls on the plinth. The panel is unlit and reads
                // at any hour on its own; these are for the *stone*, which
                // otherwise goes flat at dusk.
                lamp(-1.35),
                lamp(1.35),
            ],
        )],
    )
}

/// The portrait assembly: a recessed bronze surround, the backing plate, the
/// panel itself, and the coping over it.
///
/// The frame is authored as four bars rather than one slab with the panel on
/// top, so the portrait sits *in* a reveal — the same reason a window gets a
/// reveal instead of a sticker.
fn portrait(did: &str) -> Vec<Generator> {
    let front = -0.39;
    let bar = 0.16;
    let mut out = vec![
        // Backing plate. The panel is single-sided; this is what it is fixed
        // to, and what stops the monument being see-through from behind.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.1, PANEL + 0.1, 0.08],
                0.0,
                bronze([0.30, 0.24, 0.14]),
            )),
            [0.0, PANEL_Y, front + 0.1],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, front + 0.03]),
        // Coping over the stele, oversailing it so the head is not a cut edge.
        prim(
            solid(cuboid_tapered([2.6, 0.26, 0.9], 0.06, stone(STONE))),
            [0.0, 4.87, 0.0],
            id_quat(),
        ),
        // Bronze finial disc, the one thing above the coping.
        prim(
            solid(cuboid_tapered([0.7, 0.34, 0.18], 0.3, bronze(BRONZE))),
            [0.0, 5.17, 0.0],
            id_quat(),
        ),
        // Dedication band under the portrait — blank bronze, because there is
        // no text renderer; it reads as the plaque a name would be cut into.
        prim(
            solid(cuboid_tapered([1.5, 0.28, 0.07], 0.0, bronze(BRONZE))),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.42, front + 0.02],
            id_quat(),
        ),
    ];
    // Frame bars: two stiles and two rails, standing proud of the plate.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [bar, PANEL + bar * 2.0, 0.14],
                0.0,
                bronze(BRONZE),
            )),
            [sx * (PANEL + bar) * 0.5, PANEL_Y, front - 0.02],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered([PANEL, bar, 0.14], 0.0, bronze(BRONZE))),
            [0.0, PANEL_Y + sy * (PANEL + bar) * 0.5, front - 0.02],
            id_quat(),
        ));
    }
    out
}

/// A flame bowl on a short post, standing on the plinth.
fn lamp(x: f32) -> Generator {
    let post = prim(
        solid(cuboid_tapered([0.2, 1.15, 0.2], 0.2, stone(STONE))),
        [x, 1.21, -0.55],
        id_quat(),
    );
    nest(
        post,
        vec![
            prim(
                solid(cuboid_tapered([0.46, 0.22, 0.46], 0.35, bronze(BRONZE))),
                [x, 1.86, -0.55],
                id_quat(),
            ),
            prim(
                cuboid_tapered([0.26, 0.2, 0.26], 0.5, glow(FLAME, 2.2)),
                [x, 2.0, -0.55],
                id_quat(),
            ),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CivicMonument.build("did:plc:test"), "civic_monument");
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        crate::catalogue::items::util::assert_owner_panel(&CivicMonument, "did:plc:test");
    }
}
