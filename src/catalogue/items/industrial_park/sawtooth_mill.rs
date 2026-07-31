//! Sawtooth mill — a long brick weaving shed under a run of north-lit
//! sawtooth roof teeth, with a pilastered elevation, a lean-to engine
//! house, and a tapered brick chimney that smokes over the estate.
//!
//! The sawtooth is the point: each tooth is a `Roof(Shed)` whose sloped
//! face carries the metal deck and whose vertical back wall is glazed —
//! the north light that let a mill work by daylight. That back face only
//! became addressable in `symbios-shape` 0.3 (the `Back` roof selector was
//! added for exactly this silhouette), and the teeth are sized by
//! `height=` rather than a pitch so the rise stays constant however deep
//! the bay is cut.
//!
//! Footprint 24 × 14 — long across X, so the ridges run the length of the
//! hall and the teeth repeat across its depth, the way a real weaving shed
//! is laid out.
//!
//! Stochastic variation (per placement, from the settlement's re-stamped
//! grammar seed): the bay mix along the elevation, whether the engine
//! house takes a gable or a roof tank, the chimney's cap band, and — via
//! one `Pick` key shared by the northlights and the elevation windows —
//! whether the whole mill is working (lit glazing) or idle (dark grille).
//! Because it is one key, the building never disagrees with itself.
//!
//! The chimney and the roof tank are listed in `round_meshes`, so they
//! bake as turned cylinders while the grammar still derives plain boxes.

use std::collections::HashMap;

use crate::catalogue::items::util::{attach, footing};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind, SovereignMaterialSettings};
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRICK_DARK, CONCRETE_GREY, INDUSTRIAL_BAND, LAMP_AMBER, PIPE_GREY, STEEL_BLUE, brick, cladding,
    concrete, fx, glass, tank_steel,
};

/// Unlit mill glazing — cool, grimy, and dark enough to read as glass
/// against brick rather than as a pale panel.
const GLAZING_IDLE: [f32; 3] = [0.26, 0.32, 0.34];

/// Footprint of the grammar plot, in world units.
const LOT_X: f32 = 24.0;
const LOT_Z: f32 = 14.0;
/// Depth of the end block taken off the hall along X, and the slice of it
/// given over to the chimney yard along Z. The grammar splits on these
/// same numbers; keeping them here is what lets the smoke emitter find
/// the flue.
const END_BLOCK_X: f32 = 5.5;
const STACK_YARD_Z: f32 = 5.2;
/// Height of the chimney above the mill's base, used to place the smoke
/// plume. Kept deterministic so the emitter always sits at the flue.
const STACK_TOP: f32 = 18.6;

pub struct SawtoothMill;

impl CatalogueEntry for SawtoothMill {
    fn slug(&self) -> &'static str {
        "sawtooth_mill"
    }
    fn name(&self) -> &'static str {
        "Sawtooth Mill"
    }
    fn description(&self) -> &'static str {
        "Brick weaving shed under north-lit sawtooth roof teeth, with an engine house and smoking chimney."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// The working estate's production hall — the established register.
    /// The derelict end of the theme is the separate [`super::derelict_shed`].
    fn prosperity_band(&self) -> ProsperityBand {
        INDUSTRIAL_BAND
    }

    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 10.5,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        // Centred foundation plinth is the root; the corner-origin grammar
        // hangs beneath it offset by -footprint/2, so placement yaw turns
        // the hall about its middle and the dry-land clearance ring
        // measures from the true centre (the villa documents the idiom).
        let mut root = footing(LOT_X + 1.0, LOT_Z + 1.0, [0.0, 0.0], 10.5);
        let mut mill = Generator::from_kind(build_kind());
        mill.transform.translation = Fp3([-LOT_X / 2.0, 0.0, -LOT_Z / 2.0]);
        root.children.push(mill);

        // Signature life: the flue smokes over the estate's hum. The stack
        // is centred in the far corner plot the grammar carves for it —
        // the last `END_BLOCK_X` of the length, and within that the last
        // `STACK_YARD_Z` of the depth. Its height is fixed (not
        // stochastic) precisely so this plume always lands on the flue
        // rather than floating or sinking into brick.
        attach(
            &mut root,
            fx::stack_smoke(
                [
                    LOT_X / 2.0 - END_BLOCK_X / 2.0,
                    STACK_TOP,
                    LOT_Z / 2.0 - STACK_YARD_Z / 2.0,
                ],
                0x5A57_0074,
            ),
        );
        root.audio = fx::machine_hum();
        root
    }
}

/// The kit's palette, keyed by the `Mat("...")` names the grammar emits.
fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = HashMap::new();
    m.insert("Brick".to_string(), brick(BRICK_DARK));
    m.insert("Concrete".to_string(), concrete(CONCRETE_GREY));
    m.insert("Metal".to_string(), cladding(STEEL_BLUE));
    m.insert("Steel".to_string(), tank_steel(PIPE_GREY));
    // Two glazing states the whole mill agrees on — see the `Pick("shift")`
    // key below. Idle glazing is a cool grimy blue-grey: the kit's warm
    // `WINDOW_LIT` at zero emission reads as cream board on a surface this
    // broad, not as glass.
    m.insert("Glass".to_string(), glass(GLAZING_IDLE, 0.0));
    m.insert("LitGlass".to_string(), glass(LAMP_AMBER, 2.4));
    m
}

fn build_kind() -> GeneratorKind {
    let grammar_source = [
        // ── Declarations — the knobs an author (or the editor) can turn ──
        "const PlinthH = 0.7",
        "const ToothDepth = 3.4",
        "attr HallHeight = 7.2",
        "attr ToothRise = 1.7",
        // ── 1. Massing: the long hall, and the engine-house end block ──
        "Lot --> Split(X) { ~1: MillHall | 5.5: EndBlock }",
        "MillHall --> Extrude(HallHeight) Split(Y) { PlinthH: Plinth | ~1: HallBody | 2.4: SawRoof }",
        // ── 2. The sawtooth: ridges run along X, teeth repeat across Z ──
        //    `height=` fixes the rise so a deeper tooth does not become a
        //    cathedral; the vertical back wall is the north light.
        "SawRoof --> Repeat(Z, ToothDepth) { Tooth }",
        "Tooth --> Roof(Shed, height=ToothRise, overhang=0.18) { Slope: RoofDeck | Back: NorthLight | _: BrickWall }",
        "RoofDeck --> Mat(\"Metal\") I(\"Roof\")",
        "NorthLight --> Split(X) { 0.3: Mullion | ~1: Glazing | 0.3: Mullion }",
        "Mullion --> Extrude(0.12) Mat(\"Steel\") I(\"Mullion\")",
        // ── 3. Elevation: pilastered bays that stand down when narrow ──
        "HallBody --> Comp(Faces) { Side: MillFacade | Top: NIL | Bottom: NIL }",
        // The bay carries an ABSOLUTE width inside the rhythm group. A
        // floating `~1` here would be read as a nominal 1 m when the group
        // is tiled, packing eleven 1.05 m bays into the elevation — every
        // one too narrow for an opening, so `Fit` would quietly degrade
        // the whole hall to blank brick.
        "MillFacade --> when(scope.x < 5): BrickWall \
                        | else: Split(X) { 0.6: Pilaster | { 0.55: Pilaster | 2.3: BayField }* | 0.6: Pilaster }",
        "Pilaster --> Extrude(0.24) Mat(\"Brick\") I(\"Pilaster\")",
        // Fit still earns its place: a bay squeezed below opening width by
        // the group's stretch falls back to brick instead of cramming.
        "BayField --> Fit(X) { 1.4: OpeningBay | 0: BrickWall }",
        "OpeningBay --> 18% DoorBay | 82% WindowBay",
        "DoorBay --> Split(Y) { ~1: RollerDoor | 1.3: BrickWall }",
        "RollerDoor --> Extrude(0.14) Mat(\"Steel\") I(\"Door\")",
        "WindowBay --> Split(Y) { 1.0: BrickWall | ~1: Glazing | 0.8: BrickWall }",
        // ── 4. One glazing decision for the whole mill ──
        //    Same key in the northlights and the elevation, so a working
        //    shift lights every opening and an idle one darkens them all.
        "Glazing --> Pick(\"shift\") { 55% DarkGlass | 45% WorkingGlass }",
        "DarkGlass --> Extrude(0.1) Mat(\"Glass\") I(\"Pane\")",
        "WorkingGlass --> Extrude(0.1) Mat(\"LitGlass\") I(\"Pane\")",
        // ── 5. End block: engine house, then the chimney yard ──
        "EndBlock --> Split(Z) { ~1: EngineHouse | 5.2: StackYard }",
        "EngineHouse --> Extrude(5.4) Split(Y) { PlinthH: Plinth | ~1: EngineBody | 1.5: EngineTop }",
        "EngineBody --> Comp(Faces) { Side: EngineFacade | Top: NIL | Bottom: NIL }",
        "EngineFacade --> when(scope.x < 3.2): BrickWall | else: Repeat(X, 2.5) { WindowBay }",
        // Either a plain gabled cap, or a squat water tank on the deck.
        "EngineTop --> 60% Roof(Gable, 30, 0.3) { Slope: RoofDeck | GableEnd: BrickWall } | 40% TankDeck",
        "TankDeck --> Size(2.4, 0, 2.4) Center(XZ) TankBody",
        "TankBody --> Extrude(2.3) Split(Y) { ~1: TankDrum | 0.3: TankLid }",
        "TankDrum --> Mat(\"Steel\") I(\"Tank\")",
        "TankLid --> Mat(\"Metal\") I(\"Tank\")",
        // ── 6. The chimney: a square plot shrunk to the flue and centred ──
        "StackYard --> Size(2.3, 0, 2.3) Center(XZ) StackShaft",
        "StackShaft --> Extrude(17.9) Split(Y) { 1.1: StackFoot | ~1: Flue | 0.9: StackCap }",
        "StackFoot --> Mat(\"Brick\") I(\"StackFoot\")",
        // Entasis on a mill chimney: the flue narrows as it climbs.
        "Flue --> Taper(0.28) Mat(\"Brick\") I(\"Stack\")",
        "StackCap --> 65% SteelBand | 35% BrickBand",
        "SteelBand --> Mat(\"Steel\") I(\"StackBand\")",
        "BrickBand --> Mat(\"Brick\") I(\"StackBand\")",
        // ── 7. Shared terminals ──
        "Plinth --> Comp(Faces) { Side: PlinthFace | Top: NIL | Bottom: NIL }",
        "PlinthFace --> Extrude(0.26) Mat(\"Concrete\") I(\"Plinth\")",
        "BrickWall --> Extrude(0.34) Mat(\"Brick\") I(\"Wall\")",
    ]
    .join("\n");

    GeneratorKind::Shape {
        grammar_source,
        root_rule: "Lot".to_string(),
        footprint: Fp3([LOT_X, 0.0, LOT_Z]),
        seed: 11,
        materials: materials(),
        // The flue, its foot and cap band, and the roof tank are turned;
        // everything else on a mill is square-plan brickwork.
        round_meshes: vec![
            "Stack".to_string(),
            "StackFoot".to_string(),
            "StackBand".to_string(),
            "Tank".to_string(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::shape_grammar_test::assert_grammar_parses_and_derives;
    use crate::pds::sanitize_generator;

    #[test]
    fn grammar_parses_and_derives() {
        assert_grammar_parses_and_derives(build_kind(), "sawtooth_mill");
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = SawtoothMill.build("");
        sanitize_generator(&mut g);
        assert!(
            matches!(g.kind, GeneratorKind::Cuboid { solid: true, .. }),
            "sawtooth_mill root must be the solid foundation plinth"
        );
        let shape = &g.children[0];
        let GeneratorKind::Shape {
            root_rule,
            materials,
            round_meshes,
            ..
        } = &shape.kind
        else {
            panic!("mill body must remain Shape after sanitise");
        };
        assert_eq!(root_rule, "Lot");
        for slot in ["Brick", "Concrete", "Metal", "Steel", "Glass", "LitGlass"] {
            assert!(
                materials.contains_key(slot),
                "missing material slot: {slot}"
            );
        }
        assert!(
            round_meshes.contains(&"Stack".to_string()),
            "the chimney must survive sanitisation as a turned terminal"
        );
    }

    /// The sawtooth is the item's reason to exist: every tooth must place
    /// a glazed back wall (the north light) alongside its metal deck. A
    /// regression in the upstream `Back` roof selector would silently
    /// leave the roof a plain run of sheds.
    /// Derives the mill at `seed` through the same statement path the
    /// runtime uses.
    fn derive_mill(seed: u64) -> symbios_shape::ShapeModel {
        use symbios_shape::grammar::parse_statement;
        use symbios_shape::{Interpreter, Quat as SQuat, Scope, Vec3 as SVec3};

        let GeneratorKind::Shape {
            grammar_source,
            root_rule,
            footprint,
            ..
        } = build_kind()
        else {
            panic!("build_kind must return Shape");
        };
        let mut interp = Interpreter::new();
        for line in grammar_source.lines() {
            interp
                .add_statement(parse_statement(line).expect("statement parses"))
                .expect("statement accepted");
        }
        interp.seed = seed;
        interp
            .derive(
                Scope::new(
                    SVec3::ZERO,
                    SQuat::IDENTITY,
                    SVec3::new(
                        footprint.0[0] as f64,
                        footprint.0[1] as f64,
                        footprint.0[2] as f64,
                    ),
                ),
                &root_rule,
            )
            .expect("mill derives")
    }

    #[test]
    fn every_tooth_gets_a_northlight() {
        let model = derive_mill(11);
        let decks = model
            .terminals
            .iter()
            .filter(|t| t.mesh_id == "Roof")
            .count();
        let panes = model
            .terminals
            .iter()
            .filter(|t| t.mesh_id == "Pane")
            .count();
        assert!(decks >= 3, "expected a run of sawtooth decks, got {decks}");
        assert!(
            panes >= decks,
            "every tooth should carry a glazed north light: {decks} decks vs {panes} panes"
        );
    }

    /// The hall's elevation must actually be pierced. An earlier draft
    /// sized the rhythm group's bay with a floating `~1`, which tiles as a
    /// nominal 1 m — every bay then fell under `Fit`'s opening threshold
    /// and the whole hall silently rendered as blank brick. Nothing else
    /// in the suite noticed, because the grammar still derived cleanly.
    #[test]
    fn the_hall_elevation_is_pierced_not_blank() {
        let model = derive_mill(3);
        let openings = model
            .terminals
            .iter()
            .filter(|t| t.mesh_id == "Pane" || t.mesh_id == "Door")
            .count();
        let pilasters = model
            .terminals
            .iter()
            .filter(|t| t.mesh_id == "Pilaster")
            .count();
        assert!(
            pilasters >= 8,
            "expected a pilastered rhythm, got {pilasters} pilasters"
        );
        // Well beyond what the engine house alone contributes.
        assert!(
            openings >= 12,
            "the hall elevation degraded to blank brick: only {openings} openings"
        );
    }

    /// `Pick` is what keeps the mill from disagreeing with itself: one
    /// shift decision must light (or darken) every opening in the
    /// building, for any seed.
    #[test]
    fn glazing_is_coherent_across_the_whole_mill() {
        for seed in 0..8_u64 {
            let model = derive_mill(seed);
            let mats: std::collections::HashSet<&str> = model
                .terminals
                .iter()
                .filter(|t| t.mesh_id == "Pane")
                .filter_map(|t| t.material.as_ref().map(|m| m.id.as_str()))
                .collect();
            assert!(
                mats.len() <= 1,
                "seed {seed}: the mill mixed glazing states {mats:?} — Pick lost coherence"
            );
        }
    }
}
