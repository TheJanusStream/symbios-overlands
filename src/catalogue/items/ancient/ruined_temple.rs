//! Ruined temple — a peripteral colonnade of weathered marble columns on
//! a stepped stylobate, ringing a breached sandstone cella. Every column
//! is stochastically full, snapped or stumped, and the cella roof either
//! keeps a terracotta pediment or has collapsed to open rubble, so two
//! placements with different grammar seeds crumble differently — one a
//! near-intact temple, the other a stump-field reclaimed by moss.
//!
//! Footprint 14 × 24 — deep and narrow on the Greek long axis, so the
//! `Roof(Gable)` pediment ridges along Z and the tympanum faces the
//! front (−Z). Shape-grammar massing only: columns are entasis-tapered
//! piers (`Extrude` + `Taper`) standing between flat deck patches, not
//! round fluted shafts, and openings are recessed dark voids, not true
//! arches — the grammar cannot express those.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, SovereignGroundConfig, SovereignMaterialSettings,
    SovereignRockConfig, SovereignTextureConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{
    MARBLE_WHITE, SANDSTONE_GOLD, SANDSTONE_WEATHERED, STONE_VOID, TERRACOTTA, marble, sandstone,
    terracotta,
};

pub struct RuinedTemple;

impl CatalogueEntry for RuinedTemple {
    fn slug(&self) -> &'static str {
        "ruined_temple"
    }
    fn name(&self) -> &'static str {
        "Ruined Temple"
    }
    fn description(&self) -> &'static str {
        "Peripteral marble colonnade on a stepped stylobate around a breached sandstone cella, its terracotta pediment half-collapsed."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    /// Already decayed — fits the poorer end of the kit, never an affluent
    /// settlement's centrepiece.
    fn prosperity_band(&self) -> ProsperityBand {
        ProsperityBand::range(ProsperityTier::Poor, ProsperityTier::Modest)
    }

    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AncientClassical]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 14.5,
            min_spawn_dist: 45.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        // Centred foundation root + corner-origin 14×24 grammar child
        // offset by -footprint/2 (see the villa for the rationale). The
        // temple runs deep (Z) so the pedimented short end faces the
        // front (−Z).
        let mut root = crate::catalogue::items::util::foundation_block(15.0, 25.0, [0.0, 0.0], 2.5);
        let mut temple = Generator::from_kind(build_kind());
        temple.transform.translation = crate::pds::Fp3([-7.0, 0.0, -12.0]);
        root.children.push(temple);
        root
    }
}

fn build_kind() -> GeneratorKind {
    let mut materials = HashMap::new();

    // Veined white marble — the colonnade shafts and pediment tympanum.
    materials.insert("Marble".to_string(), marble(MARBLE_WHITE));
    // Coursed sandstone ashlar — the cella core walls.
    materials.insert("Sandstone".to_string(), sandstone(SANDSTONE_GOLD));
    // Weathered sandstone — the stepped stylobate and deck paving.
    materials.insert("Travertine".to_string(), sandstone(SANDSTONE_WEATHERED));
    // Fired terracotta — the surviving roof tiles of the pediment.
    materials.insert("Tile".to_string(), terracotta(TERRACOTTA));

    // Tumbled rubble where the roof and entablature have collapsed.
    materials.insert(
        "Rubble".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.55, 0.51, 0.45]),
            roughness: Fp(0.95),
            uv_scale: Fp(2.5),
            texture: SovereignTextureConfig::Rock(SovereignRockConfig::default()),
            ..Default::default()
        },
    );
    // Deep shadow inside breached walls.
    materials.insert(
        "Dark".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3(STONE_VOID),
            roughness: Fp(1.0),
            ..Default::default()
        },
    );
    // Overgrowth creeping across the stylobate.
    materials.insert(
        "Moss".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.22, 0.34, 0.16]),
            roughness: Fp(0.95),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Ground(SovereignGroundConfig {
                color_dry: Fp3([0.30, 0.38, 0.18]),
                color_moist: Fp3([0.14, 0.26, 0.10]),
                macro_scale: Fp64(3.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    // Peripteral temple in pure box-and-roof grammar. Footprint 14 (X) ×
    // 24 (Z); front is −Z. A low stylobate carries a colonnade ring (flank
    // rows along Z, front/back rows along X) of stochastically-ruined
    // marble columns around a breached sandstone cella whose roof either
    // keeps a terracotta pediment or has collapsed to rubble.
    let grammar_source = [
        // ── 1. Plan: flank colonnades + front/rear porticoes around a cella.
        //    Built from the Lot footprint (X/Z in-plane, Y up) — never from a
        //    Comp(Faces){Top} face, whose local Z is the zero-size normal and
        //    whose "up" would fight the Roof op. ──
        "Lot --> Split(X) { 2.4: FlankRow | ~1: CoreStrip | 2.4: FlankRow }",
        "CoreStrip --> Split(Z) { 3.2: FrontRow | ~1: CellaStrip | 3.2: BackRow }",
        // ── 2. Colonnade rows: a column plot centred amid low stylobate pads ──
        "FlankRow --> Repeat(Z, 2.4) { ColBayZ }",
        "FrontRow --> Repeat(X, 2.3) { ColBayX }",
        "BackRow --> Repeat(X, 2.3) { ColBayX }",
        "ColBayX --> Split(X) { ~1: DeckPad | 0.78: ColSliceX }",
        "ColSliceX --> Split(Z) { ~1: DeckPad | 0.78: ColPlot | ~1: DeckPad }",
        "ColBayZ --> Split(Z) { ~1: DeckPad | 0.78: ColSliceZ }",
        "ColSliceZ --> Split(X) { ~1: DeckPad | 0.78: ColPlot | ~1: DeckPad }",
        // ── 3. Stylobate pads: low weathered paving, occasional moss ──
        "DeckPad --> Extrude(0.6) DeckTop",
        "DeckTop --> 78% StoneDeck | 22% MossDeck",
        "StoneDeck --> Mat(\"Travertine\") I(\"Step\")",
        "MossDeck --> Mat(\"Moss\") I(\"Moss\")",
        // ── 4. Columns: stochastic full / snapped / stumped, entasis taper ──
        "ColPlot --> 50% FullColumn | 32% BrokenColumn | 18% StumpColumn",
        "FullColumn --> Extrude(5.6) Taper(0.12) ColumnShaft",
        "BrokenColumn --> Extrude(3.1) Taper(0.1) ColumnShaft",
        "StumpColumn --> Extrude(1.1) Taper(0.08) ColumnShaft",
        "ColumnShaft --> Mat(\"Marble\") I(\"Column\")",
        // ── 5. Cella: stylobate base, ashlar walls with breaches, ruined roof ──
        "CellaStrip --> Extrude(4.6) Split(Y) { 0.6: CellaBase | ~1: CellaBody | 1.4: CellaCrown }",
        "CellaBase --> Mat(\"Travertine\") I(\"Base\")",
        "CellaBody --> Comp(Faces) { Side: CellaWall }",
        "CellaWall --> Repeat(X, 3.0) { WallBay }",
        "WallBay --> 45% AshlarWall | 22% PilasterBay | 33% BreachBay",
        "AshlarWall --> Extrude(0.35) Mat(\"Sandstone\") I(\"Wall\")",
        "PilasterBay --> Split(X) { ~2: AshlarWall | ~1: Pilaster | ~2: AshlarWall }",
        "Pilaster --> Extrude(0.55) Taper(0.1) Mat(\"Marble\") I(\"Pilaster\")",
        "BreachBay --> Split(Y) { 1.6: AshlarWall | ~1: Breach }",
        "Breach --> Extrude(0.12) Mat(\"Dark\") I(\"Breach\")",
        // ── 6. Cella crown: surviving terracotta pediment or collapsed rubble ──
        "CellaCrown --> 60% Pediment | 40% OpenRuin",
        "Pediment --> Roof(Gable, 24, 0.5) { Slope: TileSlope | GableEnd: Tympanum }",
        "Tympanum --> Mat(\"Marble\") I(\"Tympanum\")",
        "TileSlope --> Mat(\"Tile\") I(\"Tile\")",
        "OpenRuin --> Mat(\"Rubble\") I(\"Rubble\")",
    ]
    .join("\n");

    GeneratorKind::Shape {
        grammar_source,
        root_rule: "Lot".to_string(),
        footprint: Fp3([14.0, 0.0, 24.0]),
        // Seed chosen so the catalogue render keeps its pediment; in-game
        // each placement gets its own seed, so ~60% stay roofed and the
        // rest collapse to open rubble.
        seed: 7,
        materials,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::shape_grammar_test::assert_grammar_parses_and_derives;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = RuinedTemple.build("");
        sanitize_generator(&mut g);
        // The entry root is now the centred foundation plinth; the
        // grammar hangs beneath it as the first child.
        assert!(
            matches!(g.kind, GeneratorKind::Cuboid { solid: true, .. }),
            "{} root must be the solid foundation plinth",
            "temple"
        );
        let shape = &g.children[0];
        match &shape.kind {
            GeneratorKind::Shape {
                root_rule,
                materials,
                ..
            } => {
                assert_eq!(root_rule, "Lot");
                // Classical bar: marble colonnade, sandstone ashlar cella,
                // terracotta roof; rubble + moss carry the decay.
                for slot in ["Marble", "Sandstone", "Tile", "Rubble", "Dark", "Moss"] {
                    assert!(
                        materials.contains_key(slot),
                        "missing material slot: {slot}"
                    );
                }
            }
            other => panic!("temple root must remain Shape; got {other:?}"),
        }
    }

    #[test]
    fn grammar_parses_and_derives() {
        assert_grammar_parses_and_derives(build_kind(), "ruined_temple");
    }
}
