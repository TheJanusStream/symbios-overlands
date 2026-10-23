//! Ground-cover props (#911) - the cheap scatter tier below the trees.
//!
//! Every entry here is one to four primitives, not an L-system: the seeded
//! ground-cover scatter places these by the hundred, so per-instance part
//! cost is the binding constraint - a browser pays for every part of every
//! copy each frame. Three shapes cover the whole tier:
//!
//! * **Turned cards** - one
//!   [`lathe`](crate::catalogue::items::util::lathe) cylinder wearing a
//!   vegetation texture whose atlas is wrapped once round it, flared or
//!   arched by its torture: one part that reads full from every side. The
//!   grass tuft, the fern clump and the reed clump are this shape (#1711),
//!   each the generator the agent Hypha saved in its region, the Understory
//!   (#1708), where the owner judged them better than the crossed cards they
//!   replaced; the dry grass tuft and the shore grass are the grass tuft's
//!   recipe in their own colours (#1715), and the catalogue's fern
//!   ([`lsys_fern`](crate::catalogue::items::plants::lsys_fern)) the fern's.
//!   The recipe is spelled out in `docs/agent/region.md` (a browser pays per
//!   part).
//! * **Crossed cards** - two alpha-masked quads at 90 degrees, each carrying
//!   one of the WS1 vegetation textures. Crossing them means the prop reads
//!   from every yaw instead of vanishing edge-on, which a single card does.
//!   The wildflower and the dwarf shrub are still this shape: the
//!   wildflower's blossoms are cards of their own, which the seeded tint
//!   colours scatter by scatter (#1703), and a `Broadleaf` leaf is one
//!   sprite, not an atlas of clumps a lathe could wrap.
//! * **Cushions** - encrusting cover (moss, lichen) carrying opaque
//!   *surface* textures rather than alpha cards, so a flat quad would read as
//!   a square stamped on the terrain. Each is one
//!   [`blob_group`](crate::catalogue::items::util::blob_group) of six domes
//!   (`dome`) melted into one mat, its domes' centres under the ground: the
//!   moss is Hypha's cushion, the lichen the same recipe flattened to a
//!   crust (#1715).
//!
//! ## Card geometry
//!
//! A `Plane` is horizontal by default, so a standing card is the root rotated
//! `+90` degrees about X. Children inherit the root's rotation, so the
//! crossing card cannot simply be given a world-space `Ry(90)` - its *local*
//! rotation must be `Rx(-90)*Ry(90)*Rx(90)`, which reduces to `Rz(-90)`.
//! Getting this wrong splays the second card flat instead of crossing it (see
//! the rotated-root trap in the catalogue notes).
//!
//! ## Turned-card geometry
//!
//! A turned card's stations all stand at radius 0.1 m, so its circumference
//! is 0.628 m and a `uv_scale` of 1.5915 (one over that) spans the texture
//! once round it; the atlas's columns become the clump's blades or fronds.
//! Its stations start 0.1203 m below its own origin and its root transform
//! lifts it by just that much times its height scale, so its foot stands on
//! the ground (within a few millimetres) and the wind's card profile, which
//! weighs sway by height above the entity's origin, all but holds the foot
//! (a few millimetres, under 2 cm in a storm's gust front) and moves the
//! tips (see `wind::WindSway::Card`).
//!
//! ## Entity cost
//!
//! | Prop | Parts |
//! |---|---|
//! | grass tuft, dry grass tuft, shore grass, fern clump, reed clump, moss, lichen | 1 |
//! | dwarf shrub | 2 |
//! | wildflower, lily pads | 4 |

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    blob_ellipsoid, blob_group, lathe, prim, quat_mul, quat_x, quat_y, quat_z,
};
use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::PrimCommon;
use crate::pds::generator::BlobElement;
use crate::pds::{
    Fp, Fp2, Fp3, Fp4, Fp64, Generator, GeneratorKind, SovereignBroadleafConfig,
    SovereignFlowerConfig, SovereignGrassTuftConfig, SovereignLichenConfig,
    SovereignMaterialSettings, SovereignMossConfig, SovereignNeedleConfig, SovereignReedConfig,
    SovereignTextureConfig, TortureParams, TransformData,
};

/// A single quad carrying `material`, `size` metres, at `translation` with
/// `rotation`.
fn quad(
    size: [f32; 2],
    translation: [f32; 3],
    rotation: Fp4,
    material: SovereignMaterialSettings,
) -> Generator {
    prim(
        GeneratorKind::Plane {
            size: Fp2(size),
            subdivisions: 0,
            // Ground cover is never collidable: you walk through grass, and a
            // collider per card would cost a physics body per instance on a
            // tier placed by the hundred.
            common: PrimCommon::with_material(material),
        },
        translation,
        rotation,
    )
}

/// Two standing quads crossed at 90°, rooted on the ground plane.
///
/// `width` / `height` are the card's metre extents; the pair is centred on the
/// origin in XZ with its base at `y = 0`.
fn crossed_cards(width: f32, height: f32, material: SovereignMaterialSettings) -> Generator {
    // Root: stood upright, lifted so its base meets the ground.
    let mut root = quad(
        [width, height],
        [0.0, height * 0.5, 0.0],
        quat_x(FRAC_PI_2),
        material.clone(),
    );
    // Crossing card: local rotation only, since it inherits the root's.
    root.children.push(quad(
        [width, height],
        [0.0, 0.0, 0.0],
        quat_z(-FRAC_PI_2),
        material,
    ));
    root
}

/// How far under the ground a cushion's domes are centred (m).
const DOME_DEPTH: f32 = 0.08;

/// One dome of a cushion (the moss's recipe, Hypha's): a flattened
/// ellipsoid `radius` wide and `height` tall, its centre `at` in the plane
/// and [`DOME_DEPTH`] under the ground, blended into its neighbours.
///
/// A flat quad would be simpler, but moss and lichen carry *opaque* surface
/// textures rather than alpha cards, so a quad reads as an unmistakable
/// square patch stamped on the terrain. Domes centred under the ground show
/// only their caps, melted into one mat by the blend - no silhouette edge to
/// give the trick away, and no coplanar z-fight, since each cap crosses the
/// ground at an angle rather than lying flush with it.
fn dome(at: [f32; 2], radius: f32, height: f32) -> BlobElement {
    blob_ellipsoid([at[0], -DOME_DEPTH, at[1]], [radius, height, radius], 0.09)
}

/// Matte card material - ground cover is never glossy, and `base_color` stays
/// near white so the generator's own palette shows through unmodulated.
fn card_material(texture: SovereignTextureConfig) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([1.0, 1.0, 1.0]),
        roughness: Fp(0.9),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        texture,
        ..Default::default()
    }
}

/// A turned card's stations start this far below its origin (m), as every
/// Hypha's turned card's do; its root lifts it by this times its height
/// scale, so its foot stands on the ground.
const TURNED_FOOT: f32 = -0.1203;

/// The station heights of the fern recipe (m): five rings, so the arching
/// bend reads as a curve rather than a kink.
const FERN_STATIONS: [f32; 5] = [TURNED_FOOT, 0.0777, 0.2426, 0.3811, 0.5395];

/// The station heights of a grass or reed tuft (m): three rings, the least
/// that carries a flare.
const TUFT_STATIONS: [f32; 3] = [TURNED_FOOT, 0.2426, 0.5395];

/// What a turned card wears: `texture` under a white base colour, its atlas
/// wrapped once round the 0.1 m radius (`uv_scale` one over the 0.628 m
/// circumference), nudged down a hair as Hypha's are.
fn turned_material(roughness: f32, texture: SovereignTextureConfig) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([1.0, 1.0, 1.0]),
        roughness: Fp(roughness),
        uv_scale: Fp(1.5915),
        uv_offset: Fp2([0.0, -0.0314]),
        texture,
        ..Default::default()
    }
}

/// A turned card (see the module docs): a `resolution`-sided cylinder of
/// radius 0.1 m through `stations`, joined straight, shaped by `torture`,
/// stood `lift` metres up its root and scaled by `scale`. Never solid: you
/// walk through ground cover.
fn turned_card(
    stations: &[f32],
    resolution: u32,
    material: SovereignMaterialSettings,
    torture: TortureParams,
    lift: f32,
    scale: [f32; 3],
) -> Generator {
    let points: Vec<(f32, f32)> = stations.iter().map(|&height| (0.1, height)).collect();
    let mut kind = lathe(&points, resolution, false, material);
    if let Some(common) = kind.common_mut() {
        common.torture = torture;
    }
    Generator {
        transform: TransformData {
            translation: Fp3([0.0, lift, 0.0]),
            scale: Fp3(scale),
            ..Default::default()
        },
        ..Generator::from_kind(kind)
    }
}

/// One fern of the fern recipe, Hypha's (#1468, #1708): a turned card of
/// eight sides, each one cell of an eight-frond `Needle` atlas, narrowed to
/// a point at its foot (`taper_bottom`), flared wide at its top (`taper`),
/// swollen in its middle (`bulge`) and arched over by `bend`, so the eight
/// fronds rise from one crown in a vase and spill outward. `seed`, `pairs`
/// and the two colours (linear RGB: the pinnae's bases and tips) make one
/// fern differ from another, `twist` turns the fronds about the crown, and
/// `scale` and `lift` size it. The rachis colour and the pinnae's shape are
/// the recipe's.
#[allow(clippy::too_many_arguments)]
pub(in crate::catalogue::items) fn fern(
    seed: u32,
    pairs: u32,
    base: [f32; 3],
    tip: [f32; 3],
    twist: f32,
    bend: [f32; 3],
    lift: f32,
    scale: [f32; 3],
) -> Generator {
    turned_card(
        &FERN_STATIONS,
        8,
        turned_material(
            1.0,
            SovereignTextureConfig::Needle(SovereignNeedleConfig {
                seed,
                variant_cols: 8,
                pair_count: pairs,
                color_base: Fp3(base),
                color_tip: Fp3(tip),
                color_shoot: Fp3([0.1065, 0.233, 0.0174]),
                needle_angle: Fp64(85.0),
                needle_length: Fp64(0.48),
                needle_width: Fp64(0.017),
                shoot_length: Fp64(0.86),
                shoot_width: Fp64(0.03),
                normal_strength: Fp(0.6),
                ..Default::default()
            }),
        ),
        TortureParams {
            twist: Fp(twist),
            taper: Fp2([-0.99, -0.99]),
            taper_bottom: Fp2([0.95, 0.95]),
            bend: Fp3(bend),
            bulge: Fp2([0.4, 0.4]),
            ..Default::default()
        },
        lift,
        scale,
    )
}

/// A grass or reed tuft (session 879's recipe, `docs/agent/region.md`):
/// a six-sided turned card wearing `texture`, its three-cell atlas wrapped
/// once round, flared toward its top by `flare` (a negative `taper`) and
/// turned by `twist`.
fn tuft(
    texture: SovereignTextureConfig,
    twist: f32,
    flare: f32,
    lift: f32,
    scale: [f32; 3],
) -> Generator {
    turned_card(
        &TUFT_STATIONS,
        6,
        turned_material(0.9, texture),
        TortureParams {
            twist: Fp(twist),
            taper: Fp2([flare, flare]),
            ..Default::default()
        },
        lift,
        scale,
    )
}

// ---------------------------------------------------------------------------
// Entries
// ---------------------------------------------------------------------------

/// Declare a ground-cover entry: slug, display name, description, and the
/// closure building its generator. Every entry is `StructureRole::Plant`.
macro_rules! ground_cover_entry {
    ($(#[$doc:meta])* $ty:ident, $slug:literal, $name:literal, $desc:literal, $build:expr) => {
        $(#[$doc])*
        pub struct $ty;

        impl CatalogueEntry for $ty {
            fn slug(&self) -> &'static str {
                $slug
            }
            fn name(&self) -> &'static str {
                $name
            }
            fn description(&self) -> &'static str {
                $desc
            }
            fn role(&self) -> StructureRole {
                StructureRole::Plant
            }
            fn build(&self, _local_did: &str) -> Generator {
                let f: fn() -> Generator = $build;
                f()
            }
        }
    };
}

ground_cover_entry!(
    /// Grass tuft - the ground-cover workhorse: one six-sided turned card
    /// flared from a narrow foot to a wide crown, the `GrassTuft` texture's
    /// three-tuft atlas wrapped once round it, so the blades read as a full
    /// clump from every side. It is the Understory's `heath_grass` as the
    /// agent Hypha saved it (session 879, #1708), at the catalogue's size
    /// rather than Hypha's: Hypha grew it 1.6 times larger for a heath, and
    /// the seeded meadows and the grass profile were tuned for the
    /// 0.45 m card it replaced, so its height scale is 0.68 where Hypha's
    /// is 1.15 and its width 1.5 where Hypha's is 2.7 (the root lift
    /// following, 0.08 for 0.13). Its texture is the default `GrassTuft` with
    /// `variant_cols` 3, so the seeded grass tint (#1703) and the Auto grass
    /// profile read the same colours from it as from the card.
    ///
    /// Measured with the render tool: 0.45 m tall and 0.57 x 0.52 m
    /// across, 36 triangles in 1 part (the crossed cards it replaced: 0.45
    /// m by 0.55 m, 4 triangles in 2 parts).
    GrassTuft,
    "gc_grass_tuft",
    "Grass Tuft",
    "A flared clump of grass blades - the ground-cover workhorse.",
    || tuft(
        SovereignTextureConfig::GrassTuft(SovereignGrassTuftConfig {
            variant_cols: 3,
            ..Default::default()
        }),
        0.12,
        -0.9,
        0.08,
        [1.5, 0.68, 1.5],
    )
);

ground_cover_entry!(
    /// Dry grass tuft - sun-bleached bunchgrass: the grass tuft's turned
    /// card ([`tuft`]) at the size of the 0.4 m card it replaced, a little
    /// lower and wider than the green tuft and turned a little further. Its
    /// texture is the card's - its straw colours, seven blades in ten dry -
    /// with the atlas's three columns added and its own seed, so its three
    /// clumps are not the green tuft's: a savanna scatters both, and the
    /// seeded grass tint (#1703) gives both the world's colours. The tint
    /// and the Auto grass profile read the same colours and dry share from
    /// it as from the card.
    ///
    /// Measured with the render tool: 0.40 m tall and 0.59 x 0.56 m
    /// across, 36 triangles in 1 part (the crossed cards: 0.4 m by 0.6 m, 4
    /// triangles in 2 parts).
    DryGrassTuft,
    "gc_dry_grass_tuft",
    "Dry Grass Tuft",
    "Sun-bleached grass clump - savanna, badlands and arid ground cover.",
    || tuft(
        SovereignTextureConfig::GrassTuft(SovereignGrassTuftConfig {
            seed: 1,
            variant_cols: 3,
            color_base: Fp3([0.20, 0.17, 0.07]),
            color_tip: Fp3([0.47, 0.40, 0.15]),
            color_dry: Fp3([0.52, 0.44, 0.18]),
            dry_fraction: Fp64(0.7),
            ..Default::default()
        }),
        0.15,
        -0.9,
        0.072,
        [1.58, 0.6063, 1.58],
    )
);

ground_cover_entry!(
    Wildflower,
    "gc_wildflower",
    "Wildflower Clump",
    "Grass tuft topped with a blossom - meadow and verge colour.",
    || {
        let mut root = crossed_cards(
            0.45,
            0.4,
            card_material(SovereignTextureConfig::GrassTuft(
                SovereignGrassTuftConfig {
                    color_tip: Fp3([0.34, 0.48, 0.15]),
                    dry_fraction: crate::pds::Fp64(0.1),
                    ..Default::default()
                },
            )),
        );
        // One blossom card seated among the blades. Its local rotation is
        // relative to the already-rotated root, same as the crossing card;
        // local -Z maps to world +Y through the root's Rx(90), so this sits
        // the flower just below the blade tips rather than floating above.
        let blossom = card_material(SovereignTextureConfig::Flower(SovereignFlowerConfig {
            // One blossom per card: the atlas default bakes a 2x2 grid, which
            // on a single quad reads as four floating flowers.
            variant_rows: 1,
            variant_cols: 1,
            ..Default::default()
        }));
        // Crossed like the tuft: a lone blossom card disappears edge-on, and
        // with random per-instance yaw that means half a meadow's flowers
        // wink out as the camera turns.
        root.children.push(quad(
            [0.2, 0.2],
            [0.0, 0.0, -0.12],
            quat_z(-FRAC_PI_2),
            blossom.clone(),
        ));
        root.children.push(quad(
            [0.2, 0.2],
            [0.0, 0.0, -0.12],
            Fp4([0.0, 0.0, 0.0, 1.0]),
            blossom,
        ));
        root
    }
);

ground_cover_entry!(
    /// Fern clump - eight fronds rising from one crown in an arching vase:
    /// the fern recipe ([`fern`]), a turned card of eight sides each wearing
    /// one cell of an eight-frond `Needle` atlas of fine pinnae, pinched to
    /// a point at its foot, flared and arched over at its top, so the fronds
    /// spill outward and one side leans further than the other. It is the
    /// Understory's `fern_clump` as the agent Hypha saved it (#1468, #1708),
    /// reproduced to the ten-thousandth, root transform included; the owner
    /// on first sight: "Looking good to me!". It replaced two crossed Frond
    /// cards.
    ///
    /// Measured with the render tool: 0.48 m tall and 1.29 x 1.29 m across,
    /// 80 triangles in 1 part (the crossed cards: 0.6 m by 0.8 m, 4
    /// triangles in 2 parts).
    FernClump,
    "gc_fern_clump",
    "Fern Clump",
    "Arching fern fronds from one crown - forest-floor and jungle understory cover.",
    || fern(
        31,
        20,
        [0.0272, 0.1789, 0.0049],
        [0.0946, 0.3185, 0.01],
        0.22,
        [0.03, -0.4712, 0.0],
        0.25,
        [3.3166, 2.0788, 3.3166],
    )
);

ground_cover_entry!(
    /// Reed clump - tall shoreline reeds with cattail heads: one six-sided
    /// turned card, barely flared, the `Reed` texture's three-clump atlas
    /// wrapped once round it. It is the Understory's `pool_reeds` as the
    /// agent Hypha saved it (session 879, #1708), reproduced to the
    /// ten-thousandth at Hypha's size, which is the crossed cards' it
    /// replaced: the wading band (#914) still keeps its heads dry.
    ///
    /// Measured with the render tool: 1.50 m tall and 0.70 x 0.63 m across,
    /// 36 triangles in 1 part (the crossed cards: 1.5 m by 0.7 m, 4
    /// triangles in 2 parts).
    ReedClump,
    "gc_reed_clump",
    "Reed Clump",
    "Tall shoreline reeds with cattail heads - wetland and pond margins.",
    || tuft(
        SovereignTextureConfig::Reed(SovereignReedConfig {
            variant_cols: 3,
            ..Default::default()
        }),
        0.08,
        -0.35,
        0.27,
        [2.6, 2.27, 2.6],
    )
);

ground_cover_entry!(
    DwarfShrub,
    "gc_dwarf_shrub",
    "Dwarf Shrub",
    "Low woody cushion - tundra and alpine ground cover.",
    || crossed_cards(
        0.5,
        0.35,
        card_material(SovereignTextureConfig::Broadleaf(
            SovereignBroadleafConfig {
                color_base: Fp3([0.12, 0.18, 0.07]),
                color_edge: Fp3([0.28, 0.24, 0.10]),
                lobe_count: crate::pds::Fp64(3.0),
                radius: crate::pds::Fp64(0.8),
                ..Default::default()
            }
        ))
    )
);

ground_cover_entry!(
    /// Shore grass - marram on the strand line: the grass tuft's turned card
    /// ([`tuft`]) at the size of the 0.5 m card it replaced, stiffer than
    /// the green tuft - flared less and barely turned, so its blades stand
    /// up from a wider foot. Its texture is the card's - the marram's
    /// blue-green running to straw - with the atlas's three columns added
    /// and its own seed. It keeps its own colours in a seeded world, as the
    /// card did: the seeded grass tint leaves the shore's grass alone.
    ///
    /// Measured with the render tool: 0.50 m tall and 0.59 x 0.53 m
    /// across, 36 triangles in 1 part (the crossed cards: 0.5 m by 0.65 m, 4
    /// triangles in 2 parts).
    ShoreGrass,
    "gc_shore_grass",
    "Shore Grass",
    "Salt-bleached blue-green dune grass - the coastal waterline fringe.",
    || tuft(
        SovereignTextureConfig::GrassTuft(SovereignGrassTuftConfig {
            seed: 2,
            variant_cols: 3,
            // Marram-grass palette: glaucous blue-green blades running to
            // pale straw where the salt wind burns them.
            color_base: Fp3([0.09, 0.15, 0.10]),
            color_tip: Fp3([0.33, 0.43, 0.29]),
            color_dry: Fp3([0.51, 0.48, 0.28]),
            dry_fraction: Fp64(0.4),
            ..Default::default()
        }),
        0.06,
        -0.75,
        0.09,
        [1.7, 0.7578, 1.7],
    )
);

ground_cover_entry!(
    LilyPad,
    "gc_lily_pad",
    "Lily Pads",
    "Floating lily pads with a blossom - still-water cover for wetland pools.",
    || {
        // Pads are HORIZONTAL cards - a `Plane` needs no rotation - floating
        // at the water surface (the scatter opts into `float_on_water`).
        // Lifted a few centimetres so the card is never coplanar with the
        // water plane (see the z-fight gotcha in the catalogue notes); the
        // stagger between the two pads keeps them clear of each other too.
        let pad = |lobe_seed: u32| {
            card_material(SovereignTextureConfig::Broadleaf(
                SovereignBroadleafConfig {
                    seed: lobe_seed,
                    // One near-circular blade with the radial slit that
                    // reads as "water lily": a single shallow lobe fanned
                    // wide, cut by a deep basal notch, no petiole.
                    lobe_count: crate::pds::Fp64(1.0),
                    lobe_depth: crate::pds::Fp64(0.08),
                    fan_angle: crate::pds::Fp64(110.0),
                    radius: crate::pds::Fp64(0.95),
                    base_notch: crate::pds::Fp64(0.42),
                    petiole_length: crate::pds::Fp64(0.0),
                    color_base: Fp3([0.07, 0.20, 0.10]),
                    color_edge: Fp3([0.20, 0.34, 0.13]),
                    ..Default::default()
                },
            ))
        };
        let mut root = quad(
            [0.55, 0.55],
            [0.0, 0.045, 0.0],
            Fp4([0.0, 0.0, 0.0, 1.0]),
            pad(3),
        );
        root.children
            .push(quad([0.35, 0.35], [0.32, 0.012, 0.18], quat_y(1.1), pad(9)));
        // One blossom sitting on the big pad, crossed so it reads from
        // every yaw. Children inherit the root's (identity) rotation, so a
        // standing card is a plain Rx(90) and its cross adds a Y quarter
        // turn on top.
        let blossom = card_material(SovereignTextureConfig::Flower(SovereignFlowerConfig {
            variant_rows: 1,
            variant_cols: 1,
            petal: crate::pds::SovereignPetalConfig {
                color_base: Fp3([0.97, 0.92, 0.88]),
                color_edge: Fp3([0.94, 0.72, 0.80]),
                color_throat: Fp3([0.99, 0.90, 0.60]),
                ..Default::default()
            },
            center_color: Fp3([1.0, 0.85, 0.35]),
            ..Default::default()
        }));
        root.children.push(quad(
            [0.16, 0.16],
            [-0.06, 0.09, 0.04],
            quat_x(FRAC_PI_2),
            blossom.clone(),
        ));
        root.children.push(quad(
            [0.16, 0.16],
            [-0.06, 0.09, 0.04],
            quat_mul(quat_y(FRAC_PI_2), quat_x(FRAC_PI_2)),
            blossom,
        ));
        root
    }
);

ground_cover_entry!(
    /// Moss patch - a velvet cushion of six domes melted into one mat: one
    /// [`blob_group`] of six flattened ellipsoids, a big dome in the middle
    /// and five smaller ones round it, their centres 8 cm under the ground
    /// and the field sliced off below 30% of its height (`profile_cut`), so
    /// only the domes show and nothing lies flush with the terrain. Its
    /// `Moss` texture is darker and drier-flecked than the default, under a
    /// grey-green base colour. It is the Understory's `moss_cushion` as the
    /// agent Hypha saved it (#1708), reproduced to the ten-thousandth; it
    /// replaced one squashed lawn-green sphere.
    ///
    /// Measured with the render tool: 0.25 m above the ground and 1.76 x
    /// 1.76 m across, 348 triangles in 1 part (the sphere: 0.16 m above the
    /// ground and 1.4 m across, 720 triangles in 1 part).
    MossPatch,
    "gc_moss_patch",
    "Moss Patch",
    "Velvet moss cushion of melted domes - damp forest and boreal floors.",
    || {
        let mut kind = blob_group(
            vec![
                dome([0.0, 0.0], 0.58, 0.3532),
                dome([0.5638, 0.2052], 0.4, 0.176),
                dome([-0.0453, 0.518], 0.46, 0.166),
                dome([-0.6202, 0.2257], 0.34, 0.172),
                dome([-0.3394, -0.3394], 0.42, 0.1596),
                dome([0.35, -0.6062], 0.32, 0.1548),
            ],
            10,
            SovereignMaterialSettings {
                base_color: Fp3([0.72, 0.78, 0.66]),
                roughness: Fp(0.95),
                texture: SovereignTextureConfig::Moss(SovereignMossConfig {
                    seed: 57,
                    cushion_scale: Fp64(4.0),
                    color_deep: Fp3([0.01, 0.0331, 0.0049]),
                    color_tip: Fp3([0.1329, 0.2738, 0.0196]),
                    color_dry: Fp3([0.2633, 0.214, 0.055]),
                    dry_patches: Fp64(0.12),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        if let Some(common) = kind.common_mut() {
            common.torture.profile_cut = Fp2([0.3, 1.0]);
        }
        Generator::from_kind(kind)
    }
);

ground_cover_entry!(
    /// Lichen patch - a crust of lichen over stone: the moss's recipe
    /// flattened, one [`blob_group`] of six low domes ([`dome`]), two broad
    /// ones side by side and four small ones bulging their edge, none
    /// standing more than 9 cm proud, so their caps melt into one crust with
    /// an uneven, lobed edge, as the colonies of a crustose lichen grow and
    /// meet. Its domes are closed under the ground and none is cut, so on
    /// a gentle slope its downhill edge stays nearer the ground than the
    /// sphere's did (4 cm clear at 10 degrees, where the sphere's stood 8);
    /// on a steep one it stands clear as any cushion does, since a scatter
    /// leans its copies at random rather than to the ground. The default
    /// `Lichen` texture, sage and rust colonies over grey stone, under a
    /// white base colour. It replaced one squashed sphere, a round disc
    /// whose texture ran from a pole at its middle.
    ///
    /// Measured with the render tool: 0.09 m above the ground and 1.47 x
    /// 1.25 m across, 552 triangles in 1 part (the sphere: 0.09 m above the
    /// ground and 1.1 m across, 720 triangles in 1 part).
    LichenPatch,
    "gc_lichen_patch",
    "Lichen Patch",
    "Crustose lichen crust over stone - tundra ground cover.",
    || Generator::from_kind(blob_group(
        vec![
            dome([-0.14, 0.05], 0.42, 0.17),
            dome([0.26, -0.08], 0.34, 0.15),
            dome([0.12, 0.4], 0.24, 0.125),
            dome([-0.52, -0.16], 0.22, 0.12),
            dome([0.58, 0.16], 0.2, 0.115),
            dome([-0.1, -0.42], 0.25, 0.125),
        ],
        14,
        card_material(SovereignTextureConfig::Lichen(
            SovereignLichenConfig::default()
        )),
    ))
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// Every ground-cover entry, for the shared invariants below.
    fn all() -> Vec<&'static dyn CatalogueEntry> {
        vec![
            &GrassTuft,
            &DryGrassTuft,
            &Wildflower,
            &FernClump,
            &ReedClump,
            &ShoreGrass,
            &LilyPad,
            &DwarfShrub,
            &MossPatch,
            &LichenPatch,
        ]
    }

    #[test]
    fn entries_round_trip_through_sanitize() {
        for e in all() {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The whole point of this tier is that it is cheap. A ground-cover prop
    /// that grew a deep tree would blow the scatter's entity budget.
    #[test]
    fn entity_cost_stays_tiny() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        for e in all() {
            let n = count(&e.build(""));
            assert!(
                n <= 4,
                "{} costs {n} entities; ground cover must stay <= 4",
                e.slug()
            );
        }
    }

    /// Cards must stand on the ground, not float or sink: the root's base sits
    /// at y = 0 (cards) or just above it (decals).
    #[test]
    fn props_sit_on_the_ground() {
        for e in all() {
            let g = e.build("");
            let y = g.transform.translation.0[1];
            assert!(
                (0.0..1.0).contains(&y),
                "{} root y = {y}, expected to rest on the ground plane",
                e.slug()
            );
        }
    }

    /// Lily pads float at the water surface, so their cards must be
    /// horizontal (no rotation on the pad quads) and lifted clear of the
    /// water plane - a card at exactly y = 0 would be coplanar with the
    /// surface it floats on (the z-fight trap).
    #[test]
    fn lily_pads_are_horizontal_and_clear_of_the_water_plane() {
        let g = LilyPad.build("");
        assert!(
            g.transform.translation.0[1] > 0.02,
            "root pad must ride above the water plane, got y = {}",
            g.transform.translation.0[1]
        );
        // Root pad and the secondary pad keep the Plane's default horizontal
        // orientation: identity or yaw-only rotation (x/z components zero).
        let yaw_only = |q: &crate::pds::Fp4| q.0[0].abs() < 1e-6 && q.0[2].abs() < 1e-6;
        assert!(yaw_only(&g.transform.rotation), "root pad must lie flat");
        let second_pad = &g.children[0];
        assert!(
            yaw_only(&second_pad.transform.rotation),
            "secondary pad must lie flat"
        );
        assert!(
            second_pad.transform.translation.0[1] > 0.0,
            "pads must not be coplanar with each other"
        );
    }

    /// Cushion `g`'s domes, read from the built blob group: one part, six
    /// ellipsoids, each centred under the ground and flatter than it is
    /// wide, and every one of them showing a cap above the ground. Their
    /// caps make one mat: each dome's circle on the ground (where its
    /// surface crosses it) meets another's, so no cap stands apart as a
    /// patch of its own.
    fn assert_one_mat_of_buried_domes(g: &Generator, slug: &str) -> Vec<BlobElement> {
        assert!(g.children.is_empty(), "{slug}: one part");
        let GeneratorKind::BlobGroup { elements, .. } = &g.kind else {
            panic!("{slug}: a blob group");
        };
        assert_eq!(elements.len(), 6, "{slug}");
        // Each dome's centre in the plane and the radius it crosses the
        // ground at.
        let mut circles = Vec::new();
        for e in elements {
            let [rx, ry, rz] = e.radii.0;
            let [x, y, z] = e.position.0;
            assert_eq!(e.shape, crate::pds::generator::BlobShape::Ellipsoid);
            assert!(y < 0.0, "{slug}: a dome's centre is buried: {e:?}");
            assert!(
                ry < rx && rx == rz,
                "{slug}: a round dome flatter than wide: {e:?}"
            );
            assert!(y + ry > 0.0, "{slug}: a dome's cap shows: {e:?}");
            circles.push(([x, z], rx * (1.0 - (y / ry).powi(2)).sqrt()));
        }
        let mut joined = vec![0];
        while let Some(next) = (0..circles.len()).find(|&i| {
            !joined.contains(&i)
                && joined.iter().any(|&j| {
                    let ([ax, az], ar) = circles[i];
                    let ([bx, bz], br) = circles[j];
                    (ax - bx).hypot(az - bz) < ar + br
                })
        }) {
            joined.push(next);
        }
        assert_eq!(joined.len(), circles.len(), "{slug}: a cap stands apart");
        elements.clone()
    }

    /// The moss is one blob mat of domes whose centres lie under the
    /// ground, flatter than they are wide, with the field sliced off below
    /// the ground so no face lies flush with it.
    #[test]
    fn the_moss_is_one_mat_of_buried_domes() {
        let g = MossPatch.build("");
        assert_one_mat_of_buried_domes(&g, "gc_moss_patch");
        let common = g.kind.common().expect("a primitive");
        assert_eq!(common.torture.profile_cut, Fp2([0.3, 1.0]));
    }

    /// The lichen is the moss's recipe flattened to a crust (#1715): one mat
    /// of buried domes, none of them standing more than 9 cm proud where
    /// the moss's middle stands a quarter of a metre, and none cut, so each
    /// is closed under the ground and shows its own rounded flank, not a
    /// flat cut, where a slope falls away from it.
    #[test]
    fn the_lichen_is_a_crust_of_buried_domes() {
        let g = LichenPatch.build("");
        let domes = assert_one_mat_of_buried_domes(&g, "gc_lichen_patch");
        for e in &domes {
            let proud = e.position.0[1] + e.radii.0[1];
            assert!(proud <= 0.09 + 1e-6, "a crust, not a cushion: {e:?}");
        }
        let common = g.kind.common().expect("a primitive");
        assert_eq!(common.torture, TortureParams::default(), "no cut");
        assert!(matches!(
            common.material.texture,
            SovereignTextureConfig::Lichen(_)
        ));
    }

    /// The turned cards, the fern among them (#1711), and the dry and shore
    /// grass (#1715).
    fn turned() -> [&'static dyn CatalogueEntry; 6] {
        [
            &GrassTuft,
            &DryGrassTuft,
            &ShoreGrass,
            &FernClump,
            &ReedClump,
            &crate::catalogue::items::plants::lsys_fern::Fern,
        ]
    }

    /// The two cushions.
    fn cushions() -> [&'static dyn CatalogueEntry; 2] {
        [&MossPatch, &LichenPatch]
    }

    /// A turned card is one part, its foot on the ground: the lowest
    /// station lifted by the root lands within a few millimetres of it, so
    /// the clump neither hovers nor sinks, and the wind's card profile
    /// (which weighs sway by height over the root) all but holds the foot.
    /// This is what holds a turned card's foot: the floating guard cannot
    /// fail for an entry of one part.
    #[test]
    fn a_turned_card_stands_on_the_ground_in_one_part() {
        for e in turned() {
            let g = e.build("");
            assert!(g.children.is_empty(), "{}: one part", e.slug());
            let GeneratorKind::Lathe { points, smooth, .. } = &g.kind else {
                panic!("{}: a lathe", e.slug());
            };
            assert!(!smooth, "{}: flat faces, one atlas cell each", e.slug());
            let lowest = points
                .iter()
                .map(|p| p.height.0)
                .fold(f32::INFINITY, f32::min);
            let foot = g.transform.translation.0[1] + lowest * g.transform.scale.0[1];
            assert!(
                (-0.01..=0.001).contains(&foot),
                "{}: the foot stands at {foot} m",
                e.slug()
            );
            // Below the root, so the root (the sway's pivot) is inside the
            // clump and the foot weighs least.
            assert!(foot < g.transform.translation.0[1], "{}", e.slug());
        }
    }

    /// The new cover holds together: nothing draws a face where another
    /// does. The floating guard is the catalogue's usual pair, but each of
    /// these is one part, which it cannot fail: the feet are held by
    /// `a_turned_card_stands_on_the_ground_in_one_part` and the cushions'
    /// own tests.
    #[test]
    fn the_new_cover_holds_together() {
        let mut entries = turned().to_vec();
        entries.extend(cushions());
        for e in entries {
            let g = e.build("");
            assert_no_z_fighting(&g, e.slug());
            assert_nothing_floats(&g, e.slug());
        }
    }

    /// The grass tufts stand as tall as the cards they replaced - the green
    /// one 0.45 m, not Hypha's 1.6 times that, the dry one 0.4 m and the
    /// shore's 0.5 m - and each wears its card's texture: its colours and
    /// dry share as the card had them, the rest the default `GrassTuft`'s
    /// but for the atlas's three columns and, for the dry and shore grass,
    /// a seed of their own. So the seeded tint and the Auto grass profile,
    /// which read only the colours and the dry share, read the same from
    /// each as from its card.
    #[test]
    fn the_grass_tufts_are_their_cards_sizes_and_colours() {
        let coloured = |base, tip, dry, dry_fraction| SovereignGrassTuftConfig {
            color_base: Fp3(base),
            color_tip: Fp3(tip),
            color_dry: Fp3(dry),
            dry_fraction: Fp64(dry_fraction),
            ..Default::default()
        };
        // Each tuft, its card's height, its own seed and its card's texture.
        let cards: [(&dyn CatalogueEntry, f32, u32, SovereignGrassTuftConfig); 3] = [
            (&GrassTuft, 0.45, 0, SovereignGrassTuftConfig::default()),
            (
                &DryGrassTuft,
                0.4,
                1,
                coloured(
                    [0.20, 0.17, 0.07],
                    [0.47, 0.40, 0.15],
                    [0.52, 0.44, 0.18],
                    0.7,
                ),
            ),
            (
                &ShoreGrass,
                0.5,
                2,
                coloured(
                    [0.09, 0.15, 0.10],
                    [0.33, 0.43, 0.29],
                    [0.51, 0.48, 0.28],
                    0.4,
                ),
            ),
        ];
        for (entry, card_tall, seed, card) in cards {
            let g = entry.build("");
            let GeneratorKind::Lathe { points, common, .. } = &g.kind else {
                panic!("{}: a lathe", entry.slug());
            };
            let span = points.last().unwrap().height.0 - points[0].height.0;
            let tall = span * g.transform.scale.0[1];
            assert!(
                (tall - card_tall).abs() < 0.01,
                "{}: {tall} m tall",
                entry.slug()
            );
            assert_eq!(
                common.material.texture,
                SovereignTextureConfig::GrassTuft(SovereignGrassTuftConfig {
                    seed,
                    variant_cols: 3,
                    ..card
                }),
                "{}",
                entry.slug()
            );
        }
    }

    /// The turned cards sway as the cards did - their textures are the
    /// swaying kinds - and the cushions, surfaces, do not.
    #[test]
    fn the_turned_cards_sway_and_the_cushions_do_not() {
        for e in turned() {
            let g = e.build("");
            let texture = &g.kind.material().expect("a material").texture;
            assert!(crate::wind::sways(texture), "{}: {texture:?}", e.slug());
        }
        for e in cushions() {
            let g = e.build("");
            let texture = &g.kind.material().expect("a material").texture;
            assert!(!crate::wind::sways(texture), "{}: {texture:?}", e.slug());
        }
    }

    /// No re-skins: no seeded pool, Berlin or the garden bed names a
    /// variant for these eight (the ground-cover pools carry none, the fern
    /// stands are `plain`), and the only recolour they take is the seeded
    /// grass tint, which reaches the green and dry grass tufts' textures
    /// (`seeded_defaults::room::grass`). A variant added here would need a
    /// recolour test of its own, and the fern's a route through the tree
    /// scatters, which re-skin L-system materials only.
    #[test]
    fn the_new_cover_has_no_skins() {
        let mut entries = turned().to_vec();
        entries.extend(cushions());
        for e in entries {
            assert!(e.variants().is_empty(), "{}", e.slug());
        }
    }
}
