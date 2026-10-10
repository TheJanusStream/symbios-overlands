//! Berlin's street buildings, in every theme's dress (#1598).
//!
//! A Berlin region stands the theme's own catalogue buildings on Berlin's
//! footprints (#1588). Owner decisions on #1598 (2026-10-09): every theme
//! has THREE shape-grammar buildings of BERLIN'S STREET TYPES in its own
//! dress; a Berlin footprint draws its rows from these ALONE where a theme
//! has them, its landmarks staying as they were; and each copy is SHAPED TO
//! ITS FOOTPRINT - its frontage, depth and storeys come from Berlin's data,
//! on a few steps so copies still share meshes. Elsewhere - the inventory,
//! a road network's lots - they are ordinary catalogue entries, built at
//! their kind's own fit; a seeded settlement leaves them out. Owner
//! decisions on #1600 (2026-10-10): every theme has TWO MORE - a detached
//! house and a hall - so the planner can draw a block of villas, a
//! commercial area's sheds, as Berlin's urban structure types them.
//!
//! - **The kinds** ([`StreetKind`]): the street house - Berlin's
//!   perimeter-block house, the Altbau, three to seven storeys, shops on
//!   its ground floor where the building trades; the long block - the
//!   Gruenderzeit block or the Plattenbau slab, six to twelve storeys; the
//!   low building - a cottage row, a shop or a workshop of one or two
//!   storeys; the detached house - a house or a villa of one to three
//!   storeys standing free in its garden; and the hall - a works hall, a
//!   warehouse or a retail box of one or two tall storeys.
//! - **The fit** ([`StreetFit`]): the frontage along the street (local X),
//!   the depth back from it (local Z), the storeys, and whether the ground
//!   floor trades. Each kind has its own steps.
//! - **The spec** ([`StreetSpec`]): one building's slug, name, theme, kind,
//!   storey heights, palette and rules; [`StreetEntry`] makes it a
//!   catalogue entry, and [`StreetSpec::build`] builds it at a fit.
//!
//! # The rules file
//!
//! A street building's grammar is a `.cga` file beside its spec, read with
//! `include_str!`: one statement a line as the world parses a grammar,
//! except that a line starting with white space continues the statement
//! above it, and a comment is a `//` line of its own (see [`statements`]).
//! [`StreetSpec::grammar`] puts the fit's declarations ahead of it and
//! [`OPENINGS`] after it.
//!
//! The render tool draws a street building with a rules file read at run
//! time - `--catalogue <slug> --street-rules <file>` - and holds one to the
//! conventions below with `--street-check`, so a grammar is drafted without
//! a rebuild between drafts.
//!
//! # The conventions
//!
//! Every street grammar keeps them; [`check`] holds a drawing to them, and
//! the tests below hold every street building in the catalogue to them at
//! its kind's smallest and largest fits and two between, at its own grammar
//! seed and three others - and, fired by hand after a grammar changes, at
//! every fit its kind builds (an ignored sweep, #1600).
//!
//! - **The lot** is `frontage x depth`, its corner at the grammar's origin,
//!   and its street side is `Front` (local -Z), which the Berlin planner
//!   turns to the street. Nothing reaches past the lot's sides: neighbours
//!   stand flush against them, party wall to party wall. The front and the
//!   back may reach out [`MAX_REACH_M`] - steps, balconies, oriels, a
//!   cornice, an awning.
//! - **A detached house stands free** ([`StreetKind::freestanding`]): no
//!   neighbour stands against it, so its sides are walls with windows, not
//!   party walls, and every face - front, back and both sides - has a
//!   window. Nothing reaches past its lot's sides all the same: where its
//!   roof's verge would, its walls stand in from them. It never trades.
//! - **The storeys are its storeys.** The fit's declarations are `Storeys`,
//!   `Trade` (1 where the ground floor trades), `Frontage` and `Depth`, and
//!   the spec's [`StreetSpec::storey_m`] as `GroundH` and `FloorH`; its
//!   walls stand `GroundH + FloorH * (Storeys - 1)` high, its front has a
//!   door and a window in every storey, and only a crown - a cornice, a
//!   parapet, a roof - stands above them, ten metres at most.
//! - **A facade stands inside its mass** (#972 lesson 46): the side faces
//!   are party walls the full depth, the front and back are inset by the
//!   wall's depth at both ends so the party walls turn the corners, and a
//!   face is translated inward by `WallD` before it is extruded back out to
//!   its own plane.
//! - **A window is a real opening with something behind it** (#972 lesson
//!   1): [`OPENINGS`]' `Glazing` sets a `Pane` card back `PaneSet` in its
//!   reveal and a lit or dark room at the wall's back.
//! - **A building is sunk** 0.35 m (the lot builder's and the Berlin
//!   planner's `FOUNDATION_SINK_M`) wherever it is placed, so no door or
//!   window starts lower than that: a door stands on a step, a shop window
//!   on a plinth.
//! - **Its solid parts are listed** in `solid_meshes` (#1572): walls, roofs,
//!   doors, panes, everything a visitor should not walk through.
//! - **Nothing z-fights and nothing floats** (#1440, #1571): no two faces
//!   are drawn in one place, and every part touches the body.
//! - **It fits a record**: the grammar with its declarations and openings
//!   is within the sanitizer's 16 KiB for a grammar, past which a record
//!   cuts it short.

use std::collections::HashMap;

use crate::catalogue::items::util::{attach, footing};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind, SovereignMaterialSettings};
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod check;

/// How far a street building may reach out from its lot's front or back
/// (m): steps, balconies, oriels, a cornice, an awning. Never from its
/// sides, where its neighbours stand.
pub(crate) const MAX_REACH_M: f32 = 2.0;

/// The street types of Berlin a theme dresses (#1598, #1600).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StreetKind {
    /// Berlin's perimeter-block house, the Altbau: three to seven storeys
    /// on a frontage of 12 to 20 m, shops on its ground floor where the
    /// building trades.
    House,
    /// The long block - a Gruenderzeit block, a Plattenbau slab - six to
    /// twelve storeys on a frontage of 24 to 48 m.
    Block,
    /// A low building of one or two storeys: a cottage row, or, where it
    /// trades, a shop, a workshop.
    Low,
    /// A detached house or a villa (#1600): one to three storeys on a
    /// frontage of 8 to 16 m, standing free in its garden, homes only.
    Detached,
    /// A hall (#1600) - a works hall, a warehouse, a retail box - of one or
    /// two tall storeys on a frontage of 16 to 96 m: a store's front where
    /// it trades, a works' doors where it does not.
    Hall,
}

impl StreetKind {
    /// Every kind.
    pub const ALL: [Self; 5] = [
        Self::House,
        Self::Block,
        Self::Low,
        Self::Detached,
        Self::Hall,
    ];

    /// The frontages a copy is built at (m), smallest first.
    pub fn frontages(self) -> &'static [u16] {
        match self {
            StreetKind::House => &[12, 16, 20],
            StreetKind::Block => &[24, 36, 48],
            StreetKind::Low => &[8, 12, 16, 24],
            StreetKind::Detached => &[8, 10, 12, 16],
            StreetKind::Hall => &[16, 24, 36, 48, 72, 96],
        }
    }

    /// The depths a copy is built at (m), smallest first: down to a side
    /// wing's, or a garage's, so a narrow footprint is drawn near its own
    /// size rather than shrunk.
    pub fn depths(self) -> &'static [u16] {
        match self {
            StreetKind::House | StreetKind::Block => &[8, 11, 14],
            StreetKind::Low => &[6, 9, 13, 17],
            StreetKind::Detached => &[8, 10, 12, 16],
            StreetKind::Hall => &[12, 16, 24, 36, 48],
        }
    }

    /// The storeys a copy is built with, fewest first.
    pub fn storeys(self) -> &'static [u8] {
        match self {
            StreetKind::House => &[3, 4, 5, 6, 7],
            StreetKind::Block => &[6, 8, 10, 12],
            StreetKind::Low => &[1, 2],
            StreetKind::Detached => &[1, 2, 3],
            StreetKind::Hall => &[1, 2],
        }
    }

    /// Whether it stands free (#1600): no neighbour against its sides, so
    /// they have windows - a detached house.
    pub fn freestanding(self) -> bool {
        self == StreetKind::Detached
    }

    /// Whether its ground floor may trade: a detached house's never does.
    pub fn trades(self) -> bool {
        self != StreetKind::Detached
    }

    /// The fit an entry of this kind is built at where no footprint shapes
    /// it: in the inventory, on a road network's lot.
    pub fn default_fit(self) -> StreetFit {
        match self {
            StreetKind::House => StreetFit::new(16, 14, 5, true),
            StreetKind::Block => StreetFit::new(36, 14, 8, false),
            StreetKind::Low => StreetFit::new(12, 13, 2, false),
            StreetKind::Detached => StreetFit::new(10, 10, 2, false),
            StreetKind::Hall => StreetFit::new(36, 24, 1, false),
        }
    }

    /// A short name, for logs and test failures.
    pub fn name(self) -> &'static str {
        match self {
            StreetKind::House => "street house",
            StreetKind::Block => "long block",
            StreetKind::Low => "low building",
            StreetKind::Detached => "detached house",
            StreetKind::Hall => "hall",
        }
    }
}

/// The step of `steps` nearest `value`; of two as near, the lower.
fn nearest<T: Copy + Into<f32>>(steps: &[T], value: f32) -> T {
    steps
        .iter()
        .copied()
        .min_by(|a, b| {
            let (da, db) = (((*a).into() - value).abs(), ((*b).into() - value).abs());
            da.total_cmp(&db)
        })
        .expect("a kind has steps")
}

/// What a street building is built to (#1598): its frontage along the
/// street (local X) and its depth back from it (local Z) in whole metres,
/// its storeys, and whether its ground floor trades.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StreetFit {
    pub frontage: u16,
    pub depth: u16,
    pub storeys: u8,
    pub trade: bool,
}

impl StreetFit {
    pub const fn new(frontage: u16, depth: u16, storeys: u8, trade: bool) -> Self {
        StreetFit {
            frontage,
            depth,
            storeys,
            trade,
        }
    }

    /// The frontage (m).
    pub fn frontage_m(self) -> f32 {
        f32::from(self.frontage)
    }

    /// The depth (m).
    pub fn depth_m(self) -> f32 {
        f32::from(self.depth)
    }

    /// How far its building reaches from its lot's middle, turned any way
    /// (m): half the diagonal of its lot and the [`MAX_REACH_M`] its front
    /// and back may stand out.
    pub fn reach_m(self) -> f32 {
        self.frontage_m().hypot(self.depth_m() + 2.0 * MAX_REACH_M) / 2.0
    }

    /// The fit `kind` builds nearest `self`: each dimension on its nearest
    /// step, and trading only where the kind may.
    pub fn snapped(self, kind: StreetKind) -> Self {
        StreetFit {
            frontage: nearest(kind.frontages(), self.frontage_m()),
            depth: nearest(kind.depths(), self.depth_m()),
            storeys: nearest(kind.storeys(), f32::from(self.storeys)),
            trade: self.trade && kind.trades(),
        }
    }
}

/// One theme's street building (#1598): what it is called, which theme and
/// kind it is, and what its grammar is drawn from.
pub struct StreetSpec {
    pub slug: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub themes: &'static [ThemeArchetype],
    pub kind: StreetKind,
    /// The rooms it suits: a destitute quarter's, a prosperous one's.
    pub band: ProsperityBand,
    /// Its ground storey's height and every other storey's (m): its walls
    /// stand `ground + floor * (storeys - 1)` high.
    pub storey_m: (f32, f32),
    /// Its grammar's rules, root rule `Lot`: a `.cga` file beside it (see
    /// the module docs).
    pub rules: &'static str,
    /// Its grammar's own seed: a placement's, or a Berlin footprint's,
    /// takes its place.
    pub seed: u64,
    /// Its palette: every slot its rules and [`OPENINGS`] name. A grammar
    /// keeps only those ([`Self::grammar`]), so a theme's buildings may
    /// share one.
    pub materials: fn() -> HashMap<String, SovereignMaterialSettings>,
    /// Its turned terminals, drawn round (`round_meshes`).
    pub round_meshes: &'static [&'static str],
    /// What a visitor meets as solid (#1572): its walls, doors and panes at
    /// least.
    pub solid_meshes: &'static [&'static str],
}

impl StreetSpec {
    /// How high its walls stand at `storeys` (m).
    pub fn walls_m(&self, storeys: u8) -> f32 {
        self.storey_m.0 + self.storey_m.1 * f32::from(storeys.saturating_sub(1))
    }

    /// Its Shape at `fit`, snapped to its kind's steps: the fit's
    /// declarations, its rules and [`OPENINGS`], on a lot the fit's size
    /// with its corner at the origin and its front local -Z.
    pub fn grammar(&self, fit: StreetFit) -> GeneratorKind {
        self.grammar_with(fit, self.rules)
    }

    /// [`Self::grammar`] with `rules` in place of its own: a draft read at
    /// run time (`--street-rules`).
    pub(crate) fn grammar_with(&self, fit: StreetFit, rules: &str) -> GeneratorKind {
        let fit = fit.snapped(self.kind);
        let declarations = [
            format!("const Storeys = {}", fit.storeys),
            format!("const Trade = {}", u8::from(fit.trade)),
            format!("const Frontage = {}", fit.frontage),
            format!("const Depth = {}", fit.depth),
            format!("const GroundH = {}", self.storey_m.0),
            format!("const FloorH = {}", self.storey_m.1),
        ];
        let grammar_source = declarations
            .into_iter()
            .chain(statements(rules))
            .chain(OPENINGS.iter().map(|s| s.to_string()))
            .collect::<Vec<_>>()
            .join("\n");
        // Only what the rules name: a theme's buildings may share one
        // palette, and a slot no `Mat` names would cost every record that
        // keeps a copy its bytes for nothing.
        let mut materials = (self.materials)();
        materials.retain(|slot, _| grammar_source.contains(&format!("Mat(\"{slot}\"")));
        GeneratorKind::Shape {
            grammar_source,
            root_rule: "Lot".to_string(),
            footprint: Fp3([fit.frontage_m(), 0.0, fit.depth_m()]),
            seed: self.seed,
            materials,
            round_meshes: self.round_meshes.iter().map(|s| s.to_string()).collect(),
            solid_meshes: self.solid_meshes.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// The building at `fit`, snapped to its kind's steps: on a footing its
    /// lot's size - held in a few centimetres by the helper, so two
    /// neighbours' footings never touch - its grammar hung from the
    /// footing's middle by its corner. `attach`, as every grammar entry
    /// does: the footing's root is sunk, and a bare child inherits it.
    pub fn build(&self, fit: StreetFit) -> Generator {
        self.build_with(fit, self.rules)
    }

    /// [`Self::build`] with `rules` in place of its own (`--street-rules`).
    pub(crate) fn build_with(&self, fit: StreetFit, rules: &str) -> Generator {
        let fit = fit.snapped(self.kind);
        let (frontage, depth) = (fit.frontage_m(), fit.depth_m());
        let mut root = footing(frontage, depth, [0.0, 0.0], fit.reach_m());
        let mut body = Generator::from_kind(self.grammar_with(fit, rules));
        body.transform.translation = Fp3([-frontage / 2.0, 0.0, -depth / 2.0]);
        attach(&mut root, body);
        root
    }
}

/// A [`StreetSpec`] as a catalogue entry: a secondary building of its
/// theme, built at its kind's own fit.
pub struct StreetEntry(pub &'static StreetSpec);

impl CatalogueEntry for StreetEntry {
    fn slug(&self) -> &'static str {
        self.0.slug
    }
    fn name(&self) -> &'static str {
        self.0.name
    }
    fn description(&self) -> &'static str {
        self.0.description
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        self.0.themes
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn prosperity_band(&self) -> ProsperityBand {
        self.0.band
    }
    fn footprint(&self) -> Footprint {
        let reach = self.0.kind.default_fit().reach_m();
        Footprint {
            clearance: reach,
            min_spawn_dist: reach + 8.0,
        }
    }
    fn build(&self, _local_did: &str) -> Generator {
        self.0.build(self.0.kind.default_fit())
    }
    fn street(&self) -> Option<&'static StreetSpec> {
        Some(self.0)
    }
    /// None: it stands flush against its neighbours, party wall to party
    /// wall, and a fought-over room's lean would push it into them.
    fn ruin_max_lean(&self) -> Option<f32> {
        Some(0.0)
    }
}

/// The openings every street grammar may use, after its own constants
/// `WallD` (the wall's depth) and `PaneSet` (how far a pane sits back in
/// its reveal): each a face-scope rule, its scope on the wall's inner plane
/// as an inward facade leaves it (see the module docs).
///
/// - `Glazing`: a window - a `Pane` card (`Glass`) set back `PaneSet` from
///   the wall's face, and behind it a lit (`RoomLit`) or dark (`RoomDark`)
///   room at the wall's back, lit and dark mixed along a street window by
///   window.
/// - `ShopGlazing`: a shop window - one card (`ShopGlass`) over a lit shop
///   (`ShopLit`).
/// - `Wall`: the wall itself, `WallD` deep, in whatever cladding the
///   building's rules stamped above it.
///
/// The sides of an opening are the walls round it, so it needs no reveal
/// of its own.
pub(crate) const OPENINGS: [&str; 10] = [
    "Glazing --> 42% LitPane | 58% DarkPane",
    "LitPane --> Extrude(WallD - PaneSet) Comp(Faces) { Back: PaneCard | Front: LitRoom }",
    "DarkPane --> Extrude(WallD - PaneSet) Comp(Faces) { Back: PaneCard | Front: DarkRoom }",
    "PaneCard --> Mat(\"Glass\") I(\"Pane\")",
    "LitRoom --> Mat(\"RoomLit\") I(\"Room\")",
    "DarkRoom --> Mat(\"RoomDark\") I(\"Room\")",
    "ShopGlazing --> Extrude(WallD - PaneSet) Comp(Faces) { Back: ShopCard | Front: ShopRoom }",
    "ShopCard --> Mat(\"ShopGlass\") I(\"Pane\")",
    "ShopRoom --> Mat(\"ShopLit\") I(\"Room\")",
    "Wall --> Extrude(WallD) I(\"Wall\")",
];

/// How the [`OPENINGS`] of a theme's street buildings look: the window's
/// frame and its panes across and up, the light of a lit room and of a lit
/// shop.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Glazing {
    pub frame: [f32; 3],
    pub panes: (u32, u32),
    pub room: [f32; 3],
    pub shop: [f32; 3],
}

/// The palette [`OPENINGS`] reads, in `glazing`'s look: a `Window` card
/// on every opening (#972 lesson 1: an alpha card, `uv_scale` 1, its panes
/// cut away at the mask), a warm room or a dark one behind it, and a shop's
/// card over a brighter shop. The room is a plain surface lit flat, never
/// a tinted card: a tint on the card lights the frame too (see
/// `modern_city::rowhouse_terrace::sash_glass`). Low strengths (#972 lesson
/// 30): a room is a broad surface.
pub(crate) fn opening_materials(glazing: Glazing) -> HashMap<String, SovereignMaterialSettings> {
    let lit = |tint: [f32; 3], glow: f32| SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: crate::pds::Fp(glow),
        roughness: crate::pds::Fp(0.9),
        ..Default::default()
    };
    let (across, up) = glazing.panes;
    HashMap::from([
        (
            "Glass".to_string(),
            crate::catalogue::items::util::window_card(glazing.frame, across, up, 0.42, 0.08),
        ),
        ("RoomLit".to_string(), lit(glazing.room, 2.2)),
        ("RoomDark".to_string(), lit([0.07, 0.08, 0.10], 0.0)),
        (
            "ShopGlass".to_string(),
            crate::catalogue::items::util::window_card(glazing.frame, 2, 1, 0.38, 0.04),
        ),
        ("ShopLit".to_string(), lit(glazing.shop, 2.8)),
    ])
}

/// The statements of a `.cga` rules file, each on one line as the world
/// parses a grammar: a line that starts with white space continues the
/// statement above it, and blank lines and `//` lines are dropped - a
/// comment costs a record bytes it may not have to spare.
pub(crate) fn statements(rules: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in rules.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        match out.last_mut() {
            Some(statement) if raw.starts_with(char::is_whitespace) => {
                statement.push(' ');
                statement.push_str(line);
            }
            _ => out.push(line.to_string()),
        }
    }
    out
}

/// Every street building in the catalogue.
#[cfg(test)]
pub(crate) fn street_entries() -> impl Iterator<Item = &'static dyn CatalogueEntry> {
    super::ENTRIES
        .iter()
        .copied()
        .filter(|entry| entry.street().is_some())
}

#[cfg(test)]
mod tests {
    use super::check::{SEEDS, conventions, sample_fits, whole};
    use super::*;

    /// The spec of every street building in the catalogue.
    fn specs() -> Vec<&'static StreetSpec> {
        street_entries()
            .map(|entry| entry.street().expect("a street entry"))
            .collect()
    }

    #[test]
    fn a_fit_snaps_to_its_kinds_steps() {
        let fit = StreetFit::new(17, 30, 9, true).snapped(StreetKind::House);
        assert_eq!(fit, StreetFit::new(16, 14, 7, true));
        let low = StreetFit::new(3, 1, 0, false).snapped(StreetKind::Low);
        assert_eq!(low, StreetFit::new(8, 6, 1, false));
        let house = StreetFit::new(11, 13, 4, true).snapped(StreetKind::Detached);
        assert_eq!(
            house,
            StreetFit::new(10, 12, 3, false),
            "a detached house never trades"
        );
        let hall = StreetFit::new(100, 30, 1, true).snapped(StreetKind::Hall);
        assert_eq!(hall, StreetFit::new(96, 24, 1, true));
        for kind in StreetKind::ALL {
            let default = kind.default_fit();
            assert_eq!(default.snapped(kind), default, "{}", kind.name());
        }
    }

    #[test]
    fn a_rules_file_is_one_statement_a_line_with_continuations() {
        let rules = "// a comment\nconst WallD = 0.4\n\nLot --> Split(X) { 1: A\n    | ~1: B }\n  \
                     // inside\n    | C\nA --> I(\"Wall\")\n";
        assert_eq!(
            statements(rules),
            [
                "const WallD = 0.4",
                "Lot --> Split(X) { 1: A | ~1: B } | C",
                "A --> I(\"Wall\")",
            ]
        );
    }

    /// Every street building of `kind` keeps the conventions (see the
    /// module docs) at every sampled fit and seed: [`check::conventions`].
    fn keeps_the_conventions(kind: StreetKind) {
        for spec in specs().into_iter().filter(|spec| spec.kind == kind) {
            for fit in sample_fits(spec.kind) {
                for seed in [None].into_iter().chain(SEEDS.map(Some)) {
                    let problems = conventions(spec, spec.rules, fit, seed);
                    assert!(
                        problems.is_empty(),
                        "{} at {fit:?}, grammar seed {seed:?}:\n  {}",
                        spec.slug,
                        problems.join("\n  ")
                    );
                }
            }
        }
    }

    /// Nothing of a street building of `kind` draws two faces in one place
    /// (#1440) or floats free of it (#1571), as it is built at its kind's
    /// smallest and largest fits, at its own grammar seed and [`SEEDS`]:
    /// [`check::whole`].
    fn is_whole(kind: StreetKind) {
        for spec in specs().into_iter().filter(|spec| spec.kind == kind) {
            let fits = sample_fits(spec.kind);
            for fit in [fits[0], fits[1]] {
                for seed in [None].into_iter().chain(SEEDS.map(Some)) {
                    let problems = whole(spec, spec.rules, fit, seed);
                    assert!(
                        problems.is_empty(),
                        "{} at {fit:?}, grammar seed {seed:?}:\n  {}",
                        spec.slug,
                        problems.join("\n  ")
                    );
                }
            }
        }
    }

    // One test a kind, so the walks run side by side: a block is the
    // dearest, and plain `cargo test` derives every grammar unoptimised.

    #[test]
    fn every_street_house_keeps_the_conventions() {
        keeps_the_conventions(StreetKind::House);
    }

    #[test]
    fn every_long_block_keeps_the_conventions() {
        keeps_the_conventions(StreetKind::Block);
    }

    #[test]
    fn every_low_building_keeps_the_conventions() {
        keeps_the_conventions(StreetKind::Low);
    }

    #[test]
    fn no_street_house_z_fights_or_floats() {
        is_whole(StreetKind::House);
    }

    #[test]
    fn no_long_block_z_fights_or_floats() {
        is_whole(StreetKind::Block);
    }

    #[test]
    fn no_low_building_z_fights_or_floats() {
        is_whole(StreetKind::Low);
    }

    #[test]
    fn every_detached_house_keeps_the_conventions() {
        keeps_the_conventions(StreetKind::Detached);
    }

    #[test]
    fn every_hall_keeps_the_conventions() {
        keeps_the_conventions(StreetKind::Hall);
    }

    #[test]
    fn no_detached_house_z_fights_or_floats() {
        is_whole(StreetKind::Detached);
    }

    #[test]
    fn no_hall_z_fights_or_floats() {
        is_whole(StreetKind::Hall);
    }

    /// Every street building keeps the conventions at EVERY fit its kind
    /// builds - each frontage, depth and storey step, trading and not where
    /// it may trade - at its own grammar seed (#1600). The tests above
    /// sample four fits, and a grammar can fail between them: a split part
    /// that comes to nothing at one width, a storey's windows lost at one
    /// depth. Ignored, as the gates leave a sweep of nine thousand
    /// derivations: fire it by hand after a street grammar changes.
    #[test]
    #[ignore = "a sweep of every fit: fire by hand after a street grammar changes"]
    fn every_street_building_keeps_the_conventions_at_every_fit() {
        let mut faults = Vec::new();
        for spec in specs() {
            let kind = spec.kind;
            let trades: &[bool] = if kind.trades() {
                &[false, true]
            } else {
                &[false]
            };
            for &frontage in kind.frontages() {
                for &depth in kind.depths() {
                    for &storeys in kind.storeys() {
                        for &trade in trades {
                            let fit = StreetFit::new(frontage, depth, storeys, trade);
                            let problems = conventions(spec, spec.rules, fit, None);
                            if !problems.is_empty() {
                                faults.push(format!(
                                    "{} at {fit:?}:\n  {}",
                                    spec.slug,
                                    problems.join("\n  ")
                                ));
                            }
                        }
                    }
                }
            }
        }
        assert!(
            faults.is_empty(),
            "{} fits fail:\n{}",
            faults.len(),
            faults.join("\n")
        );
    }

    /// Every theme has its street buildings, one of each kind (#1598,
    /// #1600): a Berlin footprint draws its rows from them alone, so a theme
    /// short of one would fill its rows with what Berlin's streets were not
    /// drawn for.
    #[test]
    fn every_theme_has_one_street_building_of_each_kind() {
        for theme in ThemeArchetype::ALL {
            for kind in StreetKind::ALL {
                let of: Vec<&str> = specs()
                    .into_iter()
                    .filter(|spec| spec.kind == kind && spec.themes.contains(&theme))
                    .map(|spec| spec.slug)
                    .collect();
                assert_eq!(of.len(), 1, "{theme:?}'s {}: {of:?}", kind.name());
            }
        }
    }

    /// The catalogue's street entries are secondary buildings, each its
    /// spec's, one slug apiece.
    #[test]
    fn every_street_entry_is_a_secondary_building_of_its_own() {
        let mut slugs = Vec::new();
        for entry in street_entries() {
            let spec = entry.street().expect("a street entry");
            assert_eq!(entry.slug(), spec.slug);
            assert_eq!(entry.role(), StructureRole::Secondary, "{}", spec.slug);
            assert!(!slugs.contains(&spec.slug), "{} twice", spec.slug);
            slugs.push(spec.slug);
        }
        assert!(!slugs.is_empty(), "no street buildings");
    }
}
