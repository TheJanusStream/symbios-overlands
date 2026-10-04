//! Arcade block - a Cyberpunk secondary, rebuilt under #1559 as a mid-rise
//! mixed-use block: six floors of flats and offices in a punched precast
//! facade carried over a street arcade on four columns, two lit shopfronts
//! set into the ground floor between stone piers with the lobby door between
//! them, a framed sign on the fascia over each shop, a framed blade sign on
//! the corner, a glazed bay up the front and rooftop plant. About 26 m to
//! the parapet at scale 1.
//!
//! What it replaced was a one-storey dark box with neon on every edge and a
//! tile sign on the roof - an arcade in the games sense, and the most
//! "stylized" thing on Isoline's streets. The name now means the colonnade:
//! the upper floors overhang the pavement and the shopfronts sit back under
//! a lit soffit, which is where a street of these buildings puts its light.
//!
//! The upper facade is one stack-bond texture ([`facade`]) with a fat joint,
//! so every window is a cell and the precast between is the mortar. Each
//! shopfront is a real recess: the ground floor's front stands 0.6 m behind
//! the piers and the fascia, the shop's lit room fills the reveal and its
//! glazing sits 10 cm in from the pier faces.
//!
//! Its sizes: a lot fits it by its half side ([`LOT_HALF`], so a typical
//! cleared lot in Isoline draws it at about its authored size), a seeded room
//! spaces it by a circle that keeps props off its plinth ([`CLEARANCE`]) and
//! floors it on the ground under its footing ([`GROUND_R`]), and ruin sways
//! its top about a metre at most ([`RUIN_SWAY_M`]). The footing carries the
//! building - with everything that hangs on its walls, the door, the signs,
//! the bay - as the ruin pass's anchor; the pieces a fought-over room may
//! lose are the ones that stand on something: the shops, the columns, the
//! coping and the plant.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, prim, solid, with_face,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::ThemeArchetype;

use super::facade::{
    MIDRISE, OFFICE_JOINT, PUNCHED_JOINT, block, facade, framed_sign, glazing, light_bar, lit_room,
    louvre, palette, paving, precast, steel,
};
use super::fx;

/// The block: 8 bays square, centred on the axis.
const W: f32 = 13.6;
const D: f32 = 13.6;
const FRONT: f32 = -D * 0.5;
/// The pavement slab (root).
const PLINTH_H: f32 = 0.3;
/// The shopfront line - the piers' faces, under the arcade - the ground
/// floor's own front 0.6 m behind it, and the ground floor's head.
const SHOP_FACE: f32 = -4.3;
const CORE_FRONT: f32 = SHOP_FACE + 0.6;
const GROUND_TOP: f32 = 5.3;
/// Six upper floors.
const UPPER_TOP: f32 = GROUND_TOP + 6.0 * MIDRISE.floor;
/// The shops either side of the lobby door: x from `SHOP_X0` to `SHOP_X1`,
/// and their head (the fascia's foot).
const SHOP_X0: f32 = 1.4;
const SHOP_X1: f32 = 6.2;
const SHOP_HEAD: f32 = 4.3;
/// The lobby door between the shops.
const DOOR_H: f32 = 2.6;

/// The circle a seeded room spaces the block by. The settlement keeps a
/// prop only half the two clearances from a building, so this is about
/// twice the plinth's 9.9 m corner reach: over every Cyberpunk room in seeds
/// 0-6000 no prop stands inside the block or touches it.
const CLEARANCE: f32 = 18.0;
/// The ground the block stands on: the footing's corner reach, so its floor
/// is set by the highest ground under it, not under its spacing circle.
const GROUND_R: f32 = 10.0;
/// Half the narrow side of the lot the block fills at scale 1: the plinth's
/// half side and a hand's breadth (the blade sign overhangs the pavement).
const LOT_HALF: f32 = 7.1;
/// How far the ruin pass may lean the whole building, at its tip (m). The
/// pass also knocks the trunk askew on its own, on two axes at once - up to
/// sqrt(2)/4 of that while the trunk carries pieces, sqrt(2) times it once
/// they are gone - and measured over thousands of conflict ruins the top
/// moves about a metre at most.
const RUIN_SWAY_M: f32 = 1.0;
/// The tallest part, the top of the flue on the roof.
const TIP: f32 = UPPER_TOP + 0.55 + 3.6;
/// How deep the roof plant is bedded into the coping (m).
const ROOF_BED: f32 = 0.15;
/// The smallest scale the block still reads as a building at: its 3.4 m
/// floors stay 2.65 m floor to floor and its door 2 m tall (#1559).
#[cfg(test)]
const MIN_SCALE: f32 = 0.78;

/// Warm grey precast - the upper floors' wall.
const PRECAST_WARM: [f32; 3] = [0.30, 0.285, 0.27];
/// The ground floor's dark stone.
const GROUND_STONE: [f32; 3] = [0.11, 0.11, 0.12];

pub struct ArcadeBlock;

impl CatalogueEntry for ArcadeBlock {
    fn slug(&self) -> &'static str {
        "arcade_block"
    }
    fn name(&self) -> &'static str {
        "Arcade Block"
    }
    fn description(&self) -> &'static str {
        "Mid-rise block of flats and offices over a street arcade, lit shopfronts and \
         framed signs behind the colonnade, plant on the roof."
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
            min_spawn_dist: 34.0,
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
            [W + 0.4, PLINTH_H, D + 0.4],
            0.0,
            paving([0.30, 0.30, 0.31]),
        )),
        [0.0, PLINTH_H * 0.5, 0.0],
        id_quat(),
    );
    // The trunk: the footing (4 cm wider than the plinth each way) carrying
    // the ground floor, which carries its piers, fascia, door and signs and
    // the floors above with the bay and the blade sign.
    let base = footing(W + 0.6, D + 0.6, [0.0, 0.0], CLEARANCE);
    let mut parts = vec![nest(base, vec![ground_floor(upper_floors())])];
    parts.extend(street_pieces());
    parts.extend(roof());
    nest(plinth, parts)
}

/// The ground floor: dark stone, its front 0.6 m behind the shopfront line,
/// the three piers and the fascia band that frame the two shops and the
/// door on that line. It carries `upper`.
fn ground_floor(upper: Generator) -> Generator {
    let back = D * 0.5 + 0.05;
    let core = block(
        [W + 0.1, GROUND_TOP - PLINTH_H + 0.05, back - CORE_FRONT],
        [
            0.0,
            (PLINTH_H + GROUND_TOP + 0.05) * 0.5,
            (CORE_FRONT + back) * 0.5,
        ],
        precast(GROUND_STONE),
    );
    let (z0, z1) = (SHOP_FACE, CORE_FRONT + 0.05);
    // The piers start 1 cm into the plinth, so their feet never share the
    // plane of the ground floor's.
    let pier_h = SHOP_HEAD + 0.05 - (PLINTH_H - 0.01);
    let pier_y = PLINTH_H - 0.01 + pier_h * 0.5;
    let end = W * 0.5 + 0.01;
    let mut parts = vec![block(
        [SHOP_X0 * 2.0, pier_h, z1 - z0],
        [0.0, pier_y, (z0 + z1) * 0.5],
        precast(GROUND_STONE),
    )];
    for sx in [-1.0_f32, 1.0] {
        parts.push(block(
            [end - SHOP_X1, pier_h, z1 - z0],
            [sx * (end + SHOP_X1) * 0.5, pier_y, (z0 + z1) * 0.5],
            precast(GROUND_STONE),
        ));
    }
    // The fascia: 2 cm proud of the piers, 2 cm inside the core's flanks,
    // its top 2 cm under the core's and its back 2 cm short of the piers',
    // so it shares no face's plane.
    let fascia_top = GROUND_TOP + 0.03;
    let fascia_back = z1 - 0.02;
    parts.push(block(
        [W + 0.06, fascia_top - SHOP_HEAD, fascia_back - (z0 - 0.02)],
        [
            0.0,
            (SHOP_HEAD + fascia_top) * 0.5,
            (z0 - 0.02 + fascia_back) * 0.5,
        ],
        precast(GROUND_STONE),
    ));
    parts.extend(street_signs());
    parts.push(upper);
    nest(core, parts)
}

/// The six upper floors: punched precast, the arcade soffit lit under them.
fn upper_floors() -> Generator {
    let h = UPPER_TOP - GROUND_TOP;
    let c = [0.0, GROUND_TOP + h * 0.5, 0.0];
    let mut body = block(
        [W, h, D],
        c,
        facade(
            MIDRISE,
            [0.035, 0.040, 0.050],
            PRECAST_WARM,
            PUNCHED_JOINT,
            c,
            GROUND_TOP,
        ),
    );
    body.kind = with_face(body.kind, FaceKey::Bottom, glow(palette::WARM_LIGHT, 0.7));
    nest(body, front_fittings())
}

/// The street-level pieces: the two shops (a warm cafe, a cooler store),
/// each a lit room filling its reveal behind glazing, and the four columns
/// of the colonnade. Each stands on the plinth, so a fought-over room may
/// take any of them and leave nothing hanging.
fn street_pieces() -> Vec<Generator> {
    let mut out = Vec::new();
    let w = SHOP_X1 - SHOP_X0;
    let (y0, y1) = (PLINTH_H - 0.02, SHOP_HEAD + 0.03);
    let (rz0, rz1) = (SHOP_FACE + 0.15, CORE_FRONT + 0.04);
    for (sx, room, lit) in [
        (-1.0_f32, [0.72, 0.56, 0.38], 0.55),
        (1.0, [0.52, 0.60, 0.68], 0.5),
    ] {
        let cx = sx * (SHOP_X0 + SHOP_X1) * 0.5;
        out.push(nest(
            lit_room(
                [w + 0.1, y1 - y0, rz1 - rz0],
                [cx, (y0 + y1) * 0.5, (rz0 + rz1) * 0.5],
                room,
                lit,
            ),
            vec![glazing(
                [w + 0.06, SHOP_HEAD - PLINTH_H + 0.06],
                [cx, (PLINTH_H + SHOP_HEAD) * 0.5, SHOP_FACE + 0.1],
                (4, 2),
            )],
        ));
    }
    let col = 0.5;
    let col_z = FRONT + 0.35;
    for x in [-6.25_f32, -2.1, 2.1, 6.25] {
        out.push(block(
            [col, GROUND_TOP - PLINTH_H + 0.05, col],
            [x, (PLINTH_H + GROUND_TOP + 0.05) * 0.5, col_z],
            precast(PRECAST_WARM),
        ));
    }
    out
}

/// What hangs on the ground floor's front: the lobby door's light and a
/// framed sign on the fascia over each shop, the cafe's buzzing - the one
/// sound the block keeps, at the street. They ride the trunk: the ruin pass
/// knocks each piece askew on its own, and a sign knocked one way off a
/// wall knocked the other hangs in the air beside it.
fn street_signs() -> Vec<Generator> {
    let w = SHOP_X1 - SHOP_X0;
    let mut out = vec![light_bar(
        [2.2, DOOR_H, 0.06],
        [0.0, PLINTH_H + 0.01 + DOOR_H * 0.5, SHOP_FACE - 0.02],
        palette::WARM_LIGHT,
        1.2,
    )];
    for (i, (sx, color)) in [(-1.0_f32, [0.95, 0.55, 0.16]), (1.0, [0.16, 0.66, 0.72])]
        .into_iter()
        .enumerate()
    {
        let mut sign = framed_sign(
            SHOP_FACE - 0.02,
            [sx * (SHOP_X0 + SHOP_X1) * 0.5, SHOP_HEAD + 0.5],
            [w - 0.4, 0.72],
            color,
            2.2,
        );
        let face = sign.pop().expect("a sign has a face");
        let mut backing = sign.pop().expect("a sign has a backing");
        if i == 0 {
            backing.audio = fx::neon_buzz();
        }
        out.push(nest(backing, vec![face]));
    }
    out
}

/// What hangs on the street face above the arcade, riding the upper floors
/// as the signs below ride the ground floor: a glazed bay two windows wide
/// up four floors, 0.6 m proud, and a framed blade sign on the corner, lit
/// on both faces and projecting over the pavement - not solid, as a sign
/// seven metres up is nothing anyone walks into.
fn front_fittings() -> Vec<Generator> {
    let bay_y0 = GROUND_TOP + 2.0 * MIDRISE.floor;
    // Its top 4 cm under the parapet's, inside the coping.
    let bay_h = UPPER_TOP - 0.04 - bay_y0;
    let bay_d = 0.65;
    let bay_c = [
        2.0 * MIDRISE.panel,
        bay_y0 + bay_h * 0.5,
        FRONT - bay_d * 0.5 + 0.05,
    ];
    let mut bay = block(
        [2.0 * MIDRISE.panel, bay_h, bay_d],
        bay_c,
        facade(
            MIDRISE,
            palette::GLASS_OFFICE,
            palette::FRAME_GRAPHITE,
            OFFICE_JOINT,
            bay_c,
            GROUND_TOP,
        ),
    );
    bay.kind = with_face(bay.kind, FaceKey::Top, steel(palette::FRAME_GRAPHITE));

    let blade_z = FRONT - 0.45;
    let blade_at = [-W * 0.5 + 0.8, GROUND_TOP + 5.5, blade_z];
    let blade = nest(
        prim(
            cuboid_tapered([0.3, 7.0, 1.0], 0.0, steel(palette::LOUVRE_DARK)),
            blade_at,
            id_quat(),
        ),
        vec![light_bar(
            [0.36, 6.4, 0.7],
            blade_at,
            [0.72, 0.12, 0.14],
            2.0,
        )],
    );
    vec![bay, blade]
}

/// The parapet and the plant on the roof: a lift overrun, two condensers
/// and a flue - each its own piece, bedded [`ROOF_BED`] into the coping so
/// the ruin pass's knock, which lowers the coping under a unit by up to a
/// dozen centimetres, never lifts one clear of it.
fn roof() -> Vec<Generator> {
    let cap_h = 0.6;
    let mut coping = block(
        [W + 0.3, cap_h, D + 0.3],
        [0.0, UPPER_TOP + cap_h * 0.5 - 0.05, 0.0],
        precast(PRECAST_WARM),
    );
    coping.kind = with_face(coping.kind, FaceKey::Top, paving(palette::ROOF_GREY));
    let top = UPPER_TOP + cap_h - 0.05;
    // Each unit's foot is ROOF_BED under the coping's top; its top is where
    // it always was.
    let unit = |size: [f32; 3], at: [f32; 2], mat: crate::pds::SovereignMaterialSettings| {
        let h = size[1] + ROOF_BED;
        block(
            [size[0], h, size[2]],
            [at[0], top + size[1] - h * 0.5, at[1]],
            mat,
        )
    };
    let flue_h = 3.6 + ROOF_BED;
    vec![
        coping,
        unit([3.0, 2.8, 3.0], [-3.8, 2.6], precast(palette::CONCRETE_MID)),
        unit([2.4, 1.5, 1.8], [0.9, 3.4], louvre([0.24, 0.25, 0.26])),
        unit([2.4, 1.5, 1.8], [3.8, 3.4], louvre([0.24, 0.25, 0.26])),
        prim(
            solid(cylinder_tapered(
                0.3,
                flue_h,
                10,
                0.0,
                steel(palette::FRAME_GRAPHITE),
            )),
            [5.2, top + 3.6 - flue_h * 0.5, -2.4],
            id_quat(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::super::tests::SPACING;
    use super::*;
    use crate::catalogue::items::util::{
        assert_cards_do_not_overlap, assert_no_coplanar_faces, assert_no_glazing_on_solids,
        assert_no_tilted_parents, assert_sanitize_stable, has_emissive, window_cards,
    };
    use crate::pds::GeneratorKind;

    fn count(g: &Generator) -> usize {
        1 + g.children.iter().map(count).sum::<usize>()
    }

    fn voices(g: &Generator, at: f32, out: &mut Vec<f32>) {
        let y = at + g.transform.translation.0[1];
        if !matches!(g.audio, crate::pds::SovereignAudioConfig::None) {
            out.push(y);
        }
        g.children.iter().for_each(|c| voices(c, y, out));
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ArcadeBlock.build(""), "arcade_block");
    }

    #[test]
    fn no_glazing_lands_on_a_solid() {
        assert_no_glazing_on_solids(&ArcadeBlock.build(""), "arcade_block");
    }

    #[test]
    fn glazed_surfaces_do_not_collide() {
        assert_cards_do_not_overlap(&ArcadeBlock.build(""), "arcade_block");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&ArcadeBlock.build(""), "arcade_block");
    }

    #[test]
    fn no_faces_tie_for_depth() {
        assert_no_coplanar_faces(&ArcadeBlock.build(""), "arcade_block");
    }

    #[test]
    fn has_its_lit_shopfronts() {
        assert!(has_emissive(&ArcadeBlock.build("")));
    }

    #[test]
    fn stays_inside_the_part_budget() {
        let n = count(&ArcadeBlock.build(""));
        assert!(n <= 60, "{n} parts - a district multiplies every one");
    }

    /// The punched facade's window rows sit on the floors, and the street
    /// face and the bay end on piers (#1559).
    #[test]
    fn the_facade_meets_the_floors_and_the_corners() {
        let root = ArcadeBlock.build("");
        let floors = super::super::facade::assert_floor_lines(&root, "arcade_block", GROUND_TOP);
        let corners = super::super::facade::assert_corner_mullions(&root, "arcade_block");
        assert!(floors >= 2 && corners >= 2, "{floors} / {corners}");
    }

    /// One sound, and it is at the street: the cafe sign's buzz, not a roof
    /// hum heard from nowhere (#1559).
    #[test]
    fn its_one_sound_is_at_street_level() {
        let mut at = Vec::new();
        voices(&ArcadeBlock.build(""), 0.0, &mut at);
        assert_eq!(at.len(), 1, "one looping voice");
        assert!(at[0] < GROUND_TOP, "the voice is {} m up", at[0]);
    }

    /// The arcade is a real one, deep and tall enough to walk under, and the
    /// shopfronts are recesses: every shop's glazing stands behind the pier
    /// faces either side of it and its lit room behind the glazing - no lit
    /// box stuck on the wall (#1559).
    #[test]
    fn the_shopfronts_are_recessed_under_a_real_arcade() {
        let root = ArcadeBlock.build("");
        let cards = window_cards(&root);
        assert_eq!(cards.len(), 2, "two shopfronts");
        for card in &cards {
            assert!(
                card.center[2] > SHOP_FACE + 0.05,
                "glazing at z {} is not behind the piers at {SHOP_FACE}",
                card.center[2]
            );
            assert!(
                card.center[2] - FRONT >= 2.0,
                "a {} m arcade in front of the shops",
                card.center[2] - FRONT
            );
        }
        // Every lit surface at street level stands behind the pier line.
        fn walk(g: &Generator, at: [f32; 3], out: &mut Vec<([f32; 3], [f32; 3])>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let GeneratorKind::Cuboid { size, common, .. } = &g.kind
                && common.material.emission_strength.0 > 0.0
                && common.material.emission_strength.0 < 1.0
            {
                out.push((here, size.0));
            }
            g.children.iter().for_each(|c| walk(c, here, out));
        }
        let mut rooms = Vec::new();
        walk(&root, [0.0; 3], &mut rooms);
        assert!(rooms.len() >= 2, "the shops lost their lit rooms");
        for (at, size) in rooms {
            assert!(
                at[2] - size[2] * 0.5 > SHOP_FACE,
                "a lit room at {at:?} stands proud of the shopfront line"
            );
        }
        let soffit = GROUND_TOP - PLINTH_H;
        assert!(soffit >= 4.5, "a {soffit} m soffit");
    }

    /// Three sizes that agree: the lot fit's half side covers the plinth,
    /// the settlement's circle is [`SPACING`] times its corner reach, and
    /// the ruin sways the top by the metre declared.
    #[test]
    fn its_three_sizes_cover_what_they_measure() {
        let root = ArcadeBlock.build("");
        let GeneratorKind::Cuboid { size, .. } = &root.kind else {
            panic!("the plinth is a cuboid");
        };
        let half = size.0[0].max(size.0[2]) * 0.5;
        let e = ArcadeBlock;
        assert!(e.lot_half_width() >= half, "a {half} m half side on a lot");
        assert!(
            e.footprint().clearance >= SPACING * half * std::f32::consts::SQRT_2,
            "a {} m circle",
            e.footprint().clearance
        );
        let lean = e.ruin_max_lean().expect("the block bounds its lean");
        assert!(lean * TIP <= RUIN_SWAY_M + 1e-3, "ruin sways the top");
    }

    /// The ruin pass takes pieces, never the building: the trunk carrying
    /// the facade is the root's lowest child and survives every roll.
    #[test]
    fn ruin_takes_pieces_never_the_building() {
        for seed in 0..64_u64 {
            let mut g = ArcadeBlock.build("");
            crate::pds::ruin::apply_ruin_bounded(&mut g, 0.95, seed, ArcadeBlock.ruin_max_lean());
            let facades = super::super::facade::facades(&g);
            assert!(
                facades.iter().any(|(_, s, _, _)| s[1] > 15.0),
                "seed {seed}: the ruin took the building"
            );
        }
    }

    /// Down to its smallest real scale the block keeps real proportions:
    /// floors at least 2.65 m floor to floor and a door 2 m tall (#1559).
    #[test]
    fn reads_as_a_building_down_to_its_smallest_scale() {
        let root = ArcadeBlock.build("");
        for (c, _, m, _) in super::super::facade::facades(&root) {
            let floor = super::super::facade::floor_of(&m);
            assert!(
                floor * MIN_SCALE >= 2.65,
                "the {floor} m floors at {c:?} are {} m at {MIN_SCALE}",
                floor * MIN_SCALE
            );
        }
        // The door's light rides the ground floor, in the trunk.
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
        let door = door_of(&root).expect("a lobby door");
        assert!(door * MIN_SCALE >= 2.0, "a {door} m door at {MIN_SCALE}");
    }
}
