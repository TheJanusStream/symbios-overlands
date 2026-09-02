//! Rowhouse terrace — a run of five brownstones sharing party walls, each
//! with its own cladding, height and entrance, all under one cornice line.
//!
//! The item exists to show a terrace behaving the way a real one does: it
//! was **developed as a single scheme**, so the roofline is common to every
//! house, but the houses were **fitted out individually**, so cladding,
//! storey count, entrance type and window rhythm all differ door to door.
//! The grammar gets that from two different mechanisms working against
//! each other:
//!
//! - `Pick("terrace")` resolves **once per derivation**, so the cornice /
//!   parapet / mansard decision is shared by the whole row.
//! - Ordinary weighted rules resolve **per shape**, so each house draws its
//!   own cladding and height from its own seed stream.
//!
//! Cladding rides on `Mat` inheritance: the per-house roll stamps a
//! material, and every wall, parapet and reveal below it that does *not*
//! name its own material inherits it. Only the trim (cornice stone, slate,
//! glazing, ironwork) overrides, so a house is one consistent colour from
//! stoop to roofline without any of those rules knowing which house they
//! are in.
//!
//! Footprint 22 × 11 — a shallow city block frontage. `Repeat` cycles three
//! bay widths so the party walls do not land on a metronome.

use std::collections::HashMap;

use crate::catalogue::items::util::footing;
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, SovereignMaterialSettings, SovereignTextureConfig,
    SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{BRICK_RED, CITY_BAND, LAMP_WARM, STEEL_GREY, brick, concrete, steel};

/// Cool neutral glazing for the sash card. Never tinted warm — see
/// [`sash_glass`].
const SASH_GLASS: [f32; 3] = [0.42, 0.48, 0.54];
/// The unlit interior seen through a dark window.
const ROOM_UNLIT: [f32; 3] = [0.07, 0.08, 0.10];

/// A domestic sash, rather than the kit's [`super::glass`].
///
/// Two departures from the shared helper, both deliberate:
///
/// 1. **Coarser grid.** The kit's glass is tuned for curtain walls — a
///    4 × 5 grid of twenty panes, which on a 1.2 m house window reads as
///    an office elevation shrunk down. A sash is a handful of large lights.
/// 2. **No bright tint, no emission.** A `Window` card is a single
///    material across frame *and* glass, so tinting it warm to suggest a
///    lit room lights the joinery too, and the frame reads as glowing
///    plastic rather than painted timber. The card therefore stays a cool
///    neutral, and the light comes from a separate emissive surface set
///    behind it (see the `LitWindow` rule) — which is also what makes the
///    masked panes read as an opening rather than a frame over solid wall.
///
/// Local to this item on purpose: changing the kit helper would restyle
/// the tower and the office block too.
fn sash_glass() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(SASH_GLASS),
        roughness: Fp(0.15),
        metallic: Fp(0.4),
        // Window is an alpha card: it must span its quad exactly once, so
        // `uv_scale` stays 1.0 and the shape spawner stretches its UVs.
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 2,
            panes_y: 3,
            glass_opacity: Fp64(0.42),
            grime_level: Fp64(0.1),
            color_frame: Fp3([0.16, 0.17, 0.20]),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// A plain emissive surface — no `Window` texture — set behind the sash to
/// be the lit room. Flat, unpatterned and warm: all the character comes
/// from the card in front of it.
fn room_light(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.9),
        ..Default::default()
    }
}

/// Footprint of the grammar plot, in world units.
const LOT_X: f32 = 22.0;
const LOT_Z: f32 = 11.0;

/// Second cladding brick — a browner stock alongside the kit's red.
const BRICK_BROWN: [f32; 3] = [0.38, 0.27, 0.21];
/// Painted render on the houses that were stuccoed over.
const STUCCO_CREAM: [f32; 3] = [0.74, 0.71, 0.65];
/// Pale dressed stone — cornices, stoops, sills.
const TRIM_STONE: [f32; 3] = [0.80, 0.78, 0.74];
/// Slate roofing and the tarred flat decks behind a parapet.
const SLATE_GREY: [f32; 3] = [0.27, 0.29, 0.33];
const DECK_TAR: [f32; 3] = [0.20, 0.20, 0.21];
/// Painted front doors — the one saturated note on a brick street.
const DOOR_GREEN: [f32; 3] = [0.16, 0.30, 0.24];

pub struct RowhouseTerrace;

impl CatalogueEntry for RowhouseTerrace {
    fn slug(&self) -> &'static str {
        "rowhouse_terrace"
    }
    fn name(&self) -> &'static str {
        "Rowhouse Terrace"
    }
    fn description(&self) -> &'static str {
        "Run of brownstones sharing party walls, each with its own cladding and stoop, under one cornice."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Residential street frontage for the established downtown — the
    /// destitute end of the theme is the separate [`super::tenement`].
    fn prosperity_band(&self) -> ProsperityBand {
        CITY_BAND
    }

    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 10.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        // Centred foundation plinth is the root; the corner-origin grammar
        // hangs beneath it offset by -footprint/2 (the villa documents the
        // idiom) so placement yaw turns the row about its middle.
        let mut root = footing(LOT_X + 1.0, LOT_Z + 1.0, [0.0, 0.0], 10.0);
        let mut terrace = Generator::from_kind(build_kind());
        terrace.transform.translation = Fp3([-LOT_X / 2.0, 0.0, -LOT_Z / 2.0]);
        // `attach` (not a bare push): `footing` returns a root whose own
        // transform is sunk by half the buried plinth, and a plain child
        // inherits it — which drops the whole building below grade (#1039).
        crate::catalogue::items::util::attach(&mut root, terrace);
        root
    }
}

/// The palette, keyed by the `Mat("...")` names the grammar emits. The four
/// cladding slots are what the per-house lottery chooses between.
fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = HashMap::new();
    // Cladding — one of these is stamped per house and inherited downward.
    m.insert("BrickRed".to_string(), brick(BRICK_RED));
    m.insert("BrickBrown".to_string(), brick(BRICK_BROWN));
    m.insert("Stucco".to_string(), concrete(STUCCO_CREAM));
    m.insert("StoneFace".to_string(), concrete(TRIM_STONE));
    // Trim — always named explicitly, so it never inherits the cladding.
    m.insert("Trim".to_string(), concrete(TRIM_STONE));
    m.insert("Slate".to_string(), concrete(SLATE_GREY));
    m.insert("Deck".to_string(), concrete(DECK_TAR));
    m.insert("Iron".to_string(), steel(STEEL_GREY));
    m.insert("Door".to_string(), concrete(DOOR_GREEN));
    // Glazing is two surfaces, not one: a neutral `Window` card on a flat
    // quad, and a plain emissive (or dark) room behind it. See `sash_glass`
    // for why the card is never tinted warm, and #972 lesson 1 for why a
    // card belongs on a plane with something real behind it.
    m.insert("Sash".to_string(), sash_glass());
    m.insert("RoomLit".to_string(), room_light(LAMP_WARM, 2.4));
    m.insert("RoomUnlit".to_string(), room_light(ROOM_UNLIT, 0.0));
    m
}

fn build_kind() -> GeneratorKind {
    let grammar_source = [
        // ── Declarations ──
        "const FloorH = 3.1",
        // Wall thickness, shared by the wall slabs and the window reveals
        // so an opening is exactly as deep as the wall it pierces.
        "const WallD = 0.3",
        "const GroundH = 3.4",
        "attr Storeys = 4",
        // ── 1. The row: cycled frontages so party walls avoid a metronome ──
        "Lot --> Repeat(X, [4.4, 3.9, 4.8]) { House }",
        // ── 2. Per-house identity ──
        //    Two independent draws, each from the house's own seed stream.
        //    `Mat` here propagates to every terminal below that does not
        //    name its own material — that is the whole cladding mechanism.
        "House --> 36% Mat(\"BrickRed\") Storeyed \
                   | 26% Mat(\"BrickBrown\") Storeyed \
                   | 22% Mat(\"Stucco\") Storeyed \
                   | 16% Mat(\"StoneFace\") Storeyed",
        "Storeyed --> 30% Extrude(GroundH + FloorH * 2) Massing \
                      | 44% Extrude(GroundH + FloorH * Storeys) Massing \
                      | 26% Extrude(GroundH + FloorH * 3) Massing",
        "Massing --> Split(Y) { GroundH: GroundFloor | ~1: UpperFloors | 1.2: RoofZone }",
        // ── 3. Ground floor: stoop-and-door, or a flush entrance ──
        "GroundFloor --> Comp(Faces) { Front: EntryFace | Back: RearFace | Top: NIL | Bottom: NIL | _: PartyWall }",
        "EntryFace --> when(scope.x < 3.2): FlushEntry | else: EntryChoice",
        "EntryChoice --> 66% StoopEntry | 34% FlushEntry",
        "StoopEntry --> Split(X) { ~1: GroundWindow | 1.6: StoopBay }",
        "StoopBay --> Split(Y) { 0.5: StoopStep | 2.2: DoorLeaf | ~1: Wall }",
        "StoopStep --> Extrude(0.6) Mat(\"Trim\") I(\"Stoop\")",
        "FlushEntry --> Split(X) { ~1: GroundWindow | 1.4: FlatBay }",
        "FlatBay --> Split(Y) { 2.4: DoorLeaf | ~1: Wall }",
        "DoorLeaf --> Extrude(0.16) Mat(\"Door\") I(\"Door\")",
        "GroundWindow --> when(scope.x < 1.4): Wall | else: GroundWindowBay",
        "GroundWindowBay --> Split(X) { 0.35: Wall | ~1: TallSash | 0.35: Wall }",
        "TallSash --> Split(Y) { 0.5: Wall | ~1: Glazing | 0.55: Lintel }",
        // ── 4. Upper floors: BANDS first, then bays ──
        //    Without the vertical `Repeat(Y)` a taller house would stretch
        //    one row of windows over its whole elevation.
        "UpperFloors --> Comp(Faces) { Front: UpperFace | Back: RearFace | Top: NIL | Bottom: NIL | _: PartyWall }",
        "UpperFace --> Repeat(Y, FloorH) { FloorBand }",
        "FloorBand --> when(scope.x < 2.2): Wall \
                       | else: Split(X) { 0.4: Wall | { 0.3: Wall | 1.2: Bay }* | 0.4: Wall }",
        // Absolute bay width inside the group: a floating slot here would
        // tile at its weight-as-length and pack the elevation with slivers.
        "Bay --> 64% SashBay | 17% BalconyBay | 19% Wall",
        "SashBay --> Split(Y) { 0.7: Wall | ~1: Glazing | 0.55: Lintel }",
        "BalconyBay --> Split(Y) { 0.55: BalconyRail | ~1: Glazing | 0.55: Lintel }",
        "BalconyRail --> Extrude(0.5) Mat(\"Iron\") I(\"Rail\")",
        // Lit and dark windows mix along a street — this is deliberately a
        // per-window roll, not a `Pick`.
        "Glazing --> 46% LitWindow | 54% DarkWindow",
        // The opening is cut to the wall's own depth, so the sash card sits
        // flush in the reveal with the room surface at the shell plane
        // behind it. `Back` is the outer face of that little box and
        // `Front` the inner one.
        "LitWindow --> Extrude(WallD) Comp(Faces) { Back: SashCard | Front: LitRoom | _: RevealFace }",
        "DarkWindow --> Extrude(WallD) Comp(Faces) { Back: SashCard | Front: UnlitRoom | _: RevealFace }",
        "SashCard --> Mat(\"Sash\") I(\"Pane\")",
        "LitRoom --> Mat(\"RoomLit\") I(\"Room\")",
        "UnlitRoom --> Mat(\"RoomUnlit\") I(\"Room\")",
        "RevealFace --> Mat(\"Trim\") I(\"Reveal\")",
        "Lintel --> Extrude(0.14) Mat(\"Trim\") I(\"Lintel\")",
        // ── 5. Roofline: one scheme for the whole terrace ──
        //    `Pick` resolves once per derivation, so every house in the row
        //    gets the same treatment however different their fronts are.
        "RoofZone --> Pick(\"terrace\") { 42% CorniceRun | 30% ParapetRun | 28% MansardRun }",
        "CorniceRun --> Comp(Faces) { Side: CorniceFace | Top: RoofDeck | Bottom: NIL }",
        "CorniceFace --> Extrude(0.34) Mat(\"Trim\") I(\"Cornice\")",
        // The parapet names no material, so it inherits the house cladding
        // and reads as the brickwork simply continuing past the gutter.
        "ParapetRun --> Comp(Faces) { Side: ParapetFace | Top: RoofDeck | Bottom: NIL }",
        "ParapetFace --> Extrude(0.2) I(\"Parapet\")",
        "MansardRun --> Roof(Mansard, 72, secondary=26, tier=0.62) { LowerSlope: SlateFace | UpperSlope: SlateFace | _: Wall }",
        "SlateFace --> Mat(\"Slate\") I(\"Slate\")",
        "RoofDeck --> Mat(\"Deck\") I(\"Deck\")",
        // ── 6. Shared terminals ──
        //    None of these name a material: they inherit the house's.
        "Wall --> Extrude(WallD) I(\"Wall\")",
        "PartyWall --> Extrude(0.26) I(\"Wall\")",
        "RearFace --> when(scope.x < 2.2): Wall | else: Repeat(X, 2.0) { RearBay }",
        "RearBay --> 55% SashBay | 45% Wall",
    ]
    .join("\n");

    GeneratorKind::Shape {
        grammar_source,
        root_rule: "Lot".to_string(),
        footprint: Fp3([LOT_X, 0.0, LOT_Z]),
        seed: 23,
        materials: materials(),
        // A terrace is square-plan throughout — nothing is turned.
        round_meshes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::shape_grammar_test::assert_grammar_parses_and_derives;
    use crate::pds::PrimCommon;
    use crate::pds::sanitize_generator;
    use symbios_shape::ShapeModel;

    /// Derives the terrace at `seed` through the same statement path the
    /// runtime uses.
    fn derive_terrace(seed: u64) -> ShapeModel {
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
            .expect("terrace derives")
    }

    /// Material ids stamped on terminals with the given mesh id.
    fn mats_of<'a>(model: &'a ShapeModel, mesh: &str) -> Vec<&'a str> {
        model
            .terminals
            .iter()
            .filter(|t| t.mesh_id == mesh)
            .filter_map(|t| t.material.as_ref().map(|m| m.id.as_str()))
            .collect()
    }

    #[test]
    fn grammar_parses_and_derives() {
        assert_grammar_parses_and_derives(build_kind(), "rowhouse_terrace");
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = RowhouseTerrace.build("");
        sanitize_generator(&mut g);
        assert!(
            matches!(
                g.kind,
                GeneratorKind::Cuboid {
                    common: PrimCommon { solid: true, .. },
                    ..
                }
            ),
            "rowhouse_terrace root must be the solid foundation plinth"
        );
        let GeneratorKind::Shape {
            root_rule,
            materials,
            ..
        } = &g.children[0].kind
        else {
            panic!("terrace body must remain Shape after sanitise");
        };
        assert_eq!(root_rule, "Lot");
        for slot in [
            "BrickRed",
            "BrickBrown",
            "Stucco",
            "StoneFace",
            "Trim",
            "Slate",
            "Iron",
            "Door",
            "Sash",
            "RoomLit",
            "RoomUnlit",
        ] {
            assert!(
                materials.contains_key(slot),
                "missing material slot: {slot}"
            );
        }
    }

    /// The elevation must be pierced, and banded vertically — a taller
    /// house has to gain window rows, not stretch one. Guards the
    /// rhythm-group sizing that silently blanked the mill's hall.
    #[test]
    fn the_terrace_front_is_pierced_and_banded() {
        let model = derive_terrace(23);
        let panes = model
            .terminals
            .iter()
            .filter(|t| t.mesh_id == "Pane")
            .count();
        let lintels = model
            .terminals
            .iter()
            .filter(|t| t.mesh_id == "Lintel")
            .count();
        assert!(
            panes >= 20,
            "the terrace elevation degraded to blank wall: only {panes} panes"
        );
        assert!(lintels >= panes / 2, "windows lost their lintel bands");

        // Several distinct pane heights ⇒ several floor bands, i.e. the
        // vertical `Repeat(Y)` really is subdividing the elevation.
        let mut rows: Vec<i64> = model
            .terminals
            .iter()
            .filter(|t| t.mesh_id == "Pane")
            .map(|t| (t.scope.position.y * 10.0).round() as i64)
            .collect();
        rows.sort_unstable();
        rows.dedup();
        assert!(
            rows.len() >= 3,
            "expected several floor bands of windows, found {} distinct rows",
            rows.len()
        );
    }

    /// Cladding is per house and inherited: a row must show more than one
    /// cladding material, and every wall terminal must have inherited one
    /// (none may fall through to the spawner's default).
    #[test]
    fn each_house_claims_its_own_cladding() {
        let mut seen_multi = false;
        for seed in 0..10_u64 {
            let model = derive_terrace(seed);
            let walls = mats_of(&model, "Wall");
            assert!(!walls.is_empty(), "seed {seed}: no wall terminals at all");
            assert!(
                walls
                    .iter()
                    .all(|m| ["BrickRed", "BrickBrown", "Stucco", "StoneFace"].contains(m)),
                "seed {seed}: a wall inherited something that is not cladding: {walls:?}"
            );
            let distinct: std::collections::HashSet<&&str> = walls.iter().collect();
            if distinct.len() > 1 {
                seen_multi = true;
            }
        }
        assert!(
            seen_multi,
            "no seed produced a row of mixed cladding — the per-house lottery is not firing"
        );
    }

    /// `Pick` is the counterweight: however varied the houses are, the
    /// roofline is one decision for the whole row. Exactly one of the three
    /// treatments may appear in any single derivation.
    #[test]
    fn the_roofline_is_shared_by_the_whole_row() {
        for seed in 0..10_u64 {
            let model = derive_terrace(seed);
            let cornices = model
                .terminals
                .iter()
                .filter(|t| t.mesh_id == "Cornice")
                .count();
            let parapets = model
                .terminals
                .iter()
                .filter(|t| t.mesh_id == "Parapet")
                .count();
            let slates = model
                .terminals
                .iter()
                .filter(|t| t.mesh_id == "Slate")
                .count();
            let present = [cornices, parapets, slates]
                .iter()
                .filter(|n| **n > 0)
                .count();
            assert_eq!(
                present, 1,
                "seed {seed}: the row mixed rooflines \
                 (cornice {cornices}, parapet {parapets}, slate {slates}) — Pick lost coherence"
            );
        }
    }
}
