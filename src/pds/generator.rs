//! Open-union [`GeneratorKind`] and [`Placement`] enums - the building blocks
//! of a `RoomRecord`'s recipe. Both use `#[serde(other)] Unknown` so a client
//! visiting a room authored by a newer engine version skips unrecognised
//! variants instead of crashing its deserializer.
//!
//! **Unified Construct Model.** Every generator is hierarchical: it carries a
//! [`GeneratorKind`] (the variant-specific parameters), a local
//! [`TransformData`], and a `Vec<Generator>` of children. Any kind - primitive,
//! L-system, portal - can have children, so a portal can wear a doorframe, a
//! cuboid can carry a chimney, and Constructs are no longer a distinct kind.
//! Two positional rules survive sanitisation: `Terrain` is **root-only**
//! (it may carry children - the "region blueprint" shape - but a Terrain
//! nested as a child is rewritten to a default cuboid because the terrain
//! plugin owns the single world heightmap), and `Water` is **child-only
//! and leaf-only** (it needs an ancestor's transform to anchor its volume,
//! and its own `children` list is cleared at sanitisation time).

use super::prim::PropMeshType;
use super::terrain::SovereignTerrainConfig;
use super::texture::SovereignMaterialSettings;
use super::types::{
    BiomeFilter, Fp, Fp2, Fp3, Fp4, ScatterBounds, ScatterNaturalness, TransformData, default_true,
    is_false, is_true, map_u16_as_string, option_u64_as_string, sorted_string_map, u64_as_string,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-volume appearance and wave parameters for [`GeneratorKind::Water`].
///
/// Everything on this struct describes the water body itself (its colour,
/// choppiness, prevailing wave direction). Room-wide water settings -
/// detail-normal tiling, sun glitter strength, shoreline foam width - live on
/// [`crate::pds::Environment`] instead so they match the room's overall mood
/// rather than varying between adjacent water volumes.
///
/// `#[serde(default)]` at both struct and field level means a record that only
/// carries `level_offset` (the pre-overhaul schema) round-trips cleanly with
/// every appearance field filled in from [`WaterSurface::default`].
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct WaterSurface {
    /// sRGBA tint seen looking straight down (low alpha = transparent).
    pub shallow_color: Fp4,
    /// sRGBA tint seen at grazing angles (high alpha = opaque).
    pub deep_color: Fp4,
    /// PBR perceptual roughness. Water is typically very low (~0.05–0.12).
    pub roughness: Fp,
    /// PBR metallic. Water is dielectric so this is ~0.
    pub metallic: Fp,
    /// Schlick F0 reflectance - the base fraction of light reflected when
    /// viewed head-on. Real water is ~0.02; higher values bias toward a
    /// stylised, glossy look.
    pub reflectance: Fp,
    /// Global amplitude multiplier on the Gerstner waves. `0.0` = flat pond.
    pub wave_scale: Fp,
    /// Global time multiplier on the Gerstner waves. `0.0` = frozen.
    pub wave_speed: Fp,
    /// Prevailing wave direction in the world XZ plane. Need not be
    /// unit-length - the shader normalises.
    pub wave_direction: Fp2,
    /// Gerstner steepness in `[0, 1]`. `0` = smooth sines, `1` = sharp crests.
    pub wave_choppiness: Fp,
    /// Strength of the procedural foam on wave crests (`[0, 1]`).
    pub foam_amount: Fp,
    /// Force-per-metre-submerged applied to objects floating in this water,
    /// directed along the steepest-descent tangent of the surface (the
    /// projection of gravity onto the plane). `0.0` = still water; ~9.81 ≈
    /// "free-fall along the slope" for a 1-metre-deep avatar. Has no effect
    /// on flat water - the tangent component of gravity is then zero -
    /// which keeps existing rooms unchanged. This is the *physics* knob;
    /// the visual flow-map blend lives separately on `flow_amount`.
    pub flow_strength: Fp,
    /// Visual flow-map blend in `[0, 1]`. `0.0` = classic standing-wave
    /// Gerstner (still pond, even on a tilt - the existing look). `1.0` =
    /// pure flow-map mode (scrolling detail normals along the surface's
    /// downhill direction, suppressed Gerstner amplitude - the river /
    /// stream look). Mix in between for a choppy flowing river.
    /// Independent of `flow_strength` so a glassy "infinity-pool" effect
    /// (visible flow, no avatar push) is authorable.
    pub flow_amount: Fp,
    /// Strength of the avatar-wake ripple effect (Phase 1 of the
    /// interaction framework - see [`crate::interaction`]). `0.0`
    /// disables the effect entirely so existing scenes render
    /// unchanged. Higher values amplify the ripple per contact sample.
    pub wake_strength: Fp,
    /// Distance between ripple peaks in the wake, world metres.
    /// Smaller = tighter, busier ripples; larger = broader swells.
    pub wake_ripple_wavelength: Fp,
    /// Radial distance at which a single wake sample's contribution
    /// falls to `1/e` (~37%). Larger values produce wider wakes that
    /// reach further from the avatar; smaller values keep effects
    /// tightly localised.
    pub wake_decay_radius: Fp,
}

impl Default for WaterSurface {
    fn default() -> Self {
        // Defaults tuned against the six-Gerstner-wave table in water.wgsl.
        // Lower choppiness + moderate roughness keep the specular lobe wide
        // enough to absorb small residual normal errors without revealing
        // wave interference bands at grazing angles.
        Self {
            shallow_color: Fp4([0.18, 0.48, 0.56, 0.22]),
            deep_color: Fp4([0.02, 0.14, 0.24, 0.9]),
            roughness: Fp(0.14),
            metallic: Fp(0.0),
            reflectance: Fp(0.3),
            wave_scale: Fp(0.7),
            wave_speed: Fp(1.0),
            wave_direction: Fp2([1.0, 0.3]),
            wave_choppiness: Fp(0.3),
            foam_amount: Fp(0.25),
            flow_strength: Fp(0.0),
            flow_amount: Fp(0.0),
            // Wake effect off by default - existing rooms read as
            // pre-wake, only opt-in volumes show the ripples.
            wake_strength: Fp(0.0),
            wake_ripple_wavelength: Fp(1.5),
            wake_decay_radius: Fp(4.0),
        }
    }
}

// Default-eliding wire format (#695); the container `#[serde(default)]`
// above is the matching read-side contract.
crate::pds::serde_util::impl_default_eliding_serialize!(WaterSurface {
    shallow_color,
    deep_color,
    roughness,
    metallic,
    reflectance,
    wave_scale,
    wave_speed,
    wave_direction,
    wave_choppiness,
    foam_amount,
    flow_strength,
    flow_amount,
    wake_strength,
    wake_ripple_wavelength,
    wake_decay_radius,
});

/// Authored parameters for a [`GeneratorKind::RoadNetwork`] - a tensor-field
/// street grid that drapes over the parent terrain (see [`crate::urban`]). The
/// *config* is serialized / editable; the road *geometry* is recomputed
/// at load from this plus the heightmap, never stored. Like Water, a road
/// network is only valid as a child of a Terrain generator. Seeded rooms grow
/// no network (too heavy for a good default room on wasm) - this is
/// editor-opt-in, though records saved when roads were seeded still carry one.
///
/// Default-eliding wire format (#695): fields matching
/// [`RoadConfig::default`] are omitted on write; the container
/// `#[serde(default)]` restores them on read.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct RoadConfig {
    /// Master toggle - a disabled network grows no roads (the editor "off").
    pub enabled: bool,
    /// Seed for the road layout *alone*, so an author can re-roll the streets
    /// without disturbing terrain or settlement.
    #[serde(with = "u64_as_string")]
    pub seed: u64,
    /// Half-extent (m from the district centre) of the district the network
    /// fills.
    pub district_half_extent: Fp,
    /// District centre offset (m, XZ) from the room origin (#889). Zero =
    /// the historical spawn-centred district; a non-zero offset moves the
    /// whole built-up area (streets, lots, the wild-scatter exclusion)
    /// without touching the terrain. The road builder clamps the window
    /// back inside the heightmap when the offset pushes it past an edge.
    pub center: Fp2,
    /// Street-plan character (#890): how the tensor field trades the
    /// axis-aligned grid against terrain-following directions.
    pub style: RoadStyle,
    /// Whether streets stop at the room's water line (#1552): the tracer
    /// is handed the water level, so no street starts under water and a
    /// street reaching the shore ends there instead of running on across
    /// the lake bed. Off by default so a network saved before the field
    /// existed traces exactly as it always did - turning it on changes the
    /// whole trace, not only the drowned streets, and would stand that
    /// network's saved lot buildings on a new layout. The editor's new
    /// networks switch it on.
    pub avoid_water: bool,
    /// The street field (#1556): what shapes the streets beyond the land
    /// itself - a smoothing scale, designer fields (ring roads round a
    /// point, a grid turned to a compass bearing) and discs no street
    /// enters and no lot inside grows a building. Untouched (the default, and
    /// every network saved before it) it stays off the wire and the
    /// streets trace exactly as they always did; any edit re-traces the
    /// whole district and regrows its lots.
    pub field: RoadField,
    /// Optional per-surface look overrides (#891). Every `None` falls back
    /// to the room theme's road palette, so an untouched network keeps its
    /// theme identity. Appearance edits re-tint the live materials without
    /// re-extruding the network.
    pub appearance: RoadAppearance,
    /// Building-layer authoring knobs (#892), consumed by the lot
    /// population system alongside `populate_lots`.
    pub lots: LotSettings,
    /// Street-furniture layer (#893): theme props (lamps, signs, clutter)
    /// planted at intervals along the streets, just outside the curbs.
    pub furniture: FurnitureSettings,
    /// Spacing (m) between parallel major / minor roads.
    pub major_spacing: Fp,
    pub minor_spacing: Fp,
    /// Drivable-deck half-widths (m) by road class.
    pub major_half_width: Fp,
    pub minor_half_width: Fp,
    /// Curb lip height (m), curb-top flat width (m), and outward chamfer run (m).
    pub curb_height: Fp,
    pub curb_top_width: Fp,
    pub chamfer_width: Fp,
    /// Depth (m) the foundation skirt drops below the deck.
    pub skirt_depth: Fp,
    /// Whether the room grows buildings on the network's enclosed lots. When
    /// set, the terrain plugin's load-time populate-lots system derives
    /// footprints from this network and injects themed catalogue buildings onto
    /// them (see [`crate::terrain`] / [`crate::urban::extract_building_lots`]).
    /// Defaults on; older records without the field deserialise to `true`.
    #[serde(default = "default_populate_lots")]
    pub populate_lots: bool,
    /// The street plan's layout revision (#1558): which version of the
    /// graph tidy and the lot clearance the network's streets and lots are
    /// derived with. 0 (the default, and every network saved before the
    /// field) is the pipeline every saved district was grown by, byte for
    /// byte. 1 tidies the traced street graph - junctions a few metres
    /// apart merged into one, streets running off the drawn district cut at
    /// its edge, doubled streets running side by side, tiny loops and short
    /// stub streets removed - and keeps every lot clear of every street's
    /// curb by a sidewalk's margin, growing no building larger than its
    /// lot. 2 derives all of that with portable maths (#1563): every
    /// `sin`, `cos`, `tan`, `acos`, `atan2` and `hypot` the streets, lots
    /// and street props depend on comes from the `libm` crate rather than
    /// the platform's own, which differ in the last bit between a native
    /// client and the web one - so every client, whatever it runs on, grows
    /// the same district. Changing it re-traces the district and regrows its buildings,
    /// so a later fix to either derivation bumps the revision rather than
    /// adding a switch. The editor's new networks take
    /// [`Self::LATEST_LAYOUT`]; the sanitiser reads a revision from a newer
    /// client as the latest this build knows. How a network is MESHED is
    /// not part of it: the mesh is rebuilt by every client and never saved.
    pub layout_revision: u32,
}

impl RoadConfig {
    /// The newest layout revision this build derives (#1558, #1563) - see
    /// [`Self::layout_revision`].
    pub const LATEST_LAYOUT: u32 = 2;

    /// Whether the network's streets and lots are derived with the tidied
    /// graph and street-clear lots of layout revision 1 or later (#1558).
    pub fn tidies_layout(&self) -> bool {
        self.layout_revision >= 1
    }

    /// Whether the network is derived with portable maths, the same on
    /// every client: layout revision 2 or later (#1563).
    pub fn portable_math(&self) -> bool {
        self.layout_revision >= 2
    }

    /// The [`symbios_tensor::MathMode`] the network's layout is derived
    /// with - see [`Self::portable_math`].
    pub fn math_mode(&self) -> symbios_tensor::MathMode {
        if self.portable_math() {
            symbios_tensor::MathMode::Portable
        } else {
            symbios_tensor::MathMode::Platform
        }
    }
}

/// Serde default for [`RoadConfig::populate_lots`] - a road network in a record
/// predating the field still grows lot buildings.
fn default_populate_lots() -> bool {
    true
}

/// Street-plan character for a [`RoadConfig`] (#890) - maps onto the tensor
/// field's grid-vs-terrain blend at trace time (see
/// `crate::urban::graph::tensor_config`). Open union so future styles
/// degrade gracefully on older clients: `Unknown` traces as [`Self::Hillside`],
/// the historical terrain-adaptive behavior a record without the field also
/// gets.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(tag = "$type")]
pub enum RoadStyle {
    /// Terrain-adaptive blend (the historical default): a Manhattan grid on
    /// near-flat ground, contour-following streets on slopes.
    #[default]
    #[serde(rename = "network.symbios.road_style.hillside")]
    Hillside,
    /// Axis-aligned Manhattan grid everywhere - terrain is ignored for street
    /// *direction* (decks still drape over its height).
    #[serde(rename = "network.symbios.road_style.grid")]
    Grid,
    /// Contour-following everywhere with gentle directional jitter - streets
    /// wander with the land and never settle into a grid, even on flats.
    #[serde(rename = "network.symbios.road_style.organic")]
    Organic,
    #[serde(other, skip_serializing)]
    Unknown,
}

/// Per-surface look overrides for a road network (#891). Field-level
/// `Option`s: `None` = the room theme's [`road palette`] value for that
/// surface, `Some` = the author's override - so one surface can be re-tinted
/// while the rest keep the theme identity. All-`None` (the default) is
/// elided from the wire entirely.
///
/// [`road palette`]: crate::terrain
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(default)]
pub struct RoadAppearance {
    /// Drivable-deck base colour (linear-ish sRGB triplet, 0–1).
    pub deck_color: Option<Fp3>,
    /// Deck roughness (0 = mirror-wet, 1 = matte).
    pub deck_roughness: Option<Fp>,
    /// Curb / skirt / bottom (structure) base colour.
    pub structure_color: Option<Fp3>,
    /// Curb edge-line colour (before the strength multiplier).
    pub neon_color: Option<Fp3>,
    /// Edge-line emissive strength (~6 = hot neon tube, ~1 = painted line,
    /// 0 = off).
    pub neon_strength: Option<Fp>,
}

impl RoadAppearance {
    /// Whether every field defers to the theme palette.
    pub fn is_all_theme(&self) -> bool {
        *self == Self::default()
    }
}

/// Building-layer authoring knobs for a road network (#892). Field-level
/// serde defaults keep pre-#892 records at the historical behavior; the
/// whole struct is elided from the wire while untouched.
///
/// The struct derives `Serialize`, so a touched one writes every field -
/// except the two socio overrides (#1555), which stay off the wire while
/// `None`, so a record saved before they existed writes back
/// byte-identical.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct LotSettings {
    /// Fraction (0–1) of the extracted lots that grow a building. Thinning
    /// keeps the LARGEST lots (rank order), so low density reads as a
    /// sparse town core rather than random gaps.
    pub density: Fp,
    /// Building-catalogue theme label override (matched against
    /// `ThemeArchetype::label()`, case-insensitive). Empty or unrecognised
    /// = the room's own theme.
    pub theme_override: String,
    /// Role emphasis across the ranked lots.
    pub tier_bias: LotTierBias,
    /// Building fit-scale clamp (relative to lot size). Props are held to
    /// at most 1.0 inside it (#1553) - see `terrain::lots`. Read only when
    /// [`Self::fit`] is set: the world compile draws an absolute placement
    /// at its generator's size (#1454), so without the fit baked into the
    /// generator a building is drawn at its catalogue size whatever these
    /// say.
    pub scale_min: Fp,
    pub scale_max: Fp,
    /// Draw each building at its lot's size (#1553): the lot's fit, rounded
    /// down to a quarter-octave step inside the clamp, baked into a shared
    /// generator per catalogue entry and step. Off (the default, and every
    /// network saved before the field) grows exactly what it always did: one
    /// generator per entry at its catalogue size, so a saved district that
    /// is grown again - after a portal, say - comes back byte for byte. The
    /// editor's new networks switch it on.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub fit: bool,
    /// The largest lot a block is subdivided into, in square metres (#1555;
    /// symbios-tensor's `LotConfig::max_lot_area`): blocks split across
    /// their longest side until each piece is under it. 400 (the default,
    /// and every network saved before the field) gives house-sized lots of
    /// 6-15 m across; a downtown of towers wants a few thousand.
    #[serde(skip_serializing_if = "is_default_lot_area")]
    pub lot_area: Fp,
    /// The district's core, in room metres (XZ), when it has one (#1555):
    /// lots rank by their distance to it, nearest first, so the landmarks
    /// stand at the core and [`Self::density`] thinning keeps the lots
    /// round it - a downtown. `None` (the default, and every network saved
    /// before the field) ranks them by size, biggest first, as before.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<Fp2>,
    /// The prosperity (0 poor to 1 rich) the lot and street-furniture
    /// layers grow with (#1555), in place of the room's own seeded scene
    /// value: it picks the catalogue pools by tier and drives the material
    /// finish. `None` is the room's own scene.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prosperity: Option<Fp>,
    /// The escalation (0 peaceful to 1 open conflict) the lot and
    /// street-furniture layers grow with (#1555), in place of the room's own
    /// seeded scene value: it picks the catalogue pools by tier (barricades
    /// and wreckage from two thirds up), drives the scorch finish and the
    /// ruin that leans and collapses the buildings. `None` is the room's own
    /// scene.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escalation: Option<Fp>,
}

impl Default for LotSettings {
    fn default() -> Self {
        Self {
            density: Fp(1.0),
            theme_override: String::new(),
            tier_bias: LotTierBias::Balanced,
            scale_min: Fp(0.5),
            scale_max: Fp(2.0),
            fit: false,
            lot_area: Fp(LotSettings::DEFAULT_LOT_AREA),
            focus: None,
            prosperity: None,
            escalation: None,
        }
    }
}

/// Whether `area` is [`LotSettings::DEFAULT_LOT_AREA`], kept off the wire.
fn is_default_lot_area(area: &Fp) -> bool {
    area.0 == LotSettings::DEFAULT_LOT_AREA
}

impl LotSettings {
    /// The largest lot area a network grows by default, in square metres:
    /// symbios-tensor's own `LotConfig` default, which every network was
    /// subdivided by before [`Self::lot_area`] existed.
    pub const DEFAULT_LOT_AREA: f32 = 400.0;

    /// What a non-finite prosperity override reads as (#1555): the material
    /// finish's neutral midpoint, a Modest tier. The sanitiser writes the
    /// same value, so the two never answer differently.
    pub const NEUTRAL_PROSPERITY: f32 = 0.5;
    /// What a non-finite escalation override reads as (#1555): peace - no
    /// conflict props, no scorch, no ruin.
    pub const NEUTRAL_ESCALATION: f32 = 0.0;

    /// The prosperity the lot layer grows with (#1555): the authored
    /// override, clamped to the unit range, or `scene` - the room's own
    /// seeded value - when there is none.
    pub fn prosperity_or(&self, scene: f32) -> f32 {
        self.prosperity
            .map_or(scene, |p| unit_or(p.0, Self::NEUTRAL_PROSPERITY))
    }

    /// The escalation the lot layer grows with (#1555): the authored
    /// override, clamped to the unit range, or `scene` - the room's own
    /// seeded value - when there is none.
    pub fn escalation_or(&self, scene: f32) -> f32 {
        self.escalation
            .map_or(scene, |e| unit_or(e.0, Self::NEUTRAL_ESCALATION))
    }
}

/// `v` clamped into `[0, 1]`, or `neutral` when it is not finite.
fn unit_or(v: f32, neutral: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        neutral
    }
}

/// Street-furniture layer settings (#893). Opt-in (`enabled` defaults
/// false so pre-#893 records - and fresh networks - stay uncluttered);
/// the whole struct is elided from the wire while untouched.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct FurnitureSettings {
    /// Whether the layer plants anything at all.
    pub enabled: bool,
    /// Arc-length interval (m) between props along each street, sides
    /// alternating.
    pub spacing: Fp,
}

impl Default for FurnitureSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            spacing: Fp(30.0),
        }
    }
}

/// The street field of a [`RoadConfig`] (#1556): what the tensor field its
/// streets are traced along is made of beyond the land's own contour and
/// fall lines. It maps onto symbios-tensor's `TensorFieldConfig`
/// (`smoothing`, `terrain_weight`, `basis`) and `TensorConfig::keep_out` at
/// trace time (see `crate::urban::graph::tensor_config`), every
/// centre moved from the room frame into the district window the trace runs
/// in and every radius kept.
///
/// Default-eliding wire format (#695): fields matching
/// [`RoadField::default`] are omitted on write, and an untouched field is
/// left off its network altogether, so a network saved before it existed
/// writes back byte-identical and traces the same streets.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct RoadField {
    /// Scale (m) below which the land's relief does not steer the streets:
    /// the field's directions come from a copy of the heightmap blurred
    /// over about this radius, so a street follows a hillside's broad form
    /// rather than turning at every hummock on it. Street heights, the
    /// water line and the lots still read the land itself. 0 (the default)
    /// reads the land unblurred.
    pub smoothing: Fp,
    /// Weight of the land's own field against the [`Self::basis`] fields
    /// where they reach: 1 (the default) is an equal partner of a field of
    /// strength 1, and 0 lets the basis fields alone decide inside their
    /// reach. Outside every basis field the land decides alone, so with no
    /// basis field this changes nothing.
    pub terrain_weight: Fp,
    /// Designer fields laid over the land's and summed with it, at most
    /// [`Self::MAX_BASIS`]. Empty (the default) is the land's field alone.
    pub basis: Vec<RoadBasis>,
    /// Discs no street enters and no lot centred inside grows a building,
    /// at most [`Self::MAX_KEEP_OUT`]. Empty (the default) keeps nothing
    /// out.
    pub keep_out: Vec<RoadKeepOut>,
}

impl Default for RoadField {
    fn default() -> Self {
        Self {
            smoothing: Fp(0.0),
            terrain_weight: Fp(1.0),
            basis: Vec::new(),
            keep_out: Vec::new(),
        }
    }
}

// Default-eliding wire format (#695); the container `#[serde(default)]`
// above is the matching read-side contract.
crate::pds::serde_util::impl_default_eliding_serialize!(RoadField {
    smoothing,
    terrain_weight,
    basis,
    keep_out,
});

impl RoadField {
    /// The most basis fields a network keeps; the sanitiser drops the rest.
    pub const MAX_BASIS: usize = 8;
    /// The most keep-out discs a network keeps; the sanitiser drops the
    /// rest.
    pub const MAX_KEEP_OUT: usize = 16;
    /// The smoothing scale's bounds (m): the sanitiser's clamp, and the
    /// editor's range.
    pub const SMOOTHING_M: std::ops::RangeInclusive<f32> = 0.0..=100.0;
    /// The terrain weight's bounds.
    pub const TERRAIN_WEIGHT: std::ops::RangeInclusive<f32> = 0.0..=10.0;
    /// A basis field's reach (m).
    pub const BASIS_RADIUS_M: std::ops::RangeInclusive<f32> = 5.0..=1024.0;
    /// A basis field's strength.
    pub const BASIS_STRENGTH: std::ops::RangeInclusive<f32> = 0.0..=10.0;
    /// A keep-out disc's radius (m).
    pub const KEEP_OUT_RADIUS_M: std::ops::RangeInclusive<f32> = 2.0..=512.0;
    /// How far from the room origin (m), on either axis, a basis field's or
    /// a keep-out disc's centre may stand.
    pub const CENTER_LIMIT_M: f32 = 1024.0;
}

/// One designer field of a [`RoadField`] (#1556), laid over the land's own
/// tensor field and summed with it as a tensor (after Chen et al. 2008,
/// "Interactive Procedural Street Modeling"). Each reaches `radius` metres
/// from its `center`, its pull falling smoothly from `strength` there to
/// nothing at the edge (`strength * (1 - (d/r)^2)^2`); beyond it the land
/// decides alone. Centres are room metres (X, Z), the frame placements and
/// the district centre use.
///
/// Open union: a kind from a newer client reads as [`Self::Unknown`], which
/// the trace ignores, so an older client still traces the rest of the
/// field. Like every `Unknown`, it cannot be written back (#1111): content
/// this build cannot read is content it must not overwrite.
///
/// Its members read leniently, as [`RoadKeepOut`]'s do: one missing from
/// the wire takes the value a new field starts with (the room origin,
/// bearing 0, [`Self::DEFAULT_RADIUS`], [`Self::DEFAULT_STRENGTH`]), so a
/// writer that leaves a default off never makes the whole terrain
/// unreadable. This build writes every member.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "$type")]
pub enum RoadBasis {
    /// Ring roads: major streets ring `center` and minor streets run
    /// straight out from it - the field a lone round hill there would give,
    /// without the hill (symbios-tensor's `BasisField::Radial`).
    #[serde(rename = "network.symbios.road_basis.ring")]
    Ring {
        /// The ring's centre, room metres (X, Z).
        #[serde(default)]
        center: Fp2,
        /// How far the field reaches (m).
        #[serde(default = "RoadBasis::default_radius")]
        radius: Fp,
        /// Its pull at the centre: 1 pulls as hard as the land does at
        /// terrain weight 1.
        #[serde(default = "RoadBasis::default_strength")]
        strength: Fp,
    },
    /// A straight grid turned to a compass bearing: major streets run along
    /// `bearing` and minor streets square to them (symbios-tensor's
    /// `BasisField::Grid`).
    #[serde(rename = "network.symbios.road_basis.grid")]
    Grid {
        /// The grid field's centre, room metres (X, Z).
        #[serde(default)]
        center: Fp2,
        /// The compass bearing (degrees) the major streets run along:
        /// clockwise from north, where north is -Z and east +X, so 0 runs
        /// them north-south and 90 east-west. A grid repeats every half
        /// turn, so the sanitiser folds it into `[0, 180)` (see
        /// [`RoadBasis::canonical_bearing`]).
        #[serde(default)]
        bearing: Fp,
        /// How far the field reaches (m).
        #[serde(default = "RoadBasis::default_radius")]
        radius: Fp,
        /// Its pull at the centre: 1 pulls as hard as the land does at
        /// terrain weight 1.
        #[serde(default = "RoadBasis::default_strength")]
        strength: Fp,
    },
    /// A kind from a newer client: kept as it was read, ignored by the
    /// trace.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl Default for RoadBasis {
    /// Ring roads round the room origin at the editor's starting reach and
    /// strength.
    fn default() -> Self {
        Self::ring_at(Fp2([0.0, 0.0]))
    }
}

impl RoadBasis {
    /// The reach (m) a field added in the editor starts with.
    pub const DEFAULT_RADIUS: f32 = 120.0;
    /// The strength a field added in the editor starts with: as strong as
    /// the land at its centre.
    pub const DEFAULT_STRENGTH: f32 = 1.0;

    /// The reach a ring or grid read without one takes.
    fn default_radius() -> Fp {
        Fp(Self::DEFAULT_RADIUS)
    }

    /// The strength a ring or grid read without one takes.
    fn default_strength() -> Fp {
        Fp(Self::DEFAULT_STRENGTH)
    }

    /// Ring roads round `center` at the starting reach and strength - what
    /// the editor's `+ Ring` adds.
    pub fn ring_at(center: Fp2) -> Self {
        Self::Ring {
            center,
            radius: Fp(Self::DEFAULT_RADIUS),
            strength: Fp(Self::DEFAULT_STRENGTH),
        }
    }

    /// A grid at `center` with its major streets running north-south
    /// (bearing 0), at the starting reach and strength - what the editor's
    /// `+ Grid` adds.
    pub fn grid_at(center: Fp2) -> Self {
        Self::Grid {
            center,
            bearing: Fp(0.0),
            radius: Fp(Self::DEFAULT_RADIUS),
            strength: Fp(Self::DEFAULT_STRENGTH),
        }
    }

    /// `bearing` (degrees) folded into `[0, 180)`, the one value a grid's
    /// bearing has: a grid at 180 is the grid at 0. A non-finite bearing
    /// reads as 0. The sanitiser writes it and the trace reads every bearing
    /// through it, so an owner's 180 and a guest's 0 trace the same streets
    /// to the bit.
    pub fn canonical_bearing(bearing: f32) -> f32 {
        if !bearing.is_finite() {
            return 0.0;
        }
        let folded = bearing.rem_euclid(180.0);
        // `rem_euclid` rounds a hair below zero up to the modulus itself.
        if folded >= 180.0 { 0.0 } else { folded }
    }
}

/// A disc no street of its network enters and no lot building grows in
/// (#1556): a plaza, a park, a landmark's ground. The tracer ends a street
/// at its rim as it does at the shore - though a street can graze the rim
/// by a few metres where it snaps onto a junction beside it - and a lot
/// whose centre lies inside grows nothing, since a block of streets round
/// the disc still encloses it. Room metres (X, Z), like [`RoadBasis`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct RoadKeepOut {
    /// The disc's centre, room metres (X, Z).
    pub center: Fp2,
    /// Its radius (m).
    pub radius: Fp,
}

impl Default for RoadKeepOut {
    fn default() -> Self {
        Self::at(Fp2([0.0, 0.0]))
    }
}

impl RoadKeepOut {
    /// The radius (m) a disc added in the editor starts with.
    pub const DEFAULT_RADIUS: f32 = 30.0;

    /// A disc of the starting radius at `center` - what the editor's
    /// `+ Keep-out` adds.
    pub fn at(center: Fp2) -> Self {
        Self {
            center,
            radius: Fp(Self::DEFAULT_RADIUS),
        }
    }

    /// Whether the room point `p` (X, Z) lies inside the disc. Its rim is
    /// outside, as symbios-tensor's `KeepOut::contains` counts it.
    pub fn contains(&self, p: [f32; 2]) -> bool {
        let (dx, dz) = (p[0] - self.center.0[0], p[1] - self.center.0[1]);
        dx * dx + dz * dz < self.radius.0 * self.radius.0
    }
}

/// Role emphasis for lot buildings (#892). Open union: `Unknown` populates
/// as `Balanced`, the historical mix.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(tag = "$type")]
pub enum LotTierBias {
    /// Historical mix: the biggest lot takes the landmark, the next ~20%
    /// secondary buildings, the tail props.
    #[default]
    #[serde(rename = "network.symbios.lot_bias.balanced")]
    Balanced,
    /// Monument-heavy: the top ~10% of lots take landmarks, the next ~30%
    /// secondary.
    #[serde(rename = "network.symbios.lot_bias.monumental")]
    Monumental,
    /// No landmarks: dwellings (secondary) on the bigger lots, props on
    /// the rest.
    #[serde(rename = "network.symbios.lot_bias.residential")]
    Residential,
    /// Props only - street clutter without buildings.
    #[serde(rename = "network.symbios.lot_bias.props_only")]
    PropsOnly,
    /// A built-up city (#1555): a building on every lot - the top ~15% of
    /// the lots (at least one) take landmarks, the rest secondary
    /// buildings. Props grow only where the theme offers no secondary at
    /// its tier: then the tail takes props rather than one landmark on
    /// every lot. A client that predates it reads it as `Unknown`, so as
    /// `Balanced`.
    #[serde(rename = "network.symbios.lot_bias.downtown")]
    Downtown,
    #[serde(other, skip_serializing)]
    Unknown,
}

impl LotTierBias {
    /// Picker rows for the editor: `(value, label, tooltip)`.
    pub fn pickers() -> [(Self, &'static str, &'static str); 5] {
        [
            (
                Self::Balanced,
                "Balanced",
                "One landmark on the biggest lot, some secondaries, mostly props",
            ),
            (
                Self::Monumental,
                "Monumental",
                "Landmark-heavy: grand buildings dominate the district",
            ),
            (
                Self::Residential,
                "Residential",
                "No landmarks - dwellings and props only",
            ),
            (
                Self::PropsOnly,
                "Props only",
                "Street clutter without buildings",
            ),
            (
                Self::Downtown,
                "Downtown",
                "A city: a building on every lot - landmarks on the top 15%, \
                 secondaries on the rest",
            ),
        ]
    }
}

impl RoadStyle {
    /// Picker rows for the editor: `(value, label, tooltip)`.
    pub fn pickers() -> [(Self, &'static str, &'static str); 3] {
        [
            (
                Self::Hillside,
                "Hillside",
                "Grid on flats, contour-following on slopes - the adaptive default",
            ),
            (
                Self::Grid,
                "Grid",
                "Axis-aligned Manhattan grid everywhere, whatever the terrain",
            ),
            (
                Self::Organic,
                "Organic",
                "Streets follow the land everywhere, with a gentle wander",
            ),
        ]
    }
}

crate::pds::serde_util::impl_default_eliding_serialize!(RoadConfig {
    enabled,
    seed via u64_as_string(u64),
    district_half_extent,
    center,
    style,
    avoid_water,
    field,
    appearance,
    lots,
    furniture,
    major_spacing,
    minor_spacing,
    major_half_width,
    minor_half_width,
    curb_height,
    curb_top_width,
    chamfer_width,
    skirt_depth,
    populate_lots,
    layout_revision,
});

impl Default for RoadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            seed: 0,
            district_half_extent: Fp(170.0),
            center: Fp2([0.0, 0.0]),
            style: RoadStyle::Hillside,
            avoid_water: false,
            field: RoadField::default(),
            appearance: RoadAppearance::default(),
            lots: LotSettings::default(),
            furniture: FurnitureSettings::default(),
            major_spacing: Fp(95.0),
            minor_spacing: Fp(55.0),
            major_half_width: Fp(3.5),
            minor_half_width: Fp(2.0),
            curb_height: Fp(0.18),
            curb_top_width: Fp(0.22),
            chamfer_width: Fp(0.4),
            skirt_depth: Fp(5.0),
            populate_lots: true,
            layout_revision: 0,
        }
    }
}

/// Vertex-torture parameters shared by every parametric primitive. Bundled
/// into one struct (rather than three flat fields on all eight variants) so a
/// new torture knob is a single field add - `#[serde(default)]` fills it on
/// records that predate it - instead of an edit to every variant and every
/// construction site. Applied CPU-side in `world_builder::prim`.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct TortureParams {
    /// Radians of rotation around Y, linear in normalised height.
    pub twist: Fp,
    /// Per-axis taper: X and Z each scale by `1 - taper[axis] * t` toward the
    /// top. Equal components give a uniform taper (a cone / frustum); unequal
    /// ones give a wedge / fin.
    pub taper: Fp2,
    /// Per-axis **bottom** taper: X and Z each scale by
    /// `1 - taper_bottom[axis] * (1 - t)` toward the base, composing with
    /// `taper` so one prim can narrow at both ends (a lens / spearhead) -
    /// without the old author-it-upside-down-and-flip-π workaround that
    /// top-only taper forced on every downward-narrowing form.
    pub taper_bottom: Fp2,
    /// Quadratic top displacement `(x, y, z) * t²` - a single arc that pins
    /// the base and swings the top.
    pub bend: Fp3,
    /// Serpentine S-curve: a `sin(2π t)` lateral wave of amplitude `(x, z)`
    /// layered on top of `bend`, so a column can snake rather than only arc.
    pub s_bend: Fp2,
    /// Top-shear: a *linear* lateral displacement `(x, z) * t` that slides the
    /// top sideways relative to the pinned base (a parallelepiped / leaning
    /// tower / slanted roof). Unlike `bend` (quadratic, tangent at the base)
    /// the offset grows uniformly, so vertical edges stay straight but tilted.
    pub shear: Fp2,
    /// Per-axis mid-profile bulge (+) / pinch (−): X and Z scale gain
    /// `bulge[axis] * sin(π t)` - zero at both ends, peaking at mid-height.
    /// One positive slider turns a straight capsule into a muscle / belly /
    /// tree-trunk swell; a negative one gives a waist / hourglass. The
    /// combined per-axis scale is floored just above zero in the deform pass
    /// so a hard pinch collapses to the axis instead of inverting the surface.
    pub bulge: Fp2,
    // --- Topology cuts (SL-style; honoured during mesh *generation*, not
    // the vertex post-pass). As of #725 every prim honours them except
    // Plane (no revolve axis). Semantics per family: revolved prims cut
    // angularly / by band / by bore; box prims (Cuboid / Bevel) take a pie
    // wedge / vertical slice / matching bore; tubes (Helix / Spine) open
    // into channels / trim their path / become shells; Lathe trims the
    // kept arc-length band of its silhouette; BlobGroup applies them as
    // hard CSG on its distance field (Y-slab slice / pie wedge / inner
    // shell). Default = identity (full sweep, full profile, solid). ---
    /// Kept angular fraction of the main sweep, `[begin, end]` in turns (0..1).
    /// `[0, 1]` = full revolution (no cut); `[0, 0.5]` keeps a half (half-
    /// cylinder trough, half-dome, half-torus archway). The opening gains two
    /// radial cap faces.
    pub path_cut: Fp2,
    /// Kept fraction of the cross-section / latitude, `[begin, end]` in 0..1.
    /// On a revolved profile (Sphere) this is the latitude band: `[0, 1]` full,
    /// `[0.5, 1]` a top dome, `[0, 0.5]` a bowl. On a Torus it opens the tube
    /// into a C-channel. Adds cap faces at the cut.
    pub profile_cut: Fp2,
    /// Bore as a fraction of the outer radius, `0..0.95`. `0` = solid; `> 0`
    /// hollows the prim (pipe / funnel / ring / shell) with an inner wall and
    /// annular rim caps - the general form of [`GeneratorKind::Tube`].
    pub hollow: Fp,
}

impl Default for TortureParams {
    fn default() -> Self {
        Self {
            twist: Fp(0.0),
            taper: Fp2([0.0, 0.0]),
            taper_bottom: Fp2([0.0, 0.0]),
            bend: Fp3([0.0, 0.0, 0.0]),
            s_bend: Fp2([0.0, 0.0]),
            shear: Fp2([0.0, 0.0]),
            bulge: Fp2([0.0, 0.0]),
            path_cut: Fp2([0.0, 1.0]),
            profile_cut: Fp2([0.0, 1.0]),
            hollow: Fp(0.0),
        }
    }
}

// Default-eliding wire format (#695): an identity TortureParams - the
// overwhelmingly common case across catalogue prims - serializes as `{}`,
// and any prim whose torture IS identity omits the field entirely via
// `skip_serializing_if` at the variant field. The container
// `#[serde(default)]` above is the matching read-side contract.
crate::pds::serde_util::impl_default_eliding_serialize!(TortureParams {
    twist,
    taper,
    taper_bottom,
    bend,
    s_bend,
    shear,
    bulge,
    path_cut,
    profile_cut,
    hollow,
});

impl TortureParams {
    /// `true` when the whole struct equals its default - the wire-format
    /// skip predicate for prim `torture` fields (#695).
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// `true` when no vertex deform is active (twist / taper / bulge / bend /
    /// S-bend / shear all zero). Meshers use this to skip the vertical
    /// subdivisions that only exist to give the deform pass mid-height
    /// vertices to move - a 2-ring wall renders a `sin(π t)` bulge as
    /// nothing at all.
    pub fn deforms_are_identity(&self) -> bool {
        let flat2 = |v: &Fp2| v.0[0].abs() < 1e-6 && v.0[1].abs() < 1e-6;
        self.twist.0.abs() < 1e-6
            && flat2(&self.taper)
            && flat2(&self.taper_bottom)
            && flat2(&self.bulge)
            && self.bend.0.iter().all(|c| c.abs() < 1e-6)
            && flat2(&self.s_bend)
            && flat2(&self.shear)
    }

    /// `true` when no topology cut is active (full sweep, full profile, solid),
    /// so the mesher can take the cheap closed-surface path.
    pub fn cuts_are_identity(&self) -> bool {
        self.path_cut.0[0] <= 1e-4
            && self.path_cut.0[1] >= 1.0 - 1e-4
            && self.profile_cut.0[0] <= 1e-4
            && self.profile_cut.0[1] >= 1.0 - 1e-4
            && self.hollow.0 <= 1e-4
    }
}

/// Primitive shape of one [`BlobElement`]. Open union so future shapes
/// degrade gracefully on older clients - an `Unknown` element evaluates as
/// a sphere rather than failing the record.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(tag = "$type")]
pub enum BlobShape {
    #[serde(rename = "network.symbios.blob.sphere")]
    #[default]
    Sphere,
    /// Capsule along the element's local +Y axis.
    #[serde(rename = "network.symbios.blob.capsule")]
    Capsule,
    #[serde(rename = "network.symbios.blob.ellipsoid")]
    Ellipsoid,
    /// Axis-aligned box (pre-rotation) - flat faces and hard masses inside
    /// smooth blends: pedestals, slabs, jaws.
    #[serde(rename = "network.symbios.blob.box")]
    Box,
    /// Capped cylinder along the element's local +Y axis.
    #[serde(rename = "network.symbios.blob.cylinder")]
    Cylinder,
    /// Torus lying in the element's local XZ plane (axis +Y).
    #[serde(rename = "network.symbios.blob.torus")]
    Torus,
    /// Capped cone: base radius `radii[0]` at local −Y, tip radius
    /// `radii[2]` at +Y (the sanitiser's 0.01 floor ≈ a point, so plain
    /// cones need no extra field; a real tip radius makes the
    /// truncated-cone limb segment).
    #[serde(rename = "network.symbios.blob.cone")]
    Cone,
    #[serde(other, skip_serializing)]
    Unknown,
}

/// UV projection a [`GeneratorKind::BlobGroup`] bakes into its mesh (#739).
/// Surface nets has no analytic parameterisation, so texture coordinates
/// come from projecting each vertex - and which projection reads well is
/// shape-dependent, so it's an authorable knob rather than a constant.
/// Open union so future modes degrade gracefully on older clients - an
/// `Unknown` mode meshes as the default.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(tag = "$type")]
pub enum UvMapping {
    /// Equirectangular projection of each vertex's direction from the
    /// surface centroid - the original #739 mapping, and the wire default
    /// until #742. Reads well on roundish masses; elongated or multi-lobed
    /// groups stretch (direction ignores distance) and concave regions
    /// repeat the texture where two surface points share a direction.
    #[serde(rename = "network.symbios.uv.spherical")]
    Spherical,
    /// Baked tri-planar box projection: each triangle projects along the
    /// axis its normal leans into most, at one uniform scale, so texel
    /// density is even everywhere. The all-round distortion fix and the
    /// wire default since #742 (a field-less record renders Box - chosen
    /// over Spherical because it reads better on almost every real group,
    /// humanoid avatar masses especially); strongly patterned textures
    /// show seams where the projection axis changes.
    #[serde(rename = "network.symbios.uv.box")]
    #[default]
    Box,
    /// Wrap around the prim-local Y axis (the same reference axis the
    /// topology cuts use): U is azimuth, V climbs with height scaled so a
    /// texel stays square against the group's mean circumference (the
    /// swept prims' convention). Suits limbs, trunks and columns; surface
    /// facing straight up or down swirls.
    #[serde(rename = "network.symbios.uv.cylindrical")]
    Cylindrical,
    /// Flat projection along local X (texture lies on the YZ plane).
    #[serde(rename = "network.symbios.uv.planar_x")]
    PlanarX,
    /// Flat projection along local Y - top-down, for slab-like masses.
    #[serde(rename = "network.symbios.uv.planar_y")]
    PlanarY,
    /// Flat projection along local Z (texture lies on the XY plane).
    #[serde(rename = "network.symbios.uv.planar_z")]
    PlanarZ,
    /// Keep the mesher's own parameterisation, normalised so the texture
    /// spans the surface exactly once - the pre-#933 behaviour, now an
    /// explicit choice rather than the only option.
    ///
    /// This is what an **alpha card** needs. The `Window`, foliage and
    /// sprite generators upload clamp-to-edge rather than repeating, so a
    /// card must cover its quad once and once only; under the metre
    /// convention it would instead tile, and past `1.0` the sampler would
    /// smear its edge texels across the remainder. It is also the right
    /// pick for anything whose texture is a single picture rather than a
    /// material - a sign face, a painted panel.
    #[serde(rename = "network.symbios.uv.fit")]
    Fit,
    #[serde(other, skip_serializing)]
    Unknown,
}

impl UvMapping {
    /// Wire-format skip predicate: the default mode stays off the wire
    /// (#695 elision discipline). A field-less record therefore tracks
    /// whatever the engine's current default is - that's how #742 flipped
    /// every untouched blob to Box without a migration - while an explicit
    /// non-default choice (now including Spherical) serialises its tag.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Constructor for the per-field serde default on card-carrying kinds
    /// (`Plane`), whose sensible default is [`Fit`](Self::Fit) rather than
    /// the enum-wide [`Box`](Self::Box).
    pub fn fit() -> Self {
        Self::Fit
    }

    /// Elision predicate paired with [`fit`](Self::fit): a `Plane` that
    /// wants the card behaviour keeps the field off the wire, and only a
    /// deliberate non-card choice serialises its tag.
    pub fn is_fit(&self) -> bool {
        *self == Self::Fit
    }
}

/// Semantic identity of one face of a parametric primitive (#955) - the
/// key a [`FaceOverride`] addresses. Keys are *semantic*, not positional:
/// they name what a face **is** (the outer wall, the bore, the +X side…),
/// so an override keeps meaning when torture parameters change the face
/// census. An override whose face the current cut/hollow state does not
/// produce is **dormant** - kept in the record, invisible in the mesh -
/// and it reappears when the face does (the SL behaviour).
///
/// Vocabulary per family (the mesher emits exactly these):
///
/// * **Cuboid**: [`SidePx`](Self::SidePx) / [`SideNx`](Self::SideNx) /
///   [`SidePz`](Self::SidePz) / [`SideNz`](Self::SideNz) +
///   [`Top`](Self::Top) / [`Bottom`](Self::Bottom).
/// * **Bevel**: one wrapped [`Wall`](Self::Wall) - its rounded corners
///   bridge the four sides geometrically - plus `Top` / `Bottom`.
/// * **Wedge**: [`Slope`](Self::Slope) / [`Back`](Self::Back) / `Bottom`
///   + the [`Left`](Self::Left) / [`Right`](Self::Right) triangles.
/// * **Tetrahedron**: [`Base`](Self::Base) + [`Front`](Self::Front) /
///   `Left` / `Right`.
/// * **Plane**: a single [`Surface`](Self::Surface).
/// * **Revolved** (Cylinder / Cone / Tube / Lathe / Spine / Helix):
///   `Wall` + `Top` / `Bottom` caps.
/// * **Smooth closed** (Sphere / Capsule / Superellipsoid / BlobGroup):
///   a single `Surface`; a profile-cut opens `Top` / `Bottom` cap discs.
///   BlobGroup stays `Surface`-only - its cuts are carved into the SDF,
///   so cut faces are emergent geometry, not taggable blocks.
/// * **Cuts** add [`Bore`](Self::Bore) (the hollow inner shell),
///   [`PathCutStart`](Self::PathCutStart) / [`PathCutEnd`](Self::PathCutEnd)
///   (the two radial faces closing a pie wedge) and
///   [`ProfileCutStart`](Self::ProfileCutStart) /
///   [`ProfileCutEnd`](Self::ProfileCutEnd) (the two open lips of a
///   torus / helix cross-section band).
///
/// Open union like [`UvMapping`]: a face name minted by a newer client
/// decodes as [`Unknown`](Self::Unknown) and simply stays dormant.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(tag = "$type")]
pub enum FaceKey {
    /// Outer lateral surface of a revolved prim, or the Bevel's wrapped side.
    #[serde(rename = "network.symbios.face.wall")]
    Wall,
    /// Inner shell created by `hollow` (or the Tube's built-in bore).
    #[serde(rename = "network.symbios.face.bore")]
    Bore,
    /// +Y cap.
    #[serde(rename = "network.symbios.face.top")]
    Top,
    /// −Y cap.
    #[serde(rename = "network.symbios.face.bottom")]
    Bottom,
    /// Cuboid +X side.
    #[serde(rename = "network.symbios.face.side_px")]
    SidePx,
    /// Cuboid −X side.
    #[serde(rename = "network.symbios.face.side_nx")]
    SideNx,
    /// Cuboid +Z side.
    #[serde(rename = "network.symbios.face.side_pz")]
    SidePz,
    /// Cuboid −Z side.
    #[serde(rename = "network.symbios.face.side_nz")]
    SideNz,
    /// The Wedge's inclined face.
    #[serde(rename = "network.symbios.face.slope")]
    Slope,
    /// The Tetrahedron's +Z lateral face.
    #[serde(rename = "network.symbios.face.front")]
    Front,
    /// The Wedge's vertical −Z face.
    #[serde(rename = "network.symbios.face.back")]
    Back,
    /// −X lateral face (Wedge / Tetrahedron).
    #[serde(rename = "network.symbios.face.left")]
    Left,
    /// +X lateral face (Wedge / Tetrahedron).
    #[serde(rename = "network.symbios.face.right")]
    Right,
    /// The Tetrahedron's −Y face.
    #[serde(rename = "network.symbios.face.base")]
    Base,
    /// The whole surface of a smooth closed prim (Sphere / Capsule /
    /// Superellipsoid / BlobGroup) or of a Plane.
    #[serde(rename = "network.symbios.face.surface")]
    Surface,
    /// Radial face closing the pie wedge at the path-cut's start angle.
    #[serde(rename = "network.symbios.face.path_cut_start")]
    PathCutStart,
    /// Radial face closing the pie wedge at the path-cut's end angle.
    #[serde(rename = "network.symbios.face.path_cut_end")]
    PathCutEnd,
    /// Open lip at the profile-cut band's start (torus / helix cross-section).
    #[serde(rename = "network.symbios.face.profile_cut_start")]
    ProfileCutStart,
    /// Open lip at the profile-cut band's end (torus / helix cross-section).
    #[serde(rename = "network.symbios.face.profile_cut_end")]
    ProfileCutEnd,
    #[serde(other, skip_serializing)]
    Unknown,
}

impl FaceKey {
    /// Short human-readable name - the `kind_tag` analogue for the face
    /// picker UI and the render tool's dump output.
    pub fn label(&self) -> &'static str {
        match self {
            FaceKey::Wall => "Wall",
            FaceKey::Bore => "Bore",
            FaceKey::Top => "Top",
            FaceKey::Bottom => "Bottom",
            FaceKey::SidePx => "Side +X",
            FaceKey::SideNx => "Side −X",
            FaceKey::SidePz => "Side +Z",
            FaceKey::SideNz => "Side −Z",
            FaceKey::Slope => "Slope",
            FaceKey::Front => "Front",
            FaceKey::Back => "Back",
            FaceKey::Left => "Left",
            FaceKey::Right => "Right",
            FaceKey::Base => "Base",
            FaceKey::Surface => "Surface",
            FaceKey::PathCutStart => "Cut start",
            FaceKey::PathCutEnd => "Cut end",
            FaceKey::ProfileCutStart => "Slice start",
            FaceKey::ProfileCutEnd => "Slice end",
            FaceKey::Unknown => "Unknown",
        }
    }
}

/// One face's appearance override (#955). The override is **complete and
/// independent**: `material` is the face's whole material (per-face
/// `uv_scale` / offset / rotation ride on it), not a delta - editing the
/// prim's base material later does not bleed into overridden faces (the
/// SL model). `uv_mapping` is the one exception: `None` inherits the
/// prim's own projection so a plain recolour never changes the mesh.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FaceOverride {
    /// Which face this override paints. Duplicate keys are dropped by the
    /// sanitizer (first entry wins); unknown keys are kept dormant.
    pub face: FaceKey,
    #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
    pub material: SovereignMaterialSettings,
    /// Projection override for this face; `None` inherits the prim's
    /// `uv_mapping`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uv_mapping: Option<UvMapping>,
}

/// The five fields every parametric primitive carries besides its own
/// dimensional knobs (#1188): whether it is solid, how a texture is
/// projected onto it, its base material, its per-face overrides and its
/// vertex torture. One struct carried by all sixteen variants, so a shared
/// accessor is one arm, an editor takes one argument, and #955's next
/// shared field is a one-line addition rather than a twenty-signature one.
///
/// **The wire form is the flat one it always was.** The block is
/// `#[serde(flatten)]`ed into each variant, so a primitive still serialises
/// as a single object - its own fields, then `solid`, `uv_mapping`,
/// `material`, `faces`, `torture` in that order, the default-valued members
/// elided exactly as before. Child room records are content-addressed over
/// those bytes, and `tests/prim_wire.rs` pins them.
///
/// `uv_mapping` is `None` for the family's own projection - `Box` on the
/// flat family (Cuboid, Tetrahedron, Bevel, Wedge, Superellipsoid,
/// BlobGroup), `Fit` on the revolved one and the Plane - which is what the
/// wire omits and [`GeneratorKind::uv_mapping`] resolves. A `Some` equal to
/// that default is never stored: [`GeneratorKind::set_uv_mapping`] folds
/// it and the sanitiser folds one that arrives on the wire, so the bytes a
/// record re-publishes never gain a key an older client elided.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct PrimCommon {
    /// Whether the spawner attaches the prim's matching collider.
    pub solid: bool,
    /// Texture projection baked onto this prim (#937, #955); `None` is the
    /// family's own default (see the type docs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uv_mapping: Option<UvMapping>,
    #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
    pub material: SovereignMaterialSettings,
    /// Per-face material / projection overrides (#955); empty = the
    /// whole prim wears `material`. See [`FaceOverride`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<FaceOverride>,
    #[serde(default, skip_serializing_if = "TortureParams::is_default")]
    pub torture: TortureParams,
}

impl PrimCommon {
    /// A non-solid block wearing `material` and nothing else - the
    /// catalogue constructors' starting point.
    pub fn with_material(material: SovereignMaterialSettings) -> Self {
        Self {
            material,
            ..Default::default()
        }
    }
}

/// One stamp in a [`GeneratorKind::BlobGroup`]'s ordered edit list - the
/// Dreams model: elements evaluate in list order, each smoothly added to
/// (or carved out of) everything before it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct BlobElement {
    pub shape: BlobShape,
    /// Element centre in the prim's local space.
    pub position: Fp3,
    /// Element orientation (unit quaternion) - orients a capsule's axis or
    /// an ellipsoid's semi-axes; irrelevant for a sphere.
    pub rotation: Fp4,
    /// Per-shape size: Sphere uses `radii[0]`; Ellipsoid and Box read all
    /// three (semi-axes / half-extents); Capsule and Cylinder read
    /// `radii[0]` = radius and `radii[1]` = half-length / half-height;
    /// Cone reads `radii[0]` = base radius, `radii[1]` = half-height and
    /// `radii[2]` = tip radius; Torus reads `radii[0]` = ring (major)
    /// radius and `radii[1]` = tube (minor) radius.
    pub radii: Fp3,
    /// `true` carves this element out of the accumulated shape (smooth
    /// subtraction - eye sockets, nostrils, creases) instead of adding it.
    pub subtract: bool,
    /// Smooth-blend distance (metres): how far from contact this element
    /// starts merging with the accumulated surface. `0` = hard union.
    pub blend: Fp,
}

impl Default for BlobElement {
    fn default() -> Self {
        Self {
            shape: BlobShape::Sphere,
            position: Fp3([0.0, 0.0, 0.0]),
            rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
            radii: Fp3([0.25, 0.25, 0.25]),
            subtract: false,
            blend: Fp(0.1),
        }
    }
}

/// One control point of a [`GeneratorKind::Spine`]: a local-space position
/// the tube's centreline passes through, and the tube radius there. Both are
/// interpolated with the same Catmull-Rom spline, so the radius flows as
/// smoothly along the tube as the path does.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct SpinePoint {
    pub position: Fp3,
    pub radius: Fp,
}

/// One station of a [`GeneratorKind::Lathe`] profile: radial distance from
/// the Y axis at a given local height. Stations are meshed bottom-to-top in
/// list order; a zero radius pinches the surface onto the axis (a pole).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct LathePoint {
    pub radius: Fp,
    pub height: Fp,
}

/// Full parameter set of a [`GeneratorKind::ParticleSystem`] emitter (#648).
///
/// Lives behind a `Box` on the variant so the enum's stack size doesn't
/// carry all ~30 fields (the same shape as `LocomotionConfig`'s boxed
/// `*Params`). Wire compat: an internally-tagged (`$type`) enum serialises
/// a newtype variant's struct fields inline beside the tag - byte-identical
/// to the old struct-variant form, so existing records round-trip
/// unchanged (guarded by the `particle_params_wire_format_*` tests).
///
/// Default-eliding wire format (#695): fields matching
/// [`ParticleParams::default`] are omitted on write; the container
/// `#[serde(default)]` restores them on read.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ParticleParams {
    pub emitter_shape: EmitterShape,

    /// Continuous emit rate in particles per second.
    pub rate_per_second: Fp,
    /// Per-cycle burst count. `0` disables bursts; `>0` emits that
    /// many particles at the start of each loop iteration (or at
    /// emitter activation for non-looping emitters).
    pub burst_count: u32,
    /// Hard cap on simultaneously-alive particles. Exhausting this
    /// budget causes new spawns to be skipped rather than evicting
    /// the oldest particle, which keeps the visual style stable
    /// under load.
    pub max_particles: u32,
    /// `true` re-emits forever; `false` stops emitting after
    /// `duration` seconds (existing particles continue to age out).
    pub looping: bool,
    /// Active-emit duration in seconds. For looping emitters this is
    /// the burst-cadence period.
    pub duration: Fp,

    /// Per-particle lifetime range in seconds. Sampled uniformly
    /// per spawn.
    pub lifetime_min: Fp,
    pub lifetime_max: Fp,
    /// Per-particle initial-speed range in metres / second. Sampled
    /// uniformly per spawn and scales the direction vector
    /// produced by `emitter_shape`.
    pub speed_min: Fp,
    pub speed_max: Fp,

    /// Multiplier on world gravity applied each frame. `1.0` =
    /// terrestrial, `0.0` = floats, `-1.0` = anti-gravity (smoke
    /// rising effect without a custom force).
    pub gravity_multiplier: Fp,
    /// Constant per-particle acceleration in world space (m/s²).
    /// Stacks with `gravity_multiplier * world_gravity`.
    pub acceleration: Fp3,
    /// Exponential linear damping per second. `0.0` = no drag,
    /// higher values brake the particle quadratically over its
    /// lifetime.
    pub linear_drag: Fp,

    /// Quad size at the start and end of the particle's lifetime;
    /// linearly interpolated each frame.
    pub start_size: Fp,
    pub end_size: Fp,
    /// RGBA at the start and end of lifetime; linearly
    /// interpolated each frame.
    pub start_color: Fp4,
    pub end_color: Fp4,
    pub blend_mode: ParticleBlendMode,
    /// `true` orients the quad to always face the camera (classic
    /// billboard); `false` aligns the quad along the velocity
    /// vector (streak / spark look).
    pub billboard: bool,

    pub simulation_space: SimulationSpace,
    /// Fraction of the emitter's world velocity added to each
    /// particle's initial velocity at spawn. `0.0` = ignore
    /// (sparks fly purely along their own emit direction), `1.0` =
    /// match emitter (running-dust effect), `>1.0` = exhaust
    /// (jets ahead). Sanitised to `[0, 2]`.
    pub inherit_velocity: Fp,

    /// Toggle particle collisions against the room's terrain
    /// heightfield. `false` = visual-only (cheaper).
    pub collide_terrain: bool,
    /// Toggle collisions against finite water surfaces.
    pub collide_water: bool,
    /// Toggle collisions against arbitrary avian3d colliders
    /// (placed primitives, walls, …).
    pub collide_colliders: bool,
    /// Restitution applied on collision: `0.0` = stick, `1.0` =
    /// perfect bounce.
    pub bounce: Fp,
    /// Friction applied to the tangential velocity on collision:
    /// `0.0` = frictionless slide, `1.0` = stick.
    pub friction: Fp,

    /// Deterministic emission seed. Same seed + same dt path on
    /// every peer produces the same particle stream.
    #[serde(with = "u64_as_string")]
    pub seed: u64,

    /// Optional per-particle texture. Resolves through the same
    /// [`SignSource`] union Sign uses, so a "leaf falling" emitter
    /// and a Sign signpost pointing at the same atlas image share
    /// one HTTPS round trip via [`super::super::world_builder::image_cache::BlobImageCache`].
    /// `None` keeps v1 behaviour: solid coloured quads tinted by
    /// `start_color` / `end_color`.
    pub texture: Option<SignSource>,
    /// Treat the loaded texture as a sprite-sheet atlas of
    /// `rows × cols` cells. `None` uses the whole image as a single
    /// frame (the default).
    pub texture_atlas: Option<TextureAtlas>,
    /// How a particle picks its current atlas frame. `Still` keeps
    /// frame 0 forever; `RandomFrame` picks once at spawn (per-RNG-
    /// stream draw) so different particles show different sprites
    /// from the same atlas; `OverLifetime { fps }` cycles through
    /// the frame array at the configured rate.
    pub frame_mode: AnimationFrameMode,
    /// Sampler filter applied to the loaded image. `Linear` is the
    /// natural smooth filtering for soft sprites; `Nearest` for
    /// pixel-art / retro looks. The cache keys on filter so a
    /// Linear and a Nearest request for the same source produce
    /// two distinct GPU images, neither stomping the other.
    pub texture_filter: TextureFilter,
    /// Procedurally-baked particle sprite, generated locally instead of
    /// fetched. When this is set (non-`None`) and `texture` is `None`,
    /// the emitter bakes this generator at
    /// [`crate::config::textures::PARTICLE_CELL`] per atlas cell and
    /// uses the result as the particle albedo. The sprite generators
    /// (SoftDisc, Snowflake, Flame, …) carry `variant_rows × variant_cols`,
    /// which auto-derives the `texture_atlas` so a `RandomFrame` emitter
    /// draws a different variant per particle from one bake. The legacy
    /// `texture` reference wins when both are set, so already-published
    /// records are unaffected.
    ///
    /// Wire-format note (#695): an ABSENT key legally means "pre-sprite
    /// legacy record → plain `None` quads" (this field-level default),
    /// which differs from the struct default (`SoftDisc`, #367). The field
    /// is therefore marked `(always)` in the eliding-serialize invocation
    /// below - it is written unconditionally so elision can never rewrite
    /// the legacy meaning.
    #[serde(default)]
    pub procedural_texture: super::texture::SovereignTextureConfig,
}

crate::pds::serde_util::impl_default_eliding_serialize!(ParticleParams {
    emitter_shape,
    rate_per_second,
    burst_count,
    max_particles,
    looping,
    duration,
    lifetime_min,
    lifetime_max,
    speed_min,
    speed_max,
    gravity_multiplier,
    acceleration,
    linear_drag,
    start_size,
    end_size,
    start_color,
    end_color,
    blend_mode,
    billboard,
    simulation_space,
    inherit_velocity,
    collide_terrain,
    collide_water,
    collide_colliders,
    bounce,
    friction,
    seed via u64_as_string(u64),
    texture,
    texture_atlas,
    frame_mode,
    texture_filter,
    procedural_texture(always),
});

impl Default for ParticleParams {
    /// Canonical default emitter - a small upward-spraying cone with
    /// 32 particles/s, 2 s lifetime, white→fade-out alpha-blended
    /// particles on a soft-disc sprite (#367, so a freshly-added emitter
    /// reads as soft motes rather than hard squares), no inheritance, no
    /// collisions. See [`GeneratorKind::default_particles`].
    fn default() -> Self {
        Self {
            emitter_shape: EmitterShape::Cone {
                half_angle: Fp(0.4),
                height: Fp(0.5),
            },
            rate_per_second: Fp(32.0),
            burst_count: 0,
            max_particles: 128,
            looping: true,
            duration: Fp(1.0),
            lifetime_min: Fp(1.0),
            lifetime_max: Fp(2.0),
            speed_min: Fp(1.0),
            speed_max: Fp(2.0),
            gravity_multiplier: Fp(0.0),
            acceleration: Fp3([0.0, 0.0, 0.0]),
            linear_drag: Fp(0.5),
            start_size: Fp(0.1),
            end_size: Fp(0.0),
            start_color: Fp4([1.0, 1.0, 1.0, 1.0]),
            end_color: Fp4([1.0, 1.0, 1.0, 0.0]),
            blend_mode: ParticleBlendMode::Alpha,
            billboard: true,
            simulation_space: SimulationSpace::World,
            inherit_velocity: Fp(0.0),
            collide_terrain: false,
            collide_water: false,
            collide_colliders: false,
            bounce: Fp(0.3),
            friction: Fp(0.5),
            seed: 0xC0FFEE,
            texture: None,
            texture_atlas: None,
            frame_mode: AnimationFrameMode::Still,
            texture_filter: TextureFilter::Linear,
            procedural_texture: super::texture::SovereignTextureConfig::SoftDisc(
                super::texture::SovereignSoftDiscConfig::default(),
            ),
        }
    }
}

/// Serde default for [`GeneratorKind::Gateway`]'s interaction zone -
/// arch-sized: roomy enough to walk into without hugging a pillar.
fn default_gateway_size() -> Fp3 {
    Fp3([2.5, 3.0, 2.5])
}

/// Serde default for the Sign's legacy `uv_repeat` (#964): the identity
/// window. A record written after the unification omits the field, and
/// folding `1.0` into the material's scale is a no-op - which is what makes
/// the migration safe to run on every record, old or new.
fn unit_uv_repeat() -> Fp2 {
    Fp2([1.0, 1.0])
}

/// The single declaration of the **parametric-primitive family** - the
/// sixteen [`GeneratorKind`] variants the shared mesher owns, each carrying
/// the same `solid` / `material` / `torture` / `faces` / `uv_mapping` block
/// alongside its own dimensional knobs.
///
/// Before #1156 that roster was re-typed as a sixteen-arm or-pattern in ten
/// places that knew nothing about each other, and a seventeenth primitive
/// had to be hand-added to all of them. Nine of those ladders ended in a
/// `_ =>` catch-all, so a missed one was not a compile error - it was a new
/// shape that silently reported "no material", "not a primitive", or "no
/// bounds". The roster now lives here and nowhere else.
///
/// # Forms
///
/// * `for_each_primitive!(kind_expr, { field, … } => body)` - expands to a
///   `match` over the family binding `field, …` in every arm, evaluating
///   `body` to `Some(_)`; non-primitive variants fall through to `None`.
///   Binds through both `&` and `&mut`.
/// * `for_each_primitive!(pattern { field, … })` - expands to the bare
///   or-pattern, for use as one arm of a caller's own `match`. The field
///   list may be empty. Because the expansion is an ordinary pattern, the
///   caller's match keeps its exhaustiveness check: a variant added to the
///   enum but *not* to this roster fails to compile at every such site.
/// * `for_each_primitive!(tags)` - the roster as
///   `&'static [&'static str]`, matching
///   [`GeneratorKind::kind_tag`]. This
///   is what lets a test *enumerate* the family (see
///   [`primitive_kind_tags`]).
///
/// # Adding a primitive
///
/// 1. Add the variant to `GeneratorKind` and to the roster below.
/// 2. Add its mesher: one `PrimitiveShape` impl and one `prim_parts` arm
///    in `world_builder::prim::shapes` (#644).
/// 3. Add its `kind_tag` and `default_primitive_for_tag` arms.
/// 4. Add its **clamp arm** to `pds::sanitize::primitive`.
/// 5. Add its `pds::ruin::kind_bounds` arm and its editor panel arm in
///    `ui::room::generators::detail`.
///
/// Only `kind_tag` and the editor panel are compile errors. Every other
/// ladder there ends in a catch-all - that is what made a missed one
/// silent - so each has an enumerating test that walks this roster and
/// fails naming the variant: `every_primitive_reaches_a_mesher_arm`
/// (`world_builder::prim`), `every_primitive_has_a_default_and_a_matching_tag`,
/// `every_primitive_clamps_hostile_wire_values` and
/// `every_primitive_delegates_the_shared_blocks` (`pds::sanitize::primitive`),
/// and `every_primitive_has_bounds` (`pds::ruin`). No macro can write a
/// per-variant bound, mesh, or bounding box; a test can insist they exist.
#[macro_export]
macro_rules! for_each_primitive {
    // --- the roster: the one place the family is written down ---------------
    (@roster $mode:tt $args:tt) => {
        $crate::for_each_primitive!(@build $mode $args [
            Cuboid Sphere Cylinder Capsule Cone Torus Plane Tetrahedron
            Tube Bevel Wedge Helix Superellipsoid Spine Lathe BlobGroup
        ])
    };

    // --- expansions ---------------------------------------------------------
    (@build (accessor) ($kind:expr, $fields:tt, $body:expr) [$($v:ident)*]) => {
        match $kind {
            $( $crate::pds::generator::GeneratorKind::$v $fields )|* => Some($body),
            _ => None,
        }
    };
    (@build (pattern) $fields:tt [$($v:ident)*]) => {
        $( $crate::pds::generator::GeneratorKind::$v $fields )|*
    };
    (@build (tags) () [$($v:ident)*]) => {
        &[$(stringify!($v)),*]
    };

    // --- public forms -------------------------------------------------------
    ($kind:expr, { $($field:ident),* $(,)? } => $body:expr) => {
        $crate::for_each_primitive!(@roster (accessor) ($kind, { $($field,)* .. }, $body))
    };
    (pattern { $($field:ident),* $(,)? }) => {
        $crate::for_each_primitive!(@roster (pattern) { $($field,)* .. })
    };
    (tags) => {
        $crate::for_each_primitive!(@roster (tags) ())
    };
}

pub use for_each_primitive;

/// The primitive roster as short tags, in declaration order - the same
/// strings [`GeneratorKind::kind_tag`] returns and
/// [`GeneratorKind::default_primitive_for_tag`] accepts.
///
/// Exists so tests can *enumerate* the family rather than re-listing it:
/// the sanitiser's hostile-record suite and the ruin bounds test both walk
/// this, so a seventeenth primitive is covered the moment it joins the
/// roster in [`for_each_primitive!`].
pub fn primitive_kind_tags() -> &'static [&'static str] {
    for_each_primitive!(tags)
}

/// Variant-specific payload for a [`Generator`]. Open union: unrecognised
/// `$type` tags deserialise to `Unknown` instead of failing the whole record.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "$type")]
// The Terrain variant carries a full `SovereignTerrainConfig` (~400 bytes);
// boxing it would force serde through a wrapping layer that breaks the
// current round-trip tests and the Raw JSON editor format. Generators are
// kept by owning HashMaps, not in hot paths, so the size penalty is fine.
#[allow(clippy::large_enum_variant)]
pub enum GeneratorKind {
    #[serde(rename = "network.symbios.gen.terrain")]
    Terrain(SovereignTerrainConfig),

    /// Water volume. Vertical position comes from the placement
    /// transform's translation.y - no separate level_offset field
    /// (removed as redundant; see [`crate::pds::room`]'s
    /// `default_for_did` for how the canonical homeworld places its
    /// water at the historical altitude via the placement transform).
    #[serde(rename = "network.symbios.gen.water")]
    Water {
        #[serde(default)]
        surface: WaterSurface,
    },

    /// Tensor-field road network draped over the parent terrain. Child-only
    /// (like Water); the sanitiser drops it at root. Its mesh is built by the
    /// terrain plugin from [`RoadConfig`] + the finished heightmap, so the
    /// compile dispatch treats it as inert (no entity), exactly as it does the
    /// Terrain root's own mesh.
    #[serde(rename = "network.symbios.gen.road_network")]
    RoadNetwork(RoadConfig),

    #[serde(rename = "network.symbios.gen.portal")]
    Portal { target_did: String, target_pos: Fp3 },

    /// Social gateway (#747): a walk-in zone that opens the destination
    /// picker listing the room owner's mutual follows. Unlike
    /// [`GeneratorKind::Portal`] it carries no destination - the list is
    /// resolved at interaction time from the live social graph, never
    /// baked into the record. Clients predating this variant decode it as
    /// [`GeneratorKind::Unknown`] (open union) and simply render no gate.
    #[serde(rename = "network.symbios.gen.gateway")]
    Gateway {
        /// Interaction-zone extents in metres - the sensor volume the
        /// themed structure is built around.
        #[serde(default = "default_gateway_size")]
        size: Fp3,
    },

    #[serde(rename = "network.symbios.gen.lsystem")]
    LSystem {
        source_code: String,
        finalization_code: String,
        iterations: u32,
        #[serde(with = "u64_as_string")]
        seed: u64,
        angle: Fp,
        step: Fp,
        width: Fp,
        elasticity: Fp,
        tropism: Option<Fp3>,
        /// Material slot id → PBR settings.
        #[serde(with = "map_u16_as_string")]
        materials: HashMap<u16, SovereignMaterialSettings>,
        /// Prop id → mesh shape.
        #[serde(with = "map_u16_as_string")]
        prop_mappings: HashMap<u16, PropMeshType>,
        prop_scale: Fp,
        mesh_resolution: u32,
    },

    #[serde(rename = "network.symbios.gen.shape")]
    Shape {
        /// Multi-rule CGA Shape Grammar source. One rule per line in the
        /// `Name --> ops` form documented by `symbios_shape::grammar::parse_rule`.
        /// Lines that are blank or start with `//` are skipped at compile time.
        grammar_source: String,
        /// Entry rule that the interpreter starts deriving from. Must appear
        /// in `grammar_source`; if absent, the spawner skips the generator.
        root_rule: String,
        /// Initial scope size passed to `Interpreter::derive`. Y is
        /// typically `0.0` because most grammars `Extrude` the footprint
        /// themselves; the placement transform contributes the world
        /// position and rotation.
        footprint: Fp3,
        /// Stochastic-rule RNG seed. The interpreter weights `A | B | C` by
        /// percentage; the same seed across peers reproduces the same draw.
        #[serde(with = "u64_as_string")]
        seed: u64,
        /// Material name (the string emitted by `Mat("...")` in the grammar)
        /// → PBR settings. A terminal whose `material` is `None` or whose
        /// name has no entry here falls back to the spawner's default
        /// material.
        #[serde(default, serialize_with = "sorted_string_map")]
        materials: HashMap<String, SovereignMaterialSettings>,
        /// Terminal mesh ids - the strings emitted by `I("...")` - whose
        /// terminals render with a **round** cross-section: an elliptical
        /// prism inscribed in the scope's footprint rather than a box.
        /// `Rectangle` profiles become cylinders, `Taper(t)` frusta, and
        /// `Taper(1.0)` cones, which is how columns, silos, chimneys and
        /// spires are built without leaving the OBB-pure grammar.
        ///
        /// Keyed on the *mesh* id, not the material slot: a colonnade's
        /// shafts and its flat entablature commonly share one stone
        /// material, and only the shafts should be turned.
        ///
        /// Empty (the default) keeps every terminal square, so records
        /// written before this field round-trip byte-identically.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        round_meshes: Vec<String>,
        /// Terminal mesh ids - the strings emitted by `I("...")` - whose
        /// terminals are solid (#1506): each carries a box collider on its
        /// scope, as a solid primitive carries one, so a grammar building
        /// stands in a visitor's way with no hidden solid primitive inside
        /// it, and an open shed lists the walls it has and not its open
        /// front. Keyed on the mesh id, as [`Self::Shape`]'s `round_meshes`
        /// is.
        ///
        /// The collider is the scope's box whatever the mesh draws in it:
        /// a turned column collides as the box it is inscribed in, a gable
        /// end as the rectangle round its triangle, and a face a grammar
        /// splits off flat (a wall of `Comp(Faces)`, a roof's slope) as a
        /// slab 1 mm thick, the thickness the mesher draws it at.
        ///
        /// Empty (the default) leaves every terminal without a collider, as
        /// every grammar was before, and is not written, so records written
        /// before this field round-trip byte-identically. A client built
        /// before it ignores the key and lets a visitor walk through - and
        /// a save from one writes the node without it, as it drops a
        /// placement's grammar seed (#1505).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        solid_meshes: Vec<String>,
    },

    #[serde(rename = "network.symbios.gen.cuboid")]
    Cuboid {
        size: Fp3,
        /// The shared block every parametric primitive carries (#1188).
        #[serde(flatten)]
        common: PrimCommon,
    },

    #[serde(rename = "network.symbios.gen.sphere")]
    Sphere {
        radius: Fp,
        resolution: u32,
        /// The shared block every parametric primitive carries (#1188).
        #[serde(flatten)]
        common: PrimCommon,
    },

    #[serde(rename = "network.symbios.gen.cylinder")]
    Cylinder {
        radius: Fp,
        height: Fp,
        resolution: u32,
        /// The shared block every parametric primitive carries (#1188).
        #[serde(flatten)]
        common: PrimCommon,
    },

    #[serde(rename = "network.symbios.gen.capsule")]
    Capsule {
        radius: Fp,
        length: Fp,
        latitudes: u32,
        longitudes: u32,
        /// The shared block every parametric primitive carries (#1188).
        #[serde(flatten)]
        common: PrimCommon,
    },

    #[serde(rename = "network.symbios.gen.cone")]
    Cone {
        radius: Fp,
        height: Fp,
        resolution: u32,
        /// The shared block every parametric primitive carries (#1188).
        #[serde(flatten)]
        common: PrimCommon,
    },

    #[serde(rename = "network.symbios.gen.torus")]
    Torus {
        minor_radius: Fp,
        major_radius: Fp,
        minor_resolution: u32,
        major_resolution: u32,
        /// The shared block every parametric primitive carries (#1188).
        #[serde(flatten)]
        common: PrimCommon,
    },

    #[serde(rename = "network.symbios.gen.plane")]
    Plane {
        size: Fp2,
        subdivisions: u32,
        /// The shared block every parametric primitive carries (#1188).
        #[serde(flatten)]
        common: PrimCommon,
    },

    #[serde(rename = "network.symbios.gen.tetrahedron")]
    Tetrahedron {
        size: Fp,
        /// The shared block every parametric primitive carries (#1188).
        #[serde(flatten)]
        common: PrimCommon,
    },

    /// Hollow cylinder (pipe / ring / well-curb). `radius` is the outer wall,
    /// `inner_radius` the bore (`< radius`); annular caps close the ends. The
    /// collider is a solid outer cylinder - the bore is not a walk-through
    /// volume.
    #[serde(rename = "network.symbios.gen.tube")]
    Tube {
        radius: Fp,
        inner_radius: Fp,
        height: Fp,
        resolution: u32,
        /// The shared block every parametric primitive carries (#1188).
        #[serde(flatten)]
        common: PrimCommon,
    },

    /// Box with chamfered / rounded **vertical** edges - an extruded
    /// rounded-rectangle prism (columns, furniture, rounded buildings).
    /// `bevel` is the corner cut/radius; `bevel_segments` is `1` for a flat
    /// chamfer (octagonal prism) or higher for a rounded corner.
    #[serde(rename = "network.symbios.gen.bevel")]
    Bevel {
        size: Fp3,
        bevel: Fp,
        bevel_segments: u32,
        /// The shared block every parametric primitive carries (#1188).
        #[serde(flatten)]
        common: PrimCommon,
    },

    /// Right-triangular prism - a ramp / roof pitch / buttress / eave. `size` is
    /// the bounding box; the slope rises from the front-bottom (`+Z`, `-Y`) to
    /// the back-top (`-Z`, `+Y`) across the full width (X).
    #[serde(rename = "network.symbios.gen.wedge")]
    Wedge {
        size: Fp3,
        /// The shared block every parametric primitive carries (#1188).
        #[serde(flatten)]
        common: PrimCommon,
    },

    /// Helical tube - a spring / screw / spiral-stair rail / horn / vine.
    /// `radius` is the helix radius, `tube_radius` the wire thickness, `pitch`
    /// the vertical rise per full turn, `turns` the revolution count, and
    /// `resolution` the segments per turn.
    #[serde(rename = "network.symbios.gen.helix")]
    Helix {
        radius: Fp,
        tube_radius: Fp,
        pitch: Fp,
        turns: Fp,
        resolution: u32,
        /// The shared block every parametric primitive carries (#1188).
        #[serde(flatten)]
        common: PrimCommon,
    },

    /// Barr superellipsoid - one prim that morphs continuously from box
    /// (small exponents) through pillow / sphere (`1.0`) toward a pinched
    /// octahedral form (large exponents). `exponent_ns` shapes the
    /// north–south (latitude) profile, `exponent_ew` the east–west
    /// cross-section; `half_extents` scale the three axes. The organic
    /// workhorse for skulls, torsos, pebbles, cushions - the rounded masses
    /// that previously took a scaled sphere or a bevel-box compromise.
    #[serde(rename = "network.symbios.gen.superellipsoid")]
    Superellipsoid {
        half_extents: Fp3,
        exponent_ns: Fp,
        exponent_ew: Fp,
        latitudes: u32,
        longitudes: u32,
        /// The shared block every parametric primitive carries (#1188).
        #[serde(flatten)]
        common: PrimCommon,
    },

    /// Circular-profile tube swept along a user-editable Catmull-Rom spine
    /// with a per-point radius - the one-prim replacement for the tapered-
    /// capsule chains that limbs / tails / horns / tentacles / vines used to
    /// take. The spline passes through every control point (2..16); radius
    /// interpolates along the same spline, and both ends are capped with
    /// flat discs. Vertex torture composes on top, and the topology cuts
    /// map tube-wise (#691): `path_cut` keeps an angular range of the ring
    /// (an open gutter / half-pipe along the curve), `profile_cut` trims
    /// the kept stretch of the path, and `hollow` makes the tube a shell.
    #[serde(rename = "network.symbios.gen.spine")]
    Spine {
        points: Vec<SpinePoint>,
        /// Ring segments around the tube's circular cross-section.
        resolution: u32,
        /// Path samples per spline segment (between consecutive points).
        samples_per_segment: u32,
        /// The shared block every parametric primitive carries (#1188).
        #[serde(flatten)]
        common: PrimCommon,
    },

    /// Profile revolved around local Y - the SL-"rokuro" vase / bell / hoof /
    /// chess-piece prim. `points` is the silhouette from bottom to top
    /// (2..16 stations of radius-at-height); `smooth` interpolates it with a
    /// Catmull-Rom spline (organic curves from few points) or keeps straight
    /// polyline segments (sharp ridges). `path_cut` (angular wedge) and
    /// `hollow` (proportional inner shell) compose exactly like the other
    /// swept prims; `profile_cut` keeps an arc-length band of the silhouette
    /// (slice a vase's top off without re-authoring its stations), with the
    /// trimmed ends capped.
    #[serde(rename = "network.symbios.gen.lathe")]
    Lathe {
        points: Vec<LathePoint>,
        /// Revolve segments around the Y axis.
        resolution: u32,
        /// Spline (`true`) vs straight-segment (`false`) profile.
        smooth: bool,
        /// The shared block every parametric primitive carries (#1188).
        #[serde(flatten)]
        common: PrimCommon,
    },

    /// Smooth-blend SDF group - an ordered list of add/subtract elements
    /// (spheres / capsules / ellipsoids / boxes / cylinders / tori / cones)
    /// evaluated as one signed distance field with per-element polynomial
    /// smooth-min, then meshed once on spawn with surface nets. The Spore /
    /// Dreams organic primitive: a pile of overlapping ellipsoids becomes
    /// one seamless muscle mass, a subtracted sphere carves an eye socket,
    /// and the result is watertight by construction (a broken mesh is
    /// unrepresentable). `resolution` is the sample-grid cell count along
    /// the group's longest axis - the quality/cost dial, clamped hard in
    /// sanitize because grid cost is cubic. Topology cuts apply as hard CSG
    /// on the final field: `profile_cut` keeps a Y-band of the group's
    /// bounds (flat slices - a blob that sits flush on the ground),
    /// `path_cut` keeps a pie wedge around the prim-local Y axis, and
    /// `hollow` erodes an inner shell whose wall is `(1 - hollow)` of the
    /// group's thinnest half-extent (visible wherever a carve or cut opens
    /// the surface). `uv_mapping` picks the texture projection baked into
    /// the meshed surface - see [`UvMapping`] for the trade-offs per mode.
    #[serde(rename = "network.symbios.gen.blob_group")]
    BlobGroup {
        elements: Vec<BlobElement>,
        resolution: u32,
        /// The shared block every parametric primitive carries (#1188).
        #[serde(flatten)]
        common: PrimCommon,
    },

    /// Hand-rolled CPU + ECS particle emitter. Spawns billboarded /
    /// velocity-aligned quads from a parametric shape (point / sphere /
    /// box / cone), integrates them with gravity / drag / constant
    /// acceleration, fades start→end size and colour over each
    /// particle's lifetime, and optionally collides them against
    /// terrain / water / colliders. WASM-friendly because no GPU compute
    /// is involved.
    ///
    /// Velocity inheritance: at spawn, each particle's initial velocity
    /// is `init_velocity + inherit_velocity * emitter_world_velocity`,
    /// where the emitter velocity comes from avian3d's `LinearVelocity`
    /// on the nearest `RigidBody` ancestor (covers the "particle
    /// generator parented under a moving avatar" case) or, failing
    /// that, a numerical derivative of the emitter's world transform.
    /// This lets exhaust trails move correctly with airplanes /
    /// hover-boats / running humanoids without any per-vehicle code.
    ///
    /// Optional texturing rides on the same [`SignSource`] union as the
    /// `Sign` generator and shares the
    /// [`BlobImageCache`](super::super::world_builder::image_cache::BlobImageCache),
    /// so a Sign panel and a particle emitter pointing at the same
    /// image issue one HTTPS round trip. `texture_atlas` plus
    /// `frame_mode` turns a sprite-sheet into per-particle animation
    /// (still / random / over-lifetime cycling).
    ///
    /// Determinism: every emitter carries a `seed`. Networked peers
    /// stepping the same dt path produce the same particle stream.
    #[serde(rename = "network.symbios.gen.particles")]
    ParticleSystem(Box<ParticleParams>),

    /// Image-bearing panel - a flat plane textured with a fetched image
    /// from one of three [`SignSource`] variants. Subsumes the standalone
    /// "profile picture panel" use case (Portal already does the same fetch
    /// internally). `size` is the panel extent in metres, and the
    /// StandardMaterial toggles surface every common knob a signpost /
    /// billboard / pfp panel might need. How the image sits on the panel -
    /// scale, offset, rotation - is the `material`'s job, the same as on
    /// every other surface in the app (#964).
    #[serde(rename = "network.symbios.gen.sign")]
    Sign {
        source: SignSource,
        /// Panel size in metres along the local X / Z axes.
        size: Fp2,
        /// **Legacy** per-axis UV window, superseded by the material's
        /// `uv_scale` / `uv_offset` / `uv_rotation` (#964).
        ///
        /// Kept so records written before the unification still say what
        /// they meant: [`sanitize`](crate::pds::sanitize) folds these two
        /// into `material` and resets them to the identity, after which
        /// nothing downstream reads them.
        ///
        /// **Still written**, at the identity, rather than elided. A client
        /// built before #964 requires the key, so dropping it would fail its
        /// decode of the whole generator; writing the identity instead makes
        /// an old client render the image spanning the panel once - the
        /// same graceful degradation the per-face work chose (#956). The
        /// serde default covers hand-written JSON that omits it.
        ///
        /// The migration is exact for the square case. A legacy *anisotropic*
        /// window cannot survive a uniform scale, so the larger repeat wins -
        /// no axis gains image content it did not already show.
        #[serde(default = "unit_uv_repeat")]
        uv_repeat: Fp2,
        /// **Legacy** UV offset; see [`uv_repeat`](Self::Sign::uv_repeat).
        #[serde(default)]
        uv_offset: Fp2,
        /// Tint + emissive + PBR knobs, plus the UV transform (`uv_scale` =
        /// how many times the image spans the panel, `uv_offset` in spans,
        /// `uv_rotation` in degrees). The fetched image overrides the
        /// procedural slot - set `texture` to `None` so the loaded image is
        /// the only colour source.
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
        material: SovereignMaterialSettings,
        /// `true` renders both faces of the plane (and disables backface
        /// culling). Useful for free-standing signs viewable from either
        /// side; `false` for wall-mounted decals.
        double_sided: bool,
        /// Translucency mode. `Opaque` (no alpha), `Mask(cutoff)` for
        /// punch-through PNGs (cutout signs), `Blend` for soft-edged
        /// translucent textures. Mirrors Bevy's `AlphaMode` open-union-
        /// style.
        alpha_mode: AlphaModeKind,
        /// `true` skips PBR lighting, painting the texture flat regardless
        /// of sun angle. Critical for legibility on profile pics / signs.
        unlit: bool,
        /// Sampler filter for the fetched image. `Linear` (the default,
        /// and the behaviour of every pre-#663 record) smooths photos;
        /// `Nearest` keeps pixel-art signage crisp. Serde-defaulted so
        /// existing records deserialize unchanged.
        #[serde(default)]
        texture_filter: TextureFilter,
    },

    #[serde(other, skip_serializing)]
    Unknown,
}

/// Image-source alias retained for backwards compatibility. The canonical
/// type is [`SovereignAssetReference`] - the same enum was originally
/// introduced here as `SignSource` but generalised when texture and audio
/// dropdowns gained their own Referenced variants. The `$type` wire tags
/// (`network.symbios.sign.*`) are unchanged so already-published records
/// keep deserialising; only the in-code name moved.
///
/// All three variants still resolve through the shared `BlobImageCache`
/// in `world_builder::image_cache` for image consumers (Sign panels,
/// particle textures); audio consumers go through the sibling
/// `BlobAudioCache` pattern.
///
/// [`SovereignAssetReference`]: crate::pds::asset_reference::SovereignAssetReference
pub use crate::pds::asset_reference::SovereignAssetReference as SignSource;

/// Open-union mirror of Bevy's `AlphaMode`. Wire-tagged so an unknown
/// variant from a forward-compatible record decodes to `Unknown` rather
/// than failing the whole generator.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(tag = "$type")]
pub enum AlphaModeKind {
    /// Fully opaque - no alpha lookup, fastest.
    #[serde(rename = "network.symbios.alpha.opaque")]
    #[default]
    Opaque,
    /// Hard cutout: alpha < `cutoff` → discard, alpha ≥ `cutoff` → opaque.
    /// `cutoff` is in `[0, 1]`; the sanitiser clamps.
    #[serde(rename = "network.symbios.alpha.mask")]
    Mask { cutoff: Fp },
    /// Standard alpha blending. Sorted by Bevy's transparent-pass writer.
    #[serde(rename = "network.symbios.alpha.blend")]
    Blend,

    #[serde(other, skip_serializing)]
    Unknown,
}

/// Emitter-shape open union for [`GeneratorKind::ParticleSystem`]. Each
/// variant defines the spawn-position distribution and the default
/// emit-direction; per-variant payload fields tune the shape itself.
/// `Unknown` keeps a record from a future engine version round-tripping.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(tag = "$type")]
pub enum EmitterShape {
    /// Single point emitter at the local origin. Default emit direction
    /// is local +Y; particle spread comes from per-particle randomness
    /// in the speed sample rather than the shape itself.
    #[serde(rename = "network.symbios.particle.point")]
    #[default]
    Point,
    /// Solid sphere of `radius`. Particles spawn at a uniform-random
    /// position inside the sphere and inherit a default outward emit
    /// direction (radial unit vector).
    #[serde(rename = "network.symbios.particle.sphere")]
    Sphere { radius: Fp },
    /// Axis-aligned box of `half_extents`. Particles spawn uniformly
    /// inside; emit direction defaults to local +Y.
    #[serde(rename = "network.symbios.particle.box")]
    Box { half_extents: Fp3 },
    /// Cone with apex at the local origin pointing along local +Y.
    /// `half_angle` (radians) bounds the spawn cone; `height` scales
    /// the cone's depth so particles can spawn anywhere along it.
    #[serde(rename = "network.symbios.particle.cone")]
    Cone { half_angle: Fp, height: Fp },

    #[serde(other, skip_serializing)]
    Unknown,
}

/// Particle blend-mode open union. `Alpha` is standard front-to-back
/// transparency (smoke, soft sprites); `Additive` is brightness-additive
/// (sparks, fire, glow). Mirrors the two surface-level blend modes any
/// reasonable particle system supports without exposing the full GPU
/// blend-state matrix.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(tag = "$type")]
pub enum ParticleBlendMode {
    #[serde(rename = "network.symbios.particle.blend.alpha")]
    #[default]
    Alpha,
    #[serde(rename = "network.symbios.particle.blend.additive")]
    Additive,

    #[serde(other, skip_serializing)]
    Unknown,
}

/// Simulation-space open union for [`GeneratorKind::ParticleSystem`].
/// `Local` parents particles under the emitter (auras and clouds that
/// follow the emitter); `World` spawns particles unparented in world
/// coordinates so they are left behind as the emitter moves (exhaust,
/// dust trails).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(tag = "$type")]
pub enum SimulationSpace {
    #[serde(rename = "network.symbios.particle.space.world")]
    #[default]
    World,
    #[serde(rename = "network.symbios.particle.space.local")]
    Local,

    #[serde(other, skip_serializing)]
    Unknown,
}

/// Sprite-sheet atlas dimensions for a textured particle. The image is
/// divided into a `rows × cols` grid; each cell is one animation frame
/// (or one randomised sprite, depending on
/// [`AnimationFrameMode`]). The sanitiser caps each axis at 16, so an
/// atlas tops out at 256 frames - well past any plausible particle
/// effect and inside the per-frame mesh-cache budget.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TextureAtlas {
    pub rows: u32,
    pub cols: u32,
}

impl Default for TextureAtlas {
    fn default() -> Self {
        Self { rows: 1, cols: 1 }
    }
}

/// Frame-cycling mode for textured particles.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(tag = "$type")]
pub enum AnimationFrameMode {
    /// Single static frame (frame 0). Default - matches a solid
    /// non-animated sprite.
    #[serde(rename = "network.symbios.particle.frame.still")]
    #[default]
    Still,
    /// Each particle picks one frame uniformly at spawn and keeps it
    /// for its entire lifetime. Useful when an atlas holds a set of
    /// "leaf shape" or "snowflake" variants and you want visual
    /// variety without animation.
    #[serde(rename = "network.symbios.particle.frame.random")]
    RandomFrame,
    /// Cycle through every frame in `rows × cols` order at the
    /// configured rate. Particles whose lifetime is shorter than
    /// `frame_count / fps` truncate; longer lifetimes loop back to
    /// frame 0 (modulo).
    #[serde(rename = "network.symbios.particle.frame.over_lifetime")]
    OverLifetime { fps: Fp },

    #[serde(other, skip_serializing)]
    Unknown,
}

/// Sampler filter applied to a textured particle's image. `Linear`
/// smooth-filters (default; soft sprites); `Nearest` snaps to texels
/// (pixel-art look). The image cache keys on this so a Linear and a
/// Nearest request for the same source coexist as separate Image
/// assets.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[serde(tag = "$type")]
pub enum TextureFilter {
    #[serde(rename = "network.symbios.particle.filter.linear")]
    #[default]
    Linear,
    #[serde(rename = "network.symbios.particle.filter.nearest")]
    Nearest,

    #[serde(other, skip_serializing)]
    Unknown,
}

impl GeneratorKind {
    /// Canonical default kind for a newly-added primitive - a 1×1×1 cuboid
    /// with zero torture and a blank material. Used by UI "+ Cuboid" flows
    /// and when the sanitizer overwrites a forbidden `Terrain`/`Water`
    /// generator nested inside another generator.
    pub fn default_cuboid() -> Self {
        GeneratorKind::Cuboid {
            size: Fp3([1.0, 1.0, 1.0]),
            common: PrimCommon {
                solid: true,
                material: SovereignMaterialSettings::default(),
                ..Default::default()
            },
        }
    }

    /// The shared block of any parametric primitive (#1188); `None` for
    /// non-primitive variants. The one place the sixteen-arm match lives,
    /// generated from the [`for_each_primitive!`] roster.
    pub fn common(&self) -> Option<&PrimCommon> {
        for_each_primitive!(self, { common } => common)
    }

    /// Mutable access to a primitive's shared block; `None` for
    /// non-primitive variants.
    pub fn common_mut(&mut self) -> Option<&mut PrimCommon> {
        for_each_primitive!(self, { common } => common)
    }

    /// Shared read access to the vertex-torture parameters of any parametric
    /// primitive; `None` for non-primitive variants.
    pub fn torture(&self) -> Option<&TortureParams> {
        self.common().map(|c| &c.torture)
    }

    /// Shared mutable access to a primitive's vertex-torture parameters; `None`
    /// for non-primitive variants.
    pub fn torture_mut(&mut self) -> Option<&mut TortureParams> {
        self.common_mut().map(|c| &mut c.torture)
    }

    /// Shared read access to a parametric primitive's **base** material;
    /// `None` for non-primitive variants. The `Sign` panel and the
    /// slot-mapped `Shape` / `LSystem` material maps are deliberately
    /// excluded - `node_materials_mut` in `material_finish` is the
    /// every-material walk.
    pub fn material(&self) -> Option<&SovereignMaterialSettings> {
        self.common().map(|c| &c.material)
    }

    /// Shared mutable access to a primitive's base material; `None` for
    /// non-primitive variants.
    pub fn material_mut(&mut self) -> Option<&mut SovereignMaterialSettings> {
        self.common_mut().map(|c| &mut c.material)
    }

    /// Shared read access to a primitive's per-face overrides (#955);
    /// `None` for non-primitive variants.
    pub fn faces(&self) -> Option<&[FaceOverride]> {
        self.common().map(|c| c.faces.as_slice())
    }

    /// Shared mutable access to a primitive's per-face overrides; `None`
    /// for non-primitive variants.
    pub fn faces_mut(&mut self) -> Option<&mut Vec<FaceOverride>> {
        self.common_mut().map(|c| &mut c.faces)
    }

    /// The projection a primitive family uses when its record names none:
    /// `Box` for the flat family (Cuboid, Tetrahedron, Bevel, Wedge,
    /// Superellipsoid, BlobGroup) - metre-scale tri-planar, so texel
    /// density is even across faces of any proportion - and `Fit` for the
    /// Plane and the revolved family, whose meshers have an analytic
    /// parameterisation of their own. `None` for non-primitive variants.
    ///
    /// Two lists rather than a catch-all so a seventeenth primitive has to
    /// be placed; `every_primitive_has_a_default_and_a_matching_tag` walks
    /// the roster and fails on one that is not.
    pub fn family_uv_mapping(&self) -> Option<UvMapping> {
        match self {
            GeneratorKind::Cuboid { .. }
            | GeneratorKind::Tetrahedron { .. }
            | GeneratorKind::Bevel { .. }
            | GeneratorKind::Wedge { .. }
            | GeneratorKind::Superellipsoid { .. }
            | GeneratorKind::BlobGroup { .. } => Some(UvMapping::Box),
            GeneratorKind::Sphere { .. }
            | GeneratorKind::Cylinder { .. }
            | GeneratorKind::Capsule { .. }
            | GeneratorKind::Cone { .. }
            | GeneratorKind::Torus { .. }
            | GeneratorKind::Plane { .. }
            | GeneratorKind::Tube { .. }
            | GeneratorKind::Helix { .. }
            | GeneratorKind::Spine { .. }
            | GeneratorKind::Lathe { .. } => Some(UvMapping::Fit),
            _ => None,
        }
    }

    /// The primitive's whole-prim texture projection (#955), resolved:
    /// the record's own choice, or the family default
    /// ([`Self::family_uv_mapping`]) when the record names none. `None`
    /// for non-primitive variants.
    pub fn uv_mapping(&self) -> Option<UvMapping> {
        let family = self.family_uv_mapping()?;
        Some(self.common()?.uv_mapping.unwrap_or(family))
    }

    /// Set a primitive's texture projection, storing the family default
    /// as `None` so the wire form stays what an older client wrote for the
    /// same picture. Returns `false` (and does nothing) for non-primitive
    /// variants.
    pub fn set_uv_mapping(&mut self, mapping: UvMapping) -> bool {
        let Some(family) = self.family_uv_mapping() else {
            return false;
        };
        let Some(common) = self.common_mut() else {
            return false;
        };
        common.uv_mapping = (mapping != family).then_some(mapping);
        true
    }

    /// `true` when the variant is a parametric primitive - one of the
    /// sixteen on the [`for_each_primitive!`] roster. Used by the UI
    /// primitive-kind picker and by the spawner to dispatch into the
    /// shared mesh/collider path.
    pub fn is_primitive(&self) -> bool {
        self.common().is_some()
    }

    /// Whether a node of this kind may turn (#1604). A spin turns a node
    /// about its own origin with everything below it: on a Terrain root that
    /// is a whole region carried round its origin, and Water and roads are
    /// laid in the world's terms whatever their node's pose. The sanitiser
    /// takes a spin off these kinds and the spawner ignores one.
    pub fn may_spin(&self) -> bool {
        !matches!(
            self,
            GeneratorKind::Terrain(_) | GeneratorKind::Water { .. } | GeneratorKind::RoadNetwork(_)
        )
    }

    /// Short **wire** tag for the variant - the serialized `$type`
    /// discriminant, and the key into
    /// `ui::room::construct::make_default_for_kind`.
    ///
    /// **Not a label.** These are CamelCase serde tags, and until #1267
    /// they were the display text of every buildable kind: the creation
    /// menu handed the owner a bare list reading "BlobGroup", "LSystem",
    /// "Superellipsoid", "ParticleSystem", with no descriptions anywhere,
    /// and a visitor deciding whether to accept a gift was told its kind
    /// as "BlobGroup". Use [`display_name`](Self::display_name) and
    /// [`blurb`](Self::blurb) for anything a person reads; this stays
    /// exactly as it is, because it is on the wire.
    pub fn kind_tag(&self) -> &'static str {
        match self {
            GeneratorKind::Terrain(_) => "Terrain",
            GeneratorKind::Water { .. } => "Water",
            GeneratorKind::RoadNetwork(_) => "RoadNetwork",
            GeneratorKind::Portal { .. } => "Portal",
            GeneratorKind::Gateway { .. } => "Gateway",
            GeneratorKind::LSystem { .. } => "LSystem",
            GeneratorKind::Shape { .. } => "Shape",
            GeneratorKind::Cuboid { .. } => "Cuboid",
            GeneratorKind::Sphere { .. } => "Sphere",
            GeneratorKind::Cylinder { .. } => "Cylinder",
            GeneratorKind::Capsule { .. } => "Capsule",
            GeneratorKind::Cone { .. } => "Cone",
            GeneratorKind::Torus { .. } => "Torus",
            GeneratorKind::Plane { .. } => "Plane",
            GeneratorKind::Tetrahedron { .. } => "Tetrahedron",
            GeneratorKind::Tube { .. } => "Tube",
            GeneratorKind::Bevel { .. } => "Bevel",
            GeneratorKind::Wedge { .. } => "Wedge",
            GeneratorKind::Helix { .. } => "Helix",
            GeneratorKind::Superellipsoid { .. } => "Superellipsoid",
            GeneratorKind::Spine { .. } => "Spine",
            GeneratorKind::Lathe { .. } => "Lathe",
            GeneratorKind::BlobGroup { .. } => "BlobGroup",
            GeneratorKind::Sign { .. } => "Sign",
            GeneratorKind::ParticleSystem(..) => "ParticleSystem",
            GeneratorKind::Unknown => "Unknown",
        }
    }

    /// What a person is told this kind is (#1267 f214).
    ///
    /// Keyed on the wire tag rather than on `self`, so the creation menus -
    /// which offer tags, not constructed values - can name what they are
    /// offering without building one of each first. An unrecognised tag
    /// comes back as itself: a menu that silently dropped an entry it
    /// could not name would be worse than one that shows the raw word.
    ///
    /// The names are the *thing*, not the algorithm behind it: a
    /// `Superellipsoid` is a rounded box to everybody who is not
    /// implementing one, and `LSystem` is how the plant is grown, not what
    /// it is.
    pub fn display_name(tag: &str) -> &str {
        match tag {
            "Terrain" => "Terrain",
            "Water" => "Water",
            "RoadNetwork" => "Roads",
            "Portal" => "Portal",
            "Gateway" => "Gateway",
            "LSystem" => "Plant",
            "Shape" => "Shape grammar",
            "Cuboid" => "Box",
            "Sphere" => "Sphere",
            "Cylinder" => "Cylinder",
            "Capsule" => "Capsule",
            "Cone" => "Cone",
            "Torus" => "Ring",
            "Plane" => "Flat panel",
            "Tetrahedron" => "Tetrahedron",
            "Tube" => "Tube",
            "Bevel" => "Bevelled box",
            "Wedge" => "Wedge",
            "Helix" => "Helix",
            "Superellipsoid" => "Rounded box",
            "Spine" => "Swept shape",
            "Lathe" => "Turned profile",
            "BlobGroup" => "Blob group",
            "Sign" => "Sign",
            "ParticleSystem" => "Particles",
            other => other,
        }
    }

    /// One line saying what this kind is FOR, for the hover beside
    /// [`display_name`](Self::display_name). Empty for a tag with nothing
    /// worth saying - the callers skip the hover rather than attach a
    /// blank one.
    ///
    /// The Catalogue proved this affordable: its leaves have hovered
    /// `entry.description()` since they shipped, while the primary
    /// creation surface for the owner-only feature the product is built
    /// around had none.
    pub fn blurb(tag: &str) -> &'static str {
        match tag {
            "Terrain" => "The ground itself - height, erosion and the materials on it",
            "Water" => "One water surface across the whole world",
            "RoadNetwork" => "Streets and junctions, with buildings along them",
            "Portal" => "A doorway to somebody else's world",
            "Gateway" => "A doorway to the people you and the owner both follow",
            "LSystem" => "A tree, vine or shrub grown from a grammar",
            "Shape" => "A structure grown by rules - split, repeat, taper",
            "Cuboid" => "A box",
            "Sphere" => "A ball",
            "Cylinder" => "A rod or disc",
            "Capsule" => "A rod with rounded ends",
            "Cone" => "A cone or a truncated one",
            "Torus" => "A doughnut ring",
            "Plane" => "A flat rectangle with no thickness",
            "Tetrahedron" => "A four-faced pyramid",
            "Tube" => "A pipe - a cylinder with the middle taken out",
            "Bevel" => "A box with its edges cut back",
            "Wedge" => "A ramp",
            "Helix" => "A spiral - a ramp, a spring or a screw",
            "Superellipsoid" => "A box you can round off towards a ball",
            "Spine" => "A profile swept along a curve you draw",
            "Lathe" => "A profile spun around an axis, like a vase",
            "BlobGroup" => "Soft shapes that melt into one another",
            "Sign" => "A flat panel carrying an image or text",
            "ParticleSystem" => "A continuous emitter - smoke, sparks, dust",
            _ => "",
        }
    }

    /// Build a default primitive kind for `tag`. Returns `None` for non-
    /// primitive tags - callers that want an L-system or Portal should
    /// construct those variants directly since they carry more state than
    /// sensible defaults capture.
    pub fn default_primitive_for_tag(tag: &str) -> Option<Self> {
        let mat = SovereignMaterialSettings::default();
        Some(match tag {
            "Cuboid" => GeneratorKind::Cuboid {
                common: PrimCommon {
                    solid: true,
                    material: mat,
                    ..Default::default()
                },
                size: Fp3([1.0, 1.0, 1.0]),
            },
            "Sphere" => GeneratorKind::Sphere {
                radius: Fp(0.5),
                resolution: 3,
                common: PrimCommon {
                    solid: true,
                    material: mat,
                    ..Default::default()
                },
            },
            "Cylinder" => GeneratorKind::Cylinder {
                radius: Fp(0.5),
                height: Fp(1.0),
                resolution: 16,
                common: PrimCommon {
                    solid: true,
                    material: mat,
                    ..Default::default()
                },
            },
            "Capsule" => GeneratorKind::Capsule {
                radius: Fp(0.5),
                length: Fp(1.0),
                latitudes: 8,
                longitudes: 16,
                common: PrimCommon {
                    solid: true,
                    material: mat,
                    ..Default::default()
                },
            },
            "Cone" => GeneratorKind::Cone {
                radius: Fp(0.5),
                height: Fp(1.0),
                resolution: 16,
                common: PrimCommon {
                    solid: true,
                    material: mat,
                    ..Default::default()
                },
            },
            "Torus" => GeneratorKind::Torus {
                minor_radius: Fp(0.1),
                major_radius: Fp(0.5),
                minor_resolution: 12,
                major_resolution: 24,
                common: PrimCommon {
                    solid: true,
                    material: mat,
                    ..Default::default()
                },
            },
            "Plane" => GeneratorKind::Plane {
                common: PrimCommon {
                    solid: true,
                    material: mat,
                    ..Default::default()
                },
                size: Fp2([1.0, 1.0]),
                subdivisions: 0,
            },
            "Tetrahedron" => GeneratorKind::Tetrahedron {
                common: PrimCommon {
                    solid: true,
                    material: mat,
                    ..Default::default()
                },
                size: Fp(1.0),
            },
            "Tube" => GeneratorKind::Tube {
                radius: Fp(0.5),
                inner_radius: Fp(0.3),
                height: Fp(1.0),
                resolution: 24,
                common: PrimCommon {
                    solid: true,
                    material: mat,
                    ..Default::default()
                },
            },
            "Bevel" => GeneratorKind::Bevel {
                common: PrimCommon {
                    solid: true,
                    material: mat,
                    ..Default::default()
                },
                size: Fp3([1.0, 1.0, 1.0]),
                bevel: Fp(0.15),
                bevel_segments: 3,
            },
            "Wedge" => GeneratorKind::Wedge {
                common: PrimCommon {
                    solid: true,
                    material: mat,
                    ..Default::default()
                },
                size: Fp3([1.0, 1.0, 1.0]),
            },
            "Helix" => GeneratorKind::Helix {
                radius: Fp(0.5),
                tube_radius: Fp(0.1),
                pitch: Fp(0.4),
                turns: Fp(3.0),
                resolution: 24,
                common: PrimCommon {
                    solid: true,
                    material: mat,
                    ..Default::default()
                },
            },
            // Exponents at 0.5 default to the pillow / rounded-box middle of
            // the family - visually distinct from both Cuboid and Sphere, so
            // a freshly-added prim reads as its own thing.
            "Superellipsoid" => GeneratorKind::Superellipsoid {
                common: PrimCommon {
                    solid: true,
                    material: mat,
                    ..Default::default()
                },
                half_extents: Fp3([0.5, 0.5, 0.5]),
                exponent_ns: Fp(0.5),
                exponent_ew: Fp(0.5),
                latitudes: 16,
                longitudes: 24,
            },
            // A gentle forward-arcing taper so a freshly-added spine reads
            // as a limb / tail rather than a straight pipe.
            "Spine" => GeneratorKind::Spine {
                points: vec![
                    SpinePoint {
                        position: Fp3([0.0, -0.5, 0.0]),
                        radius: Fp(0.2),
                    },
                    SpinePoint {
                        position: Fp3([0.12, 0.0, 0.08]),
                        radius: Fp(0.15),
                    },
                    SpinePoint {
                        position: Fp3([0.0, 0.5, 0.0]),
                        radius: Fp(0.09),
                    },
                ],
                resolution: 12,
                samples_per_segment: 8,
                common: PrimCommon {
                    solid: true,
                    material: mat,
                    ..Default::default()
                },
            },
            // A bellied vase silhouette - the canonical lathe demo shape.
            "Lathe" => GeneratorKind::Lathe {
                points: vec![
                    LathePoint {
                        radius: Fp(0.18),
                        height: Fp(-0.5),
                    },
                    LathePoint {
                        radius: Fp(0.32),
                        height: Fp(-0.25),
                    },
                    LathePoint {
                        radius: Fp(0.2),
                        height: Fp(0.1),
                    },
                    LathePoint {
                        radius: Fp(0.1),
                        height: Fp(0.3),
                    },
                    LathePoint {
                        radius: Fp(0.16),
                        height: Fp(0.5),
                    },
                ],
                resolution: 24,
                smooth: true,
                common: PrimCommon {
                    solid: true,
                    material: mat,
                    ..Default::default()
                },
            },
            // Two generously-blended spheres - the smallest recipe that
            // shows what the prim is for (they merge into one peanut mass).
            "BlobGroup" => GeneratorKind::BlobGroup {
                elements: vec![
                    BlobElement {
                        position: Fp3([0.0, -0.15, 0.0]),
                        radii: Fp3([0.3, 0.3, 0.3]),
                        ..Default::default()
                    },
                    BlobElement {
                        position: Fp3([0.0, 0.22, 0.0]),
                        radii: Fp3([0.2, 0.2, 0.2]),
                        blend: Fp(0.15),
                        ..Default::default()
                    },
                ],
                resolution: 32,
                common: PrimCommon {
                    solid: true,
                    material: mat,
                    ..Default::default()
                },
            },
            _ => return None,
        })
    }

    /// Canonical default `Sign` - a 1×1 m unlit, opaque, single-sided panel
    /// with an empty URL source. Used by the UI "+ Sign" entry and by
    /// `ui::room::construct::make_default_for_kind`.
    pub fn default_sign() -> Self {
        GeneratorKind::Sign {
            source: SignSource::default(),
            size: Fp2([1.0, 1.0]),
            uv_repeat: Fp2([1.0, 1.0]),
            uv_offset: Fp2([0.0, 0.0]),
            material: SovereignMaterialSettings::default(),
            double_sided: false,
            alpha_mode: AlphaModeKind::Opaque,
            unlit: true,
            texture_filter: TextureFilter::Linear,
        }
    }

    /// Canonical default `ParticleSystem` - a small upward-spraying
    /// emitter with 32 particles/s, 2 s lifetime, white→fade-out
    /// alpha-blended particles on a soft-disc sprite (#367, so a
    /// freshly-added emitter reads as soft motes rather than hard
    /// squares), no inheritance, no collisions. Used by the UI
    /// "+ ParticleSystem" entry; the editor surfaces every parameter -
    /// including the sprite picker - for tuning afterwards.
    pub fn default_particles() -> Self {
        GeneratorKind::ParticleSystem(Box::default())
    }
}

/// A hierarchical generator: variant-specific payload + local transform +
/// child generators. Top-level entries in `RoomRecord::generators` are
/// `Generator`s; so are every node in any of their child trees. The wire
/// format flattens `kind` so each node is one tagged JSON object carrying
/// `$type`, the variant fields, `transform`, and `children`.
///
/// A `Vec<Generator>` is heap-allocated, so the recursion through `children`
/// is finite-sized at compile time without an explicit `Box`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Generator {
    #[serde(flatten)]
    pub kind: GeneratorKind,
    // The three non-kind fields elide their common case on the wire (#695):
    // an identity transform, no children, silent audio. Each already
    // decodes missing → default, so elision is round-trip-exact.
    #[serde(default, skip_serializing_if = "TransformData::is_identity")]
    pub transform: TransformData,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Generator>,
    /// Optional emissive audio source attached to this node - spatially
    /// played at the node's world position by Bevy's spatial audio
    /// pipeline. Forward-compat across older records: missing field
    /// decodes via `#[serde(default)]` to
    /// [`SovereignAudioConfig::None`](super::audio::SovereignAudioConfig::None)
    /// (silent). Set non-None by
    /// catalogue entries that want a construct to hum / chime / drone
    /// at its location (e.g. the teleporter's portal core).
    #[serde(
        default,
        skip_serializing_if = "super::audio::SovereignAudioConfig::is_none"
    )]
    pub audio: super::audio::SovereignAudioConfig,
    /// How each client turns this node and everything below it (#1604) -
    /// animation only, never the `transform` on record. `None` (the common
    /// case, left off the wire) keeps the node at its authored pose; an
    /// older client ignores the key. See [`super::spin`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spin: Option<super::spin::Spin>,
}

impl Generator {
    /// Wrap a kind with the canonical defaults: identity transform and no
    /// children. Use this when you want a leaf-shaped generator and don't
    /// care about hierarchy.
    pub fn from_kind(kind: GeneratorKind) -> Self {
        Self {
            kind,
            transform: TransformData::default(),
            children: Vec::new(),
            audio: super::audio::SovereignAudioConfig::None,
            spin: None,
        }
    }

    /// The node's spin, when it turns anything (see
    /// [`super::spin::Spin::moves`]) on a kind that may turn
    /// ([`GeneratorKind::may_spin`]).
    pub fn moving_spin(&self) -> Option<&super::spin::Spin> {
        if !self.kind.may_spin() {
            return None;
        }
        self.spin.as_ref().filter(|spin| spin.moves())
    }

    /// Convenience constructor for the canonical 1×1×1 cuboid.
    pub fn default_cuboid() -> Self {
        Self::from_kind(GeneratorKind::default_cuboid())
    }

    /// `true` when the variant is a parametric primitive. Delegates to the
    /// inner kind so call sites that already hold a `Generator` don't have
    /// to peel into `.kind` themselves.
    pub fn is_primitive(&self) -> bool {
        self.kind.is_primitive()
    }

    /// Short human-readable tag for the variant. See [`GeneratorKind::kind_tag`].
    pub fn kind_tag(&self) -> &'static str {
        self.kind.kind_tag()
    }

    /// Build a default primitive `Generator` (identity transform, no
    /// children) for `tag`. Returns `None` for non-primitive tags.
    pub fn default_primitive_for_tag(tag: &str) -> Option<Self> {
        GeneratorKind::default_primitive_for_tag(tag).map(Self::from_kind)
    }
}

impl Default for Generator {
    fn default() -> Self {
        Self::default_cuboid()
    }
}

/// Where and how a generator is instantiated.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "$type")]
pub enum Placement {
    #[serde(rename = "network.symbios.place.absolute")]
    Absolute {
        generator_ref: String,
        #[serde(default, skip_serializing_if = "TransformData::is_identity")]
        transform: TransformData,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        snap_to_terrain: bool,
        /// When terrain-snapped, refuse submerged ground: the compiler
        /// walks the anchor along its bearing through the origin
        /// (preserving a spawn-facing yaw) until the terrain rises
        /// above the room's water line. Used by the seeded landmark so
        /// a coastal villa doesn't spawn waist-deep in the sea.
        /// `#[serde(default)]` keeps older records decoding unchanged.
        #[serde(default, skip_serializing_if = "is_false")]
        avoid_water: bool,
        /// Dry-land clearance radius (m) for [`Self::Absolute::avoid_water`]:
        /// the walk requires the centre *and* a ring of samples at this
        /// radius to clear the water line, so a wide footprint can't pass
        /// on a dry anchor while the rest of the building floods. `0`
        /// checks the centre only.
        #[serde(default)]
        avoid_water_clearance: Fp,
        /// The seed every [`GeneratorKind::Shape`] node in the placed tree
        /// derives with, in place of its own (#1505), so that one generator
        /// can stand in a street many times and each copy draw its own
        /// variety instead of each needing a copy of the grammar. It
        /// replaces the node's seed rather than mixing with it: a placement
        /// whose seed is the node's own draws exactly what an unseeded one
        /// draws. Nothing else in the tree reads it - an L-system, a
        /// particle system, the terrain keep their own seeds - so every
        /// copy of one generator grows the same plant and emits the same
        /// particle stream, in step: houses moved onto one generator keep
        /// their grammars' variety, not their chimneys' own smoke.
        ///
        /// A quoted decimal on the wire, like the Shape node's own `seed`,
        /// and left out when `None`, so a record that never set it keeps
        /// its bytes. A client built before it existed ignores the key and
        /// draws every copy with the generator's own seed. Absolute
        /// placements only: a scatter or a grid draws every copy alike.
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "option_u64_as_string"
        )]
        seed: Option<u64>,
    },

    #[serde(rename = "network.symbios.place.scatter")]
    Scatter {
        generator_ref: String,
        bounds: ScatterBounds,
        count: u32,
        #[serde(with = "u64_as_string")]
        local_seed: u64,
        /// Combined biome allow-list + water-surface relation. A default
        /// `BiomeFilter` accepts every sample (and is elided on the wire -
        /// `is_noop` is exactly the default state).
        #[serde(default, skip_serializing_if = "BiomeFilter::is_noop")]
        biome_filter: BiomeFilter,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        snap_to_terrain: bool,
        /// Apply a deterministic random yaw (per `local_seed`) to every
        /// scattered instance. Defaults to `true` for backward compatibility
        /// with records written before this field existed.
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        random_yaw: bool,
        /// Reject scatter points that fall inside the room's road-network
        /// district - a circle of radius `RoadConfig::district_half_extent`
        /// around spawn. Keeps the seeded natural scatters (trees, boulders)
        /// clear of the built-up urban area (roads *and* lot buildings)
        /// without needing an annulus bounds shape. Resolved at compile
        /// against [`crate::pds::room::find_road_config`]; a no-op in a room
        /// with no enabled road network. Defaults `false`.
        #[serde(default, skip_serializing_if = "is_false")]
        avoid_urban: bool,
        /// Spawn each instance at the room's **water surface** instead of on
        /// the terrain under it, for floating cover - lily pads on a wetland
        /// pool (#914). Only ever *raises* an instance: ground above the
        /// water line keeps its terrain height, so a pad sampled onto a
        /// shore bank sits on the bank rather than sinking to a phantom
        /// water level inside it. A room with no water surface leaves every
        /// instance terrain-snapped.
        ///
        /// Opt-in per placement, never a default - trees standing in water
        /// was a real bug (#335), and water placement stays something a
        /// species asks for explicitly.
        #[serde(default, skip_serializing_if = "is_false")]
        float_on_water: bool,
        /// Placement-naturalness dials - clustering, edge falloff,
        /// per-instance scale/tilt, slope cutoff (#912). All-default is the
        /// historical flat-uniform sprinkle and is elided on the wire.
        #[serde(default, skip_serializing_if = "ScatterNaturalness::is_noop")]
        naturalness: ScatterNaturalness,
    },

    #[serde(rename = "network.symbios.place.grid")]
    Grid {
        generator_ref: String,
        #[serde(default, skip_serializing_if = "TransformData::is_identity")]
        transform: TransformData,
        counts: [u32; 3],
        gaps: Fp3,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        snap_to_terrain: bool,
        /// Apply a per-cell deterministic random yaw. Defaults to `false`
        /// - grids are typically axis-aligned.
        #[serde(default, skip_serializing_if = "is_false")]
        random_yaw: bool,
    },

    #[serde(other, skip_serializing)]
    Unknown,
}

impl Placement {
    /// The seed every [`GeneratorKind::Shape`] node of the tree this
    /// placement plants derives with in place of its own (#1505): an
    /// absolute placement's `seed`, when it names one. Scatter and grid
    /// placements have none, and draw each Shape node with its own.
    pub fn shape_seed(&self) -> Option<u64> {
        match self {
            Placement::Absolute { seed, .. } => *seed,
            _ => None,
        }
    }
}

impl Generator {
    /// This tree as an absolute placement with `seed` draws it (#1505): a
    /// copy with every [`GeneratorKind::Shape`] node's seed replaced by
    /// `seed`, and nothing else changed - an L-system, a particle system or
    /// the terrain keep their own. For the readers that take a tree rather
    /// than a placement (the agent's z-fighting check, the render tool's
    /// triangle count), so that what they read is what the world draws. The
    /// room compile does not copy the tree: it hands the placement's seed
    /// to the Shape spawner.
    pub fn with_shape_seed(&self, seed: u64) -> Generator {
        fn reseed(node: &mut Generator, seed: u64) {
            if let GeneratorKind::Shape { seed: own, .. } = &mut node.kind {
                *own = seed;
            }
            for child in &mut node.children {
                reseed(child, seed);
            }
        }
        let mut copy = self.clone();
        reseed(&mut copy, seed);
        copy
    }

    /// Whether any node of this tree is a [`GeneratorKind::Shape`] grammar -
    /// whether a placement's seed (#1505) varies anything it draws.
    pub fn draws_a_grammar(&self) -> bool {
        matches!(self.kind, GeneratorKind::Shape { .. })
            || self.children.iter().any(Generator::draws_a_grammar)
    }
}

#[cfg(test)]
mod placement_seed_wire_tests {
    //! Wire-format guards for an absolute placement's grammar seed (#1505).
    //! Every published room predates the field, so it rests on three
    //! things: a record that never set it keeps its bytes, a set one reads
    //! back as it was written, and a client built before it existed can
    //! still read a record that carries it.
    use super::*;

    /// `Placement` exactly as it was before #1505, attribute for attribute:
    /// what a client built before the field existed decodes a record with.
    mod before_1505 {
        use crate::pds::types::{
            BiomeFilter, Fp, Fp3, ScatterBounds, ScatterNaturalness, TransformData, default_true,
            is_false, is_true, u64_as_string,
        };
        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize, Clone, Debug)]
        #[serde(tag = "$type")]
        pub(super) enum Placement {
            #[serde(rename = "network.symbios.place.absolute")]
            Absolute {
                generator_ref: String,
                #[serde(default, skip_serializing_if = "TransformData::is_identity")]
                transform: TransformData,
                #[serde(default = "default_true", skip_serializing_if = "is_true")]
                snap_to_terrain: bool,
                #[serde(default, skip_serializing_if = "is_false")]
                avoid_water: bool,
                #[serde(default)]
                avoid_water_clearance: Fp,
            },
            #[serde(rename = "network.symbios.place.scatter")]
            Scatter {
                generator_ref: String,
                bounds: ScatterBounds,
                count: u32,
                #[serde(with = "u64_as_string")]
                local_seed: u64,
                #[serde(default, skip_serializing_if = "BiomeFilter::is_noop")]
                biome_filter: BiomeFilter,
                #[serde(default = "default_true", skip_serializing_if = "is_true")]
                snap_to_terrain: bool,
                #[serde(default = "default_true", skip_serializing_if = "is_true")]
                random_yaw: bool,
                #[serde(default, skip_serializing_if = "is_false")]
                avoid_urban: bool,
                #[serde(default, skip_serializing_if = "is_false")]
                float_on_water: bool,
                #[serde(default, skip_serializing_if = "ScatterNaturalness::is_noop")]
                naturalness: ScatterNaturalness,
            },
            #[serde(rename = "network.symbios.place.grid")]
            Grid {
                generator_ref: String,
                #[serde(default, skip_serializing_if = "TransformData::is_identity")]
                transform: TransformData,
                counts: [u32; 3],
                gaps: Fp3,
                #[serde(default = "default_true", skip_serializing_if = "is_true")]
                snap_to_terrain: bool,
                #[serde(default, skip_serializing_if = "is_false")]
                random_yaw: bool,
            },
            #[serde(other, skip_serializing)]
            Unknown,
        }
    }

    fn house(seed: Option<u64>) -> Placement {
        Placement::Absolute {
            generator_ref: "house".into(),
            transform: TransformData {
                translation: Fp3([12.5, 0.0, -3.0]),
                ..Default::default()
            },
            snap_to_terrain: true,
            avoid_water: false,
            avoid_water_clearance: Fp(0.0),
            seed,
        }
    }

    /// An absolute `placement` as the type before the field held it: the
    /// same values, field for field, and the seed left behind.
    fn as_before(placement: &Placement) -> Option<before_1505::Placement> {
        let Placement::Absolute {
            generator_ref,
            transform,
            snap_to_terrain,
            avoid_water,
            avoid_water_clearance,
            seed: _,
        } = placement.clone()
        else {
            return None;
        };
        Some(before_1505::Placement::Absolute {
            generator_ref,
            transform,
            snap_to_terrain,
            avoid_water,
            avoid_water_clearance,
        })
    }

    /// Unset, the key is not written and the bytes are the ones the record
    /// had before the field existed - checked against a literal, and for
    /// every absolute placement of twelve seeded rooms against what the
    /// placement type as it was before writes for the same values. A seeded
    /// settlement's grammar buildings carry their members' seeds (#1514):
    /// their bytes are the old ones with the key after them.
    #[test]
    fn an_unseeded_placement_keeps_the_bytes_it_had() {
        assert_eq!(
            serde_json::to_string(&house(None)).expect("serialises"),
            r#"{"$type":"network.symbios.place.absolute","generator_ref":"house","transform":{"translation":[125000,0,-30000]},"avoid_water_clearance":0}"#
        );
        let (mut unseeded, mut seeded) = (0, 0);
        for seed in 1..=12u64 {
            let room = crate::pds::RoomRecord::default_for_seed(seed, "did:plc:corpus");
            for placement in &room.placements {
                let Some(before) = as_before(placement) else {
                    continue;
                };
                let old = serde_json::to_string(&before).expect("serialises");
                let expected = match placement.shape_seed() {
                    None => {
                        unseeded += 1;
                        old
                    }
                    Some(own) => {
                        seeded += 1;
                        let fields = old.strip_suffix('}').expect("an object");
                        format!(r#"{fields},"seed":"{own}"}}"#)
                    }
                };
                assert_eq!(
                    serde_json::to_string(placement).expect("serialises"),
                    expected,
                    "seed {seed}: a placement's bytes moved"
                );
            }
        }
        assert!(
            unseeded > 12 && seeded > 0,
            "the corpus holds absolute placements, seeded and not: {unseeded} and {seeded}"
        );
    }

    /// Set, it is written as a quoted decimal after the fields that were
    /// there before, and reads back as it was - the largest seed too, which
    /// a JSON number could not carry exactly.
    #[test]
    fn a_seed_is_written_as_a_quoted_decimal_and_reads_back() {
        let wire = serde_json::to_value(house(Some(123))).expect("serialises");
        assert_eq!(wire["seed"], serde_json::json!("123"));
        assert_eq!(
            serde_json::to_string(&house(Some(123))).expect("serialises"),
            r#"{"$type":"network.symbios.place.absolute","generator_ref":"house","transform":{"translation":[125000,0,-30000]},"avoid_water_clearance":0,"seed":"123"}"#
        );
        for seed in [None, Some(0), Some(123), Some(u64::MAX)] {
            let back: Placement =
                serde_json::from_value(serde_json::to_value(house(seed)).expect("serialises"))
                    .expect("reads back");
            assert_eq!(back.shape_seed(), seed);
        }
        // A seed written by hand as null is no seed; as a number, refused -
        // a number past 2^53 would not survive the PDS's float hop.
        let mut null = wire.clone();
        null["seed"] = serde_json::Value::Null;
        let read: Placement = serde_json::from_value(null).expect("null reads");
        assert_eq!(read.shape_seed(), None);
        let mut number = wire;
        number["seed"] = serde_json::json!(123);
        assert!(serde_json::from_value::<Placement>(number).is_err());
    }

    /// A client built before the field existed reads a seeded placement -
    /// the key is ignored, so it draws the generator's own seed - rather
    /// than refusing the record.
    #[test]
    fn a_client_before_the_field_ignores_it() {
        let wire = serde_json::to_value(house(Some(u64::MAX))).expect("serialises");
        let before: before_1505::Placement =
            serde_json::from_value(wire).expect("the placement type before reads it");
        let before_1505::Placement::Absolute {
            generator_ref,
            transform,
            ..
        } = &before
        else {
            panic!("read as another variant: {before:?}");
        };
        assert_eq!(generator_ref, "house");
        assert_eq!(transform.translation.0, [12.5, 0.0, -3.0]);
        let written = serde_json::to_value(&before).expect("serialises");
        assert!(written.get("seed").is_none(), "{written}");
    }

    /// Scatter and grid placements carry no grammar seed.
    #[test]
    fn only_an_absolute_placement_has_a_grammar_seed() {
        let grid: Placement = serde_json::from_value(serde_json::json!({
            "$type": "network.symbios.place.grid",
            "generator_ref": "house",
            "counts": [2, 1, 2],
            "gaps": [10000, 10000, 10000],
            "seed": "7",
        }))
        .expect("a grid reads");
        assert_eq!(grid.shape_seed(), None);
        let written = serde_json::to_value(&grid).expect("serialises");
        assert!(written.get("seed").is_none(), "{written}");
    }

    /// Every Shape node in the tree takes the seed, however deep, and
    /// nothing else in it changes - an L-system's seed included.
    #[test]
    fn with_shape_seed_reseeds_every_shape_node_and_nothing_else() {
        let shape = |seed: &str| -> Generator {
            serde_json::from_value(serde_json::json!({
                "$type": "network.symbios.gen.shape",
                "grammar_source": "Lot --> I(\"Box\")",
                "root_rule": "Lot",
                "footprint": [10000, 0, 10000],
                "seed": seed,
            }))
            .expect("a Shape node")
        };
        let mut tree = shape("1");
        let mut post = Generator::default_cuboid();
        post.children.push(shape("2"));
        tree.children.push(post);
        tree.children.push(
            crate::catalogue::by_slug("lsys_dead_shrub")
                .expect("a stochastic L-system in the catalogue")
                .build(""),
        );
        let GeneratorKind::LSystem { seed: shrub, .. } = tree.children[1].kind else {
            panic!("the shrub is an L-system");
        };
        let reseeded = tree.with_shape_seed(99);
        let seeds = |g: &Generator| {
            let mut out = Vec::new();
            fn walk(g: &Generator, out: &mut Vec<(&'static str, u64)>) {
                match &g.kind {
                    GeneratorKind::Shape { seed, .. } => out.push(("shape", *seed)),
                    GeneratorKind::LSystem { seed, .. } => out.push(("lsystem", *seed)),
                    _ => {}
                }
                for child in &g.children {
                    walk(child, out);
                }
            }
            walk(g, &mut out);
            out
        };
        assert_ne!(
            shrub, 99,
            "the fixture's L-system seed differs from the new one"
        );
        assert_eq!(
            seeds(&reseeded),
            [("shape", 99), ("shape", 99), ("lsystem", shrub)]
        );
        let mut back = reseeded.clone();
        if let GeneratorKind::Shape { seed, .. } = &mut back.kind {
            *seed = 1;
        }
        if let GeneratorKind::Shape { seed, .. } = &mut back.children[0].children[0].kind {
            *seed = 2;
        }
        assert_eq!(back, tree, "nothing but the Shape seeds changed");
    }
}

#[cfg(test)]
mod scatter_naturalness_wire_tests {
    //! Wire-format guards for the placement-naturalness knobs (#912).
    //! Every published room record predates this field, so the whole
    //! feature rests on the default decoding to "behave exactly as
    //! before" and on an unset field never reaching the wire.
    use super::*;

    fn scatter(naturalness: ScatterNaturalness) -> Placement {
        Placement::Scatter {
            generator_ref: "tree".into(),
            bounds: ScatterBounds::default(),
            count: 40,
            local_seed: 9,
            biome_filter: BiomeFilter::default(),
            snap_to_terrain: true,
            random_yaw: true,
            avoid_urban: false,
            float_on_water: false,
            naturalness,
        }
    }

    /// `float_on_water` (#914) follows the same wire discipline as the
    /// naturalness block: absent on every published record (decoding to the
    /// historical terrain-snap), and elided while false so the seeded
    /// scatters don't bloat.
    #[test]
    fn float_on_water_defaults_false_and_is_elided() {
        let old = serde_json::json!({
            "$type": "network.symbios.place.scatter",
            "generator_ref": "tree",
            "bounds": { "type": "circle", "center": [0, 0], "radius": 640_000 },
            "count": 40,
            "local_seed": "9",
        });
        let p: Placement = serde_json::from_value(old).expect("old scatter parses");
        let Placement::Scatter { float_on_water, .. } = &p else {
            panic!("variant changed");
        };
        assert!(!float_on_water, "published records must not float");

        let plain = serde_json::to_value(scatter(ScatterNaturalness::default())).unwrap();
        assert!(
            !plain.as_object().unwrap().contains_key("float_on_water"),
            "a false flag must not reach the wire"
        );

        let mut floating = scatter(ScatterNaturalness::default());
        let Placement::Scatter { float_on_water, .. } = &mut floating else {
            panic!("variant changed");
        };
        *float_on_water = true;
        let v = serde_json::to_value(&floating).unwrap();
        assert_eq!(v.get("float_on_water"), Some(&serde_json::json!(true)));
        let re: Placement = serde_json::from_value(v).expect("reparses");
        let Placement::Scatter { float_on_water, .. } = re else {
            panic!("variant changed");
        };
        assert!(float_on_water, "a set flag must round-trip");
    }

    #[test]
    fn a_record_predating_the_field_decodes_as_the_old_behaviour() {
        // Byte-for-byte what an already-published scatter looks like.
        let old = serde_json::json!({
            "$type": "network.symbios.place.scatter",
            "generator_ref": "tree",
            // `Fp` is fixed-point on the wire: i32 scaled by FP_SCALE.
            "bounds": { "type": "circle", "center": [0, 0], "radius": 640_000 },
            "count": 40,
            "local_seed": "9",
        });
        let p: Placement = serde_json::from_value(old).expect("old scatter parses");
        let Placement::Scatter { naturalness, .. } = &p else {
            panic!("variant changed");
        };
        assert!(
            naturalness.is_noop(),
            "a record with no naturalness block must sample flat-uniform"
        );
        assert!(
            naturalness.max_slope_deg.is_none(),
            "and impose no slope limit"
        );
    }

    #[test]
    fn an_unset_block_is_elided_and_a_set_one_round_trips() {
        let plain = serde_json::to_value(scatter(ScatterNaturalness::default())).unwrap();
        assert!(
            !plain.as_object().unwrap().contains_key("naturalness"),
            "the default block must not bloat every scatter on the wire"
        );

        let tuned = ScatterNaturalness {
            clumping: Fp(0.55),
            edge_falloff: Fp(1.2),
            scale_jitter: Fp(0.2),
            tilt_jitter: Fp(0.15),
            max_slope_deg: Some(Fp(36.0)),
            above_water_band: Some(Fp2([0.0, 3.5])),
            altitude_band: Some(Fp2([12.0, 90.0])),
        };
        let v = serde_json::to_value(scatter(tuned)).unwrap();
        let block = v
            .get("naturalness")
            .expect("authored knobs stay on the wire");
        assert!(block.get("clumping").is_some() && block.get("max_slope_deg").is_some());

        let re: Placement = serde_json::from_value(v).expect("reparses");
        let Placement::Scatter { naturalness, .. } = re else {
            panic!("variant changed");
        };
        assert_eq!(naturalness, tuned);
    }

    /// Per-knob elision: a scatter that only wants a slope limit should
    /// not carry four zeroes alongside it.
    #[test]
    fn only_the_knobs_that_are_set_reach_the_wire() {
        let v = serde_json::to_value(scatter(ScatterNaturalness {
            max_slope_deg: Some(Fp(30.0)),
            ..ScatterNaturalness::default()
        }))
        .unwrap();
        let block = v.get("naturalness").unwrap().as_object().unwrap();
        assert_eq!(block.len(), 1, "expected only max_slope_deg, got {block:?}");
    }

    /// The sanitiser is the record boundary - a hostile or corrupt block
    /// must not reach a transform. NaN matters specifically: `f32::clamp`
    /// propagates it, so a naive clamp would let one straight through.
    #[test]
    fn sanitize_clamps_every_knob_and_rejects_nan() {
        let mut n = ScatterNaturalness {
            clumping: Fp(50.0),
            edge_falloff: Fp(-3.0),
            scale_jitter: Fp(f32::NAN),
            tilt_jitter: Fp(f32::INFINITY),
            max_slope_deg: Some(Fp(4000.0)),
            // Inverted and non-finite bands: a band that accepts nothing
            // would silently empty the scatter rather than error.
            above_water_band: Some(Fp2([9.0, 2.0])),
            altitude_band: Some(Fp2([f32::NAN, 50.0])),
        };
        n.sanitize();
        assert_eq!(n.clumping, Fp(0.95), "a full collapse is never allowed");
        assert_eq!(n.edge_falloff, Fp(0.0));
        assert_eq!(n.scale_jitter, Fp(0.0), "NaN must not survive the clamp");
        assert_eq!(n.tilt_jitter, Fp(0.0));
        assert_eq!(n.max_slope_deg, Some(Fp(90.0)));
        assert_eq!(
            n.above_water_band,
            Some(Fp2([2.0, 9.0])),
            "an inverted band is normalised, not left to match nothing"
        );
        assert_eq!(
            n.altitude_band,
            Some(Fp2([-10_000.0, 50.0])),
            "a non-finite end collapses to the range bound"
        );
        // Idempotent: sanitising an already-clean block changes nothing.
        let mut again = n;
        again.sanitize();
        assert_eq!(again, n);
    }
}

#[cfg(test)]
mod prim_wire_tests {
    //! Wire-format guards for the organic-prim additions (#688): the new
    //! `TortureParams` knobs must default cleanly on records that predate
    //! them, and the Superellipsoid variant must round-trip with its tag.
    use super::*;

    #[test]
    fn torture_params_predating_new_knobs_default_to_identity() {
        // A pre-#688 torture block carries no `taper_bottom` / `bulge` keys -
        // and since #695 the eliding wire format omits every identity knob
        // anyway, so serializing a twist+taper-only value produces exactly
        // the shape an already-published record carries.
        let current = TortureParams {
            twist: Fp(0.5),
            taper: Fp2([0.2, 0.2]),
            ..Default::default()
        };
        let old = serde_json::to_value(current).expect("serialises");
        let obj = old.as_object().expect("one flat JSON object");
        assert!(obj.contains_key("twist"), "authored knobs stay on the wire");
        assert!(obj.contains_key("taper"), "authored knobs stay on the wire");
        assert!(
            !obj.contains_key("taper_bottom") && !obj.contains_key("bulge"),
            "identity knobs are elided (#695)"
        );

        let t: TortureParams = serde_json::from_value(old).expect("old torture block parses");
        assert_eq!(t.taper_bottom, Fp2([0.0, 0.0]));
        assert_eq!(t.bulge, Fp2([0.0, 0.0]));
        assert_eq!(t.taper, Fp2([0.2, 0.2]), "existing knobs still decode");
        assert!(!t.deforms_are_identity() && t.cuts_are_identity());

        // Round trip lands on the same value.
        let re: TortureParams = serde_json::from_value(serde_json::to_value(t).unwrap()).unwrap();
        assert_eq!(re, t);
    }

    #[test]
    fn spine_and_lathe_wire_format_round_trips() {
        for (tag, wire) in [
            ("Spine", "network.symbios.gen.spine"),
            ("Lathe", "network.symbios.gen.lathe"),
        ] {
            let kind = GeneratorKind::default_primitive_for_tag(tag).unwrap();
            let v = serde_json::to_value(&kind).expect("serialises");
            let obj = v.as_object().expect("one flat JSON object");
            assert_eq!(obj.get("$type").and_then(|t| t.as_str()), Some(wire));
            assert!(
                obj.get("points").is_some_and(|p| p.is_array()),
                "{tag} points stay an inline array"
            );
            let re: GeneratorKind = serde_json::from_value(v).expect("reparses");
            assert_eq!(re, kind);
        }
    }

    #[test]
    fn blob_group_wire_format_round_trips() {
        let kind = GeneratorKind::default_primitive_for_tag("BlobGroup").unwrap();
        let v = serde_json::to_value(&kind).expect("serialises");
        let obj = v.as_object().expect("one flat JSON object");
        assert_eq!(
            obj.get("$type").and_then(|t| t.as_str()),
            Some("network.symbios.gen.blob_group")
        );
        let elements = obj.get("elements").and_then(|e| e.as_array()).unwrap();
        assert_eq!(
            elements[0]
                .get("shape")
                .and_then(|s| s.get("$type"))
                .and_then(|t| t.as_str()),
            Some("network.symbios.blob.sphere"),
            "element shape is its own open union"
        );
        let re: GeneratorKind = serde_json::from_value(v).expect("reparses");
        assert_eq!(re, kind);

        // Every known shape round-trips through its own tag (#725 grew the
        // union past the original sphere/capsule/ellipsoid trio).
        for (shape, wire) in [
            (BlobShape::Sphere, "network.symbios.blob.sphere"),
            (BlobShape::Capsule, "network.symbios.blob.capsule"),
            (BlobShape::Ellipsoid, "network.symbios.blob.ellipsoid"),
            (BlobShape::Box, "network.symbios.blob.box"),
            (BlobShape::Cylinder, "network.symbios.blob.cylinder"),
            (BlobShape::Torus, "network.symbios.blob.torus"),
            (BlobShape::Cone, "network.symbios.blob.cone"),
        ] {
            let sv = serde_json::to_value(shape).expect("shape serialises");
            assert_eq!(sv.get("$type").and_then(|t| t.as_str()), Some(wire));
            let rs: BlobShape = serde_json::from_value(sv).expect("shape reparses");
            assert_eq!(rs, shape);
        }

        // Forward compat: an unknown element shape degrades to Unknown, not
        // a parse failure.
        let mut v2 = serde_json::to_value(&kind).unwrap();
        v2["elements"][0]["shape"]["$type"] = serde_json::json!("network.symbios.blob.hyperboloid");
        let re2: GeneratorKind = serde_json::from_value(v2).expect("future shape still parses");
        let GeneratorKind::BlobGroup { elements, .. } = &re2 else {
            panic!("wrong variant");
        };
        assert_eq!(elements[0].shape, BlobShape::Unknown);
    }

    /// The `uv_mapping` knob (#739) is default-elided on the wire, every
    /// known mode round-trips through its own tag, and an unrecognised
    /// mode tag degrades to `Unknown` instead of failing the record.
    /// #937: the flat-faced kinds carry `uv_mapping` too, and `Plane`
    /// defaults to `Fit` rather than the enum-wide `Box` - a card must span
    /// its quad once, so a field-less Plane record has to keep meaning
    /// "card", not silently start tiling.
    #[test]
    fn per_prim_uv_mapping_defaults_and_elision() {
        let json = |k: &GeneratorKind| serde_json::to_value(k).expect("serialize");

        // Cuboid: Box is the default, so it elides.
        let cuboid = GeneratorKind::default_primitive_for_tag("Cuboid").unwrap();
        assert!(cuboid.uv_mapping() == Some(UvMapping::Box));
        assert!(
            json(&cuboid).get("uv_mapping").is_none(),
            "a Box cuboid must keep the field off the wire"
        );

        // Plane: Fit is its default, and elides against *that*.
        let plane = GeneratorKind::default_primitive_for_tag("Plane").unwrap();
        assert!(
            plane.uv_mapping() == Some(UvMapping::Fit),
            "Plane must default to Fit - it is the alpha-card carrier"
        );
        assert!(
            json(&plane).get("uv_mapping").is_none(),
            "a Fit plane must keep the field off the wire"
        );

        // A field-less Plane from an older record still deserialises to Fit.
        let mut bare = json(&plane);
        assert!(bare.get("uv_mapping").is_none());
        let round: GeneratorKind = serde_json::from_value(bare.clone()).expect("deserialize");
        assert!(round.uv_mapping() == Some(UvMapping::Fit));

        // An explicit non-default choice serialises its tag and survives.
        bare["uv_mapping"] = serde_json::json!({ "$type": "network.symbios.uv.planar_y" });
        let round: GeneratorKind = serde_json::from_value(bare).expect("deserialize");
        assert!(round.uv_mapping() == Some(UvMapping::PlanarY));

        // And a mode from a newer client degrades to Unknown, which meshes
        // as the default rather than failing the record.
        let mut future = json(&cuboid);
        future["uv_mapping"] = serde_json::json!({ "$type": "network.symbios.uv.conformal" });
        let round: GeneratorKind = serde_json::from_value(future).expect("deserialize");
        assert!(round.uv_mapping() == Some(UvMapping::Unknown));
    }

    #[test]
    fn blob_group_uv_mapping_wire_format() {
        // The default mode (Box since #742) stays off the wire - a
        // field-less record and a freshly-built default serialise
        // identically.
        assert_eq!(
            UvMapping::default(),
            UvMapping::Box,
            "#742 flipped the blob UV default to Box"
        );
        let kind = GeneratorKind::default_primitive_for_tag("BlobGroup").unwrap();
        let v = serde_json::to_value(&kind).unwrap();
        assert!(
            v.get("uv_mapping").is_none(),
            "default uv_mapping must be elided"
        );

        for (mode, wire) in [
            (UvMapping::Spherical, "network.symbios.uv.spherical"),
            (UvMapping::Box, "network.symbios.uv.box"),
            (UvMapping::Cylindrical, "network.symbios.uv.cylindrical"),
            (UvMapping::PlanarX, "network.symbios.uv.planar_x"),
            (UvMapping::PlanarY, "network.symbios.uv.planar_y"),
            (UvMapping::PlanarZ, "network.symbios.uv.planar_z"),
        ] {
            let sv = serde_json::to_value(mode).expect("mode serialises");
            assert_eq!(sv.get("$type").and_then(|t| t.as_str()), Some(wire));
            let rm: UvMapping = serde_json::from_value(sv).expect("mode reparses");
            assert_eq!(rm, mode);
        }

        // A non-default mode survives a full generator round trip - since
        // #742 that includes Spherical, which must now serialise its tag
        // explicitly to keep rendering spherically.
        let GeneratorKind::BlobGroup {
            elements,
            resolution,
            common:
                PrimCommon {
                    solid,
                    material,
                    torture,
                    ..
                },
            ..
        } = kind
        else {
            panic!("wrong variant");
        };
        let kind = GeneratorKind::BlobGroup {
            elements,
            resolution,
            common: PrimCommon {
                solid,
                uv_mapping: Some(UvMapping::Spherical),
                material,
                torture,
                ..Default::default()
            },
        };
        let v = serde_json::to_value(&kind).unwrap();
        assert_eq!(
            v["uv_mapping"]["$type"].as_str(),
            Some("network.symbios.uv.spherical")
        );
        let re: GeneratorKind = serde_json::from_value(v.clone()).expect("reparses");
        assert_eq!(re, kind);

        // Forward compat: a future mode tag degrades to Unknown.
        let mut v3 = v;
        v3["uv_mapping"]["$type"] = serde_json::json!("network.symbios.uv.conformal");
        let re3: GeneratorKind = serde_json::from_value(v3).expect("future mode still parses");
        let GeneratorKind::BlobGroup {
            common: PrimCommon { uv_mapping, .. },
            ..
        } = re3
        else {
            panic!("wrong variant");
        };
        assert_eq!(uv_mapping, Some(UvMapping::Unknown));
    }

    #[test]
    fn superellipsoid_wire_format_round_trips() {
        let kind = GeneratorKind::default_primitive_for_tag("Superellipsoid").unwrap();
        let v = serde_json::to_value(&kind).expect("serialises");
        let obj = v.as_object().expect("one flat JSON object");
        assert_eq!(
            obj.get("$type").and_then(|t| t.as_str()),
            Some("network.symbios.gen.superellipsoid")
        );
        assert!(obj.contains_key("half_extents"), "fields stay inline");
        assert!(obj.contains_key("exponent_ns"));

        let re: GeneratorKind = serde_json::from_value(v).expect("reparses");
        assert_eq!(re, kind);
    }
}

#[cfg(test)]
mod face_override_tests {
    //! Wire-format guards for the per-face override additions (#955): the
    //! `faces` / revolved-`uv_mapping` fields must elide when default (so a
    //! pre-#955 record and its re-serialisation are byte-compatible), face
    //! keys must carry their namespaced tags, and a key minted by a newer
    //! client must decode dormant rather than fail the record.
    use super::*;

    fn overridden_cuboid() -> GeneratorKind {
        let mut kind = GeneratorKind::default_cuboid();
        kind.faces_mut().expect("cuboid is a primitive").extend([
            FaceOverride {
                face: FaceKey::SidePz,
                material: SovereignMaterialSettings {
                    base_color: Fp3([0.1, 0.9, 0.2]),
                    ..Default::default()
                },
                uv_mapping: Some(UvMapping::PlanarZ),
            },
            FaceOverride {
                face: FaceKey::Top,
                material: SovereignMaterialSettings::default(),
                uv_mapping: None,
            },
        ]);
        kind
    }

    #[test]
    fn face_overrides_round_trip() {
        let kind = overridden_cuboid();
        let v = serde_json::to_value(&kind).expect("serialises");
        let re: GeneratorKind = serde_json::from_value(v).expect("reparses");
        assert_eq!(re, kind);
    }

    #[test]
    fn face_key_wire_tags_are_namespaced() {
        let v = serde_json::to_value(FaceKey::SidePx).expect("serialises");
        assert_eq!(v["$type"].as_str(), Some("network.symbios.face.side_px"));
        let v = serde_json::to_value(FaceKey::ProfileCutEnd).expect("serialises");
        assert_eq!(
            v["$type"].as_str(),
            Some("network.symbios.face.profile_cut_end")
        );
    }

    #[test]
    fn unknown_face_key_decodes_dormant() {
        let re: FaceKey = serde_json::from_value(serde_json::json!({
            "$type": "network.symbios.face.hologram_shimmer"
        }))
        .expect("open union tolerates future keys");
        assert_eq!(re, FaceKey::Unknown);
    }

    /// #1552: `avoid_water` is off the wire while off, so every network
    /// saved before the field writes back byte-identical, and on it is
    /// written and read back. The record form is the generator's, as
    /// `room set` takes it.
    #[test]
    fn a_road_networks_water_switch_elides_off_and_round_trips_on() {
        let plain = GeneratorKind::RoadNetwork(RoadConfig::default());
        let v = serde_json::to_value(&plain).expect("serialises");
        assert!(
            v.as_object().expect("object").get("avoid_water").is_none(),
            "the switch off must stay off the wire: {v}"
        );

        let shore = GeneratorKind::RoadNetwork(RoadConfig {
            avoid_water: true,
            ..RoadConfig::default()
        });
        let v = serde_json::to_value(&shore).expect("serialises");
        assert_eq!(v["avoid_water"], serde_json::json!(true), "{v}");
        let back: GeneratorKind = serde_json::from_value(v).expect("reads back");
        assert_eq!(back, shore);

        let old: GeneratorKind = serde_json::from_value(serde_json::json!({
            "$type": "network.symbios.gen.road_network",
            "seed": "7"
        }))
        .expect("a network saved before the field reads");
        let GeneratorKind::RoadNetwork(old) = old else {
            panic!("a road network reads as one");
        };
        assert!(!old.avoid_water);
    }

    #[test]
    fn faceless_prim_elides_and_legacy_records_decode() {
        // Elision (#695): a prim with no overrides keeps `faces` off the wire.
        let v = serde_json::to_value(GeneratorKind::default_cuboid()).expect("serialises");
        assert!(v.as_object().expect("object").get("faces").is_none());

        // A default revolved prim serialises with neither `faces` nor
        // `uv_mapping` - which makes its wire shape *identical* to a
        // pre-#955 record's, so parsing it back exercises the legacy path…
        let sphere = GeneratorKind::default_primitive_for_tag("Sphere").expect("sphere");
        let legacy_wire = serde_json::to_value(&sphere).expect("serialises");
        let obj = legacy_wire.as_object().expect("object");
        assert!(
            !obj.contains_key("faces") && !obj.contains_key("uv_mapping"),
            "default revolved prim must elide both #955 fields: {obj:?}"
        );

        // …decoding to empty overrides and the Fit (mesher-native)
        // projection, byte-compatible across a load/save cycle.
        let legacy: GeneratorKind =
            serde_json::from_value(legacy_wire).expect("legacy sphere decodes");
        assert_eq!(legacy.faces().map(<[FaceOverride]>::len), Some(0));
        assert_eq!(legacy.uv_mapping(), Some(UvMapping::Fit));
        assert_eq!(legacy, sphere);
    }

    #[test]
    fn default_material_override_elides_its_material() {
        let v = serde_json::to_value(overridden_cuboid()).expect("serialises");
        let faces = v["faces"].as_array().expect("faces array");
        assert_eq!(faces.len(), 2);
        // The Top override carries only its face key - its default material
        // and inherit-mapping stay off the wire.
        assert!(faces[1].get("material").is_none());
        assert!(faces[1].get("uv_mapping").is_none());
        // The painted face keeps both.
        assert!(faces[0].get("material").is_some());
        assert!(faces[0].get("uv_mapping").is_some());
    }
}

#[cfg(test)]
mod lot_settings_wire_tests {
    use super::*;

    fn network(lots: LotSettings) -> serde_json::Value {
        serde_json::to_value(GeneratorKind::RoadNetwork(RoadConfig {
            lots,
            ..RoadConfig::default()
        }))
        .expect("a road network serialises")
    }

    /// #1555: the two socio overrides stay off the wire while `None`, so a
    /// network saved before they existed writes back byte-identical - an
    /// untouched `lots` is still elided whole, and a touched one carries
    /// exactly the five fields it always did.
    #[test]
    fn lot_overrides_stay_off_the_wire_until_armed() {
        assert!(
            network(LotSettings::default()).get("lots").is_none(),
            "untouched lot settings stay elided"
        );
        let touched = network(LotSettings {
            density: Fp(0.5),
            ..LotSettings::default()
        });
        let mut keys: Vec<&str> = touched["lots"]
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "density",
                "scale_max",
                "scale_min",
                "theme_override",
                "tier_bias"
            ],
            "{touched}"
        );
    }

    /// #1553, #1555: the fit and the lot area stay off the wire at their
    /// defaults (the key set above), and set they are written in the
    /// record's own form and read back.
    #[test]
    fn the_fit_and_the_lot_area_round_trip_when_set() {
        let lots = LotSettings {
            fit: true,
            lot_area: Fp(2400.0),
            focus: Some(Fp2([12.5, -260.0])),
            ..LotSettings::default()
        };
        let v = network(lots.clone());
        assert_eq!(v["lots"]["fit"], serde_json::json!(true), "{v}");
        assert_eq!(
            v["lots"]["focus"],
            serde_json::json!([125_000, -2_600_000]),
            "{v}"
        );
        assert_eq!(v["lots"]["lot_area"], serde_json::json!(24_000_000), "{v}");
        let back: GeneratorKind = serde_json::from_value(v).expect("reads back");
        let GeneratorKind::RoadNetwork(back) = back else {
            panic!("a road network reads as one");
        };
        assert_eq!(back.lots, lots);
        let old: LotSettings =
            serde_json::from_value(serde_json::json!({ "density": 5000 })).expect("reads");
        assert!(
            !old.fit,
            "a block written before the fit grows at catalogue size"
        );
        assert_eq!(old.lot_area.0, LotSettings::DEFAULT_LOT_AREA);
    }

    /// #1555: armed overrides are written in the record's fixed point and
    /// read back; a peaceful override of zero is written, not elided - it
    /// is a value, not an absence.
    #[test]
    fn lot_overrides_and_the_downtown_mix_round_trip() {
        let lots = LotSettings {
            theme_override: String::from("Cyberpunk"),
            tier_bias: LotTierBias::Downtown,
            prosperity: Some(Fp(0.9)),
            escalation: Some(Fp(0.0)),
            ..LotSettings::default()
        };
        let v = network(lots.clone());
        assert_eq!(v["lots"]["prosperity"], serde_json::json!(9000), "{v}");
        assert_eq!(v["lots"]["escalation"], serde_json::json!(0), "{v}");
        assert_eq!(
            v["lots"]["tier_bias"],
            serde_json::json!({ "$type": "network.symbios.lot_bias.downtown" }),
            "{v}"
        );
        let back: GeneratorKind = serde_json::from_value(v).expect("reads back");
        let GeneratorKind::RoadNetwork(back) = back else {
            panic!("a road network reads as one");
        };
        assert_eq!(back.lots, lots);

        // The form the brief's trial record is written in.
        let authored: LotSettings = serde_json::from_value(serde_json::json!({
            "theme_override": "Cyberpunk",
            "tier_bias": { "$type": "network.symbios.lot_bias.downtown" },
            "escalation": 0,
            "prosperity": 9000
        }))
        .expect("an authored lots block reads");
        assert_eq!(authored, lots);
    }

    /// A network saved before #1555 reads with no overrides (the room's own
    /// scene), and a mix from a newer build reads as `Unknown`, which the
    /// lot layer grows as Balanced.
    #[test]
    fn older_and_newer_lot_settings_read() {
        let old: LotSettings =
            serde_json::from_value(serde_json::json!({ "theme_override": "Cyberpunk" }))
                .expect("a pre-#1555 lots block reads");
        assert_eq!(old.prosperity, None);
        assert_eq!(old.escalation, None);
        assert_eq!(old.tier_bias, LotTierBias::Balanced);

        let newer: LotTierBias = serde_json::from_value(
            serde_json::json!({ "$type": "network.symbios.lot_bias.arcology" }),
        )
        .expect("an unknown mix reads");
        assert_eq!(newer, LotTierBias::Unknown);
    }

    /// The editor's Mix row lists the new mix with the rest (#1555).
    #[test]
    fn the_mix_picker_offers_downtown() {
        let rows = LotTierBias::pickers();
        assert!(
            rows.iter()
                .any(|(value, label, tip)| *value == LotTierBias::Downtown
                    && *label == "Downtown"
                    && !tip.is_empty()),
            "no Downtown row"
        );
        assert!(
            rows.iter()
                .all(|(value, ..)| *value != LotTierBias::Unknown),
            "Unknown is never offered"
        );
    }

    /// #1555: an override reads clamped, a non-finite one as the neutral
    /// value the sanitiser writes, and `None` as the scene's own value.
    #[test]
    fn an_override_reads_in_place_of_the_scene() {
        let none = LotSettings::default();
        assert_eq!(none.prosperity_or(0.89), 0.89);
        assert_eq!(none.escalation_or(0.73), 0.73);
        let armed = LotSettings {
            prosperity: Some(Fp(1.7)),
            escalation: Some(Fp(-0.2)),
            ..LotSettings::default()
        };
        assert_eq!(armed.prosperity_or(0.2), 1.0);
        assert_eq!(armed.escalation_or(0.73), 0.0);
        let broken = LotSettings {
            prosperity: Some(Fp(f32::NAN)),
            escalation: Some(Fp(f32::INFINITY)),
            ..LotSettings::default()
        };
        assert_eq!(broken.prosperity_or(0.2), LotSettings::NEUTRAL_PROSPERITY);
        assert_eq!(broken.escalation_or(0.73), LotSettings::NEUTRAL_ESCALATION);
    }
}

#[cfg(test)]
mod particle_params_tests {
    //! Wire-format guards for the #648 `ParticleSystem` boxed-params
    //! refactor: the internally-tagged enum must keep serialising the
    //! params inline beside `$type`, exactly like the old struct variant,
    //! so already-published records round-trip unchanged.
    use super::*;

    #[test]
    fn particle_params_wire_format_is_inline() {
        // Author a few non-default fields so they must appear on the wire -
        // since #695 default-valued params are elided, so the all-defaults
        // emitter serializes as just its `$type` tag.
        let kind = GeneratorKind::ParticleSystem(Box::new(ParticleParams {
            rate_per_second: Fp(64.0),
            burst_count: 3,
            seed: 7,
            ..Default::default()
        }));
        let v = serde_json::to_value(&kind).expect("serialises");
        let obj = v.as_object().expect("one flat JSON object");
        // Tag + fields side by side - no nested params wrapper key.
        assert_eq!(
            obj.get("$type").and_then(|t| t.as_str()),
            Some("network.symbios.gen.particles")
        );
        assert!(obj.contains_key("rate_per_second"), "fields stay inline");
        assert!(obj.contains_key("burst_count"));
        assert!(
            obj.get("seed").is_some_and(|s| s.is_string()),
            "seed keeps its string encoding"
        );
        assert!(
            !obj.contains_key("emitter_shape"),
            "default-valued params are elided (#695)"
        );
        assert!(
            !obj.values().any(|x| {
                x.as_object()
                    .is_some_and(|inner| inner.contains_key("rate_per_second"))
            }),
            "no boxed-struct wrapper object appeared"
        );
        // And the elided form round-trips to the authored value.
        let re: GeneratorKind = serde_json::from_value(v).expect("reparses");
        assert_eq!(re, kind);
    }

    #[test]
    fn particle_params_old_format_record_round_trips() {
        // A pre-#648 record fragment: struct-variant inline fields, no
        // optional texture keys (their serde defaults must fill in).
        let old = serde_json::json!({
            "$type": "network.symbios.gen.particles",
            "emitter_shape": serde_json::to_value(EmitterShape::Point).unwrap(),
            "rate_per_second": serde_json::to_value(Fp(8.0)).unwrap(),
            "burst_count": 3,
            "max_particles": 64,
            "looping": true,
            "duration": serde_json::to_value(Fp(2.0)).unwrap(),
            "lifetime_min": serde_json::to_value(Fp(0.5)).unwrap(),
            "lifetime_max": serde_json::to_value(Fp(1.5)).unwrap(),
            "speed_min": serde_json::to_value(Fp(1.0)).unwrap(),
            "speed_max": serde_json::to_value(Fp(2.0)).unwrap(),
            "gravity_multiplier": serde_json::to_value(Fp(0.0)).unwrap(),
            "acceleration": serde_json::to_value(Fp3([0.0, 0.0, 0.0])).unwrap(),
            "linear_drag": serde_json::to_value(Fp(0.1)).unwrap(),
            "start_size": serde_json::to_value(Fp(0.2)).unwrap(),
            "end_size": serde_json::to_value(Fp(0.0)).unwrap(),
            "start_color": serde_json::to_value(Fp4([1.0, 1.0, 1.0, 1.0])).unwrap(),
            "end_color": serde_json::to_value(Fp4([1.0, 1.0, 1.0, 0.0])).unwrap(),
            "blend_mode": serde_json::to_value(ParticleBlendMode::Alpha).unwrap(),
            "billboard": true,
            "simulation_space": serde_json::to_value(SimulationSpace::World).unwrap(),
            "inherit_velocity": serde_json::to_value(Fp(0.0)).unwrap(),
            "collide_terrain": false,
            "collide_water": false,
            "collide_colliders": false,
            "bounce": serde_json::to_value(Fp(0.3)).unwrap(),
            "friction": serde_json::to_value(Fp(0.5)).unwrap(),
            "seed": "42",
        });
        let kind: GeneratorKind = serde_json::from_value(old).expect("old record parses");
        let GeneratorKind::ParticleSystem(p) = &kind else {
            panic!("wrong variant");
        };
        assert_eq!(p.seed, 42);
        assert_eq!(p.burst_count, 3);
        assert!(p.texture.is_none(), "missing optional fields default");

        // Round trip: serialise + reparse lands on the same value.
        let re: GeneratorKind =
            serde_json::from_value(serde_json::to_value(&kind).unwrap()).unwrap();
        assert_eq!(re, kind);
    }
}

#[cfg(test)]
mod layout_revision_tests {
    use super::*;

    /// #1558: a network on the original street plan writes no
    /// `layout_revision` (every network saved before it writes back
    /// byte-identical), and an upgraded one writes it as a plain number and
    /// reads it back.
    #[test]
    fn the_layout_revision_stays_off_the_wire_until_it_is_raised() {
        assert_eq!(
            serde_json::to_string(&RoadConfig::default()).expect("writes"),
            "{}"
        );
        let upgraded = RoadConfig {
            layout_revision: RoadConfig::LATEST_LAYOUT,
            ..RoadConfig::default()
        };
        let wire = serde_json::to_string(&upgraded).expect("writes");
        assert_eq!(wire, r#"{"layout_revision":2}"#);
        let back: RoadConfig = serde_json::from_str(&wire).expect("reads");
        assert_eq!(back, upgraded);
        let old: RoadConfig = serde_json::from_str(r#"{"seed":"7"}"#).expect("reads");
        assert_eq!(
            old.layout_revision, 0,
            "a network without it is on the original plan"
        );
        assert!(!old.tidies_layout() && upgraded.tidies_layout());
        // #1563: only revision 2 and later derive with portable maths.
        let at = |layout_revision: u32| RoadConfig {
            layout_revision,
            ..RoadConfig::default()
        };
        assert_eq!(
            [0, 1, 2].map(|r| at(r).portable_math()),
            [false, false, true]
        );
        assert_eq!(at(2).math_mode(), symbios_tensor::MathMode::Portable);
        assert_eq!(at(1).math_mode(), symbios_tensor::MathMode::Platform);
    }
}

#[cfg(test)]
mod road_field_wire_tests {
    use super::*;
    use serde_json::json;

    /// A default network as this build wrote it before the street field
    /// existed (#1556), captured from that build.
    const PLAIN_BEFORE: &str = r#"{"$type":"network.symbios.gen.road_network"}"#;

    /// [`populated`] as this build wrote it before the street field existed
    /// (#1556), captured from that build.
    const POPULATED_BEFORE: &str = concat!(
        r#"{"$type":"network.symbios.gen.road_network","enabled":false,"#,
        r#""seed":"12345678901234567890","district_half_extent":2200000,"#,
        r#""center":[400000,-255000],"style":{"$type":"network.symbios.road_style.organic"},"#,
        r#""avoid_water":true,"appearance":{"deck_color":[1000,2000,3000],"#,
        r#""deck_roughness":4000,"structure_color":null,"neon_color":null,"#,
        r#""neon_strength":30000},"lots":{"density":6000,"theme_override":"Cyberpunk","#,
        r#""tier_bias":{"$type":"network.symbios.lot_bias.downtown"},"scale_min":7000,"#,
        r#""scale_max":15000,"fit":true,"lot_area":24000000,"focus":[100000,200000],"#,
        r#""prosperity":8000,"escalation":0},"furniture":{"enabled":true,"spacing":250000},"#,
        r#""major_spacing":700000,"minor_spacing":350000,"major_half_width":40000,"#,
        r#""minor_half_width":25000,"curb_height":2000,"curb_top_width":3000,"#,
        r#""chamfer_width":5000,"skirt_depth":60000,"populate_lots":false}"#,
    );

    /// A network with every field but the street field set off its default.
    fn populated() -> RoadConfig {
        RoadConfig {
            enabled: false,
            seed: 12_345_678_901_234_567_890,
            district_half_extent: Fp(220.0),
            center: Fp2([40.0, -25.5]),
            style: RoadStyle::Organic,
            avoid_water: true,
            appearance: RoadAppearance {
                deck_color: Some(Fp3([0.1, 0.2, 0.3])),
                deck_roughness: Some(Fp(0.4)),
                structure_color: None,
                neon_color: None,
                neon_strength: Some(Fp(3.0)),
            },
            lots: LotSettings {
                density: Fp(0.6),
                theme_override: String::from("Cyberpunk"),
                tier_bias: LotTierBias::Downtown,
                scale_min: Fp(0.7),
                scale_max: Fp(1.5),
                fit: true,
                lot_area: Fp(2400.0),
                focus: Some(Fp2([10.0, 20.0])),
                prosperity: Some(Fp(0.8)),
                escalation: Some(Fp(0.0)),
            },
            furniture: FurnitureSettings {
                enabled: true,
                spacing: Fp(25.0),
            },
            major_spacing: Fp(70.0),
            minor_spacing: Fp(35.0),
            major_half_width: Fp(4.0),
            minor_half_width: Fp(2.5),
            curb_height: Fp(0.2),
            curb_top_width: Fp(0.3),
            chamfer_width: Fp(0.5),
            skirt_depth: Fp(6.0),
            populate_lots: false,
            ..RoadConfig::default()
        }
    }

    /// #1556: an untouched street field stays off the wire, so every network
    /// saved before it writes back byte-identical - a default network, and
    /// one with every other field set, are written exactly as this build
    /// wrote them before the field existed, and those bytes read back as
    /// the same network.
    #[test]
    fn an_untouched_street_field_leaves_a_network_as_it_was_written() {
        assert_eq!(
            serde_json::to_string(&RoadConfig::default()).expect("writes"),
            "{}"
        );
        assert_eq!(
            serde_json::to_string(&GeneratorKind::RoadNetwork(RoadConfig::default()))
                .expect("writes"),
            PLAIN_BEFORE
        );
        assert_eq!(
            serde_json::to_string(&GeneratorKind::RoadNetwork(populated())).expect("writes"),
            POPULATED_BEFORE
        );
        let back: GeneratorKind = serde_json::from_str(POPULATED_BEFORE).expect("reads");
        assert_eq!(back, GeneratorKind::RoadNetwork(populated()));
    }

    /// #1556: a set street field is written in the record's own form - the
    /// fixed point, each basis kind by its `$type` - with its untouched
    /// members left out, and reads back as it was. The authored form
    /// `docs/agent/region.md` gives for one ring reads too.
    #[test]
    fn a_street_field_round_trips_in_the_records_own_form() {
        let field = RoadField {
            basis: vec![
                RoadBasis::Ring {
                    center: Fp2([90.0, -10.5]),
                    radius: Fp(200.0),
                    strength: Fp(1.5),
                },
                RoadBasis::Grid {
                    center: Fp2([-5.0, 7.5]),
                    bearing: Fp(30.0),
                    radius: Fp(80.0),
                    strength: Fp(0.5),
                },
            ],
            keep_out: vec![RoadKeepOut {
                center: Fp2([40.0, 60.0]),
                radius: Fp(25.0),
            }],
            ..RoadField::default()
        };
        let network = GeneratorKind::RoadNetwork(RoadConfig {
            field,
            ..RoadConfig::default()
        });
        let v = serde_json::to_value(&network).expect("writes");
        assert_eq!(
            v["field"],
            json!({
                "basis": [
                    {
                        "$type": "network.symbios.road_basis.ring",
                        "center": [900_000, -105_000],
                        "radius": 2_000_000,
                        "strength": 15_000
                    },
                    {
                        "$type": "network.symbios.road_basis.grid",
                        "center": [-50_000, 75_000],
                        "bearing": 300_000,
                        "radius": 800_000,
                        "strength": 5_000
                    }
                ],
                "keep_out": [{ "center": [400_000, 600_000], "radius": 250_000 }]
            }),
            "{v}"
        );
        let back: GeneratorKind = serde_json::from_value(v).expect("reads back");
        assert_eq!(back, network);

        let scalars = GeneratorKind::RoadNetwork(RoadConfig {
            field: RoadField {
                smoothing: Fp(12.5),
                terrain_weight: Fp(0.0),
                ..RoadField::default()
            },
            ..RoadConfig::default()
        });
        let v = serde_json::to_value(&scalars).expect("writes");
        assert_eq!(
            v["field"],
            json!({ "smoothing": 125_000, "terrain_weight": 0 }),
            "{v}"
        );
        let back: GeneratorKind = serde_json::from_value(v).expect("reads back");
        assert_eq!(back, scalars);

        let authored: RoadField = serde_json::from_value(json!({
            "basis": [{
                "$type": "network.symbios.road_basis.ring",
                "center": [400_000, -250_000],
                "radius": 1_500_000,
                "strength": 10_000
            }]
        }))
        .expect("the documented ring reads");
        assert_eq!(
            authored,
            RoadField {
                basis: vec![RoadBasis::Ring {
                    center: Fp2([40.0, -25.0]),
                    radius: Fp(150.0),
                    strength: Fp(1.0),
                }],
                ..RoadField::default()
            }
        );
    }

    /// #1556: a basis kind from a newer client reads as `Unknown` - the
    /// network still reads, the rest of its field with it - and, like every
    /// `Unknown` (#1111), is never written back: a save is refused rather
    /// than replacing the newer client's field with a husk.
    #[test]
    fn a_basis_kind_from_a_newer_client_reads_as_unknown() {
        let kind: GeneratorKind = serde_json::from_value(json!({
            "$type": "network.symbios.gen.road_network",
            "field": {
                "basis": [
                    {
                        "$type": "network.symbios.road_basis.spiral",
                        "center": [0, 0],
                        "turns": 30_000
                    },
                    {
                        "$type": "network.symbios.road_basis.ring",
                        "center": [0, 0],
                        "radius": 1_000_000,
                        "strength": 10_000
                    }
                ]
            }
        }))
        .expect("a network carrying a newer kind reads");
        let GeneratorKind::RoadNetwork(road) = &kind else {
            panic!("a road network reads as one");
        };
        assert_eq!(
            road.field.basis,
            vec![
                RoadBasis::Unknown,
                RoadBasis::Ring {
                    center: Fp2([0.0, 0.0]),
                    radius: Fp(100.0),
                    strength: Fp(1.0),
                },
            ]
        );
        let refused = serde_json::to_string(&kind).expect_err("an unknown kind is not written");
        assert!(
            refused.to_string().contains("cannot be serialized"),
            "{refused}"
        );
    }

    /// #1556 critic: a ring or grid with members left off the wire still
    /// reads, each missing one at the value a new field starts with. A
    /// required member used to fail the network's decode, and with it the
    /// whole terrain generator: a room with no ground, water or streets.
    #[test]
    fn a_basis_field_missing_members_reads_with_their_defaults() {
        let terrain: Generator = serde_json::from_value(json!({
            "$type": "network.symbios.gen.terrain",
            "children": [{
                "$type": "network.symbios.gen.road_network",
                "field": {
                    "basis": [
                        {
                            "$type": "network.symbios.road_basis.ring",
                            "center": [100_000, -200_000],
                            "radius": 900_000
                        },
                        {"$type": "network.symbios.road_basis.grid", "bearing": 450_000},
                        {"$type": "network.symbios.road_basis.grid"}
                    ]
                }
            }]
        }))
        .expect("a terrain whose network leaves basis members off still reads");
        let GeneratorKind::RoadNetwork(road) = &terrain.children[0].kind else {
            panic!("the terrain's child reads as its road network");
        };
        let starting = Fp(RoadBasis::DEFAULT_RADIUS);
        assert_eq!(
            road.field.basis,
            vec![
                RoadBasis::Ring {
                    center: Fp2([10.0, -20.0]),
                    radius: Fp(90.0),
                    strength: Fp(RoadBasis::DEFAULT_STRENGTH),
                },
                RoadBasis::Grid {
                    center: Fp2([0.0, 0.0]),
                    bearing: Fp(45.0),
                    radius: starting,
                    strength: Fp(RoadBasis::DEFAULT_STRENGTH),
                },
                RoadBasis::grid_at(Fp2([0.0, 0.0])),
            ]
        );
    }

    /// #1556: a grid's bearing has one value per grid, in `[0, 180)`: a half
    /// turn more is the same grid, a non-finite bearing reads as north-south,
    /// and a hair below zero - which `rem_euclid` rounds up to 180 - is 0.
    #[test]
    fn a_grid_bearing_folds_into_a_half_turn() {
        for (given, folded) in [
            (0.0, 0.0),
            (30.0, 30.0),
            (179.5, 179.5),
            (180.0, 0.0),
            (210.0, 30.0),
            (-30.0, 150.0),
            (540.0, 0.0),
            (-1.0e-8, 0.0),
            (f32::NAN, 0.0),
            (f32::NEG_INFINITY, 0.0),
        ] {
            assert_eq!(RoadBasis::canonical_bearing(given), folded, "{given}");
        }
    }
}
