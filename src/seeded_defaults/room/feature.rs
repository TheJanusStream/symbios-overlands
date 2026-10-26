//! A seeded world's signature feature (#1781): one memorable thing of its
//! land by its settlement - a lone giant tree, a sea stack, a hilltop ruin,
//! a stone ring or a lake in a dip of its land (#1793, [`super::lake`]) -
//! from a table by its landform and biome ([`FeatureKind::candidates`],
//! [`FeatureKind::lake_suits`]), sited on ground that fits it, clear of the
//! settlement, its paths and the view down to its landmark, and - where any
//! such ground does - where the arrival's first frame shows it beside the
//! landmark: the view's second attractor.
//!
//! Which of its kinds a world tries first is drawn from a stream of its own,
//! so that no other deriver's draws move; a kind no ground takes gives way
//! to the next, and the stone ring, which fits nearly any land, comes last.
//! A lake comes first wherever it suits: the land holds one in few worlds
//! (the owner's choice of 2026-10-26, a lake only where the land holds one),
//! and a world whose land holds none tries its kinds as before it.
//!
//! The siting reads the settlement's probe, ten metres a sample, which
//! under-reads the full map's relief (#1773, #1788): a feature asks more
//! of it than a building does - flatter ground and gentle ground a ring
//! further out for a built one, steady dry ground for a tree, deep water all
//! round a sea stack. A giant tree and a sea stack avoid no water, so the
//! compile neither walks them nor stands them on the highest ground round
//! them - a trunk has no plinth - and the scatters keep off them all the
//! same (`world_builder::compile::scatter::FEATURE`). Where the compile
//! walks a settlement member off ground it misread, the member can still
//! come to stand by the feature: the walk is the compile's own.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use super::paths::{distance2, off_segment};
use super::settlement::{SITE_SLOPE, facing, member_fits};
use super::siting::TerrainProbe;
use super::viewpoint::Viewpoint;
use crate::pds::{
    Fp, Fp2, Fp3, Fp4, Generator, GeneratorKind, PrimCommon, SovereignMaterialSettings,
    SovereignRockConfig, SovereignTextureConfig, TortureParams,
};
use crate::seeded_defaults::scene::{
    BiomeArchetype, LandformArchetype, SceneCharacter, pick, unit_f32,
};

/// Sub-stream salt distinct from every sibling room deriver.
const FEATURE_STREAM_SALT: u64 = 0x1781_5163_1781_5163;

/// How near the arrival a feature stands at the least (m): past the
/// forecourt and the first of the settlement.
const NEAREST: f32 = 50.0;

/// How far from the arrival a feature stands at the most (m).
const FARTHEST: f32 = 260.0;

/// How far from the arrival a feature stands best (m).
const BEST_REACH: f32 = 120.0;

/// How far off the view's heading a feature stands best, as the cosine of
/// that angle - about 20 degrees: beside the landmark in the first frame.
const BEST_OFF: f32 = 0.94;

/// How near the view's heading a feature stands at the most, as the cosine
/// - about 8 degrees: never behind the landmark, or before it.
const LEAST_OFF: f32 = 0.99;

/// How far a feature keeps off the ground of what stands in the
/// settlement, and off its paths' edges (m).
const MARGIN: f32 = 10.0;

/// How many times its size a giant tree is drawn.
const GIANT: f32 = 2.5;

/// How far round a giant tree its crown reaches (m): what it keeps clear
/// of, the ground it is sited on, and the clearing the scatters leave it.
pub(super) const CLEARING: f32 = 12.0;

/// The trees that stand as a giant, of those a world grows: a bush, a reed,
/// a fern or a mangrove drawn two and a half times its size is no tree.
const GIANT_TREES: [&str; 12] = [
    "lsys_oak",
    "lsys_young_oak",
    "lsys_birch",
    "lsys_poplar",
    "lsys_yew",
    "lsys_acacia",
    "lsys_apple",
    "lsys_flowering_tree",
    "lsys_monopodial_tree",
    "lsys_sympodial_tree",
    "lsys_ternary_gravity",
    "lsys_palm",
];

/// How deep the sea under a sea stack is at the least and at the most (m):
/// deep enough that the shallows the probe smooths over are not its, and
/// shallow enough for the stack to rise well clear of it.
const STACK_DEPTH: [f32; 2] = [4.0, 14.0];

/// How deep the sea stays all round a sea stack's reach at the least (m):
/// the probe smooths a beach out into the water.
const STACK_FLOOR: f32 = 2.5;

/// How far off the shore a sea stack stands at the least and at the most
/// (m): its reach and ten metres more, so neither pillar stands on a beach.
const STACK_OFFSHORE: [f32; 2] = [STACK_RADIUS + 10.0, 70.0];

/// How far round a sea stack's two stacks reach (m).
pub(super) const STACK_RADIUS: f32 = 12.0;

/// How high over the ground round it the crest a ruin stands on rises at
/// the least (m, [`TerrainProbe::prominence`]).
const CREST: f32 = 2.0;

/// How far out a built feature's ground is read for a fall (as a share of
/// its reach): a cliff the probe's rim reads between its samples - a mesa's
/// edge - shows on a ring further out.
const CLIFF_REACH: f32 = 1.6;

/// How much the probe's ground may rise and fall under a built feature at
/// the most (m): the compile stands it on the highest of its ground, and a
/// ring's stones or a ruin's walls carry no plinth deep enough for more
/// than the full map's few times that.
const FEATURE_RELIEF: f32 = 0.6;

/// How steep a giant tree's ground may be at the most (rise/run): its trunk
/// stands where it is sited, and the probe reads the full map's slopes
/// gentler than they are.
const TREE_SLOPE: f32 = 0.25;

/// How far over the water a giant tree's ground stands at the least (m):
/// the probe reads a shore drier than the full map draws it.
const TREE_FREEBOARD: f32 = 2.0;

/// The catalogue entries a hilltop ruin is drawn from: authored fallen.
/// The escalation pass felled a whole one too little to read as a ruin -
/// its tiers wear a settlement, they do not raze it.
const RUINS: [&str; 2] = ["ruined_temple", "ruined_chapel"];

/// What a world's signature feature is (#1781).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureKind {
    /// A tree of the world's own, drawn [`GIANT`] times its size.
    GiantTree,
    /// Stacks of the world's rock standing out of the sea off its shore.
    SeaStack,
    /// A temple or a chapel long fallen, on a crest.
    HilltopRuin,
    /// A ring of standing stones.
    StoneRing,
    /// Water standing in a dip of the land above the sea (#1793): a second
    /// terrain water layer, not a placement ([`super::lake`]).
    Lake,
}

impl FeatureKind {
    /// Whether the feature is built - a ruin, a ring - and stands as a
    /// building does: snapped to the highest of its ground, walked dry.
    pub fn is_built(self) -> bool {
        matches!(self, Self::HilltopRuin | Self::StoneRing)
    }

    /// Whether a lake suits a world of `scene` (#1793): any but a desert's,
    /// a badland's or an ice field's - its land decides, holding a lake in
    /// few worlds of any biome (its basins are a mesa's, near enough all).
    pub fn lake_suits(scene: &SceneCharacter) -> bool {
        !matches!(
            scene.biome,
            BiomeArchetype::Arid | BiomeArchetype::Badlands | BiomeArchetype::Glacial
        )
    }

    /// The kinds a world of `scene` may have, from the table by its landform
    /// and biome: a sea stack off a coast, a giant tree where trees grow
    /// (`trees`: where the world grows one that stands as a giant,
    /// [`stands_as_giant`]), a ruin on hilly land, and - last - a stone ring
    /// on any.
    pub fn candidates(scene: &SceneCharacter, trees: bool) -> Vec<Self> {
        let mut kinds = Vec::new();
        if scene.landform == LandformArchetype::Archipelago
            || scene.biome == BiomeArchetype::Coastal
        {
            kinds.push(Self::SeaStack);
        }
        if trees
            && matches!(
                scene.biome,
                BiomeArchetype::Lush
                    | BiomeArchetype::Jungle
                    | BiomeArchetype::TemperateForest
                    | BiomeArchetype::Boreal
                    | BiomeArchetype::Wetland
                    | BiomeArchetype::Meadow
                    | BiomeArchetype::Savanna
            )
        {
            kinds.push(Self::GiantTree);
        }
        if matches!(
            scene.landform,
            LandformArchetype::Craggy
                | LandformArchetype::Mesa
                | LandformArchetype::Rolling
                | LandformArchetype::Valleys
        ) {
            kinds.push(Self::HilltopRuin);
        }
        kinds.push(Self::StoneRing);
        kinds
    }
}

/// Whether the tree of catalogue `slug` stands as a giant ([`GIANT_TREES`]).
pub fn stands_as_giant(slug: &str) -> bool {
    GIANT_TREES.contains(&slug)
}

/// A world's signature feature as sited (#1781).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SignatureFeature {
    pub kind: FeatureKind,
    /// Where it stands, world XZ.
    pub at: [f32; 2],
    /// The way it is turned (radians): a built feature's front to the
    /// arrival.
    pub yaw: f32,
    /// How far round it reaches (m): what it keeps clear of, and the ground
    /// its placement carries, which the scatters keep off - and which the
    /// compile stands a built one on the highest of and walks dry
    /// ([`FeatureKind::is_built`]).
    pub radius: f32,
    /// The catalogue entry a built feature is drawn from, else `""`.
    pub slug: &'static str,
    /// The seed its draw takes: a ruin's grammar, a stack's chunks.
    pub seed: u64,
    /// A lake's water layer: its level and its rectangle about `at`, the
    /// floor of its basin (#1793).
    pub lake: Option<super::lake::LakeLayer>,
}

/// What a world's signature feature keeps clear of (#1781).
pub struct KeepClear<'a> {
    /// What stands in the settlement and the view down to its landmark, as
    /// `(spot, radius)`.
    pub stands: &'a [([f32; 2], f32)],
    /// The settlement's paths, each a polyline of world XZ points.
    pub paths: &'a [Vec<[f32; 2]>],
    /// How wide the paths are (m).
    pub width: f32,
}

/// The world's signature feature (#1781): the first of its kinds
/// ([`FeatureKind::candidates`]; the world's `seed` drawing which of them
/// before the stone ring comes first) that ground takes, at the best-scored
/// site round the arrival of `viewpoint` ([`site`]), clear of what `clear`
/// holds. `trees`: whether the world grows a tree that stands as a giant.
/// `None` where no kind finds a site.
pub fn choose(
    probe: &TerrainProbe,
    scene: &SceneCharacter,
    seed: u64,
    trees: bool,
    viewpoint: &Viewpoint,
    clear: &KeepClear,
) -> Option<SignatureFeature> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ FEATURE_STREAM_SALT);
    let mut kinds = FeatureKind::candidates(scene, trees);
    // Every draw is made before any kind is tried, so the order a world's
    // kinds are tried in takes no draws from the others.
    let first = rng.next_u32() as usize;
    let ruin = pick(&RUINS, &mut rng);
    let turn = unit_f32(&mut rng) * std::f32::consts::TAU;
    let draw = rng.next_u64();
    // The stone ring stays last: it fits nearly any land, so a kind tried
    // after it would never stand.
    let before_the_ring = kinds.len() - 1;
    if before_the_ring > 1 {
        kinds[..before_the_ring].rotate_left(first % before_the_ring);
    }
    // A lake first, wherever the land holds one (#1793).
    if FeatureKind::lake_suits(scene) {
        kinds.insert(0, FeatureKind::Lake);
    }
    kinds.into_iter().find_map(|kind| {
        if kind == FeatureKind::Lake {
            let (at, radius, lake) = super::lake::site(probe, viewpoint, clear)?;
            return Some(SignatureFeature {
                kind,
                at,
                yaw: 0.0,
                radius,
                slug: "",
                seed: draw,
                lake: Some(lake),
            });
        }
        let slug = match kind {
            FeatureKind::HilltopRuin => ruin,
            FeatureKind::StoneRing => "stone_circle",
            FeatureKind::GiantTree | FeatureKind::SeaStack | FeatureKind::Lake => "",
        };
        let (at, radius) = site(probe, kind, slug, viewpoint, clear)?;
        let yaw = match kind {
            FeatureKind::HilltopRuin | FeatureKind::StoneRing => {
                facing([viewpoint.landing[0] - at[0], viewpoint.landing[1] - at[1]])
            }
            FeatureKind::GiantTree | FeatureKind::SeaStack | FeatureKind::Lake => turn,
        };
        Some(SignatureFeature {
            kind,
            at,
            yaw,
            radius,
            slug,
            seed: draw,
            lake: None,
        })
    })
}

/// Where a feature of `kind` (drawn from `slug` where it is built) stands
/// best round the arrival of `viewpoint`, and its reach: of the points a
/// probe cell apart between [`NEAREST`] and [`FARTHEST`] of the landing and
/// off the view's heading, those whose ground takes it - clear by
/// [`MARGIN`] of what `clear` holds, the paths' edges - scored by whether
/// the first frame shows it, how near its angle off the heading comes to
/// [`BEST_OFF`] and its distance to [`BEST_REACH`], and for a built one the
/// crest it stands on. The first of equals.
fn site(
    probe: &TerrainProbe,
    kind: FeatureKind,
    slug: &str,
    viewpoint: &Viewpoint,
    clear: &KeepClear,
) -> Option<([f32; 2], f32)> {
    let half = clear.width * 0.5;
    let entry = crate::catalogue::by_slug(slug);
    let radius = match (kind, entry) {
        (FeatureKind::GiantTree, _) => CLEARING,
        (FeatureKind::SeaStack, _) => STACK_RADIUS,
        (_, Some(entry)) => entry.footprint().clearance,
        (_, None) => return None,
    };
    let landing = viewpoint.landing;
    let heading = viewpoint.toward;
    let cell = probe.cell_size();
    let steps = (FARTHEST / cell).ceil() as i32;
    let edge = probe.half_extent() - radius;
    let mut best: Option<(f32, [f32; 2])> = None;
    for j in -steps..=steps {
        for i in -steps..=steps {
            let at = [landing[0] + i as f32 * cell, landing[1] + j as f32 * cell];
            let (dx, dz) = (at[0] - landing[0], at[1] - landing[1]);
            let far = (dx * dx + dz * dz).sqrt();
            if !(NEAREST..=FARTHEST).contains(&far) || at[0].abs() > edge || at[1].abs() > edge {
                continue;
            }
            let off = (dx * heading[0] + dz * heading[1]) / far;
            if off > LEAST_OFF {
                continue;
            }
            let kept_clear = clear.stands.iter().all(|&(spot, r)| {
                let reach = r + radius + MARGIN;
                distance2(at, spot) >= reach * reach
            }) && clear
                .paths
                .iter()
                .flat_map(|line| line.windows(2))
                .all(|pair| off_segment(at, pair[0], pair[1]) >= half + radius + MARGIN);
            if !kept_clear {
                continue;
            }
            let height = probe.height_between(at);
            let (fits, shows_at) = match (kind, entry) {
                (FeatureKind::GiantTree, _) => {
                    let foot = probe.footprint(at, CLEARING * 0.5);
                    (
                        foot.above_water >= TREE_FREEBOARD && foot.steepest <= TREE_SLOPE,
                        height + 12.0,
                    )
                }
                (FeatureKind::SeaStack, _) => (stands_at_sea(probe, at), probe.water_y() + 6.0),
                (FeatureKind::HilltopRuin, Some(entry)) => (
                    probe.prominence(at) >= CREST && built_fits(probe, at, entry, radius),
                    height + 4.0,
                ),
                (_, Some(entry)) => (built_fits(probe, at, entry, radius), height + 2.0),
                (_, None) => (false, height),
            };
            if !fits {
                continue;
            }
            let mut score = if viewpoint.shows(probe, at, shows_at, radius) {
                3.0
            } else {
                0.0
            };
            score -= (off - BEST_OFF).abs() * 8.0;
            score -= ((far - BEST_REACH) / BEST_REACH).abs();
            if off < 0.0 {
                // Behind the arrival: met on the way out, not seen coming in.
                score -= 2.0;
            }
            score += match kind {
                FeatureKind::HilltopRuin => (probe.prominence(at) * 0.1).min(1.5),
                FeatureKind::StoneRing => (probe.prominence(at) * 0.05).clamp(0.0, 0.5),
                FeatureKind::GiantTree | FeatureKind::SeaStack | FeatureKind::Lake => 0.0,
            };
            if best.is_none_or(|(kept, _)| score > kept) {
                best = Some((score, at));
            }
        }
    }
    best.map(|(_, at)| (at, radius))
}

/// Whether a built feature of `entry`, `radius` round, fits the ground at
/// `at`: as a member does ([`member_fits`]), its ground rising and falling
/// [`FEATURE_RELIEF`] at the most, and gentle on a ring [`CLIFF_REACH`]
/// times its reach out - no cliff falls away beside it.
fn built_fits(
    probe: &TerrainProbe,
    at: [f32; 2],
    entry: &dyn crate::catalogue::CatalogueEntry,
    radius: f32,
) -> bool {
    member_fits(probe, at, entry, 1.0, 0.0)
        && probe.footprint(at, radius).relief <= FEATURE_RELIEF
        && probe.footprint(at, radius * CLIFF_REACH).steepest <= SITE_SLOPE
}

/// Whether a sea stack fits the water at `at`: [`STACK_DEPTH`] deep there,
/// [`STACK_FLOOR`] deep at the least all round its reach, and its nearest
/// shore [`STACK_OFFSHORE`] off.
fn stands_at_sea(probe: &TerrainProbe, at: [f32; 2]) -> bool {
    let depth = |p: [f32; 2]| probe.water_y() - probe.height_between(p);
    // Eight points round its reach, a unit circle's eighths.
    const D: f32 = std::f32::consts::FRAC_1_SQRT_2;
    const ROUND: [[f32; 2]; 8] = [
        [1.0, 0.0],
        [D, D],
        [0.0, 1.0],
        [-D, D],
        [-1.0, 0.0],
        [-D, -D],
        [0.0, -1.0],
        [D, -D],
    ];
    (STACK_DEPTH[0]..=STACK_DEPTH[1]).contains(&depth(at))
        && ROUND.iter().all(|&[x, z]| {
            depth([at[0] + x * STACK_RADIUS, at[1] + z * STACK_RADIUS]) >= STACK_FLOOR
        })
        && land_within(probe, at, STACK_OFFSHORE[1]).is_some_and(|d| d >= STACK_OFFSHORE[0])
}

/// The generator `feature` draws for the world of `did` (#1781): its
/// `tree` - the first of the world's stands whose tree stands as a giant -
/// drawn [`GIANT`] times its size; a [`sea_stack`] of its rock, `stone` and
/// `gap`; or its ruin or its stone ring as built. `None` for a giant tree
/// where the world has no such tree, or a built feature whose entry no
/// longer resolves.
pub fn generator(
    feature: &SignatureFeature,
    did: &str,
    tree: Option<&Generator>,
    stone: [f32; 3],
    gap: [f32; 3],
) -> Option<Generator> {
    match feature.kind {
        FeatureKind::GiantTree => {
            let mut giant = tree?.clone();
            crate::seeded_defaults::room::build::scale_about_ground(&mut giant, GIANT);
            Some(giant)
        }
        FeatureKind::SeaStack => Some(sea_stack(stone, gap, feature.seed)),
        FeatureKind::HilltopRuin | FeatureKind::StoneRing => {
            crate::catalogue::by_slug(feature.slug).map(|entry| entry.build(did))
        }
        // A terrain water layer, not a placement (`build`).
        FeatureKind::Lake => None,
    }
}

/// How far from `at`, `reach` metres at the most, the nearest dry ground
/// lies, read a probe cell apart - `None` where none lies that near.
fn land_within(probe: &TerrainProbe, at: [f32; 2], reach: f32) -> Option<f32> {
    let cell = probe.cell_size();
    let steps = (reach / cell).ceil() as i32;
    let mut nearest: Option<f32> = None;
    for j in -steps..=steps {
        for i in -steps..=steps {
            let (dx, dz) = (i as f32 * cell, j as f32 * cell);
            let d = (dx * dx + dz * dz).sqrt();
            if d <= reach
                && nearest.is_none_or(|kept| d < kept)
                && probe.height_between([at[0] + dx, at[1] + dz]) > probe.water_y()
            {
                nearest = Some(d);
            }
        }
    }
    nearest
}

/// A sea stack (#1781): rock of the world's own - `stone` and `gap` its
/// palette's light and dark - hewn as its boulders are, faceted chunks
/// twisted and tapered, piled into a stack rising some 26 m off the sea
/// floor at the generator's origin, with a lesser one beside it; the
/// chunks' turns and twist are `seed`'s. The stack stands at least 14 m
/// clear of the deepest water it is sited in ([`STACK_DEPTH`]).
pub fn sea_stack(stone: [f32; 3], gap: [f32; 3], seed: u64) -> Generator {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let twist = 0.3 + unit_f32(&mut rng) * 0.3;
    let chunk = |radius: f32, twist: f32| {
        Generator::from_kind(GeneratorKind::Sphere {
            radius: Fp(radius),
            resolution: 1,
            common: PrimCommon {
                solid: true,
                material: SovereignMaterialSettings {
                    base_color: Fp3(stone),
                    roughness: Fp(0.95),
                    uv_scale: Fp(1.5),
                    texture: SovereignTextureConfig::Rock(SovereignRockConfig {
                        color_light: Fp3(stone),
                        color_dark: Fp3(gap),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                torture: TortureParams {
                    twist: Fp(twist),
                    taper: Fp2([0.3, 0.3]),
                    ..Default::default()
                },
                ..Default::default()
            },
        })
    };
    // The stack's foot, the root: unstretched, so that what stands on it,
    // in its frame, keeps its own shape.
    let mut stack = chunk(6.2, twist);
    stack.transform.translation = Fp3([0.0, 4.5, 0.0]);
    // Each `(offset in the foot's frame, radius, stretch)`: the stack's
    // upper chunks, then the lesser stack's two.
    let parts = [
        ([0.8, 9.0, 0.3], 4.8, [0.95, 1.5, 1.0]),
        ([-0.5, 16.5, 0.6], 3.4, [1.0, 1.45, 0.9]),
        ([8.0, -0.5, 2.4], 3.6, [1.0, 1.5, 1.0]),
        ([8.3, 5.0, 2.0], 2.6, [0.9, 1.4, 1.0]),
    ];
    for (offset, radius, stretch) in parts {
        let mut part = chunk(radius, -twist);
        // libm's (#1132): the turn is in the record every peer derives.
        let half = unit_f32(&mut rng) * std::f32::consts::PI;
        part.transform.translation = Fp3(offset);
        part.transform.rotation = Fp4([0.0, libm::sinf(half), 0.0, libm::cosf(half)]);
        part.transform.scale = Fp3(stretch);
        stack.children.push(part);
    }
    stack
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A world's table: a sea stack off a coast, a giant tree only where
    /// trees grow and the world has one that stands as a giant, a ruin on
    /// hilly land, and a stone ring last, everywhere.
    #[test]
    fn the_table_offers_what_the_land_holds() {
        let mut scene = SceneCharacter::for_seed(1);
        scene.landform = LandformArchetype::Archipelago;
        scene.biome = BiomeArchetype::Lush;
        assert_eq!(
            FeatureKind::candidates(&scene, true),
            [
                FeatureKind::SeaStack,
                FeatureKind::GiantTree,
                FeatureKind::StoneRing
            ]
        );
        assert_eq!(
            FeatureKind::candidates(&scene, false),
            [FeatureKind::SeaStack, FeatureKind::StoneRing]
        );
        scene.landform = LandformArchetype::Craggy;
        scene.biome = BiomeArchetype::Glacial;
        assert_eq!(
            FeatureKind::candidates(&scene, true),
            [FeatureKind::HilltopRuin, FeatureKind::StoneRing]
        );
        assert!(stands_as_giant("lsys_oak") && stands_as_giant("lsys_acacia"));
        for not_a_giant in ["lsys_bush", "lsys_bamboo", "lsys_fern", "lsys_mangrove"] {
            assert!(!stands_as_giant(not_a_giant), "{not_a_giant}");
        }
        for slug in GIANT_TREES {
            assert!(crate::catalogue::by_slug(slug).is_some(), "{slug}");
        }
    }

    /// The nearest dry ground off a straight coast is read straight across
    /// it - no nearer than it lies, and at most a cell further, the cells
    /// being where it is read - and none where the sea runs past the reach.
    #[test]
    fn the_land_is_read_across_the_water() {
        // The sea's samples west of x = 0: the water line crosses x = 1,
        // 36 m across from x = -35.
        const GRID: usize = 96;
        let half = (GRID - 1) as f32 * 10.0 * 0.5;
        let map = gen_jobs::HeightmapData {
            width: GRID as u32,
            height: GRID as u32,
            scale: 10.0,
            data: (0..GRID * GRID)
                .map(|i| {
                    if (i % GRID) as f32 * 10.0 - half < 0.0 {
                        -6.0
                    } else {
                        4.0
                    }
                })
                .collect(),
        };
        let coast = TerrainProbe::new(&map, 0.0, crate::seeded_defaults::BUILD_SLOPE_LIMIT);
        let near = land_within(&coast, [-35.0, 5.0], 70.0).expect("the coast is in reach");
        assert!((36.0..=46.0).contains(&near), "{near}");
        assert_eq!(land_within(&coast, [-200.0, 5.0], 70.0), None);
        // 13 m off: shallow at its reach and short of the shore a stack
        // keeps; 60 m out, deep all round, it stands.
        assert!(!stands_at_sea(&coast, [-12.0, 5.0]));
        assert!(stands_at_sea(&coast, [-60.0, 5.0]));
    }

    /// A feature draws what it is: the world's tree two and a half times its
    /// size about its foot, a ruin as it was authored, fallen, and a stack of
    /// five chunks whose turns are its seed's.
    #[test]
    fn a_feature_draws_what_it_is() {
        let feature = |kind, slug| SignatureFeature {
            kind,
            at: [0.0, 0.0],
            yaw: 0.0,
            radius: 10.0,
            slug,
            seed: 7,
            lake: None,
        };
        let oak = crate::catalogue::by_slug("lsys_oak")
            .expect("the oak")
            .build("did:plc:feature");
        let giant = generator(
            &feature(FeatureKind::GiantTree, ""),
            "did:plc:feature",
            Some(&oak),
            [0.5; 3],
            [0.2; 3],
        )
        .expect("a giant");
        assert_eq!(
            giant.transform.scale.0,
            oak.transform.scale.0.map(|s| s * GIANT)
        );
        assert_eq!(
            generator(
                &feature(FeatureKind::GiantTree, ""),
                "did:plc:feature",
                None,
                [0.5; 3],
                [0.2; 3]
            ),
            None
        );
        let built = |slug| {
            crate::catalogue::by_slug(slug)
                .expect("a ruin")
                .build("did:plc:feature")
        };
        let ruin = |slug| {
            generator(
                &feature(FeatureKind::HilltopRuin, slug),
                "did:plc:feature",
                None,
                [0.5; 3],
                [0.2; 3],
            )
            .expect("a ruin")
        };
        for slug in RUINS {
            assert_eq!(ruin(slug), built(slug), "{slug}");
        }
        assert!(FeatureKind::HilltopRuin.is_built() && FeatureKind::StoneRing.is_built());
        assert!(!FeatureKind::GiantTree.is_built() && !FeatureKind::SeaStack.is_built());
        let stack = sea_stack([0.5; 3], [0.2; 3], 7);
        assert_eq!(stack.children.len(), 4);
        assert_ne!(stack, sea_stack([0.5; 3], [0.2; 3], 8), "the seed turns it");
    }
}
