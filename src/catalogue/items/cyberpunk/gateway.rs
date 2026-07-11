//! Neon Transit Gate — the Cyberpunk bespoke social gateway (#755). Replaces
//! the neutral placeholder arch for this theme: a pair of heavy dark-metal
//! transit pylons framing a lit walk-through channel, spanned by a header
//! girder carrying a neon destination marquee, with hot cyan jamb tubes,
//! an amber hazard band, a floor threshold runway and a lime "this-way"
//! chevron over the mouth. The one functional element is the single
//! [`GeneratorKind::Gateway`] zone centred in the opening — walking into it
//! opens the destination picker.
//!
//! Emissive discipline (HDR + bloom): the broad marquee tiles stay moderate
//! (`~1.9–2.4`) so they read as lit colour, while the thin jamb tubes, frame
//! border, floor runway and chevron run hot (`~5–6`) for a white-hot neon
//! core with a coloured halo.

use crate::catalogue::items::util::{
    cuboid_tapered, foundation_block, glow, id_quat, prim, quat_z, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, NEON_LIME, NEON_MAGENTA, fx, metal};

pub struct CyberpunkGateway;

/// Warning-amber hazard banding — the one warm note against the cold neon,
/// marking the pylon feet like a live piece of transit infrastructure.
const HAZARD: [f32; 3] = [1.0, 0.62, 0.08];

impl CatalogueEntry for CyberpunkGateway {
    fn slug(&self) -> &'static str {
        "cyberpunk_gateway"
    }
    fn name(&self) -> &'static str {
        "Neon Transit Gate"
    }
    fn description(&self) -> &'static str {
        "Twin neon pylons and a lit destination marquee framing a walk-through transit channel."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.5,
            min_spawn_dist: 8.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let body = DARK_METAL;
    let slab_h = 0.3_f32;
    let foot = 5.0_f32; // slab width (X)
    let depth = 1.6_f32; // slab depth (Z)
    let px = 1.7_f32; // pylon centre X — inner faces leave a ~2.7 m mouth
    let pw = 0.7_f32; // pylon width (X)
    let pd = 0.9_f32; // pylon depth (Z)
    let pyl_h = 4.0_f32;
    let top = slab_h + pyl_h; // header springing height

    // Forecourt slab — the flat-base root; never tilt it or every child spins.
    let mut root = prim(
        solid(cuboid_tapered([foot, slab_h, depth], 0.0, metal(body))),
        [0.0, slab_h * 0.5, 0.0],
        id_quat(),
    );
    // Author every other piece in plain ground-relative Y, then rebase onto
    // the root's raised centre.
    let rel = |ground_y: f32| ground_y - slab_h * 0.5;

    // Buried plinth so a slope-snapped gate shows metal, not daylight.
    let mut base = foundation_block(foot, depth, [0.0, 0.0], 1.5);
    base.transform.translation.0[1] -= slab_h * 0.5;
    root.children.push(base);

    // ---- Pylons flanking the walk-through mouth --------------------------
    for sx in [-1.0_f32, 1.0] {
        // Splayed base plate.
        root.children.push(prim(
            solid(cuboid_tapered([pw + 0.4, 0.3, pd + 0.3], 0.0, metal(body))),
            [sx * px, rel(slab_h + 0.15), 0.0],
            id_quat(),
        ));
        // Column, lightly tapered so it reads as a cast pylon not a post. One
        // pylon carries the signature live-machine buzz.
        let mut column = prim(
            solid(cuboid_tapered([pw, pyl_h, pd], 0.06, metal(body))),
            [sx * px, rel(slab_h + pyl_h * 0.5), 0.0],
            id_quat(),
        );
        if sx < 0.0 {
            column.audio = fx::neon_buzz();
        }
        root.children.push(column);
        // Amber hazard band near the foot.
        root.children.push(prim(
            cuboid_tapered([pw + 0.06, 0.28, pd + 0.06], 0.0, glow(HAZARD, 2.5)),
            [sx * px, rel(slab_h + 0.9), 0.0],
            id_quat(),
        ));
        // Hot cyan jamb tube down the inner-front face — the illuminated
        // door-post that reads the gap as a threshold.
        root.children.push(prim(
            cuboid_tapered([0.09, pyl_h * 0.82, 0.14], 0.0, glow(NEON_CYAN, 6.0)),
            [
                sx * (px - pw * 0.5 - 0.02),
                rel(slab_h + pyl_h * 0.52),
                -0.28,
            ],
            id_quat(),
        ));
        // Beacon lamp capping each pylon.
        root.children.push(prim(
            sphere(0.11, 3, glow(NEON_MAGENTA, 6.0)),
            [sx * px, rel(top + 0.12), 0.0],
            id_quat(),
        ));
    }

    // ---- Header girder spanning the pylon tops ---------------------------
    root.children.push(prim(
        solid(cuboid_tapered([2.0 * px + pw, 0.5, 0.8], 0.0, metal(body))),
        [0.0, rel(top + 0.25), 0.0],
        id_quat(),
    ));
    // Threshold underglow — the lit top edge of the doorway, spanning the
    // mouth on the front (-Z) face.
    root.children.push(prim(
        cuboid_tapered([2.0 * px - pw, 0.1, 0.14], 0.0, glow(NEON_MAGENTA, 5.0)),
        [0.0, rel(top - 0.02), -0.32],
        id_quat(),
    ));

    // ---- Neon destination marquee (front, -Z) ----------------------------
    // Front of the girder sits at z = -0.4; the marquee mounts forward of it
    // so it faces the approach without skewering the beam. Layered
    // front-to-back: dark housing → row of lit destination tiles → hot frame.
    let cy = rel(top + 0.55);
    root.children.push(prim(
        cuboid_tapered([3.4, 1.0, 0.12], 0.0, metal(shade(body))),
        [0.0, cy, -0.46],
        id_quat(),
    ));
    // A departures-board row of lit tiles — moderate glow so the broad faces
    // hold their hue instead of blowing to white.
    let tiles = [NEON_CYAN, NEON_MAGENTA, NEON_CYAN, NEON_MAGENTA];
    for (i, c) in tiles.into_iter().enumerate() {
        let tx = (i as f32 - 1.5) * 0.72;
        root.children.push(prim(
            cuboid_tapered([0.6, 0.58, 0.05], 0.0, glow(c, 1.9 + 0.15 * i as f32)),
            [tx, cy, -0.54],
            id_quat(),
        ));
    }
    // Hot cyan frame — a crisp lit border reads the marquee as a sign.
    for sy in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cuboid_tapered([3.5, 0.16, 0.4], 0.0, glow(NEON_CYAN, 5.0)),
            [0.0, cy + sy * 0.55, -0.48],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cuboid_tapered([0.16, 1.2, 0.4], 0.0, glow(NEON_CYAN, 5.0)),
            [sx * 1.75, cy, -0.48],
            id_quat(),
        ));
    }
    // Holographic shimmer drifting off the marquee face.
    root.children
        .push(fx::rising_motes([0.0, cy, -0.7], NEON_MAGENTA, 0x6A7E_0A7E));

    // ---- "This-way" chevron over the mouth (front, -Z) -------------------
    // Two hot lime arms meeting at a downward apex — a go-signal pointing the
    // player through the opening. Mirrored Z-rotation keeps it symmetric.
    let chevron_y = rel(slab_h + 3.35);
    for (sx, angle) in [(-1.0_f32, 0.5_f32), (1.0, -0.5)] {
        root.children.push(prim(
            cuboid_tapered([0.85, 0.1, 0.06], 0.0, glow(NEON_LIME, 5.5)),
            [sx * 0.33, chevron_y, -0.34],
            quat_z(angle),
        ));
    }

    // ---- Floor threshold runway (front, -Z lead-in) ----------------------
    // Two cyan floor lines along the walkway edges, guiding the approach into
    // the mouth.
    for sx in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cuboid_tapered([0.12, 0.05, depth * 0.9], 0.0, glow(NEON_CYAN, 4.0)),
            [sx * 1.25, rel(slab_h + 0.03), 0.0],
            id_quat(),
        ));
    }

    // ---- The functional zone --------------------------------------------
    // Bottom at the slab top, spanning floor-to-header through the mouth.
    root.children.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, rel(slab_h + 1.75), 0.0],
        id_quat(),
    ));

    root
}

/// A darker shade of a body colour — the recessed marquee housing.
fn shade(c: [f32; 3]) -> [f32; 3] {
    [c[0] * 0.6, c[1] * 0.6, c[2] * 0.6]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CyberpunkGateway.build(""), "cyberpunk_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is furniture, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = CyberpunkGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
