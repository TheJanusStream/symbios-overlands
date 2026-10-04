//! Neon megatower (shown as "Supertall Tower") - the Cyberpunk landmark,
//! rebuilt as a supertall office tower under #1559: a dark glass curtain
//! wall stepping back twice over louvred plant floors, notched at every
//! corner, standing on a fluted stone podium whose glazed lobby sits deep
//! under a cantilevered canopy, and crowned by a louvred plant screen with
//! one thin lit line, a mast and red aviation lights. About 142 m to the
//! roof at scale 1.
//!
//! What it replaced was four stacked dark boxes ringed in festive neon bands
//! with a holo halo round an observation drum - the "too stylized" the owner
//! saw from Isoline's streets. The curtain wall is one stack-bond texture
//! per tier ([`facade`]): glass, mullions and a dark spandrel at every floor,
//! with no emission on the glass at all. The light is small and purposeful -
//! the lobby through its glazing, the canopy soffit, a name line on the
//! canopy edge, the crown line and the aviation reds.
//!
//! # Three sizes
//!
//! - **On a lot** it is fitted by its real half side, [`LOT_HALF`]: Isoline's
//!   cleared lots are 15-20 m on their narrow side, so the 18.4 m plinth is
//!   drawn near its authored size.
//! - **In a seeded room** it is spaced by [`CLEARANCE`], a circle well
//!   outside its corners: the settlement keeps a prop only half the two
//!   clearances from a building, so a tight circle let props stand inside
//!   the podium. It stands on [`GROUND_R`], its footing's reach: floored on
//!   the highest ground within the spacing circle instead, it would stand
//!   metres over its downhill side on a slope.
//! - **Under ruin** its top sways about a metre at most ([`RUIN_SWAY_M`]):
//!   the escalation tier's lean is an angle, and 0.22 rad of it swings a
//!   160 m mast 35 m.
//!
//! # A trunk and its pieces
//!
//! The ruin pass rolls each of the root's children whole: it fells a part
//! as one piece of debris or deletes it, and never the part with the lowest
//! base. So the footing carries everything that is the building - podium,
//! lobby, tiers, plant floors, crown screen - and is that lowest part, and
//! the plinth's other children are the pieces a fought-over room loses: the
//! canopy, the plant intake, the coping, the mast, the aviation lights.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, foundation_block, glow, id_quat, nest, prim, solid, with_face,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::ThemeArchetype;

use super::facade::{
    Lobby, OFFICE, OFFICE_JOINT, aviation_light, block, facade, fluted_stone, light_bar, lobby,
    louvre, palette, paving, precast, steel,
};
use super::fx;

/// The circle a seeded room spaces the tower by. The settlement keeps a
/// prop only half the two clearances from a building, so this is about
/// twice the plinth's 13 m corner reach: over every Cyberpunk room in seeds
/// 0-6000 no prop stands inside the tower or touches it.
const CLEARANCE: f32 = 23.0;
/// The ground the tower stands on: the footing's corner reach, so its floor
/// is set by the highest ground under it, not under its spacing circle.
const GROUND_R: f32 = 13.1;
/// Half the narrow side of the lot the tower fills at scale 1: the plinth's
/// half side and a hand's breadth.
const LOT_HALF: f32 = 9.3;
/// How far the ruin pass may lean the whole building, at its tip (m). The
/// pass also knocks the trunk askew on its own, on two axes at once - up to
/// sqrt(2)/4 of that while the trunk carries pieces, sqrt(2) times it once
/// they are gone - and measured over thousands of conflict ruins the top
/// moves about a metre at most.
const RUIN_SWAY_M: f32 = 1.0;
/// The smallest scale the tower still reads as a building at: its 4 m
/// office floors stay 2.65 m floor to floor - a lot fitting it smaller
/// draws a doll's house (#1559).
#[cfg(test)]
const MIN_SCALE: f32 = 0.67;

/// How deep the footing reaches below the plinth (m). Deeper than the
/// catalogue's rule asks of a footprint this wide
/// ([`required_depth`](crate::catalogue::items::foundation::required_depth)
/// stops at 6 m): the landmark is drawn up to 1.45x on whatever ground its
/// room sites it on, and over the Cyberpunk rooms in seeds 0-3000 a 6 m
/// footing left daylight under two of its 81 placements. On flat ground it
/// is out of sight; on a slope it is the tower's podium wall.
const FOOTING_DEPTH: f32 = 10.0;

/// The plinth (root) and the podium on it.
const PLINTH_W: f32 = 18.4;
const PLINTH_H: f32 = 0.6;
const BASE_W: f32 = 18.0;
/// The podium's front plane behind the lobby, and the canopy edge in front.
const BODY_FRONT: f32 = -2.5;
const CANOPY_FRONT: f32 = -BASE_W * 0.5;
/// The lobby's glazing plane, set into the reveal between the piers.
const GLAZE_Z: f32 = -5.0;
const LOBBY_TOP: f32 = 8.1;
const PODIUM_TOP: f32 = 13.6;

/// Every floor edge is `PODIUM_TOP + k * OFFICE.floor`.
const DATUM: f32 = PODIUM_TOP;

/// The tiers: (arm width, notch, top). Each tier is two crossed slabs, so
/// every corner is notched by `notch`; the plant floors sit between them.
const TIER1: (f32, f32, f32) = (16.0, 1.6, 61.6);
const BAND1: (f32, f32) = (13.2, 65.6);
const TIER2: (f32, f32, f32) = (12.8, 1.6, 105.6);
const BAND2: (f32, f32) = (10.0, 109.6);
const TIER3_W: f32 = 9.6;
const TIER3_TOP: f32 = 133.6;
const CROWN_W: f32 = 9.8;
const CROWN_TOP: f32 = 141.6;
const COPING_W: f32 = 10.2;
const ROOF: f32 = CROWN_TOP + 0.48;
/// The mast on the lift overrun, and the tip of its light.
const OVERRUN_H: f32 = 3.0;
const MAST_H: f32 = 14.0;
const TIP: f32 = ROOF + OVERRUN_H + MAST_H + 0.7;
/// How deep the roof's pieces are bedded into the coping (m).
const ROOF_BED: f32 = 0.15;

pub struct NeonMegatower;

impl CatalogueEntry for NeonMegatower {
    fn slug(&self) -> &'static str {
        "neon_megatower"
    }
    fn name(&self) -> &'static str {
        "Supertall Tower"
    }
    fn description(&self) -> &'static str {
        "Supertall office tower: a dark curtain wall stepping back over louvred plant \
         floors to a crown line, a mast and aviation lights."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
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
            min_spawn_dist: 70.0,
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

/// Dark office glass in graphite frames, on the tower's floor datum.
fn glass(center: [f32; 3]) -> crate::pds::SovereignMaterialSettings {
    facade(
        OFFICE,
        palette::GLASS_OFFICE,
        palette::FRAME_GRAPHITE,
        OFFICE_JOINT,
        center,
        DATUM,
    )
}

/// A glazed slab whose top is a setback terrace (or hidden under the next
/// tier, where the override costs nothing).
fn glazed(size: [f32; 3], center: [f32; 3]) -> Generator {
    let mut g = block(size, center, glass(center));
    g.kind = with_face(g.kind, FaceKey::Top, paving(palette::ROOF_GREY));
    g
}

/// A louvred plant floor; its top is a terrace round the tier above.
fn plant_floor(width: f32, top: f32) -> Generator {
    let h = OFFICE.floor;
    let mut g = block(
        [width, h, width],
        [0.0, top - h * 0.5, 0.0],
        louvre(palette::LOUVRE_DARK),
    );
    g.kind = with_face(g.kind, FaceKey::Top, paving(palette::ROOF_GREY));
    g
}

/// One notched tier: two crossed glass slabs from `bottom` to `top`, the
/// first carrying the second and `above`. The second runs 2 cm further down
/// and up, into the floors below and above it, so the two never share the
/// plane of a top or a bottom face.
fn tier((arm, notch, top): (f32, f32, f32), bottom: f32, above: Vec<Generator>) -> Generator {
    let h = top - bottom;
    let c = [0.0, bottom + h * 0.5, 0.0];
    let mut parts = vec![glazed([arm - notch * 2.0, h + 0.04, arm], c)];
    parts.extend(above);
    nest(glazed([arm, h, arm - notch * 2.0], c), parts)
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
    let mut parts = vec![trunk()];
    parts.extend(pieces());
    nest(plinth, parts)
}

/// The trunk: the footing - 4 cm wider than the plinth each way, so the
/// plinth's underside rests on it wherever the ground falls away, and
/// [`FOOTING_DEPTH`] deep - carrying the podium and its lobby, which carry
/// the tiers, plant floors and crown screen.
fn trunk() -> Generator {
    let base = foundation_block(PLINTH_W + 0.2, PLINTH_W + 0.2, [0.0, 0.0], FOOTING_DEPTH);
    nest(base, vec![podium(tower())])
}

/// The pieces a fought-over room may lose: the canopy with its name line,
/// the plant intake, the coping, the overrun with its mast, and the four
/// corner lights.
fn pieces() -> Vec<Generator> {
    let mut out = vec![canopy(), intake()];
    out.push(block(
        [COPING_W, 0.5, COPING_W],
        [0.0, ROOF - 0.25, 0.0],
        precast(palette::STONE_DARK),
    ));
    let mast_base = ROOF + OVERRUN_H;
    let mast = nest(
        prim(
            solid(cylinder_tapered(
                0.35,
                MAST_H,
                10,
                0.6,
                steel(palette::FRAME_GRAPHITE),
            )),
            [0.0, mast_base + MAST_H * 0.5, 0.8],
            id_quat(),
        ),
        vec![aviation_light([0.0, mast_base + MAST_H + 0.3, 0.8], 0.4)],
    );
    // The overrun and the corner lights are bedded ROOF_BED into the
    // coping: the ruin pass knocks the coping and each of them askew on
    // their own, and a unit standing flush on it was sometimes left a few
    // centimetres over it.
    let overrun_h = OVERRUN_H + ROOF_BED;
    out.push(nest(
        block(
            [3.6, overrun_h, 3.6],
            [0.0, ROOF + OVERRUN_H - overrun_h * 0.5, 0.8],
            louvre(palette::LOUVRE_DARK),
        ),
        vec![mast],
    ));
    let k = TIER3_W * 0.5;
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        out.push(aviation_light([sx * k, ROOF + 0.3 - ROOF_BED, sz * k], 0.3));
    }
    out
}

/// The stack above the podium, innermost first, up to the crown screen.
fn tower() -> Generator {
    let crown_h = CROWN_TOP - TIER3_TOP;
    let crown = nest(
        block(
            [CROWN_W, crown_h, CROWN_W],
            [0.0, TIER3_TOP + crown_h * 0.5, 0.0],
            louvre(palette::LOUVRE_DARK),
        ),
        crown_line(),
    );

    // Tier 3: one plain glass shaft under the crown.
    let t3_bottom = BAND2.1;
    let t3_h = TIER3_TOP - t3_bottom;
    let t3c = [0.0, t3_bottom + t3_h * 0.5, 0.0];
    let tier3 = nest(
        block([TIER3_W, t3_h, TIER3_W], t3c, glass(t3c)),
        vec![crown],
    );

    // Tier 2 over its plant floor, tier 1 over the podium.
    let band2 = nest(plant_floor(BAND2.0, BAND2.1), vec![tier3]);
    let tier2 = tier(TIER2, BAND1.1, vec![band2]);
    let band1 = nest(plant_floor(BAND1.0, BAND1.1), vec![tier2]);
    tier(TIER1, PODIUM_TOP, vec![band1])
}

/// The crown's one restrained light: a thin cool line round the top of the
/// plant screen, four bars just under the coping - a line, never a lit lid.
/// The bars on the X faces are a hair lower and slimmer than those on the
/// Z faces and each pair ends inside the other, so no two faces at a corner
/// share a plane.
fn crown_line() -> Vec<Generator> {
    let y = CROWN_TOP - 0.12;
    let r = CROWN_W * 0.5 + 0.05;
    let len = CROWN_W + 0.1;
    let mut out = Vec::new();
    for s in [-1.0_f32, 1.0] {
        out.push(light_bar(
            [len, 0.2, 0.12],
            [0.0, y, s * r],
            palette::COOL_WHITE,
            4.0,
        ));
        out.push(light_bar(
            [0.12, 0.18, len],
            [s * r, y - 0.005, 0.0],
            palette::COOL_WHITE,
            4.0,
        ));
    }
    out
}

/// The podium: a fluted stone base and its lobby set back between two
/// stone piers. It carries `tower`.
fn podium(tower: Generator) -> Generator {
    let body_d = BASE_W * 0.5 - BODY_FRONT;
    let body = block(
        [BASE_W, PODIUM_TOP - PLINTH_H, body_d],
        [
            0.0,
            (PLINTH_H + PODIUM_TOP) * 0.5,
            BODY_FRONT + body_d * 0.5,
        ],
        fluted_stone(palette::STONE_DARK),
    );
    let mut parts = lobby(
        &Lobby {
            half_w: BASE_W * 0.5,
            floor: PLINTH_H,
            head: LOBBY_TOP,
            glaze_z: GLAZE_Z,
            back_z: BODY_FRONT,
            pier_w: 1.1,
            panes: (7, 2),
            room: [0.62, 0.52, 0.40],
            lit: 0.5,
        },
        fluted_stone(palette::STONE_DARK),
    );
    parts.push(tower);
    nest(body, parts)
}

/// The canopy: the upper podium carried 6.5 m out over the lobby, its
/// soffit lit warm - a deep, shadowed, lit recess at the foot of a dark
/// tower - and the tower's name in one thin warm line on its edge.
fn canopy() -> Generator {
    let canopy_h = PODIUM_TOP - LOBBY_TOP;
    let canopy_d = BODY_FRONT - CANOPY_FRONT;
    let mut canopy = block(
        [BASE_W, canopy_h, canopy_d],
        [
            0.0,
            LOBBY_TOP + canopy_h * 0.5,
            CANOPY_FRONT + canopy_d * 0.5,
        ],
        fluted_stone(palette::STONE_DARK),
    );
    canopy.kind = with_face(canopy.kind, FaceKey::Bottom, glow(palette::WARM_LIGHT, 0.9));
    let name = light_bar(
        [6.0, 0.3, 0.08],
        [0.0, LOBBY_TOP + canopy_h * 0.45, CANOPY_FRONT - 0.03],
        [1.0, 0.92, 0.80],
        2.5,
    );
    nest(canopy, vec![name])
}

/// A louvred plant intake on the street flank, where the tower's substation
/// hum is heard from the pavement - the one sound it keeps.
fn intake() -> Generator {
    let mut intake = prim(
        cuboid_tapered([0.12, 3.0, 5.0], 0.0, louvre(palette::LOUVRE_DARK)),
        [-BASE_W * 0.5 - 0.04, PLINTH_H + 1.49, 3.0],
        id_quat(),
    );
    intake.audio = fx::transformer_hum();
    intake
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

    fn top_of(g: &Generator, at: f32) -> f32 {
        let y = at + g.transform.translation.0[1];
        let own = match &g.kind {
            GeneratorKind::Cuboid { size, .. } => y + size.0[1] * 0.5,
            _ => y,
        };
        g.children.iter().fold(own, |m, c| m.max(top_of(c, y)))
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&NeonMegatower.build(""), "neon_megatower");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&NeonMegatower.build(""), "neon_megatower");
    }

    /// No two faces share a plane and a facing - the depth tie that flickers
    /// (#1559: the crown line's corners did).
    #[test]
    fn no_faces_tie_for_depth() {
        assert_no_coplanar_faces(&NeonMegatower.build(""), "neon_megatower");
    }

    #[test]
    fn has_its_lights() {
        assert!(
            crate::catalogue::items::util::has_emissive(&NeonMegatower.build("")),
            "the megatower lost its crown line and aviation lights"
        );
    }

    /// A city multiplies every part (#1559): Isoline grew ten of these.
    #[test]
    fn stays_inside_the_part_budget() {
        let n = count(&NeonMegatower.build(""));
        assert!(n <= 60, "{n} parts - a district multiplies every one");
    }

    /// The transoms sit on the floor lines and a mullion runs down every
    /// corner, read from each glass prim's own material (#1559).
    #[test]
    fn the_curtain_wall_meets_the_floors_and_the_corners() {
        let root = NeonMegatower.build("");
        let floors = super::super::facade::assert_floor_lines(&root, "neon_megatower", DATUM);
        let corners = super::super::facade::assert_corner_mullions(&root, "neon_megatower");
        assert!(
            floors >= 5 && corners >= 5,
            "only {floors} / {corners} glass prims found"
        );
    }

    /// No emission on glass: it adds flat colour over panes and frames alike.
    #[test]
    fn the_glass_is_not_lit() {
        for (c, _, m, _) in super::super::facade::facades(&NeonMegatower.build("")) {
            assert_eq!(m.emission_strength.0, 0.0, "lit glass at {c:?}");
        }
    }

    /// A supertall at its authored size (#1559: 120-200 m to the roof), and
    /// three sizes that agree: the lot fit's half side covers the plinth, the
    /// settlement's circle is [`SPACING`] times its corner reach, and the
    /// ruin's lean sways the tip by the metre declared.
    #[test]
    fn is_a_supertall_with_its_three_sizes() {
        let root = NeonMegatower.build("");
        let tip = top_of(&root, 0.0);
        assert!(
            (120.0..=200.0).contains(&(tip - OVERRUN_H - MAST_H)),
            "the roof is at {} m",
            tip - OVERRUN_H - MAST_H
        );
        let GeneratorKind::Cuboid { size, .. } = &root.kind else {
            panic!("the plinth is a cuboid");
        };
        let half = size.0[0].max(size.0[2]) * 0.5;
        let corner = half * std::f32::consts::SQRT_2;
        let e = NeonMegatower;
        assert!(e.lot_half_width() >= half, "a {half} m half side on a lot");
        assert!(
            e.footprint().clearance >= SPACING * corner,
            "a {} m circle round {corner} m corners",
            e.footprint().clearance
        );
        let lean = e.ruin_max_lean().expect("a tall tower bounds its lean");
        assert!(
            lean * tip <= RUIN_SWAY_M + 1e-3,
            "ruin sways the tip {} m",
            lean * tip
        );
    }

    /// The ruin pass can take the tower's pieces but never the tower: the
    /// trunk carrying every glass tier is the root's lowest child (the
    /// pass's anchor), and no other child reaches 20 m tall.
    #[test]
    fn ruin_takes_pieces_never_the_tower() {
        let root = NeonMegatower.build("");
        let base = |g: &Generator| -> f32 {
            crate::catalogue::items::measure::solids(&nest(
                prim(
                    cuboid_tapered([0.01; 3], 0.0, Default::default()),
                    [0.0; 3],
                    id_quat(),
                ),
                vec![g.clone()],
            ))
            .iter()
            .skip(1)
            .map(|s| s.bounds.min.y)
            .fold(f32::MAX, f32::min)
        };
        let trunk = &root.children[0];
        let glass_in_trunk = super::super::facade::facades(trunk).len();
        assert_eq!(
            glass_in_trunk,
            super::super::facade::facades(&root).len(),
            "every glass tier rides the trunk"
        );
        for piece in &root.children[1..] {
            assert!(
                base(piece) > base(trunk),
                "a piece is based below the trunk"
            );
            let span = top_of(piece, 0.0) - base(piece);
            assert!(span < 20.0, "a {span} m piece would fall as a tower");
        }
        // Every ruin in the conflict band leaves the trunk standing: still
        // there, still upright to within the declared sway.
        for seed in 0..64_u64 {
            let mut g = NeonMegatower.build("");
            crate::pds::ruin::apply_ruin_bounded(&mut g, 0.95, seed, NeonMegatower.ruin_max_lean());
            assert!(
                super::super::facade::facades(&g).len() == glass_in_trunk,
                "seed {seed}: the ruin took a glass tier"
            );
            let q = g.transform.rotation.0;
            let tilt = 2.0 * (q[0] * q[0] + q[2] * q[2]).sqrt().asin();
            assert!(
                tilt * TIP <= RUIN_SWAY_M + 1e-3,
                "seed {seed}: leans {tilt} rad"
            );
        }
    }

    /// The one sound the tower keeps is the substation hum at the street
    /// intake, not a hum from its middle (#1559).
    #[test]
    fn its_one_sound_is_at_the_street() {
        fn voices(g: &Generator, at: [f32; 3], out: &mut Vec<[f32; 3]>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if !matches!(g.audio, crate::pds::SovereignAudioConfig::None) {
                out.push(here);
            }
            g.children.iter().for_each(|c| voices(c, here, out));
        }
        let mut at = Vec::new();
        voices(&NeonMegatower.build(""), [0.0; 3], &mut at);
        assert_eq!(at.len(), 1, "one looping voice: {at:?}");
        let [x, y, _] = at[0];
        assert!(y < 4.0, "the hum is {y} m up");
        assert!(
            x.abs() >= BASE_W * 0.5,
            "the hum is inside the podium at x {x}"
        );
    }

    /// Down to its smallest real scale the tower keeps real proportions: an
    /// office floor at least 2.65 m floor to floor and a lobby a person
    /// walks into (#1559). Read from the built tree.
    #[test]
    fn reads_as_a_building_down_to_its_smallest_scale() {
        let root = NeonMegatower.build("");
        for (c, _, m, _) in super::super::facade::facades(&root) {
            let floor = super::super::facade::floor_of(&m);
            assert!(
                floor * MIN_SCALE >= 2.65,
                "the {floor} m floors at {c:?} are {} m at {MIN_SCALE}",
                floor * MIN_SCALE
            );
        }
        let cards = crate::catalogue::items::util::window_cards(&root);
        assert!(!cards.is_empty(), "the lobby lost its glazing");
        for card in cards {
            assert!(
                card.size[1] * MIN_SCALE >= 3.0,
                "a {} m lobby is {} m at {MIN_SCALE}",
                card.size[1],
                card.size[1] * MIN_SCALE
            );
        }
        let lot = 2.0 * NeonMegatower.lot_half_width() * MIN_SCALE;
        assert!(lot >= 12.0, "it grows on lots down to {lot} m");
    }
}
