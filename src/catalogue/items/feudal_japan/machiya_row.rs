//! Machiya row — a block of Kyoto merchant town-houses, lattice-fronted on
//! the street and running back down their long "eel bed" lots.
//!
//! A machiya is defined by its plan before anything else: a narrow frontage
//! and an absurd depth (*unagi no nedoko*, "eel's bed"), because the street
//! frontage was what got taxed. This grammar takes that literally — every
//! house is a `Split(Z)` into three zones, front to back:
//!
//! 1. the **omoya**, the two-storey street block with the shop in it;
//! 2. the **tsuboniwa**, a gravelled courtyard garden with set stones;
//! 3. the **kura**, a thick-walled fireproof storehouse (on most lots).
//!
//! It is the transpose of the [rowhouse terrace][super::super::modern_city::rowhouse_terrace]:
//! the same "one development, individually fitted out" problem, but where a
//! terrace is all frontage and no depth, a machiya row is all depth. The two
//! items also divide the stochastic labour differently. The terrace shares
//! its *roofline* across the row and varies everything else per house; here
//! `Pick` shares the **roof family** and the **hour of day** — because a
//! street has one roof vocabulary and one sunset — while the per-house rolls
//! take storey height, shop-versus-residence front, lattice pattern, the
//! presence of a fire-wall and the presence of a storehouse.
//!
//! The hero detail is the **koshi**, the fine timber lattice screening the
//! ground floor. In Kyoto the lattice pattern advertised the trade, so it is
//! a per-house roll: a close *itoya* weave, a coarser one, or a braced
//! screen with a mid-rail. It is built as real slats standing off a recessed
//! interior surface — so the gaps read as gaps, and at night the room behind
//! shows through them.
//!
//! Above it sits the **mushikomado**, the "insect cage window": a low loft
//! storey in earthen plaster with fat plastered bars. And on the houses that
//! prospered, an **udatsu** — a plastered fire wall standing up through the
//! eaves at the party line. Not having one was the Edo-period idiom for
//! failing to get ahead in life, which makes it the most honest stochastic
//! element in the catalogue.
//!
//! Footprint 21 × 14.4 — four to five houses across a deep block.

use std::collections::HashMap;

use crate::catalogue::items::util::{attach, footing};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, SovereignMaterialSettings, SovereignTextureConfig,
};
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    FEUDAL_BAND, PAPER_CREAM, PLASTER_WHITE, STONE_GREY, TILE_SLATE, TIMBER_BROWN, TIMBER_DARK,
    paper, plaster, roof_tile, rough_stone, stone, timber,
};

// ── Plan ──────────────────────────────────────────────────────────────────

/// Frontage of the whole block.
const LOT_X: f32 = 21.0;
/// Depth of the lot. The three zones below must sum to exactly this.
const LOT_Z: f32 = 14.4;

/// Depth of the street block (omoya).
const FRONT_D: f32 = 6.6;
/// Depth of the courtyard garden (tsuboniwa) on lots that also have a kura.
const COURT_D: f32 = 3.6;

// ── Massing ───────────────────────────────────────────────────────────────

/// Ground-storey height. Deliberately a constant rather than a per-house
/// roll: it is what puts every hisashi canopy on one line down the street,
/// however different the houses above them are.
const SHOP_H: f32 = 3.0;
/// The three upper storeys a house can roll. `LOFT_H` is the old
/// *tsushi-nikai* crawl loft — too low to stand in, which was the point
/// under sumptuary law; `TALL_H` is the later, franker second storey.
const LOFT_H: f32 = 1.5;
const UPPER_H: f32 = 2.1;
const TALL_H: f32 = 2.7;

/// Height of the Y band the main roof fills.
const ROOF_H: f32 = 1.5;
/// Main roof pitch, degrees. Shallow, as tiled Japanese roofs are.
const ROOF_PITCH_DEG: f32 = 26.0;
/// Eave overhang beyond the footprint.
const ROOF_OVER: f32 = 0.5;

/// Wall thickness. Also the depth of every window reveal, so an opening is
/// exactly as deep as the wall it pierces.
const WALL_D: f32 = 0.24;

/// Clearance added on top of the tuck the roof geometry strictly demands.
///
/// Small on purpose: the band is dead space, so anything beyond what the
/// eaves need opens a visible gap under them.
const TUCK_MARGIN: f32 = 0.02;

// ── Street furniture ──────────────────────────────────────────────────────

/// Height of the hisashi band — the shop canopy over the shopfront.
const HISASHI_H: f32 = 0.42;
/// How far the hisashi projects over the street.
const HISASHI_OUT: f32 = 0.85;

/// Height of the stone plinth a residence stands on, and of its door leaf.
///
/// These two are the deepest absolute stack on the street face, and the
/// face they divide is only `SHOP_H - HISASHI_H` tall to begin with — so
/// they are the pair most likely to overflow when anything above them
/// moves. The `the_entry_stack_fits_the_shopfront` test does that arithmetic
/// rather than leaving it to a derive failure.
const PLINTH_H: f32 = 0.5;
const DOOR_H: f32 = 1.8;

/// Thickness, street-depth and total height of an udatsu fire wall.
///
/// `UDATSU_TOP` clears the eaves of even a `TALL_H` house
/// (`SHOP_H + TALL_H + roof_tuck()`) and stands through the roof of a
/// shorter one, which is what an udatsu is: a party-line fire break that
/// interrupts the roofline rather than tucking under it.
const UDATSU_W: f32 = 0.34;
const UDATSU_D: f32 = 1.7;
const UDATSU_TOP: f32 = 6.7;

// ── Kura and garden ───────────────────────────────────────────────────────

/// Total height and roof band of the rear storehouse.
const KURA_H: f32 = 4.6;
const KURA_ROOF_H: f32 = 1.15;
/// Height of the raked courtyard bed, and the depth of the gravel within
/// it. The difference is the thin band the garden stones are scattered on:
/// they are grown around their scatter point, so they end up straddling the
/// gravel surface rather than sitting on it.
const COURT_H: f32 = 0.2;
const GRAVEL_H: f32 = 0.14;

/// Weights of the two `Pick("hour")` branches, in percent.
///
/// Both `Pick` sites keyed `"hour"` **must** declare these same two weights
/// in this same order: the winning index is a pure function of the seed and
/// the key, so identical weight lists agree and mismatched ones would let
/// the row show lit interiors behind dark paper screens. Formatting both
/// sites from these constants is what keeps that true, and the
/// `the_row_agrees_on_the_hour` test holds them to it.
const HOUR_DAY_PCT: f32 = 54.0;
const HOUR_NIGHT_PCT: f32 = 46.0;

/// The tuck band an overhanging roof of this pitch demands over a wall
/// standing `WALL_D` proud of the mass face.
///
/// A roof descends as it runs outward from its springing plane, so by the
/// time it reaches a proud wall it has already dropped `WALL_D · tan(pitch)`
/// and the wall head pokes through its own eaves.
fn required_roof_tuck() -> f32 {
    WALL_D * ROOF_PITCH_DEG.to_radians().tan()
}

/// The dead band the grammar actually declares between wall head and roof
/// springing.
///
/// Derived rather than authored, so raising `ROOF_PITCH_DEG` widens the band
/// with it instead of silently pushing every wall through the eaves — the
/// failure mode that cost the stave church two rounds of review (#1042).
fn roof_tuck() -> f32 {
    required_roof_tuck() + TUCK_MARGIN
}

// ── Palette ───────────────────────────────────────────────────────────────

/// Indigo-dyed cotton for the noren shop curtain.
const NOREN_INDIGO: [f32; 3] = [0.11, 0.18, 0.33];
/// Raked pale gravel of the tsuboniwa.
const GRAVEL_PALE: [f32; 3] = [0.68, 0.66, 0.60];
/// Ochre earthen plaster of the loft storey — warmer than the kura's lime.
/// Held clear of green: a desaturated yellow goes khaki once the sky light
/// cools its shadow side, and the whole loft storey reads military.
const PLASTER_OCHRE: [f32; 3] = [0.79, 0.68, 0.49];
/// The unlit interior seen through the lattice by day.
const INTERIOR_DARK: [f32; 3] = [0.06, 0.05, 0.045];
/// Lamplight in a shop after dark.
const INTERIOR_WARM: [f32; 3] = [1.0, 0.74, 0.40];

/// A plain surface set behind the lattice to be the room.
///
/// No texture at all: everything the eye reads here is the slat pattern in
/// front of it. Tinting a patterned card warm to fake a lit room is the
/// mistake the terrace documents — a `Window` card is one material across
/// frame *and* glass — and the same reasoning applies to a lattice, where
/// the "frame" is the timber itself and must stay timber-coloured.
fn interior(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.9),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

/// Shoji paper, optionally lit from within. Keeps the theme's woven-fibre
/// [`paper`] surface and only adds the glow, so a lit screen is the same
/// material the tea house uses rather than a flat emissive panel.
fn shoji(glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        emission_color: Fp3(PAPER_CREAM),
        emission_strength: Fp(glow),
        ..paper(PAPER_CREAM)
    }
}

pub struct MachiyaRow;

impl CatalogueEntry for MachiyaRow {
    fn slug(&self) -> &'static str {
        "machiya_row"
    }
    fn name(&self) -> &'static str {
        "Machiya Row"
    }
    fn description(&self) -> &'static str {
        "Block of lattice-fronted merchant town-houses on deep lots, with courtyard gardens and storehouses behind."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// The merchant town, not the farmstead — the destitute end of the theme
    /// stays the [`minka`](super::minka) kit.
    fn prosperity_band(&self) -> ProsperityBand {
        FEUDAL_BAND
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::FeudalJapan]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 10.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        // Centred plinth root; the corner-origin grammar hangs beneath it
        // offset by -footprint/2 so placement yaw turns the block about its
        // middle rather than about its street corner.
        let mut root = footing(LOT_X + 1.0, LOT_Z + 1.0, [0.0, 0.0], 10.0);
        let mut block = Generator::from_kind(build_kind());
        block.transform.translation = Fp3([-LOT_X / 2.0, 0.0, -LOT_Z / 2.0]);
        // `attach`, never a bare child push: `footing` returns a root whose
        // own transform is sunk by half the buried plinth, and a plain child
        // inherits that and sinks the whole block below grade (#1039).
        attach(&mut root, block);
        root
    }
}

/// The palette, keyed by the `Mat("...")` names the grammar emits.
fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = HashMap::new();
    m.insert("Timber".to_string(), timber(TIMBER_BROWN));
    m.insert("TimberDark".to_string(), timber(TIMBER_DARK));
    m.insert("Plaster".to_string(), plaster(PLASTER_OCHRE));
    m.insert("Lime".to_string(), plaster(PLASTER_WHITE));
    m.insert("Tile".to_string(), roof_tile(TILE_SLATE));
    m.insert("Stone".to_string(), stone(STONE_GREY));
    m.insert("Gravel".to_string(), rough_stone(GRAVEL_PALE));
    m.insert("Rock".to_string(), rough_stone(STONE_GREY));
    m.insert("Noren".to_string(), paper(NOREN_INDIGO));
    // The two halves of the hour: paper screens and the rooms behind the
    // lattice, lit or unlit together.
    m.insert("ShojiDay".to_string(), shoji(0.0));
    m.insert("ShojiNight".to_string(), shoji(1.6));
    m.insert("InteriorDay".to_string(), interior(INTERIOR_DARK, 0.0));
    m.insert("InteriorNight".to_string(), interior(INTERIOR_WARM, 2.2));
    m
}

fn build_kind() -> GeneratorKind {
    // The grammar's own constants are formatted from the Rust ones above, so
    // the numbers the rules split on and the numbers `required_roof_tuck`
    // and the tests reason about cannot drift apart.
    let tuck = roof_tuck();
    let declarations = format!(
        "const ShopH = {SHOP_H}\n\
         const LoftH = {LOFT_H}\n\
         const UpperH = {UPPER_H}\n\
         const TallH = {TALL_H}\n\
         const RoofH = {ROOF_H}\n\
         const RoofTuck = {tuck}\n\
         const RoofPitch = {ROOF_PITCH_DEG}\n\
         const RoofOver = {ROOF_OVER}\n\
         const WallD = {WALL_D}\n\
         const HisashiH = {HISASHI_H}\n\
         const HisashiOut = {HISASHI_OUT}\n\
         const PlinthH = {PLINTH_H}\n\
         const DoorH = {DOOR_H}\n\
         const UdatsuW = {UDATSU_W}\n\
         const UdatsuD = {UDATSU_D}\n\
         const UdatsuTop = {UDATSU_TOP}\n\
         const FrontD = {FRONT_D}\n\
         const CourtD = {COURT_D}\n\
         const CourtH = {COURT_H}\n\
         const GravelH = {GRAVEL_H}\n\
         const KuraH = {KURA_H}\n\
         const KuraRoofH = {KURA_ROOF_H}"
    );

    // Both `Pick("hour")` sites, built from one pair of weights.
    let hour_interior = format!(
        "ShopInterior --> Pick(\"hour\") {{ {HOUR_DAY_PCT}% DayRoom | {HOUR_NIGHT_PCT}% NightRoom }}"
    );
    let hour_shoji = format!(
        "ShojiCard --> Pick(\"hour\") {{ {HOUR_DAY_PCT}% ShojiDay | {HOUR_NIGHT_PCT}% ShojiNight }}"
    );

    let rules = [
        // ── 1. The block: cycled frontages, so party walls avoid a metronome
        "Lot --> Repeat(X, [4.6, 4.0, 5.4]) { House }",
        // ── 2. The eel-bed plan: street block, garden, storehouse ──────────
        "House --> Split(Z) { FrontD: FrontLot | ~1: Backland }",
        // Most lots kept a kura at the back; the rest ran the garden the
        // whole way to the alley.
        "Backland --> 64% Split(Z) { CourtD: Courtyard | ~1: RearBlock } | 36% Courtyard",
        // ── 3. Did this house get ahead? ──────────────────────────────────
        //    An udatsu is a plastered fire wall on the party line, and the
        //    Edo idiom for a man who never made anything of himself was that
        //    he "could not raise an udatsu".
        "FrontLot --> 36% UdatsuLot | 64% FrontBlock",
        "UdatsuLot --> Split(X) { UdatsuW: UdatsuStrip | ~1: FrontBlock }",
        // Only the street end of the party line is walled; the rest of the
        // strip vanishes, and the neighbours' eaves close the gap above it.
        "UdatsuStrip --> Split(Z) { UdatsuD: UdatsuFin | ~1: NIL }",
        "UdatsuFin --> Extrude(UdatsuTop) Comp(Faces) { Bottom: NIL | Top: UdatsuCap | _: LimeFace }",
        // Outward `Offset` so the tiled cap oversails the blade on all four
        // sides. Flush with it, the fire wall reads as a capped pier; with
        // the oversail it reads as the little roof an udatsu actually wears.
        "UdatsuCap --> Offset(0.09) { Inside: UdatsuCapSlab }",
        "UdatsuCapSlab --> Extrude(0.14) Mat(\"Tile\") I(\"Cap\")",
        // ── 4. Storey lottery ─────────────────────────────────────────────
        //    Only the height above the shop varies: the ground storey is
        //    fixed, so the canopy line survives the lottery.
        "FrontBlock --> 26% Extrude(ShopH + LoftH + RoofTuck + RoofH) Massing \
                        | 48% Extrude(ShopH + UpperH + RoofTuck + RoofH) Massing \
                        | 26% Extrude(ShopH + TallH + RoofTuck + RoofH) Massing",
        "Massing --> Split(Y) { ShopH: ShopStorey | ~1: LoftStorey | RoofTuck: NIL | RoofH: RoofZone }",
        // ── 5. The shopfront ──────────────────────────────────────────────
        "ShopStorey --> Comp(Faces) { Front: StreetFace | Top: NIL | Bottom: NIL | _: FlankWall }",
        // Boarded skirt (shitami-ita) up to waist height, plaster above.
        // Without it the end of the row is one blank storey-high slab.
        "FlankWall --> Split(Y) { 1.3: FlankSkirt | ~1: FlankPlaster }",
        "FlankSkirt --> Extrude(0.22) Mat(\"TimberDark\") I(\"Skirt\")",
        "FlankPlaster --> Extrude(0.2) Mat(\"Plaster\") I(\"Wall\")",
        "StreetFace --> Split(Y) { ~1: ShopZone | HisashiH: Hisashi }",
        "ShopZone --> when(scope.x < 2.4): PlankWall | else: FrontChoice",
        "FrontChoice --> 58% ShopEntry | 42% HouseEntry",
        // A shop opens its front bay to the street under a noren.
        "ShopEntry --> Split(X) { 0.22: Post | ~1: KoshiRun | 1.5: NorenBay | 0.22: Post }",
        // A residence keeps a stone plinth, more lattice and a small door.
        "HouseEntry --> Split(Y) { PlinthH: Plinth | ~1: HouseFront }",
        "HouseFront --> Split(X) { 0.22: Post | ~1: KoshiRun | 1.05: DoorBay | 0.22: Post }",
        "Plinth --> Extrude(WallD + 0.06) Mat(\"Stone\") I(\"Plinth\")",
        "Post --> Extrude(WallD + 0.05) Mat(\"Timber\") I(\"Post\")",
        // The noren hangs from the lintel across the top of an open bay.
        "NorenBay --> Split(Y) { ~1: ShopOpening | 0.8: Noren }",
        "ShopOpening --> Extrude(WallD) Comp(Faces) { Back: NIL | Front: ShopInterior | _: RevealFace }",
        "Noren --> Extrude(0.06) Mat(\"Noren\") I(\"Noren\")",
        "DoorBay --> Split(Y) { DoorH: DoorLeaf | ~1: Ranma }",
        "DoorLeaf --> Extrude(0.12) Mat(\"TimberDark\") I(\"Door\")",
        "Ranma --> Extrude(0.1) Mat(\"ShojiDay\") I(\"Ranma\")",
        // ── 6. Koshi — the hero ───────────────────────────────────────────
        //    Real slats standing off a recessed room surface, so the gaps
        //    are gaps and the room shows through them after dark.
        "KoshiRun --> Split(Y) { 0.42: Kamachi | ~1: Koshi | 0.3: KoshiHead }",
        "Kamachi --> Extrude(WallD) Mat(\"Timber\") I(\"Sill\")",
        "KoshiHead --> Extrude(WallD) Mat(\"Timber\") I(\"Head\")",
        "Koshi --> Extrude(WallD) Comp(Faces) { Back: KoshiScreen | Front: ShopInterior | _: RevealFace }",
        // The pattern advertised the trade, so it is a per-house roll.
        "KoshiScreen --> 38% FineKoshi | 34% WideKoshi | 28% BracedKoshi",
        "FineKoshi --> Repeat(X, 0.17) { KoshiCell }",
        "WideKoshi --> Repeat(X, 0.27) { KoshiCell }",
        "BracedKoshi --> Split(Y) { ~1: FineKoshi | 0.12: KoshiBrace | ~2: FineKoshi }",
        "KoshiCell --> Split(X) { 0.06: Slat | ~1: NIL }",
        "Slat --> Extrude(0.055) Mat(\"Timber\") I(\"Slat\")",
        "KoshiBrace --> Extrude(0.06) Mat(\"Timber\") I(\"Brace\")",
        // ── 7. The canopy, and its rafter ends ────────────────────────────
        "Hisashi --> Extrude(HisashiOut) Comp(Faces) { Front: NIL | Top: CanopyTiles | Bottom: Soffit | _: CanopyEdge }",
        "CanopyTiles --> Mat(\"Tile\") I(\"Tiles\")",
        "CanopyEdge --> Mat(\"Timber\") I(\"Fascia\")",
        "Soffit --> Repeat(X, 0.42) { RafterCell }",
        "RafterCell --> Split(X) { 0.085: Rafter | ~1: SoffitPanel }",
        "Rafter --> Extrude(0.07) Mat(\"Timber\") I(\"Rafter\")",
        "SoffitPanel --> Mat(\"Timber\") I(\"Soffit\")",
        // ── 8. The loft storey ────────────────────────────────────────────
        // No skirt up here — the boarding is a ground-floor detail.
        "LoftStorey --> Comp(Faces) { Front: LoftFace | Top: NIL | Bottom: NIL | _: FlankPlaster }",
        "LoftFace --> when(scope.x < 2.4): PlasterWall | else: LoftBays",
        // A crawl loft gets insect-cage windows; a full storey gets shoji.
        "LoftBays --> when(scope.y < 1.8): MushikoRow | else: ShojiRow",
        "MushikoRow --> Split(X) { 0.5: PlasterWall | { 0.6: PlasterWall | 1.0: MushikoBay }* | 0.5: PlasterWall }",
        "MushikoBay --> Split(Y) { 0.4: PlasterWall | ~1: Mushikomado | 0.32: PlasterWall }",
        "Mushikomado --> Extrude(WallD) Comp(Faces) { Back: MushikoGrille | Front: LoftInterior | _: RevealFace }",
        // Fat plastered bars, not timber — that is why it is a cage.
        "MushikoGrille --> Repeat(X, 0.2) { MushikoCell }",
        "MushikoCell --> Split(X) { 0.1: FatBar | ~1: NIL }",
        "FatBar --> Extrude(0.08) Mat(\"Plaster\") I(\"Bar\")",
        "ShojiRow --> Split(X) { 0.45: PlasterWall | { 0.5: PlasterWall | 1.25: ShojiBay }* | 0.45: PlasterWall }",
        "ShojiBay --> Split(Y) { 0.55: PlasterWall | ~1: ShojiWindow | 0.45: PlasterWall }",
        "ShojiWindow --> Extrude(WallD) Comp(Faces) { Back: ShojiScreen | Front: LoftInterior | _: RevealFace }",
        // A shoji is a gridded frame, not a sheet. Left as one flat panel it
        // reads as a poster pasted on the wall — the mullions are what make
        // the paper look like paper.
        "ShojiScreen --> Split(Y) { ~1: ShojiTier | 0.05: Mullion | ~1: ShojiTier }",
        "ShojiTier --> Repeat(X, 0.3) { ShojiCell }",
        "ShojiCell --> Split(X) { 0.05: Mullion | ~1: ShojiCard }",
        "Mullion --> Extrude(0.04) Mat(\"Timber\") I(\"Mullion\")",
        "ShojiDay --> Mat(\"ShojiDay\") I(\"Shoji\")",
        "ShojiNight --> Mat(\"ShojiNight\") I(\"Shoji\")",
        // ── 9. The hour — one decision for the whole street ───────────────
        "DayRoom --> Mat(\"InteriorDay\") I(\"Room\")",
        "NightRoom --> Mat(\"InteriorNight\") I(\"Room\")",
        "LoftInterior --> Mat(\"InteriorDay\") I(\"Room\")",
        // ── 10. Roofs — one family for the whole street ───────────────────
        //    `ridge=X` is what makes this a row rather than a terrace of
        //    gable ends: machiya sit hirairi, eaves-side to the street.
        "RoofZone --> Pick(\"street\") { 44% Irimoya | 32% Kirizuma | 24% Yosemune }",
        "Irimoya --> Roof(DutchGable, RoofPitch, tier=0.58, overhang=RoofOver, fascia=0.16, ridge=X) \
                     { Slope: TileFace | GableEnd: GableFace | Fascia: FasciaFace | _: TileFace }",
        "Kirizuma --> Roof(Gable, RoofPitch, overhang=RoofOver, fascia=0.16, ridge=X) \
                      { Slope: TileFace | GableEnd: GableFace | Fascia: FasciaFace | _: TileFace }",
        "Yosemune --> Roof(Hip, RoofPitch, overhang=RoofOver, fascia=0.16, ridge=X) \
                      { Slope: TileFace | Fascia: FasciaFace | _: TileFace }",
        "TileFace --> Mat(\"Tile\") I(\"Tiles\")",
        // White lime, not the wall ochre. A gable tympanum in the same
        // colour as the storey below reads as a hole punched in the
        // roofline; in white it reads as the plastered panel it is.
        "GableFace --> Mat(\"Lime\") I(\"Gable\")",
        "FasciaFace --> Mat(\"TimberDark\") I(\"Fascia\")",
        // ── 11. The courtyard garden ──────────────────────────────────────
        //    Raked gravel with a few set stones. `Scatter` drops point
        //    scopes on the bed; `Size` + `Center(XYZ)` grows each stone
        //    around its point, half-buried, so none of them perch.
        "Courtyard --> Extrude(CourtH) Split(Y) { GravelH: CourtBed | ~1: Planting }",
        "CourtBed --> Mat(\"Gravel\") I(\"Gravel\")",
        "Planting --> Scatter(Top, 3) { GardenStone }",
        "GardenStone --> Size(0.52, 0.4, 0.46) Center(XYZ) Mat(\"Rock\") I(\"Stone\")",
        // ── 12. The kura ──────────────────────────────────────────────────
        //    Lime-plastered, near windowless, with a stone base course —
        //    the point of it was to survive the fire that took the house.
        "RearBlock --> Extrude(KuraH) Split(Y) { ~1: KuraWalls | RoofTuck: NIL | KuraRoofH: KuraRoof }",
        "KuraWalls --> Comp(Faces) { Top: NIL | Bottom: NIL | Front: KuraFace | _: KuraWall }",
        "KuraWall --> Split(Y) { 0.85: KuraBase | ~1: LimeFace }",
        "KuraBase --> Extrude(WallD + 0.05) Mat(\"Stone\") I(\"Base\")",
        "LimeFace --> Extrude(WallD) Mat(\"Lime\") I(\"Wall\")",
        "KuraFace --> when(scope.x < 2.2): KuraWall | else: KuraDoorway",
        "KuraDoorway --> Split(X) { ~1: KuraWall | 1.3: KuraDoorBay | ~1: KuraWall }",
        "KuraDoorBay --> Split(Y) { 2.0: KuraDoor | ~1: LimeFace }",
        "KuraDoor --> Extrude(0.18) Mat(\"TimberDark\") I(\"Door\")",
        "KuraRoof --> Roof(Hip, RoofPitch, overhang=0.42, fascia=0.14, ridge=X) \
                      { Slope: TileFace | Fascia: FasciaFace | _: TileFace }",
        // ── 13. Shared terminals ──────────────────────────────────────────
        "PlankWall --> Extrude(WallD) Mat(\"Timber\") I(\"Wall\")",
        "PlasterWall --> Extrude(WallD) Mat(\"Plaster\") I(\"Wall\")",
        "RevealFace --> Mat(\"Timber\") I(\"Reveal\")",
    ];

    let grammar_source = std::iter::once(declarations.as_str())
        .chain(std::iter::once(hour_interior.as_str()))
        .chain(std::iter::once(hour_shoji.as_str()))
        .chain(rules)
        .collect::<Vec<_>>()
        .join("\n");

    GeneratorKind::Shape {
        grammar_source,
        root_rule: "Lot".to_string(),
        footprint: Fp3([LOT_X, 0.0, LOT_Z]),
        seed: 3,
        materials: materials(),
        // Nothing here is turned — a machiya is all square timber.
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

    /// Derives the block at `seed` through the same statement path the
    /// runtime uses.
    fn derive_row(seed: u64) -> ShapeModel {
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
            .expect("machiya row derives")
    }

    fn count(model: &ShapeModel, mesh: &str) -> usize {
        model.terminals.iter().filter(|t| t.mesh_id == mesh).count()
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
        assert_grammar_parses_and_derives(build_kind(), "machiya_row");
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = MachiyaRow.build("");
        sanitize_generator(&mut g);
        assert!(
            matches!(
                g.kind,
                GeneratorKind::Cuboid {
                    common: PrimCommon { solid: true, .. },
                    ..
                }
            ),
            "machiya_row root must be the solid foundation plinth"
        );
        let GeneratorKind::Shape {
            root_rule,
            materials,
            ..
        } = &g.children[0].kind
        else {
            panic!("machiya body must remain Shape after sanitise");
        };
        assert_eq!(root_rule, "Lot");
        for slot in [
            "Timber",
            "TimberDark",
            "Plaster",
            "Lime",
            "Tile",
            "Stone",
            "Gravel",
            "Rock",
            "Noren",
            "ShojiDay",
            "ShojiNight",
            "InteriorDay",
            "InteriorNight",
        ] {
            assert!(
                materials.contains_key(slot),
                "missing material slot: {slot}"
            );
        }
    }

    /// The three zones must tile the lot exactly, or the kura hangs off the
    /// back of the plinth.
    #[test]
    fn the_lot_zones_tile_the_lot() {
        let deepest_plan = FRONT_D + COURT_D;
        assert!(
            deepest_plan < LOT_Z,
            "the front block and courtyard already fill the lot — no room for a kura"
        );
        assert!(
            LOT_Z - deepest_plan > 3.0,
            "the kura zone is too shallow to hold a storehouse and its roof"
        );
    }

    /// The tuck band exists for exactly one reason: a wall standing `WALL_D`
    /// proud of the mass would otherwise pierce its own overhanging eaves.
    /// Assert it covers that and is not padded far beyond it — a fat band
    /// would open a visible gap under the roof instead.
    #[test]
    fn the_tuck_band_clears_the_wall_head() {
        let required = required_roof_tuck();
        let declared = roof_tuck();
        assert!(
            declared > required,
            "walls stand {WALL_D} proud under a {ROOF_PITCH_DEG}° roof, \
             which needs more than a {required:.3} tuck — the grammar \
             declares only {declared:.3}"
        );
        assert!(
            declared < required + 0.05,
            "the declared tuck {declared:.3} is far above the {required:.3} \
             it needs — that opens a gap under the eaves"
        );
    }

    /// The street face is the tightest budget in the grammar: the canopy,
    /// the plinth and the door leaf are all absolute, and they divide a face
    /// only `SHOP_H` tall. Anything that grows here overflows the split
    /// rather than degrading, so do the arithmetic here — a derive failure
    /// names the scope but not the constant that outgrew it.
    #[test]
    fn the_entry_stack_fits_the_shopfront() {
        let shop_zone = SHOP_H - HISASHI_H;
        let house_front = shop_zone - PLINTH_H;
        assert!(
            DOOR_H < house_front,
            "a {DOOR_H} door leaf cannot fit the {house_front} left under the \
             plinth and canopy"
        );
        assert!(
            house_front - DOOR_H > 0.2,
            "the ranma transom above the door is thinner than 0.2 — \
             it will read as a seam, not an opening"
        );
    }

    /// The lattice is the item. Assert the street actually grows slats, and
    /// that several distinct houses grew them — "derives cleanly" is not
    /// "has a shopfront".
    #[test]
    fn the_street_is_latticed() {
        for seed in 0..8_u64 {
            let model = derive_row(seed);
            let slats = count(&model, "Slat");
            assert!(
                slats >= 30,
                "seed {seed}: the shopfronts degraded to blank wall — only {slats} lattice slats"
            );
            // Slats cluster into runs, one per house front. Distinct X bands
            // ⇒ more than one house grew a lattice.
            let mut runs: Vec<i64> = model
                .terminals
                .iter()
                .filter(|t| t.mesh_id == "Slat")
                .map(|t| (t.scope.position.x / 2.0).floor() as i64)
                .collect();
            runs.sort_unstable();
            runs.dedup();
            assert!(
                runs.len() >= 3,
                "seed {seed}: lattice found in only {} bands of the block",
                runs.len()
            );
        }
    }

    /// The loft storey must be pierced too, by one kind of opening or the
    /// other, and the shop canopy must run the whole street.
    #[test]
    fn every_house_is_capped_and_pierced_above_the_shop() {
        for seed in 0..8_u64 {
            let model = derive_row(seed);
            let loft_openings = count(&model, "Bar") + count(&model, "Shoji");
            assert!(
                loft_openings >= 6,
                "seed {seed}: the loft storey is blank — {loft_openings} openings"
            );
            assert!(
                count(&model, "Rafter") >= 20,
                "seed {seed}: the hisashi canopy lost its rafter ends"
            );
            assert!(
                count(&model, "Tiles") >= 5,
                "seed {seed}: houses are missing their roofs"
            );
        }
    }

    /// `Pick` is the coherence mechanism: one roof family and one hour for
    /// the whole street, however different the houses are.
    #[test]
    fn the_row_shares_one_roof_family() {
        for seed in 0..10_u64 {
            let model = derive_row(seed);
            // A gable end only exists under Irimoya/Kirizuma; the hipped
            // Yosemune has none. Either the whole row has them or none does.
            let gables = count(&model, "Gable");
            let houses = count(&model, "Tiles");
            assert!(houses > 0, "seed {seed}: no roofs at all");
            assert!(
                gables == 0 || gables >= 4,
                "seed {seed}: {gables} gable ends across the row — \
                 Pick lost coherence and mixed roof families"
            );
        }
    }

    /// Both `Pick("hour")` sites are formatted from one pair of weights, so
    /// the rooms behind the lattice and the paper screens above them must
    /// always agree. A mismatch here means the weights drifted apart.
    #[test]
    fn the_row_agrees_on_the_hour() {
        let mut saw_night = false;
        let mut saw_day = false;
        for seed in 0..12_u64 {
            let model = derive_row(seed);
            let rooms = mats_of(&model, "Room");
            let screens = mats_of(&model, "Shoji");
            let night_rooms = rooms.iter().filter(|m| **m == "InteriorNight").count();
            let night_screens = screens.iter().filter(|m| **m == "ShojiNight").count();
            // `LoftInterior` is always the day material, so a lit row shows
            // *some* night rooms rather than all of them.
            if night_rooms > 0 {
                saw_night = true;
                assert!(
                    screens.is_empty() || night_screens > 0,
                    "seed {seed}: rooms are lit but every paper screen is dark"
                );
            } else {
                saw_day = true;
                assert_eq!(
                    night_screens, 0,
                    "seed {seed}: screens are lit but no room behind the lattice is"
                );
            }
        }
        assert!(saw_night, "no seed lit the street — the hour Pick is stuck");
        assert!(
            saw_day,
            "no seed left the street dark — the hour Pick is stuck"
        );
    }

    /// Some houses raise an udatsu and some do not — and when one is raised
    /// it must stand above the eaves of even the tallest neighbour, or it
    /// reads as a buttress rather than a fire wall.
    #[test]
    fn the_prosperous_houses_raise_a_fire_wall() {
        let tallest_eave = SHOP_H + TALL_H + roof_tuck();
        assert!(
            UDATSU_TOP > tallest_eave + 0.5,
            "UDATSU_TOP {UDATSU_TOP} barely clears the {tallest_eave} eave line"
        );
        let mut with = 0;
        let mut without = 0;
        for seed in 0..12_u64 {
            let model = derive_row(seed);
            if count(&model, "Cap") > 0 {
                with += 1;
            } else {
                without += 1;
            }
        }
        assert!(with > 0, "no seed raised a single udatsu");
        assert!(without > 0 || with < 12, "every seed raised an udatsu");
    }

    /// The courtyard must be gravelled and stoned — `Scatter` silently
    /// producing nothing would leave a bald pad no test would otherwise see.
    #[test]
    fn the_courtyards_are_planted() {
        let model = derive_row(3);
        assert!(count(&model, "Gravel") >= 3, "courtyard beds are missing");
        assert!(
            count(&model, "Stone") >= 6,
            "no garden stones were scattered"
        );
        // Half-buried, not perched: each stone straddles the bed surface.
        for t in model.terminals.iter().filter(|t| t.mesh_id == "Stone") {
            assert!(
                t.scope.position.y < COURT_H as f64,
                "a garden stone was seated on top of the bed instead of set into it"
            );
        }
    }
}
