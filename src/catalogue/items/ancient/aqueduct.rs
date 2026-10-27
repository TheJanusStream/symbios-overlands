//! Aqueduct - an AncientClassical leader for a valley floor. A stretch of
//! Roman aqueduct striding across the flat: two tiers of round arches in
//! coursed sandstone, the open water channel (the specus) running along
//! its crown, and at its +X end the castellum, a small square distribution
//! house whose bronze pipes spill into a basin on its -Z front.
//!
//! The long axis runs along X and the arches open through Z, so both long
//! faces show the arcade; the castellum's door, tablet and basin face -Z.
//!
//! Arches are not cut from a wall - the primitive family has no booleans -
//! so each bay is a half-tube voussoir ring standing a few centimetres
//! proud of a wall built round it: piers rising past the springing, a
//! haunch block between the rings, and a continuous band above. See
//! [`arcade`] for the rule that keeps those blocks out of the openings.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::coastal_resort::{POOL_AQUA, water};
use crate::catalogue::items::util::{
    attach, cuboid_tapered, cylinder_tapered, footing, foundation_block, id_quat, nest, prim,
    quat_x, solid, sphere, tint, torus, tube, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRONZE_GREEN, MARBLE_WHITE, SANDSTONE_GOLD, SANDSTONE_WEATHERED, STONE_VOID, TERRACOTTA,
    bronze, marble, roof_tile, sandstone,
};

pub struct Aqueduct;

/// Spacing circle the settlement keeps round the aqueduct: half its length.
const CLEARANCE: f32 = 21.5;
/// Centre-to-centre spacing of the piers, both tiers.
const BAY: f32 = 5.0;
/// Bays in the arcade.
const BAYS: usize = 7;
/// Centre of the first (-X) pier.
const X0: f32 = -20.25;
/// Depth (Z) of the lower and the upper tier's arch rings.
const DEPTH_LO: f32 = 3.0;
const DEPTH_HI: f32 = 2.5;
/// How far a voussoir ring stands proud of the wall built round it.
const RING_PROUD: f32 = 0.04;
/// Castellum: its side, and the x of its centre (it abuts the last pier).
const CAST_W: f32 = 5.5;
const CAST_H: f32 = 14.0;

impl CatalogueEntry for Aqueduct {
    fn slug(&self) -> &'static str {
        "aqueduct"
    }
    fn name(&self) -> &'static str {
        "Aqueduct"
    }
    fn description(&self) -> &'static str {
        "Two tiers of sandstone arches carrying a water channel, ending in a square castellum."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1796).
    fn leads(&self) -> bool {
        true
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AncientClassical]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ANCIENT_BAND
    }
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::ValleyFloor
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 45.0,
        }
    }
    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// x of the castellum's centre: it abuts the last pier's outer face.
fn castellum_x() -> f32 {
    X0 + BAYS as f32 * BAY + PIER_LO * 0.5 + CAST_W * 0.5
}

/// Lower and upper pier widths (X).
const PIER_LO: f32 = 1.7;
const PIER_HI: f32 = 1.5;

/// One tier of the arcade, as the top-level parts it falls apart into: a
/// pier for each bay end carrying its haunch blocks, a voussoir ring for
/// each bay, and the band
/// across the top. `base` is the floor the piers stand on, `spring` the
/// springing line, `top` the band's top; `ring_t` the ring's thickness.
///
/// Every block between two rings is kept inside the ring's annulus at the
/// heights it spans, so its corners hide behind the proud ring and none of
/// it reaches into an opening. For a block from `h0` to `h1` above the
/// springing that holds when `h1^2 - h0^2 <= Ro^2 - Ri^2`, which is the step
/// the blocks climb by.
#[allow(clippy::too_many_arguments)]
fn arcade(
    pier_w: f32,
    ring_t: f32,
    depth: f32,
    base: f32,
    spring: f32,
    top: f32,
    band_end: f32,
    ring_tint: [f32; 3],
) -> Vec<Generator> {
    let wall = depth - 2.0 * RING_PROUD;
    let ri = (BAY - pier_w) * 0.5;
    let ro = ri + ring_t;
    let step2 = (ro * ro - ri * ri) * 0.95;
    // Heights (above the springing) the blocks climb by, up to the crown.
    let mut hs = vec![0.0_f32];
    while *hs.last().expect("starts at zero") < ri {
        let h = *hs.last().expect("non-empty");
        hs.push((h * h + step2).sqrt());
    }
    let band_from = spring + *hs.last().expect("non-empty");
    let half = BAY * 0.5;
    let x_ext = |h: f32| half - (ro * ro - h * h).max(0.0).sqrt();
    let x_int = |h: f32| half - (ri * ri - h * h).max(0.0).sqrt();

    let mut parts = Vec::new();
    for i in 0..=BAYS {
        let x = X0 + i as f32 * BAY;
        let pier_top = spring + hs[1];
        // The last pier runs on into the castellum's wall, as the band does.
        let (x_lo, x_hi) = if i == BAYS {
            (x - pier_w * 0.5, band_end)
        } else {
            (x - pier_w * 0.5, x + pier_w * 0.5)
        };
        let pier = prim(
            solid(cuboid_tapered(
                [x_hi - x_lo, pier_top - base, wall],
                0.0,
                sandstone(SANDSTONE_GOLD),
            )),
            [(x_lo + x_hi) * 0.5, (base + pier_top) * 0.5, 0.0],
            id_quat(),
        );
        // An end pier carries its haunches on its inner side only; its outer
        // side is the pier's own face, up to the band.
        let mut haunches = Vec::new();
        for w in hs.windows(2).skip(1) {
            let (h0, h1) = (w[0], w[1]);
            let hx = (x_ext(h1) + 0.02).min(x_int(h0) - 0.005);
            let (lo, hi) = match i {
                0 => (x_lo, x + hx),
                _ if i == BAYS => (x - hx, x_hi),
                _ => (x - hx, x + hx),
            };
            haunches.push(prim(
                solid(cuboid_tapered(
                    [hi - lo, h1 - h0, wall],
                    0.0,
                    sandstone(SANDSTONE_GOLD),
                )),
                [(lo + hi) * 0.5, spring + (h0 + h1) * 0.5, 0.0],
                id_quat(),
            ));
        }
        parts.push(nest(pier, haunches));
    }
    for b in 0..BAYS {
        let x = X0 + (b as f32 + 0.5) * BAY;
        parts.push(prim(
            solid(with_cut(
                tube(ro, ri, depth, 28, sandstone(ring_tint)),
                [0.0, 0.5],
                [0.0, 1.0],
                0.0,
            )),
            [x, spring, 0.0],
            quat_x(-FRAC_PI_2),
        ));
    }
    // The band over the crowns, from the first pier's outer face to
    // `band_end`.
    let x_lo = X0 - pier_w * 0.5;
    parts.push(prim(
        solid(cuboid_tapered(
            [band_end - x_lo, top - band_from, wall],
            0.0,
            sandstone(SANDSTONE_GOLD),
        )),
        [(x_lo + band_end) * 0.5, (band_from + top) * 0.5, 0.0],
        id_quat(),
    ));
    parts
}

/// A projecting cornice course along the arcade, its top `rise` above the
/// band it caps so the two tops never share a plane.
fn cornice(top: f32, depth: f32, band_end: f32, pier_w: f32) -> Generator {
    let x_lo = X0 - pier_w * 0.5 - 0.12;
    let h = 0.36;
    let rise = 0.05;
    prim(
        solid(cuboid_tapered(
            [band_end - x_lo, h, depth + 0.24],
            0.0,
            sandstone(SANDSTONE_WEATHERED),
        )),
        [(x_lo + band_end) * 0.5, top + rise - h * 0.5, 0.0],
        id_quat(),
    )
}

fn build_tree() -> Generator {
    let cx = castellum_x();
    // Bands and cornices run 0.2 m into the castellum's wall.
    let into_cast = cx - CAST_W * 0.5 + 0.2;
    let ring_tint = tint(SANDSTONE_GOLD, [1.08, 1.07, 1.05]);

    // ── Lower tier ──
    let spring_lo = 5.0;
    let top_lo = 7.7;
    let mut lower = arcade(
        PIER_LO, 0.6, DEPTH_LO, 0.0, spring_lo, top_lo, into_cast, ring_tint,
    );
    let cornice_lo = cornice(top_lo, DEPTH_LO, into_cast, PIER_LO);
    let floor_hi = top_lo + 0.05;

    // ── Upper tier ──
    let spring_hi = floor_hi + 2.2;
    let top_hi = spring_hi + 1.75 + 0.5 + 0.35;
    let upper = arcade(
        PIER_HI, 0.5, DEPTH_HI, floor_hi, spring_hi, top_hi, into_cast, ring_tint,
    );
    let floor_ch = top_hi + 0.05;

    // ── The specus: two channel walls with coping, the water between ──
    let ch_len = into_cast - (X0 - PIER_HI * 0.5 - 0.12);
    let ch_x = (into_cast + X0 - PIER_HI * 0.5 - 0.12) * 0.5;
    let wall_h = 1.1;
    let mut channel = vec![];
    for sz in [-1.0_f32, 1.0] {
        let wall = prim(
            solid(cuboid_tapered(
                [ch_len, wall_h, 0.45],
                0.0,
                sandstone(SANDSTONE_GOLD),
            )),
            [ch_x, floor_ch + wall_h * 0.5, sz * 0.725],
            id_quat(),
        );
        let coping = prim(
            solid(cuboid_tapered(
                [ch_len + 0.1, 0.16, 0.62],
                0.0,
                sandstone(SANDSTONE_WEATHERED),
            )),
            [ch_x - 0.05, floor_ch + wall_h + 0.08, sz * 0.725],
            id_quat(),
        );
        channel.push(nest(wall, vec![coping]));
    }
    // The channel's -X end is walled across; the water runs from it into
    // the castellum.
    let ch_lo = ch_x - ch_len * 0.5;
    channel.push(prim(
        solid(cuboid_tapered(
            [0.45, wall_h, 1.0],
            0.0,
            sandstone(SANDSTONE_GOLD),
        )),
        [ch_lo + 0.225, floor_ch + wall_h * 0.5, 0.0],
        id_quat(),
    ));
    let water_lo = ch_lo + 0.45;
    channel.push(prim(
        cuboid_tapered([into_cast - water_lo, 0.55, 1.04], 0.0, water(POOL_AQUA)),
        [(water_lo + into_cast) * 0.5, floor_ch + 0.275, 0.0],
        id_quat(),
    ));
    let cornice_hi = nest(cornice(top_hi, DEPTH_HI, into_cast, PIER_HI), channel);

    // ── Castellum ──
    let castellum = castellum(cx);

    // The root: a footing under the arcade, the castellum on its own.
    let run_lo = X0 - PIER_LO * 0.5;
    let run_hi = cx - CAST_W * 0.5;
    let mut root = footing(
        run_hi - run_lo,
        DEPTH_LO + 0.3,
        [(run_lo + run_hi) * 0.5, 0.0],
        CLEARANCE,
    );
    for p in lower.drain(..) {
        attach(&mut root, p);
    }
    attach(&mut root, cornice_lo);
    for p in upper {
        attach(&mut root, p);
    }
    attach(&mut root, cornice_hi);
    attach(&mut root, castellum);
    root
}

/// The castellum divisorium: a square sandstone house the channel runs
/// into at the top, with a door, a dedication tablet and four bronze
/// pipes on its -Z front feeding a basin, under a tiled pyramid roof.
fn castellum(cx: f32) -> Generator {
    let w = CAST_W;
    let fz = -w * 0.5; // the -Z face
    let depth = crate::catalogue::items::foundation::required_depth(CLEARANCE);
    // Its own footing, stopping short of the arcade's so their tops do not
    // overlap, and reaching forward under the basin.
    let base = foundation_block(w + 0.2, 8.3, [cx, -1.1], depth);

    // Body, plinth, string courses and cornice.
    let mut body_parts = vec![
        prim(
            solid(cuboid_tapered(
                [w + 0.4, 0.8, w + 0.4],
                0.0,
                sandstone(SANDSTONE_WEATHERED),
            )),
            [cx, 0.4, 0.0],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(
                [w + 0.3, 0.3, w + 0.3],
                0.0,
                marble(MARBLE_WHITE),
            )),
            [cx, 7.0, 0.0],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(
                [w + 0.5, 0.42, w + 0.5],
                0.0,
                marble(MARBLE_WHITE),
            )),
            [cx, CAST_H + 0.17, 0.0],
            id_quat(),
        ),
    ];
    // Corner pilasters, proud of both faces they turn.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            body_parts.push(prim(
                solid(cuboid_tapered(
                    [0.6, CAST_H - 0.8, 0.6],
                    0.0,
                    sandstone(SANDSTONE_WEATHERED),
                )),
                [
                    cx + sx * (w * 0.5 - 0.22),
                    0.8 + (CAST_H - 0.8) * 0.5,
                    sz * (w * 0.5 - 0.22),
                ],
                id_quat(),
            ));
        }
    }
    // Arched door: a dark recess under a marble ring.
    body_parts.push(prim(
        cuboid_tapered([1.6, 2.6, 0.3], 0.0, sandstone(STONE_VOID)),
        [cx, 0.8 + 1.3, fz - 0.1],
        id_quat(),
    ));
    body_parts.push(prim(
        with_cut(
            torus(0.16, 0.95, marble(MARBLE_WHITE)),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        ),
        [cx, 0.8 + 2.6, fz - 0.12],
        quat_x(-FRAC_PI_2),
    ));
    body_parts.push(arch_head(0.8, [cx, 0.8 + 2.6, fz - 0.1]));
    // Dedication tablet over the door, and an arched window above the
    // string course.
    body_parts.push(prim(
        solid(cuboid_tapered([2.6, 1.1, 0.16], 0.0, marble(MARBLE_WHITE))),
        [cx, 5.4, fz - 0.06],
        id_quat(),
    ));
    for k in 0..3 {
        body_parts.push(prim(
            cuboid_tapered([1.9 - k as f32 * 0.3, 0.07, 0.04], 0.0, marble(STONE_VOID)),
            [cx, 5.68 - k as f32 * 0.26, fz - 0.155],
            id_quat(),
        ));
    }
    body_parts.push(prim(
        cuboid_tapered([1.0, 1.8, 0.3], 0.0, sandstone(STONE_VOID)),
        [cx, 8.6, fz - 0.1],
        id_quat(),
    ));
    body_parts.push(prim(
        with_cut(
            torus(0.12, 0.6, marble(MARBLE_WHITE)),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        ),
        [cx, 9.5, fz - 0.12],
        quat_x(-FRAC_PI_2),
    ));
    body_parts.push(arch_head(0.5, [cx, 9.5, fz - 0.1]));

    // Roof: a low tiled pyramid with a bronze pine-cone finial.
    let roof_y = CAST_H + 0.38;
    let roof = nest(
        prim(
            solid(cuboid_tapered(
                [w + 0.9, 1.1, w + 0.9],
                0.99,
                roof_tile(TERRACOTTA),
            )),
            [cx, roof_y + 0.55, 0.0],
            id_quat(),
        ),
        vec![prim(
            sphere(0.3, 4, bronze(BRONZE_GREEN)),
            [cx, roof_y + 1.22, 0.0],
            id_quat(),
        )],
    );
    body_parts.push(roof);

    // Four bronze pipes leaving the -Z face, each spilling into the basin.
    let basin_z = fz - 1.5;
    let basin_top = 0.75;
    let mut basin_parts = Vec::new();
    for k in [-1.75_f32, -1.2, 1.2, 1.75] {
        let px = cx + k;
        body_parts.push(prim(
            cylinder_tapered(0.11, 1.0, 10, 0.0, bronze(BRONZE_GREEN)),
            [px, 1.7, fz - 0.45],
            quat_x(FRAC_PI_2),
        ));
        basin_parts.push(prim(
            cylinder_tapered(0.06, 1.0, 8, 0.0, water(POOL_AQUA)),
            [px, 1.15, fz - 0.9],
            id_quat(),
        ));
    }
    let body = nest(
        prim(
            solid(cuboid_tapered(
                [w, CAST_H, w],
                0.0,
                sandstone(SANDSTONE_GOLD),
            )),
            [cx, CAST_H * 0.5, 0.0],
            id_quat(),
        ),
        body_parts,
    );

    // The basin: a stone trough brimming with the castellum's water.
    basin_parts.push(prim(
        cuboid_tapered([4.0, 0.12, 1.3], 0.0, water(POOL_AQUA)),
        [cx, basin_top - 0.14, basin_z],
        id_quat(),
    ));
    let mut basin_wall = vec![];
    for sz in [-1.0_f32, 1.0] {
        basin_wall.push(prim(
            solid(cuboid_tapered(
                [4.6, basin_top, 0.3],
                0.0,
                marble(MARBLE_WHITE),
            )),
            [cx, basin_top * 0.5, basin_z + sz * 0.8],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        basin_wall.push(prim(
            solid(cuboid_tapered(
                [0.3, basin_top, 1.3],
                0.0,
                marble(MARBLE_WHITE),
            )),
            [cx + sx * 2.15, basin_top * 0.5, basin_z],
            id_quat(),
        ));
    }
    basin_parts.extend(basin_wall);
    let mut basin = nest(
        prim(
            solid(cuboid_tapered(
                [4.4, 0.2, 1.4],
                0.0,
                sandstone(SANDSTONE_WEATHERED),
            )),
            [cx, 0.1, basin_z],
            id_quat(),
        ),
        basin_parts,
    );
    basin.audio = crate::catalogue::items::fx::water_trickle();

    nest(base, vec![body, basin])
}

/// The dark round head of an arched opening: a half disc of `r` standing
/// on `at`, its face to -Z.
fn arch_head(r: f32, at: [f32; 3]) -> Generator {
    prim(
        with_cut(
            cylinder_tapered(r, 0.3, 20, 0.0, sandstone(STONE_VOID)),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        ),
        at,
        quat_x(-FRAC_PI_2),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Aqueduct.build(""), "aqueduct");
    }

    #[test]
    fn nothing_z_fights() {
        assert_no_z_fighting(&Aqueduct.build(""), "aqueduct");
    }

    #[test]
    fn nothing_floats() {
        assert_nothing_floats(&Aqueduct.build(""), "aqueduct");
    }

    /// The run is 36 to 44 m long and 12 to 16 m tall, along X.
    #[test]
    fn it_spans_a_valley() {
        let solids = crate::catalogue::items::measure::solids(&Aqueduct.build(""));
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for s in &solids {
            for i in 0..3 {
                lo[i] = lo[i].min(s.bounds.min[i]);
                hi[i] = hi[i].max(s.bounds.max[i]);
            }
        }
        let len = hi[0] - lo[0];
        assert!((36.0..=44.0).contains(&len), "{len} m long");
        assert!((12.0..=16.0).contains(&hi[1]), "{} m tall", hi[1]);
        assert!(hi[2] - lo[2] < 12.0, "{} m deep", hi[2] - lo[2]);
    }

    #[test]
    fn record_stays_inside_the_budget() {
        let bytes = crate::pds::record_size::serialized_record_bytes(&Aqueduct.build(""))
            .expect("serialises");
        eprintln!("aqueduct record: {bytes} B");
        assert!(bytes < 80 * 1024, "{bytes} B");
    }
}
