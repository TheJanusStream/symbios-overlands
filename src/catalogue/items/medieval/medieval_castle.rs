//! Procedural medieval castle - courtyard-based layout with corner
//! towers, gatehouse, cloistered wings, and a great keep (sometimes
//! ruined). Adapted from `bevy_symbios_shape`'s `medieval_castle`
//! example.
//!
//! Heavy use of stochastic alternatives (`weight%` syntax) means each
//! place dropped into a room generates a slightly different castle -
//! tower heights vary, some get spires vs battlements, the keep may
//! be intact or ruined, walls intersperse arrow-slits among solid
//! sections. Variation is driven by the per-generator `seed`; the
//! catalogue ships with a fixed seed for predictable starter results,
//! but the user can re-roll by changing it in the editor.
//!
//! Every mass's facade stands inside the mass (#972): a face's walls are
//! shifted in by their own depth, so their outer faces lie on the mass's
//! plane, and the front and back faces stop that depth short of the
//! corners, which the side faces' walls turn. The masses meet in shared
//! planes - a tower against a curtain wall, a wing against the keep - and
//! while every wall grew outward, each mass's walls grew into its
//! neighbour's and drew faces in one place with them, mostly inside the
//! castle. With each window's side faces, which lay on the walls beside it
//! (those walls already make the reveal, and the faces are gone), that came
//! to 635 pairs and 916 m2 at the catalogue's seed, the most in the
//! catalogue (#1440). Of what protrudes past a mass's plane, nothing is
//! drawn in one plane with another face: a balcony deck sits at heights no
//! other split uses and short of its bay's edges, so where it runs into a
//! neighbouring mass it is buried in its wall; each wing's ends are plain
//! walls against the front wall and the keep; a wing's pavilion roofs stop
//! their overhang short at their ends, so neighbours meet edge to edge,
//! and the cloister vaults alternate pitch, so crossing overhangs meet at
//! an angle. A gable end is a flat face of its own: under the wall rule it
//! was extruded, which draws a triangle flat at the middle of its
//! extrusion, and a Dutch gable's floated 0.16 m off its hip (#1571).
//!
//! The courtyard lawn and the ruin's rubble floor stand on the footing,
//! whose top is 15 cm above the grammar's ground (they were drawn under
//! it); the lower west wing is a metre taller, since its 5 m body was too
//! short for a storey and drew no long walls at all; tower tops, wall
//! walks and the gatehouse are decked; a corbel band projects past the
//! wall below it; each pane sits back in its reveal over a wooden sill;
//! and the ruin, a roofless shell, has empty dark openings rather than lit
//! windows. Walls, roofs, walks, gates, doors, balconies and panes are
//! solid (#1572); arches, arrow slits and the gate passage are open, so a
//! visitor can walk in through the gate.
//!
//! The texture refresh (#972): the walls are coursed rubble (DryStone)
//! rather than an ashlar grid, and the ruin's floor is coarse rubble
//! (Gravel). RoofTile was judged and left: its clay barrel tiles and
//! pantiles are a southern roof, and these spires read as slate.
use std::collections::HashMap;

use crate::catalogue::items::util::{tile, tiles_per_metre};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, SovereignDryStoneConfig, SovereignGravelConfig,
    SovereignGroundConfig, SovereignMaterialSettings, SovereignPlankConfig, SovereignShingleConfig,
    SovereignTextureConfig, SovereignWindowConfig,
};
use crate::seeded_defaults::ThemeArchetype;

pub struct MedievalCastle;

impl CatalogueEntry for MedievalCastle {
    fn slug(&self) -> &'static str {
        "medieval_castle"
    }
    fn name(&self) -> &'static str {
        "Medieval Castle"
    }
    fn description(&self) -> &'static str {
        "Courtyard castle with corner towers, gatehouse, cloistered wings, and a great keep."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    /// The burgh's seat of power - the established town landmark, shared
    /// across the Modest-to-Rich band (the landmark scale shrinks the keep
    /// for a modest town). The destitute end grows the [`super::wattle_hovel`]
    /// instead.
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MEDIEVAL_BAND
    }

    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 54.0,
            min_spawn_dist: 110.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        // Centred foundation root + corner-origin 75×75 grammar child
        // offset by -footprint/2 (see the villa for the rationale). The
        // castle lands on craggy alpine slopes, so it gets the deepest
        // foundation in the pool.
        let mut root = crate::catalogue::items::util::footing(77.0, 77.0, [0.0, 0.0], 54.0);
        let mut castle = Generator::from_kind(build_kind());
        castle.transform.translation = crate::pds::Fp3([-37.5, 0.0, -37.5]);
        // `attach` (not a bare push): `footing` returns a root whose own
        // transform is sunk by half the buried plinth, and a plain child
        // inherits it - which drops the whole building below grade (#1039).
        crate::catalogue::items::util::attach(&mut root, castle);
        root
    }
}

fn build_kind() -> GeneratorKind {
    let mut materials = HashMap::new();

    // Coursed rubble: irregular, roughly squared stones laid in rough
    // courses with pale lime mortar between them, as curtain walls and keeps
    // were mostly built. It replaced a dressed ashlar whose block grid read
    // as a checkerboard in sunlight (#972's texture refresh: DryStone arrived
    // in symbios-texture 0.8.0). The palette is the ashlar's, kept bright so
    // the keep reads as stone rather than a dark mass; the relief is held low
    // because a wall seen edge-on shimmers with any more.
    materials.insert(
        "Stone".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.60, 0.57, 0.52]),
            roughness: Fp(0.9),
            uv_scale: tiles_per_metre(tile::DRY_STONE_COURSE * 8.0),
            texture: SovereignTextureConfig::DryStone(SovereignDryStoneConfig {
                courses: 8,
                stones_per_course: 5,
                course_jitter: Fp64(0.35),
                gap_width: Fp64(0.06),
                irregularity: Fp64(0.45),
                face_relief: Fp64(0.3),
                cell_variance: Fp64(0.45),
                moss_level: Fp64(0.06),
                color_stone: Fp3([0.60, 0.57, 0.52]),
                color_stone_alt: Fp3([0.54, 0.49, 0.42]),
                color_gap: Fp3([0.66, 0.63, 0.57]),
                normal_strength: Fp(2.5),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    materials.insert(
        "Shingle".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.34, 0.31, 0.30]),
            roughness: Fp(0.8),
            uv_scale: tiles_per_metre(tile::SHINGLE),
            texture: SovereignTextureConfig::Shingle(SovereignShingleConfig::default()),
            ..Default::default()
        },
    );

    materials.insert(
        "Wood".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.38, 0.22, 0.10]),
            roughness: Fp(0.7),
            uv_scale: tiles_per_metre(tile::PLANK_BOARD * 5.0),
            texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
                color_wood_light: Fp3([0.4, 0.22, 0.10]),
                color_wood_dark: Fp3([0.22, 0.12, 0.04]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    // The leaded pane itself: a cool neutral card. A `Window` texture is
    // one material across frame *and* glass, so tinting it warm to suggest
    // candlelight lights the leading too and the cames read as glowing wire
    // rather than dark metal. The hearth-light lives on the separate
    // `Hearth` surface set behind this card.
    materials.insert(
        "Glass".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.40, 0.44, 0.48]),
            roughness: Fp(0.3),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Window(SovereignWindowConfig {
                panes_x: 2,
                panes_y: 3,
                frame_width: Fp64(0.1),
                glass_opacity: Fp64(0.35),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    // The candle-lit room behind the keep's leaded windows - a plain
    // emissive surface, no texture, so all the pattern comes from the pane
    // card in front of it. This is what makes the castle read inhabited.
    materials.insert(
        "Hearth".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 0.62, 0.26]),
            emission_color: Fp3([1.0, 0.55, 0.22]),
            emission_strength: Fp(2.6),
            roughness: Fp(0.9),
            ..Default::default()
        },
    );

    // "Dark" - solid near-black, no texture. Used for arrow slits, gate
    // mouth, cloister arches; reads as deep shadow / void.
    materials.insert(
        "Dark".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.02, 0.02, 0.03]),
            roughness: Fp(1.0),
            uv_scale: Fp(1.0),
            ..Default::default()
        },
    );

    materials.insert(
        "Grass".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.20, 0.32, 0.14]),
            roughness: Fp(0.9),
            uv_scale: tiles_per_metre(tile::GROUND),
            texture: SovereignTextureConfig::Ground(SovereignGroundConfig {
                color_dry: Fp3([0.28, 0.38, 0.18]),
                color_moist: Fp3([0.14, 0.24, 0.10]),
                macro_scale: Fp64(4.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    // The ruined keep's floor: its fallen upper storeys as coarse, angular
    // rubble rather than the dressed ashlar it used to wear (#972's texture
    // refresh - Gravel arrived after this castle's materials were chosen).
    materials.insert(
        "Rubble".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.52, 0.49, 0.45]),
            roughness: Fp(0.95),
            uv_scale: tiles_per_metre(tile::GRAVEL_STONE * 8.0),
            texture: SovereignTextureConfig::Gravel(SovereignGravelConfig {
                scale: Fp64(8.0),
                metric: bevy_symbios_texture::noise::CellMetric::Chebyshev,
                color_stone: Fp3([0.60, 0.57, 0.52]),
                color_dark: Fp3([0.30, 0.28, 0.25]),
                color_fines: Fp3([0.42, 0.39, 0.35]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    // Grammar adapted from `bevy_symbios_shape/examples/medieval_castle.rs`.
    // Stochastic alternatives use the `weight%` syntax documented by
    // `symbios_shape::grammar::parse_rule`. Footprint 75×75 m matches the
    // example's `DVec3::new(75.0, 0.0, 75.0)`.
    let grammar_source = [
        // ── 1. Macro layout (concentric wards) ──
        // Wall thickness, merlon depth and how far a pane sits back in its reveal.
        "const WallD = 0.5",
        "const MerlonD = 0.2",
        "const PaneSet = 0.15",
        "Lot --> Split(X) { 8: LeftWall | ~1: CastleCore | 8: RightWall }",
        "CastleCore --> Split(Z) { 8: FrontWall | ~1: InnerWard | 22: KeepMass }",
        "InnerWard --> Split(X) { 8: WestWing | ~1: Courtyard | 8: EastWing }",
        // ── 2. Courtyard & cloisters ──
        "Courtyard --> Split(Z) { 4: CloisterZ | ~1: YardZ | 4: CloisterZ }",
        "YardZ --> Split(X) { 4: CloisterX | ~1: YardCenter | 4: CloisterX }",
        "YardCenter --> Translate(0, 0.16, 0) Mat(\"Grass\") I(\"Grass\")",
        // Each vault's pitch differs from both neighbours' - the front and back
        // rows alternate 35 and 37 degrees, the side strips 39 and 41 - so where two
        // overhangs cross, their slopes meet at an angle and never lie in one plane.
        "CloisterZ --> Repeat(X, 4) { CloisterBlockX }",
        "CloisterX --> Repeat(Z, 4) { CloisterBlockZ }",
        "CloisterBlockX --> when(split.i % 2 == 0): CloisterBlock(35) | else: CloisterBlock(37)",
        "CloisterBlockZ --> when(split.i % 2 == 0): CloisterBlock(39) | else: CloisterBlock(41)",
        "CloisterBlock(pitch) --> Extrude(4.5) Split(Y) { ~1: CloisterBody | 1.5: CloisterVault(pitch) }",
        "CloisterBody --> Comp(Faces) { Left: CloisterFacadeIn | Right: CloisterFacadeIn | Front: CloisterFacadeInset | Back: CloisterFacadeInset }",
        "CloisterFacadeInset --> Split(X) { WallD: NIL | ~1: CloisterFacadeIn | WallD: NIL }",
        "CloisterFacadeIn --> Translate(0, 0, -WallD) CloisterFacade",
        "CloisterFacade --> Split(X) { 0.5: SolidWall | ~1: OpenArch | 0.5: SolidWall }",
        "OpenArch --> Extrude(0.2) Mat(\"Dark\") I(\"Hole\")",
        "CloisterVault(pitch) --> Roof(Pyramid, pitch, 0.2) { Slope: ShingleRoof }",
        // ── 3. Barracks / wings (stochastic heights) ──
        // A wing body is its height less the 5 m of roofs, and a facade
        // storey is 6 m: a lower wing would draw no long walls at all.
        "WestWing --> 50% Extrude(14) WingSub | 50% Extrude(11) WingSub",
        "EastWing --> 50% Extrude(14) WingSub | 50% Extrude(18) WingSub",
        // One body a wing under a row of pavilion roofs. Its ends stand against the
        // front wall and the keep, so they are plain walls.
        "WingSub --> Split(Y) { ~1: WingBody | 5: WingRoofs }",
        "WingBody --> Comp(Faces) { Left: KeepFacadeIn | Right: KeepFacadeIn | Front: WingEnd | Back: WingEnd }",
        "WingEnd --> Split(X) { WallD: NIL | ~1: SolidWallIn | WallD: NIL }",
        "WingRoofs --> Repeat(Z, 15) { WingRoof }",
        // Each pavilion's roof stops its overhang short at its ends, so two
        // neighbours' slopes meet edge to edge rather than overlap.
        "WingRoof --> Split(Z) { 0.3: NIL | ~1: PavilionRoof | 0.3: NIL }",
        "PavilionRoof --> 40% Roof(Gable, 40, 0.3) { Slope: ShingleRoof | GableEnd: GableWall } | 30% Roof(Jerkinhead, 45, overhang=0.3, tier=0.3) { Slope: ShingleRoof | GableEnd: GableWall | HipEnd: ShingleRoof } | 20% Roof(DutchGable, 40, overhang=0.3, tier=0.5) { Slope: ShingleRoof | GableEnd: GableWall } | 10% Roof(Gambrel, 55, 20, overhang=0.3, tier=0.6) { LowerSlope: ShingleRoof | UpperSlope: ShingleRoof | GableEnd: GableWall }",
        "ShingleRoof --> Mat(\"Shingle\") I(\"Roof\")",
        "GableWall --> Mat(\"Stone\") I(\"Wall\")",
        // ── 4. Outer walls & gatehouse ──
        "LeftWall --> Split(Z) { 10: Tower | ~1: WallSegment | 10: Tower | ~1: WallSegment | 10: Tower }",
        "RightWall --> Split(Z) { 10: Tower | ~1: WallSegment | 10: Tower | ~1: WallSegment | 10: Tower }",
        "FrontWall --> Split(X) { ~1: WallSegment | 14: Gatehouse | ~1: WallSegment }",
        "WallSegment --> Extrude(12) Split(Y) { ~1: WallBody | 1.5: Battlements }",
        "WallBody --> Comp(Faces) { Left: WallFacadeIn | Right: WallFacadeIn | Front: WallFacadeInset | Back: WallFacadeInset | Top: WallWalkway }",
        "WallFacadeInset --> Split(X) { WallD: NIL | ~1: WallFacadeIn | WallD: NIL }",
        "WallFacadeIn --> Translate(0, 0, -WallD) WallFacade",
        "WallWalkway --> Offset(-WallD) { Inside: WalkDeck }",
        "WalkDeck --> Translate(0, 0, -0.3) Extrude(0.3) Mat(\"Stone\") I(\"Walkway\")",
        "WallFacade --> Repeat(X, 4) { WallBay }",
        "WallBay --> 70% SolidWall | 30% ArrowSlitBay",
        "Gatehouse --> Extrude(22) Split(Y) { 8: GatePassage | ~1: GateUpper | 2: Battlements }",
        "GatePassage --> Comp(Faces) { Front: GateArchInset | Back: GateArchInset | Left: SolidWallIn | Right: SolidWallIn | Top: WallWalkway }",
        "GateArchInset --> Split(X) { WallD: NIL | ~1: GateArchIn | WallD: NIL }",
        "GateArchIn --> Translate(0, 0, -WallD) GateArch",
        "SolidWallIn --> Translate(0, 0, -WallD) SolidWall",
        "GateArch --> Split(X) { ~1: SolidWall | 6: Portcullis | ~1: SolidWall }",
        "Portcullis --> Split(Y) { ~1: GateHole | 4: WoodGate }",
        "GateHole --> Extrude(0.1) Mat(\"Dark\") I(\"Hole\")",
        "WoodGate --> Extrude(0.4) Mat(\"Wood\") I(\"Gate\")",
        "GateUpper --> Comp(Faces) { Left: KeepFacadeIn | Right: KeepFacadeIn | Front: KeepFacadeInset | Back: KeepFacadeInset | Top: WallWalkway }",
        // ── 5. Towers (stochastic heights + tops) ──
        "Tower --> 30% Extrude(18) TowerSub | 40% Extrude(26) TowerSub | 30% Extrude(34) TowerSub",
        "TowerSub --> Split(Y) { ~1: TowerBody | 3: TowerUpper }",
        "TowerBody --> Comp(Faces) { Left: TowerFacadeIn | Right: TowerFacadeIn | Front: TowerFacadeInset | Back: TowerFacadeInset }",
        "TowerFacadeInset --> Split(X) { WallD: NIL | ~1: TowerFacadeIn | WallD: NIL }",
        "TowerFacadeIn --> Translate(0, 0, -WallD) TowerFacade",
        "TowerUpper --> Split(Y) { 0.5: CorbelBand | ~1: TowerTop }",
        "CorbelBand --> Translate(-MerlonD, 0, -MerlonD) Size(scope.x + 2 * MerlonD, scope.y, scope.z + 2 * MerlonD) Mat(\"Stone\") I(\"Wall\")",
        "TowerTop --> 50% Battlements | 50% TowerSpire",
        "TowerSpire --> 40% Roof(Pyramid, 70, 0.2) { Slope: ShingleRoof } | 30% Roof(Mansard, 75, 20, overhang=0.2, tier=0.6) { LowerSlope: ShingleRoof | UpperSlope: ShingleRoof } | 30% Roof(PyramidHip, 65, 0.2) { Slope: ShingleRoof }",
        "TowerFacade --> Repeat(Y, 4) { TowerFloor }",
        "TowerFloor --> Repeat(X, 3) { TowerBay }",
        "TowerBay --> 60% SolidWall | 40% ArrowSlitBay",
        // ── 6. Battlements & details ──
        "Battlements --> Comp(Faces) { Left: BattlementIn | Right: BattlementIn | Front: BattlementInset | Back: BattlementInset }",
        "BattlementInset --> Split(X) { MerlonD: NIL | ~1: BattlementIn | MerlonD: NIL }",
        "BattlementIn --> Translate(0, 0, -MerlonD) BattlementSide",
        "BattlementSide --> Repeat(X, 1.5) { Crenellation }",
        "Crenellation --> Split(X) { 0.8: Merlon | ~1: Crenel }",
        "Merlon --> Extrude(MerlonD) Mat(\"Stone\") I(\"Wall\")",
        "Crenel --> Extrude(0.05) Mat(\"Dark\") I(\"Wall\")",
        // ── 7. The Great Keep (with stochastic ruin variation) ──
        "KeepMass --> 70% GreatKeep | 30% RuinedKeep",
        "GreatKeep --> Extrude(45) Split(Y) { 37: KeepLower | 8: KeepUpper }",
        "KeepLower --> Comp(Faces) { Left: KeepFacadeIn | Right: KeepFacadeIn | Front: KeepFacadeInset | Back: KeepFacadeInset | Top: WallWalkway }",
        "KeepUpper --> Split(Y) { 1: CorbelBand | ~1: KeepTopBody | 1.5: Battlements }",
        "KeepTopBody --> Comp(Faces) { Left: TowerFacadeIn | Right: TowerFacadeIn | Front: TowerFacadeInset | Back: TowerFacadeInset }",
        "RuinedKeep --> Split(Z) { ~1: GreatKeep | 16: RuinedSection }",
        "RuinedSection --> Extrude(18) Comp(Faces) { Bottom: RuinFloor | Back: RuinFacadeInset | Left: RuinFacadeIn | Right: RuinFacadeIn }",
        "RuinFloor --> Translate(0, 0, -0.16) Mat(\"Rubble\") I(\"Rubble\")",
        "RuinFacadeInset --> Split(X) { WallD: NIL | ~1: RuinFacadeIn | WallD: NIL }",
        "RuinFacadeIn --> Translate(0, 0, -WallD) RuinFacade",
        // A roofless shell keeps its window openings, empty and dark: no
        // pane, and no lit room behind a wall with nothing behind it.
        "RuinFacade --> Repeat(Y, 6) { RuinStorey }",
        "RuinStorey --> Repeat(X, 4) { RuinBay }",
        "RuinBay --> 60% SolidWall | 40% EmptyWindowBay",
        "EmptyWindowBay --> Split(X) { ~1: SolidWall | 2.5: EmptyWindowVert | ~1: SolidWall }",
        "EmptyWindowVert --> Split(Y) { 1.5: SolidWall | 3.5: EmptyWindow | ~1: SolidWall }",
        "EmptyWindow --> Extrude(0.1) Mat(\"Dark\") I(\"Hole\")",
        "KeepFacadeInset --> Split(X) { WallD: NIL | ~1: KeepFacadeIn | WallD: NIL }",
        "KeepFacadeIn --> Translate(0, 0, -WallD) KeepFacade",
        "KeepFacade --> Repeat(Y, 6) { KeepFloor }",
        "KeepFloor --> Repeat(X, 4) { KeepBay }",
        "KeepBay --> 50% SolidWall | 30% LargeWindowBay | 20% BalconyBay",
        "LargeWindowBay --> Split(X) { ~1: SolidWall | 2.5: WindowVert | ~1: SolidWall }",
        "WindowVert --> Split(Y) { 1.5: SolidWall | 3.5: WindowOpening | ~1: SolidWall }",
        // A wooden sill under the pane: let into the jambs, 5 cm proud of
        // the wall face and back just behind the glass, its top and foot at
        // heights no other split uses.
        "WindowOpening --> Split(Y) { 0.001: WindowSill | ~1: GlassWindow }",
        "WindowSill --> Translate(-0.1, -0.01, WallD - PaneSet - 0.02) Size(scope.x + 0.2, 0.06, 0) Extrude(PaneSet + 0.07) Mat(\"Wood\") I(\"Sill\")",
        // Two surfaces per window: the leaded card set back in its reveal, the
        // candle-lit room behind it at the back of the wall.
        "GlassWindow --> Extrude(WallD - PaneSet) Comp(Faces) { Back: PaneCard | Front: HearthRoom }",
        "PaneCard --> Mat(\"Glass\") I(\"Pane\")",
        "HearthRoom --> Mat(\"Hearth\") I(\"Room\")",
        "BalconyBay --> Split(Y) { 1.45: BalconySupport | 3.55: BalconyDoor | ~1: SolidWall }",
        "BalconySupport --> Split(Y) { ~1: SolidWall | 0.3: BalconyDeck }",
        "BalconyDeck --> Split(X) { 0.1: SolidWall | ~1: BalconySlab | 0.1: SolidWall }",
        "BalconySlab --> Extrude(WallD + 0.4) Mat(\"Stone\") I(\"Balcony\")",
        "BalconyDoor --> Split(X) { ~1: SolidWall | 1.8: WoodDoor | ~1: SolidWall }",
        "WoodDoor --> Extrude(0.3) Mat(\"Wood\") I(\"Door\")",
        // ── 8. Core terminal geometry ──
        "SolidWall --> Extrude(WallD) Mat(\"Stone\") I(\"Wall\")",
        "ArrowSlitBay --> Split(X) { ~1: SolidWall | 0.4: ArrowSlit | ~1: SolidWall }",
        "ArrowSlit --> Split(Y) { 1.5: SolidWall | 2.5: SlitHole | ~1: SolidWall }",
        "SlitHole --> Extrude(0.1) Mat(\"Dark\") I(\"Hole\")",
    ]
    .join("\n");

    GeneratorKind::Shape {
        grammar_source,
        root_rule: "Lot".to_string(),
        footprint: Fp3([75.0, 0.0, 75.0]),
        seed: 42,
        materials,
        // Nothing turned: the castle's masses are all square-plan.
        round_meshes: Vec::new(),
        // Everything a visitor should meet; the arches, arrow slits and gate
        // passage (`Hole`) stay open, the lawn and rubble are ground, and a
        // hearth card is behind a pane.
        solid_meshes: ["Balcony", "Door", "Gate", "Pane", "Roof", "Walkway", "Wall"]
            .map(String::from)
            .to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure::{mesh_bounds, transform_of};
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::pds::PrimCommon;
    use crate::pds::sanitize_generator;

    /// Grammar seeds the guards draw the castle with besides its own (42):
    /// between them a ruined keep and a whole one, and both heights of each
    /// wing (see `the_seeds_draw_each_kind_of_castle`).
    const SEEDS: [u64; 4] = [0, 1, 3, 9];

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = MedievalCastle.build("");
        sanitize_generator(&mut g);
        // The entry root is now the centred foundation plinth; the
        // grammar hangs beneath it as the first child.
        assert!(
            matches!(
                g.kind,
                GeneratorKind::Cuboid {
                    common: PrimCommon { solid: true, .. },
                    ..
                }
            ),
            "{} root must be the solid foundation plinth",
            "castle"
        );
        let shape = &g.children[0];
        match &shape.kind {
            GeneratorKind::Shape {
                grammar_source,
                root_rule,
                materials,
                ..
            } => {
                assert!(!grammar_source.is_empty());
                assert_eq!(root_rule, "Lot");
                for slot in [
                    "Stone", "Shingle", "Wood", "Glass", "Hearth", "Dark", "Grass", "Rubble",
                ] {
                    assert!(
                        materials.contains_key(slot),
                        "missing material slot: {slot}"
                    );
                }
            }
            other => panic!("castle root must remain Shape after sanitise; got {other:?}"),
        }
    }

    /// The overhaul's two checks (#972, #1575), on the castle as the
    /// catalogue ships it and as [`SEEDS`] draw it - a seeded settlement's
    /// castle draws with a grammar seed of its own (#1505, #1514), so the
    /// rules have to hold for the grammar, not for one drawing of it.
    /// Against the shipped grammar: 635 pairs of faces drawn in one place at
    /// seed 42, and a Dutch gable's two ends floating.
    #[test]
    fn nothing_z_fights_and_nothing_floats() {
        let built = MedievalCastle.build("");
        assert_no_z_fighting(&built, "medieval_castle");
        assert_nothing_floats(&built, "medieval_castle");
        for seed in SEEDS {
            let drawn = built.with_shape_seed(seed);
            let slug = format!("medieval_castle at grammar seed {seed}");
            assert_no_z_fighting(&drawn, &slug);
            assert_nothing_floats(&drawn, &slug);
        }
    }

    /// The terminals the castle's grammar derives at `seed`, each by its
    /// mesh id and its box in the castle node's frame.
    fn terminals(seed: u64) -> Vec<(String, [f64; 3], [f64; 3])> {
        let mut kind = build_kind();
        if let GeneratorKind::Shape { seed: own, .. } = &mut kind {
            *own = seed;
        }
        let model = kind
            .shape_def()
            .expect("a Shape node")
            .derive()
            .ok()
            .expect("the castle derives");
        model
            .terminals
            .iter()
            .map(|t| {
                let s = &t.scope;
                let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
                for k in 0..8 {
                    let pick = |bit: usize, extent: f64| if k & bit != 0 { extent } else { 0.0 };
                    let local = symbios_shape::Vec3::new(
                        pick(1, s.size.x),
                        pick(2, s.size.y),
                        pick(4, s.size.z),
                    );
                    let corner = (s.position + s.rotation * local).to_array();
                    for i in 0..3 {
                        lo[i] = lo[i].min(corner[i]);
                        hi[i] = hi[i].max(corner[i]);
                    }
                }
                (t.mesh_id.clone(), lo, hi)
            })
            .collect()
    }

    /// [`SEEDS`] cover what they are said to: a ruined keep (its rubble
    /// floor) and a whole one, and each wing's two heights (the plain wall
    /// across each wing's end is the body's height).
    #[test]
    fn the_seeds_draw_each_kind_of_castle() {
        let mut ruined = (false, false);
        let mut west = std::collections::BTreeSet::new();
        let mut east = std::collections::BTreeSet::new();
        for seed in std::iter::once(42).chain(SEEDS) {
            let drawn = terminals(seed);
            if drawn.iter().any(|(id, ..)| id == "Rubble") {
                ruined.0 = true;
            } else {
                ruined.1 = true;
            }
            // A wing end: a wall across the wing's 7 m inside its corners,
            // standing against the front wall (z = 8).
            for (id, lo, hi) in &drawn {
                let across = hi[0] - lo[0];
                if id == "Wall" && (across - 7.0).abs() < 1e-3 && (lo[2] - 8.0).abs() < 1e-3 {
                    let height = (hi[1] - lo[1]).round() as i64;
                    if hi[0] < 37.5 {
                        west.insert(height);
                    } else {
                        east.insert(height);
                    }
                }
            }
        }
        assert_eq!(
            ruined,
            (true, true),
            "the seeds draw a ruined keep and a whole one"
        );
        assert_eq!(
            west.len(),
            2,
            "the west wing at both heights, drew {west:?}"
        );
        assert_eq!(
            east.len(),
            2,
            "the east wing at both heights, drew {east:?}"
        );
    }

    /// **Every wing draws its long walls.** A facade storey is 6 m
    /// (`Repeat(Y, 6)`) and `Repeat` lays only whole tiles, so a wing body
    /// under 6 m draws no long walls at all: the lower west wing's 5 m body
    /// was a hollow box of roofs and end walls at the catalogue's own seed.
    /// For each checked drawing, the walls on each of a wing's long sides
    /// cover at least half of it - windows, doors and balconies the rest -
    /// the body's height read from the plain wall across its front end.
    #[test]
    fn every_wing_draws_its_long_walls() {
        let near = |a: f64, b: f64| (a - b).abs() < 1e-3;
        for seed in std::iter::once(42).chain(SEEDS) {
            let drawn = terminals(seed);
            for (wing, inside, sides) in [
                ("west", (8.5, 15.5), [(8.0, 8.5), (15.5, 16.0)]),
                ("east", (59.5, 66.5), [(59.0, 59.5), (66.5, 67.0)]),
            ] {
                let body = drawn
                    .iter()
                    .find(|(id, lo, hi)| {
                        id == "Wall"
                            && near(lo[0], inside.0)
                            && near(hi[0], inside.1)
                            && near(lo[2], 8.0)
                    })
                    .map(|(_, lo, hi)| hi[1] - lo[1])
                    .unwrap_or_else(|| {
                        panic!("seed {seed}: the {wing} wing has no front end wall")
                    });
                for (x0, x1) in sides {
                    let walls: f64 = drawn
                        .iter()
                        .filter(|(id, lo, hi)| {
                            id == "Wall"
                                && lo[0] > x0 - 1e-3
                                && hi[0] < x1 + 1e-3
                                && lo[2] > 8.0 - 1e-3
                                && hi[2] < 53.0 + 1e-3
                                && hi[1] < body + 1e-3
                        })
                        .map(|(_, lo, hi)| (hi[2] - lo[2]) * (hi[1] - lo[1]))
                        .sum();
                    let share = walls / (45.0 * body);
                    assert!(
                        share >= 0.5,
                        "medieval_castle at grammar seed {seed}: the {wing} wing's long side at \
                         x {x0}..{x1} is {:.0} % wall over its {body} m body",
                        share * 100.0
                    );
                }
            }
        }
    }

    /// **The lawn and the rubble are drawn on the footing, not under it.**
    /// The footing's top stands 15 cm above the grammar's ground, so ground
    /// cover drawn at the grammar's y = 0 - as the courtyard lawn was - is
    /// inside the plinth and the courtyard shows plinth. Read from the
    /// footing's drawn box and the derived terminals.
    #[test]
    fn ground_cover_is_drawn_on_the_footing() {
        let built = MedievalCastle.build("");
        let footing = mesh_bounds(&built.kind, &transform_of(&built.transform))
            .expect("the footing is drawn")
            .max
            .y;
        let castle_y = f64::from(built.children[0].transform.translation.0[1])
            + f64::from(built.transform.translation.0[1]);
        let mut seen = 0;
        for seed in std::iter::once(42).chain(SEEDS) {
            for (id, lo, _) in terminals(seed) {
                if id == "Grass" || id == "Rubble" {
                    seen += 1;
                    let y = lo[1] + castle_y;
                    assert!(
                        y > f64::from(footing),
                        "medieval_castle at grammar seed {seed}: the {id} is drawn at {y}, \
                         under the footing's top at {footing}"
                    );
                }
            }
        }
        assert!(seen > SEEDS.len(), "the lawn and the rubble were not found");
    }

    /// Walks every grammar line through the shared harness. Critical for
    /// the castle because its rules use weighted alternatives (`70% A |
    /// 30% B`) the simple villa doesn't - regressions there would only
    /// surface as runtime warnings.
    #[test]
    fn grammar_parses_and_resolves_materials() {
        crate::catalogue::items::shape_grammar_test::assert_grammar_parses_and_derives(
            build_kind(),
            "castle",
        );
    }
}
