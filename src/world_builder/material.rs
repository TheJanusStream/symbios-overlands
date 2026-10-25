//! Water volume spawning and procedural material building. Texture
//! generation is dispatched per target: on **native** through the upstream
//! [`bevy_symbios_texture::PatchMaterialTextures`] task entity (baked on the
//! upstream's private rayon pool, patched by its `patch_material_textures`
//! system for the [`WeatheredMaterial`] every part is, #1686); on **wasm**
//! through the
//! pooled gen-worker via [`super::surface_bake`] (#807), because Bevy's task
//! pools collapse onto the main thread there and every bake would stall a
//! frame. The material itself comes from
//! [`bevy_symbios_texture::MaterialSettings::standard_material`] on both
//! targets, so the dispatch forks per platform and the appearance cannot.
//! (#1170 finding 39: that helper used to be a hand-kept copy here, and the
//! parity test guarding it stood up a World on every run.)

use bevy::math::Affine2;
use bevy::prelude::*;
use bevy_symbios_texture::{TextureCache, apply_generated_handles};

use crate::config::material::{ROUGH_REFLECTANCE_AT, ROUGH_REFLECTANCE_FROM};
use crate::config::terrain::splat::MATERIAL_REFLECTANCE;
use crate::weathered::{WeatheredMaterial, weathered_glow};

use crate::pds::{Environment, SovereignMaterialSettings, SovereignTextureConfig, WaterSurface};
use crate::terrain::WaterVolume;
use crate::water::{
    WAKE_SAMPLES_MAX, WaterExtension, WaterMaterial, WaterPlane, WaterPlaneIndex, WaterSurfaces,
    WaterUniforms,
};

use super::RoomEntity;
use super::compile::SpawnCtx;

/// Translate a [`WaterSurface`] + [`Environment`] pair into the uniform block
/// the water shader reads. Every value that the shader depends on flows
/// through this function so the egui widgets, raw JSON edits, and peer
/// broadcasts all converge on the same GPU state.
fn build_water_uniforms(surface: &WaterSurface, env: &Environment) -> WaterUniforms {
    let sky = crate::sky::sky_uniforms(env);
    WaterUniforms {
        shallow_color: Vec4::from_array(surface.shallow_color.0),
        deep_color: Vec4::from_array(surface.deep_color.0),
        scatter_color: Vec4::new(
            env.water_scatter_color.0[0],
            env.water_scatter_color.0[1],
            env.water_scatter_color.0[2],
            0.0,
        ),
        // Unit toward-sun for the glitter lobe. `sun_position` is the
        // spot the directional light looks at the origin from, so its
        // normalised form IS the toward-sun direction - the same value
        // `apply_environment_state` re-derives from the live light
        // transform on every later environment change.
        sun_dir: Vec3::from_array(env.sun_position.0)
            .normalize_or(Vec3::Y)
            .extend(0.0),
        // The sky it reflects (#1672), as the sky shader is given it.
        sky_zenith: sky.zenith,
        sky_look: sky.look,
        wave_direction: Vec2::from_array(surface.wave_direction.0),
        wave_scale: surface.wave_scale.0,
        wave_speed: surface.wave_speed.0,
        wave_choppiness: surface.wave_choppiness.0,
        roughness: surface.roughness.0,
        metallic: surface.metallic.0,
        reflectance: surface.reflectance.0,
        foam_amount: surface.foam_amount.0,
        normal_scale_near: env.water_normal_scale_near.0,
        normal_scale_far: env.water_normal_scale_far.0,
        sun_glitter: env.water_sun_glitter.0,
        shore_foam_width: env.water_shore_foam_width.0,
        flow_amount: surface.flow_amount.0,
        // Wake perturbation channel. Both arrays zeroed at spawn;
        // `feed_water_wakes` (interaction consumer) overwrites entries
        // 0..wake_active_count each frame the volume has live
        // perturbations. Default `wake_strength = 0` in PDS means a
        // freshly spawned volume contributes nothing visible until
        // authored on.
        wake_samples_a: [Vec4::ZERO; WAKE_SAMPLES_MAX],
        wake_samples_b: [Vec4::ZERO; WAKE_SAMPLES_MAX],
        wake_active_count: 0,
        wake_strength: surface.wake_strength.0,
        wake_ripple_wavelength: surface.wake_ripple_wavelength.0,
        wake_decay_radius: surface.wake_decay_radius.0,
        // The drawn weather's, written by `apply_environment_state`.
        rain: 0.0,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_water_volume(
    commands: &mut Commands,
    surface: &WaterSurface,
    env: &Environment,
    placement_tf: Transform,
    world_extent: f32,
    meshes: &mut Assets<Mesh>,
    water_materials: &mut Assets<WaterMaterial>,
    water_surfaces: &mut WaterSurfaces,
    owner: usize,
) -> Entity {
    // Water sits at the placement transform's altitude exactly - the
    // record no longer carries a `level_offset` field, and the old
    // implicit `base_wl` baseline (LEVEL_FACTOR * HEIGHT_SCALE) is
    // gone too. The default homeworld preserves its historical
    // waterline by setting that altitude on the Water generator's
    // placement transform inside `RoomRecord::default_for_did`.

    // Flat `Plane3d` at the placement transform's altitude. The
    // previous iteration spawned a 1×1×1 `Cuboid` scaled to
    // `(world_extent, wl, world_extent)` and then discarded five out
    // of six faces in the fragment shader - a lot of rasterisation
    // work for zero visible fragments, and `fwidth`-after-`discard`
    // is only well-defined under uniform quad control flow. The plane
    // eliminates both.
    let half_extent = world_extent / 2.0;
    let mesh = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(half_extent)));
    // The mesh half-extent is recorded BEFORE the transform's scale is
    // applied - `WaterSurfaces::surface_at` re-applies the scale via the
    // inverse transform when testing containment.
    let plane = WaterPlane {
        world_from_local: placement_tf,
        local_half_extents: Vec2::splat(half_extent),
        flow_strength: surface.flow_strength.0,
        // Placement ownership lets the incremental compiler retire
        // exactly this plane when its placement is rebuilt. Avatar-mode
        // spawns carry NO_OWNER and only fall to full-pass clears.
        owner,
        cells: None,
    };
    spawn_water_surface(
        commands,
        surface,
        env,
        mesh,
        plane,
        water_materials,
        water_surfaces,
    )
}

/// A body of water at its own level (#1725): a lake on any terrain
/// (#1760) or one of Berlin's (#1724), drawn in the world's frame over its
/// cells ([`crate::water::bodies::body_mesh`]) at its level, and water to
/// physics over them alone (#1726). Its entity stands apart from the
/// layer's node, which is where its record puts it and what the editor
/// moves.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_water_body(
    commands: &mut Commands,
    surface: &WaterSurface,
    env: &Environment,
    body: &crate::water::bodies::WaterBody,
    meshes: &mut Assets<Mesh>,
    water_materials: &mut Assets<WaterMaterial>,
    water_surfaces: &mut WaterSurfaces,
    owner: usize,
) -> Entity {
    let grid = body.cells.grid;
    let mesh = meshes.add(crate::water::bodies::body_mesh(&body.cells));
    // Its rectangle is the whole grid and the half cell round it, its
    // cells' squares: the cells bound it.
    let reach = Vec2::new(grid.width as f32, grid.height as f32) * grid.scale * 0.5;
    let plane = WaterPlane {
        world_from_local: Transform::from_xyz(0.0, body.level, 0.0),
        local_half_extents: reach,
        flow_strength: surface.flow_strength.0,
        owner,
        cells: Some(body.cells.clone()),
    };
    spawn_water_surface(
        commands,
        surface,
        env,
        mesh,
        plane,
        water_materials,
        water_surfaces,
    )
}

/// Spawn a water surface drawn as `mesh` with `surface`'s material, placed
/// as `plane` says and registered there for the physics to find.
fn spawn_water_surface(
    commands: &mut Commands,
    surface: &WaterSurface,
    env: &Environment,
    mesh: Handle<Mesh>,
    plane: WaterPlane,
    water_materials: &mut Assets<WaterMaterial>,
    water_surfaces: &mut WaterSurfaces,
) -> Entity {
    // Straight-down view colour seeds the StandardMaterial base colour for
    // any non-shader-overridden path (shadow-caster fallback, editor outline,
    // etc.). The shader re-derives the view-dependent blend itself.
    let base = surface.shallow_color.0;
    let water_mat = water_materials.add(WaterMaterial {
        base: StandardMaterial {
            base_color: Color::srgba(base[0], base[1], base[2], base[3]),
            perceptual_roughness: surface.roughness.0,
            metallic: surface.metallic.0,
            alpha_mode: AlphaMode::Blend,
            // Back-face cull the plane: viewed from underwater the surface
            // contributes nothing, matching the previous Cuboid+discard
            // behaviour without the wasted side-face rasterisation.
            cull_mode: Some(bevy::render::render_resource::Face::Back),
            ..default()
        },
        extension: WaterExtension {
            uniforms: build_water_uniforms(surface, env),
        },
    });

    // Register this surface in the runtime lookup so per-frame physics
    // (rover buoyancy, scatter biome filter) can find it without re-walking
    // the record.
    //
    // `plane_idx` is captured before the push so it can be attached to
    // the spawned entity as a `WaterPlaneIndex`. The interaction
    // framework's water-wake consumer walks every `WaterVolume` and
    // uses this index to route contact samples (which the
    // ContactClassifier emits indexed against `WaterSurfaces.planes`)
    // to the correct material asset.
    let (tf, owner) = (plane.world_from_local, plane.owner);
    let plane_idx = water_surfaces.planes.len();
    water_surfaces.planes.push(plane);

    commands
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(water_mat),
            tf,
            WaterVolume,
            WaterPlaneIndex(plane_idx),
            RoomEntity,
            // Same owner as the registry plane above: the incremental
            // compiler's flat unit sweep retires this mesh even when the
            // gizmo has detached it from its anchor hierarchy (the
            // "duplicate water layer after a Y drag" bug).
            super::PlacementUnit(owner),
        ))
        .id()
}

/// Thin `SpawnCtx` wrapper around [`build_procedural_material`] for the
/// world-builder hot path, which already holds a [`SpawnCtx`] and doesn't
/// need to unpack its individual `&mut` resources at every call site.
pub(super) fn spawn_procedural_material(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    settings: &SovereignMaterialSettings,
) -> Handle<WeatheredMaterial> {
    let handle = build_procedural_material(
        ctx.commands,
        ctx.materials,
        ctx.images,
        ctx.texture_cache,
        settings,
        ctx.weathers,
    );

    // Referenced textures leave `base_color_texture` empty (see
    // `SovereignTextureConfig::to_texture_config`, which collapses
    // Referenced to `TextureConfig::None`). Enqueue a BlobImageCache fetch
    // so the resolved bytes are painted into the same material slot once
    // the async fetch completes. Until then the material renders with the
    // procedural builder's flat fallback colour from
    // `SovereignMaterialSettings::base_color`. The cache coalesces
    // duplicate requests, so a room with many references to the same
    // source issues one HTTPS round-trip total.
    if let SovereignTextureConfig::Referenced { source } = &settings.texture {
        super::image_cache::request_blob_image(
            ctx.commands,
            ctx.blob_image_cache,
            super::image_cache::PaintInto::Weathered(ctx.materials, &handle),
            source,
        );
    }

    handle
}

/// Free-function core of [`spawn_procedural_material`] - takes the
/// resources upstream's
/// [`bevy_symbios_texture::build_procedural_material_async`] needs instead
/// of the full [`SpawnCtx`], so it could be reused without constructing a
/// world-builder context. Today that split is only an internal seam:
/// [`spawn_procedural_material`] is its sole caller.
///
/// Returns a [`WeatheredMaterial`] handle - drawn under the weather when
/// `weathers` (#1686), its glow shown from dusk to dawn when its settings
/// say so (#1782) - whose texture slots are populated asynchronously
/// once the texture-generator task finishes - or synchronously when
/// `texture_cache` already holds the config's fingerprint, in which case no
/// task is spawned at all. The async-path patching is performed by the
/// texture crate's `patch_material_textures` for this material (registered
/// by [`crate::weathered::WeatheredMaterialPlugin`]), which also writes
/// completed bakes back into the cache.
pub fn build_procedural_material(
    commands: &mut Commands,
    materials: &mut Assets<WeatheredMaterial>,
    images: &mut Assets<Image>,
    texture_cache: &mut TextureCache,
    settings: &SovereignMaterialSettings,
    weathers: bool,
) -> Handle<WeatheredMaterial> {
    let native = settings.to_native();
    // Every procedural material - catalogue constructs, primitives, foliage
    // cards, avatars - bakes at the shared surface/card resolution
    // (`config::textures::SURFACE`); the ground-splat path is separate and
    // stays at its own higher resolution. Every variant gets the same
    // dimensions so foliage cards and tiling surfaces share the cache layout.
    // The `TextureCache` dedups by content fingerprint *across* generators -
    // crucially it covers primitives, which have no generator-level material
    // cache, so N identical boulders bake one texture set and an unchanged
    // config re-bakes nothing on a rebuild.
    let size = crate::config::textures::SURFACE;

    let mut material = native.standard_material();
    let cache_key = native.cache_key(size, size);
    // #957: UV offset / rotation live overlands-side only - the upstream
    // `MaterialSettings` has no such fields - so they're applied over the
    // upstream material rather than inside it. `standard_material` sets a
    // uniform `uv_scale` transform and documents that a caller with its own
    // convention overwrites this one field; with offset and rotation at
    // their defaults this reproduces that `from_scale` exactly.
    material.uv_transform = sovereign_uv_transform(settings);
    // #1732: likewise overlands-side, a rough part's sheen is the ground's.
    material.reflectance = rough_reflectance(material.perceptual_roughness, material.reflectance);

    // Cache hit: write handles into the material before we hand it to Bevy.
    // Full lookup - disk-backed stores read their blob and upload it into
    // `images` here, so a store hit short-circuits generation exactly like a
    // memory hit (same contract as the upstream helper this mirrors).
    if let Some(key) = cache_key.as_ref()
        && let Some(handles) = texture_cache.get(key, images)
    {
        apply_generated_handles(&mut material, &handles);
        bump_texture_cache_counter(commands, true);
        return materials.add(weathered_glow(material, weathers, settings.dusk_to_dawn));
    }
    if cache_key.is_some() {
        bump_texture_cache_counter(commands, false);
    }

    let handle = materials.add(weathered_glow(material, weathers, settings.dusk_to_dawn));

    // Cache miss: dispatch generation if a generator is selected.
    //
    // Native - the upstream path, byte-for-byte: `TextureConfig::spawn` bakes
    // on the upstream's private rayon pool (deliberately NOT Bevy's
    // `AsyncComputeTaskPool`, which its monolithic generate() loop would
    // starve) and the upstream `patch_material_textures` system, registered
    // for this material, patches the base's slots + writes the cache.
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(pending) = native.texture.spawn(size, size) {
        commands.spawn((
            pending,
            bevy_symbios_texture::PatchMaterialTextures {
                target: handle.clone(),
                cache_key,
            },
        ));
    }

    // Wasm - Bevy's task pools collapse onto the main thread, so the upstream
    // path bakes every cache miss as a frame stall (#807: the avatar re-roll
    // freeze). Route the bake through the pooled gen-worker instead; the
    // dispatch runs as a queued command so it can reach the
    // `PendingSurfaceBakes` resource without growing this signature.
    #[cfg(target_arch = "wasm32")]
    if let (Some(key), Some(job)) = (
        cache_key,
        super::surface_bake::surface_bake_job(&native.texture),
    ) {
        let target = handle.clone();
        let is_card = native.texture.render_properties().is_card;
        commands.queue(move |world: &mut World| {
            super::surface_bake::dispatch_surface_bake(world, key, job, size, target, is_card);
        });
    }

    handle
}

/// Record a texture-cache hit or miss (#811 fingerprint/eviction health -
/// see `names::RUNTIME_TEXTURE_CACHE_HIT_COUNT`) via a queued command, so
/// [`build_procedural_material`]'s signature doesn't grow a registry
/// parameter. A headless/test app without the metrics resource no-ops.
fn bump_texture_cache_counter(commands: &mut Commands, hit: bool) {
    commands.queue(move |world: &mut World| {
        if let Some(mut m) = world.get_resource_mut::<crate::diagnostics::MetricsRegistry>() {
            if hit {
                crate::diagnostics::samplers::texture_cache_hit(&mut m);
            } else {
                crate::diagnostics::samplers::texture_cache_miss(&mut m);
            }
        }
    });
}

/// Compose the `StandardMaterial::uv_transform` a sovereign material asks
/// for (#957): metre-space rotation and offset, then the tiles-per-metre
/// scale. `uv_offset` is authored in metres of surface, so its translation
/// is pre-multiplied by the scale; `uv_rotation` is degrees, positive
/// counter-clockwise. The scale is uniform, so rotation and scale commute
/// and the composed `T·R·S` equals the intended `S·(R·p + offset)`.
pub(crate) fn sovereign_uv_transform(settings: &SovereignMaterialSettings) -> Affine2 {
    let scale = settings.uv_scale.0;
    Affine2::from_scale_angle_translation(
        Vec2::splat(scale),
        settings.uv_rotation.0.to_radians(),
        Vec2::from_array(settings.uv_offset.0) * scale,
    )
}

/// The reflectance a procedural material wears for its `roughness` (#1732):
/// `reflectance` as built below [`ROUGH_REFLECTANCE_FROM`], the ground's
/// [`MATERIAL_REFLECTANCE`] from [`ROUGH_REFLECTANCE_AT`], and a smoothstep
/// between, so two materials a hair apart in roughness never differ by a
/// step.
///
/// **A rough dielectric gleams toward a low sun however it is coloured.**
/// Every procedural material came out of `standard_material` at the
/// renderer's default reflectance 0.5, twice the ground's 0.25 since #1467
/// (F0 0.04 against 0.01), and seen at a grazing angle toward the sun that
/// throws a broad
/// sheen in the sun's own colour over its base colour and texture alike:
/// Ashmere's fallow ridges - spine prims at roughness 0.97 wearing the
/// Ground texture - read pale grey-white from its landing at 15:00, like
/// sheets of water, brighter than the ground they lie on, and nothing in a
/// record could damp it. So a rough part wears the ground's reflectance, as
/// the owner chose on 2026-10-22 (#1758): no schema change, every world's
/// rough parts at once.
///
/// Keyed on the material's roughness as authored - the number the editor
/// shows, and the factor its baked roughness map is scaled by. Rain still
/// wets every part toward water's reflectance in `weathered.wgsl`, from
/// whichever base it has, and a metal's sheen is its base colour's whatever
/// this says.
pub(crate) fn rough_reflectance(roughness: f32, reflectance: f32) -> f32 {
    let span = ROUGH_REFLECTANCE_AT - ROUGH_REFLECTANCE_FROM;
    let t = ((roughness - ROUGH_REFLECTANCE_FROM) / span).clamp(0.0, 1.0);
    let eased = t * t * (3.0 - 2.0 * t);
    reflectance + (MATERIAL_REFLECTANCE - reflectance) * eased
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_symbios_texture::{MaterialSettings, TextureConfig};

    /// #957: the sovereign uv_transform must degrade to the pure uniform
    /// scale [`MaterialSettings::standard_material`] sets at defaults, and
    /// compose offset (metres, pre-scale) with rotation (degrees CCW) as
    /// `S·(R·p + offset)` otherwise.
    #[test]
    fn sovereign_uv_transform_composes_scale_rotation_offset() {
        use crate::pds::types::{Fp, Fp2};

        let mut s = SovereignMaterialSettings {
            uv_scale: Fp(2.0),
            ..Default::default()
        };
        assert_eq!(
            sovereign_uv_transform(&s),
            Affine2::from_scale(Vec2::splat(2.0)),
            "defaults must reproduce standard_material's from_scale exactly"
        );

        s.uv_offset = Fp2([1.5, -0.5]);
        s.uv_rotation = Fp(90.0);
        let t = sovereign_uv_transform(&s);
        // (1, 0) rotated 90° CCW → (0, 1); offset by (1.5, −0.5) metres →
        // (1.5, 0.5); at 2 tiles/metre → (3, 1).
        let p = t.transform_point2(Vec2::new(1.0, 0.0));
        assert!(
            (p - Vec2::new(3.0, 1.0)).length() < 1e-5,
            "composed transform off: {p:?}"
        );
    }

    /// The cache key must fingerprint the exact config the job will bake, at
    /// the resolution overlands chose. Upstream owns `cache_key`'s contents
    /// (and tests them); what is ours is that every procedural material bakes
    /// at `config::textures::SURFACE` square, so a wasm job and a native bake
    /// of the same config land on one entry.
    #[test]
    fn cache_key_matches_config_identity() {
        let bark = MaterialSettings {
            texture: TextureConfig::Bark(bevy_symbios_texture::bark::BarkConfig::default()),
            ..MaterialSettings::default()
        };
        let size = crate::config::textures::SURFACE;
        let key = bark
            .cache_key(size, size)
            .expect("textured settings carry a cache key");
        assert_eq!(key.kind, bark.texture.label());
        assert_eq!(key.fingerprint, bark.texture.fingerprint());
        assert_eq!((key.width, key.height), (size, size));

        assert!(
            MaterialSettings::default().cache_key(size, size).is_none(),
            "TextureConfig::None has no cache key"
        );
    }

    /// #1732: a material keeps the reflectance it was built with below
    /// roughness 0.7, wears the ground's from 0.9, and never steps between -
    /// the steepest thousandth of roughness moves it by a smoothstep's
    /// slope, 1.5 times the even share of the fall.
    #[test]
    fn a_rough_material_wears_the_grounds_reflectance() {
        let built = StandardMaterial::default().reflectance;
        assert_eq!(rough_reflectance(0.5, built), built);
        assert_eq!(rough_reflectance(ROUGH_REFLECTANCE_FROM, built), built);
        assert_eq!(
            rough_reflectance(ROUGH_REFLECTANCE_AT, built),
            MATERIAL_REFLECTANCE
        );
        assert_eq!(rough_reflectance(1.0, built), MATERIAL_REFLECTANCE);
        let (mut last, mut steepest) = (built, 0.0f32);
        for step in 0..=1000 {
            let roughness = step as f32 / 1000.0;
            let now = rough_reflectance(roughness, built);
            assert!(now <= last, "the reflectance rose at roughness {roughness}");
            steepest = steepest.max(last - now);
            last = now;
        }
        let even = (built - MATERIAL_REFLECTANCE)
            / 1000.0
            / (ROUGH_REFLECTANCE_AT - ROUGH_REFLECTANCE_FROM);
        assert!(
            steepest <= even * 1.5 + 1e-6,
            "the reflectance fell {steepest} in a thousandth of roughness"
        );
    }

    /// The reflectance a material built through [`build_procedural_material`]
    /// at `roughness` comes out with.
    fn built_reflectance(roughness: f32) -> f32 {
        use crate::pds::types::Fp;
        use bevy::ecs::system::RunSystemOnce;

        let mut world = World::new();
        world.init_resource::<Assets<WeatheredMaterial>>();
        world.init_resource::<Assets<Image>>();
        world.insert_resource(TextureCache::memory(4));
        world
            .run_system_once(
                move |mut commands: Commands,
                      mut materials: ResMut<Assets<WeatheredMaterial>>,
                      mut images: ResMut<Assets<Image>>,
                      mut cache: ResMut<TextureCache>| {
                    let settings = SovereignMaterialSettings {
                        roughness: Fp(roughness),
                        ..Default::default()
                    };
                    let handle = build_procedural_material(
                        &mut commands,
                        &mut materials,
                        &mut images,
                        &mut cache,
                        &settings,
                        true,
                    );
                    materials
                        .get(&handle)
                        .expect("the material was added")
                        .base
                        .reflectance
                },
            )
            .expect("the builder runs")
    }

    /// #1732, where it is applied: every catalogue construct, primitive,
    /// foliage card and avatar part is built through
    /// [`build_procedural_material`], and one as rough as Ashmere's fallow
    /// ridges comes out wearing the ground's reflectance while a smooth one
    /// keeps the renderer's default.
    #[test]
    fn every_procedural_material_takes_the_rough_rule() {
        assert_eq!(built_reflectance(0.97), MATERIAL_REFLECTANCE);
        assert_eq!(
            built_reflectance(0.5),
            StandardMaterial::default().reflectance
        );
    }
}
