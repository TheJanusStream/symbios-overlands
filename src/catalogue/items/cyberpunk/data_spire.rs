//! Data spire (shown as "Helix Tower") - a slim Cyberpunk secondary,
//! rebuilt under #1559 as a slender residential tower that keeps its helix
//! in architecture: thirty floors of dark bronze glass round a square core,
//! and at every floor a concrete balcony slab turned six degrees further than
//! the one below, so the slab corners climb the tower as four helices
//! through half a turn. On a two-storey stone podium with a glazed lobby
//! under a lit soffit, topped by a louvred penthouse, a mast and aviation
//! lights. About 105 m to the roof at scale 1.
//!
//! What it replaced was an 18 m metal needle wound with a glowing
//! double-helix wire - the identity as neon. The turn is now built from
//! straight pieces (the glass core never twists, so its panel grid stays
//! square - Isoline's Spire showed what a twisted grid looks like), and the
//! glass carries no emission ([`facade`]). Every slab is wide enough to
//! enclose the core at its turn, so no glass corner ever stands past the
//! floor that carries it.
//!
//! Its sizes, as the megatower has them: a lot fits it by its slabs' reach
//! ([`LOT_HALF`]), a seeded room spaces it by a circle that keeps props off
//! its podium ([`CLEARANCE`]) and floors it on the ground under its footing
//! ([`GROUND_R`]), and ruin sways its top about a metre at most
//! ([`RUIN_SWAY_M`]). The footing carries the whole tower - podium, core,
//! slabs - as the ruin pass's anchor, and the pieces a fought-over room may
//! lose are the roof slab, the penthouse with its mast, and the corner
//! lights.

use std::f32::consts::PI;

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, prim, quat_y, solid, with_face,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::ThemeArchetype;

use super::facade::{
    Lobby, OFFICE_JOINT, RESIDENTIAL, aviation_light, block, facade, fluted_stone, lobby, louvre,
    palette, paving, precast, steel,
};

/// The plinth (root) and the podium on it.
const PLINTH_W: f32 = 12.0;
const PLINTH_H: f32 = 0.5;
const PODIUM_W: f32 = 11.6;
const PODIUM_TOP: f32 = 8.5;
/// The podium's front behind the lobby, the lobby glazing and its head.
const BODY_FRONT: f32 = -2.6;
const GLAZE_Z: f32 = -4.85;
const LOBBY_TOP: f32 = 5.3;

/// The glass core: whole panels wide, one floor datum.
const CORE_W: f32 = 9.6;
const FLOORS: usize = 30;
const ROOF: f32 = PODIUM_TOP + FLOORS as f32 * RESIDENTIAL.floor;
/// The balcony slabs: square, wide enough to cover the core's corners at
/// any turn (the core's half diagonal is 6.79 m), thick enough to read from
/// the street, each turned `TURN_DEG` further than the one below it.
const SLAB_W: f32 = 13.7;
const SLAB_T: f32 = 0.45;
const TURN_DEG: f32 = 6.0;
/// The penthouse and mast on the roof, and the tip of the mast's light.
const PENT_H: f32 = 3.6;
const MAST_H: f32 = 10.0;
const ROOF_SLAB_T: f32 = 0.6;
/// How deep the roof's pieces are bedded into the roof slab (m).
const ROOF_BED: f32 = 0.15;
const TIP: f32 = ROOF + ROOF_SLAB_T - 0.05 + PENT_H + MAST_H + 0.5;

/// The circle a seeded room spaces the tower by. The settlement keeps a
/// prop only half the two clearances from a building, so this is about
/// twice the plinth's 8.5 m corner reach: over every Cyberpunk room in seeds
/// 0-6000 no prop stands inside the tower or touches it.
const CLEARANCE: f32 = 17.0;
/// The ground the tower stands on: the footing's corner reach, so its floor
/// is set by the highest ground under it, not under its spacing circle.
const GROUND_R: f32 = 8.6;
/// Half the narrow side of the lot the tower fills at scale 1: the slab
/// corners' reach and a hand's breadth.
const LOT_HALF: f32 = 9.8;
/// How far the ruin pass may lean the whole building, at its tip (m). The
/// pass also knocks the trunk askew on its own, on two axes at once - up to
/// sqrt(2)/4 of that while the trunk carries pieces, sqrt(2) times it once
/// they are gone - and measured over thousands of conflict ruins the top
/// moves about a metre at most.
const RUIN_SWAY_M: f32 = 1.0;
/// The smallest scale the tower still reads as a building at: its 3.2 m
/// floors stay 2.65 m floor to floor (#1559).
#[cfg(test)]
const MIN_SCALE: f32 = 0.83;

pub struct DataSpire;

impl CatalogueEntry for DataSpire {
    fn slug(&self) -> &'static str {
        "data_spire"
    }
    fn name(&self) -> &'static str {
        "Helix Tower"
    }
    fn description(&self) -> &'static str {
        "Slender residential tower whose balconies turn a few degrees at every floor \
         round a dark glass core - a helix in concrete."
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

fn build_tree() -> Generator {
    let plinth = prim(
        solid(cuboid_tapered(
            [PLINTH_W, PLINTH_H, PLINTH_W],
            0.0,
            paving(palette::STONE_DARK),
        )),
        [0.0, PLINTH_H * 0.5, 0.0],
        id_quat(),
    );
    // The trunk: the footing (4 cm wider than the plinth each way) carrying
    // the podium, which carries the core and its balconies.
    let base = footing(PLINTH_W + 0.2, PLINTH_W + 0.2, [0.0, 0.0], CLEARANCE);
    let mut parts = vec![nest(base, vec![podium(core())])];
    parts.extend(roof());
    nest(plinth, parts)
}

/// The tower above the podium: the glass core carrying its balconies. Its
/// top is a paved roof, so a ruin that takes the roof slab leaves one.
fn core() -> Generator {
    let h = ROOF - PODIUM_TOP;
    let c = [0.0, PODIUM_TOP + h * 0.5, 0.0];
    let mut glass = block(
        [CORE_W, h, CORE_W],
        c,
        facade(
            RESIDENTIAL,
            palette::GLASS_BRONZE,
            palette::FRAME_GRAPHITE,
            OFFICE_JOINT,
            c,
            PODIUM_TOP,
        ),
    );
    glass.kind = with_face(glass.kind, FaceKey::Top, paving(palette::ROOF_GREY));
    let mut parts = Vec::new();
    for k in 1..FLOORS {
        let top = PODIUM_TOP + k as f32 * RESIDENTIAL.floor;
        parts.push(prim(
            solid(cuboid_tapered(
                [SLAB_W, SLAB_T, SLAB_W],
                0.0,
                precast([0.44, 0.44, 0.43]),
            )),
            [0.0, top - SLAB_T * 0.5, 0.0],
            quat_y(turn(k)),
        ));
    }
    nest(glass, parts)
}

/// The turn of the balcony slab on floor `k`, in radians.
fn turn(k: usize) -> f32 {
    (k as f32 * TURN_DEG).to_radians() % PI
}

/// The pieces on the roof: the roof slab (square to the core, as the turn
/// has come round half a turn), the louvred penthouse carrying the mast and
/// its light, and two corner lights.
fn roof() -> Vec<Generator> {
    let mut slab = block(
        [SLAB_W, ROOF_SLAB_T, SLAB_W],
        [0.0, ROOF + ROOF_SLAB_T * 0.5 - 0.05, 0.0],
        precast([0.44, 0.44, 0.43]),
    );
    slab.kind = with_face(slab.kind, FaceKey::Top, paving(palette::ROOF_GREY));
    let top = ROOF + ROOF_SLAB_T - 0.05;

    // The penthouse and the corner lights are bedded ROOF_BED into the
    // slab: the ruin pass knocks the slab and each of them askew on their
    // own, and a unit standing flush on it can be left over it.
    let pent_h = PENT_H + ROOF_BED;
    let mut penthouse = block(
        [7.6, pent_h, 7.6],
        [0.0, top + PENT_H - pent_h * 0.5, 0.6],
        louvre(palette::LOUVRE_DARK),
    );
    penthouse.kind = with_face(penthouse.kind, FaceKey::Top, paving(palette::ROOF_GREY));
    let mast_base = top + PENT_H;
    let mast = nest(
        prim(
            solid(cylinder_tapered(
                0.25,
                MAST_H,
                8,
                0.5,
                steel(palette::FRAME_GRAPHITE),
            )),
            [0.0, mast_base + MAST_H * 0.5, 0.6],
            id_quat(),
        ),
        vec![aviation_light([0.0, mast_base + MAST_H + 0.2, 0.6], 0.3)],
    );
    let k = SLAB_W * 0.5 - 0.3;
    vec![
        slab,
        nest(penthouse, vec![mast]),
        aviation_light([-k, top + 0.25 - ROOF_BED, -k], 0.25),
        aviation_light([k, top + 0.25 - ROOF_BED, k], 0.25),
    ]
}

/// The podium: stone, its lobby set back between piers under the upper
/// floor, whose soffit is lit warm. It carries the core.
fn podium(core: Generator) -> Generator {
    let half = PODIUM_W * 0.5;
    let body_d = half - BODY_FRONT;
    let body = block(
        [PODIUM_W, PODIUM_TOP - PLINTH_H, body_d],
        [
            0.0,
            (PLINTH_H + PODIUM_TOP) * 0.5,
            BODY_FRONT + body_d * 0.5,
        ],
        fluted_stone([0.15, 0.145, 0.14]),
    );
    let band_h = PODIUM_TOP - LOBBY_TOP;
    let band_d = BODY_FRONT + half;
    let mut band = block(
        [PODIUM_W, band_h, band_d],
        [0.0, LOBBY_TOP + band_h * 0.5, -half + band_d * 0.5],
        fluted_stone([0.15, 0.145, 0.14]),
    );
    band.kind = with_face(band.kind, FaceKey::Bottom, glow(palette::WARM_LIGHT, 0.8));

    let mut parts = vec![band];
    parts.extend(lobby(
        &Lobby {
            half_w: half,
            floor: PLINTH_H,
            head: LOBBY_TOP,
            glaze_z: GLAZE_Z,
            back_z: BODY_FRONT,
            pier_w: 0.9,
            panes: (5, 2),
            room: [0.64, 0.54, 0.42],
            lit: 0.5,
        },
        fluted_stone([0.15, 0.145, 0.14]),
    ));
    parts.push(core);
    nest(body, parts)
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

    /// The core node: the trunk (footing) carries the podium, the podium
    /// carries the core last.
    fn core_of(root: &Generator) -> &Generator {
        root.children[0].children[0]
            .children
            .last()
            .expect("the podium carries the core")
    }

    fn slabs(core: &Generator) -> Vec<&Generator> {
        core.children
            .iter()
            .filter(|g| {
                matches!(&g.kind, GeneratorKind::Cuboid { size, .. } if (size.0[1] - SLAB_T).abs() < 1e-4)
            })
            .collect()
    }

    fn yaw(g: &Generator) -> f32 {
        2.0 * g.transform.rotation.0[1].atan2(g.transform.rotation.0[3])
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&DataSpire.build(""), "data_spire");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&DataSpire.build(""), "data_spire");
    }

    #[test]
    fn no_faces_tie_for_depth() {
        assert_no_coplanar_faces(&DataSpire.build(""), "data_spire");
    }

    #[test]
    fn has_its_lights() {
        assert!(crate::catalogue::items::util::has_emissive(
            &DataSpire.build("")
        ));
    }

    #[test]
    fn stays_inside_the_part_budget() {
        let n = count(&DataSpire.build(""));
        assert!(n <= 60, "{n} parts - a district multiplies every one");
    }

    /// The helix is the slabs: one on every floor, each turned the same step
    /// further than the one below, and every one wide enough to enclose the
    /// core's corners at its turn - a glass corner never stands past the
    /// floor that carries it (#1559: it did on 20 of 29).
    #[test]
    fn the_balconies_turn_floor_by_floor_round_the_core() {
        let root = DataSpire.build("");
        let core = core_of(&root);
        let GeneratorKind::Cuboid { size: c, .. } = &core.kind else {
            panic!("the core is a cuboid");
        };
        let slabs = slabs(core);
        assert_eq!(slabs.len(), FLOORS - 1, "a balcony on every floor");
        for pair in slabs.windows(2) {
            let step = (yaw(pair[1]) - yaw(pair[0])).rem_euclid(PI).to_degrees();
            assert!(
                (step - TURN_DEG).abs() < 1e-2,
                "a slab turns {step} degrees past the one below"
            );
            let rise = pair[1].transform.translation.0[1] - pair[0].transform.translation.0[1];
            assert!(
                (rise - RESIDENTIAL.floor).abs() < 1e-4,
                "slabs {rise} m apart"
            );
        }
        for slab in &slabs {
            let GeneratorKind::Cuboid { size: s, .. } = &slab.kind else {
                unreachable!()
            };
            let t = yaw(slab);
            let need = c.0[0] * 0.5 * (t.cos().abs() + t.sin().abs());
            assert!(
                s.0[0] * 0.5 >= need,
                "a slab turned {} degrees covers {} m of the core's {need} m reach",
                t.to_degrees(),
                s.0[0] * 0.5
            );
        }
    }

    /// The transoms sit on the floor lines and a mullion runs down every
    /// corner of the core (#1559).
    #[test]
    fn the_core_meets_the_floors_and_the_corners() {
        let root = DataSpire.build("");
        let floors = super::super::facade::assert_floor_lines(&root, "data_spire", PODIUM_TOP);
        let corners = super::super::facade::assert_corner_mullions(&root, "data_spire");
        assert!(floors >= 1 && corners >= 1, "no glass core found");
        for (c, _, m, _) in super::super::facade::facades(&root) {
            assert_eq!(m.emission_strength.0, 0.0, "lit glass at {c:?}");
        }
    }

    /// Three sizes that agree: the lot fit's half side covers the slabs'
    /// reach, the settlement's circle is [`SPACING`] times the podium's
    /// corner reach and covers the slabs too, and the ruin sways the tip by
    /// the metre declared.
    #[test]
    fn its_three_sizes_cover_what_they_measure() {
        let root = DataSpire.build("");
        let GeneratorKind::Cuboid { size: plinth, .. } = &root.kind else {
            panic!("the plinth is a cuboid");
        };
        let slab_w = slabs(core_of(&root))
            .iter()
            .map(|g| match &g.kind {
                GeneratorKind::Cuboid { size, .. } => size.0[0],
                _ => 0.0,
            })
            .fold(0.0, f32::max);
        let reach = slab_w * 0.5 * std::f32::consts::SQRT_2;
        let ground = plinth.0[0] * 0.5 * std::f32::consts::SQRT_2;
        let e = DataSpire;
        assert!(e.lot_half_width() >= reach, "slabs reach {reach} m");
        assert!(
            e.footprint().clearance >= (SPACING * ground).max(reach),
            "a {} m circle round {ground} m plinth corners",
            e.footprint().clearance
        );
        let lean = e.ruin_max_lean().expect("a tall tower bounds its lean");
        assert!(lean * TIP <= RUIN_SWAY_M + 1e-3, "ruin sways the tip");
    }

    /// The ruin pass can take the roof's pieces but never the tower: the
    /// trunk carrying the core and every slab is the root's lowest child.
    #[test]
    fn ruin_takes_pieces_never_the_tower() {
        let slabs_in = |g: &Generator| -> usize {
            fn walk(g: &Generator) -> usize {
                let own = usize::from(matches!(
                    &g.kind,
                    GeneratorKind::Cuboid { size, .. } if (size.0[1] - SLAB_T).abs() < 1e-4
                ));
                own + g.children.iter().map(walk).sum::<usize>()
            }
            walk(g)
        };
        for seed in 0..64_u64 {
            let mut g = DataSpire.build("");
            crate::pds::ruin::apply_ruin_bounded(&mut g, 0.95, seed, DataSpire.ruin_max_lean());
            assert_eq!(
                slabs_in(&g),
                FLOORS - 1,
                "seed {seed}: the ruin took a balcony"
            );
            assert_eq!(
                super::super::facade::facades(&g).len(),
                1,
                "seed {seed}: the ruin took the core"
            );
        }
    }

    /// A residential tower is silent from the street (#1559).
    #[test]
    fn is_silent() {
        fn walk(g: &Generator) {
            assert!(matches!(g.audio, crate::pds::SovereignAudioConfig::None));
            g.children.iter().for_each(walk);
        }
        walk(&DataSpire.build(""));
    }

    /// Down to its smallest real scale the tower keeps real proportions:
    /// floors at least 2.65 m floor to floor and a lobby a person walks into.
    #[test]
    fn reads_as_a_building_down_to_its_smallest_scale() {
        let root = DataSpire.build("");
        for (c, _, m, _) in super::super::facade::facades(&root) {
            let floor = super::super::facade::floor_of(&m);
            assert!(
                floor * MIN_SCALE >= 2.65,
                "the {floor} m floors at {c:?} are {} m at {MIN_SCALE}",
                floor * MIN_SCALE
            );
        }
        for card in crate::catalogue::items::util::window_cards(&root) {
            assert!(
                card.size[1] * MIN_SCALE >= 2.4,
                "a {} m lobby at {MIN_SCALE}",
                card.size[1]
            );
        }
    }
}
