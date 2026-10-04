//! Parking stack - a Cyberpunk secondary, rebuilt under #1559 as a modern
//! open-deck parking structure: five precast spandrel bands over open decks
//! whose lit interior shows between them, corner piers and columns, a
//! perforated metal screen across most of the front and down the `+X`
//! flank, a concrete circulation core rising above the roof with a lit lift
//! door and one framed "P" sign, and a roof deck with lamp posts and parked
//! cars. About 16 m to the roof deck at scale 1, the core 19.5 m.
//!
//! What it replaced was a frame of four posts with a glowing spiral ramp
//! and neon deck edges. A real garage reads by its bands and the light
//! between them, so that is where its light is: the decks' interior is a
//! dim cool lit volume seen only through the open storeys, and the one
//! bright thing is the sign.
//!
//! Sized for a downtown lot: 16 m by 14 m - a compact garage of one ramped
//! module - its core 0.4 m proud at one corner and the plinth wide enough to
//! carry it. Its sizes: a lot fits it by its half side ([`LOT_HALF`]), a
//! seeded room spaces it by a circle that keeps props off its plinth
//! ([`CLEARANCE`]) and floors it on the ground under its footing
//! ([`GROUND_R`]), and ruin sways its top about a metre at most
//! ([`RUIN_SWAY_M`]). The footing carries the decks and the core, with the
//! door light and the sign on its face, as the ruin pass's anchor; the
//! pieces a fought-over room may lose are the lamps, the cars and the
//! barrier.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, footing, id_quat, nest, prim, solid, with_face,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::ThemeArchetype;

use super::facade::{
    OFFICE_JOINT, RESIDENTIAL, block, facade, framed_sign, light_bar, palette, paving, perforated,
    precast, steel,
};

/// The decks' plan: x within `HALF_W` of the axis, z within `HALF_D`.
const HALF_W: f32 = 8.0;
const HALF_D: f32 = 7.0;
const PLINTH_H: f32 = 0.3;
/// Where the piers, columns and core stand: 1 cm into the plinth, so their
/// feet never share the plane of the decks' interior.
const FOOT: f32 = PLINTH_H - 0.01;
/// Storey height, and the five elevated decks (the fifth is the roof).
const STOREY: f32 = 3.0;
const DECKS: usize = 5;
/// A spandrel band: the deck slab and its upstand barrier, from 0.35 m
/// under the deck to 0.85 m over it.
const BAND_BELOW: f32 = 0.35;
const BAND_ABOVE: f32 = 0.85;
const ROOF_TOP: f32 = PLINTH_H + DECKS as f32 * STOREY + BAND_ABOVE;
/// The circulation core at the front corner on `-X` (the viewer's right,
/// seen from the street), proud of both faces.
const CORE: [f32; 4] = [-8.4, -4.4, -7.4, -3.4]; // x0, x1, z0, z1
const CORE_TOP: f32 = 19.5;
/// The plinth: the decks' plan, out past the core by 5 cm.
const PLINTH_HALF: [f32; 2] = [8.45, 7.45];
/// The lift door at the core's foot.
const DOOR_H: f32 = 2.6;
/// The lamp posts on the roof deck, and how deep they are bedded into it.
const POLE_H: f32 = 5.0;
const ROOF_BED: f32 = 0.2;
/// How far the cars stand into the roof deck below the line of their
/// bodies' feet (m).
const CAR_BED: f32 = 0.12;

/// The circle a seeded room spaces the garage by. The settlement keeps a
/// prop only half the two clearances from a building, so this is about
/// twice the plinth's 11.2 m corner reach: over every Cyberpunk room in
/// seeds 0-6000 no prop stands inside the garage or touches it.
const CLEARANCE: f32 = 20.0;
/// The ground the garage stands on: the footing's corner reach, so its
/// floor is set by the highest ground under it, not under its spacing
/// circle.
const GROUND_R: f32 = 11.4;
/// Half the narrow side of the lot the garage fills at scale 1: the core's
/// reach and a hand's breadth.
const LOT_HALF: f32 = 8.5;
/// How far the ruin pass may lean the whole building, at its tip (m). The
/// pass also knocks the trunk askew on its own, on two axes at once - up to
/// sqrt(2)/4 of that while the trunk carries pieces, sqrt(2) times it once
/// they are gone - and measured over thousands of conflict ruins the top
/// moves about a metre at most.
const RUIN_SWAY_M: f32 = 1.0;
const TIP: f32 = ROOF_TOP + POLE_H;
/// The smallest scale the garage still reads as one at: a car's 2.1 m of
/// headroom under every deck and a 2 m lift door (#1559).
#[cfg(test)]
const MIN_SCALE: f32 = 0.8;

pub struct ParkingStack;

impl CatalogueEntry for ParkingStack {
    fn slug(&self) -> &'static str {
        "parking_stack"
    }
    fn name(&self) -> &'static str {
        "Parking Stack"
    }
    fn description(&self) -> &'static str {
        "Open-deck parking structure: precast bands, a perforated screen, a stair and \
         lift core with one framed sign, cars on the roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CYBER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 30.0,
        }
    }
    fn lot_half_width(&self) -> f32 {
        LOT_HALF
    }
    fn ground_radius(&self) -> Option<f32> {
        Some(GROUND_R)
    }
    fn ruin_max_lean(&self) -> Option<f32> {
        Some(RUIN_SWAY_M / TIP)
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The height of deck `k` (1 = the first elevated deck).
fn deck(k: usize) -> f32 {
    PLINTH_H + k as f32 * STOREY
}

fn build_tree() -> Generator {
    let [px, pz] = PLINTH_HALF;
    let plinth = prim(
        solid(cuboid_tapered(
            [px * 2.0, PLINTH_H, pz * 2.0],
            0.0,
            paving([0.24, 0.24, 0.25]),
        )),
        [0.0, PLINTH_H * 0.5, 0.0],
        id_quat(),
    );
    // The trunk: the footing (4 cm wider than the plinth each way) carrying
    // the decks and the core with its door light and sign.
    let base = footing(px * 2.0 + 0.2, pz * 2.0 + 0.2, [0.0, 0.0], CLEARANCE);
    let mut parts = vec![nest(base, vec![decks(), nest(core(), core_fittings())])];
    parts.extend(pieces());
    nest(plinth, parts)
}

/// The decks: a dim lit interior volume carrying the five spandrel bands,
/// the piers and columns, and the screens.
fn decks() -> Generator {
    let top = deck(DECKS) - BAND_BELOW + 0.05;
    let interior = prim(
        cuboid_tapered(
            [HALF_W * 2.0 - 1.2, top - PLINTH_H, HALF_D * 2.0 - 1.2],
            0.0,
            deck_light(),
        ),
        [0.0, (PLINTH_H + top) * 0.5, 0.0],
        id_quat(),
    );

    let mut parts = Vec::new();
    let band_h = BAND_BELOW + BAND_ABOVE;
    for k in 1..=DECKS {
        let mut band = block(
            [HALF_W * 2.0, band_h, HALF_D * 2.0],
            [0.0, deck(k) + (BAND_ABOVE - BAND_BELOW) * 0.5, 0.0],
            precast(palette::CONCRETE_MID),
        );
        if k == DECKS {
            band.kind = with_face(band.kind, FaceKey::Top, paving([0.17, 0.17, 0.18]));
        }
        parts.push(band);
    }
    // Corner piers 10 cm proud of the bands, stopping 5 cm under the roof
    // deck (the one at the core's corner stands inside the core); columns
    // 5 cm behind the band faces, seen in the open storeys and stopping
    // inside the roof band. Neither top shares the deck's plane.
    let pier_top = ROOF_TOP - 0.05;
    for (sx, sz) in [(1.0_f32, -1.0_f32), (1.0, 1.0), (-1.0, 1.0)] {
        parts.push(block(
            [0.8, pier_top - FOOT, 0.8],
            [
                sx * (HALF_W - 0.3),
                (FOOT + pier_top) * 0.5,
                sz * (HALF_D - 0.3),
            ],
            precast(palette::CONCRETE_MID),
        ));
    }
    let col_top = ROOF_TOP - 0.3;
    for (x, z) in [
        (-1.6_f32, -HALF_D + 0.3),
        (-2.8, HALF_D - 0.3),
        (2.8, HALF_D - 0.3),
    ] {
        parts.push(block(
            [0.5, col_top - FOOT, 0.5],
            [x, (FOOT + col_top) * 0.5, z],
            precast(palette::CONCRETE_MID),
        ));
    }
    // The screen: perforated metal across most of the front and down the
    // +X flank, bedded 2 cm into the bands, from 10 cm above the first
    // band's foot to 10 cm under the roof deck - neither end in a band's
    // plane.
    let screen_y0 = deck(1) - BAND_BELOW + 0.1;
    let screen_h = ROOF_TOP - 0.1 - screen_y0;
    let front_x0 = 0.8;
    let front_x1 = HALF_W - 0.1;
    parts.push(block(
        [front_x1 - front_x0, screen_h, 0.42],
        [
            (front_x0 + front_x1) * 0.5,
            screen_y0 + screen_h * 0.5,
            -HALF_D - 0.19,
        ],
        perforated([0.20, 0.21, 0.22]),
    ));
    let side_z0 = -3.2;
    let side_z1 = HALF_D - 1.2;
    parts.push(block(
        [0.42, screen_h, side_z1 - side_z0],
        [
            HALF_W + 0.19,
            screen_y0 + screen_h * 0.5,
            (side_z0 + side_z1) * 0.5,
        ],
        perforated([0.20, 0.21, 0.22]),
    ));
    nest(interior, parts)
}

/// The decks seen through the open storeys: dark in daylight, as a shaded
/// interior is, with the faint cool wash of its strip lights that carries
/// at dusk. Not `lit_interior`, which takes its glow from the surface's own
/// colour - a dark deck would never glow.
fn deck_light() -> crate::pds::SovereignMaterialSettings {
    crate::pds::SovereignMaterialSettings {
        base_color: crate::pds::Fp3([0.06, 0.065, 0.07]),
        emission_color: crate::pds::Fp3([0.78, 0.84, 0.90]),
        emission_strength: crate::pds::Fp(0.12),
        roughness: crate::pds::Fp(0.9),
        metallic: crate::pds::Fp(0.0),
        ..Default::default()
    }
}

/// The stair and lift core: concrete, its front a strip of stair glazing.
fn core() -> Generator {
    let [x0, x1, z0, z1] = CORE;
    let h = CORE_TOP - FOOT;
    let c = [(x0 + x1) * 0.5, FOOT + h * 0.5, (z0 + z1) * 0.5];
    let mut body = block([x1 - x0, h, z1 - z0], c, precast([0.27, 0.27, 0.27]));
    body.kind = with_face(
        body.kind,
        FaceKey::SideNz,
        facade(
            RESIDENTIAL,
            palette::GLASS_OFFICE,
            palette::FRAME_GRAPHITE,
            OFFICE_JOINT,
            c,
            PLINTH_H,
        ),
    );
    body.kind = with_face(body.kind, FaceKey::Top, paving(palette::ROOF_GREY));
    body
}

/// What hangs on the core's face: the lift door's light and the framed "P"
/// sign high up. They ride the core, in the trunk: the ruin pass knocks each
/// piece askew on its own, and at 17 m the trunk's knock alone moves the
/// core's face 30 cm - a sign knocked the other way hung in the air.
fn core_fittings() -> Vec<Generator> {
    let [x0, x1, z0, _] = CORE;
    let cx = (x0 + x1) * 0.5;
    // The door's light stands 2 cm into the plinth, its face 1 cm inside
    // the plinth's: the core's face is 5 cm inside the plinth's edge, the
    // trunk's knock can carry the light past it, and a foot 16 cm over the
    // footing it then hangs above read as floating where 13 cm does not.
    let mut out = vec![light_bar(
        [1.6, DOOR_H, 0.05],
        [cx, PLINTH_H - 0.02 + DOOR_H * 0.5, z0 - 0.015],
        palette::WARM_LIGHT,
        1.2,
    )];
    let sign_y = CORE_TOP - 2.4;
    let mut sign = framed_sign(z0, [cx, sign_y], [3.0, 3.0], [0.05, 0.22, 0.70], 2.0);
    let face = sign.pop().expect("a sign has a face");
    let frame = sign.pop().expect("a sign has a frame");
    // The "P": four white strokes bedded into the sign's face. Seen from
    // the front (-Z) the world's +X is on the viewer's left, so the stem
    // stands at +X and the bowl reaches toward -X. Each stroke is 8 mm
    // thinner than the one it ends inside, so their faces stand 4 mm apart
    // - 2 mm on the smallest lot, still clear of a depth tie.
    let pz = z0 - 0.17;
    let mut strokes = Vec::new();
    for (dx, dy, w, hgt, t) in [
        (0.42_f32, 0.0_f32, 0.3_f32, 1.84_f32, 0.056_f32),
        (0.05, 0.75, 1.0, 0.3, 0.048),
        (0.05, 0.03, 1.0, 0.3, 0.048),
        (-0.36, 0.39, 0.3, 0.98, 0.040),
    ] {
        strokes.push(light_bar(
            [w, hgt, t],
            [cx + dx, sign_y + dy, pz],
            [0.95, 0.96, 1.0],
            4.0,
        ));
    }
    out.push(nest(frame, vec![nest(face, strokes)]));
    out
}

/// The pieces a fought-over room may lose: two lamp posts and two parked
/// cars on the roof deck, and the barrier at the vehicle entrance. The
/// posts are bedded [`ROOF_BED`] into the roof band and the cars stand in
/// it a little: the trunk's knock lowers the deck under them by up to a
/// dozen centimetres, and a thin post the knock barely tilts would be left
/// standing on nothing.
fn pieces() -> Vec<Generator> {
    let mut out = Vec::new();
    let pole_h = POLE_H + ROOF_BED;
    for z in [-2.6_f32, 3.0] {
        out.push(nest(
            prim(
                solid(cylinder_tapered(
                    0.09,
                    pole_h,
                    8,
                    0.2,
                    steel(palette::FRAME_GRAPHITE),
                )),
                [-1.0, ROOF_TOP + POLE_H - pole_h * 0.5, z],
                id_quat(),
            ),
            vec![light_bar(
                [0.6, 0.14, 0.26],
                [-0.8, ROOF_TOP + POLE_H - 0.05, z],
                palette::COOL_WHITE,
                4.0,
            )],
        ));
    }
    for (x, z, paint) in [
        (3.8_f32, -1.4_f32, [0.07, 0.07, 0.08]),
        (3.8, 3.0, [0.55, 0.56, 0.58]),
    ] {
        out.push(car([x, ROOF_TOP - CAR_BED, z], paint));
    }
    let bz = -HALF_D;
    out.push(nest(
        block(
            [0.3, 1.1, 0.3],
            [0.2, PLINTH_H + 0.55, bz],
            steel([0.75, 0.7, 0.1]),
        ),
        vec![block(
            [3.4, 0.1, 0.1],
            [-1.6, PLINTH_H + 1.0, bz],
            steel([0.85, 0.12, 0.1]),
        )],
    ));
    out
}

/// A parked car standing on `at`, nose along X: a body and a cabin.
fn car(at: [f32; 3], paint: [f32; 3]) -> Generator {
    let [x, y, z] = at;
    nest(
        block([4.4, 0.8, 1.8], [x, y + 0.38, z], steel(paint)),
        vec![prim(
            cuboid_tapered([2.3, 0.62, 1.66], 0.25, steel([0.05, 0.05, 0.06])),
            [x - 0.2, y + 1.08, z],
            id_quat(),
        )],
    )
}

#[cfg(test)]
mod tests {
    use super::super::tests::SPACING;
    use super::*;
    use crate::catalogue::items::util::{
        assert_no_coplanar_faces, assert_no_tilted_parents, assert_sanitize_stable,
    };
    use crate::pds::GeneratorKind;

    fn count(g: &Generator) -> usize {
        1 + g.children.iter().map(count).sum::<usize>()
    }

    /// Every band: its foot and its top in the ground frame.
    fn bands(root: &Generator) -> Vec<(f32, f32)> {
        fn walk(g: &Generator, at: f32, out: &mut Vec<(f32, f32)>) {
            let y = at + g.transform.translation.0[1];
            if let GeneratorKind::Cuboid { size, .. } = &g.kind
                && size.0[0] >= HALF_W * 2.0 - 1e-3
                && size.0[2] >= HALF_D * 2.0 - 1e-3
                && size.0[1] < 2.0
            {
                out.push((y - size.0[1] * 0.5, y + size.0[1] * 0.5));
            }
            g.children.iter().for_each(|c| walk(c, y, out));
        }
        let mut out = Vec::new();
        walk(&root.children[0], root.transform.translation.0[1], &mut out);
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ParkingStack.build(""), "parking_stack");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&ParkingStack.build(""), "parking_stack");
    }

    /// No two faces share a plane and a facing (#1559: the roof deck tied
    /// with six pier and column tops, two screen feet with the first band's
    /// underside, and the "P" strokes with each other).
    #[test]
    fn no_faces_tie_for_depth() {
        assert_no_coplanar_faces(&ParkingStack.build(""), "parking_stack");
    }

    #[test]
    fn has_its_sign() {
        assert!(crate::catalogue::items::util::has_emissive(
            &ParkingStack.build("")
        ));
    }

    #[test]
    fn stays_inside_the_part_budget() {
        let n = count(&ParkingStack.build(""));
        assert!(n <= 60, "{n} parts - a district multiplies every one");
    }

    /// The decks are open: a spandrel band at every deck, open storeys
    /// between them, and the roof deck at a real parking structure's height.
    #[test]
    fn the_decks_are_open_storeys() {
        let bands = bands(&ParkingStack.build(""));
        assert_eq!(bands.len(), DECKS, "a spandrel band at every deck");
        for pair in bands.windows(2) {
            let open = pair[1].0 - pair[0].1;
            assert!(open >= 1.6, "a {open} m open storey");
        }
        let roof = bands.last().map(|b| b.1).unwrap_or_default();
        assert!((15.0..=20.0).contains(&roof), "the roof deck at {roof} m");
    }

    /// Everything that stands on the ground stands on the plinth: the core
    /// floated over falling ground where it reached past it (#1559).
    #[test]
    fn the_plinth_carries_everything_on_the_ground() {
        let root = ParkingStack.build("");
        let GeneratorKind::Cuboid { size: plinth, .. } = &root.kind else {
            panic!("the plinth is a cuboid");
        };
        let half = [plinth.0[0] * 0.5, plinth.0[2] * 0.5];
        for s in crate::catalogue::items::measure::solids(&root) {
            if s.bounds.min.y > 0.5 || s.bounds.min.y < 0.0 {
                continue;
            }
            assert!(
                s.bounds.min.x >= -half[0] - 1e-3
                    && s.bounds.max.x <= half[0] + 1e-3
                    && s.bounds.min.z >= -half[1] - 1e-3
                    && s.bounds.max.z <= half[1] + 1e-3,
                "a {} at {:?} stands past the plinth",
                s.kind_tag,
                s.bounds
            );
        }
    }

    /// Three sizes that agree: the lot fit's half side covers the core, the
    /// settlement's circle is [`SPACING`] times the plinth's corner reach,
    /// and the ruin sways the top by the metre declared.
    #[test]
    fn its_three_sizes_cover_what_they_measure() {
        let root = ParkingStack.build("");
        let GeneratorKind::Cuboid { size, .. } = &root.kind else {
            panic!("the plinth is a cuboid");
        };
        let (hx, hz) = (size.0[0] * 0.5, size.0[2] * 0.5);
        let e = ParkingStack;
        assert!(
            e.lot_half_width() >= hx.max(hz) - 0.05,
            "a {} m half side on a lot",
            hx.max(hz)
        );
        assert!(
            e.footprint().clearance >= SPACING * (hx * hx + hz * hz).sqrt(),
            "a {} m circle",
            e.footprint().clearance
        );
        let lean = e.ruin_max_lean().expect("the garage bounds its lean");
        assert!(lean * TIP <= RUIN_SWAY_M + 1e-3, "ruin sways the top");
    }

    /// The ruin pass takes pieces, never the garage: every band and the core
    /// survive every roll.
    #[test]
    fn ruin_takes_pieces_never_the_garage() {
        for seed in 0..64_u64 {
            let mut g = ParkingStack.build("");
            crate::pds::ruin::apply_ruin_bounded(&mut g, 0.95, seed, ParkingStack.ruin_max_lean());
            assert_eq!(bands(&g).len(), DECKS, "seed {seed}: the ruin took a deck");
            assert_eq!(
                super::super::facade::facades(&g).len(),
                1,
                "seed {seed}: the ruin took the core"
            );
        }
    }

    /// A parking structure is silent from the street (#1559).
    #[test]
    fn is_silent() {
        fn walk(g: &Generator) {
            assert!(matches!(g.audio, crate::pds::SovereignAudioConfig::None));
            g.children.iter().for_each(walk);
        }
        walk(&ParkingStack.build(""));
    }

    /// Down to its smallest real scale the garage keeps a car's headroom
    /// under every deck and a door a person walks through (#1559).
    #[test]
    fn reads_as_a_garage_down_to_its_smallest_scale() {
        let bands = bands(&ParkingStack.build(""));
        for pair in bands.windows(2) {
            // From the deck (the band's foot plus the slab) to the soffit of
            // the next band.
            let headroom = pair[1].0 - (pair[0].0 + BAND_BELOW);
            assert!(
                headroom * MIN_SCALE >= 2.1,
                "{headroom} m of headroom is {} m at {MIN_SCALE}",
                headroom * MIN_SCALE
            );
        }
        // The door's light rides the core, in the trunk.
        fn door_of(g: &Generator) -> Option<f32> {
            match &g.kind {
                GeneratorKind::Cuboid { size, common, .. }
                    if common.material.emission_strength.0 > 1.0
                        && size.0[1] > 2.0
                        && size.0[2] < 0.1 =>
                {
                    Some(size.0[1])
                }
                _ => g.children.iter().find_map(door_of),
            }
        }
        let door = door_of(&ParkingStack.build("")).expect("a lift door");
        assert!(door * MIN_SCALE >= 2.0, "a {door} m door at {MIN_SCALE}");
    }
}
