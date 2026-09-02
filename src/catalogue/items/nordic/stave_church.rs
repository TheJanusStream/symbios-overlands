//! Stave church — tarred vertical staves under a stack of steep shake
//! roofs, ringed by a low ambulatory and guarded by dragon-head finials.
//!
//! The silhouette is the whole point: a stave church is a telescope of
//! square tiers, each rising through the roof skirt of the one below and
//! inset from it, so the mass steps inward as it climbs. The grammar
//! builds that with a **recursive parameterised rule** — `Tier(n)` lays a
//! band of stave walls, sheds a hip skirt over them, then hands the
//! remaining height to `Tier(n - 1)` shrunk and re-centred on the axis.
//! `when(n <= 0)` caps the stack with a gabled crown. Two, three or four
//! tiers are drawn per placement, and because the extrusion height is
//! drawn with the tier count, a four-tier church is genuinely taller
//! rather than four squashed bands.
//!
//! That recursion is only expressible since `symbios-shape` 0.3 (rule
//! parameters, guards, and `Size` + `Center` for the inset); before it,
//! each tier had to be written out by hand at a fixed count.
//!
//! Footprint 16 × 16 — square, the way a stave church plan is. The
//! ambulatory (*svalgang*) is carved as a ring of four strips around the
//! nave core by nested splits, the same idiom the castle uses for its
//! wards.

use std::collections::HashMap;

use crate::catalogue::items::util::{attach, footing, tile, tiles_per_metre, upright_boards};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, SovereignMaterialSettings, SovereignShingleConfig,
    SovereignTextureConfig,
};
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    DRAGON_EYE, IRON_DARK, NORDIC_BAND, STONE_GREY, WOOD_DARK, WOOD_WARM, dragon_head, iron, stone,
    timber,
};

/// Footprint of the grammar plot, in world units.
const LOT: f32 = 16.0;
/// Depth of the ambulatory strip carved off each side.
const SKIRT_D: f32 = 2.6;
/// Total height of the ambulatory mass, the share of it given over to the
/// roof slot, and the roof's pitch. All three are kept in step with the
/// `SkirtH` / `SkirtRoofH` / `Roof(Hip, …)` numbers in the grammar below,
/// because [`skirt_ridge_y`] derives the dragon-head mounting height from
/// them.
const SKIRT_H: f32 = 2.9;
const SKIRT_ROOF_H: f32 = 1.1;
const SKIRT_PITCH_DEG: f32 = 34.0;
const TIER_PITCH_DEG: f32 = 50.0;
const CROWN_PITCH_DEG: f32 = 62.0;
/// How far wall slabs and arcade posts stand proud of their face. The
/// posts are the deeper of the two, so they set the tuck (below).
const WALL_D: f32 = 0.28;
const POST_PROUD: f32 = 0.30;
/// Top of the ambulatory wall band.
const SKIRT_WALL_TOP: f32 = SKIRT_H - SKIRT_ROOF_H - SKIRT_TUCK;

/// How far the dragon heads' necks sink into the shakes they stand on.
/// Seated exactly on the ridge they balance on the apex like ornaments;
/// buried a little, they read as carved posts rising out of the roof.
const HEAD_EMBED: f32 = 0.22;

/// Tuck bands — the gap between a wall band's top and the springing of the
/// roof above it.
///
/// A roof descends as it runs OUTWARD from its springing plane:
/// `y(d) = y_spring − d·tan(pitch)`. Walls and posts are extruded *proud*
/// of their face, so unless the roof springs higher than the wall top by
/// `proud · tan(pitch)`, the wall breaches its own roof and its top edge
/// pokes through the shakes. Every roofed band in this grammar needs one.
///
/// The gap is not a hole: at `d = proud` the overhang has descended
/// exactly to the wall top, so the roof closes the void from outside.
///
/// These are the single source of truth: [`build_kind`] formats them into
/// the grammar's `const` declarations, so the rules and the Rust-side
/// geometry cannot drift apart.
const SKIRT_TUCK: f32 = 0.21;
const TIER_TUCK: f32 = 0.34;
const CROWN_TUCK: f32 = 0.53;

/// Height of the ambulatory hip roof's ridge above grade.
///
/// The dragon heads stand on the four corners at `±(LOT/2 − SKIRT_D/2)`,
/// which is the *centreline* of each skirt strip — and on a hip roof the
/// ridge runs along that centreline, terminating in the hip-end apex
/// exactly over the corner. So the corner is at RIDGE height, not eave
/// height: springing level + `(strip depth / 2) · tan(pitch)`.
///
/// Worth spelling out because every wrong answer looks plausible.
/// Mounting at the mass total leaves the heads hanging in the air;
/// mounting at the wall line buries them in the shakes.
fn skirt_ridge_y() -> f32 {
    SKIRT_WALL_TOP + SKIRT_TUCK + (SKIRT_D / 2.0) * SKIRT_PITCH_DEG.to_radians().tan()
}

/// Tarred shake roofing — the one surface the nordic kit has no helper for
/// (it roofs in thatch and turf). A stave church is shingled in pitch-black
/// split pine, so the item carries its own.
fn shake(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        uv_scale: tiles_per_metre(tile::SHINGLE),
        texture: SovereignTextureConfig::Shingle(SovereignShingleConfig {
            color_tile: Fp3(color),
            color_grout: Fp3([0.06, 0.05, 0.05]),
            // Split pine, not fired tile: flatter profile, deeper overlap,
            // and no moss on a roof kept black with pine tar.
            shape_profile: Fp64(0.25),
            overlap: Fp64(0.55),
            moss_level: Fp64(0.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

pub struct StaveChurch;

impl CatalogueEntry for StaveChurch {
    fn slug(&self) -> &'static str {
        "stave_church"
    }
    fn name(&self) -> &'static str {
        "Stave Church"
    }
    fn description(&self) -> &'static str {
        "Tarred stave walls under stacked shake roofs, ringed by an ambulatory and dragon-head finials."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// The settlement's church — the established carved-timber register.
    /// The destitute end of the theme is the separate [`super::turf_house`].
    fn prosperity_band(&self) -> ProsperityBand {
        NORDIC_BAND
    }

    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Nordic]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 11.0,
            min_spawn_dist: 38.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        // Centred foundation plinth is the root. `attach` (not a bare
        // push): `footing` returns a root already sunk by half the buried
        // plinth, and a plain child inherits that offset (#1039).
        let mut root = footing(LOT + 1.0, LOT + 1.0, [0.0, 0.0], 11.0);
        let mut church = Generator::from_kind(build_kind());
        church.transform.translation = Fp3([-LOT / 2.0, 0.0, -LOT / 2.0]);
        attach(&mut root, church);

        // Dragon heads on the four ambulatory corners. Deliberately mounted
        // on the *skirt*, whose height is fixed — the tiers above it are
        // drawn per placement, so a finial pinned up there would float or
        // sink with the lottery. The corner sits on each strip's
        // centreline, which is where the hip ridge runs, so they seat at
        // ridge height (see `skirt_ridge_y`).
        let c = LOT / 2.0 - SKIRT_D * 0.5;
        // Sunk slightly into the shakes so each head reads as rising out of
        // the roof rather than balancing on its apex.
        let seat = skirt_ridge_y() - HEAD_EMBED;
        for (sx, sz, yaw) in [
            (-1.0_f32, -1.0_f32, std::f32::consts::PI),
            (1.0, -1.0, 0.0),
            (1.0, 1.0, 0.0),
            (-1.0, 1.0, std::f32::consts::PI),
        ] {
            attach(
                &mut root,
                dragon_head([sx * c, seat, sz * c], 0.7, yaw, WOOD_DARK, DRAGON_EYE),
            );
        }
        root
    }
}

/// The palette, keyed by the `Mat("...")` names the grammar emits.
fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = HashMap::new();
    // Staves run vertically — that is what the building is named for — so
    // the plank pattern is quarter-turned; the generator only lays courses
    // up V otherwise.
    m.insert("Stave".to_string(), upright_boards(timber(WOOD_DARK)));
    m.insert("Board".to_string(), timber(WOOD_WARM));
    m.insert("Shake".to_string(), shake([0.16, 0.13, 0.12]));
    m.insert("Sill".to_string(), stone(STONE_GREY));
    m.insert("Iron".to_string(), iron(IRON_DARK));
    // The doorway and the slit windows are voids, not glazing: a stave
    // church is famously dark inside.
    m.insert(
        "Void".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.03, 0.03, 0.04]),
            roughness: Fp(1.0),
            ..Default::default()
        },
    );
    m
}

fn build_kind() -> GeneratorKind {
    // Declarations formatted from the Rust-side geometry constants, so the
    // wall depths, roof pitches and tuck bands the rules split on are the
    // same numbers `skirt_ridge_y` and the guards reason about.
    let declarations = [
        format!("const WallD = {WALL_D}"),
        format!("const PostProud = {POST_PROUD}"),
        format!("const SkirtRoofH = {SKIRT_ROOF_H}"),
        // The NIL tuck bands lift each roof clear of the proud walls and
        // posts below it — see the tuck-band docs above.
        format!("const SkirtTuck = {SKIRT_TUCK}"),
        format!("const TierTuck = {TIER_TUCK}"),
        format!("const CrownTuck = {CROWN_TUCK}"),
        format!("const SkirtPitch = {SKIRT_PITCH_DEG}"),
        format!("const TierPitch = {TIER_PITCH_DEG}"),
        format!("const CrownPitch = {CROWN_PITCH_DEG}"),
    ];
    let rules = [
        // ── Declarations ──
        "const SkirtD = 2.6",
        "const SkirtH = 2.9",
        // Height of one tier's stave band, and of the skirt roof over it.
        "const TierH = 2.5",
        "const TierRoofH = 1.4",
        // ── 1. Plan: an ambulatory ring carved around the nave core ──
        "Lot --> Split(Z) { SkirtD: SkirtRow | ~1: MidBand | SkirtD: SkirtRow }",
        "MidBand --> Split(X) { SkirtD: SkirtRow | ~1: NaveCore | SkirtD: SkirtRow }",
        // ── 2. The ambulatory: low stave wall under a shake lean-to ──
        "SkirtRow --> Extrude(SkirtH) Split(Y) { ~1: SkirtWalls | SkirtTuck: NIL | SkirtRoofH: SkirtCap }",
        "SkirtWalls --> Comp(Faces) { Side: SkirtFace | Top: NIL | Bottom: NIL }",
        "SkirtFace --> when(scope.x < 2.4): StaveWall | else: ArcadeRun",
        // Open arcading is what a svalgang actually is — a covered walk.
        "ArcadeRun --> Repeat(X, 1.5) { ArcadeBay }",
        "ArcadeBay --> Split(X) { 0.32: Post | ~1: ArcadeOpening | 0.32: Post }",
        "Post --> Extrude(PostProud) Mat(\"Board\") I(\"Post\")",
        "ArcadeOpening --> Split(Y) { ~1: Shade | '0.34: StaveWall }",
        "Shade --> Extrude(0.06) Mat(\"Void\") I(\"Shade\")",
        "SkirtCap --> Roof(Hip, SkirtPitch, 0.5) { Slope: ShakeFace | _: ShakeFace }",
        // ── 3. The nave: a telescope of inset tiers ──
        //    The tier count and the height are drawn together, so a taller
        //    church really is a taller stack rather than squashed bands.
        // Heights are (tiers x 3.9) + a ~2.8 m crown, so the belfry stays
        // the same size whatever the lottery draws and the STACK grows
        // beneath it. Letting the crown simply inherit the remainder made a
        // two-tier church a third belfry.
        "NaveCore --> 30% Extrude(10.6) Tier(2) | 44% Extrude(14.5) Tier(3) | 26% Extrude(18.4) Tier(4)",
        // Recursion: walls, a skirt roof shed over them, then the remaining
        // height handed up to a smaller tier standing on the same axis.
        "Tier(n) --> when(n <= 0): Crown | else: TierStack(n)",
        "TierStack(n) --> Split(Y) { TierH: TierWalls | TierTuck: NIL | TierRoofH: TierSkirt | ~1: NextTier(n) }",
        "TierWalls --> Comp(Faces) { Side: NaveFace | Top: NIL | Bottom: NIL }",
        // The upper tier rises *through* the skirt below it — the roof and
        // the tier above deliberately overlap in Y.
        "TierSkirt --> Roof(Hip, TierPitch, 0.45) { Slope: ShakeFace | _: ShakeFace }",
        "NextTier(n) --> Size(scope.x * 0.76, scope.y, scope.z * 0.76) Center(XZ) Tier(n - 1)",
        // ── 4. The crown: a steep gabled belfry closing the stack ──
        //    The crown inherits whatever height the recursion left, which
        //    shrinks with every tier — so it is sized in FLOATING and
        //    RELATIVE shares only. An absolute split here overflows the
        //    moment a four-tier church leaves it a short remainder.
        "Crown --> when(scope.y < 2.2): CrownCapOnly | else: CrownBelfry",
        "CrownBelfry --> Split(Y) { ~1: BelfryWalls | CrownTuck: NIL | ~1.6: BelfryRoof }",
        "CrownCapOnly --> BelfryRoof",
        "BelfryWalls --> Comp(Faces) { Side: BelfryFace | Top: NIL | Bottom: NIL }",
        "BelfryFace --> Split(X) { '0.2: StaveWall | ~1: BelfryOpening | '0.2: StaveWall }",
        "BelfryOpening --> Split(Y) { '0.18: StaveWall | ~1: Shade | '0.18: StaveWall }",
        "BelfryRoof --> Roof(Gable, CrownPitch, 0.3) { Slope: ShakeFace | GableEnd: StaveWall }",
        // ── 5. Nave elevation: stave boarding pierced by slit windows ──
        "NaveFace --> when(scope.x < 2.6): StaveWall | else: Repeat(X, 2.0) { NaveBay }",
        "NaveBay --> 62% StaveWall | 38% SlitBay",
        "SlitBay --> Split(X) { ~1: StaveWall | 0.34: SlitCut | ~1: StaveWall }",
        // Relative bands, so a short tier still gets a proportioned slit.
        "SlitCut --> Split(Y) { '0.35: StaveWall | ~1: Slit | '0.24: StaveWall }",
        // A slit is a real opening cut to the wall's depth, not a decal:
        // the outer face is dark void, the reveals are boarded.
        "Slit --> Extrude(WallD) Comp(Faces) { Back: VoidFace | Front: VoidFace | _: BoardReveal }",
        "VoidFace --> Mat(\"Void\") I(\"Slit\")",
        "BoardReveal --> Mat(\"Board\") I(\"Reveal\")",
        // ── 6. Shared terminals ──
        "StaveWall --> Extrude(WallD) Mat(\"Stave\") I(\"Wall\")",
        "ShakeFace --> Mat(\"Shake\") I(\"Shake\")",
    ];
    let grammar_source = declarations
        .into_iter()
        .chain(rules.iter().map(|r| (*r).to_string()))
        .collect::<Vec<String>>()
        .join("\n");

    GeneratorKind::Shape {
        grammar_source,
        root_rule: "Lot".to_string(),
        footprint: Fp3([LOT, 0.0, LOT]),
        seed: 5,
        materials: materials(),
        // Every mass is square-plan; the turning is in the carving, not the
        // geometry.
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

    fn derive_church(seed: u64) -> ShapeModel {
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
            .expect("church derives")
    }

    #[test]
    fn grammar_parses_and_derives() {
        assert_grammar_parses_and_derives(build_kind(), "stave_church");
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = StaveChurch.build("");
        sanitize_generator(&mut g);
        assert!(
            matches!(
                g.kind,
                GeneratorKind::Cuboid {
                    common: PrimCommon { solid: true, .. },
                    ..
                }
            ),
            "stave_church root must be the solid foundation plinth"
        );
        let GeneratorKind::Shape {
            root_rule,
            materials,
            ..
        } = &g.children[0].kind
        else {
            panic!("church body must remain Shape after sanitise");
        };
        assert_eq!(root_rule, "Lot");
        for slot in ["Stave", "Board", "Shake", "Sill", "Iron", "Void"] {
            assert!(
                materials.contains_key(slot),
                "missing material slot: {slot}"
            );
        }
    }

    /// Each roofed band's tuck must be at least `proud · tan(pitch)`, or
    /// the wall below it breaches its own roof and its top edge pokes
    /// through the shakes — which is exactly what shipped before this
    /// guard. Also bounded above, so an over-generous tuck does not open a
    /// visible gap under the eaves.
    #[test]
    fn tuck_bands_clear_their_walls() {
        for (what, tuck, proud, pitch) in [
            ("ambulatory", SKIRT_TUCK, POST_PROUD, SKIRT_PITCH_DEG),
            ("nave tier", TIER_TUCK, WALL_D, TIER_PITCH_DEG),
            ("crown belfry", CROWN_TUCK, WALL_D, CROWN_PITCH_DEG),
        ] {
            let needed = proud * pitch.to_radians().tan();
            assert!(
                tuck >= needed,
                "{what}: tuck {tuck:.3} < required {needed:.3} — the wall will \
                 pierce its roof by {:.3} m",
                needed - tuck
            );
            assert!(
                tuck < needed + 0.05,
                "{what}: tuck {tuck:.3} overshoots {needed:.3} — that opens a \
                 gap under the eaves"
            );
        }
    }

    /// The telescope is the item: the recursion must actually step inward,
    /// producing several distinct wall widths stacked up the building. A
    /// broken `Size`/`Center` inset would still derive cleanly — it would
    /// just come out as one blunt tower.
    #[test]
    fn the_tiers_telescope_inward_as_they_climb() {
        let model = derive_church(5);
        // Wall terminals from the nave, grouped by height band. Higher
        // bands must be narrower than lower ones.
        let mut bands: Vec<(i64, f64)> = model
            .terminals
            .iter()
            .filter(|t| t.mesh_id == "Wall")
            .map(|t| {
                (
                    (t.scope.position.y * 2.0).round() as i64,
                    t.scope.position.x,
                )
            })
            .collect();
        bands.sort_by_key(|(y, _)| *y);

        // Span of the nave at the lowest and highest wall bands.
        let lowest = bands.first().expect("wall terminals").0;
        let highest = bands.last().unwrap().0;
        assert!(highest > lowest, "no vertical stacking at all");

        let span = |band: i64| -> f64 {
            let xs: Vec<f64> = bands
                .iter()
                .filter(|(y, _)| *y == band)
                .map(|(_, x)| *x)
                .collect();
            let lo = xs.iter().cloned().fold(f64::INFINITY, f64::min);
            let hi = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            hi - lo
        };
        assert!(
            span(highest) < span(lowest),
            "the stack does not narrow: bottom span {:.2} vs top span {:.2}",
            span(lowest),
            span(highest)
        );
    }

    /// The tier lottery has to change the building, not just its seed: a
    /// two-tier church must be measurably shorter than a four-tier one.
    #[test]
    fn the_tier_lottery_changes_the_height() {
        let mut heights: Vec<f64> = (0..14_u64)
            .map(|seed| {
                derive_church(seed)
                    .terminals
                    .iter()
                    .map(|t| t.scope.position.y + t.scope.size.y)
                    .fold(0.0_f64, f64::max)
            })
            .collect();
        heights.sort_by(f64::total_cmp);
        let (lowest, tallest) = (heights[0], heights[heights.len() - 1]);
        assert!(
            tallest - lowest > 4.0,
            "the tier lottery barely moved the roofline: {lowest:.1} m to {tallest:.1} m"
        );
    }

    /// Every derivation must close the stack with a crown — an unterminated
    /// recursion would leave the top tier open to the sky.
    #[test]
    fn every_church_is_crowned() {
        for seed in 0..10_u64 {
            let model = derive_church(seed);
            let shakes = model
                .terminals
                .iter()
                .filter(|t| t.mesh_id == "Shake")
                .count();
            assert!(
                shakes >= 8,
                "seed {seed}: only {shakes} shake panels — the stack lost its roofs"
            );
        }
    }
}
