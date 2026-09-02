//! Palace range — a long low multi-doorway building wrapped on three sides
//! of a raised court, in the manner of the Puuc quadrangles.
//!
//! Where the [step pyramid](super::step_pyramid) is the theme's vertical
//! statement, the range structure is its horizontal one: the same battered
//! limestone, laid out as a *plan* instead of a climb. It carves that plan
//! with `ShapeU` — the range takes the three `Shape` parts, the court takes
//! the `Remainder` — which nothing else in the catalogue has used and which
//! is the only way to get a real court rather than four buildings pretending
//! to be one.
//!
//! The elevation is the Puuc order, bottom to top: a plain lower wall broken
//! by a rhythm of doorways, a projecting **medial moulding**, a deep
//! decorated **frieze**, a **cornice**, and — on the buildings that carried
//! one — a **roof comb**, the pierced openwork crest that made a one-storey
//! building read from across the plaza.
//!
//! **This item distributes its randomness the opposite way to the two rows.**
//! A terrace or a machiya block is many buildings that happen to touch, so
//! almost everything is a per-house roll. A palace is *one* building, so
//! almost everything is a `Pick`: the frieze scheme, the bay rhythm and the
//! crown are single decisions binding all three wings. What varies shape to
//! shape is only what genuinely varied bay to bay — whether a given bay is
//! an open doorway, a blind niche, or left solid.
//!
//! Two details are worth knowing when reading the grammar:
//!
//! - the podium is **battered** (`Taper`), so its top is inset from its base;
//!   the range is set back further than that batter and the stair is embedded
//!   into it, or both would hang over a face that has already leaned away;
//! - the frieze **colonnettes are round** (`round_meshes`), because a Puuc
//!   colonnette band is a row of engaged half-columns and square ones read as
//!   a picket fence.
//!
//! Footprint 24 × 21, and the whole thing is turned to face the court at the
//! viewer — see [`PalaceRange::build`].

use std::collections::HashMap;

use crate::catalogue::items::util::{attach, footing};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp4, Generator, GeneratorKind, SovereignMaterialSettings, SovereignTextureConfig,
};
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    LIMESTONE_PALE, MESO_BAND, STONE_GREY, STUCCO_CREAM, STUCCO_RED, TIMBER_BROWN, limestone,
    painted, patterned_floor, timber,
};

// ── Plan ──────────────────────────────────────────────────────────────────

/// Footprint of the whole precinct, stair included.
const LOT_X: f32 = 24.0;
const LOT_Z: f32 = 21.0;

/// Depth of the stair apron, taken off the back of the lot. The podium fills
/// what is left.
const STAIR_D: f32 = 3.2;
/// Width of the stair and the rise of one step.
const STAIR_W: f32 = 6.0;
const STEP_H: f32 = 0.3;

/// Height of the podium the whole complex stands on.
const PLAT_H: f32 = 2.4;
/// Batter of the podium, as [`symbios_shape`]'s `Taper` fraction: `0.0` is a
/// box, `1.0` a pyramid. A talud this shallow reads as masonry leaning in
/// rather than as a slope.
const TALUD: f32 = 0.07;
/// Terrace walkway left *beyond* the batter, and how far a stair tread bites
/// into the podium past it.
///
/// Both are clearances on top of what the talud already takes, never the
/// whole set-back — see [`plat_step`] and [`stair_embed`].
const TERRACE_W: f32 = 0.65;
const STAIR_BITE: f32 = 0.28;

/// Depth of the closed back range, and width of each side wing.
const BAR_D: f32 = 5.6;
const WING_W: f32 = 5.6;

// ── The Puuc order, bottom to top ─────────────────────────────────────────

/// Plain lower wall, up to the medial moulding.
const LOWER_H: f32 = 2.5;
/// The two projecting mouldings — medial (below the frieze) and cornice
/// (above it).
const MEDIAL_H: f32 = 0.3;
const CORNICE_H: f32 = 0.35;
/// The decorated band between them. This is where the theme lives.
const FRIEZE_H: f32 = 1.5;
/// Everything above the cornice: the parapet, and the air a roof comb needs.
const CROWN_H: f32 = 3.6;
/// Height of the parapet standing on the roof deck.
const PARAPET_H: f32 = 0.7;

/// Masonry thickness, and the depth of every doorway reveal.
const WALL_D: f32 = 0.35;
/// How far the mouldings project past the wall face.
const MOULD_OUT: f32 = 0.28;
/// Depth of the lintel band carried over every doorway. The clear height of
/// the opening is whatever the lower wall has left — see [`door_h`].
const LINTEL_H: f32 = 0.3;

/// Thickness of the roof comb, and the height of its solid base band.
const COMB_T: f32 = 0.45;
const COMB_BASE_H: f32 = 0.55;
/// Pitch of the comb's perforations and the width of one slot.
///
/// A crestería is a *wall* with holes punched through it, not a colonnade:
/// authored the other way round — narrow piers with wide gaps — it reads
/// unmistakably as a balcony railing, which is exactly what the first render
/// of this item produced.
const COMB_PITCH: f32 = 1.25;
const COMB_SLOT: f32 = 0.36;

/// Total wall height from the podium top to the top of the crown zone.
fn range_height() -> f32 {
    LOWER_H + MEDIAL_H + FRIEZE_H + CORNICE_H + CROWN_H
}

/// How far the top of the battered podium is drawn in from its base, per
/// side, on each axis.
fn talud_inset() -> (f32, f32) {
    (TALUD * LOT_X / 2.0, TALUD * (LOT_Z - STAIR_D) / 2.0)
}

/// Set-back from the podium edge to the range walls.
///
/// Derived from the batter rather than authored: a talud recedes as it
/// rises, so a set-back measured against the podium's *base* leaves the
/// range overhanging a face that has already leaned away. Deepening
/// [`TALUD`] now widens the terrace with it instead of eating it.
fn plat_step() -> f32 {
    let (inset_x, inset_z) = talud_inset();
    inset_x.max(inset_z) + TERRACE_W
}

/// How far each stair tread is buried into the podium.
///
/// The same requirement from the other side: a tread flush with the
/// podium's base leaves a wedge of daylight behind every step above the
/// bottom one, because the face it meets has receded.
fn stair_embed() -> f32 {
    talud_inset().1 + STAIR_BITE
}

/// Clear height of a doorway — whatever the lower wall has left once the
/// lintel band is taken off the top. Derived, so a doorway can never
/// outgrow the wall it pierces.
fn door_h() -> f32 {
    LOWER_H - LINTEL_H
}

// Pure design relations between authored constants. These cannot fail at run
// time, only at edit time, so they belong at compile time rather than in a
// test that clippy would rightly call a constant assertion.
const _: () = assert!(
    BAR_D - 2.0 * WALL_D > 2.0,
    "the back range is too shallow to hold a room"
);
const _: () = assert!(
    CROWN_H - PARAPET_H > 2.0,
    "no room above the parapet for a roof comb worth the name"
);

// ── Palette ───────────────────────────────────────────────────────────────

/// The dark of a doorway with a corbel-vaulted room behind it. Flat and
/// untextured on purpose: it is a void, and any pattern on it reads as a
/// painted panel closing the opening.
const VOID_DARK: [f32; 3] = [0.05, 0.045, 0.04];

fn shadow() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(VOID_DARK),
        roughness: Fp(0.95),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

pub struct PalaceRange;

impl CatalogueEntry for PalaceRange {
    fn slug(&self) -> &'static str {
        "palace_range"
    }
    fn name(&self) -> &'static str {
        "Palace Range"
    }
    fn description(&self) -> &'static str {
        "Long multi-doorway range wrapping a raised court, under a mosaic frieze and roof comb."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// The dressed-stone city, not the farming settlement — the destitute end
    /// of the theme stays the [`adobe_hut`](super::adobe_hut) kit.
    fn prosperity_band(&self) -> ProsperityBand {
        MESO_BAND
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 12.0,
            min_spawn_dist: 44.0,
        }
    }

    /// Builds the plinth root with the grammar turned to face the viewer.
    ///
    /// `ShapeU` always closes its U toward low Z and opens it toward high Z,
    /// and `Comp`'s `Front` selector is the low-Z face — so left alone, this
    /// item would present its blank back wall to whoever approaches and hide
    /// the court behind it. The child therefore carries a half-turn about Y,
    /// which puts the court mouth and its stair on the approach side. The
    /// translation is `+footprint/2` rather than the usual `-footprint/2`
    /// because the half-turn has already mapped the corner-origin grammar
    /// into negative local space.
    fn build(&self, _local_did: &str) -> Generator {
        let mut root = footing(LOT_X + 1.0, LOT_Z + 1.0, [0.0, 0.0], 12.0);
        let mut range = Generator::from_kind(build_kind());
        range.transform.rotation = Fp4([0.0, 1.0, 0.0, 0.0]);
        range.transform.translation = Fp3([LOT_X / 2.0, 0.0, LOT_Z / 2.0]);
        // `attach`, not a bare child push: `footing` returns a root already
        // sunk by half the buried plinth, and a plain child inherits that
        // and drops the whole precinct below grade (#1039).
        attach(&mut root, range);
        root
    }
}

/// The palette, keyed by the `Mat("...")` names the grammar emits.
fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = HashMap::new();
    m.insert("Limestone".to_string(), limestone(LIMESTONE_PALE));
    // Frieze backing: a smoother stuccoed plane, so the mosaic blocks
    // standing off it read as blocks rather than as more coursed masonry.
    m.insert("Stucco".to_string(), painted(STUCCO_CREAM));
    // Roof combs and their crest were the most heavily painted part of a
    // Maya facade; red is what survives of them.
    m.insert("Red".to_string(), painted(STUCCO_RED));
    m.insert("Plaza".to_string(), patterned_floor(STONE_GREY));
    // The roof was plastered, not paved: in the court's cold grey it reads
    // as a hole in the building rather than the top of it.
    m.insert("Roof".to_string(), painted(STUCCO_CREAM));
    m.insert("Timber".to_string(), timber(TIMBER_BROWN));
    m.insert("Shadow".to_string(), shadow());
    m
}

fn build_kind() -> GeneratorKind {
    // The grammar's constants are formatted from the Rust ones, so the
    // numbers the rules split on and the numbers the geometry tests reason
    // about cannot drift apart.
    let declarations = format!(
        "const StairD = {STAIR_D}\n\
         const StairW = {STAIR_W}\n\
         const StairEmbed = {stair_embed}\n\
         const StepH = {STEP_H}\n\
         const PlatH = {PLAT_H}\n\
         const Talud = {TALUD}\n\
         const PlatStep = {plat_step}\n\
         const RangeH = {range_h}\n\
         const BarD = {BAR_D}\n\
         const WingW = {WING_W}\n\
         const LowerH = {LOWER_H}\n\
         const MedialH = {MEDIAL_H}\n\
         const FriezeH = {FRIEZE_H}\n\
         const CorniceH = {CORNICE_H}\n\
         const ParapetH = {PARAPET_H}\n\
         const WallD = {WALL_D}\n\
         const MouldOut = {MOULD_OUT}\n\
         const DoorH = {door_h}\n\
         const CombT = {COMB_T}\n\
         const CombBaseH = {COMB_BASE_H}\n\
         const CombPitch = {COMB_PITCH}\n\
         const CombSlot = {COMB_SLOT}",
        range_h = range_height(),
        plat_step = plat_step(),
        stair_embed = stair_embed(),
        door_h = door_h(),
    );

    let rules = [
        // ── 1. Precinct and stair ─────────────────────────────────────────
        //    The stair is taken off the back of the lot rather than carved
        //    out of the podium, so the podium stays one undivided battered
        //    block with no seams down its face.
        "Lot --> Split(Z) { ~1: PrecinctZone | StairD: StairApron }",
        "PrecinctZone --> Extrude(PlatH + RangeH) Complex",
        "Complex --> Split(Y) { PlatH: Podium | ~1: Upper }",
        "Podium --> Taper(Talud) Mat(\"Limestone\") I(\"Podium\")",
        // Set back past the batter, leaving a terrace walkway all round.
        "Upper --> Size(scope.x - 2 * PlatStep, scope.y, scope.z - 2 * PlatStep) Center(XZ) Precinct",
        "StairApron --> Split(X) { ~1: NIL | StairW: Stair | ~1: NIL }",
        "Stair --> Extrude(PlatH) StairFlight",
        "StairFlight --> Repeat(Y, StepH) { StepBand }",
        // Each tread is shallower than the one below it — that is the whole
        // staircase, and it needs `split.i` to know how far up it is. The
        // `Translate` buries the tread in the podium; without it every step
        // above the bottom leaves a wedge of daylight behind it, because the
        // battered face it meets has already leaned away.
        "StepBand --> Size(scope.x, scope.y, StairEmbed + StairD * (split.n - split.i) / split.n) \
                      Translate(0, 0, -StairEmbed) Mat(\"Limestone\") I(\"Step\")",
        // ── 2. The U ──────────────────────────────────────────────────────
        //    Three wings and a court, carved from one footprint. The wings
        //    are treated uniformly: faces that end up buried where a wing
        //    meets the back range emit geometry nobody sees, which is the
        //    price of not needing to know which wing is which.
        "Precinct --> ShapeU(BarD, WingW, WingW) { Shape: RangeWing | Remainder: CourtVoid }",
        "CourtVoid --> Comp(Faces) { Bottom: CourtFloor | _: NIL }",
        "CourtFloor --> Mat(\"Plaza\") I(\"Floor\")",
        "RangeWing --> Split(Y) { LowerH: LowerZone | MedialH: Moulding | FriezeH: FriezeZone \
                                  | CorniceH: Moulding | ~1: CrownZone }",
        // ── 3. Lower wall: the doorway rhythm ─────────────────────────────
        "LowerZone --> Comp(Faces) { Top: NIL | Bottom: NIL | _: WallFace }",
        // Three regimes by width. The short ends of the wings cannot hold a
        // rhythm at all — the widest one needs 7.4 m before its flanks and a
        // single cycle fit — so they take one central bay instead, which is
        // what a range end actually had.
        "WallFace --> when(scope.x < 3.6): PlainWall \
                      | when(scope.x < 7.6): EndWall \
                      | else: DoorRun",
        "EndWall --> Split(X) { ~1: PlainWall | 1.6: Bay | ~1: PlainWall }",
        // One rhythm for the whole palace: a range is a single building, so
        // its bays are laid out once, not negotiated wall by wall.
        "DoorRun --> Pick(\"bays\") { 42% WideBays | 34% CloseBays | 24% GrandBays }",
        // Absolute widths inside the rhythm group — a floating slot here
        // tiles at its weight-as-length and packs the wall with slivers.
        "WideBays --> Split(X) { 1.0: PlainWall | { 1.9: PlainWall | 1.5: Bay }* | 1.0: PlainWall }",
        "CloseBays --> Split(X) { 0.8: PlainWall | { 1.3: PlainWall | 1.3: Bay }* | 0.8: PlainWall }",
        "GrandBays --> Split(X) { 1.4: PlainWall | { 2.6: PlainWall | 2.0: Bay }* | 1.4: PlainWall }",
        // Per-bay, and only per-bay: this is what really varied along a range.
        "Bay --> 68% Doorway | 20% Niche | 12% PlainWall",
        "Doorway --> Split(Y) { DoorH: DoorOpening | ~1: LintelBand }",
        "DoorOpening --> Extrude(WallD) Comp(Faces) { Back: NIL | Front: VoidFace | Top: LintelFace | _: JambFace }",
        "Niche --> Split(Y) { 0.9: PlainWall | ~1: NicheBox | 0.5: PlainWall }",
        "NicheBox --> Extrude(WallD) Comp(Faces) { Back: NIL | Front: VoidFace | _: JambFace }",
        "LintelBand --> Extrude(WallD + 0.05) Mat(\"Limestone\") I(\"Lintel\")",
        // The lintel over a Maya doorway was a beam, not an arch.
        "LintelFace --> Mat(\"Timber\") I(\"Beam\")",
        "JambFace --> Mat(\"Limestone\") I(\"Jamb\")",
        "VoidFace --> Mat(\"Shadow\") I(\"Room\")",
        // ── 4. Mouldings ──────────────────────────────────────────────────
        "Moulding --> Comp(Faces) { Top: NIL | Bottom: NIL | _: MouldFace }",
        "MouldFace --> Extrude(WallD + MouldOut) Mat(\"Limestone\") I(\"Moulding\")",
        // ── 5. The frieze ─────────────────────────────────────────────────
        //    Also one decision for the whole palace. A range with a lattice
        //    on one wing and colonnettes on the next is two buildings.
        "FriezeZone --> Comp(Faces) { Top: NIL | Bottom: NIL | _: FriezeFace }",
        "FriezeFace --> when(scope.x < 2.2): BackWall | else: FriezeRun",
        "FriezeRun --> Pick(\"frieze\") { 36% LatticeFrieze | 34% ColonnetteFrieze | 30% FretFrieze }",
        // Celosía lattice: a true checkerboard needs the column's parity,
        // which is what `split.i` is for — without it the columns are all
        // identical and the band reads as vertical stripes.
        "LatticeFrieze --> Repeat(X, 0.6) { LatticeCol }",
        "LatticeCol --> when(split.i % 2 < 0.5): LatticeOdd | else: LatticeEven",
        "LatticeOdd --> Split(Y) { ~1: LatticeA | ~1: LatticeB | ~1: LatticeA }",
        "LatticeEven --> Split(Y) { ~1: LatticeB | ~1: LatticeA | ~1: LatticeB }",
        "LatticeA --> Split(X) { ~1: Boss | ~1: BackWall }",
        "LatticeB --> Split(X) { ~1: BackWall | ~1: Boss }",
        // Engaged half-columns between two rails — the Puuc signature. These
        // are the entries in `round_meshes`, so they mesh as cylinders.
        "ColonnetteFrieze --> Split(Y) { 0.2: FriezeRail | ~1: ColonnetteRun | 0.2: FriezeRail }",
        "ColonnetteRun --> Repeat(X, 0.44) { ColonnetteCell }",
        "ColonnetteCell --> Split(X) { 0.3: Colonnette | ~1: BackWall }",
        "Colonnette --> Extrude(WallD + 0.2) Mat(\"Limestone\") I(\"Colonnette\")",
        "FriezeRail --> Extrude(WallD + 0.12) Mat(\"Limestone\") I(\"Rail\")",
        // Step-fret (xicalcoliuhqui): a stepped block marching along the band.
        "FretFrieze --> Repeat(X, 1.5) { FretCell }",
        "FretCell --> Split(Y) { ~1: FretTop | ~1: FretMid | ~1: FretLow }",
        "FretTop --> Split(X) { ~1: Boss | 0.5: BackWall }",
        "FretMid --> Split(X) { 0.45: BackWall | 0.5: Boss | ~1: BackWall }",
        "FretLow --> Split(X) { 0.45: BackWall | ~1: Boss }",
        "Boss --> Extrude(WallD + 0.16) Mat(\"Limestone\") I(\"Boss\")",
        "BackWall --> Extrude(WallD) Mat(\"Stucco\") I(\"Frieze\")",
        // ── 6. The crown ──────────────────────────────────────────────────
        // The comb strip is carved out of the crown zone BEFORE the parapet
        // is banded, so the crest rises from the roof surface. Splitting Y
        // first instead leaves it starting at parapet height, floating a
        // wall clear of the deck it is supposed to stand on.
        "CrownZone --> Pick(\"crown\") { 46% CombCrown | 54% PlainCrown }",
        "PlainCrown --> Split(Y) { ParapetH: ParapetBand | ~1: NIL }",
        // The comb always runs along the wing's long axis, which the wing's
        // own proportions give away — no need to know which wing it is.
        "CombCrown --> when(scope.x > scope.z): Split(Z) { ~1: PlainCrown | CombT: CombAlongX | ~1: PlainCrown } \
                       | else: Split(X) { ~1: PlainCrown | CombT: CombAlongZ | ~1: PlainCrown }",
        "ParapetBand --> Comp(Faces) { Top: NIL | Bottom: RoofDeck | _: PlainWall }",
        "RoofDeck --> Mat(\"Roof\") I(\"Deck\")",
        "CombAlongX --> Comp(Faces) { Back: CombScreen | _: NIL }",
        "CombAlongZ --> Comp(Faces) { Right: CombScreen | _: NIL }",
        "CombScreen --> when(scope.x < 2.5): NIL | else: CombPanel",
        // Two pierced tiers between three solid rails — mostly wall, with
        // slots punched through it. The piercing is what stopped a crest
        // this tall from acting as a sail.
        "CombPanel --> Split(Y) { CombBaseH: CombRail | ~1: CombGrid | 0.32: CombRail | ~1: CombGrid | 0.28: CombRail }",
        "CombGrid --> Repeat(X, CombPitch) { CombCell }",
        "CombCell --> Split(X) { ~1: CombPier | CombSlot: NIL }",
        "CombPier --> Extrude(CombT) Mat(\"Red\") I(\"Comb\")",
        "CombRail --> Extrude(CombT) Mat(\"Red\") I(\"Comb\")",
        // ── 7. Shared terminals ───────────────────────────────────────────
        "PlainWall --> Extrude(WallD) Mat(\"Limestone\") I(\"Wall\")",
    ];

    let grammar_source = std::iter::once(declarations.as_str())
        .chain(rules)
        .collect::<Vec<_>>()
        .join("\n");

    GeneratorKind::Shape {
        grammar_source,
        root_rule: "Lot".to_string(),
        footprint: Fp3([LOT_X, 0.0, LOT_Z]),
        seed: 7,
        materials: materials(),
        // Engaged half-columns, not square pickets.
        round_meshes: vec!["Colonnette".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::shape_grammar_test::assert_grammar_parses_and_derives;
    use crate::pds::PrimCommon;
    use crate::pds::sanitize_generator;
    use symbios_shape::ShapeModel;

    /// Derives the precinct at `seed` through the same statement path the
    /// runtime uses.
    fn derive_range(seed: u64) -> ShapeModel {
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
            .expect("palace range derives")
    }

    fn count(model: &ShapeModel, mesh: &str) -> usize {
        model.terminals.iter().filter(|t| t.mesh_id == mesh).count()
    }

    #[test]
    fn grammar_parses_and_derives() {
        assert_grammar_parses_and_derives(build_kind(), "palace_range");
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = PalaceRange.build("");
        sanitize_generator(&mut g);
        assert!(
            matches!(
                g.kind,
                GeneratorKind::Cuboid {
                    common: PrimCommon { solid: true, .. },
                    ..
                }
            ),
            "palace_range root must be the solid foundation plinth"
        );
        let GeneratorKind::Shape {
            root_rule,
            materials,
            round_meshes,
            ..
        } = &g.children[0].kind
        else {
            panic!("palace body must remain Shape after sanitise");
        };
        assert_eq!(root_rule, "Lot");
        for slot in [
            "Limestone",
            "Stucco",
            "Red",
            "Plaza",
            "Roof",
            "Timber",
            "Shadow",
        ] {
            assert!(
                materials.contains_key(slot),
                "missing material slot: {slot}"
            );
        }
        assert!(
            round_meshes.iter().any(|id| id == "Colonnette"),
            "the colonnette band lost its round meshing"
        );
    }

    /// The half-turn in `build` is what puts the court and its stair on the
    /// approach side. Without it the item presents a blank back wall.
    #[test]
    fn the_court_is_turned_to_face_the_approach() {
        let g = PalaceRange.build("");
        let child = &g.children[0];
        assert_eq!(
            child.transform.rotation.0,
            [0.0, 1.0, 0.0, 0.0],
            "the grammar child must carry the half-turn about Y"
        );
        // The half-turn maps the corner-origin grammar into negative local
        // space, so the recentring translation is +footprint/2. Only X and Z
        // are ours: `attach` rebases Y out of the sunk plinth frame, and the
        // registry-wide grade guard is what pins that.
        let [tx, _, tz] = child.transform.translation.0;
        assert_eq!(
            [tx, tz],
            [LOT_X / 2.0, LOT_Z / 2.0],
            "the turned child must be recentred by +footprint/2, not -"
        );
    }

    /// Both sides of the battered podium: the range must land inside the
    /// podium's *top*, and the stair must reach it. A talud recedes as it
    /// rises, so anything measured against the podium's base is wrong.
    #[test]
    fn the_range_lands_inside_the_battered_podium() {
        let (inset_x, inset_z) = talud_inset();
        let step = plat_step();
        assert!(
            step > inset_x && step > inset_z,
            "the range is set back {step:.3} but the podium top draws in \
             ({inset_x:.3}, {inset_z:.3}) — the walls would overhang the batter"
        );
        assert!(
            stair_embed() > inset_z,
            "treads are buried {:.3} into a podium whose face recedes \
             {inset_z:.3} — the upper steps would leave daylight behind them",
            stair_embed()
        );
        // A walkway that survives the batter, or the range appears to grow
        // straight out of the podium edge.
        assert!(
            step - inset_x > 0.4,
            "only {:.3} of terrace is left outside the range wall",
            step - inset_x
        );
    }

    /// `ShapeU` must leave a court worth walking into, and the three wings
    /// must be deep enough to hold rooms.
    #[test]
    fn the_u_leaves_a_real_court() {
        let court_x = LOT_X - 2.0 * plat_step() - 2.0 * WING_W;
        let court_z = LOT_Z - STAIR_D - 2.0 * plat_step() - BAR_D;
        assert!(
            court_x > 6.0 && court_z > 6.0,
            "the court came out {court_x:.1} x {court_z:.1} — too small to read as one"
        );
    }

    /// The doorway height falls out of the wall, so it can never overflow —
    /// what still needs checking is that what falls out is a doorway a
    /// person could walk through rather than a slot or a barn door.
    #[test]
    fn the_derived_doorway_is_a_human_opening() {
        let h = door_h();
        assert!(
            (1.9..=2.6).contains(&h),
            "a {LOWER_H} lower wall less a {LINTEL_H} lintel leaves a {h} \
             opening — that is not a doorway"
        );
    }

    /// The elevation must actually be pierced, and carry its mouldings.
    /// "Derives cleanly" is not "has doorways".
    #[test]
    fn the_range_is_pierced_and_moulded() {
        for seed in 0..8_u64 {
            let model = derive_range(seed);
            let rooms = count(&model, "Room");
            assert!(
                rooms >= 12,
                "seed {seed}: the range degraded to blank wall — {rooms} openings"
            );
            assert!(
                count(&model, "Beam") >= 8,
                "seed {seed}: doorways lost their timber lintels"
            );
            // Two mouldings on every face of every wing.
            assert!(
                count(&model, "Moulding") >= 12,
                "seed {seed}: the medial and cornice bands are missing"
            );
            assert!(count(&model, "Step") >= 6, "seed {seed}: the stair is gone");
            assert!(
                count(&model, "Floor") > 0,
                "seed {seed}: the court has no floor"
            );
        }
    }

    /// The frieze is the theme. Whichever scheme `Pick` lands on, the band
    /// must carry real relief — and one scheme only, across the whole palace.
    #[test]
    fn one_frieze_scheme_dresses_the_whole_palace() {
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for seed in 0..12_u64 {
            let model = derive_range(seed);
            let bosses = count(&model, "Boss");
            let colonnettes = count(&model, "Colonnette");
            // Colonnettes and bosses belong to different schemes: a palace
            // showing both means Pick lost coherence between wings.
            assert!(
                bosses == 0 || colonnettes == 0,
                "seed {seed}: the frieze mixed schemes ({bosses} bosses, \
                 {colonnettes} colonnettes) across the range"
            );
            assert!(
                bosses + colonnettes >= 20,
                "seed {seed}: the frieze band is bare — {bosses} bosses, \
                 {colonnettes} colonnettes"
            );
            if colonnettes > 0 {
                seen.insert("colonnette");
            } else {
                seen.insert("boss");
            }
        }
        assert_eq!(
            seen.len(),
            2,
            "12 seeds never produced both a colonnette frieze and a mosaic one"
        );
    }

    /// The crown is the other complex-wide decision: either the palace wears
    /// a roof comb or it does not, and the parapet is there either way.
    #[test]
    fn the_palace_wears_one_crown() {
        let mut with = 0;
        let mut without = 0;
        for seed in 0..12_u64 {
            let model = derive_range(seed);
            assert!(
                count(&model, "Deck") >= 3,
                "seed {seed}: a wing lost its roof deck"
            );
            if count(&model, "Comb") > 0 {
                with += 1;
            } else {
                without += 1;
            }
        }
        assert!(with > 0, "no seed raised a roof comb");
        assert!(without > 0, "every seed raised a roof comb");
    }
}
