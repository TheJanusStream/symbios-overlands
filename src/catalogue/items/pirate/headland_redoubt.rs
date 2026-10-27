//! Headland redoubt - a Pirate leader for the crest. A lookout fort on the
//! headland above the anchorage: a low angular bastion of battered limestone
//! whose salient points out to sea (-Z), four guns run out through the
//! embrasures of its two faces, and behind them on the terreplein a tall
//! flagstaff flying the black colours, a squat powder house and a timber
//! lookout tower.
//!
//! Where the harbour battery is a long casemated wall at the waterline, this
//! is a solitary work on high ground: one arrowhead of rampart open to the
//! sky, read by its point and its flag.
//!
//! Each run of parapet carries the guns that fire through it, built in its
//! own frame and turned to its face (see [`stand`]).

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use crate::catalogue::items::util::{
    attach, cuboid_tapered, cylinder_tapered, footing, id_quat, nest, prim, quat_x, quat_y, quat_z,
    railing, solid, sphere, strut, superellipsoid, with_cut, with_face,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRONZE_GUN, ENSIGN_RED, GOLD_LEAF, HULL_OAK, HULL_TAR, IRON_BLACK, OAK_JOINERY, PORT_BAND,
    ROPE_HEMP, SHINGLE_GREY, STONE_LIME, STONE_QUAY, ashlar, board, bronze, cobbles, fx, hemp,
    iron, jolly_roger, lantern, shingle, strake,
};

pub struct HeadlandRedoubt;

/// Spacing circle: the footing's half diagonal.
const CLEARANCE: f32 = 15.0;
/// The rampart's half width at its foot, and its batter (the taper).
const HALF: f32 = 9.0;
const BATTER: f32 = 0.04;
/// The rear block's z span at the foot, and the diamond's centre.
const REAR: [f32; 2] = [-2.0, 8.0];
const DIAMOND_Z: f32 = -2.0;
/// Terreplein heights: the rear block's, and the salient's a hair above it
/// so their tops are not one plane.
const DECK: f32 = 3.0;
const DECK_SALIENT: f32 = 3.03;
/// The parapet's depth, and how far its outer face stands in from the edge.
const PARAPET_D: f32 = 0.9;
const PARAPET_IN: f32 = 0.1;

impl CatalogueEntry for HeadlandRedoubt {
    fn slug(&self) -> &'static str {
        "headland_redoubt"
    }
    fn name(&self) -> &'static str {
        "Headland Redoubt"
    }
    fn description(&self) -> &'static str {
        "Angular stone bastion on a headland: guns in its embrasures, the black colours, a powder house and a lookout."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1796).
    fn leads(&self) -> bool {
        true
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Pirate]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        PORT_BAND
    }
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::Crest
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Stand parts built in their own frame (the first their root) at `at`,
/// turned by `yaw` about the frame's origin.
fn stand(mut parts: Vec<Generator>, at: [f32; 3], yaw: f32) -> Generator {
    let root = parts.remove(0);
    let mut g = nest(root, parts);
    let [x, y, z] = g.transform.translation.0;
    let (s, c) = yaw.sin_cos();
    g.transform.translation.0 = [at[0] + x * c + z * s, at[1] + y, at[2] - x * s + z * c];
    g.transform.rotation = quat_y(yaw);
    g
}

fn wall() -> crate::pds::SovereignMaterialSettings {
    ashlar(STONE_LIME, 0xBA57)
}

/// A gun on a garrison carriage, in its own frame: run out to -Z with its
/// muzzle at `muzzle_z`, the carriage on the deck at `deck`. The barrel is
/// a leaf carrying its own quarter turn; everything hangs off the flat bed.
fn cannon(x: f32, deck: f32, muzzle_z: f32) -> Vec<Generator> {
    let len = 2.2;
    let r = 0.17;
    let bore = deck + 0.84;
    let breech = muzzle_z + len;
    let bed_lo = deck + 0.26;
    let bed_hi = deck + 0.40;
    let mut carried = vec![
        prim(
            solid(with_cut(
                cylinder_tapered(r, len, 12, 0.3, bronze(BRONZE_GUN, 0x9A)),
                [0.0, 1.0],
                [0.0, 1.0],
                0.42,
            )),
            [x, bore, muzzle_z + len * 0.5],
            quat_x(-FRAC_PI_2),
        ),
        prim(
            sphere(r * 0.66, 3, bronze(BRONZE_GUN, 0x9A)),
            [x, bore, breech + r * 0.3],
            id_quat(),
        ),
    ];
    let trunnion_z = muzzle_z + len * 0.6;
    // Cheeks up to the trunnions, red as a sea-service carriage was.
    for sx in [-1.0_f32, 1.0] {
        let h = bore + 0.02 - bed_hi;
        carried.push(prim(
            solid(cuboid_tapered([0.1, h, 1.0], 0.0, strake(ENSIGN_RED))),
            [x + sx * (r + 0.06), bed_hi + h * 0.5, trunnion_z + 0.4],
            id_quat(),
        ));
    }
    carried.push(prim(
        solid(cylinder_tapered(
            0.06,
            (r + 0.13) * 2.0,
            8,
            0.0,
            bronze(BRONZE_GUN, 0x9A),
        )),
        [x, bore, trunnion_z],
        quat_z(FRAC_PI_2),
    ));
    // Four trucks under the bed.
    for sz in [trunnion_z + 0.1, trunnion_z + 0.9] {
        for sx in [-1.0_f32, 1.0] {
            carried.push(prim(
                solid(cylinder_tapered(
                    0.17,
                    0.09,
                    10,
                    0.0,
                    iron(IRON_BLACK, 0x9B),
                )),
                [x + sx * (r + 0.12), deck + 0.17, sz],
                quat_z(FRAC_PI_2),
            ));
        }
    }
    vec![nest(
        prim(
            solid(cuboid_tapered(
                [(r + 0.06) * 2.0 + 0.1, bed_hi - bed_lo, 1.2],
                0.0,
                strake(HULL_TAR),
            )),
            [x, (bed_lo + bed_hi) * 0.5, trunnion_z + 0.5],
            id_quat(),
        ),
        carried,
    )]
}

/// A garland of round shot as one pinched heap standing on `base`.
fn shot_pile(base: [f32; 3]) -> Generator {
    prim(
        superellipsoid([0.34, 0.28, 0.34], 1.35, 0.9, iron(IRON_BLACK, 0x9C)),
        [base[0], base[1] + 0.26, base[2]],
        id_quat(),
    )
}

/// A run of parapet `len` long in its own frame, its outer face to -Z,
/// standing on `deck` with its top `h` above it: merlons either side of an
/// embrasure at each of `embrasures`, a low sole wall in each, and, given a
/// `gun_deck`, a gun run out through each with its shot beside it.
fn parapet(
    len: f32,
    deck: f32,
    h: f32,
    embrasures: &[f32],
    gun_deck: Option<f32>,
) -> Vec<Generator> {
    let gap = 1.1;
    let sole = 0.62;
    let mut edges = vec![-len * 0.5];
    for &e in embrasures {
        edges.push(e - gap * 0.5);
        edges.push(e + gap * 0.5);
    }
    edges.push(len * 0.5);
    let mut parts = Vec::new();
    for m in edges.chunks(2) {
        parts.push(prim(
            solid(cuboid_tapered([m[1] - m[0], h, PARAPET_D], 0.0, wall())),
            [(m[0] + m[1]) * 0.5, deck + h * 0.5, 0.0],
            id_quat(),
        ));
    }
    for &e in embrasures {
        parts.push(prim(
            solid(cuboid_tapered([gap, sole, PARAPET_D], 0.0, wall())),
            [e, deck + sole * 0.5, 0.0],
            id_quat(),
        ));
        if let Some(gd) = gun_deck {
            parts.extend(cannon(e, gd, -PARAPET_D * 0.5 - 0.35));
            parts.push(shot_pile([e + 1.0, gd, PARAPET_D * 0.5 + 1.2]));
        }
    }
    parts
}

/// The terreplein: the rear block and the diamond of the salient, both
/// battered, their tops cobbled.
fn rampart() -> Generator {
    let rear_len = REAR[1] - REAR[0];
    let rear = prim(
        solid(with_face(
            cuboid_tapered([HALF * 2.0, DECK, rear_len], BATTER, wall()),
            FaceKey::Top,
            cobbles(STONE_QUAY, 0xC0B),
        )),
        [0.0, DECK * 0.5, (REAR[0] + REAR[1]) * 0.5],
        id_quat(),
    );
    let side = HALF * std::f32::consts::SQRT_2;
    let diamond = prim(
        solid(with_face(
            cuboid_tapered([side, DECK_SALIENT, side], BATTER, wall()),
            FaceKey::Top,
            cobbles(STONE_QUAY, 0xC0B),
        )),
        [0.0, DECK_SALIENT * 0.5, DIAMOND_Z],
        quat_y(FRAC_PI_4),
    );
    nest(rear, vec![diamond])
}

/// The two faces of the salient, each a parapet with two guns.
fn faces() -> Vec<Generator> {
    // The salient and the shoulders, at the top of the battered rampart.
    let reach = HALF * (1.0 - BATTER);
    let salient = [0.0, DIAMOND_Z - reach];
    let len = reach * std::f32::consts::SQRT_2;
    let mut out = Vec::new();
    for (k, s) in [1.0_f32, -1.0].into_iter().enumerate() {
        let shoulder = [s * reach, DIAMOND_Z];
        let mid = [
            (salient[0] + shoulder[0]) * 0.5,
            (salient[1] + shoulder[1]) * 0.5,
        ];
        // In from the edge along the face's inward normal.
        let inward = PARAPET_IN + PARAPET_D * 0.5;
        let n = [
            -s * std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
        ];
        let at = [mid[0] + n[0] * inward, 0.0, mid[1] + n[1] * inward];
        let h = 1.30 + 0.02 * k as f32;
        let parts = parapet(
            len + PARAPET_D,
            DECK,
            h,
            &[-len * 0.25, len * 0.25],
            Some(DECK_SALIENT),
        );
        out.push(stand(parts, at, -s * FRAC_PI_4));
    }
    out
}

/// The flanks and the gorge (rear) parapet, plain, the gorge open for the
/// steps.
fn flanks() -> Vec<Generator> {
    let reach = HALF * (1.0 - BATTER);
    let rear_mid = (REAR[0] + REAR[1]) * 0.5;
    let rear_half = (REAR[1] - REAR[0]) * 0.5 * (1.0 - BATTER);
    let inward = PARAPET_IN + PARAPET_D * 0.5;
    let mut out = Vec::new();
    // From the shoulder, where the face's parapet overlaps it, to the
    // gorge's edge.
    // It stops a little short of the gorge parapet's outer face, so the two
    // do not end in one plane.
    let (z_lo, z_hi) = (DIAMOND_Z - 0.3, rear_mid + rear_half - PARAPET_IN - 0.06);
    for (k, s) in [1.0_f32, -1.0].into_iter().enumerate() {
        let parts = parapet(z_hi - z_lo, DECK, 1.34 + 0.02 * k as f32, &[0.0], None);
        out.push(stand(
            parts,
            [s * (reach - inward), 0.0, (z_lo + z_hi) * 0.5],
            -s * FRAC_PI_2,
        ));
    }
    let gorge_z = rear_mid + rear_half - inward;
    for s in [-1.0_f32, 1.0] {
        let (a, b) = (1.7, reach);
        let parts = parapet(b - a, DECK, 1.0, &[], None);
        out.push(stand(parts, [s * (a + b) * 0.5, 0.0, gorge_z], PI));
    }
    out
}

/// The steps up the gorge to the terreplein, each tread its own block, the
/// top one bedded into the battered rear face.
fn gorge_steps() -> Generator {
    let n = 10;
    let run = 0.3;
    let rise = DECK / (n as f32 + 1.0);
    let face = REAR[1];
    let width = 3.0;
    let mut steps: Vec<Generator> = (0..n)
        .map(|k| {
            let top = rise * (k as f32 + 1.0);
            let z_out = face + run * (n - k) as f32;
            let z_in = if k == n - 1 { face - 0.3 } else { z_out - run };
            prim(
                solid(cuboid_tapered([width, top, z_out - z_in], 0.0, wall())),
                [0.0, top * 0.5, (z_in + z_out) * 0.5],
                id_quat(),
            )
        })
        .collect();
    let first = steps.remove(0);
    nest(first, steps)
}

/// The flagstaff at the back of the terreplein: a stone step, a tall staff
/// with a gilt truck, the black colours and three stays to the deck.
fn flagstaff(x: f32, z: f32) -> Generator {
    let staff_h = 11.0;
    let base_top = DECK + 0.5;
    let top = base_top + staff_h;
    let mut parts = vec![
        prim(
            solid(cylinder_tapered(0.13, staff_h, 10, 0.4, strake(HULL_OAK))),
            [x, base_top + staff_h * 0.5, z],
            id_quat(),
        ),
        prim(
            sphere(0.12, 3, bronze(GOLD_LEAF, 0x9D)),
            [x, top + 0.08, z],
            id_quat(),
        ),
        jolly_roger([x, top - 0.35, z], 2.3),
    ];
    for (dx, dz) in [(-2.6_f32, -1.4_f32), (2.2, -2.0), (0.4, 2.4)] {
        parts.push(strut(
            [x, top - 2.2, z],
            [x + dx, DECK - 0.03, z + dz],
            0.025,
            5,
            hemp(ROPE_HEMP),
        ));
    }
    let mut staff = nest(
        prim(
            solid(cylinder_tapered(0.5, 0.5, 12, 0.15, wall())),
            [x, DECK + 0.25, z],
            id_quat(),
        ),
        parts,
    );
    staff.audio = fx::rigging_creak();
    staff
}

/// The powder house: a squat stone hut under a shingled pyramid roof, its
/// iron-banded door to the sea, a keg either side of it.
fn powder_house(x: f32, z: f32) -> Generator {
    let w = 3.2;
    let h = 2.6;
    let front = z - w * 0.5;
    let mut parts = vec![
        prim(
            solid(cuboid_tapered(
                [w + 0.7, 1.3, w + 0.7],
                0.99,
                shingle(SHINGLE_GREY),
            )),
            [x, DECK + h + 0.65, z],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([1.0, 1.8, 0.1], 0.0, board(OAK_JOINERY))),
            [x, DECK + 0.9, front - 0.04],
            id_quat(),
        ),
    ];
    for y in [DECK + 0.45, DECK + 1.35] {
        parts.push(prim(
            cuboid_tapered([1.04, 0.07, 0.04], 0.0, iron(IRON_BLACK, 0x9E)),
            [x, y, front - 0.1],
            id_quat(),
        ));
    }
    for s in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cylinder_tapered(0.26, 0.7, 12, 0.0, strake(HULL_OAK))),
            [x + s * 1.05, DECK + 0.35, front - 0.45],
            id_quat(),
        ));
    }
    nest(
        prim(
            solid(cuboid_tapered([w, h, w], 0.0, wall())),
            [x, DECK + h * 0.5, z],
            id_quat(),
        ),
        parts,
    )
}

/// The lookout: four splayed oak legs with cross braces, a platform with a
/// rail, a ladder up its sea side and a little shingled roof over a lantern.
fn lookout(x: f32, z: f32) -> Generator {
    let foot = 1.3;
    let head = 1.0;
    let plat = DECK + 7.6;
    let timber = strake(HULL_OAK);
    let mut parts = Vec::new();
    let corners = [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    for (cx, cz) in corners {
        parts.push(strut(
            [x + cx * foot, DECK, z + cz * foot],
            [x + cx * head, plat, z + cz * head],
            0.13,
            8,
            timber.clone(),
        ));
    }
    // A cross brace on each face, at two heights.
    for i in 0..4 {
        let (ax, az) = corners[i];
        let (bx, bz) = corners[(i + 1) % 4];
        let at = |cx: f32, cz: f32, y: f32| {
            let f = (y - DECK) / (plat - DECK);
            let r = foot + (head - foot) * f;
            [x + cx * r, y, z + cz * r]
        };
        let (y0, y1) = (DECK + 0.6, DECK + 4.0);
        parts.push(strut(
            at(ax, az, y0),
            at(bx, bz, y1),
            0.06,
            6,
            timber.clone(),
        ));
    }
    let deck_w = head * 2.0 + 0.8;
    let platform = prim(
        solid(cuboid_tapered([deck_w, 0.18, deck_w], 0.0, board(HULL_OAK))),
        [x, plat + 0.09, z],
        id_quat(),
    );
    let mut on_deck = Vec::new();
    let e = deck_w * 0.5 - 0.06;
    // Each side's run stops short of the corners, so no two runs stand a
    // post in one place.
    let c = e - 0.14;
    let rail = strake(OAK_JOINERY);
    for (a, b) in [
        ([-c, -e], [c, -e]),
        ([e, -c], [e, c]),
        ([c, e], [-c, e]),
        ([-e, c], [-e, -c]),
    ] {
        on_deck.extend(railing(
            [x + a[0], plat + 0.18, z + a[1]],
            [x + b[0], plat + 0.18, z + b[1]],
            0.95,
            0.7,
            rail.clone(),
        ));
    }
    // A roof on four posts over the watch, and the lantern hung under it.
    let post_h = 2.0;
    let roof_y = plat + 0.18 + post_h;
    for (cx, cz) in corners {
        on_deck.push(prim(
            solid(cylinder_tapered(0.07, post_h, 6, 0.0, rail.clone())),
            [
                x + cx * (e - 0.02),
                plat + 0.18 + post_h * 0.5,
                z + cz * (e - 0.02),
            ],
            id_quat(),
        ));
    }
    on_deck.push(prim(
        solid(cuboid_tapered(
            [deck_w + 0.6, 0.9, deck_w + 0.6],
            0.99,
            shingle(SHINGLE_GREY),
        )),
        [x, roof_y + 0.45, z],
        id_quat(),
    ));
    on_deck.push(prim(
        cylinder_tapered(0.015, 0.4, 5, 0.0, iron(IRON_BLACK, 0x9F)),
        [x, roof_y - 0.2, z],
        id_quat(),
    ));
    on_deck.push(lantern([x, roof_y - 0.4 - 0.215, z], 0.4, 0xA0));
    parts.push(nest(platform, on_deck));
    // The ladder up the sea side, from the deck to the platform.
    let lz = z - foot - 0.25;
    for s in [-1.0_f32, 1.0] {
        parts.push(strut(
            [x + s * 0.3, DECK, lz],
            [x + s * 0.3, plat + 0.1, z - head - 0.15],
            0.04,
            6,
            timber.clone(),
        ));
    }
    let steps = 12;
    for k in 1..steps {
        let f = k as f32 / steps as f32;
        let y = DECK + (plat + 0.1 - DECK) * f;
        let rz = lz + (z - head - 0.15 - lz) * f;
        parts.push(prim(
            cylinder_tapered(0.025, 0.64, 5, 0.0, timber.clone()),
            [x, y, rz],
            quat_z(FRAC_PI_2),
        ));
    }
    // The legs stand on sole plates on the deck.
    nest(
        prim(
            solid(cuboid_tapered(
                [foot * 2.0 + 0.5, 0.12, foot * 2.0 + 0.5],
                0.0,
                strake(HULL_TAR),
            )),
            [x, DECK + 0.06, z],
            id_quat(),
        ),
        parts,
    )
}

fn build_tree() -> Generator {
    let mut root = footing(HALF * 2.0 + 0.4, 22.8, [0.0, -0.3], CLEARANCE);
    attach(&mut root, rampart());
    for f in faces() {
        attach(&mut root, f);
    }
    for f in flanks() {
        attach(&mut root, f);
    }
    attach(&mut root, gorge_steps());
    attach(&mut root, flagstaff(0.6, 4.6));
    attach(&mut root, powder_house(5.2, 4.4));
    attach(&mut root, lookout(-5.0, 3.6));
    root.audio = fx::harbour_swell();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_sanitize_stable, has_emissive};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&HeadlandRedoubt.build(""), "headland_redoubt");
    }

    #[test]
    fn nothing_z_fights() {
        assert_no_z_fighting(&HeadlandRedoubt.build(""), "headland_redoubt");
    }

    #[test]
    fn nothing_floats() {
        assert_nothing_floats(&HeadlandRedoubt.build(""), "headland_redoubt");
    }

    #[test]
    fn its_lookout_is_lit() {
        assert!(has_emissive(&HeadlandRedoubt.build("")));
    }

    #[test]
    fn record_stays_inside_the_budget() {
        let bytes = crate::pds::record_size::serialized_record_bytes(&HeadlandRedoubt.build(""))
            .expect("serialises");
        eprintln!("headland_redoubt record: {bytes} B");
        assert!(bytes < 80 * 1024, "{bytes} B");
    }
}
