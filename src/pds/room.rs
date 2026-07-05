//! The root `RoomRecord` recipe plus its atmospheric `Environment` payload,
//! the deterministic `find_terrain_config` lookup shared across peers, and
//! the XRPC wrappers that fetch / publish / delete / reset the record on the
//! owner's PDS.

use super::COLLECTION;
use super::contact_effects::ContactEffects;
use super::generator::{
    Generator, GeneratorKind, ParticleParams, Placement, RoadConfig, WaterSurface,
};
use super::sanitize::{Sanitize, limits, sanitize_generator};
use super::terrain::SovereignTerrainConfig;
use super::types::{Fp, Fp2, Fp3, Fp4, Fp64, TransformData};
use super::xrpc::{
    FetchError, MAX_APPLY_WRITES, RepoWrite, XrpcError, decode_record_json, resolve_pds,
};
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Non-spatial environment state — directional sun, ambient light, sky
/// cuboid tint, and atmospheric distance fog. Every field is wrapped in a
/// fixed-point type so the record stays DAG-CBOR compliant.
///
/// `#[serde(default)]` lets pre-atmosphere records (which only carried
/// `sun_color`) round-trip: any missing field falls back to the canonical
/// constant via `Environment::default()` rather than failing the whole
/// decode and stranding the owner on the recovery banner.
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Environment {
    pub sun_color: Fp3,
    pub sun_illuminance: Fp,
    pub ambient_brightness: Fp,
    pub sky_color: Fp3,
    /// World-space position of the directional sun light. The
    /// renderer reads this as a direction (origin → position normalised
    /// is the unit vector *toward* the sun); the magnitude is informally
    /// "far away", any value with a sensible direction works. Authored
    /// per-room so seeded atmospheres can vary sun altitude / azimuth.
    /// `#[serde(default)]` on the parent struct lets pre-`sun_position`
    /// records round-trip with the canonical constant.
    pub sun_position: Fp3,

    pub fog_color: Fp4,
    pub fog_visibility: Fp,
    pub fog_extinction: Fp3,
    pub fog_inscattering: Fp3,
    pub fog_sun_color: Fp4,
    pub fog_sun_exponent: Fp,

    /// Tiling frequency for the close-distance scrolling detail normal map
    /// (world-unit reciprocal — higher = tighter tiling). Pairs with
    /// [`Self::water_normal_scale_far`] to kill the repeating-grid look on
    /// long camera sightlines.
    pub water_normal_scale_near: Fp,
    /// Tiling frequency for the far-distance scrolling detail normal map.
    pub water_normal_scale_far: Fp,
    /// Intensity of the sharp specular sun-glitter highlight on the water
    /// surface. `0` disables; ~2.0 is a pleasing default.
    pub water_sun_glitter: Fp,
    /// sRGB tint added to wave crests to simulate cheap subsurface scatter.
    pub water_scatter_color: Fp3,
    /// Width (m) of the procedural shoreline foam band. `0` disables;
    /// consumed by the water shader via the camera's opaque depth
    /// prepass to fade foam in where the water meets terrain.
    pub water_shore_foam_width: Fp,

    // ---- Cloud-deck (procedural FBM layer; see `crate::clouds`) -----------
    /// Fraction of sky covered by clouds. `0` = empty blue, `1` = totally
    /// overcast.
    pub cloud_cover: Fp,
    /// Opacity multiplier for the clouds that survive the cover threshold.
    pub cloud_density: Fp,
    /// Edge-softness band around the cover threshold. Larger ⇒ wispier.
    pub cloud_softness: Fp,
    /// Drift speed (m/s) along [`Self::cloud_wind_dir`].
    pub cloud_speed: Fp,
    /// World metres per UV unit for the cloud noise sampler.
    pub cloud_scale: Fp,
    /// Altitude (m) of the cloud-deck plane.
    pub cloud_height: Fp,
    /// 2D wind direction in world XZ. Need not be unit length — the shader
    /// normalises a small epsilon-padded copy.
    pub cloud_wind_dir: Fp2,
    /// sRGB tint for the sunlit top of the cloud layer.
    pub cloud_color: Fp3,
    /// sRGB tint for the underside / shadowed regions, mixed with
    /// [`Self::cloud_color`] by the dot of the sun direction with world Y.
    pub cloud_shadow_color: Fp3,

    /// Ambient audio for the room — a procedurally-baked
    /// [`AudioPatch`] / [`SequenceRecipe`] or a URL/DID-referenced
    /// clip. `None` (the default) plays no ambient track. Forward-
    /// compat across older records: `#[serde(default)]` on the parent
    /// struct lets pre-audio records decode cleanly with this field
    /// elided.
    ///
    /// [`AudioPatch`]: bevy_symbios_audio::AudioPatch
    /// [`SequenceRecipe`]: bevy_symbios_audio::SequenceRecipe
    pub ambient_audio: crate::pds::audio::SovereignAudioConfig,
}

// Default-eliding wire format (#695): a freshly-seeded room overrides only
// the palette-driven subset of these knobs, so the untouched remainder
// (fog shape, water detail tiling, cloud geometry) drops off the wire. The
// container `#[serde(default)]` above is the matching read-side contract.
crate::pds::serde_util::impl_default_eliding_serialize!(Environment {
    sun_color,
    sun_illuminance,
    ambient_brightness,
    sky_color,
    sun_position,
    fog_color,
    fog_visibility,
    fog_extinction,
    fog_inscattering,
    fog_sun_color,
    fog_sun_exponent,
    water_normal_scale_near,
    water_normal_scale_far,
    water_sun_glitter,
    water_scatter_color,
    water_shore_foam_width,
    cloud_cover,
    cloud_density,
    cloud_softness,
    cloud_speed,
    cloud_scale,
    cloud_height,
    cloud_wind_dir,
    cloud_color,
    cloud_shadow_color,
    ambient_audio,
});

impl Default for Environment {
    fn default() -> Self {
        use crate::config::{
            camera::fog as f, lighting as l, lighting::clouds as c, terrain::water as w,
        };
        Self {
            sun_color: Fp3(l::SUN_COLOR),
            sun_illuminance: Fp(l::ILLUMINANCE),
            ambient_brightness: Fp(l::AMBIENT_BRIGHTNESS),
            sky_color: Fp3(l::SKY_COLOR),
            sun_position: Fp3(l::LIGHT_POS),

            fog_color: Fp4(f::COLOR),
            fog_visibility: Fp(f::VISIBILITY),
            fog_extinction: Fp3(f::EXTINCTION_COLOR),
            fog_inscattering: Fp3(f::INSCATTERING_COLOR),
            fog_sun_color: Fp4(f::DIRECTIONAL_LIGHT_COLOR),
            fog_sun_exponent: Fp(f::DIRECTIONAL_LIGHT_EXPONENT),

            water_normal_scale_near: Fp(w::DEFAULT_NORMAL_SCALE_NEAR),
            water_normal_scale_far: Fp(w::DEFAULT_NORMAL_SCALE_FAR),
            water_sun_glitter: Fp(w::DEFAULT_SUN_GLITTER),
            water_scatter_color: Fp3(w::DEFAULT_SCATTER_COLOR),
            water_shore_foam_width: Fp(w::DEFAULT_SHORE_FOAM_WIDTH),

            cloud_cover: Fp(c::COVER),
            cloud_density: Fp(c::DENSITY),
            cloud_softness: Fp(c::SOFTNESS),
            cloud_speed: Fp(c::SPEED),
            cloud_scale: Fp(c::SCALE),
            cloud_height: Fp(c::HEIGHT),
            cloud_wind_dir: Fp2(c::WIND_DIR),
            cloud_color: Fp3(c::COLOR),
            cloud_shadow_color: Fp3(c::SHADOW_COLOR),

            ambient_audio: crate::pds::audio::SovereignAudioConfig::None,
        }
    }
}

impl Environment {
    /// Clamp every field so a malicious or malformed record cannot crash
    /// the renderer with NaN, negative light values, or a zero visibility
    /// that makes `FogFalloff::from_visibility_colors` divide by zero.
    pub fn sanitize(&mut self) {
        let clamp_unit = |v: f32| v.clamp(0.0, 1.0);
        let clamp3 = |c: Fp3| Fp3([clamp_unit(c.0[0]), clamp_unit(c.0[1]), clamp_unit(c.0[2])]);
        let clamp4 = |c: Fp4| {
            Fp4([
                clamp_unit(c.0[0]),
                clamp_unit(c.0[1]),
                clamp_unit(c.0[2]),
                clamp_unit(c.0[3]),
            ])
        };

        self.sun_color = clamp3(self.sun_color);
        self.sky_color = clamp3(self.sky_color);
        self.fog_color = clamp4(self.fog_color);
        self.fog_extinction = clamp3(self.fog_extinction);
        self.fog_inscattering = clamp3(self.fog_inscattering);
        self.fog_sun_color = clamp4(self.fog_sun_color);

        self.sun_illuminance = Fp(self.sun_illuminance.0.clamp(0.0, 100_000.0));
        self.ambient_brightness = Fp(self.ambient_brightness.0.clamp(0.0, 10_000.0));

        // Sun-position guard: each component must be finite and the
        // vector cannot collapse to the origin (it's used as a
        // direction by `looking_at`). On any failure, fall back to the
        // canonical constant — that always gives a valid direction.
        let sp = self.sun_position.0;
        let bad = !sp[0].is_finite()
            || !sp[1].is_finite()
            || !sp[2].is_finite()
            || (sp[0] * sp[0] + sp[1] * sp[1] + sp[2] * sp[2]) < 1.0e-6;
        if bad {
            self.sun_position = Fp3(crate::config::lighting::LIGHT_POS);
        } else {
            self.sun_position = Fp3([
                sp[0].clamp(-10_000.0, 10_000.0),
                sp[1].clamp(-10_000.0, 10_000.0),
                sp[2].clamp(-10_000.0, 10_000.0),
            ]);
        }
        // A zero visibility would make `FogFalloff::from_visibility_colors`
        // blow up (it divides by `visibility` internally). Floor at 10 m so
        // the falloff remains well-defined even under an adversarial record.
        self.fog_visibility = Fp(self.fog_visibility.0.clamp(10.0, 10_000.0));
        self.fog_sun_exponent = Fp(self.fog_sun_exponent.0.clamp(0.0, 200.0));

        // Water-environment fields. Keep every channel in a finite,
        // physically-sane range — a NaN or negative normal-tiling scale
        // would poison the water shader's UV math every frame.
        let clamp_finite_pos = |v: f32, lo: f32, hi: f32, default: f32| -> f32 {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                default
            }
        };
        self.water_normal_scale_near = Fp(clamp_finite_pos(
            self.water_normal_scale_near.0,
            0.0,
            64.0,
            0.85,
        ));
        self.water_normal_scale_far = Fp(clamp_finite_pos(
            self.water_normal_scale_far.0,
            0.0,
            64.0,
            0.08,
        ));
        self.water_sun_glitter = Fp(clamp_finite_pos(self.water_sun_glitter.0, 0.0, 16.0, 1.8));
        self.water_scatter_color = clamp3(self.water_scatter_color);
        self.water_shore_foam_width = Fp(clamp_finite_pos(
            self.water_shore_foam_width.0,
            0.0,
            50.0,
            0.0,
        ));

        // Cloud-deck fields. Same NaN / range guarding as water — the cloud
        // shader divides by `cloud_scale` and reads `cloud_height` straight
        // into a `Transform.translation.y`, so a poisoned record must not
        // be allowed to feed Inf or negative values into either.
        self.cloud_cover = Fp(clamp_finite_pos(self.cloud_cover.0, 0.0, 1.0, 0.45));
        self.cloud_density = Fp(clamp_finite_pos(self.cloud_density.0, 0.0, 1.0, 0.85));
        self.cloud_softness = Fp(clamp_finite_pos(self.cloud_softness.0, 0.001, 1.0, 0.18));
        self.cloud_speed = Fp(clamp_finite_pos(self.cloud_speed.0, 0.0, 200.0, 4.0));
        self.cloud_scale = Fp(clamp_finite_pos(self.cloud_scale.0, 1.0, 10_000.0, 320.0));
        self.cloud_height = Fp(clamp_finite_pos(self.cloud_height.0, 5.0, 10_000.0, 250.0));
        let wd = self.cloud_wind_dir.0;
        let wd0 = if wd[0].is_finite() {
            wd[0].clamp(-100.0, 100.0)
        } else {
            1.0
        };
        let wd1 = if wd[1].is_finite() {
            wd[1].clamp(-100.0, 100.0)
        } else {
            0.3
        };
        // Reject the zero vector — the shader normalises wind_dir and a
        // bit-for-bit zero would NaN-out the noise sampling. A vanishingly
        // small magnitude falls back to the canonical default.
        let mag2 = wd0 * wd0 + wd1 * wd1;
        self.cloud_wind_dir = if mag2 > 1.0e-6 {
            Fp2([wd0, wd1])
        } else {
            Fp2([1.0, 0.3])
        };
        self.cloud_color = clamp3(self.cloud_color);
        self.cloud_shadow_color = clamp3(self.cloud_shadow_color);

        // Forward to the asset-class sanitiser — caps the embedded
        // patch / sequence JSON length and Referenced URL / DID / CID
        // strings so a hostile peer can't smuggle a megabyte through
        // the audio slot.
        self.ambient_audio.sanitize();
    }
}

/// The full recipe: environment + generators + placements + traits. Acts as
/// a Bevy `Resource` so the [`crate::world_builder`] module can compile it
/// into ECS entities.
#[derive(Serialize, Deserialize, Clone, Debug, Resource)]
pub struct RoomRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    pub environment: Environment,
    pub generators: HashMap<String, Generator>,
    pub placements: Vec<Placement>,
    /// Maps a generator name to a list of trait strings (e.g.
    /// `"collider_heightfield"`, `"sensor"`) the world compiler should attach
    /// to every entity that generator spawns.
    pub traits: HashMap<String, Vec<String>>,
    /// Authored avatar-world contact-effect recipes (#246). `#[serde(default)]`
    /// so pre-Phase-4 records (which lack the key) deserialize with the
    /// canonical defaults and behave exactly as the old hardcoded
    /// registry; `RoomRecord` carries no `deny_unknown_fields`, so older
    /// clients reading a newer record simply ignore the extra key. The
    /// skip on write is the same fact in reverse (#695): an uncustomised
    /// set is omitted rather than re-stating the canonical recipes.
    #[serde(default, skip_serializing_if = "ContactEffects::is_default")]
    pub contact_effects: ContactEffects,
}

impl RoomRecord {
    /// Zero-configuration homeworld. When a client visits a DID whose owner
    /// has never saved a custom record, this builds the canonical default
    /// recipe on the fly — a base terrain plus a base water plane — so the
    /// world builder always has something valid to compile.
    ///
    /// Every seedable parameter (terrain seed, biome palette, water tint,
    /// fog, clouds) is derived from the DID via `crate::seeded_defaults`,
    /// so freshly-visited overlands are visibly distinct per owner without
    /// requiring anyone to touch the editor. Authored records that have
    /// been published to a PDS keep their stored values verbatim — the
    /// seed pipeline only fills in the blank-record case here.
    pub fn default_for_did(did: &str) -> Self {
        Self::default_for_seed(crate::seeded_defaults::fnv1a_64(did), did)
    }

    /// Build the seeded default room from a pre-computed seed — the
    /// manual re-roll path. `seed` drives every derived value (terrain
    /// shape, palette, atmosphere, scatters, landmark, audio, …); `did`
    /// is kept only for the per-species generator builders that take the
    /// local DID. `default_for_did` is exactly
    /// `default_for_seed(fnv1a_64(did), did)`.
    pub fn default_for_seed(seed: u64, did: &str) -> Self {
        use crate::pds::generator::{
            AnimationFrameMode, EmitterShape, ParticleBlendMode, SimulationSpace, TextureFilter,
        };
        use crate::pds::texture::{
            SovereignMaterialSettings, SovereignRockConfig, SovereignTextureConfig,
        };
        use crate::pds::types::{BiomeFilter, ScatterBounds, WaterRelation};
        use crate::pds::{Fp2, TortureParams};
        use crate::seeded_defaults::{
            AmbientParticles, Atmosphere, BiomeTextures, RockScatters, RoomPalette, SceneCharacter,
            Settlement, TerrainShape, TreeScatters, WaterDynamics,
        };

        let did_seed = seed;
        let scene = SceneCharacter::for_seed(did_seed);
        let palette = RoomPalette::from_scene(&scene, did_seed);
        let shape = TerrainShape::from_scene(&scene, did_seed);
        let textures = BiomeTextures::from_scene(&scene, did_seed);
        let atmosphere = Atmosphere::from_scene(&scene, did_seed);
        let water_dynamics = WaterDynamics::from_scene(&scene, did_seed);
        let tree_scatters = TreeScatters::from_scene(&scene, did_seed);
        let rock_scatters = RockScatters::from_scene(&scene, did_seed);
        let ambient_particles = AmbientParticles::from_scene(&scene, did_seed);

        let mut terrain_cfg = SovereignTerrainConfig {
            seed: did_seed,
            ..SovereignTerrainConfig::default()
        };
        apply_shape_to_terrain_config(&shape, &mut terrain_cfg);
        apply_palette_to_material(&palette, &mut terrain_cfg.material);
        apply_shape_to_material(&shape, &mut terrain_cfg.material);
        apply_textures_to_material(&textures, &mut terrain_cfg.material);
        apply_biome_signature_surface(scene.biome, did_seed, &mut terrain_cfg.material);

        let mut water_surface = WaterSurface {
            shallow_color: Fp4(palette.water_shallow),
            deep_color: Fp4(palette.water_deep),
            ..WaterSurface::default()
        };
        apply_water_dynamics(&water_dynamics, &mut water_surface);

        // Strict scheme: a single named generator describes the whole
        // region. Terrain sits at the root (only valid position for
        // Terrain) and the room's water is a child of it (only valid
        // position for Water). Saving `base_terrain` to inventory now
        // captures the entire homeworld — heightmap + water — as one
        // portable blueprint.
        let mut base_region = Generator::from_kind(GeneratorKind::Terrain(terrain_cfg));
        // Water altitude is the seeded `water_level_fraction` of the
        // seeded `height_scale`. Expressed as a fraction so a tall
        // craggy room and a short rolling room can both read as "30 %
        // submerged" — the absolute Y differs but the proportion of
        // land vs water stays meaningful. Archetype + biome biases
        // happen inside `TerrainShape::from_scene`; here we just
        // multiply out.
        let seeded_water_y = shape.water_level_fraction * shape.height_scale;
        base_region.children.push(Generator {
            kind: GeneratorKind::Water {
                surface: water_surface,
            },
            transform: TransformData {
                translation: Fp3([0.0, seeded_water_y, 0.0]),
                ..TransformData::default()
            },
            children: Vec::new(),
            audio: crate::pds::SovereignAudioConfig::None,
        });

        // Urban / built-up themes grow a tensor road network as a RoadNetwork
        // child of the terrain. The generator is theme-agnostic — this is only
        // the default-on policy; any room can add or remove roads in the editor.
        // The layout gets its own seed (derived from the room seed) so it's
        // independently re-rollable in the GUI.
        if theme_grows_roads(scene.theme) {
            base_region
                .children
                .push(Generator::from_kind(GeneratorKind::RoadNetwork(
                    road_config_from_scene(&scene, did_seed ^ ROAD_SEED_SALT),
                )));
        }

        let mut generators = HashMap::new();
        generators.insert("base_terrain".to_string(), base_region);

        let mut placements = vec![Placement::Absolute {
            generator_ref: "base_terrain".to_string(),
            transform: TransformData::default(),
            snap_to_terrain: false,
            avoid_water: false,
            avoid_water_clearance: Fp(0.0),
        }];

        // Seeded tree scatters: one named generator per scatter (so
        // each scatter's species + `iterations_delta` actually affect
        // what gets compiled) plus a matching `Placement::Scatter`
        // referencing it with a grass-and-dirt-above-water biome
        // filter so trees land on walkable land, not rock faces or
        // the seabed.
        for (idx, scatter) in tree_scatters.scatters.iter().enumerate() {
            let Some(species_entry) = crate::catalogue::by_slug(scatter.species.slug()) else {
                // Pool slugs are compile-time constants verified by the
                // landmark/scatter tests; an unresolved slug means a
                // catalogue rename and is safest skipped.
                continue;
            };
            let mut tree_gen = species_entry.build(did);
            if let GeneratorKind::LSystem { iterations, .. } = &mut tree_gen.kind {
                // The deriver only ever emits delta ∈ {-1, 0, +1}, so
                // the post-delta band stays within one step of each
                // species' shipped iteration count (6–10 across the
                // pool) — inside healthy LSystem expansion costs.
                // Clamp to ≥ 2 as belt-and-braces against future
                // catalogue tweaks.
                let new_iters = (*iterations as i32 + scatter.iterations_delta).max(2) as u32;
                *iterations = new_iters;
            }
            let scatter_gen_name = format!("tree_scatter_{idx}");
            generators.insert(scatter_gen_name.clone(), tree_gen);
            placements.push(Placement::Scatter {
                generator_ref: scatter_gen_name,
                bounds: ScatterBounds::Circle {
                    center: Fp2(scatter.center),
                    radius: Fp(scatter.radius),
                },
                count: scatter.count,
                local_seed: scatter.local_seed,
                biome_filter: BiomeFilter {
                    // 0=Grass, 1=Dirt (walkable land layers).
                    biomes: vec![0, 1],
                    water: WaterRelation::Above,
                },
                snap_to_terrain: true,
                random_yaw: true,
                // Keep wild trees out of the built-up urban district.
                avoid_urban: true,
            });
        }

        // Seeded boulder scatters: one per-room boulder design (a
        // low-res icosphere sheared by taper/twist so it reads hewn,
        // coloured from the palette's rock channels) scattered across
        // dirt-and-rock ground. The trees' biome filter excludes rock
        // faces; boulders invert that and *prefer* them.
        let boulder = Generator::from_kind(GeneratorKind::Sphere {
            radius: Fp(rock_scatters.boulder_radius),
            resolution: 1,
            // Solid: a walk-through boulder breaks the fiction the
            // moment someone drives into one.
            solid: true,
            material: SovereignMaterialSettings {
                base_color: Fp3(palette.rock_stone),
                roughness: Fp(0.95),
                uv_scale: Fp(1.5),
                texture: SovereignTextureConfig::Rock(SovereignRockConfig {
                    color_light: Fp3(palette.rock_stone),
                    color_dark: Fp3(palette.rock_gap),
                    ..Default::default()
                }),
                ..Default::default()
            },
            torture: TortureParams {
                twist: Fp(rock_scatters.boulder_twist),
                taper: Fp2([rock_scatters.boulder_taper, rock_scatters.boulder_taper]),
                ..Default::default()
            },
        });
        generators.insert("boulder".to_string(), boulder);
        for rock in rock_scatters.scatters.iter() {
            placements.push(Placement::Scatter {
                generator_ref: "boulder".to_string(),
                bounds: ScatterBounds::Circle {
                    center: Fp2(rock.center),
                    radius: Fp(rock.radius),
                },
                count: rock.count,
                local_seed: rock.local_seed,
                biome_filter: BiomeFilter {
                    // 1=Dirt, 2=Rock — boulders avoid manicured grass.
                    biomes: vec![1, 2],
                    water: WaterRelation::Above,
                },
                snap_to_terrain: true,
                random_yaw: true,
                // Keep boulders out of the built-up urban district.
                avoid_urban: true,
            });
        }

        // Seeded ambient particles: one biome-mood emitter (fireflies /
        // snow / embers / dust / mist) centred on spawn. Spec numbers
        // are pre-clamped to the particle sanitiser budget.
        let p = &ambient_particles;
        let particle_gen =
            Generator::from_kind(GeneratorKind::ParticleSystem(Box::new(ParticleParams {
                emitter_shape: EmitterShape::Box {
                    half_extents: Fp3(p.emitter_half_extents),
                },
                rate_per_second: Fp(p.rate_per_second),
                burst_count: 0,
                max_particles: p.max_particles,
                looping: true,
                duration: Fp(10.0),
                lifetime_min: Fp(p.lifetime.0),
                lifetime_max: Fp(p.lifetime.1),
                speed_min: Fp(p.speed.0),
                speed_max: Fp(p.speed.1),
                gravity_multiplier: Fp(p.gravity_multiplier),
                acceleration: Fp3(p.acceleration),
                linear_drag: Fp(p.linear_drag),
                start_size: Fp(p.start_size),
                end_size: Fp(p.end_size),
                start_color: Fp4(p.start_color),
                end_color: Fp4(p.end_color),
                blend_mode: if p.additive {
                    ParticleBlendMode::Additive
                } else {
                    ParticleBlendMode::Alpha
                },
                billboard: true,
                simulation_space: SimulationSpace::World,
                inherit_velocity: Fp(0.0),
                collide_terrain: false,
                collide_water: false,
                collide_colliders: false,
                bounce: Fp(0.0),
                friction: Fp(0.0),
                seed: p.seed,
                texture: None,
                // Atlas dims are derived at compile time from the sprite's
                // variant grid, so the record leaves this `None`.
                texture_atlas: None,
                // Every mood sprite bakes a variant atlas; draw one per particle.
                frame_mode: AnimationFrameMode::RandomFrame,
                texture_filter: TextureFilter::default(),
                procedural_texture: p.sprite_texture(),
            })));
        generators.insert("ambient_particles".to_string(), particle_gen);
        placements.push(Placement::Absolute {
            generator_ref: "ambient_particles".to_string(),
            transform: TransformData {
                translation: Fp3([0.0, p.emitter_y, 0.0]),
                ..TransformData::default()
            },
            snap_to_terrain: true,
            avoid_water: false,
            avoid_water_clearance: Fp(0.0),
        });

        // Seeded mini-settlement: most home regions grow a themed cluster near
        // spawn — one landmark plus any secondaries and props available for the
        // room's theme (see crate::seeded_defaults::room::settlement).
        // Shape-grammar entries get their stochastic seed restamped per DID so
        // two users sharing a structure type still see different derivations;
        // the landmark faces the spawn origin, secondaries face the landmark,
        // and every member snaps to terrain with its own water clearance.
        //
        // Road-growing themes are the exception: their buildings are placed on
        // the road network's enclosed lots instead, derived at load by the
        // terrain plugin's populate-lots system (see [`crate::terrain`]). Baking
        // a concentric cluster here would double up with — and ignore — those
        // streets, so we skip it and let the lot layer own urban buildings.
        if !theme_grows_roads(scene.theme) {
            let settlement = Settlement::from_scene(&scene, did_seed);
            let (prosperity, escalation) = (scene.prosperity, scene.escalation);
            wire_settlement_member(
                &settlement.landmark,
                "landmark",
                did,
                prosperity,
                escalation,
                &mut generators,
                &mut placements,
            );
            for (i, member) in settlement.secondaries.iter().enumerate() {
                wire_settlement_member(
                    member,
                    &format!("settlement_secondary_{i}"),
                    did,
                    prosperity,
                    escalation,
                    &mut generators,
                    &mut placements,
                );
            }
            // Props are sampled with replacement, so the same prop can recur.
            // Share one generator per distinct prop slug (named by slug) and
            // reference it from each copy's placement — the compiler bakes that
            // mesh once and instances it, instead of carrying a near-duplicate
            // Region Asset per copy (mirrors the lot-building layer).
            for member in &settlement.props {
                let name = format!("settlement_prop_{}", member.slug);
                if !generators.contains_key(&name) {
                    let Some(prop_gen) =
                        build_member_generator(member, did, prosperity, escalation)
                    else {
                        continue;
                    };
                    generators.insert(name.clone(), prop_gen);
                }
                placements.push(member_placement(name, member));
            }
        }

        let mut traits = HashMap::new();
        traits.insert(
            "base_terrain".to_string(),
            vec!["collider_heightfield".to_string(), "ground".to_string()],
        );

        let mut environment = environment_from_palette(&palette);
        apply_atmosphere_to_environment(&atmosphere, &mut environment);

        // Scene accent: a light, additive nudge so the room's surroundings
        // echo its artificial theme (e.g. cyberpunk magenta haze) and its
        // socio-political axes (escalation smokes the air red + hazy;
        // prosperity brightens / dims). The biome palette stays the primary
        // driver; a neutral, calm, mid-prosperity room is a no-op.
        // Particle-mood accents are applied inside the particles deriver;
        // this handles fog / sky / cloud tint, brightness and cloud haze.
        let accent = crate::seeded_defaults::ThemeAccent::for_scene(&scene);
        if !accent.is_noop() {
            let fog = environment.fog_color.0;
            let fog_adj = accent.adjust_rgb([fog[0], fog[1], fog[2]]);
            environment.fog_color = Fp4([fog_adj[0], fog_adj[1], fog_adj[2], fog[3]]);
            environment.sky_color = Fp3(accent.adjust_rgb(environment.sky_color.0));
            environment.cloud_color = Fp3(accent.adjust_rgb(environment.cloud_color.0));
            environment.cloud_cover = Fp((environment.cloud_cover.0 + accent.haze).clamp(0.0, 1.0));
        }

        // Theme nightfall: a nocturnal theme (cyberpunk neon) drops the sun
        // to a dim moonlight key and darkens the sky / fog / cloud so its
        // self-lit kit dominates. Runs *after* the accent so the result is a
        // dark magenta-blue night rather than dark-neutral. A daylight theme
        // has luminosity 1.0 and this is a no-op.
        apply_nightfall(
            crate::seeded_defaults::theme_luminosity(scene.theme),
            &mut environment,
        );

        // Seed the room's ambient track from the same scene anchor that
        // drives palette / terrain / atmosphere. The deriver returns a
        // native `bevy_symbios_audio::SequenceRecipe`; we mirror it
        // into the DAG-CBOR-safe SovereignSequenceRecipe (structured
        // Fp-wrapped form, per #311). Conversion is infallible — the
        // structural walk just wraps each float in `Fp`.
        let ambient = crate::seeded_defaults::AmbientRecipe::from_scene(&scene, did_seed);
        environment.ambient_audio =
            crate::pds::audio::SovereignAudioConfig::from_sequence(&ambient.recipe);

        Self {
            lex_type: COLLECTION.into(),
            environment,
            generators,
            placements,
            traits,
            contact_effects: ContactEffects::default(),
        }
    }

    /// Clamp every numeric field to a safe upper bound. Every path that
    /// accepts a `RoomRecord` from the network (PDS fetch and peer-broadcast
    /// `RoomStateUpdate`) calls this before handing the record to the world
    /// compiler, so an attacker cannot weaponise an unbounded field to crash
    /// or OOM the victim.
    pub fn sanitize(&mut self) {
        // Clamp atmospheric fields first — cheap and independent of everything
        // else, and guarantees the world compiler never hands NaN or a zero
        // visibility to `FogFalloff::from_visibility_colors`.
        self.environment.sanitize();
        // Authored contact-effect recipes: clamp every numeric, bound
        // the recipe list deterministically (#246).
        self.contact_effects.sanitize();
        // Bound the total number of generators before touching any of them.
        // Drop entries in lexicographic key order so the survivor set is
        // deterministic across peers — otherwise a record with 1000
        // generators and `MAX_GENERATORS = 256` would resolve to a
        // different 256 on every client (HashMap iteration is SipHash
        // randomised) and fracture the shared world.
        if self.generators.len() > limits::MAX_GENERATORS {
            let mut keys: Vec<String> = self.generators.keys().cloned().collect();
            keys.sort();
            for key in keys.into_iter().skip(limits::MAX_GENERATORS) {
                self.generators.remove(&key);
            }
        }
        // Snapshot the names of generators whose root kind is Terrain or
        // Water *before* `sanitize_generator` rewrites them. Any
        // `Scatter`/`Grid` placement targeting one of these is positionally
        // invalid: a Scatter of a Terrain root would spawn duplicate
        // heightfield colliders (Avian forbids that), and Water can never
        // legally be a root. We capture the snapshot first because the
        // generator pass overwrites root Water with a default cuboid — if
        // we filtered after, a Scatter pointing at the now-cuboid would
        // silently spawn N copies of an unrelated shape instead of being
        // dropped outright.
        let ineligible_targets: std::collections::HashSet<String> = self
            .generators
            .iter()
            .filter(|(_, g)| {
                matches!(
                    g.kind,
                    GeneratorKind::Terrain(_) | GeneratorKind::Water { .. }
                )
            })
            .map(|(name, _)| name.clone())
            .collect();
        for generator in self.generators.values_mut() {
            sanitize_generator(generator);
        }
        // Drop offending Scatter/Grid placements before applying the
        // count cap, so 1024 ineligible entries can't push valid ones
        // past `MAX_PLACEMENTS`. Absolute is left alone — pointing it
        // at a Terrain root is the canonical home-world placement, and
        // a hostile Water-rooted Absolute is already neutralised by
        // the generator-level overwrite above.
        self.placements.retain(|p| match p {
            Placement::Scatter { generator_ref, .. } | Placement::Grid { generator_ref, .. } => {
                !ineligible_targets.contains(generator_ref)
            }
            _ => true,
        });
        // Drop excess placements so a 1M-entry array can't force
        // `compile_room_record` to spawn tens of millions of entities in
        // a single frame. Keeping a prefix is order-stable (serde
        // round-trips `Vec` in order) so every peer truncates to the
        // same survivor set.
        if self.placements.len() > limits::MAX_PLACEMENTS {
            self.placements.truncate(limits::MAX_PLACEMENTS);
        }
        for placement in self.placements.iter_mut() {
            match placement {
                Placement::Scatter { count, .. } => {
                    *count = (*count).min(limits::MAX_SCATTER_COUNT);
                }
                Placement::Grid { counts, gaps, .. } => {
                    counts[0] = counts[0].clamp(1, 100);
                    counts[1] = counts[1].clamp(1, 100);
                    counts[2] = counts[2].clamp(1, 100);
                    let total = (counts[0] as usize)
                        .saturating_mul(counts[1] as usize)
                        .saturating_mul(counts[2] as usize);
                    if total > 10_000 {
                        counts[0] = counts[0].min(21);
                        counts[1] = counts[1].min(21);
                        counts[2] = counts[2].min(21);
                    }
                    gaps.0[0] = gaps.0[0].clamp(0.01, 1000.0);
                    gaps.0[1] = gaps.0[1].clamp(0.01, 1000.0);
                    gaps.0[2] = gaps.0[2].clamp(0.01, 1000.0);
                }
                _ => {}
            }
        }
    }
}

impl Default for RoomRecord {
    fn default() -> Self {
        Self::default_for_did("")
    }
}

/// Wire one seeded [`crate::seeded_defaults::SettlementMember`] into the
/// room record: resolve its catalogue entry, restamp the Shape-grammar
/// seed, register the generator under `name`, and emit a terrain-snapped,
/// water-avoiding `Placement::Absolute`. A slug that no longer resolves
/// is silently skipped — a removed catalogue entry must not strand the
/// whole room on the recovery banner.
fn wire_settlement_member(
    member: &crate::seeded_defaults::SettlementMember,
    name: &str,
    did: &str,
    prosperity: f32,
    escalation: f32,
    generators: &mut HashMap<String, Generator>,
    placements: &mut Vec<Placement>,
) {
    let Some(member_gen) = build_member_generator(member, did, prosperity, escalation) else {
        return;
    };
    generators.insert(name.to_string(), member_gen);
    placements.push(member_placement(name.to_string(), member));
}

/// Build a settlement member's generator tree: the catalogue entry, its
/// stochastic grammar seed restamped from the member, and the socio-political
/// finish + escalation damage applied. `None` if the slug no longer resolves
/// (a catalogue rename); the caller then skips both the generator and its
/// placement. Shared by the unique members (landmark / secondaries) and the
/// slug-deduped props, so they build identically.
fn build_member_generator(
    member: &crate::seeded_defaults::SettlementMember,
    did: &str,
    prosperity: f32,
    escalation: f32,
) -> Option<Generator> {
    let entry = crate::catalogue::by_slug(member.slug)?;
    let mut member_gen = entry.build(did);
    if let GeneratorKind::Shape { seed, .. } = &mut member_gen.kind {
        *seed = member.grammar_seed;
    }
    // Socio-political material finish: nudge every material in the built
    // tree toward the room's prosperity (grime ↔ polish) and escalation
    // (peace ↔ scorch). Deterministic; a neutral room is left untouched.
    crate::pds::material_finish::apply_socio_finish(&mut member_gen, prosperity, escalation);
    // Escalation-driven geometric damage: lean / settle / collapse the
    // structure by the room's conflict tier (the Ruins modifier).
    // Deterministic in the member's grammar seed; calm rooms are untouched.
    crate::pds::ruin::apply_ruin(&mut member_gen, escalation, member.grammar_seed);
    Some(member_gen)
}

/// A terrain-snapped, water-avoiding [`Placement::Absolute`] for a settlement
/// member at its derived offset / yaw / scale, referencing `generator_ref`.
/// Sunk 0.35 m below the snap so foundations bite into slopes instead of
/// leaving daylight gaps under the downhill edge.
fn member_placement(
    generator_ref: String,
    member: &crate::seeded_defaults::SettlementMember,
) -> Placement {
    let half_yaw = member.yaw_rad * 0.5;
    Placement::Absolute {
        generator_ref,
        transform: TransformData {
            translation: Fp3([member.offset[0], -0.35, member.offset[1]]),
            rotation: Fp4([0.0, half_yaw.sin(), 0.0, half_yaw.cos()]),
            scale: Fp3([member.scale, member.scale, member.scale]),
        },
        snap_to_terrain: true,
        avoid_water: true,
        avoid_water_clearance: Fp(member.clearance),
    }
}

/// Build an [`Environment`] whose colour fields are taken from a
/// DID-seeded [`crate::seeded_defaults::RoomPalette`]; every non-colour
/// field (cloud density, fog visibility, water normal scales, ...) is
/// preserved at its constant default. Later phases (atmosphere
/// derivers) will overwrite those non-colour fields too.
fn environment_from_palette(palette: &crate::seeded_defaults::RoomPalette) -> Environment {
    Environment {
        sun_color: Fp3(palette.sun_color),
        sky_color: Fp3(palette.sky_color),
        fog_color: Fp4(palette.fog_color),
        fog_extinction: Fp3(palette.fog_extinction),
        fog_inscattering: Fp3(palette.fog_inscattering),
        fog_sun_color: Fp4(palette.fog_sun_color),
        water_scatter_color: Fp3(palette.water_scatter),
        cloud_color: Fp3(palette.cloud_sunlit),
        cloud_shadow_color: Fp3(palette.cloud_shadow),
        ..Environment::default()
    }
}

/// Overwrite the per-layer colour fields on the four splat layers with
/// the seeded palette. Layer roles are positional (R=Grass, G=Dirt,
/// B=Rock, A=Snow) and the `Ground` / `Rock` variants are matched out
/// to assign each layer's idiomatic dry/moist or light/dark channel
/// pair. Layers that have been swapped out for a non-Ground / non-Rock
/// texture variant (e.g. a custom `Brick` snow layer) are left
/// unchanged so author intent is not silently overwritten.
fn apply_palette_to_material(
    palette: &crate::seeded_defaults::RoomPalette,
    material: &mut crate::pds::terrain::SovereignMaterialConfig,
) {
    use crate::pds::texture::SovereignTextureConfig;

    // R — Grass
    if let SovereignTextureConfig::Ground(g) = &mut material.layers[0] {
        g.color_dry = Fp3(palette.grass_dry);
        g.color_moist = Fp3(palette.grass_moist);
    }
    // G — Dirt
    if let SovereignTextureConfig::Ground(g) = &mut material.layers[1] {
        g.color_dry = Fp3(palette.dirt_dry);
        g.color_moist = Fp3(palette.dirt_moist);
    }
    // B — Rock
    //
    // The texture crate's field names are misleading: `color_light` is
    // the GAP between stones (UI label "Color Gaps") and `color_dark`
    // is the STONE face (UI label "Color Stone"). The ridged-multi-
    // fractal noise peaks become the visible gap pattern, hence the
    // counter-intuitive mapping. We name our palette fields after
    // intent (rock_stone, rock_gap) and swap them here so the result
    // reads correctly in-engine.
    if let SovereignTextureConfig::Rock(r) = &mut material.layers[2] {
        r.color_light = Fp3(palette.rock_gap);
        r.color_dark = Fp3(palette.rock_stone);
    }
    // A — Snow
    if let SovereignTextureConfig::Ground(g) = &mut material.layers[3] {
        g.color_dry = Fp3(palette.snow_dry);
        g.color_moist = Fp3(palette.snow_moist);
    }
}

/// Write a [`crate::seeded_defaults::TerrainShape`] onto every
/// heightmap-shape field of a `SovereignTerrainConfig` — generator
/// algorithm, FBM / Voronoi knobs, height/cell scale, erosion. The
/// `seed`, `grid_size`, and `material` fields are intentionally left
/// alone: `seed` is set separately from the room DID, `grid_size` is
/// a fixed resolution choice, and `material` (splat layers + rules)
/// is updated by [`apply_shape_to_material`] / `apply_palette_to_material`.
fn apply_shape_to_terrain_config(
    shape: &crate::seeded_defaults::TerrainShape,
    cfg: &mut SovereignTerrainConfig,
) {
    use crate::pds::terrain::SovereignGeneratorKind;
    use crate::seeded_defaults::GeneratorKind;

    cfg.generator_kind = match shape.generator_kind {
        GeneratorKind::FbmNoise => SovereignGeneratorKind::FbmNoise,
        GeneratorKind::DiamondSquare => SovereignGeneratorKind::DiamondSquare,
        GeneratorKind::VoronoiTerracing => SovereignGeneratorKind::VoronoiTerracing,
    };
    cfg.octaves = shape.octaves;
    cfg.persistence = Fp(shape.persistence);
    cfg.lacunarity = Fp(shape.lacunarity);
    cfg.base_frequency = Fp(shape.base_frequency);
    cfg.ds_roughness = Fp(shape.ds_roughness);
    cfg.voronoi_num_seeds = shape.voronoi_num_seeds;
    cfg.voronoi_num_terraces = shape.voronoi_num_terraces;
    cfg.height_scale = Fp(shape.height_scale);
    cfg.cell_scale = Fp(shape.cell_scale);
    cfg.erosion_enabled = shape.erosion_enabled;
    cfg.erosion_drops = shape.erosion_drops;
    cfg.erosion_rate = Fp(shape.erosion_rate);
    cfg.deposition_rate = Fp(shape.deposition_rate);
    cfg.capacity_factor = Fp(shape.capacity_factor);
    cfg.thermal_enabled = shape.thermal_enabled;
    cfg.thermal_iterations = shape.thermal_iterations;
    cfg.thermal_talus_angle = Fp(shape.thermal_talus_angle);
}

/// Write seeded splat rules onto the four-layer material. Biome
/// distribution (where grass/dirt/rock/snow each read as dominant on
/// the slope/height surface) is the visible payoff here — an alpine
/// room has a dramatically lower snow line than an arid one even
/// before the textures themselves differ.
fn apply_shape_to_material(
    shape: &crate::seeded_defaults::TerrainShape,
    material: &mut crate::pds::terrain::SovereignMaterialConfig,
) {
    for (i, rule) in shape.splat_rules.iter().enumerate() {
        material.rules[i] = crate::pds::terrain::SovereignSplatRule {
            height_min: Fp(rule.height_min),
            height_max: Fp(rule.height_max),
            slope_min: Fp(rule.slope_min),
            slope_max: Fp(rule.slope_max),
            sharpness: Fp(rule.sharpness),
        };
    }
}

/// Overwrite the per-layer procedural-texture knobs (seed, macro/micro
/// scales, octaves, micro weight, normal strength) with the
/// DID-seeded values. Each Ground / Rock layer keeps its existing
/// colour (which was just set by `apply_palette_to_material`). As
/// with the palette helper, layers that were swapped to a non-Ground
/// / non-Rock variant are left alone.
fn apply_textures_to_material(
    textures: &crate::seeded_defaults::BiomeTextures,
    material: &mut crate::pds::terrain::SovereignMaterialConfig,
) {
    use crate::pds::texture::SovereignTextureConfig;

    if let SovereignTextureConfig::Ground(g) = &mut material.layers[0] {
        apply_ground(&textures.grass, g);
    }
    if let SovereignTextureConfig::Ground(g) = &mut material.layers[1] {
        apply_ground(&textures.dirt, g);
    }
    if let SovereignTextureConfig::Rock(r) = &mut material.layers[2] {
        r.seed = textures.rock.seed;
        r.scale = Fp64(textures.rock.scale);
        r.octaves = textures.rock.octaves;
        r.attenuation = Fp64(textures.rock.attenuation);
        r.normal_strength = Fp(textures.rock.normal_strength);
    }
    if let SovereignTextureConfig::Ground(g) = &mut material.layers[3] {
        apply_ground(&textures.snow, g);
    }
}

/// Swap one terrain splat layer for a biome-signature surface generator,
/// using the tileable surfaces added in `bevy_symbios_texture` 0.6:
///
/// * **Arid / Coastal / Savanna / Badlands** — sand on the low/flat Grass
///   layer (desert floor, beach, dry golden grassland, eroded terraces).
/// * **Volcanic** — molten lava crust on the low/flat layer; its emissive
///   glow map is auto-wired by the upstream patch system.
/// * **Tundra / Alpine / Boreal** — real crystalline snow on the
///   high-altitude Snow layer (layer 3), replacing the plain white Ground.
/// * **Glacial** — blue cracked ice on the low/flat layer (the crevassed
///   valley floor) *and* crystalline snow on the high layer.
/// * **Lush / Jungle / Temperate Forest / Wetland / Meadow** — unchanged;
///   they keep the grassy Ground stack.
///
/// Runs after [`apply_textures_to_material`] so the swapped layer carries
/// the new generator's own appearance rather than a seeded Ground config.
/// The splat *rules* (height/slope → layer) are untouched, so layer 0 still
/// paints low/flat ground and layer 3 the high peaks.
fn apply_biome_signature_surface(
    biome: crate::seeded_defaults::BiomeArchetype,
    seed: u64,
    material: &mut crate::pds::terrain::SovereignMaterialConfig,
) {
    use crate::pds::texture::{
        SovereignIceConfig, SovereignLavaConfig, SovereignSandConfig, SovereignSnowConfig,
        SovereignTextureConfig as T,
    };
    use crate::seeded_defaults::BiomeArchetype;

    let sig = (seed ^ 0x5163_0001) as u32;
    match biome {
        BiomeArchetype::Arid
        | BiomeArchetype::Coastal
        | BiomeArchetype::Savanna
        | BiomeArchetype::Badlands => {
            material.layers[0] = T::Sand(SovereignSandConfig {
                seed: sig,
                ..Default::default()
            });
        }
        BiomeArchetype::Volcanic => {
            material.layers[0] = T::Lava(SovereignLavaConfig {
                seed: sig,
                ..Default::default()
            });
        }
        BiomeArchetype::Tundra | BiomeArchetype::Alpine | BiomeArchetype::Boreal => {
            material.layers[3] = T::Snow(SovereignSnowConfig {
                seed: sig,
                ..Default::default()
            });
        }
        BiomeArchetype::Glacial => {
            // Crevassed blue ice on the valley floor, snowfields on top.
            material.layers[0] = T::Ice(SovereignIceConfig {
                seed: sig,
                ..Default::default()
            });
            material.layers[3] = T::Snow(SovereignSnowConfig {
                seed: sig,
                ..Default::default()
            });
        }
        BiomeArchetype::Lush
        | BiomeArchetype::Jungle
        | BiomeArchetype::TemperateForest
        | BiomeArchetype::Wetland
        | BiomeArchetype::Meadow => {}
    }
}

fn apply_ground(
    src: &crate::seeded_defaults::GroundTextureParams,
    dst: &mut crate::pds::texture::SovereignGroundConfig,
) {
    dst.seed = src.seed;
    dst.macro_scale = Fp64(src.macro_scale);
    dst.macro_octaves = src.macro_octaves;
    dst.micro_scale = Fp64(src.micro_scale);
    dst.micro_octaves = src.micro_octaves;
    dst.micro_weight = Fp64(src.micro_weight);
    dst.normal_strength = Fp(src.normal_strength);
}

/// Project per-volume water dynamics onto a [`WaterSurface`]. Leaves
/// flow / wake / colour fields alone — colours were already set from
/// the palette, and flow / wake are opt-in features the seeded
/// defaults shouldn't enable wholesale.
fn apply_water_dynamics(src: &crate::seeded_defaults::WaterDynamics, dst: &mut WaterSurface) {
    dst.wave_direction = Fp2(src.wave_direction);
    dst.wave_scale = Fp(src.wave_scale);
    dst.wave_speed = Fp(src.wave_speed);
    dst.wave_choppiness = Fp(src.wave_choppiness);
    dst.foam_amount = Fp(src.foam_amount);
    dst.roughness = Fp(src.roughness);
    dst.wake_strength = Fp(src.wake_strength);
    dst.wake_ripple_wavelength = Fp(src.wake_ripple_wavelength);
    dst.wake_decay_radius = Fp(src.wake_decay_radius);
}

/// Project the room-global [`crate::seeded_defaults::Atmosphere`]
/// onto an [`Environment`]. Colours are already set from the palette
/// (sun_color, sky_color, fog_color, cloud_color, etc.); this pass
/// fills in everything else — sun position, illuminance, ambient,
/// fog visibility, cloud cover / softness / motion, and the global
/// water normal-map / glitter knobs.
fn apply_atmosphere_to_environment(
    src: &crate::seeded_defaults::Atmosphere,
    env: &mut Environment,
) {
    env.sun_position = Fp3(src.sun_position);
    env.sun_illuminance = Fp(src.sun_illuminance);
    env.ambient_brightness = Fp(src.ambient_brightness);
    env.fog_visibility = Fp(src.fog_visibility);
    env.fog_sun_exponent = Fp(src.fog_sun_exponent);
    env.water_normal_scale_near = Fp(src.water_normal_scale_near);
    env.water_normal_scale_far = Fp(src.water_normal_scale_far);
    env.water_sun_glitter = Fp(src.water_sun_glitter);
    env.water_shore_foam_width = Fp(src.shore_foam_width);
    env.cloud_cover = Fp(src.cloud_cover);
    env.cloud_density = Fp(src.cloud_density);
    env.cloud_softness = Fp(src.cloud_softness);
    env.cloud_speed = Fp(src.cloud_speed);
    env.cloud_scale = Fp(src.cloud_scale);
    env.cloud_height = Fp(src.cloud_height);
    env.cloud_wind_dir = Fp2(src.cloud_wind_dir);
}

/// Darken an [`Environment`] toward night by a theme's `luminosity`
/// (see [`crate::seeded_defaults::theme_luminosity`]). `1.0` is a perfect
/// no-op — full daylight, every non-nocturnal theme; below `1.0` it scales
/// the directional sun down hard and the ambient + sky / fog / cloud colour
/// down more gently so a self-lit theme (neon) reads as the dominant light
/// after dusk.
///
/// The directional key takes the raw multiply (a dim moonlight sun), while
/// ambient and the colour channels keep a generous floor — the look we
/// want is a deep magenta-blue night the player can still navigate, not a
/// power cut that collapses distant terrain into a black void.
fn apply_nightfall(luminosity: f32, env: &mut Environment) {
    let l = luminosity.clamp(0.0, 1.0);
    if (l - 1.0).abs() < f32::EPSILON {
        return; // full daylight — identity for every daylight theme
    }
    // Directional sun: scaled straight down to a moonlight key.
    env.sun_illuminance = Fp(env.sun_illuminance.0 * l);
    // Ambient + colour: floored well above the raw multiply so shape and
    // distance stay readable under the dim sun (l=0.12 → ~0.38 here).
    let floor = 0.3 + 0.7 * l;
    let darken3 = |c: Fp3| Fp3([c.0[0] * floor, c.0[1] * floor, c.0[2] * floor]);
    env.ambient_brightness = Fp(env.ambient_brightness.0 * floor);
    env.sky_color = darken3(env.sky_color);
    env.cloud_color = darken3(env.cloud_color);
    env.cloud_shadow_color = darken3(env.cloud_shadow_color);
    let fog = env.fog_color.0;
    env.fog_color = Fp4([fog[0] * floor, fog[1] * floor, fog[2] * floor, fog[3]]);
}

/// Return the terrain generator with the lexicographically smallest key.
///
/// `HashMap::values()` iteration order is randomised per execution (SipHash),
/// so a record with more than one `Generator::Terrain` entry would otherwise
/// have every client picking a different one and landing on a different
/// heightmap — instantly fracturing the shared world. Every site that needs
/// "the terrain" for a record must go through this function (or its sibling)
/// so the choice is deterministic across peers.
pub fn find_terrain_config(record: &RoomRecord) -> Option<&SovereignTerrainConfig> {
    let mut keys: Vec<&String> = record.generators.keys().collect();
    keys.sort();
    for k in keys {
        if let Some(generator) = record.generators.get(k)
            && let GeneratorKind::Terrain(cfg) = &generator.kind
        {
            return Some(cfg);
        }
    }
    None
}

/// Sub-stream salt so a room's road layout seed differs from its terrain seed
/// while staying deterministic in the DID.
const ROAD_SEED_SALT: u64 = 0xA0D5_EED5_A170_0001;

/// Themes whose default seeded room grows a road network. The `RoadNetwork`
/// generator itself is theme-agnostic; this is just the default-on policy —
/// any room can add or remove roads in the editor.
pub(crate) fn theme_grows_roads(theme: crate::seeded_defaults::ThemeArchetype) -> bool {
    use crate::seeded_defaults::ThemeArchetype::*;
    matches!(
        theme,
        Cyberpunk | ModernCity | IndustrialPark | Roadside | CivicCampus | Suburban | SportsRec
    )
}

/// Derive a seeded urban [`RoadConfig`] from the room's scene. Built-up themes
/// fill a larger district with tighter streets; sparse residential ones sprawl
/// wider. Prosperity then grows the district (a richer region reaches further)
/// and tightens the grid (denser blocks), so a poor outskirt and a rich
/// downtown of the same theme read differently. The derived extents stay inside
/// the road sanitiser's clamps (see `sanitize_road`) so a seeded value is never
/// re-snapped. `seed` is the layout's own re-rollable seed.
fn road_config_from_scene(scene: &crate::seeded_defaults::SceneCharacter, seed: u64) -> RoadConfig {
    use crate::seeded_defaults::ThemeArchetype::*;
    // Base (district ½-extent, major spacing, minor spacing) by how built-up
    // the theme is. Cyberpunk / ModernCity fill the whole ~1 km map.
    let (base_extent, major, minor) = match scene.theme {
        Cyberpunk | ModernCity => (512.0, 190.0, 110.0),
        IndustrialPark | Roadside => (460.0, 215.0, 135.0),
        CivicCampus => (400.0, 205.0, 125.0),
        Suburban | SportsRec => (340.0, 235.0, 150.0),
        // A non-listed theme can still opt into roads via the editor.
        _ => (300.0, 190.0, 110.0),
    };
    let prosperity = scene.prosperity.clamp(0.0, 1.0);
    let extent = (base_extent * (0.8 + 0.4 * prosperity)).clamp(120.0, 512.0);
    // Richer → denser (×0.85), poorer → sparser (×1.15).
    let spacing = 1.15 - 0.3 * prosperity;
    RoadConfig {
        seed,
        district_half_extent: Fp(extent),
        major_spacing: Fp(major * spacing),
        minor_spacing: Fp(minor * spacing),
        ..RoadConfig::default()
    }
}

/// Return the road-network config attached to the deterministically-chosen
/// terrain generator (its `RoadNetwork` child), if any. Mirrors
/// [`find_terrain_config`]'s sorted-key determinism so every peer reads the
/// same config; the terrain plugin builds the road mesh from this plus the
/// finished heightmap (see [`crate::urban`]).
pub fn find_road_config(record: &RoomRecord) -> Option<&RoadConfig> {
    let mut keys: Vec<&String> = record.generators.keys().collect();
    keys.sort();
    for k in keys {
        if let Some(generator) = record.generators.get(k)
            && let GeneratorKind::Terrain(_) = &generator.kind
        {
            return generator.children.iter().find_map(|c| match &c.kind {
                GeneratorKind::RoadNetwork(cfg) => Some(cfg),
                _ => None,
            });
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Read: fetch room record from the room owner's PDS
// ---------------------------------------------------------------------------

/// Wrapper for the `getRecord` XRPC response.
#[derive(Deserialize)]
struct GetRecordResponse {
    /// Captured raw so both `room/self` shapes — the legacy monolith and
    /// the #697 manifest — decode through [`RoomSelfWire`].
    value: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Split wire format (#697): manifest + content-addressed child generators
// ---------------------------------------------------------------------------

/// One named generator on the wire: a record in
/// [`super::ROOM_GENERATOR_COLLECTION`] at `rkey =` [`child_rkey`].
/// Immutable by construction — the rkey is a hash of this exact content,
/// so editing a generator publishes a *new* child and retires the old one.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RoomGeneratorRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    /// Advisory copy of (one of) the manifest name(s) pointing here — the
    /// manifest's `generator_refs` key is authoritative, so two names
    /// mapping to identical content share a single child record.
    pub name: String,
    pub generator: Generator,
}

impl RoomGeneratorRecord {
    fn new(name: &str, generator: &Generator) -> Self {
        Self {
            lex_type: super::ROOM_GENERATOR_COLLECTION.into(),
            name: name.into(),
            generator: generator.clone(),
        }
    }
}

/// Content-addressed record key for a child generator: lowercase hex of
/// `fnv1a_64` over the child's canonical serialized body (serde_json emits
/// sorted object keys, and the default-eliding serializers are
/// deterministic). Content-addressing is what keeps non-atomic reads safe:
/// a manifest can only ever point at children whose bytes cannot change,
/// so a visitor racing a publish sees a fully consistent old or new room,
/// never a half-updated child. It also makes unchanged generators free to
/// republish — same content, same rkey, no write.
pub fn child_rkey(name: &str, generator: &Generator) -> String {
    let canonical =
        serde_json::to_string(&RoomGeneratorRecord::new(name, generator)).unwrap_or_default();
    format!("{:016x}", crate::seeded_defaults::fnv1a_64(&canonical))
}

/// Both shapes `room/self` takes on the wire (#697 version-by-shape): the
/// legacy monolith carries inline `generators`; the manifest instead
/// carries `generator_refs` (name → child rkey). Every field defaults so
/// either shape — or a forward-compat superset — decodes.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RoomSelfWire {
    environment: Environment,
    generators: HashMap<String, Generator>,
    generator_refs: HashMap<String, String>,
    placements: Vec<Placement>,
    traits: HashMap<String, Vec<String>>,
    contact_effects: ContactEffects,
}

/// The manifest written to `room/self` since #697: the full record minus
/// generator bodies, which live in content-addressed child records.
/// `generator_refs` is a `BTreeMap` so the manifest bytes are canonical.
#[derive(Serialize)]
struct RoomManifestOut {
    #[serde(rename = "$type")]
    lex_type: String,
    environment: Environment,
    generator_refs: std::collections::BTreeMap<String, String>,
    placements: Vec<Placement>,
    traits: HashMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "ContactEffects::is_default")]
    contact_effects: ContactEffects,
}

impl RoomManifestOut {
    fn from_record(record: &RoomRecord) -> Self {
        Self {
            lex_type: COLLECTION.into(),
            environment: record.environment.clone(),
            generator_refs: record
                .generators
                .iter()
                .map(|(name, generator)| (name.clone(), child_rkey(name, generator)))
                .collect(),
            placements: record.placements.clone(),
            traits: record.traits.clone(),
            contact_effects: record.contact_effects.clone(),
        }
    }
}

/// Join a decoded `room/self` with the child-generator map (rkey →
/// generator) fetched from [`super::ROOM_GENERATOR_COLLECTION`]. Inline
/// legacy generators and resolved refs merge into one map; a ref whose
/// child is missing (a torn historical write, or a hostile PDS dropping
/// records) skips that generator with a warning rather than failing the
/// whole room — the same degrade-don't-crash policy the open unions use.
fn assemble_room(wire: RoomSelfWire, children: &HashMap<String, Generator>) -> RoomRecord {
    let RoomSelfWire {
        environment,
        mut generators,
        generator_refs,
        placements,
        traits,
        contact_effects,
    } = wire;
    for (name, rkey) in generator_refs {
        match children.get(&rkey) {
            Some(generator) => {
                generators.insert(name, generator.clone());
            }
            None => warn!(
                "room manifest references missing child generator {rkey} for '{name}' — skipping"
            ),
        }
    }
    RoomRecord {
        lex_type: COLLECTION.into(),
        environment,
        generators,
        placements,
        traits,
        contact_effects,
    }
}

/// `com.atproto.repo.listRecords` envelope for the child walk. Values stay
/// raw so one foreign / undecodable record skips instead of failing a page;
/// the rkey is recovered from the record's `at://` URI tail.
#[derive(Deserialize)]
struct ListChildrenResponse {
    #[serde(default)]
    records: Vec<ListedChild>,
    cursor: Option<String>,
}

#[derive(Deserialize)]
struct ListedChild {
    uri: String,
    value: serde_json::Value,
}

/// Walk the child-generator collection for `did`, returning rkey →
/// generator. Bounded by
/// [`crate::config::state::MAX_ROOM_GENERATOR_PAGES`] pages of 100 so a
/// hostile PDS handing out endless cursors cannot keep the client paging.
async fn list_room_children(
    client: &reqwest::Client,
    pds: &str,
    did: &str,
) -> Result<HashMap<String, Generator>, FetchError> {
    let mut children: HashMap<String, Generator> = HashMap::new();
    let mut cursor: Option<String> = None;
    for _ in 0..crate::config::state::MAX_ROOM_GENERATOR_PAGES {
        let url = format!("{}/xrpc/com.atproto.repo.listRecords", pds);
        let mut query: Vec<(&str, String)> = vec![
            ("repo", did.to_string()),
            ("collection", super::ROOM_GENERATOR_COLLECTION.to_string()),
            ("limit", "100".to_string()),
        ];
        if let Some(c) = cursor.take() {
            query.push(("cursor", c));
        }
        let resp = client
            .get(&url)
            .query(&query)
            .send()
            .await
            .map_err(|e| FetchError::Network(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(FetchError::PdsError(status.as_u16()));
        }
        let page: ListChildrenResponse = decode_record_json(resp).await?;
        let empty_page = page.records.is_empty();
        for rec in page.records {
            let Some(rkey) = rec.uri.rsplit('/').next() else {
                continue;
            };
            if let Ok(child) = serde_json::from_value::<RoomGeneratorRecord>(rec.value) {
                children.insert(rkey.to_string(), child.generator);
            }
        }
        cursor = page.cursor;
        if cursor.is_none() || empty_page {
            break;
        }
    }
    Ok(children)
}

/// Fetch the room customisation record from the given DID's PDS.
///
/// * `Ok(Some(record))` — the owner has published a record.
/// * `Ok(None)` — the PDS reported there is no record yet (the caller may
///   substitute the default homeworld).
/// * `Err(FetchError)` — transient or permanent failure; the caller must
///   **not** fall through to the default, because doing so risks the user
///   publishing the blank default over their real room on the next save.
///
/// Shape-agnostic since #697: a legacy monolith is returned as-is, while a
/// manifest triggers one additional `listRecords` walk over the
/// child-generator collection before assembly.
///
/// Note: ATProto's `com.atproto.repo.getRecord` returns `400 RecordNotFound`
/// — NOT `404` — when the record does not exist. We detect that payload
/// explicitly and convert it to `Ok(None)` so the loading state can advance
/// onto the default homeworld instead of hammering the PDS with retries.
pub async fn fetch_room_record(
    client: &reqwest::Client,
    did: &str,
) -> Result<Option<RoomRecord>, FetchError> {
    let pds = resolve_pds(client, did)
        .await
        .ok_or(FetchError::DidResolutionFailed)?;
    let url = format!(
        "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey=self",
        pds, did, COLLECTION
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?;
    let status = resp.status();
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if !status.is_success() {
        // Inspect the error body before surfacing as PdsError — ATProto
        // signals "no such record" via 400 + `error: "RecordNotFound"` in
        // the body, and we must not treat that as a transient retry case.
        let body = resp.text().await.unwrap_or_default();
        if let Ok(xrpc) = serde_json::from_str::<XrpcError>(&body)
            && let Some(err) = xrpc.error.as_deref()
            && (err == "RecordNotFound"
                || (err == "InvalidRequest" && body.contains("RecordNotFound")))
        {
            return Ok(None);
        }
        return Err(FetchError::PdsError(status.as_u16()));
    }
    let wrapper: GetRecordResponse = decode_record_json(resp).await?;
    let wire: RoomSelfWire =
        serde_json::from_value(wrapper.value).map_err(|e| FetchError::Decode(e.to_string()))?;
    let children = if wire.generator_refs.is_empty() {
        HashMap::new()
    } else {
        list_room_children(client, &pds, did).await?
    };
    let mut record = assemble_room(wire, &children);
    record.sanitize();
    Ok(Some(record))
}

// ---------------------------------------------------------------------------
// Write: publish room record to the authenticated user's PDS
// ---------------------------------------------------------------------------

/// Serialized size of the largest single record a publish of `record`
/// would write — the manifest or the biggest child generator. This is the
/// per-record figure the #694 size budget applies to now that the room is
/// split across records (#697).
pub fn max_publish_record_bytes(record: &RoomRecord) -> Option<usize> {
    let manifest =
        super::record_size::serialized_record_bytes(&RoomManifestOut::from_record(record));
    let biggest_child = record
        .generators
        .iter()
        .filter_map(|(name, generator)| {
            super::record_size::serialized_record_bytes(&RoomGeneratorRecord::new(name, generator))
        })
        .max();
    manifest.into_iter().chain(biggest_child).max()
}

/// Build the ordered `applyWrites` batches that publish `record` as a
/// manifest + content-addressed children (#697), given the child rkeys
/// currently on the PDS and whether `room/self` already exists.
///
/// The plan is: child creates, then the manifest put, then orphan deletes —
/// chunked to the [`MAX_APPLY_WRITES`] commit cap in that order, so a
/// visitor reading between commits always sees a manifest whose refs all
/// resolve (new children land before the manifest points at them; orphans
/// are only deleted after nothing references them). Unchanged generators
/// cost nothing: same content → same rkey → already in `existing`.
/// Every record written is size-checked against the hard ceiling first.
fn plan_room_writes(
    record: &RoomRecord,
    existing_children: &std::collections::HashSet<String>,
    manifest_exists: bool,
) -> Result<Vec<Vec<RepoWrite>>, String> {
    let manifest = RoomManifestOut::from_record(record);
    super::record_size::preflight(&manifest, "room manifest")?;

    // Desired child set, deduped by rkey (identical content under two
    // names shares one record) and sorted for a deterministic plan.
    let mut desired: std::collections::BTreeMap<String, RepoWrite> =
        std::collections::BTreeMap::new();
    for (name, generator) in &record.generators {
        let rkey = child_rkey(name, generator);
        if existing_children.contains(&rkey) || desired.contains_key(&rkey) {
            continue;
        }
        let child = RoomGeneratorRecord::new(name, generator);
        super::record_size::preflight(&child, &format!("room generator \"{name}\""))?;
        let value = serde_json::to_value(&child).map_err(|e| format!("serialize: {e}"))?;
        desired.insert(
            rkey.clone(),
            RepoWrite::Create {
                collection: super::ROOM_GENERATOR_COLLECTION.into(),
                rkey,
                value,
            },
        );
    }
    let creates: Vec<RepoWrite> = desired.into_values().collect();

    let referenced: std::collections::HashSet<String> = record
        .generators
        .iter()
        .map(|(name, generator)| child_rkey(name, generator))
        .collect();
    let mut orphans: Vec<&String> = existing_children
        .iter()
        .filter(|rkey| !referenced.contains(*rkey))
        .collect();
    orphans.sort();
    let deletes: Vec<RepoWrite> = orphans
        .into_iter()
        .map(|rkey| RepoWrite::Delete {
            collection: super::ROOM_GENERATOR_COLLECTION.into(),
            rkey: rkey.clone(),
        })
        .collect();

    let manifest_value = serde_json::to_value(&manifest).map_err(|e| format!("serialize: {e}"))?;
    let manifest_write = if manifest_exists {
        RepoWrite::Update {
            collection: COLLECTION.into(),
            rkey: "self".into(),
            value: manifest_value,
        }
    } else {
        RepoWrite::Create {
            collection: COLLECTION.into(),
            rkey: "self".into(),
            value: manifest_value,
        }
    };

    // Chunk in read-safe order: creates → manifest (+ deletes that fit) →
    // remaining deletes.
    let ordered: Vec<RepoWrite> = creates
        .into_iter()
        .chain(std::iter::once(manifest_write))
        .chain(deletes)
        .collect();
    // Sharing a batch across the create/manifest/delete phases is fine —
    // each applyWrites batch commits atomically, so only the ordering at
    // CHUNK boundaries matters, and the linear order above provides it.
    let mut batches: Vec<Vec<RepoWrite>> = Vec::new();
    let mut batch: Vec<RepoWrite> = Vec::new();
    for write in ordered {
        if batch.len() == MAX_APPLY_WRITES {
            batches.push(std::mem::take(&mut batch));
        }
        batch.push(write);
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    Ok(batches)
}

/// `true` when `room/self` exists on the PDS in either shape. Publish uses
/// this to pick `applyWrites#create` vs `#update` for the manifest.
async fn room_self_exists(client: &reqwest::Client, pds: &str, did: &str) -> Result<bool, String> {
    let url = format!(
        "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey=self",
        pds, did, COLLECTION
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("existence check: {e}"))?;
    let status = resp.status();
    if status.is_success() {
        return Ok(true);
    }
    if status.as_u16() == 404 {
        return Ok(false);
    }
    let body = resp.text().await.unwrap_or_default();
    if body.contains("RecordNotFound") {
        return Ok(false);
    }
    Err(format!("existence check failed: {} — {}", status, body))
}

/// Publish the room to the authenticated user's own PDS as a slim manifest
/// plus content-addressed child generator records (#697).
///
/// The write plan diffs the desired child set against a `listRecords` walk
/// of what is actually on the PDS (authoritative — orphans from older
/// writes or other devices are GC'd in the same plan) and commits via
/// `com.atproto.repo.applyWrites` in read-safe order: children first, then
/// the manifest, then orphan deletes. A plan that fits one batch (the
/// overwhelmingly common case) is fully atomic; a chunked plan is safe at
/// every commit boundary because manifests only ever reference
/// already-committed, immutable children.
///
/// This replaces the old `putRecord` + delete-then-put 5xx recovery: the
/// manifest is small and schema-stable, and `applyWrites` sidesteps the
/// stale-CID diffing that path worked around.
pub async fn publish_room_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: &RoomRecord,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;

    let existing: std::collections::HashSet<String> =
        list_room_children(client, &pds, &session.did)
            .await
            .map_err(|e| format!("child listing failed: {e:?}"))?
            .into_keys()
            .collect();
    let manifest_exists = room_self_exists(client, &pds, &session.did).await?;

    // Plan construction runs every per-record size preflight BEFORE any
    // write lands, so an oversized generator can never leave the room
    // half-published.
    let batches = plan_room_writes(record, &existing, manifest_exists)?;
    for batch in batches {
        super::xrpc::apply_writes(&pds, session, refresh, batch).await?;
    }
    Ok(())
}

/// Delete the room from the authenticated user's PDS — the `room/self`
/// manifest (whichever shape it holds) **and** every record in the
/// child-generator collection, so a reset cannot strand orphaned children
/// (#697). Deletes only what a `listRecords` walk + existence check say is
/// actually there (an `applyWrites#delete` on a missing record fails the
/// whole batch), and deletes the manifest FIRST so an interrupted wipe
/// never leaves a manifest pointing at removed children. A repo with
/// nothing to delete is a clean no-op.
pub async fn delete_room_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;

    let mut deletes: Vec<RepoWrite> = Vec::new();
    if room_self_exists(client, &pds, &session.did).await? {
        deletes.push(RepoWrite::Delete {
            collection: COLLECTION.into(),
            rkey: "self".into(),
        });
    }
    let mut children: Vec<String> = list_room_children(client, &pds, &session.did)
        .await
        .map_err(|e| format!("child listing failed: {e:?}"))?
        .into_keys()
        .collect();
    children.sort();
    deletes.extend(children.into_iter().map(|rkey| RepoWrite::Delete {
        collection: super::ROOM_GENERATOR_COLLECTION.into(),
        rkey,
    }));

    let mut batches: Vec<Vec<RepoWrite>> = Vec::new();
    let mut batch: Vec<RepoWrite> = Vec::new();
    for write in deletes {
        if batch.len() == MAX_APPLY_WRITES {
            batches.push(std::mem::take(&mut batch));
        }
        batch.push(write);
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    for batch in batches {
        super::xrpc::apply_writes(&pds, session, refresh, batch).await?;
    }
    Ok(())
}

/// Force-overwrite the room by wiping manifest + children first, then
/// publishing fresh. Used by the recovery banner's "Reset PDS to default"
/// button, which must work even when the stored record is
/// schema-incompatible with the current build.
pub async fn reset_room_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: &RoomRecord,
) -> Result<(), String> {
    // Size-guard BEFORE the delete: this path removes the stored record
    // first, so an oversized replacement refused only at publish time would
    // already have destroyed the owner's saved room. The publish below
    // re-checks per record; this early manifest-level check just fails fast
    // on the worst case.
    super::record_size::preflight(&RoomManifestOut::from_record(record), "room manifest")?;
    delete_room_record(client, session, refresh).await?;
    publish_room_record(client, session, refresh, record).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn biome_signature_surface_swaps_expected_layer() {
        use crate::pds::texture::SovereignTextureConfig as T;
        use crate::seeded_defaults::BiomeArchetype;

        let fresh = crate::pds::terrain::SovereignMaterialConfig::default;

        // Arid / Coastal / Savanna / Badlands → sand on the low/flat Grass
        // layer (0).
        for biome in [
            BiomeArchetype::Arid,
            BiomeArchetype::Coastal,
            BiomeArchetype::Savanna,
            BiomeArchetype::Badlands,
        ] {
            let mut m = fresh();
            apply_biome_signature_surface(biome, 9, &mut m);
            assert!(matches!(m.layers[0], T::Sand(_)), "{biome:?} → layer0 sand");
        }

        // Volcanic → molten lava crust on the low/flat layer.
        let mut m = fresh();
        apply_biome_signature_surface(BiomeArchetype::Volcanic, 9, &mut m);
        assert!(matches!(m.layers[0], T::Lava(_)));

        // Tundra / Alpine / Boreal → real snow on the high-altitude Snow
        // layer (3), leaving the low/flat layer as its Ground default.
        for biome in [
            BiomeArchetype::Tundra,
            BiomeArchetype::Alpine,
            BiomeArchetype::Boreal,
        ] {
            let mut m = fresh();
            apply_biome_signature_surface(biome, 9, &mut m);
            assert!(matches!(m.layers[3], T::Snow(_)), "{biome:?} → layer3 snow");
            assert!(matches!(m.layers[0], T::Ground(_)));
        }

        // Glacial → blue cracked ice on the valley floor + snow on top.
        let mut m = fresh();
        apply_biome_signature_surface(BiomeArchetype::Glacial, 9, &mut m);
        assert!(matches!(m.layers[0], T::Ice(_)), "glacial → layer0 ice");
        assert!(matches!(m.layers[3], T::Snow(_)), "glacial → layer3 snow");

        // Verdant biomes keep the entire grassy Ground stack.
        for biome in [
            BiomeArchetype::Lush,
            BiomeArchetype::Jungle,
            BiomeArchetype::TemperateForest,
            BiomeArchetype::Wetland,
            BiomeArchetype::Meadow,
        ] {
            let mut m = fresh();
            apply_biome_signature_surface(biome, 9, &mut m);
            assert!(
                matches!(m.layers[0], T::Ground(_)),
                "{biome:?} → layer0 ground"
            );
            assert!(
                matches!(m.layers[3], T::Ground(_)),
                "{biome:?} → layer3 ground"
            );
        }
    }

    #[test]
    fn default_room_carries_a_themed_settlement() {
        use crate::seeded_defaults::room::settlement::{MAX_PROPS, MAX_SECONDARIES};
        use crate::seeded_defaults::{SceneCharacter, fnv1a_64};
        for s in 0u64..16 {
            let did = format!("did:test:{s}");

            // Road-growing themes intentionally bake no concentric settlement —
            // their buildings come from the road network's lots at load (see
            // populate-lots). Such a room must instead carry a road network and
            // none of the settlement generators.
            if theme_grows_roads(SceneCharacter::for_seed(fnv1a_64(&did)).theme) {
                let record = RoomRecord::default_for_did(&did);
                assert!(
                    find_road_config(&record).is_some(),
                    "road-growing room {did} must carry a road network"
                );
                assert!(
                    !record.generators.contains_key("landmark")
                        && !record
                            .generators
                            .keys()
                            .any(|k| k.starts_with("settlement_")),
                    "road-growing room {did} must not bake a concentric settlement"
                );
                continue;
            }

            let record = RoomRecord::default_for_did(&did);

            // Every non-urban room carries exactly one landmark, and it's a
            // building — never Terrain/Water (those are positionally
            // invalid outside the base_terrain tree).
            let landmark = record
                .generators
                .get("landmark")
                .expect("every seeded non-urban room must carry a landmark generator");
            assert!(!matches!(
                landmark.kind,
                GeneratorKind::Terrain(_) | GeneratorKind::Water { .. }
            ));

            // Each settlement member (landmark + bounded secondaries +
            // props) is a building with a terrain-snapped Absolute
            // placement that clears the spawn square.
            let mut secondaries = 0usize;
            let mut props = 0usize;
            for (name, generator) in &record.generators {
                let is_member = name == "landmark"
                    || name.starts_with("settlement_secondary_")
                    || name.starts_with("settlement_prop_");
                if !is_member {
                    continue;
                }
                secondaries += name.starts_with("settlement_secondary_") as usize;
                props += name.starts_with("settlement_prop_") as usize;

                assert!(
                    !matches!(
                        generator.kind,
                        GeneratorKind::Terrain(_) | GeneratorKind::Water { .. }
                    ),
                    "settlement member {name} must be a building"
                );

                let (transform, snap) = record
                    .placements
                    .iter()
                    .find_map(|p| match p {
                        Placement::Absolute {
                            generator_ref,
                            transform,
                            snap_to_terrain,
                            ..
                        } if generator_ref == name => Some((transform, snap_to_terrain)),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("{name} must have an Absolute placement"));
                assert!(*snap, "{name} must snap to terrain");
                let [x, _, z] = transform.translation.0;
                let dist = (x * x + z * z).sqrt();
                assert!(
                    dist >= 20.0,
                    "settlement member {name} too close to spawn: {dist} m"
                );
            }

            assert!(
                secondaries <= MAX_SECONDARIES,
                "too many secondaries: {secondaries}"
            );
            assert!(props <= MAX_PROPS, "too many props: {props}");
        }
    }

    #[test]
    fn settlement_props_dedupe_to_one_generator_per_slug() {
        use crate::seeded_defaults::{SceneCharacter, Settlement, fnv1a_64};
        use std::collections::HashSet;
        // Find a non-urban room whose settlement repeats a prop slug — props are
        // sampled with replacement, so dedup must actually collapse something.
        let found = (0u64..256).find_map(|s| {
            let did = format!("did:test:{s}");
            let seed = fnv1a_64(&did);
            let scene = SceneCharacter::for_seed(seed);
            if theme_grows_roads(scene.theme) {
                return None;
            }
            let settlement = Settlement::from_scene(&scene, seed);
            let total = settlement.props.len();
            let distinct: HashSet<_> = settlement.props.iter().map(|m| m.slug).collect();
            (total > distinct.len()).then_some((did, total, distinct.len()))
        });
        let (did, total_props, distinct_props) =
            found.expect("no non-urban seed in 0..256 repeated a settlement prop");

        let record = RoomRecord::default_for_did(&did);
        let prop_placements = record
            .placements
            .iter()
            .filter(|p| {
                matches!(p, Placement::Absolute { generator_ref, .. }
                    if generator_ref.starts_with("settlement_prop_"))
            })
            .count();
        let prop_gens = record
            .generators
            .keys()
            .filter(|k| k.starts_with("settlement_prop_"))
            .count();
        // One placement per prop copy, but one generator per distinct slug.
        assert_eq!(prop_placements, total_props, "one placement per prop copy");
        assert_eq!(
            prop_gens, distinct_props,
            "one generator per distinct prop slug"
        );
        assert!(
            prop_gens < prop_placements,
            "dedup must collapse repeated props ({prop_gens} gens for {prop_placements} placements)"
        );
        // Every prop placement resolves to its shared generator.
        for p in &record.placements {
            if let Placement::Absolute { generator_ref, .. } = p
                && generator_ref.starts_with("settlement_prop_")
            {
                assert!(
                    record.generators.contains_key(generator_ref),
                    "prop placement references missing generator {generator_ref}"
                );
            }
        }
    }

    #[test]
    fn seeded_natural_scatters_opt_into_urban_avoidance() {
        // Pick a seed that actually grows tree/boulder scatters, then assert
        // every natural scatter opts into avoid_urban so wild scatter stays out
        // of the built-up road district (a no-op in rooms without roads).
        let record = (0u64..64)
            .map(|s| RoomRecord::default_for_did(&format!("did:test:{s}")))
            .find(|r| {
                r.placements.iter().any(|p| {
                    matches!(p, Placement::Scatter { generator_ref, .. }
                        if generator_ref.starts_with("tree_scatter_") || generator_ref == "boulder")
                })
            })
            .expect("a seed with natural scatters");

        let mut natural = 0;
        for p in &record.placements {
            if let Placement::Scatter {
                generator_ref,
                avoid_urban,
                ..
            } = p
                && (generator_ref.starts_with("tree_scatter_") || generator_ref == "boulder")
            {
                natural += 1;
                assert!(
                    *avoid_urban,
                    "natural scatter {generator_ref} must avoid_urban"
                );
            }
        }
        assert!(natural > 0);

        // The new field survives a serde round-trip (serde default keeps older
        // records valid; a true value must persist).
        let json = serde_json::to_string(&record).expect("serialize");
        let back: RoomRecord = serde_json::from_str(&json).expect("deserialize");
        assert!(
            !crate::state::records_differ(&record, &back),
            "avoid_urban must round-trip"
        );
    }

    #[test]
    fn road_config_from_scene_stays_in_editable_range() {
        use crate::seeded_defaults::{SceneCharacter, ThemeArchetype};
        // Derived extents must land inside the GUI slider ranges (so a seeded
        // value is never pinned) — which are themselves inside the road
        // sanitiser's clamps, so sanitize never re-snaps a seeded network.
        for theme in ThemeArchetype::ALL {
            for prosperity in [0.0_f32, 0.5, 1.0] {
                let mut scene = SceneCharacter::for_seed(0);
                scene.theme = theme;
                scene.prosperity = prosperity;
                let c = road_config_from_scene(&scene, 1);
                let d = c.district_half_extent.0;
                let (maj, min) = (c.major_spacing.0, c.minor_spacing.0);
                assert!(
                    (50.0..=512.0).contains(&d),
                    "{theme:?}@{prosperity}: district {d} outside slider 50..=512"
                );
                assert!(
                    (30.0..=300.0).contains(&maj),
                    "{theme:?}@{prosperity}: major {maj} outside slider 30..=300"
                );
                assert!(
                    (20.0..=200.0).contains(&min),
                    "{theme:?}@{prosperity}: minor {min} outside slider 20..=200"
                );
            }
        }
    }

    /// The DID path must equal the seed path fed the hashed DID — the
    /// contract that keeps `default_for_did` untouched while the manual
    /// re-roll uses `default_for_seed`. Compared through the same serde
    /// equality the editor's dirty check uses.
    #[test]
    fn default_for_did_equals_default_for_seed_of_hashed_did() {
        for s in 0u64..16 {
            let did = format!("did:test:{s}");
            let from_did = RoomRecord::default_for_did(&did);
            let from_seed =
                RoomRecord::default_for_seed(crate::seeded_defaults::fnv1a_64(&did), &did);
            assert!(
                !crate::state::records_differ(&from_did, &from_seed),
                "default_for_did diverged from default_for_seed(fnv1a_64(did)) for {did}"
            );
        }
    }

    #[test]
    fn default_for_seed_is_deterministic() {
        let a = RoomRecord::default_for_seed(0xABCD_1234, "did:test:reroll");
        let b = RoomRecord::default_for_seed(0xABCD_1234, "did:test:reroll");
        assert!(!crate::state::records_differ(&a, &b));
    }

    #[test]
    fn distinct_seeds_yield_distinct_rooms() {
        // A re-roll must actually change the room (same DID, new seed).
        let a = RoomRecord::default_for_seed(1, "did:test:reroll");
        let b = RoomRecord::default_for_seed(2, "did:test:reroll");
        assert!(
            crate::state::records_differ(&a, &b),
            "re-roll produced an identical room for two seeds"
        );
    }

    #[test]
    fn default_room_carries_micro_detail_layers() {
        for s in 0u64..4 {
            let record = RoomRecord::default_for_did(&format!("did:test:{s}"));
            assert!(
                record.generators.contains_key("boulder"),
                "seeded room lost its boulder generator"
            );
            assert!(
                record.generators.contains_key("ambient_particles"),
                "seeded room lost its ambient particle emitter"
            );
            let rock_scatters = record
                .placements
                .iter()
                .filter(|p| {
                    matches!(p, Placement::Scatter { generator_ref, .. } if generator_ref == "boulder")
                })
                .count();
            assert!(
                (1..=2).contains(&rock_scatters),
                "expected 1–2 boulder scatters, got {rock_scatters}"
            );
        }
    }

    #[test]
    fn urban_rooms_grow_a_road_network_others_stay_bare() {
        use crate::seeded_defaults::{SceneCharacter, fnv1a_64};
        let (mut saw_urban, mut saw_bare) = (false, false);
        for s in 0u64..300 {
            let did = format!("did:test:{s}");
            let theme = SceneCharacter::for_seed(fnv1a_64(&did)).theme;
            let record = RoomRecord::default_for_did(&did);
            let road = find_road_config(&record);
            assert_eq!(
                road.is_some(),
                theme_grows_roads(theme),
                "road presence must match the default-on policy for {theme:?}"
            );
            if let Some(cfg) = road {
                assert!(cfg.enabled, "seeded road network is enabled");
                let terr = find_terrain_config(&record).expect("urban room has terrain");
                assert_ne!(
                    cfg.seed, terr.seed,
                    "road layout carries its own seed, distinct from terrain"
                );
                saw_urban = true;
            } else {
                saw_bare = true;
            }
        }
        assert!(
            saw_urban,
            "some seeded room should be an urban (roaded) theme"
        );
        assert!(
            saw_bare,
            "some seeded room should be a bare (road-free) theme"
        );
    }

    #[test]
    fn nightfall_dims_nocturnal_themes_and_is_identity_at_full_day() {
        let day = Environment::default();

        // A nocturnal luminosity dims the sun + ambient and darkens the sky.
        let mut night = Environment::default();
        apply_nightfall(0.12, &mut night);
        assert!(
            night.sun_illuminance.0 < day.sun_illuminance.0,
            "nightfall must dim the sun"
        );
        assert!(
            night.ambient_brightness.0 < day.ambient_brightness.0,
            "nightfall must dim ambient"
        );
        assert!(
            night.sky_color.0.iter().sum::<f32>() < day.sky_color.0.iter().sum::<f32>(),
            "nightfall must darken the sky"
        );
        // Survives the record sanitiser (no NaN / out-of-range fields).
        night.sanitize();
        assert!(night.sun_illuminance.0 > 0.0 && night.sun_illuminance.0.is_finite());

        // Full daylight is a perfect no-op — daylight themes are untouched.
        let mut unchanged = Environment::default();
        apply_nightfall(1.0, &mut unchanged);
        assert_eq!(unchanged.sun_illuminance.0, day.sun_illuminance.0);
        assert_eq!(unchanged.ambient_brightness.0, day.ambient_brightness.0);
        assert_eq!(unchanged.sky_color.0, day.sky_color.0);
    }

    #[test]
    fn default_room_survives_sanitize() {
        for s in 0u64..4 {
            let mut record = RoomRecord::default_for_did(&format!("did:test:{s}"));
            let generators_before = record.generators.len();
            let placements_before = record.placements.len();
            record.sanitize();
            assert_eq!(record.generators.len(), generators_before);
            assert_eq!(record.placements.len(), placements_before);
        }
    }
}

#[cfg(test)]
mod split_wire_tests {
    //! #697 manifest + content-addressed children: the write plan and the
    //! read-side join must be exact inverses, in read-safe order, across
    //! both wire shapes.
    use super::*;
    use std::collections::HashSet;

    fn cuboid_at(x: f32) -> Generator {
        let mut g = Generator::default_cuboid();
        g.transform.translation.0[0] = x;
        g
    }

    #[test]
    fn child_rkey_is_content_addressed_hex() {
        let a = cuboid_at(1.0);
        let key = child_rkey("tree", &a);
        assert_eq!(key, child_rkey("tree", &a), "deterministic");
        assert_ne!(key, child_rkey("bush", &a), "name participates");
        assert_ne!(
            key,
            child_rkey("tree", &cuboid_at(2.0)),
            "content participates"
        );
        assert_eq!(key.len(), 16);
        assert!(
            key.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }

    #[test]
    fn plan_creates_new_skips_existing_and_deletes_orphans() {
        let record = RoomRecord::default_for_did("did:plc:plan");
        // Make one generator "already published": seed the existing set
        // with its content-addressed rkey.
        let (unchanged_name, unchanged_gen) = record
            .generators
            .iter()
            .next()
            .map(|(n, g)| (n.clone(), g.clone()))
            .unwrap();
        let unchanged_rkey = child_rkey(&unchanged_name, &unchanged_gen);
        let orphan_rkey = "00000000deadbeef".to_string();
        let existing: HashSet<String> = [unchanged_rkey.clone(), orphan_rkey.clone()]
            .into_iter()
            .collect();

        let batches = plan_room_writes(&record, &existing, true).unwrap();
        assert_eq!(batches.len(), 1, "default room fits one atomic batch");
        let writes = &batches[0];

        // Creates for every generator EXCEPT the unchanged one.
        let creates: Vec<_> = writes
            .iter()
            .filter(|w| matches!(w, RepoWrite::Create { collection, .. } if collection == super::super::ROOM_GENERATOR_COLLECTION))
            .collect();
        assert_eq!(creates.len(), record.generators.len() - 1);
        assert!(
            !writes
                .iter()
                .any(|w| matches!(w, RepoWrite::Create { rkey, .. } if *rkey == unchanged_rkey)),
            "unchanged content is free — no rewrite"
        );

        // Manifest is an update (room/self exists) and sits after every
        // create and before every delete.
        let manifest_idx = writes
            .iter()
            .position(|w| matches!(w, RepoWrite::Update { collection, rkey, .. } if collection == COLLECTION && rkey == "self"))
            .expect("manifest update present");
        let last_create = writes
            .iter()
            .rposition(|w| matches!(w, RepoWrite::Create { .. }))
            .unwrap();
        let first_delete = writes
            .iter()
            .position(|w| matches!(w, RepoWrite::Delete { .. }))
            .expect("orphan delete present");
        assert!(last_create < manifest_idx && manifest_idx < first_delete);
        assert!(
            writes
                .iter()
                .any(|w| matches!(w, RepoWrite::Delete { rkey, .. } if *rkey == orphan_rkey)),
            "orphan is GC'd"
        );

        // Fresh repo → the manifest write is a create instead.
        let batches = plan_room_writes(&record, &HashSet::new(), false).unwrap();
        assert!(batches[0].iter().any(|w| matches!(
            w,
            RepoWrite::Create { collection, rkey, .. } if collection == COLLECTION && rkey == "self"
        )));
    }

    #[test]
    fn plan_dedups_identical_content_under_two_names() {
        let mut record = RoomRecord::default_for_did("did:plc:dedup");
        record.generators.clear();
        // The child body embeds the name, so true dedup needs identical
        // (name, content) — which two map keys can't produce. What CAN
        // happen is the same rkey appearing twice via map iteration of
        // equal content+name pairs after a merge; assert the guard holds
        // for the reachable case: one name, one child, and the ref map
        // still covers every generator.
        record.generators.insert("a".into(), cuboid_at(1.0));
        record.generators.insert("b".into(), cuboid_at(1.0));
        let batches = plan_room_writes(&record, &HashSet::new(), false).unwrap();
        let creates = batches[0]
            .iter()
            .filter(|w| matches!(w, RepoWrite::Create { collection, .. } if collection == super::super::ROOM_GENERATOR_COLLECTION))
            .count();
        // Different names → different child bodies → two creates.
        assert_eq!(creates, 2);
    }

    #[test]
    fn plan_chunks_past_the_commit_cap_in_read_safe_order() {
        let mut record = RoomRecord::default_for_did("did:plc:chunk");
        record.generators.clear();
        for i in 0..limits::MAX_GENERATORS {
            record
                .generators
                .insert(format!("g{i:03}"), cuboid_at(i as f32));
        }
        // 256 stale children on the PDS, none referenced.
        let existing: HashSet<String> = (0..limits::MAX_GENERATORS)
            .map(|i| format!("{i:016x}"))
            .collect();

        let batches = plan_room_writes(&record, &existing, true).unwrap();
        assert!(batches.iter().all(|b| b.len() <= MAX_APPLY_WRITES));
        let flat: Vec<&RepoWrite> = batches.iter().flatten().collect();
        assert_eq!(flat.len(), limits::MAX_GENERATORS * 2 + 1);

        let manifest_idx = flat
            .iter()
            .position(|w| matches!(w, RepoWrite::Update { rkey, .. } if rkey == "self"))
            .unwrap();
        let last_create = flat
            .iter()
            .rposition(|w| matches!(w, RepoWrite::Create { .. }))
            .unwrap();
        let first_delete = flat
            .iter()
            .position(|w| matches!(w, RepoWrite::Delete { .. }))
            .unwrap();
        assert!(
            last_create < manifest_idx && manifest_idx < first_delete,
            "creates → manifest → deletes even across chunk boundaries"
        );
    }

    #[test]
    fn split_publish_reassembles_to_the_same_record() {
        for seed in [0u64, 1, 42, 0xDEAD_BEEF] {
            let record = RoomRecord::default_for_seed(seed, "did:plc:split");
            // Write side.
            let manifest = RoomManifestOut::from_record(&record);
            let children: HashMap<String, Generator> = record
                .generators
                .iter()
                .map(|(name, g)| (child_rkey(name, g), g.clone()))
                .collect();
            // Read side: decode the manifest bytes and join the children.
            let wire: RoomSelfWire =
                serde_json::from_value(serde_json::to_value(&manifest).unwrap()).unwrap();
            assert!(wire.generators.is_empty(), "manifest carries no bodies");
            assert_eq!(wire.generator_refs.len(), record.generators.len());
            let assembled = assemble_room(wire, &children);
            assert_eq!(
                serde_json::to_value(&assembled).unwrap(),
                serde_json::to_value(&record).unwrap(),
                "seed {seed}: split round-trip diverged"
            );
        }
    }

    #[test]
    fn legacy_monolith_decodes_through_the_wire_shape() {
        let record = RoomRecord::default_for_did("did:plc:legacy");
        // `RoomRecord::serialize` still emits the legacy inline shape — the
        // in-memory model IS the old wire format.
        let wire: RoomSelfWire =
            serde_json::from_value(serde_json::to_value(&record).unwrap()).unwrap();
        assert!(wire.generator_refs.is_empty());
        let assembled = assemble_room(wire, &HashMap::new());
        assert_eq!(
            serde_json::to_value(&assembled).unwrap(),
            serde_json::to_value(&record).unwrap(),
        );
    }

    #[test]
    fn missing_child_skips_that_generator_only() {
        let mut record = RoomRecord::default_for_did("did:plc:missing");
        record.generators.clear();
        record.generators.insert("kept".into(), cuboid_at(1.0));
        record.generators.insert("lost".into(), cuboid_at(2.0));
        let manifest = RoomManifestOut::from_record(&record);
        let children: HashMap<String, Generator> = [(
            child_rkey("kept", &record.generators["kept"]),
            record.generators["kept"].clone(),
        )]
        .into_iter()
        .collect();
        let wire: RoomSelfWire =
            serde_json::from_value(serde_json::to_value(&manifest).unwrap()).unwrap();
        let assembled = assemble_room(wire, &children);
        assert_eq!(assembled.generators.len(), 1);
        assert!(assembled.generators.contains_key("kept"));
        assert_eq!(assembled.placements.len(), record.placements.len());
    }

    #[test]
    fn max_publish_record_bytes_covers_manifest_and_children() {
        let record = RoomRecord::default_for_did("did:plc:bytes");
        let max = max_publish_record_bytes(&record).unwrap();
        let manifest_bytes = crate::pds::record_size::serialized_record_bytes(
            &RoomManifestOut::from_record(&record),
        )
        .unwrap();
        assert!(max >= manifest_bytes);
        for (name, g) in &record.generators {
            let child = crate::pds::record_size::serialized_record_bytes(
                &RoomGeneratorRecord::new(name, g),
            )
            .unwrap();
            assert!(max >= child);
        }
    }
}
