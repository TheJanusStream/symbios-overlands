//! Vegetation wind sway (#916) — the `MaterialExtension` that animates
//! foliage in the vertex shader, and the plumbing that attaches it.
//!
//! # What sways
//!
//! Foliage only: L-system leaf buckets and ground-cover cards. Trunks,
//! branches and every other surface keep a plain [`StandardMaterial`] and
//! stay rigid, so the cost of this feature is confined to the geometry it
//! actually moves. [`sways`] is the predicate that decides, keyed on the
//! material's procedural texture — leaves and blades sway, moss and lichen
//! (which are encrusting *surfaces*, not cards) do not, and neither do the
//! non-vegetation card textures like glass and grilles.
//!
//! # How it attaches
//!
//! The spawn paths cannot build this material directly. A procedural
//! material's textures arrive *asynchronously* — the bake completes some
//! frames later and the upstream patch system writes the image handles into
//! `Assets<StandardMaterial>` by handle. An [`ExtendedMaterial`] embeds its
//! base by value, so a copy taken at spawn time would never see those
//! textures and every leaf would render as an opaque untextured card.
//!
//! So the spawn paths only mark: they insert a [`WindSway`] component
//! alongside the ordinary `MeshMaterial3d<StandardMaterial>`, and
//! [`attach_wind_materials`] swaps the component type on a later frame.
//! [`mirror_wind_material_bases`] then keeps the embedded base in step with
//! the source material for the rest of the session, which is what lets the
//! bake — and any later re-bake or blob-image fetch — land on swaying foliage
//! exactly as it lands on static geometry.
//!
//! One frame of un-swayed foliage between the two is not observable: the
//! material is identical bar the vertex displacement, which starts at the
//! rest pose anyway.
//!
//! # Batching
//!
//! [`WindMaterialLinks`] keys on the *source* material's [`AssetId`], so a
//! scatter of 500 cards sharing one `StandardMaterial` handle (which the
//! content-addressed prim cache guarantees) collapses onto one wind material
//! too. Per-instance variation is derived in the shader from the instance
//! origin instead of from a uniform, because a per-instance uniform would
//! fork the material handle per instance and undo that.
//!
//! The links map deliberately stores an [`AssetId`] rather than a
//! [`Handle`]: holding a strong handle would pin every wind material — and
//! through it a `StandardMaterial` and its four images — for the life of the
//! session, which is exactly the retention shape #919 was raised to fix.
//! Entries whose material has been dropped simply fail to resolve and are
//! rebuilt on demand.

use bevy::asset::{AssetEvent, AssetId};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use std::collections::{HashMap, HashSet};

use crate::config::vegetation_wind as cfg;
use crate::pds::SovereignTextureConfig;

const WIND_SHADER_PATH: &str = "shaders/wind.wgsl";

/// Ceiling on [`WindMaterialLinks`] before dead entries are swept. Distinct
/// foliage materials number in the dozens per region, so this is far above
/// any real room; it exists so a long session of re-rolls cannot accumulate
/// stale `AssetId` pairs without bound.
const MAX_LINKS: usize = 4_096;

/// Which motion profile a swaying entity uses.
///
/// The distinction is about where the entity's origin sits relative to the
/// geometry, not about the species: an L-system bucket's origin is at the
/// plant's base with all its foliage above, whereas a ground-cover card's
/// origin is at the card's own centre.
#[derive(Component, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum WindSway {
    /// Foliage hanging off an L-system plant. Weight climbs from the plant's
    /// base to [`cfg::branch::REFERENCE_HEIGHT`].
    Branch,
    /// A ground-cover card, whose origin is its centre — weight is biased so
    /// the bottom edge is near-static and the top edge mobile.
    Card,
}

impl WindSway {
    /// The per-profile half of the uniform block. The global half
    /// (direction and speed) is filled in from [`VegetationWind`].
    fn uniforms(self) -> WindUniforms {
        match self {
            Self::Branch => WindUniforms {
                strength: cfg::branch::AMPLITUDE,
                height_scale: 1.0 / cfg::branch::REFERENCE_HEIGHT,
                height_bias: 0.0,
                flutter: cfg::branch::FLUTTER,
                ..Default::default()
            },
            Self::Card => WindUniforms {
                strength: cfg::card::AMPLITUDE,
                height_scale: 1.0 / cfg::card::HALF_HEIGHT,
                // Half a card's height below the origin weighs 0, the same
                // distance above weighs 1.
                height_bias: 0.5,
                flutter: cfg::card::FLUTTER,
                ..Default::default()
            },
        }
    }
}

/// `true` when a material's procedural texture is vegetation foliage that
/// should sway.
///
/// Deliberately narrower than the upstream `RenderProperties::is_card` flag,
/// which also covers windows, stained glass and iron grilles — all of them
/// alpha-masked cards, none of them things that move in the wind. Moss and
/// lichen are excluded from the other direction: they are encrusting
/// *surfaces* painted onto cushion mounds, so there is nothing to bend.
pub fn sways(texture: &SovereignTextureConfig) -> bool {
    matches!(
        texture,
        SovereignTextureConfig::Leaf(_)
            | SovereignTextureConfig::Twig(_)
            | SovereignTextureConfig::Needle(_)
            | SovereignTextureConfig::GrassTuft(_)
            | SovereignTextureConfig::Frond(_)
            | SovereignTextureConfig::Reed(_)
            | SovereignTextureConfig::Broadleaf(_)
            | SovereignTextureConfig::Flower(_)
    )
}

/// GPU uniform block shared with `wind.wgsl`.
///
/// 32 bytes, which WebGL2 requires to be a multiple of 16 — the trailing
/// `_pad0` is what makes it so, and `wgsl_block_mirrors_the_rust_one` below
/// keeps this declaration and the shader's copy from drifting apart.
#[derive(Debug, Clone, Default, ShaderType)]
pub struct WindUniforms {
    /// 2D wind direction in world XZ, straight from the room's
    /// `Environment::cloud_wind_dir`. Need not be unit length — the shader
    /// normalises an epsilon-padded copy, so an all-zero direction is
    /// harmless rather than a NaN.
    pub wind_dir: Vec2,
    /// Wind speed (m/s), from `Environment::cloud_speed`. Scales time, so it
    /// sets how fast the sway cycles rather than how far it reaches.
    pub speed: f32,
    /// Peak lean in metres at full height weight. Per-profile; see
    /// [`WindSway::uniforms`].
    pub strength: f32,
    /// Reciprocal of the height (m) over which the sway weight ramps from
    /// the entity origin to full.
    pub height_scale: f32,
    /// Constant added to the height weight before clamping — `0.5` for a
    /// card, whose origin is its centre rather than its base.
    pub height_bias: f32,
    /// Cross-wind flutter as a fraction of [`Self::strength`].
    pub flutter: f32,
    /// Pad to 32 bytes: WebGL2 rejects a uniform block whose size is not a
    /// multiple of 16, and the device-side validator is the only thing that
    /// catches it — native and the test suite both pass, and the wasm deploy
    /// fails at pipeline creation naming neither struct nor field. Mirror
    /// this in `WindUniforms` in `wind.wgsl`.
    pub _pad0: f32,
}

/// [`MaterialExtension`] that drives `wind.wgsl`.
///
/// Bind-group slots (group `MATERIAL_BIND_GROUP`, 100 +):
/// - 100 [`WindUniforms`] uniform
///
/// Uniform-only by design. `StandardMaterial`'s own PBR textures plus Bevy's
/// view-group shadow and IBL textures already put this material close to the
/// 16-slot ceiling wgpu-hal's GLES backend imposes (see
/// [`crate::splat::SplatExtension`], which sits *at* it), and a vertex-stage
/// wind effect needs no textures of its own.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct WindExtension {
    #[uniform(100)]
    pub uniforms: WindUniforms,
    /// The `StandardMaterial` this extension's base was cloned from.
    ///
    /// Not a binding — it is a strong handle held so the source asset
    /// outlives the wind material that mirrors it, and so
    /// [`mirror_wind_material_bases`] can tell which source a given wind
    /// material follows. Without it the source would be dropped as soon as
    /// the compile caches released it, and the pending texture bake would
    /// then patch an asset nobody holds.
    pub source: Handle<StandardMaterial>,
}

impl MaterialExtension for WindExtension {
    fn vertex_shader() -> ShaderRef {
        WIND_SHADER_PATH.into()
    }

    /// Bevy renders shadow maps through the prepass pipeline, so the
    /// displacement has to be repeated here or every foliage shadow stays
    /// frozen in the mesh's rest pose while the geometry above it moves.
    /// `wind.wgsl` serves both from one file, guarded by `PREPASS_PIPELINE`.
    fn prepass_vertex_shader() -> ShaderRef {
        WIND_SHADER_PATH.into()
    }

    /// The deferred path reaches the same shader for the same reason. This
    /// renderer is forward-only today, so it is unused — but a mismatch here
    /// would surface as foliage whose g-buffer geometry disagrees with its
    /// forward geometry, which is a far harder thing to diagnose than it is
    /// to pre-empt.
    fn deferred_vertex_shader() -> ShaderRef {
        WIND_SHADER_PATH.into()
    }
}

/// Convenience alias for the full extended-material type used by foliage.
pub type VegetationWindMaterial = ExtendedMaterial<StandardMaterial, WindExtension>;

/// The room's live wind, as the vegetation shader sees it.
///
/// Written by the world compiler's `apply_environment_state` from
/// `Environment::cloud_wind_dir` / `cloud_speed` — the same values that drive
/// the cloud deck, so a wind-direction drag in the editor turns the clouds
/// and the foliage together rather than leaving them disagreeing.
#[derive(Resource, Debug, Clone, Copy)]
pub struct VegetationWind {
    pub dir: Vec2,
    pub speed: f32,
}

impl Default for VegetationWind {
    fn default() -> Self {
        Self {
            dir: Vec2::from_array(crate::config::lighting::clouds::WIND_DIR),
            speed: crate::config::lighting::clouds::SPEED,
        }
    }
}

/// Source `StandardMaterial` → the wind material that wraps it, per profile.
///
/// See the [module docs](self) for why the value is an [`AssetId`] and not a
/// [`Handle`].
#[derive(Resource, Default)]
pub struct WindMaterialLinks {
    links: HashMap<(AssetId<StandardMaterial>, WindSway), AssetId<VegetationWindMaterial>>,
}

impl WindMaterialLinks {
    /// Number of live links. Exposed for the retention test.
    pub fn len(&self) -> usize {
        self.links.len()
    }

    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }
}

/// Swap the marked entities' `MeshMaterial3d<StandardMaterial>` for the wind
/// material wrapping the same source.
///
/// Entities drop out of this query the moment they are converted (the
/// `StandardMaterial` component is removed), so it costs one archetype probe
/// per frame once a room has settled.
pub fn attach_wind_materials(
    mut commands: Commands,
    pending: Query<(Entity, &MeshMaterial3d<StandardMaterial>, &WindSway)>,
    std_materials: Res<Assets<StandardMaterial>>,
    mut wind_materials: ResMut<Assets<VegetationWindMaterial>>,
    mut links: ResMut<WindMaterialLinks>,
    wind: Res<VegetationWind>,
) {
    for (entity, source, &profile) in pending.iter() {
        let source_id = source.0.id();
        let key = (source_id, profile);

        // A live link is reused; one whose material has since been dropped
        // resolves to `None` and is rebuilt below.
        let existing = links
            .links
            .get(&key)
            .copied()
            .and_then(|id| wind_materials.get_strong_handle(id));

        let handle = match existing {
            Some(handle) => handle,
            None => {
                // The source is added synchronously by the spawn path, so a
                // miss here means the asset was dropped between spawn and
                // now. Leave the entity marked and retry next frame rather
                // than substituting a default material.
                let Some(base) = std_materials.get(&source.0) else {
                    continue;
                };
                let mut uniforms = profile.uniforms();
                uniforms.wind_dir = wind.dir;
                uniforms.speed = wind.speed;
                let handle = wind_materials.add(VegetationWindMaterial {
                    base: base.clone(),
                    extension: WindExtension {
                        uniforms,
                        source: source.0.clone(),
                    },
                });
                links.links.insert(key, handle.id());
                handle
            }
        };

        commands
            .entity(entity)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert(MeshMaterial3d(handle));
    }

    if links.links.len() > MAX_LINKS {
        links
            .links
            .retain(|_, id| wind_materials.get_strong_handle(*id).is_some());
    }
}

/// Keep each wind material's embedded base in step with the
/// `StandardMaterial` it was cloned from.
///
/// This is what makes the async texture pipeline work through an
/// [`ExtendedMaterial`]: the bake lands in `Assets<StandardMaterial>` some
/// frames after the material was built, and without this mirror the foliage
/// would keep the untextured copy taken at attach time — alpha-masked cards
/// with no alpha, i.e. opaque squares.
pub fn mirror_wind_material_bases(
    mut events: MessageReader<AssetEvent<StandardMaterial>>,
    std_materials: Res<Assets<StandardMaterial>>,
    mut wind_materials: ResMut<Assets<VegetationWindMaterial>>,
) {
    let modified: HashSet<AssetId<StandardMaterial>> = events
        .read()
        .filter_map(|event| match event {
            AssetEvent::Modified { id } => Some(*id),
            _ => None,
        })
        .collect();
    if modified.is_empty() {
        return;
    }

    // Collect first, then patch by id. `iter_mut` would flag *every* wind
    // material as changed — and so re-upload its bind group — no matter how
    // few of them actually follow a modified source.
    let targets: Vec<AssetId<VegetationWindMaterial>> = wind_materials
        .iter()
        .filter(|(_, mat)| modified.contains(&mat.extension.source.id()))
        .map(|(id, _)| id)
        .collect();

    for wind_id in targets {
        let Some(base) = wind_materials
            .get(wind_id)
            .and_then(|mat| std_materials.get(&mat.extension.source))
            .cloned()
        else {
            continue;
        };
        if let Some(mut mat) = wind_materials.get_mut(wind_id) {
            mat.base = base;
        }
    }
}

/// Push the room's wind onto every live wind material when it changes.
///
/// Only the two global fields are written: the per-profile ones were set when
/// the material was built and do not depend on the environment.
pub fn apply_wind_state(
    wind: Res<VegetationWind>,
    mut wind_materials: ResMut<Assets<VegetationWindMaterial>>,
) {
    if !wind.is_changed() {
        return;
    }
    for (_, mat) in wind_materials.iter_mut() {
        mat.extension.uniforms.wind_dir = wind.dir;
        mat.extension.uniforms.speed = wind.speed;
    }
}

/// Registers the wind material, its resources and the three systems above.
///
/// Added by both the game app and
/// [`world_builder::register_headless_spawn`](crate::world_builder::register_headless_spawn).
/// The render tool takes a still, so the sway contributes nothing to a
/// contact sheet — but the headless path is where the foliage render pipeline
/// is actually created, and `wind.wgsl` is loaded at runtime rather than
/// compiled by the build. Registering it there is what makes a WGSL error
/// fail on the machine doing the editing instead of in a browser.
pub struct VegetationWindPlugin;

impl Plugin for VegetationWindPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<VegetationWindMaterial>::default())
            .init_resource::<VegetationWind>()
            .init_resource::<WindMaterialLinks>()
            .add_systems(
                Update,
                (
                    attach_wind_materials,
                    mirror_wind_material_bases,
                    apply_wind_state,
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::render_resource::ShaderType;

    /// WebGL2 rejects a uniform block whose size is not a multiple of 16
    /// (`DownlevelFlags::BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED` is unsupported
    /// there). Only the device-side validator catches it, so native and this
    /// test suite would both pass while the wasm deploy failed at pipeline
    /// creation — naming neither the struct nor the field that broke it.
    #[test]
    fn wind_uniforms_block_is_16_byte_aligned() {
        let size = WindUniforms::min_size().get();
        assert_eq!(
            size % 16,
            0,
            "WindUniforms is {size} bytes — WebGL2 needs a multiple of 16. \
             Add or adjust a `_pad` field, and mirror it in wind.wgsl."
        );
    }

    /// Ordered `(name, type)` pairs of a `struct <name> { .. }` block in WGSL
    /// source, ignoring comments and blank lines.
    fn wgsl_fields(src: &str, struct_name: &str) -> Vec<(String, String)> {
        let head = format!("struct {struct_name} {{");
        let start = src.find(&head).expect("struct not found in shader") + head.len();
        let body = &src[start..start + src[start..].find('}').expect("unterminated struct")];
        body.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with("//") && l.contains(':'))
            .map(|l| {
                let (name, ty) = l.split_once(':').expect("filtered on ':'");
                (
                    name.trim().to_string(),
                    ty.trim().trim_end_matches(',').to_string(),
                )
            })
            .collect()
    }

    /// The Rust and WGSL declarations of the uniform block are two
    /// hand-written copies of one layout, and nothing in the build compiles
    /// the shader — WGSL is loaded at runtime. Adding a field to one and
    /// forgetting the other produces no error anywhere: the GPU reads the
    /// block at the wrong offsets and the foliage sways wrongly, or not at
    /// all.
    ///
    /// Field *names* are compared rather than counted because `wind_dir` is
    /// a `vec2` — a count would have to encode the 8-vs-4-byte distinction
    /// and would quietly stop meaning anything the next time a vector field
    /// is added.
    #[test]
    fn wgsl_block_mirrors_the_rust_one() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/shaders/wind.wgsl"
        ))
        .expect("wind.wgsl is a tracked asset");

        let expected = [
            ("wind_dir", "vec2<f32>"),
            ("speed", "f32"),
            ("strength", "f32"),
            ("height_scale", "f32"),
            ("height_bias", "f32"),
            ("flutter", "f32"),
            ("_pad0", "f32"),
        ];
        let actual = wgsl_fields(&src, "WindUniforms");
        assert_eq!(
            actual.len(),
            expected.len(),
            "wind.wgsl declares {} fields, WindUniforms has {}: {actual:?}",
            actual.len(),
            expected.len()
        );
        for (got, want) in actual.iter().zip(expected) {
            assert_eq!(
                (got.0.as_str(), got.1.as_str()),
                want,
                "wind.wgsl field order / naming drifted from WindUniforms"
            );
        }

        // The Rust block's size is the other half of the tie: the field list
        // above can only describe a 32-byte block (8 + 6×4, `vec2` aligned to
        // 8), so a Rust-side field added without touching the shader moves
        // this even when the names still line up.
        assert_eq!(
            WindUniforms::min_size().get(),
            32,
            "the mirrored field list describes a 32-byte block"
        );
    }

    /// The prepass binds a smaller view layout than the main pass, and
    /// `Globals` sits at a *different index* in it — binding 1 rather than
    /// 11. Reaching for `mesh_view_bindings::globals` in the prepass branch
    /// compiles and composes perfectly happily, then panics at pipeline
    /// creation with `Shader global ResourceBinding { group: 0, binding: 11 }
    /// is not available in the pipeline layout` the first time a
    /// shadow-casting light sees foliage.
    ///
    /// This is a textual guard rather than a real compile, because nothing in
    /// the build compiles WGSL and the render tool renders without shadows —
    /// so the prepass variant of this shader has no other automated check at
    /// all. It locks the one mistake that has actually been made here.
    #[test]
    fn the_prepass_branch_binds_globals_at_its_own_index() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/shaders/wind.wgsl"
        ))
        .expect("wind.wgsl is a tracked asset");

        let start = src
            .find("#ifdef PREPASS_PIPELINE")
            .expect("the shader must branch on PREPASS_PIPELINE");
        let end = start
            + src[start..]
                .find("#else")
                .expect("the PREPASS_PIPELINE branch must have an #else");
        // Comments are stripped first: the branch *documents* the wrong
        // import by name, and a prose mention of it is not a binding.
        let prepass: String = src[start..end]
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            prepass.contains("@group(0) @binding(1) var<uniform> globals: Globals;"),
            "the prepass branch must declare Globals at its own binding index"
        );
        assert!(
            !prepass.contains("mesh_view_bindings::globals"),
            "mesh_view_bindings declares Globals at binding 11, which the \
             prepass view layout does not have — this panics at pipeline \
             creation as soon as foliage casts a shadow"
        );
    }

    /// The predicate is the whole scope of the feature: it decides what gets
    /// a second material type and a vertex shader, and what stays as it was.
    #[test]
    fn only_vegetation_foliage_sways() {
        use crate::pds::{
            SovereignBarkConfig, SovereignFrondConfig, SovereignGrassTuftConfig,
            SovereignIronGrilleConfig, SovereignLeafConfig, SovereignLichenConfig,
            SovereignMossConfig, SovereignWindowConfig,
        };

        assert!(sways(&SovereignTextureConfig::Leaf(
            SovereignLeafConfig::default()
        )));
        assert!(sways(&SovereignTextureConfig::GrassTuft(
            SovereignGrassTuftConfig::default()
        )));
        assert!(sways(&SovereignTextureConfig::Frond(
            SovereignFrondConfig::default()
        )));

        // Bark is the reason the feature is foliage-only: trunks stay rigid.
        assert!(!sways(&SovereignTextureConfig::Bark(
            SovereignBarkConfig::default()
        )));
        // Cards that are not vegetation. `RenderProperties::is_card` is true
        // for both of these, which is precisely why it is not the predicate.
        assert!(!sways(&SovereignTextureConfig::Window(
            SovereignWindowConfig::default()
        )));
        assert!(!sways(&SovereignTextureConfig::IronGrille(
            SovereignIronGrilleConfig::default()
        )));
        // Encrusting surfaces on cushion mounds — nothing to bend.
        assert!(!sways(&SovereignTextureConfig::Moss(
            SovereignMossConfig::default()
        )));
        assert!(!sways(&SovereignTextureConfig::Lichen(
            SovereignLichenConfig::default()
        )));
        assert!(!sways(&SovereignTextureConfig::None));
    }

    /// A card's origin is its centre, so its weight has to straddle zero:
    /// the bottom edge near-static, the top edge fully mobile. A branch
    /// profile measures from the plant's base instead, so it starts at zero.
    #[test]
    fn the_two_profiles_weight_height_differently() {
        let card = WindSway::Card.uniforms();
        let branch = WindSway::Branch.uniforms();

        // Shader weight before clamping, for a vertex `dy` metres above the
        // entity origin.
        let weight =
            |u: &WindUniforms, dy: f32| (dy * u.height_scale + u.height_bias).clamp(0.0, 1.0);

        // A 0.3 m card: the base sits 0.15 m below its origin.
        assert!(
            weight(&card, -0.15) < 0.25,
            "a card's base must be near-still"
        );
        assert!(weight(&card, 0.15) > 0.75, "a card's tip must move");

        // An L-system bucket's origin is the plant base, so nothing is below it.
        assert_eq!(weight(&branch, 0.0), 0.0, "no sway at the plant's base");
        assert!(weight(&branch, 4.0) >= 1.0, "full sway in the canopy");
        assert!(
            weight(&branch, 1.0) < weight(&branch, 3.0),
            "sway must grow with height"
        );
    }

    /// Both profiles must produce a finite, bounded displacement — the
    /// amplitudes are metres of travel, and a leaf that swings further than
    /// its own branch is worse than one that does not move at all.
    #[test]
    fn profile_amplitudes_stay_within_their_geometry() {
        let card = WindSway::Card.uniforms();
        assert!(
            card.strength < cfg::card::HALF_HEIGHT * 0.25,
            "a ground-cover card must not swing further than its own height"
        );
        let branch = WindSway::Branch.uniforms();
        assert!(
            branch.strength < cfg::branch::REFERENCE_HEIGHT * 0.25,
            "canopy foliage must not detach from its branch"
        );
    }
}
