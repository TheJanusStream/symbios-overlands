//! Craftsman bungalow — the early-1900s pattern-book house: one-and-a-half
//! storeys under a broad low gable, a deep porch on battered piers, and a
//! gabled dormer in the attic.
//!
//! The suburban kit previously jumped straight from the mid-century
//! [`suburban_house`](super::suburban_house) to the civic buildings; this
//! fills the older, leafier end of the street. It is also the massing-lottery
//! item of the grammar set: where the rows vary *surfaces* and the palace
//! varies *bays*, the bungalow varies its **shape** — storey count, dormer,
//! chimney and garage wing are all per-house rolls, under three `Pick` keys
//! that keep the parts of one house agreeing with each other:
//!
//! - `Pick("roof")` binds the house roof, the garage roof *and* the dormer
//!   gate to one family (gable / hip / jerkinhead). A hip house grows a hip
//!   garage and — like its real counterparts — no dormer at all.
//! - `Pick("paint")` binds the house and the garage to one siding colour,
//!   carried by `Mat` inheritance so every un-named wall terminal shares it.
//! - the porch roof is always a low hip: craftsman porches took either form,
//!   and a hip seals its own side triangles where a shed leaves them open.
//!
//! **The dormer and chimney ride an overlap layer, not `Attach`.** `Attach`
//! re-erects its plane in *world* axes — it discards the slope's own frame —
//! so on a front roof slope the extrusion runs into the roof but on the back
//! slope it runs off the eave into mid-air, and the mirrored panel X leaves
//! no slope-agnostic correction (recorded on the issue as a 0.4 wishlist
//! item). Instead the attic splits off a thin sliver whose two slots each
//! `Size` themselves back to the full attic volume — `Size` is absolute and
//! unclamped, so the slots stop being a partition. One regrown slot roofs
//! the attic; the other places the dormer and chimney as massing boxes that
//! simply punch through the roof shell. Every emerge / die-into / sill
//! number is then derived from the pitch menu, the same way the machiya
//! derives its roof tuck.
//!
//! Footprint 14 × 12: house plot plus a side plot that rolls a garage or
//! stays a yard gap.

use std::collections::HashMap;

use crate::catalogue::items::util::{attach, footing};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, SovereignMaterialSettings, SovereignTextureConfig,
};
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRICK_TAN, GLASS_TINT, PORCH_WARM, ROOF_GREY, SIDING_BLUE, SIDING_CREAM, SIDING_SAGE, SUB_BAND,
    WOOD_BROWN, WOOD_WHITE, brick, concrete, enamel, glass, shingle, siding, wood,
};

// ── Plan ──────────────────────────────────────────────────────────────────

/// Footprint of the whole lot: the house plot plus the side plot.
const LOT_X: f32 = 14.0;
const LOT_Z: f32 = 12.0;

/// Width of the house plot; the rest is the side plot the garage rolls on.
const HOUSE_W: f32 = 10.4;
/// Depth of the porch strip (stair apron included); the core fills the rest.
const PORCH_D: f32 = 2.4;
/// Depth of the stair apron at the front of the porch strip.
const APRON_D: f32 = 0.9;

// ── Massing ───────────────────────────────────────────────────────────────

/// Ground-storey height, and the knee band a storey-and-a-half house adds.
const GROUND_H: f32 = 3.0;
const KNEE_H: f32 = 0.9;
/// Nominal attic band; the roof's real height comes from footprint × pitch.
const ATTIC_H: f32 = 2.2;
/// Wall thickness, and the depth of every window reveal.
const WALL_D: f32 = 0.24;
/// Brick base course at the foot of every wall.
const BASE_H: f32 = 0.4;
/// Clear height of the front door, above the deck it opens onto.
const DOOR_H: f32 = 1.95;

// ── The roof menu ─────────────────────────────────────────────────────────

/// The three roof families a house can draw, in degrees. `Pick("roof")`
/// weights below; the jerkinhead is the steepest, so every proud-wall and
/// sill derivation uses it as the worst case.
const GABLE_PITCH: f32 = 32.0;
const HIP_PITCH: f32 = 29.0;
const JERK_PITCH: f32 = 33.0;
/// Weights of the three `Pick("roof")` branches, in percent. Three sites
/// share this key — house roof, dormer gate, garage roof — and the winner is
/// a pure function of (seed, key) only while their weight lists are
/// identical, so all three are formatted from these constants.
const ROOF_GABLE_PCT: f32 = 50.0;
const ROOF_HIP_PCT: f32 = 27.0;
const ROOF_JERK_PCT: f32 = 23.0;
/// Deep craftsman eaves, and the exposed-fascia band under them.
const ROOF_OVER: f32 = 0.6;
const FASCIA_H: f32 = 0.18;

/// Weights of the two `Pick("paint")` branches trio, in percent — one siding
/// colour binding house and garage. Two sites, same rule as the roof key.
const PAINT_BLUE_PCT: f32 = 34.0;
const PAINT_CREAM_PCT: f32 = 33.0;
const PAINT_SAGE_PCT: f32 = 33.0;

// ── The overlap layer: dormer and chimney ─────────────────────────────────

/// Height of the sliver slot whose two halves regrow to the full attic.
const SLIVER_H: f32 = 0.08;
/// Front set-back of the dormer face from the attic front, its depth, the
/// height of its body, and its width.
const DORMER_SET: f32 = 0.6;
const DORMER_D: f32 = 3.2;
const DORMER_BODY_H: f32 = 1.3;
const DORMER_W: f32 = 2.9;
/// Pitch of the dormer's own front-facing gable cap.
const DORMER_PITCH: f32 = 28.0;
/// Chimney stack: plan, total height above the attic base, and cap.
const CHIM_W: f32 = 0.7;
const CHIM_D: f32 = 0.9;
const CHIM_TOP: f32 = 3.4;
const CHIM_CAP_H: f32 = 0.14;

// ── Porch ─────────────────────────────────────────────────────────────────

/// The porch stack, deck to roof: deck slab, open colonnade, exposed rafter
/// tails, and the low hipped roof band.
const DECK_H: f32 = 0.45;
const RAFTER_H: f32 = 0.18;
const PORCH_ROOF_H: f32 = 0.5;
const PORCH_PITCH: f32 = 14.0;
/// Battered porch piers on brick pedestals, between brick knee walls.
const COL_W: f32 = 0.42;
const PED_H: f32 = 0.7;
const PIER_BATTER: f32 = 0.42;
const KNEE_WALL_H: f32 = 0.5;
/// Front steps: width and rise per tread.
const STEP_W: f32 = 2.2;
const STEP_H: f32 = 0.225;

// ── Garage wing ───────────────────────────────────────────────────────────

/// The garage on the side plot: front set-back (matching the porch), depth,
/// body height, and its door.
const GAR_SET: f32 = 2.4;
const GAR_D: f32 = 6.4;
const GAR_H: f32 = 2.7;
const GDOOR_W: f32 = 2.7;
const GDOOR_H: f32 = 2.1;

// ── Derived geometry ──────────────────────────────────────────────────────

/// The dead band a proud wall needs under the steepest overhanging roof in
/// the menu — the machiya lesson, derived rather than authored.
fn roof_tuck() -> f32 {
    WALL_D * JERK_PITCH.to_radians().tan() + 0.02
}

/// The dormer window sill, measured from the dormer body's base.
///
/// The main slope crosses the dormer face at `DORMER_SET · tan(pitch)` above
/// the attic base; the sill must clear that crossing at the steepest frontal
/// pitch or the window's lower panes are buried in the roof.
fn dormer_sill() -> f32 {
    DORMER_SET * JERK_PITCH.to_radians().tan() - SLIVER_H + 0.11
}

// ── Palette ───────────────────────────────────────────────────────────────

/// Concrete for steps and the chimney cap.
const CONCRETE_GREY: [f32; 3] = [0.62, 0.62, 0.60];
/// The unlit interior behind a dark sash.
const ROOM_DARK: [f32; 3] = [0.07, 0.07, 0.09];

/// A plain surface set behind a sash card to be the room. Untextured: a
/// `Window` card is one material across frame and glass, so the light comes
/// from this backing, never from tinting the card warm (#1040).
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

pub struct CraftsmanBungalow;

impl CatalogueEntry for CraftsmanBungalow {
    fn slug(&self) -> &'static str {
        "craftsman_bungalow"
    }
    fn name(&self) -> &'static str {
        "Craftsman Bungalow"
    }
    fn description(&self) -> &'static str {
        "Low-gabled bungalow with a deep porch on battered piers, an attic dormer and a rolled garage wing."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// The kept end of the street — the destitute end of the theme stays the
    /// trailer-lot kit.
    fn prosperity_band(&self) -> ProsperityBand {
        SUB_BAND
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        // Centred plinth root; the corner-origin grammar hangs beneath it
        // offset by -footprint/2 so placement yaw turns the lot about its
        // middle. The porch faces low Z, which is the approach side.
        let mut root = footing(LOT_X + 1.0, LOT_Z + 1.0, [0.0, 0.0], 9.0);
        let mut house = Generator::from_kind(build_kind());
        house.transform.translation = Fp3([-LOT_X / 2.0, 0.0, -LOT_Z / 2.0]);
        // `attach`, never a bare push: `footing` returns a root already sunk
        // by half the buried plinth, and a plain child inherits that and
        // drops the whole lot below grade (#1039).
        attach(&mut root, house);
        root
    }
}

/// The palette, keyed by the `Mat("...")` names the grammar emits. The three
/// siding slots are what `Pick("paint")` chooses between.
fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = HashMap::new();
    m.insert("SidingBlue".to_string(), siding(SIDING_BLUE));
    m.insert("SidingCream".to_string(), siding(SIDING_CREAM));
    m.insert("SidingSage".to_string(), siding(SIDING_SAGE));
    // Trim — always named explicitly, so it never inherits the siding.
    m.insert("Trim".to_string(), wood(WOOD_WHITE));
    m.insert("Wood".to_string(), wood(WOOD_BROWN));
    m.insert("Brick".to_string(), brick(BRICK_TAN));
    m.insert("Shingle".to_string(), shingle(ROOF_GREY));
    m.insert("Concrete".to_string(), concrete(CONCRETE_GREY));
    m.insert("Enamel".to_string(), enamel(WOOD_WHITE));
    // Glazing is two surfaces: a cool neutral card in the reveal and a plain
    // emissive (or dark) room behind it.
    m.insert("Sash".to_string(), glass(GLASS_TINT, 0.0));
    m.insert("RoomLit".to_string(), interior(PORCH_WARM, 2.2));
    m.insert("RoomUnlit".to_string(), interior(ROOM_DARK, 0.0));
    m
}

fn build_kind() -> GeneratorKind {
    // The grammar's constants are formatted from the Rust ones, so the
    // numbers the rules split on and the numbers the derivations and tests
    // reason about cannot drift apart.
    let declarations = format!(
        "const HouseW = {HOUSE_W}\n\
         const PorchD = {PORCH_D}\n\
         const ApronD = {APRON_D}\n\
         const GroundH = {GROUND_H}\n\
         const KneeH = {KNEE_H}\n\
         const AtticH = {ATTIC_H}\n\
         const Tuck = {tuck}\n\
         const WallD = {WALL_D}\n\
         const BaseH = {BASE_H}\n\
         const DoorH = {DOOR_H}\n\
         const GablePitch = {GABLE_PITCH}\n\
         const HipPitch = {HIP_PITCH}\n\
         const JerkPitch = {JERK_PITCH}\n\
         const RoofOver = {ROOF_OVER}\n\
         const FasciaH = {FASCIA_H}\n\
         const SliverH = {SLIVER_H}\n\
         const DormerSet = {DORMER_SET}\n\
         const DormerD = {DORMER_D}\n\
         const DormerBodyH = {DORMER_BODY_H}\n\
         const DormerW = {DORMER_W}\n\
         const DormerPitch = {DORMER_PITCH}\n\
         const DormerSill = {sill}\n\
         const ChimW = {CHIM_W}\n\
         const ChimD = {CHIM_D}\n\
         const ChimTop = {CHIM_TOP}\n\
         const ChimCapH = {CHIM_CAP_H}\n\
         const DeckH = {DECK_H}\n\
         const RafterH = {RAFTER_H}\n\
         const PorchRoofH = {PORCH_ROOF_H}\n\
         const PorchPitch = {PORCH_PITCH}\n\
         const ColW = {COL_W}\n\
         const PedH = {PED_H}\n\
         const PierBatter = {PIER_BATTER}\n\
         const KneeWallH = {KNEE_WALL_H}\n\
         const StepW = {STEP_W}\n\
         const StepH = {STEP_H}\n\
         const GarSet = {GAR_SET}\n\
         const GarD = {GAR_D}\n\
         const GarH = {GAR_H}\n\
         const GDoorW = {GDOOR_W}\n\
         const GDoorH = {GDOOR_H}",
        tuck = roof_tuck(),
        sill = dormer_sill(),
    );

    // The shared-key Pick sites, each built from one set of weights.
    let paint_house = format!(
        "HousePlot --> Pick(\"paint\") {{ {PAINT_BLUE_PCT}% BlueHouse | {PAINT_CREAM_PCT}% CreamHouse | {PAINT_SAGE_PCT}% SageHouse }}"
    );
    let paint_garage = format!(
        "GaragePaint --> Pick(\"paint\") {{ {PAINT_BLUE_PCT}% BlueGarage | {PAINT_CREAM_PCT}% CreamGarage | {PAINT_SAGE_PCT}% SageGarage }}"
    );
    let roof_house = format!(
        "RoofPick --> Pick(\"roof\") {{ {ROOF_GABLE_PCT}% GableRoof | {ROOF_HIP_PCT}% HipRoof | {ROOF_JERK_PCT}% JerkRoof }}"
    );
    // The dormer gate: on the hip branch the side hip planes would eat the
    // cap corners, and hip bungalows classically went dormerless anyway.
    let roof_dormer = format!(
        "DormerGate --> Pick(\"roof\") {{ {ROOF_GABLE_PCT}% DormerRoll | {ROOF_HIP_PCT}% NIL | {ROOF_JERK_PCT}% DormerRoll }}"
    );
    let roof_garage = format!(
        "GarageRoofZone --> Pick(\"roof\") {{ {ROOF_GABLE_PCT}% GarGable | {ROOF_HIP_PCT}% GarHip | {ROOF_JERK_PCT}% GarJerk }}"
    );

    let rules = [
        // ── 1. The lot: house plot and side plot ──────────────────────────
        "Lot --> Split(X) { HouseW: HousePlot | ~1: SidePlot }",
        "BlueHouse --> Mat(\"SidingBlue\") House",
        "CreamHouse --> Mat(\"SidingCream\") House",
        "SageHouse --> Mat(\"SidingSage\") House",
        // ── 2. The house: porch strip, then the core ──────────────────────
        "House --> Split(Z) { PorchD: PorchStrip | ~1: CoreZone }",
        // Storey lottery: a low single storey, or the storey-and-a-half.
        "CoreZone --> 42% Extrude(GroundH + Tuck + AtticH) OneStorey \
                      | 58% Extrude(GroundH + KneeH + Tuck + AtticH) StoreyAndHalf",
        "OneStorey --> Split(Y) { GroundH: GroundStorey | Tuck: TuckBand | ~1: AtticZone }",
        "StoreyAndHalf --> Split(Y) { GroundH: GroundStorey | KneeH: KneeBand | Tuck: TuckBand | ~1: AtticZone }",
        "KneeBand --> Comp(Faces) { Top: NIL | Bottom: NIL | _: Wall }",
        // The tuck band is dead height under the roof springing, but it
        // must not be dead GEOMETRY: left NIL it is an open slit into the
        // attic void, hidden on the eave sides by the descending overhang
        // but staring straight out under every gable-end tympanum — the
        // "holes in the roof" of the first validation pass, glowing with
        // the far windows' room backings. Flush faces (no Extrude) close
        // it and are exactly tangent to the spring plane, so they cannot
        // pierce the roof the way a proud wall would.
        "TuckBand --> Comp(Faces) { Top: NIL | Bottom: NIL | _: TuckFace }",
        "TuckFace --> I(\"Tuck\")",
        // ── 3. Ground storey: brick base course under sided walls ─────────
        "GroundStorey --> Comp(Faces) { Front: FrontFace | Back: RearFace | Top: NIL | Bottom: NIL | _: SideFace }",
        // The door column owns its full height: banding the base course
        // first and splitting the door out of the wall above it stacks the
        // door on TOP of the base offset, floating the leaf 0.4 above the
        // deck it opens onto.
        "FrontFace --> Split(X) { ~1: FrontSide | 1.3: DoorStack | ~1: FrontSide }",
        "FrontSide --> Split(Y) { BaseH: BaseCourse | ~1: FrontBay }",
        "DoorStack --> Split(Y) { DeckH: BaseCourse | DoorH: FrontDoor | ~1: Wall }",
        "SideFace --> Split(Y) { BaseH: BaseCourse | ~1: SideWall }",
        "RearFace --> Split(Y) { BaseH: BaseCourse | ~1: RearWall }",
        "BaseCourse --> Extrude(WallD + 0.03) Mat(\"Brick\") I(\"Base\")",
        "FrontBay --> when(scope.x < 1.9): Wall | else: FrontWindowBay",
        // Shorter than the side sash: the head band must clear the porch
        // roof structure in front of it, or the window tops hide behind
        // the rafter band.
        "FrontWindowBay --> Split(X) { 0.3: Wall | ~1: FrontSashFrame | 0.3: Wall }",
        "FrontSashFrame --> Split(Y) { 0.75: Wall | ~1: TrimmedSash | 0.65: Wall }",
        "FrontDoor --> Extrude(0.14) Mat(\"Wood\") I(\"Door\")",
        "SideWall --> when(scope.x < 2.4): Wall \
                      | else: Split(X) { 0.6: Wall | { 1.7: Wall | 1.3: WindowBay }* | 0.6: Wall }",
        "RearWall --> when(scope.x < 2.4): Wall \
                      | else: Split(X) { 0.7: Wall | { 1.9: Wall | 1.4: WindowBay }* | 0.7: Wall }",
        // ── 4. Windows: white trim surround, cool card, lit-or-dark room ──
        "WindowBay --> Split(X) { 0.3: Wall | ~1: SashFrame | 0.3: Wall }",
        "SashFrame --> Split(Y) { 0.8: Wall | ~1: TrimmedSash | 0.45: Wall }",
        "TrimmedSash --> Split(X) { 0.09: TrimBand | ~1: SashCore | 0.09: TrimBand }",
        "SashCore --> Split(Y) { 0.09: TrimBand | ~1: Opening | 0.09: TrimBand }",
        "TrimBand --> Extrude(WallD + 0.04) Mat(\"Trim\") I(\"Trim\")",
        "Opening --> Extrude(WallD) Comp(Faces) { Back: SashCard | Front: RoomFace | _: RevealFace }",
        "SashCard --> Mat(\"Sash\") I(\"Pane\")",
        "RoomFace --> 46% LitRoom | 54% DarkRoom",
        "LitRoom --> Mat(\"RoomLit\") I(\"Room\")",
        "DarkRoom --> Mat(\"RoomUnlit\") I(\"Room\")",
        "RevealFace --> Mat(\"Trim\") I(\"Reveal\")",
        // ── 5. The attic: the overlap layer ───────────────────────────────
        //    Two slots that each `Size` back to the full attic volume, so
        //    they stop being a partition: the first roofs the attic, the
        //    second furnishes the roof.
        "AtticZone --> Split(Y) { SliverH: RoofHost | ~1: Furniture }",
        "RoofHost --> Size(scope.x, AtticH, scope.z) RoofPick",
        "GableRoof --> Roof(Gable, GablePitch, overhang=RoofOver, fascia=FasciaH, ridge=X) \
                       { Slope: Shingles | GableEnd: GableWall | Fascia: FasciaTrim | _: Shingles }",
        "HipRoof --> Roof(Hip, HipPitch, overhang=RoofOver, fascia=FasciaH, ridge=X) \
                     { Slope: Shingles | Fascia: FasciaTrim | _: Shingles }",
        "JerkRoof --> Roof(Jerkinhead, JerkPitch, tier=0.28, overhang=RoofOver, fascia=FasciaH, ridge=X) \
                      { Slope: Shingles | GableEnd: GableWall | HipEnd: Shingles | Fascia: FasciaTrim | _: Shingles }",
        "Shingles --> Mat(\"Shingle\") I(\"Shingles\")",
        // The gable tympanum inherits the house siding — the machiya lesson
        // in reverse: here the wall colour IS the correct panel colour.
        "GableWall --> I(\"Gable\")",
        "FasciaTrim --> Mat(\"Trim\") I(\"Fascia\")",
        // ── 6. Roof furniture: chimney strip and dormer strip ─────────────
        "Furniture --> Split(X) { 1.2: NIL | ChimW: ChimStrip | 1.85: NIL | DormerW: DormerStrip | ~1: NIL }",
        "ChimStrip --> 62% ChimPlace | 38% NIL",
        "ChimPlace --> Split(Z) { 2.6: NIL | ChimD: ChimBox | ~1: NIL }",
        // A solid brick stack regrown past the ridge, capped in concrete
        // that oversails it (the udatsu-cap lesson).
        "ChimBox --> Size(scope.x, ChimTop, scope.z) ChimStack",
        "ChimStack --> Split(Y) { ~1: ChimShaft | ChimCapH: ChimCap }",
        "ChimShaft --> Mat(\"Brick\") I(\"Chimney\")",
        "ChimCap --> Size(scope.x + 0.24, scope.y, scope.z + 0.24) Center(XZ) Mat(\"Concrete\") I(\"ChimneyCap\")",
        "DormerStrip --> DormerGate",
        "DormerRoll --> 62% GableDormer | 38% NIL",
        "GableDormer --> Split(Z) { DormerSet: NIL | DormerD: DormerBox | ~1: NIL }",
        "DormerBox --> Split(Y) { DormerBodyH: DormerBody | ~1: DormerCap }",
        // The cap is a front-gabled ridge running out of the main slope.
        // Overhang stays tiny: the gable-end panels sit at the zone ends
        // ± overhang, and a deep one would float the front tympanum clear
        // of the dormer face below it.
        "DormerCap --> Roof(Gable, DormerPitch, overhang=0.02, fascia=0.1, ridge=Z) \
                       { Slope: Shingles | GableEnd: GableWall | Fascia: FasciaTrim | _: NIL }",
        "DormerBody --> Comp(Faces) { Front: DormerFront | Left: DormerCheek | Right: DormerCheek | _: NIL }",
        "DormerCheek --> I(\"Wall\")",
        // The sill is derived: the main slope crosses this face DormerSill
        // below nothing — see `dormer_sill`.
        "DormerFront --> Split(Y) { DormerSill: DormerSkirt | ~1: DormerLight | 0.16: DormerSkirt }",
        "DormerSkirt --> I(\"Wall\")",
        "DormerLight --> Split(X) { 0.5: DormerSkirt | ~1: TrimmedSash | 0.5: DormerSkirt }",
        // ── 7. The porch: steps, deck, battered piers, rafter tails, hip ──
        // The porch strip arrives as a flat plot (the root scope has no
        // height), so it must raise its own volume before splitting it —
        // up to the main roof's spring plane, so the porch roof band tops
        // out exactly where the house eaves spring and the two roofs read
        // as one assembly.
        "PorchStrip --> Extrude(GroundH + Tuck) PorchVol",
        "PorchVol --> Split(Z) { ApronD: StepApron | ~1: PorchBody }",
        "StepApron --> Split(X) { ~1: NIL | StepW: StepBlockZone | ~1: NIL }",
        "StepBlockZone --> Split(Y) { DeckH: StepFlight | ~1: NIL }",
        "StepFlight --> Repeat(Y, StepH) { StepBand }",
        // Each tread anchors at the deck and recedes as it climbs — the
        // palace-stair idiom, with `split.i` sizing the run.
        "StepBand --> Size(scope.x, scope.y, ApronD * (split.n - split.i) / split.n) \
                      Translate(0, 0, ApronD * split.i / split.n) Mat(\"Concrete\") I(\"Step\")",
        "PorchBody --> Split(Y) { DeckH: DeckSlab | ~1: PorchOpen | RafterH: RafterBand | PorchRoofH: PorchRoof }",
        "DeckSlab --> Mat(\"Wood\") I(\"Deck\")",
        // The colonnade: battered piers on brick pedestals, brick knee
        // walls between them, open air above the rail line. The open zone
        // is first Sized down to a ColW-deep line at the porch front —
        // without that, every element inherits the porch body's full
        // depth, and the "piers" come out as metre-deep tapered slabs
        // with knee-height brick filling the whole floor (they did, in
        // the first validation pass; a straight-on contact sheet cannot
        // show it).
        "PorchOpen --> Size(scope.x, scope.y, ColW) Colonnade",
        // Each run is a palindrome — corner pier, knee wall, entry pier —
        // so the two sides of the steps mirror each other exactly. The
        // first pass built the runs from a start-with-a-pier rhythm group,
        // which put a pier hard against the entry on one side and a knee
        // gap on the other.
        "Colonnade --> Split(X) { ~1: ColRun | StepW: NIL | ~1: ColRun }",
        "ColRun --> Split(X) { ColW: PorchColumn | ~1: KneeBay | ColW: PorchColumn }",
        "PorchColumn --> Split(Y) { PedH: Pedestal | ~1: BatterPier }",
        "Pedestal --> Mat(\"Brick\") I(\"Pedestal\")",
        "BatterPier --> Taper(PierBatter) Mat(\"Trim\") I(\"Pier\")",
        "KneeBay --> Split(Y) { KneeWallH: KneeWall | ~1: NIL }",
        "KneeWall --> Mat(\"Brick\") I(\"Knee\")",
        // Exposed rafter tails: each one runs the porch depth and then
        // pokes past the roof edge, so the end grain hangs in the open —
        // which is why the porch roof carries no fascia board at all. A
        // fascia would sit at the eave plane, exactly in front of the
        // tails, and hide the item's signature detail (it did, in the
        // first render).
        "RafterBand --> Split(X) { 0.2: NIL | { 0.09: RafterTail | 0.42: NIL }* | 0.2: NIL }",
        "RafterTail --> Size(scope.x, scope.y, scope.z + 0.42) Translate(0, 0, -0.42) Mat(\"Wood\") I(\"Rafter\")",
        // A low hip, not a shed: the hip seals its own side triangles where
        // a shed sheet would leave them open over the porch ends.
        "PorchRoof --> Roof(Hip, PorchPitch, overhang=0.3, ridge=X) \
                       { Slope: Shingles | _: Shingles }",
        // ── 8. The garage wing ────────────────────────────────────────────
        "SidePlot --> 58% GarageWing | 42% NIL",
        "GarageWing --> Split(Z) { GarSet: NIL | GarD: GaragePaint | ~1: NIL }",
        "BlueGarage --> Mat(\"SidingBlue\") Garage",
        "CreamGarage --> Mat(\"SidingCream\") Garage",
        "SageGarage --> Mat(\"SidingSage\") Garage",
        "Garage --> Extrude(GarH + Tuck + 1.6) GarageForm",
        "GarageForm --> Split(Y) { GarH: GarageBody | Tuck: TuckBand | ~1: GarageRoofZone }",
        "GarageBody --> Comp(Faces) { Front: GarageFront | Top: NIL | Bottom: NIL | _: GarageSide }",
        "GarageFront --> Split(Y) { BaseH: BaseCourse | ~1: GarageFrontWall }",
        "GarageSide --> Split(Y) { BaseH: BaseCourse | ~1: Wall }",
        "GarageFrontWall --> Split(X) { ~1: Wall | GDoorW: GarageDoorBay | ~1: Wall }",
        "GarageDoorBay --> Split(Y) { GDoorH: GarageDoor | ~1: Wall }",
        // A roller door is horizontal slats, not one flat sheet — a
        // single enamel quad reads as plain grey primer.
        "GarageDoor --> Repeat(Y, 0.34) { DoorSlat }",
        "DoorSlat --> Split(Y) { 0.27: SlatFace | ~1: SlatGroove }",
        "SlatFace --> Extrude(0.1) Mat(\"Enamel\") I(\"GarageDoor\")",
        "SlatGroove --> Extrude(0.05) Mat(\"Wood\") I(\"GarageDoor\")",
        // Deep and narrow, so the ridge runs down the depth and the gable
        // end faces the street, matching the house family via the shared
        // Pick key. Garage gable ends get their own mesh id so the
        // coherence test can tell the two buildings apart.
        "GarGable --> Roof(Gable, GablePitch, overhang=0.4, fascia=0.14, ridge=Z) \
                      { Slope: Shingles | GableEnd: GarGableWall | Fascia: FasciaTrim | _: Shingles }",
        "GarHip --> Roof(Hip, HipPitch, overhang=0.4, fascia=0.14, ridge=Z) \
                    { Slope: Shingles | Fascia: FasciaTrim | _: Shingles }",
        "GarJerk --> Roof(Jerkinhead, JerkPitch, tier=0.28, overhang=0.4, fascia=0.14, ridge=Z) \
                     { Slope: Shingles | GableEnd: GarGableWall | HipEnd: Shingles | Fascia: FasciaTrim | _: Shingles }",
        "GarGableWall --> I(\"GGable\")",
        // ── 9. Shared terminals ───────────────────────────────────────────
        //    `Wall` names no material, so it inherits the Pick's siding.
        "Wall --> Extrude(WallD) I(\"Wall\")",
    ];

    let grammar_source = std::iter::once(declarations.as_str())
        .chain([
            paint_house.as_str(),
            paint_garage.as_str(),
            roof_house.as_str(),
            roof_dormer.as_str(),
            roof_garage.as_str(),
        ])
        .chain(rules)
        .collect::<Vec<_>>()
        .join("\n");

    GeneratorKind::Shape {
        grammar_source,
        root_rule: "Lot".to_string(),
        footprint: Fp3([LOT_X, 0.0, LOT_Z]),
        seed: 10,
        materials: materials(),
        // All square timber — even the battered piers are square in plan.
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

    /// Derives the lot at `seed` through the same statement path the runtime
    /// uses.
    fn derive_lot(seed: u64) -> ShapeModel {
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
            .expect("bungalow derives")
    }

    /// Height of the dormer cap's ridge above the attic base — the test-side
    /// mirror of what `Roof(Gable, DormerPitch, ridge=Z)` builds, kept here
    /// because the grammar cannot consume it: the cap's ridge is implied by
    /// its pitch, and only the die-in test needs the number.
    fn dormer_ridge() -> f32 {
        SLIVER_H + DORMER_BODY_H + (DORMER_W / 2.0) * DORMER_PITCH.to_radians().tan()
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
        assert_grammar_parses_and_derives(build_kind(), "craftsman_bungalow");
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = CraftsmanBungalow.build("");
        sanitize_generator(&mut g);
        assert!(
            matches!(
                g.kind,
                GeneratorKind::Cuboid {
                    common: PrimCommon { solid: true, .. },
                    ..
                }
            ),
            "craftsman_bungalow root must be the solid foundation plinth"
        );
        let GeneratorKind::Shape {
            root_rule,
            materials,
            ..
        } = &g.children[0].kind
        else {
            panic!("bungalow body must remain Shape after sanitise");
        };
        assert_eq!(root_rule, "Lot");
        for slot in [
            "SidingBlue",
            "SidingCream",
            "SidingSage",
            "Trim",
            "Wood",
            "Brick",
            "Shingle",
            "Concrete",
            "Enamel",
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

    /// The whole dormer geometry hangs on three derived inequalities: the
    /// face must emerge from the roof, the window sill must clear the slope
    /// crossing on that face, and the cap ridge must die back into the
    /// slope before the dormer's rear plane. All three are functions of the
    /// pitch menu, so this test does the arithmetic for the frontal
    /// families the dormer gate allows (gable and jerkinhead).
    #[test]
    fn the_dormer_emerges_and_dies_into_the_slope() {
        for pitch in [GABLE_PITCH, JERK_PITCH] {
            let tan = pitch.to_radians().tan();
            // Emerge: the slope crossing on the face plane sits below the
            // body top, or no window could ever show.
            let crossing = DORMER_SET * tan;
            assert!(
                crossing + 0.6 < SLIVER_H + DORMER_BODY_H,
                "at {pitch}° the slope crosses the dormer face at {crossing:.2} — \
                 the body is all but buried"
            );
            // Sill: derived from the steepest pitch, so it clears them all.
            assert!(
                dormer_sill() + SLIVER_H > crossing,
                "at {pitch}° the sill sits below the slope crossing"
            );
            // Die-in: the cap ridge stays under the main slope at the
            // dormer's rear plane.
            let slope_at_rear = (DORMER_SET + DORMER_D) * tan;
            assert!(
                dormer_ridge() < slope_at_rear,
                "at {pitch}° the dormer cap ridge ({:.2}) clears the main \
                 slope ({slope_at_rear:.2}) at the rear plane and pokes out",
                dormer_ridge()
            );
        }
        // And the dormer must stay on the front slope: its rear plane short
        // of the ridge line at half the core depth.
        let core_d = LOT_Z - PORCH_D;
        assert!(
            DORMER_SET + DORMER_D < core_d / 2.0,
            "the dormer crosses the ridge line"
        );
    }

    /// The chimney must top every ridge in the menu, or on the tallest roof
    /// it decapitates below the ridge line and reads as a stub.
    #[test]
    fn the_chimney_tops_every_ridge_in_the_menu() {
        let core_d = LOT_Z - PORCH_D;
        for pitch in [GABLE_PITCH, HIP_PITCH, JERK_PITCH] {
            let ridge = (core_d / 2.0) * pitch.to_radians().tan();
            assert!(
                CHIM_TOP > ridge + 0.2,
                "at {pitch}° the ridge reaches {ridge:.2} but the chimney \
                 stops at {CHIM_TOP}"
            );
        }
    }

    /// The porch is the item's face: it must actually grow its colonnade,
    /// its rafter tails and its steps. "Derives cleanly" is not "has a
    /// porch".
    #[test]
    fn the_porch_carries_its_colonnade_and_rafters() {
        for seed in 0..8_u64 {
            let model = derive_lot(seed);
            let piers = count(&model, "Pier");
            assert!(
                piers >= 3,
                "seed {seed}: only {piers} battered piers across the porch"
            );
            assert_eq!(
                piers,
                count(&model, "Pedestal"),
                "seed {seed}: piers and pedestals out of step"
            );
            assert!(
                count(&model, "Rafter") >= 12,
                "seed {seed}: the rafter-tail band is bare"
            );
            assert!(count(&model, "Step") >= 2, "seed {seed}: no front steps");
            assert!(
                count(&model, "Knee") >= 2,
                "seed {seed}: the knee walls are missing"
            );
            assert!(
                count(&model, "Pane") >= 6,
                "seed {seed}: the elevations lost their windows"
            );
        }
    }

    /// One paint decision for the whole lot: every un-named wall terminal —
    /// house, knee band, dormer cheek and garage alike — must inherit the
    /// same siding, and different seeds must draw different colours.
    #[test]
    fn the_house_and_garage_share_one_paint() {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for seed in 0..12_u64 {
            let model = derive_lot(seed);
            let walls = mats_of(&model, "Wall");
            assert!(!walls.is_empty(), "seed {seed}: no wall terminals");
            let distinct: std::collections::HashSet<&&str> = walls.iter().collect();
            assert_eq!(
                distinct.len(),
                1,
                "seed {seed}: the lot mixed sidings {distinct:?} — Pick lost \
                 coherence between house and garage"
            );
            assert!(
                ["SidingBlue", "SidingCream", "SidingSage"].contains(&walls[0]),
                "seed {seed}: walls inherited {} instead of a siding",
                walls[0]
            );
            seen.insert(walls[0].to_string());
        }
        assert!(
            seen.len() >= 2,
            "12 seeds never changed the paint — the lottery is stuck"
        );
    }

    /// One roof decision for the whole lot: whenever the garage is present,
    /// its family must match the house's. Gable ends are the discriminator —
    /// the gable and jerkinhead families emit them, the hip family does not.
    #[test]
    fn the_garage_roof_matches_the_house() {
        let mut with_garage = 0;
        for seed in 0..14_u64 {
            let model = derive_lot(seed);
            if count(&model, "GarageDoor") == 0 {
                continue;
            }
            with_garage += 1;
            let house_gabled = count(&model, "Gable") > 0;
            let garage_gabled = count(&model, "GGable") > 0;
            assert_eq!(
                house_gabled, garage_gabled,
                "seed {seed}: the garage drew a different roof family than \
                 the house — the shared Pick key lost coherence"
            );
        }
        assert!(
            with_garage >= 3,
            "only {with_garage} of 14 seeds rolled a garage — the wing \
             lottery is stuck"
        );
    }

    /// The first validation pass found the "piers" were metre-deep tapered
    /// slabs: the colonnade rules ran in the porch-open volume and every
    /// strip inherited its full depth. Assert every colonnade element is a
    /// front-line piece — as deep as a post, not as deep as the porch.
    #[test]
    fn the_colonnade_is_a_line_of_posts_not_slabs() {
        for seed in 0..6_u64 {
            let model = derive_lot(seed);
            for id in ["Pier", "Pedestal", "Knee"] {
                for t in model.terminals.iter().filter(|t| t.mesh_id == id) {
                    assert!(
                        (t.scope.size.z - COL_W as f64).abs() < 1e-6,
                        "seed {seed}: a {id} is {:.2} deep — a porch-filling \
                         slab, not a front-line element",
                        t.scope.size.z
                    );
                }
                assert!(
                    count(&model, id) > 0,
                    "seed {seed}: no {id} terminals at all"
                );
            }
            // And the piers really are posts. Width gets a range, not an
            // equality: the rhythm group snaps its cycle count to the run
            // and SCALES its absolute slots to fit (0.42 can come out
            // 0.55), which is fine for a post and disastrous only in
            // depth — which is pinned exactly above.
            for t in model.terminals.iter().filter(|t| t.mesh_id == "Pier") {
                let w = t.scope.size.x;
                assert!(
                    (COL_W as f64 - 1e-6..COL_W as f64 * 1.6).contains(&w),
                    "seed {seed}: a pier is {w:.2} wide — outside the \
                     rhythm-snap range of a post"
                );
            }
        }
    }

    /// The tuck band must be closed wall, not an open slit: under a
    /// gable-end tympanum there is no descending eave to hide it, and the
    /// first validation pass could see the attic void (and the far windows'
    /// lit backings) straight through the roof. Four flush faces on the
    /// house, four more on the garage when it rolls.
    #[test]
    fn the_tuck_band_is_closed_wall_not_a_slit() {
        for seed in 0..8_u64 {
            let model = derive_lot(seed);
            let tucks = count(&model, "Tuck");
            let expect = if count(&model, "GarageDoor") > 0 {
                8
            } else {
                4
            };
            assert_eq!(
                tucks, expect,
                "seed {seed}: {tucks} tuck faces — the band under the roof \
                 springing is open somewhere"
            );
            // Flush, never proud: a proud tuck face would pierce the roof.
            for t in model.terminals.iter().filter(|t| t.mesh_id == "Tuck") {
                assert!(
                    t.scope.size.z.abs() < 1e-6,
                    "seed {seed}: a tuck face grew depth {:.3} — it must stay \
                     flush with the mass",
                    t.scope.size.z
                );
            }
        }
    }

    /// A roller door is slats: the bay must emit a stack of them, not one
    /// flat sheet of grey.
    #[test]
    fn the_garage_door_is_slatted() {
        let mut checked = 0;
        for seed in 0..10_u64 {
            let model = derive_lot(seed);
            let slats = count(&model, "GarageDoor");
            if slats == 0 {
                continue;
            }
            checked += 1;
            assert!(
                slats >= 8,
                "seed {seed}: the garage door is {slats} pieces — a flat \
                 sheet, not a slatted roller"
            );
        }
        assert!(checked >= 2, "too few garages rolled to check the door");
    }

    /// The colonnade must mirror about the entry: every pier at x has a
    /// partner at HouseW - x. The first pass grew the runs from a rhythm
    /// group that started with a pier, which jammed a pier against the
    /// entry on one side and left a knee gap on the other.
    #[test]
    fn the_colonnade_mirrors_about_the_entry() {
        for seed in 0..6_u64 {
            let model = derive_lot(seed);
            let centres: Vec<f64> = model
                .terminals
                .iter()
                .filter(|t| t.mesh_id == "Pier")
                .map(|t| t.scope.position.x + t.scope.size.x / 2.0)
                .collect();
            assert!(centres.len() >= 4, "seed {seed}: colonnade lost its piers");
            for c in &centres {
                let mirrored = HOUSE_W as f64 - c;
                assert!(
                    centres.iter().any(|d| (d - mirrored).abs() < 1e-3),
                    "seed {seed}: pier at x={c:.2} has no mirror partner — \
                     the colonnade is asymmetric about the steps"
                );
            }
        }
    }

    /// The porch roof must clear every opening under it: door leaf and
    /// front sash tops sit below the open zone's ceiling (the rafter-band
    /// underside), so nothing hides behind the porch structure.
    #[test]
    fn the_porch_roof_clears_the_openings_under_it() {
        let open_top = (GROUND_H + roof_tuck() - PORCH_ROOF_H - RAFTER_H) as f64;
        for seed in 0..6_u64 {
            let model = derive_lot(seed);
            for t in model.terminals.iter().filter(|t| t.mesh_id == "Door") {
                let top = t.scope.position.y + t.scope.size.y;
                assert!(
                    top < open_top,
                    "seed {seed}: the door top ({top:.2}) hides behind the \
                     porch roof band ({open_top:.2})"
                );
            }
            // Front-face panes only: side sash lives under the main eaves.
            for t in model.terminals.iter().filter(|t| {
                t.mesh_id == "Pane"
                    && t.scope.position.z < 0.5
                    && t.scope.position.y < (GROUND_H + 0.3) as f64
            }) {
                let top = t.scope.position.y + t.scope.size.y;
                assert!(
                    top < open_top,
                    "seed {seed}: a front sash top ({top:.2}) hides behind \
                     the porch roof band ({open_top:.2})"
                );
            }
        }
    }

    /// The massing lottery must actually vary the skyline: across a seed
    /// sweep some lots carry a dormer and some do not, and likewise the
    /// chimney.
    #[test]
    fn the_skyline_varies_across_the_street() {
        let mut dormers = 0;
        let mut chimneys = 0;
        let mut plain = 0;
        for seed in 0..14_u64 {
            let model = derive_lot(seed);
            let has_dormer = count(&model, "Reveal") > 0 && dormer_present(&model);
            if has_dormer {
                dormers += 1;
            }
            if count(&model, "Chimney") > 0 {
                chimneys += 1;
            } else {
                plain += 1;
            }
        }
        assert!(dormers > 0, "no seed grew a dormer");
        assert!(
            dormers < 14,
            "every seed grew a dormer — the gate and roll are stuck"
        );
        assert!(chimneys > 0, "no seed raised a chimney");
        assert!(plain > 0, "every seed raised a chimney");
    }

    /// A dormer shows as trimmed-sash panes above every ground-floor window
    /// head. The threshold sits above the tallest ground sash (whose origin
    /// tops out around 2.5) and below the lowest dormer sill (attic base of
    /// a single-storey house plus the derived sill, about 3.6), so it holds
    /// for both massing variants.
    fn dormer_present(model: &ShapeModel) -> bool {
        model
            .terminals
            .iter()
            .any(|t| t.mesh_id == "Pane" && t.scope.position.y > (GROUND_H + 0.3) as f64)
    }
}
