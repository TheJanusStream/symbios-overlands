//! Holo billboard (shown as "Media Facade Block") - a Cyberpunk secondary,
//! rebuilt under #1559 as a media facade block: an eight-storey building of
//! dark punched precast whose street face carries a large framed screen of
//! content tiles across four floors and more, over a glazed, lit shop set
//! into the ground floor under a lit soffit, with plant on the roof. About
//! 31 m to the parapet at scale 1.
//!
//! What it replaced was a billboard on two posts - which, grown on a city
//! lot as a building (Isoline grew fourteen), read as a sign standing on a
//! lawn. The slug stays so every saved district keeps its references; what
//! it names is now the kind of building a screen hangs on.
//!
//! The screen is a dark steel frame carrying six content tiles - adverts,
//! each a lit ground with a headline across it - at a broad face's moderate
//! strength, so each reads as lit colour rather than blooming white, inside
//! a hot thin border; the rest of the building is ordinary facade
//! ([`facade`]). The shop is a real recess between two piers under a
//! fascia, its glazing 10 cm in from their faces.
//!
//! Its sizes: a lot fits it by its half side ([`LOT_HALF`]), a seeded room
//! spaces it by a circle that keeps props off its plinth ([`CLEARANCE`]) and
//! floors it on the ground under its footing ([`GROUND_R`]), and ruin sways
//! its top about a metre at most ([`RUIN_SWAY_M`]). The footing carries
//! the building and its screen as the ruin pass's anchor; the pieces a
//! fought-over room may lose are the shop, the columns, the coping and the
//! plant.

use crate::catalogue::items::util::{
    cuboid_tapered, footing, glow, id_quat, nest, prim, solid, with_face,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::ThemeArchetype;

use super::facade::{
    MEDIA, PUNCHED_JOINT, block, facade, glazing, light_bar, lit_room, louvre, palette, paving,
    precast, steel,
};

/// The upper body: 8 bays square, centred.
const HALF: f32 = 6.4;
const PLINTH_H: f32 = 0.3;
/// The shopfront line (the piers' faces) 1.6 m in under the floors above,
/// the ground floor's own front 0.6 m behind it, the shop's head and the
/// ground floor's; seven office floors above it.
const SHOP_FACE: f32 = -HALF + 1.6;
const CORE_FRONT: f32 = SHOP_FACE + 0.6;
const SHOP_HEAD: f32 = 4.4;
const SHOP_HALF: f32 = HALF - 0.9;
const GROUND_TOP: f32 = 5.3;
const UPPER_TOP: f32 = GROUND_TOP + 7.0 * MEDIA.floor;
/// The screen's frame: centre height and size, and the tile grid in it.
const SCREEN_Y: f32 = 16.5;
const SCREEN_W: f32 = 11.0;
const SCREEN_H: f32 = 16.0;
const TILES: (usize, usize) = (2, 3);

/// The circle a seeded room spaces the block by. The settlement keeps a
/// prop only half the two clearances from a building, so this is about
/// twice the plinth's 9.3 m corner reach: over every Cyberpunk room in seeds
/// 0-6000 no prop stands inside the block or touches it.
const CLEARANCE: f32 = 17.0;
/// The ground the block stands on: the footing's corner reach, so its floor
/// is set by the highest ground under it, not under its spacing circle.
const GROUND_R: f32 = 9.4;
/// Half the narrow side of the lot the block fills at scale 1: out to the
/// screen's face.
const LOT_HALF: f32 = 7.0;
/// How far the ruin pass may lean the whole building, at its tip (m). The
/// pass also knocks the trunk askew on its own, on two axes at once - up to
/// sqrt(2)/4 of that while the trunk carries pieces, sqrt(2) times it once
/// they are gone - and measured over thousands of conflict ruins the top
/// moves about a metre at most.
const RUIN_SWAY_M: f32 = 1.0;
const TIP: f32 = UPPER_TOP + 0.55 + 2.6;
/// How deep the roof plant is bedded into the coping (m).
const ROOF_BED: f32 = 0.15;
/// The smallest scale the block still reads as a building at: its 3.6 m
/// floors stay 2.65 m floor to floor (#1559).
#[cfg(test)]
const MIN_SCALE: f32 = 0.74;

/// Charcoal precast - the ordinary facade round the screen.
const CHARCOAL: [f32; 3] = [0.15, 0.15, 0.16];
/// The ground floor's dark stone.
const GROUND_STONE: [f32; 3] = [0.11, 0.11, 0.12];

/// The six content tiles, row by row from the top, each an advert in two
/// pieces: a lit ground and a headline across it - (ground colour and
/// strength, headline colour and strength, headline height as a share of
/// the tile from its foot, headline width as a share of the tile). Grounds
/// stay at a broad face's moderate strength so they hold their colour; a
/// headline is a thin bar and may run hotter, or be dark type on a light
/// ground.
type Advert = ([f32; 3], f32, [f32; 3], f32, f32, f32);
const CONTENT: [Advert; 6] = [
    ([0.06, 0.12, 0.40], 1.5, [0.95, 0.95, 0.92], 2.4, 0.2, 0.7),
    ([0.75, 0.38, 0.10], 1.4, [0.10, 0.06, 0.04], 0.0, 0.75, 0.6),
    ([0.05, 0.30, 0.36], 1.4, [0.95, 0.95, 0.92], 2.2, 0.3, 0.5),
    ([0.40, 0.08, 0.24], 1.3, [1.00, 0.85, 0.50], 2.2, 0.15, 0.8),
    ([0.70, 0.68, 0.62], 1.1, [0.06, 0.12, 0.40], 1.5, 0.8, 0.4),
    ([0.08, 0.10, 0.28], 1.4, [0.20, 0.75, 0.80], 2.2, 0.5, 0.75),
];

pub struct HoloBillboard;

impl CatalogueEntry for HoloBillboard {
    fn slug(&self) -> &'static str {
        "holo_billboard"
    }
    fn name(&self) -> &'static str {
        "Media Facade Block"
    }
    fn description(&self) -> &'static str {
        "Media facade block: an eight-storey building whose street face carries a large \
         framed screen of content tiles over a lit shop."
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
    let w = HALF * 2.0 + 0.4;
    let plinth = prim(
        solid(cuboid_tapered(
            [w, PLINTH_H, w],
            0.0,
            paving([0.30, 0.30, 0.31]),
        )),
        [0.0, PLINTH_H * 0.5, 0.0],
        id_quat(),
    );
    // The trunk: the footing (4 cm wider than the plinth each way) carrying
    // the ground floor, which carries its piers, fascia, the floors above
    // and the screen.
    let base = footing(w + 0.2, w + 0.2, [0.0, 0.0], CLEARANCE);
    let mut parts = vec![nest(base, vec![ground_floor(upper_floors())])];
    parts.extend(pieces());
    nest(plinth, parts)
}

/// The ground floor: dark stone 5 cm proud of the floors above at the sides
/// and back, its front 0.6 m behind the shopfront line, and the two piers
/// and the fascia that frame the shop on that line. It carries `upper`.
fn ground_floor(upper: Generator) -> Generator {
    let edge = HALF + 0.05;
    let core = block(
        [edge * 2.0, GROUND_TOP + 0.05 - PLINTH_H, edge - CORE_FRONT],
        [
            0.0,
            (PLINTH_H + GROUND_TOP + 0.05) * 0.5,
            (CORE_FRONT + edge) * 0.5,
        ],
        precast(GROUND_STONE),
    );
    let (z0, z1) = (SHOP_FACE, CORE_FRONT + 0.05);
    // The piers start 1 cm into the plinth, so their feet never share the
    // plane of the ground floor's.
    let pier_h = SHOP_HEAD + 0.05 - (PLINTH_H - 0.01);
    let end = HALF + 0.01;
    let mut parts = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        parts.push(block(
            [end - SHOP_HALF, pier_h, z1 - z0],
            [
                sx * (end + SHOP_HALF) * 0.5,
                PLINTH_H - 0.01 + pier_h * 0.5,
                (z0 + z1) * 0.5,
            ],
            precast(GROUND_STONE),
        ));
    }
    // The fascia: 2 cm proud of the piers, inside the core's flanks, its top
    // under the core's and its back short of the piers', so it shares no
    // face's plane.
    let fascia_top = GROUND_TOP + 0.03;
    let fascia_back = z1 - 0.02;
    parts.push(block(
        [
            HALF * 2.0 + 0.06,
            fascia_top - SHOP_HEAD,
            fascia_back - (z0 - 0.02),
        ],
        [
            0.0,
            (SHOP_HEAD + fascia_top) * 0.5,
            (z0 - 0.02 + fascia_back) * 0.5,
        ],
        precast(GROUND_STONE),
    ));
    parts.push(upper);
    nest(core, parts)
}

/// The office floors: charcoal punched precast, the overhang's soffit lit,
/// and the screen on the street face.
fn upper_floors() -> Generator {
    let h = UPPER_TOP - GROUND_TOP;
    let c = [0.0, GROUND_TOP + h * 0.5, 0.0];
    let mut body = block(
        [HALF * 2.0, h, HALF * 2.0],
        c,
        facade(
            MEDIA,
            [0.035, 0.040, 0.050],
            CHARCOAL,
            PUNCHED_JOINT,
            c,
            GROUND_TOP,
        ),
    );
    body.kind = with_face(body.kind, FaceKey::Bottom, glow(palette::WARM_LIGHT, 0.7));
    nest(body, vec![screen()])
}

/// The screen: a dark steel frame bedded 5 cm into the facade, six content
/// tiles bedded 1 cm into the frame, each carrying its headline 1 cm into
/// it, and a hot thin border round them - the top and bottom bars ending
/// inside the side bars, a hair thinner and a hair shorter of their ends,
/// so no two share a face's plane.
fn screen() -> Generator {
    let frame_t = 0.5;
    let frame_z = -HALF - frame_t * 0.5 + 0.05;
    let frame = block(
        [SCREEN_W, SCREEN_H, frame_t],
        [0.0, SCREEN_Y, frame_z],
        steel(palette::LOUVRE_DARK),
    );
    let face_z = frame_z - frame_t * 0.5;
    let tile_z = face_z - 0.02;
    let margin = 0.3;
    let gap = 0.16;
    let (cols, rows) = TILES;
    let cell_w = (SCREEN_W - margin * 2.0) / cols as f32;
    let cell_h = (SCREEN_H - margin * 2.0) / rows as f32;
    let mut parts = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let (ground, lit, type_color, type_lit, type_at, type_w) = CONTENT[r * cols + c];
            // Seen from the front, the world's +X is the viewer's left: count
            // columns from that side so the content reads in order.
            let x = ((cols as f32 - 1.0) * 0.5 - c as f32) * cell_w;
            let y = SCREEN_Y + ((rows as f32 - 1.0) * 0.5 - r as f32) * cell_h;
            let (tw, th) = (cell_w - gap, cell_h - gap);
            parts.push(nest(
                light_bar([tw, th, 0.06], [x, y, tile_z], ground, lit),
                vec![light_bar(
                    [tw * type_w, 0.48, 0.06],
                    [x, y - th * 0.5 + th * type_at, tile_z - 0.05],
                    type_color,
                    type_lit,
                )],
            ));
        }
    }
    let bar = 0.1;
    let inner_w = SCREEN_W - margin * 2.0 + bar;
    let inner_h = SCREEN_H - margin * 2.0 + bar;
    for s in [-1.0_f32, 1.0] {
        parts.push(light_bar(
            [inner_w, bar, 0.05],
            [0.0, SCREEN_Y + s * inner_h * 0.5, tile_z],
            palette::COOL_WHITE,
            3.5,
        ));
        parts.push(light_bar(
            [bar, inner_h + bar + 0.02, 0.06],
            [s * inner_w * 0.5, SCREEN_Y, tile_z],
            palette::COOL_WHITE,
            3.5,
        ));
    }
    nest(frame, parts)
}

/// The pieces a fought-over room may lose: the shop - a lit room filling its
/// reveal behind glazing - the two corner columns under the overhang, the
/// coping and the plant on the roof.
fn pieces() -> Vec<Generator> {
    let (y0, y1) = (PLINTH_H - 0.02, SHOP_HEAD + 0.03);
    let (rz0, rz1) = (SHOP_FACE + 0.15, CORE_FRONT + 0.04);
    let mut out = vec![nest(
        lit_room(
            [SHOP_HALF * 2.0 + 0.1, y1 - y0, rz1 - rz0],
            [0.0, (y0 + y1) * 0.5, (rz0 + rz1) * 0.5],
            [0.66, 0.56, 0.44],
            0.5,
        ),
        vec![glazing(
            [SHOP_HALF * 2.0 + 0.06, SHOP_HEAD - PLINTH_H + 0.06],
            [0.0, (PLINTH_H + SHOP_HEAD) * 0.5, SHOP_FACE + 0.1],
            (6, 2),
        )],
    )];
    for sx in [-1.0_f32, 1.0] {
        out.push(block(
            [0.5, GROUND_TOP + 0.05 - PLINTH_H, 0.5],
            [
                sx * (HALF - 0.35),
                (PLINTH_H + GROUND_TOP + 0.05) * 0.5,
                -HALF + 0.35,
            ],
            precast(CHARCOAL),
        ));
    }
    let cap_h = 0.6;
    let mut coping = block(
        [HALF * 2.0 + 0.3, cap_h, HALF * 2.0 + 0.3],
        [0.0, UPPER_TOP + cap_h * 0.5 - 0.05, 0.0],
        precast(CHARCOAL),
    );
    coping.kind = with_face(coping.kind, FaceKey::Top, paving(palette::ROOF_GREY));
    let top = UPPER_TOP + cap_h - 0.05;
    out.push(coping);
    // Each unit's foot is ROOF_BED under the coping's top, its top where it
    // always was: the ruin pass knocks the coping and each unit askew on
    // their own, and a unit standing flush on it can be left over it.
    let unit = |size: [f32; 3], at: [f32; 2], mat: crate::pds::SovereignMaterialSettings| {
        let h = size[1] + ROOF_BED;
        block(
            [size[0], h, size[2]],
            [at[0], top + size[1] - h * 0.5, at[1]],
            mat,
        )
    };
    out.push(unit(
        [2.6, 1.8, 2.2],
        [-3.0, 2.6],
        louvre([0.24, 0.25, 0.26]),
    ));
    out.push(unit(
        [2.6, 1.8, 2.2],
        [0.0, 2.6],
        louvre([0.24, 0.25, 0.26]),
    ));
    out.push(unit(
        [2.6, 2.6, 2.6],
        [3.4, 2.2],
        precast(palette::CONCRETE_MID),
    ));
    out
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

    /// The upper floors node: the trunk (footing) carries the ground floor,
    /// which carries the upper floors last.
    fn upper_of(root: &Generator) -> &Generator {
        root.children[0].children[0]
            .children
            .last()
            .expect("the ground floor carries the upper floors")
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&HoloBillboard.build(""), "holo_billboard");
    }

    #[test]
    fn no_glazing_lands_on_a_solid() {
        assert_no_glazing_on_solids(&HoloBillboard.build(""), "holo_billboard");
    }

    #[test]
    fn glazed_surfaces_do_not_collide() {
        assert_cards_do_not_overlap(&HoloBillboard.build(""), "holo_billboard");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&HoloBillboard.build(""), "holo_billboard");
    }

    /// No two faces share a plane and a facing (#1559: the screen border's
    /// corners did).
    #[test]
    fn no_faces_tie_for_depth() {
        assert_no_coplanar_faces(&HoloBillboard.build(""), "holo_billboard");
    }

    #[test]
    fn has_its_screen() {
        assert!(has_emissive(&HoloBillboard.build("")));
    }

    #[test]
    fn stays_inside_the_part_budget() {
        let n = count(&HoloBillboard.build(""));
        assert!(n <= 60, "{n} parts - a district multiplies every one");
    }

    /// The punched facade's window rows sit on the floors and its faces end
    /// on piers (#1559).
    #[test]
    fn the_facade_meets_the_floors_and_the_corners() {
        let root = HoloBillboard.build("");
        let floors = super::super::facade::assert_floor_lines(&root, "holo_billboard", GROUND_TOP);
        let corners = super::super::facade::assert_corner_mullions(&root, "holo_billboard");
        assert!(floors >= 1 && corners >= 1, "{floors} / {corners}");
    }

    /// A building, not a sign on posts: six to ten storeys, and the screen on
    /// the street (`-Z`) face, inside the facade's outline and over the shop.
    #[test]
    fn is_a_building_with_a_screen_on_its_street_face() {
        let root = HoloBillboard.build("");
        let upper = upper_of(&root);
        let GeneratorKind::Cuboid { size: body, .. } = &upper.kind else {
            panic!("the upper floors are a cuboid");
        };
        let storeys = 1 + (body.0[1] / MEDIA.floor).round() as i32;
        assert!((6..=10).contains(&storeys), "{storeys} storeys");
        let screen = &upper.children[0];
        let GeneratorKind::Cuboid { size: frame, .. } = &screen.kind else {
            panic!("the screen's frame is a cuboid");
        };
        let at = screen.transform.translation.0;
        assert!(
            at[2] < -body.0[2] * 0.5 + 0.1,
            "the screen is not on the street face"
        );
        assert!(frame.0[0] < body.0[0], "the screen overhangs the facade");
        assert!(
            at[1] - frame.0[1] * 0.5 > -body.0[1] * 0.5,
            "the screen hangs down over the shop"
        );
    }

    /// The shop is a recess under the floors above: its glazing stands
    /// behind the pier faces, a walkway's depth behind the street face, and
    /// its lit room stands behind the glazing - no lit box on the wall.
    #[test]
    fn the_shop_is_recessed_under_the_overhang() {
        let root = HoloBillboard.build("");
        let cards = window_cards(&root);
        assert_eq!(cards.len(), 1, "one shopfront");
        let card = &cards[0];
        assert!(
            card.center[2] > SHOP_FACE + 0.05,
            "the glazing at z {} is not behind the piers",
            card.center[2]
        );
        assert!(
            card.center[2] > -HALF + 0.8,
            "the glazing at z {} under a facade at {}",
            card.center[2],
            -HALF
        );
        let shop = &root.children[1];
        let GeneratorKind::Cuboid { size, .. } = &shop.kind else {
            panic!("the shop's lit room is a cuboid");
        };
        let front = shop.transform.translation.0[2] - size.0[2] * 0.5;
        assert!(
            front > card.center[2],
            "the lit room at {front} stands in front of its glazing"
        );
    }

    /// Broad lit faces stay moderate: every tile is under the strength at
    /// which a face blooms to white, and everything hotter is a thin bar.
    #[test]
    fn the_screen_holds_its_colour() {
        fn walk(g: &Generator) {
            if let GeneratorKind::Cuboid { size, common, .. } = &g.kind {
                let s = common.material.emission_strength.0;
                let broad = size.0.iter().filter(|d| **d > 0.5).count();
                if broad >= 2 {
                    assert!(s <= 2.0, "a {:?} face lit at {s}", size.0);
                }
            }
            g.children.iter().for_each(walk);
        }
        walk(&HoloBillboard.build(""));
    }

    /// Three sizes that agree: the lot fit's half side covers the plinth
    /// and the screen, the settlement's circle is [`SPACING`] times the
    /// plinth's corner reach, and the ruin sways the top by the metre
    /// declared.
    #[test]
    fn its_three_sizes_cover_what_they_measure() {
        let root = HoloBillboard.build("");
        let GeneratorKind::Cuboid { size, .. } = &root.kind else {
            panic!("the plinth is a cuboid");
        };
        let half = size.0[0].max(size.0[2]) * 0.5;
        let reach = crate::catalogue::items::measure::solids(&root)
            .iter()
            .map(|s| s.bounds.min.z.abs())
            .fold(0.0, f32::max);
        let e = HoloBillboard;
        assert!(
            e.lot_half_width() >= half.max(reach.min(7.0)),
            "a {half} m half side and a screen {reach} m out"
        );
        assert!(
            e.footprint().clearance >= SPACING * half * std::f32::consts::SQRT_2,
            "a {} m circle",
            e.footprint().clearance
        );
        let lean = e.ruin_max_lean().expect("the block bounds its lean");
        assert!(lean * TIP <= RUIN_SWAY_M + 1e-3, "ruin sways the top");
    }

    /// The ruin pass takes pieces, never the building or its screen.
    #[test]
    fn ruin_takes_pieces_never_the_building() {
        for seed in 0..64_u64 {
            let mut g = HoloBillboard.build("");
            crate::pds::ruin::apply_ruin_bounded(&mut g, 0.95, seed, HoloBillboard.ruin_max_lean());
            assert_eq!(
                super::super::facade::facades(&g).len(),
                1,
                "seed {seed}: the ruin took the building"
            );
            assert_eq!(
                upper_of(&g).children.len(),
                1,
                "seed {seed}: the ruin took the screen"
            );
        }
    }

    /// A media facade is silent from the street (#1559).
    #[test]
    fn is_silent() {
        fn walk(g: &Generator) {
            assert!(matches!(g.audio, crate::pds::SovereignAudioConfig::None));
            g.children.iter().for_each(walk);
        }
        walk(&HoloBillboard.build(""));
    }

    /// Down to its smallest real scale the block keeps real proportions:
    /// floors at least 2.65 m floor to floor and a shopfront a person walks
    /// into (#1559).
    #[test]
    fn reads_as_a_building_down_to_its_smallest_scale() {
        let root = HoloBillboard.build("");
        for (c, _, m, _) in super::super::facade::facades(&root) {
            let floor = super::super::facade::floor_of(&m);
            assert!(
                floor * MIN_SCALE >= 2.65,
                "the {floor} m floors at {c:?} are {} m at {MIN_SCALE}",
                floor * MIN_SCALE
            );
        }
        for card in window_cards(&root) {
            assert!(
                card.size[1] * MIN_SCALE >= 2.4,
                "a {} m shopfront at {MIN_SCALE}",
                card.size[1]
            );
        }
    }
}
