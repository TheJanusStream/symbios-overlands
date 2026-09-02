//! The DID-seeded room assembler: how a blank record becomes an inhabited
//! world.
//!
//! `RoomRecord::default_for_seed` used to live in `pds/room.rs` beside the
//! wire lexicon and the publish planner, which put a third of the terrain,
//! palette, scatter and settlement rules inside the file that defines the
//! record's serialised shape. They are different jobs with different
//! review needs: the record's shape is a compatibility contract with the
//! PDS, while this is the **determinism contract between peers** — every
//! client re-derives the same seeded room from the same DID, so a change
//! here changes what other people see standing in the same world (#1159).
//!
//! `RoomRecord::default_for_seed` and `default_for_did` remain, as one-line
//! forwarders into [`build_room`] and [`build_room_for_did`], so the sixty
//! or so call sites and every test keep the name they already use.
//!
//! Nothing here decides its own values: each rule is derived by a sibling
//! module of [`crate::seeded_defaults`] (palette, terrain, atmosphere,
//! scatters, siting, settlement, gateway, monument, audio) and this file
//! is where those derived shapes are *wired into a record* — the `apply_*`
//! helpers below are that wiring and nothing more.

use std::collections::HashMap;

use crate::pds::COLLECTION;
use crate::pds::PrimCommon;
use crate::pds::contact_effects::ContactEffects;
use crate::pds::generator::{Generator, GeneratorKind, ParticleParams, Placement, WaterSurface};
use crate::pds::room::{DefaultLanding, Environment, RoomRecord, generator_entity_count};
use crate::pds::terrain::SovereignTerrainConfig;
use crate::pds::types::{Fp, Fp2, Fp3, Fp4, Fp64, TransformData};

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
pub fn build_room_for_did(did: &str) -> RoomRecord {
    build_room(crate::seeded_defaults::fnv1a_64(did), did)
}

/// Per-instance entity ceiling for one seeded tree (#810). L-system
/// expansion is exponential in `iterations`, and the field census measured
/// 111 → 10,242 entities per tree across seeds — the deriver steps a
/// tree's iterations down until its measured expansion fits under this.
/// 1,600 keeps the lushest healthy species observed (~1,557/tree) intact
/// while amputating the order-of-magnitude outliers.
pub(crate) const TREE_ENTITY_BUDGET: u64 = 1_600;

/// Room-wide ceiling for **all** seeded vegetation entities — trees plus
/// ground cover — as Σ(scatter count × per-instance) (#810, #911).
///
/// ~8–10 % of seeds previously projected 300 k–978 k (the
/// `MAX_ROOM_ENTITIES` fail-stop is 500 k), which is a 1.4 fps slideshow
/// on wasm and the feed for the WebGL2 staging-pileup OOM (#811). 120 k
/// preserves the typical seeded region verbatim and scales down only the
/// outliers.
///
/// The two tiers share one ceiling rather than holding independent ones:
/// exceeding `MAX_ROOM_ENTITIES` makes the executor silently truncate and
/// abandon the rest of the placement queue — a hard visual cliff — so the
/// total is what has to be bounded. Sharing also lets the tiers trade
/// against each other: since #812 baked props into mesh buckets a tree
/// costs only a handful of entities, leaving most of this headroom for the
/// ground-cover tier that actually needs the instance count.
pub(crate) const ROOM_VEGETATION_ENTITY_BUDGET: u64 = 120_000;

/// Grid size of the derive-time proxy heightmap used for settlement
/// siting (#905). At the default 512-cell / ~2 m terrain this is a
/// ~10 m proxy cell — building-footprint scale, coarse enough that
/// the synchronous generation stays a low-single-digit-millisecond
/// cost inside `default_for_seed`.
const SETTLEMENT_PROXY_GRID: u32 = 96;

/// Build the seeded default room from a pre-computed seed — the
/// manual re-roll path. `seed` drives every derived value (terrain
/// shape, palette, atmosphere, scatters, landmark, audio, …); `did`
/// is kept only for the per-species generator builders that take the
/// local DID. `default_for_did` is exactly
/// `default_for_seed(fnv1a_64(did), did)`.
pub fn build_room(seed: u64, did: &str) -> RoomRecord {
    use crate::pds::generator::{
        AnimationFrameMode, EmitterShape, ParticleBlendMode, SimulationSpace, TextureFilter,
    };
    use crate::pds::texture::{
        SovereignMaterialSettings, SovereignRockConfig, SovereignTextureConfig,
    };
    use crate::pds::types::{BiomeFilter, ScatterBounds, WaterRelation};
    use crate::pds::{Fp2, TortureParams};
    use crate::seeded_defaults::{
        AmbientParticles, Atmosphere, BUILD_SLOPE_LIMIT, BiomeTextures, RockScatters, RoomPalette,
        SceneCharacter, SettlementPlan, TerrainProbe, TerrainShape, TreeScatters, WaterDynamics,
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
    let ground_cover =
        crate::seeded_defaults::room::GroundCoverScatters::from_scene(&scene, did_seed);
    let ambient_particles = AmbientParticles::from_scene(&scene, did_seed);

    let mut terrain_cfg = SovereignTerrainConfig {
        seed: did_seed,
        ..SovereignTerrainConfig::default()
    };
    apply_shape_to_terrain_config(&shape, &mut terrain_cfg);
    apply_palette_to_material(&palette, &mut terrain_cfg.material);
    apply_shape_to_material(&shape, &mut terrain_cfg.material);
    apply_textures_to_material(&textures, &mut terrain_cfg.material);
    apply_biome_signature_surface(scene.biome, did_seed, &palette, &mut terrain_cfg.material);

    // Terrain-aware settlement siting (#905): a low-resolution proxy
    // of the exact heightmap this config will generate, segmented
    // into buildable flat regions. Deterministic from the config, so
    // peers derive identical rooms. The water level uses the same
    // fraction × height-scale product the Water child below places
    // its surface at.
    let seeded_water_y = shape.water_level_fraction * shape.height_scale;
    let terrain_probe = TerrainProbe::new(
        &gen_jobs::run_heightmap_proxy(
            &crate::terrain::heightmap_params(&terrain_cfg),
            SETTLEMENT_PROXY_GRID,
        ),
        seeded_water_y,
        BUILD_SLOPE_LIMIT,
    );

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
    // seeded `height_scale` (`seeded_water_y`, computed above for
    // the terrain probe). Expressed as a fraction so a tall craggy
    // room and a short rolling room can both read as "30 %
    // submerged" — the absolute Y differs but the proportion of
    // land vs water stays meaningful. Archetype + biome biases
    // happen inside `TerrainShape::from_scene`.
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

    // Seeded rooms grow no road network: the RoadNetwork generator (and
    // its lot-building layer) is editor-opt-in only — the road graph +
    // up-to-hundreds of lot buildings are too heavy for a good default
    // room on wasm. Rooms that already carry a RoadNetwork child (saved
    // or authored) still mesh and populate as before.

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
    //
    // Two passes (#810): first build every scatter's tree and measure its
    // per-instance spawn cost (stepping iterations down under the
    // per-tree budget), then scale the scatter counts against the room
    // budget before any placement is pushed. The measurement is
    // deterministic from the seed, so peers derive identical rooms.
    let mut pending_tree_scatters = Vec::with_capacity(tree_scatters.scatters.len());
    for scatter in tree_scatters.scatters.iter() {
        let Some(species_entry) = crate::catalogue::by_slug(scatter.species.slug()) else {
            // Pool slugs are compile-time constants verified by the
            // landmark/scatter tests; an unresolved slug means a
            // catalogue rename and is safest skipped.
            continue;
        };
        let mut tree_gen = species_entry.build(did);
        // Apply this stand's material re-skin (#910). Purely a material
        // edit — geometry is untouched, so the L-system mesh cache
        // (keyed on the geometry fingerprint) still shares one derivation
        // across every variant of a species, while the separate material
        // cache keeps the variants' textures apart.
        if let GeneratorKind::LSystem { materials, .. } = &mut tree_gen.kind {
            crate::catalogue::items::plants::variant::apply_named(
                species_entry.variants(),
                scatter.variant,
                materials,
            );
        }
        // Non-L-system fallback: a plain generator spawns its own node.
        let mut per_tree_entities = 1u64;
        if let GeneratorKind::LSystem {
            source_code,
            finalization_code,
            iterations,
            seed,
            angle,
            step,
            width,
            elasticity,
            tropism,
            ..
        } = &mut tree_gen.kind
        {
            // The deriver only ever emits delta ∈ {-1, 0, +1}, but a
            // single step is enough to blow the exponential expansion up
            // an order of magnitude on lush species (#810 census:
            // 111 → 10,242 entities/tree across seeds), so the budget is
            // enforced by *measuring* the expansion below, not assumed
            // from the band. Clamp to ≥ 2 as belt-and-braces against
            // future catalogue tweaks.
            *iterations = (*iterations as i32 + scatter.iterations_delta).max(2) as u32;
            // #810 per-tree ceiling: step iterations down until the
            // measured expansion fits. A grammar error (`None`) is left
            // untouched — the spawn path skips those generators, so they
            // cost nothing either way.
            loop {
                match crate::world_builder::lsystem::lsystem_entity_estimate(
                    source_code,
                    finalization_code,
                    *iterations,
                    *seed,
                    *angle,
                    *step,
                    *width,
                    *elasticity,
                    *tropism,
                    scatter.species.slug(),
                ) {
                    Some(e) if e > TREE_ENTITY_BUDGET && *iterations > 2 => {
                        *iterations -= 1;
                    }
                    Some(e) => {
                        per_tree_entities = e;
                        break;
                    }
                    None => break,
                }
            }
        }
        pending_tree_scatters.push((scatter, tree_gen, per_tree_entities));
    }

    // Ground-cover tier (#911): the cheap card props below the trees.
    // These are primitive trees, not grammars, so their per-instance cost
    // is just the node count — exact, and no iteration stepping needed.
    let mut pending_ground_cover = Vec::with_capacity(ground_cover.scatters.len());
    for scatter in ground_cover.scatters.iter() {
        let Some(entry) = crate::catalogue::by_slug(scatter.species.slug()) else {
            // Pool slugs are compile-time constants guarded by the
            // deriver's catalogue-resolution test.
            continue;
        };
        let built = entry.build(did);
        let per_instance = generator_entity_count(&built);
        pending_ground_cover.push((scatter, built, per_instance));
    }

    // #810/#911 room budget: if the measured projection exceeds the shared
    // vegetation ceiling (dense biome × several lush scatters), scale every
    // scatter's count proportionally — both tiers thin uniformly instead of
    // one scatter vanishing or one tier starving the other. `max(1)` keeps
    // each stand and patch present so the biome still reads.
    let tree_projected: u64 = pending_tree_scatters
        .iter()
        .map(|(s, _, per_tree)| u64::from(s.count) * per_tree)
        .sum();
    let cover_projected: u64 = pending_ground_cover
        .iter()
        .map(|(s, _, per_instance)| u64::from(s.count) * per_instance)
        .sum();
    let projected = tree_projected.saturating_add(cover_projected);
    let scale = if projected > ROOM_VEGETATION_ENTITY_BUDGET {
        ROOM_VEGETATION_ENTITY_BUDGET as f64 / projected as f64
    } else {
        1.0
    };
    for (idx, (scatter, tree_gen, _)) in pending_tree_scatters.into_iter().enumerate() {
        let count = ((f64::from(scatter.count) * scale) as u32).max(1);
        let scatter_gen_name = format!("tree_scatter_{idx}");
        generators.insert(scatter_gen_name.clone(), tree_gen);
        placements.push(Placement::Scatter {
            generator_ref: scatter_gen_name,
            bounds: ScatterBounds::Circle {
                center: Fp2(scatter.center),
                radius: Fp(scatter.radius),
            },
            count,
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
            float_on_water: false,
            naturalness: {
                let mut n = crate::seeded_defaults::room::scatters::stand_naturalness(
                    scatter.species,
                    shape.height_scale,
                    seeded_water_y,
                );
                terrain_probe.relax_unsatisfiable_bands(&mut n, scatter.center, scatter.radius);
                n
            },
        });
    }

    for (idx, (scatter, cover_gen, _)) in pending_ground_cover.into_iter().enumerate() {
        let count = ((f64::from(scatter.count) * scale) as u32).max(1);
        let cover_gen_name = format!("ground_cover_{idx}");
        generators.insert(cover_gen_name.clone(), cover_gen);
        placements.push(Placement::Scatter {
            generator_ref: cover_gen_name,
            bounds: ScatterBounds::Circle {
                center: Fp2(scatter.center),
                radius: Fp(scatter.radius),
            },
            count,
            local_seed: scatter.local_seed,
            biome_filter: BiomeFilter {
                // Per-species (#913): the rock-colonising cushions are
                // allowed onto Rock, everything else keeps the walkable
                // land pair. A uniform Grass+Dirt list contradicted the
                // altitude bands, since high ground splats as Rock.
                biomes: scatter.species.biome_layers(),
                // Per-species too (#914): Above stays the default, and
                // only the aquatic cover (lilies, wading reeds) opts
                // into the water — the #335 lesson.
                water: scatter.species.water_relation(),
            },
            snap_to_terrain: true,
            random_yaw: true,
            // Unlike trees, ground cover is *welcome* in the settlement:
            // grass between the buildings is what makes a town look
            // planted rather than dropped onto bare ground.
            avoid_urban: false,
            // Species-opt-in water placement (#914/#335): only the
            // floating cover (lily pads) rides the water surface.
            float_on_water: scatter.species.floats_on_water(),
            naturalness: {
                let mut n = scatter
                    .species
                    .naturalness(shape.height_scale, seeded_water_y);
                // Habitat bands (#914) are exempt from relaxation: a
                // shoreline or shallows band covering a sliver of the
                // disc is the band working, not the band unsatisfiable
                // — see `water_band_is_habitat`.
                if !scatter.species.water_band_is_habitat() {
                    terrain_probe.relax_unsatisfiable_bands(&mut n, scatter.center, scatter.radius);
                }
                n
            },
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
        common: PrimCommon {
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
            float_on_water: false,
            naturalness: crate::seeded_defaults::room::rocks::field_naturalness(),
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

    // Seeded settlement, sited on the terrain (#905): the plan places
    // its clusters inside the probe's buildable flat regions — one
    // primary landmark cluster (kept under the historical "landmark"
    // generator name the gateway and compile layers key on), an
    // optional second landmark cluster on naturally-partitioned
    // landforms, and small hamlets in leftover regions. Shape-grammar
    // entries get their stochastic seed restamped per DID so two users
    // sharing a structure type still see different derivations; every
    // member snaps to terrain with its own water clearance.
    //
    // Built once here and reused by the gateway wiring below, which
    // anchors the gate to the primary landmark.
    let settlement_plan = SettlementPlan::from_scene_sited(&scene, did_seed, &terrain_probe);
    {
        let (prosperity, escalation) = (scene.prosperity, scene.escalation);
        for (ci, cluster) in settlement_plan.clusters.iter().enumerate() {
            if let Some(landmark) = &cluster.landmark {
                let name = if ci == 0 {
                    "landmark".to_string()
                } else {
                    format!("landmark_{ci}")
                };
                wire_settlement_member(
                    landmark,
                    &name,
                    did,
                    prosperity,
                    escalation,
                    &mut generators,
                    &mut placements,
                );
            }
            for (i, member) in cluster.secondaries.iter().enumerate() {
                let name = if ci == 0 {
                    format!("settlement_secondary_{i}")
                } else {
                    format!("settlement_c{ci}_secondary_{i}")
                };
                wire_settlement_member(
                    member,
                    &name,
                    did,
                    prosperity,
                    escalation,
                    &mut generators,
                    &mut placements,
                );
            }
            // Props are sampled with replacement, so the same prop can
            // recur (within and across clusters). Share one generator per
            // distinct prop slug (named by slug) and reference it from
            // each copy's placement — the compiler bakes that mesh once
            // and instances it, instead of carrying a near-duplicate
            // Region Asset per copy (mirrors the lot-building layer).
            for member in &cluster.props {
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
    }

    // Social gateway (#747, relocated #774, per-theme #749-772): every
    // seeded room gets one gate. The theme's bespoke gateway wins via
    // `entries_for(theme, Gateway)`; every `ThemeArchetype` has one, so
    // the `civic_gateway` cross-theme fallback below is only reached if
    // a future theme ships without a gate. The gate is a gatehouse on
    // the origin→landmark approach and the default landing sits just in
    // front of it facing the settlement, so visitors (and the owner on
    // login) arrive at the settlement frontage rather than the empty
    // region centre. (Caveat: the placement is water-avoiding, so on a
    // soaked bearing the compiled gate can walk off the recorded
    // landing — the landing still resolves its height from the
    // heightmap and stays functional.)
    let mut default_landing = None;
    let gateway_entry =
        crate::catalogue::entries_for(scene.theme, crate::catalogue::StructureRole::Gateway)
            .next()
            .or_else(|| crate::catalogue::by_slug("civic_gateway"));
    if let Some(entry) = gateway_entry {
        let gate_clearance = entry.footprint().clearance;
        let primary_landmark = settlement_plan.primary_landmark();
        let spot = crate::seeded_defaults::GatewaySpot::for_landmark(
            primary_landmark.offset,
            primary_landmark.clearance,
            gate_clearance,
        );
        let mut gate = entry.build(did);
        // Socio finish for material coherence — but no ruin pass: a
        // collapsed gate that still teleports reads as a bug, not
        // flavour.
        crate::pds::material_finish::apply_socio_finish(
            &mut gate,
            scene.prosperity,
            scene.escalation,
        );
        generators.insert("social_gateway".to_string(), gate);
        let half_yaw = spot.yaw_rad * 0.5;
        placements.push(Placement::Absolute {
            generator_ref: "social_gateway".to_string(),
            transform: TransformData {
                translation: Fp3([spot.offset[0], -0.35, spot.offset[1]]),
                rotation: Fp4([0.0, half_yaw.sin(), 0.0, half_yaw.cos()]),
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            snap_to_terrain: true,
            avoid_water: true,
            avoid_water_clearance: Fp(gate_clearance),
        });
        default_landing = Some(DefaultLanding {
            pos: Fp2(spot.landing),
            y: None,
            yaw_deg: Fp(spot.landing_yaw_deg),
        });

        // Owner monument (#975): the themed monument carrying the room
        // owner's profile picture, standing beside the gate and turned
        // toward the landing, so the first thing an arrival sees is whose
        // room they are in. Selected exactly like the gate — the theme's
        // bespoke entry wins via `entries_for(theme, Monument)`, with the
        // cross-theme `civic_monument` as the fallback.
        //
        // Nested inside the gateway block on purpose: the monument is
        // placed *relative to the gate*, so a room that somehow has no
        // gate has nothing to stand beside and simply goes without.
        //
        // Socio finish but no ruin pass, for the gate's reason turned
        // around: a collapsed gate that still teleports reads as a bug,
        // and a room owner's face on a smashed plinth reads as an insult.
        // The finish still carries the room's prosperity into its
        // materials, so a poor room's monument is a humbler one.
        let monument_entry =
            crate::catalogue::entries_for(scene.theme, crate::catalogue::StructureRole::Monument)
                .next()
                .or_else(|| crate::catalogue::by_slug("civic_monument"));
        if let Some(entry) = monument_entry {
            let clearance = entry.footprint().clearance;
            let mspot = crate::seeded_defaults::MonumentSpot::beside_gate(
                &spot,
                gate_clearance,
                clearance,
                did,
            );
            let mut monument = entry.build(did);
            crate::pds::material_finish::apply_socio_finish(
                &mut monument,
                scene.prosperity,
                scene.escalation,
            );
            generators.insert("owner_monument".to_string(), monument);
            let half_yaw = mspot.yaw_rad * 0.5;
            placements.push(Placement::Absolute {
                generator_ref: "owner_monument".to_string(),
                transform: TransformData {
                    translation: Fp3([mspot.offset[0], -0.35, mspot.offset[1]]),
                    rotation: Fp4([0.0, half_yaw.sin(), 0.0, half_yaw.cos()]),
                    scale: Fp3([1.0, 1.0, 1.0]),
                },
                snap_to_terrain: true,
                avoid_water: true,
                avoid_water_clearance: Fp(clearance),
            });
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

    RoomRecord {
        lex_type: COLLECTION.into(),
        environment,
        generators,
        placements,
        traits,
        contact_effects: ContactEffects::default(),
        default_landing,
        opaque_refs: std::collections::BTreeMap::new(),
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
    cfg.generator_kind = shape.generator_kind;
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
/// the new generator's own shape rather than a seeded Ground config.
/// The splat *rules* (height/slope → layer) are untouched, so layer 0 still
/// paints low/flat ground and layer 3 the high peaks.
///
/// The `palette` argument is what keeps the swap from going colour-blind.
/// Replacing a layer wholesale discards the palette
/// [`apply_palette_to_material`] just wrote, and the Ground/Rock-shaped
/// guards there cannot reach a Sand or Ice layer afterwards — so without
/// this every arid room shared one sand, and every glacier one ice. Each
/// signature surface is therefore built *with* the room's own colours,
/// mapped onto whichever of its fields carries the same meaning.
fn apply_biome_signature_surface(
    biome: crate::seeded_defaults::BiomeArchetype,
    seed: u64,
    palette: &crate::seeded_defaults::RoomPalette,
    material: &mut crate::pds::terrain::SovereignMaterialConfig,
) {
    use crate::pds::texture::{
        SovereignCrackedEarthConfig, SovereignForestFloorConfig, SovereignGravelConfig,
        SovereignIceConfig, SovereignLavaConfig, SovereignLichenConfig, SovereignMossConfig,
        SovereignSandConfig, SovereignSnowConfig, SovereignTextureConfig as T,
    };
    use crate::seeded_defaults::BiomeArchetype;

    let sig = (seed ^ 0x5163_0001) as u32;

    // Sand is dry earth, so it takes the room's dirt pair: the dry tone
    // catches the ripple crests, the moist tone sits in the troughs.
    let sand = |seed| {
        T::Sand(SovereignSandConfig {
            seed,
            color_crest: Fp3(palette.dirt_dry),
            color_trough: Fp3(palette.dirt_moist),
            ..Default::default()
        })
    };
    let snow = |seed| {
        T::Snow(SovereignSnowConfig {
            seed,
            color_snow: Fp3(palette.snow_dry),
            color_shadow: Fp3(palette.snow_moist),
            ..Default::default()
        })
    };
    // Dried lakebed: the plate is the room's dry earth, and the crack holds
    // the damper silt that has not baked out yet.
    let cracked_earth = |seed| {
        T::CrackedEarth(SovereignCrackedEarthConfig {
            seed,
            color_plate: Fp3(palette.dirt_dry),
            color_crack: Fp3(palette.dirt_moist),
            ..Default::default()
        })
    };
    // Scree is the room's own rock broken up, with its dust between.
    let gravel = |seed| {
        T::Gravel(SovereignGravelConfig {
            seed,
            color_stone: Fp3(palette.rock_stone),
            color_dark: Fp3(palette.rock_gap),
            color_fines: Fp3(palette.dirt_dry),
            ..Default::default()
        })
    };
    // Litter over humus: fallen leaves keep the room's dry-vegetation tone,
    // ageing down toward its earth colours.
    let forest_floor = |seed| {
        T::ForestFloor(SovereignForestFloorConfig {
            seed,
            color_humus: Fp3(palette.dirt_moist),
            color_leaf: Fp3(palette.grass_dry),
            color_leaf_old: Fp3(palette.dirt_dry),
            ..Default::default()
        })
    };

    match biome {
        BiomeArchetype::Arid | BiomeArchetype::Coastal | BiomeArchetype::Savanna => {
            material.layers[0] = sand(sig);
        }
        // Badlands are eroded, not drifted: a dry cracked pan rather than
        // dunes, with the broken rock of the scarps banding above it.
        BiomeArchetype::Badlands => {
            material.layers[0] = cracked_earth(sig);
            material.layers[1] = gravel(sig ^ 0x00A1);
        }
        BiomeArchetype::Volcanic => {
            material.layers[0] = T::Lava(SovereignLavaConfig {
                seed: sig,
                // Cooled basalt is the room's own rock, seen in its darkest
                // tone. The glow is deliberately left at the generator
                // default: it is fire, not terrain, and tinting it with a
                // room palette would drain the heat out of it.
                color_crust: Fp3(palette.rock_gap),
                ..Default::default()
            });
        }
        BiomeArchetype::Tundra => {
            material.layers[3] = snow(sig);
            // Frost-shattered stone works its way up through the thin soil.
            material.layers[1] = gravel(sig ^ 0x00A1);
        }
        // Above the treeline the flat ground is loose scree, not turf.
        BiomeArchetype::Alpine => {
            material.layers[0] = gravel(sig);
            material.layers[3] = snow(sig);
        }
        // Conifer needle litter under the snowline.
        BiomeArchetype::Boreal => {
            material.layers[0] = forest_floor(sig);
            material.layers[3] = snow(sig);
        }
        BiomeArchetype::Glacial => {
            // Crevassed blue ice on the valley floor, snowfields on top.
            // Glacier ice reads as frozen water, so it takes the room's own
            // water colours rather than a fixed blue.
            material.layers[0] = T::Ice(SovereignIceConfig {
                seed: sig,
                color_ice: Fp3([
                    palette.water_shallow[0],
                    palette.water_shallow[1],
                    palette.water_shallow[2],
                ]),
                color_crack: Fp3([
                    palette.water_deep[0],
                    palette.water_deep[1],
                    palette.water_deep[2],
                ]),
                ..Default::default()
            });
            material.layers[3] = snow(sig);
        }
        // Standing water keeps the ground permanently sodden, so the flat
        // layer is moss rather than grass.
        BiomeArchetype::Wetland => {
            material.layers[0] = T::Moss(SovereignMossConfig {
                seed: sig,
                color_tip: Fp3(palette.grass_moist),
                color_deep: Fp3(palette.grass_dry),
                ..Default::default()
            });
        }
        // Broadleaf litter carpets the floor under a closed canopy.
        BiomeArchetype::TemperateForest | BiomeArchetype::Jungle => {
            material.layers[0] = forest_floor(sig);
        }
        // Lush and Meadow are grassland: the grassy Ground stack is the
        // right answer for them, not a gap waiting to be filled.
        BiomeArchetype::Lush | BiomeArchetype::Meadow => {}
    }

    // Above the treeline bare stone crusts over with lichen, so the rock
    // layer itself changes rather than the ground beneath it.
    if matches!(biome, BiomeArchetype::Alpine | BiomeArchetype::Tundra) {
        material.layers[2] = T::Lichen(SovereignLichenConfig {
            seed: sig,
            color_rock: Fp3(palette.rock_stone),
            ..Default::default()
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A swapped signature layer must carry the room's own colours.
    ///
    /// Replacing a layer discards whatever `apply_palette_to_material` wrote,
    /// and its Ground/Rock-shaped guards cannot reach a Sand or Ice layer
    /// afterwards — so before this was wired through, every arid room shared
    /// one sand and every glacier one ice, however different their palettes.
    #[test]
    fn signature_surfaces_take_the_room_palette() {
        use crate::pds::texture::SovereignTextureConfig as T;
        use crate::seeded_defaults::{BiomeArchetype, RoomPalette, SceneCharacter};

        let fresh = crate::pds::terrain::SovereignMaterialConfig::default;
        let palette_for =
            |seed: u64| RoomPalette::from_scene(&SceneCharacter::for_seed(seed), seed);

        // Sand takes the room's dirt pair.
        let palette = palette_for(9);
        let mut m = fresh();
        apply_biome_signature_surface(BiomeArchetype::Arid, 9, &palette, &mut m);
        match &m.layers[0] {
            T::Sand(sand) => {
                assert_eq!(sand.color_crest.0, palette.dirt_dry, "sand crest");
                assert_eq!(sand.color_trough.0, palette.dirt_moist, "sand trough");
            }
            other => panic!("arid → sand, got {other:?}"),
        }

        // Glacier ice takes the room's water colours.
        let mut m = fresh();
        apply_biome_signature_surface(BiomeArchetype::Glacial, 9, &palette, &mut m);
        match &m.layers[0] {
            T::Ice(ice) => {
                assert_eq!(ice.color_ice.0[0], palette.water_shallow[0], "ice tint");
                assert_eq!(ice.color_crack.0[0], palette.water_deep[0], "crack tint");
            }
            other => panic!("glacial → ice, got {other:?}"),
        }

        // Two rooms with genuinely different palettes must not bake the same
        // sand — the whole point of routing the palette through.
        let (a, b) = (palette_for(9), palette_for(4242));
        if a.dirt_dry != b.dirt_dry {
            let mut ma = fresh();
            let mut mb = fresh();
            apply_biome_signature_surface(BiomeArchetype::Arid, 9, &a, &mut ma);
            apply_biome_signature_surface(BiomeArchetype::Arid, 9, &b, &mut mb);
            assert_ne!(
                ma.layers[0], mb.layers[0],
                "different palettes produced identical sand"
            );
        }

        // Lava keeps its glow: fire is not terrain, and tinting it with a
        // room palette would drain the heat out of it.
        let mut m = fresh();
        apply_biome_signature_surface(BiomeArchetype::Volcanic, 9, &palette, &mut m);
        match &m.layers[0] {
            T::Lava(lava) => {
                assert_eq!(lava.color_crust.0, palette.rock_gap, "crust from rock");
                let default_glow = crate::pds::texture::SovereignLavaConfig::default().color_glow;
                assert_eq!(lava.color_glow.0, default_glow.0, "glow must stay molten");
            }
            other => panic!("volcanic → lava, got {other:?}"),
        }
    }

    /// The full splat stack every biome ends up with, layer by layer.
    ///
    /// Stated as a table rather than a handful of spot checks: the biome map
    /// is the one place where a change silently repaints entire regions, so
    /// every future edit should have to say out loud which layer it moved.
    /// Layers are R=low/flat, G=dirt band, B=rock, A=high.
    #[test]
    fn biome_signature_surface_swaps_expected_layer() {
        use crate::seeded_defaults::{BiomeArchetype as B, RoomPalette, SceneCharacter};

        let palette = RoomPalette::from_scene(&SceneCharacter::for_seed(9), 9);

        // (biome, [layer0, layer1, layer2, layer3])
        let expected: [(B, [&str; 4]); 14] = [
            // Drylands: drifted sand, except the eroded badlands.
            (B::Arid, ["Sand", "Ground", "Rock", "Ground"]),
            (B::Coastal, ["Sand", "Ground", "Rock", "Ground"]),
            (B::Savanna, ["Sand", "Ground", "Rock", "Ground"]),
            (B::Badlands, ["Cracked Earth", "Gravel", "Rock", "Ground"]),
            (B::Volcanic, ["Lava", "Ground", "Rock", "Ground"]),
            // Cold: lichen-crusted rock above the treeline, scree on the
            // alpine flats, needle litter in the boreal forest.
            (B::Tundra, ["Ground", "Gravel", "Lichen", "Snow"]),
            (B::Alpine, ["Gravel", "Ground", "Lichen", "Snow"]),
            (B::Boreal, ["Forest Floor", "Ground", "Rock", "Snow"]),
            (B::Glacial, ["Ice", "Ground", "Rock", "Snow"]),
            // Verdant: litter under a canopy, moss on sodden ground, and
            // plain grass where grass is the right answer.
            (
                B::TemperateForest,
                ["Forest Floor", "Ground", "Rock", "Ground"],
            ),
            (B::Jungle, ["Forest Floor", "Ground", "Rock", "Ground"]),
            (B::Wetland, ["Moss", "Ground", "Rock", "Ground"]),
            (B::Lush, ["Ground", "Ground", "Rock", "Ground"]),
            (B::Meadow, ["Ground", "Ground", "Rock", "Ground"]),
        ];

        for (biome, layers) in expected {
            let mut m = crate::pds::terrain::SovereignMaterialConfig::default();
            apply_biome_signature_surface(biome, 9, &palette, &mut m);
            for (i, want) in layers.iter().enumerate() {
                assert_eq!(
                    &m.layers[i].label(),
                    want,
                    "{biome:?} layer{i}: expected {want}, got {}",
                    m.layers[i].label()
                );
            }
        }
    }

    /// Every biome that is not grassland should have *something* of its own
    /// on the splat stack — the gap this map exists to close.
    #[test]
    fn only_grassland_biomes_keep_the_plain_ground_stack() {
        use crate::seeded_defaults::{BiomeArchetype as B, RoomPalette, SceneCharacter};

        let palette = RoomPalette::from_scene(&SceneCharacter::for_seed(9), 9);
        let plain = crate::pds::terrain::SovereignMaterialConfig::default();

        for biome in [
            B::Arid,
            B::Coastal,
            B::Savanna,
            B::Badlands,
            B::Volcanic,
            B::Tundra,
            B::Alpine,
            B::Boreal,
            B::Glacial,
            B::TemperateForest,
            B::Jungle,
            B::Wetland,
        ] {
            let mut m = crate::pds::terrain::SovereignMaterialConfig::default();
            apply_biome_signature_surface(biome, 9, &palette, &mut m);
            assert_ne!(
                m.layers, plain.layers,
                "{biome:?} still renders as the untouched ground stack"
            );
        }

        // Grassland keeps grass on purpose.
        for biome in [B::Lush, B::Meadow] {
            let mut m = crate::pds::terrain::SovereignMaterialConfig::default();
            apply_biome_signature_surface(biome, 9, &palette, &mut m);
            assert_eq!(
                m.layers, plain.layers,
                "{biome:?} should keep the grassy stack"
            );
        }
    }

    #[test]
    fn default_room_carries_a_themed_settlement() {
        use crate::seeded_defaults::room::settlement::MAX_SECONDARIES;
        for s in 0u64..16 {
            let did = format!("did:test:{s}");
            let record = RoomRecord::default_for_did(&did);

            // Every room carries exactly one landmark, and it's a
            // building — never Terrain/Water (those are positionally
            // invalid outside the base_terrain tree).
            let landmark = record
                .generators
                .get("landmark")
                .expect("every seeded room must carry a landmark generator");
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
                // Sited members clear the ±5 m spawn-scatter square (#905);
                // non-primary clusters may legitimately sit nearer than the
                // old 30–60 m spawn ring did.
                assert!(
                    dist >= 9.5,
                    "settlement member {name} too close to spawn: {dist} m"
                );
            }

            // `settlement_secondary_*` names are primary-cluster only
            // (other clusters use `settlement_c<N>_secondary_*`), so the
            // per-cluster band cap applies directly. Prop generators are
            // deduped by slug across every cluster; bound them by the
            // room-wide member ceiling instead of the per-cluster band.
            assert!(
                secondaries <= MAX_SECONDARIES,
                "too many secondaries: {secondaries}"
            );
            assert!(props <= 20, "too many distinct props: {props}");
        }
    }

    #[test]
    fn settlement_props_dedupe_to_one_generator_per_slug() {
        // Props are sampled with replacement (within and across clusters,
        // #905), so some room's record must carry more prop placements
        // than prop generators — the dedup actually collapsing copies.
        // Structural invariants are asserted for every room checked along
        // the way; the search stops at the first room that repeats.
        let mut collapsed = None;
        for s in 0u64..64 {
            let did = format!("did:test:{s}");
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
            assert!(
                prop_gens <= prop_placements,
                "{did}: more prop generators than placements"
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
            if prop_gens < prop_placements {
                collapsed = Some((did, prop_gens, prop_placements));
                break;
            }
        }
        let (did, gens, placements) =
            collapsed.expect("no seed in 0..64 repeated a settlement prop");
        assert!(
            gens < placements,
            "{did}: dedup must collapse repeated props ({gens} gens for {placements} placements)"
        );
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

    /// #810 acceptance: the seeded tree scatters respect both entity
    /// budgets. Seeds 4 / 11 / 46 are the field-census worst offenders —
    /// pre-clamp they projected ~607k / ~978k / ~346k tree entities (the
    /// 500k `MAX_ROOM_ENTITIES` fail-stop territory, a 1.4 fps slideshow on
    /// wasm and the feeder for the #811 staging-pileup OOM). The estimate
    /// here re-derives each placed tree exactly as the compile will, so this
    /// guards the whole clamp chain, not just the arithmetic.
    #[test]
    fn seeded_vegetation_scatters_respect_entity_budgets() {
        use crate::pds::generator::Placement;
        for room_seed in [4u64, 11, 46] {
            let record = RoomRecord::default_for_seed(room_seed, "did:test:budget");
            let mut projected = 0u64;
            for placement in &record.placements {
                let Placement::Scatter {
                    generator_ref,
                    count,
                    ..
                } = placement
                else {
                    continue;
                };
                // Ground cover shares the ceiling with the trees (#911), so it
                // has to be counted here or the guard measures half the load.
                // Its props are primitive trees, so the node count is exact.
                if generator_ref.starts_with("ground_cover_") {
                    let generator = record
                        .generators
                        .get(generator_ref)
                        .expect("scatter references a derived generator");
                    projected += generator_entity_count(generator) * u64::from(*count);
                    continue;
                }
                if !generator_ref.starts_with("tree_scatter_") {
                    continue;
                }
                let generator = record
                    .generators
                    .get(generator_ref)
                    .expect("scatter references a derived generator");
                let GeneratorKind::LSystem {
                    source_code,
                    finalization_code,
                    iterations,
                    seed,
                    angle,
                    step,
                    width,
                    elasticity,
                    tropism,
                    ..
                } = &generator.kind
                else {
                    continue;
                };
                let per_tree = crate::world_builder::lsystem::lsystem_entity_estimate(
                    source_code,
                    finalization_code,
                    *iterations,
                    *seed,
                    *angle,
                    *step,
                    *width,
                    *elasticity,
                    *tropism,
                    generator_ref,
                )
                .expect("seeded species grammars are valid");
                assert!(
                    per_tree <= TREE_ENTITY_BUDGET,
                    "seed {room_seed}: {generator_ref} expands to {per_tree} entities/tree"
                );
                projected += per_tree * u64::from(*count);
            }
            assert!(
                projected <= ROOM_VEGETATION_ENTITY_BUDGET,
                "seed {room_seed}: {projected} projected vegetation entities"
            );
        }
    }

    /// The ground-cover tier must actually reach the record, and — unlike the
    /// trees and boulders — must deliberately *not* avoid the urban district:
    /// grass between the buildings is what makes a settlement look planted
    /// rather than dropped onto bare ground. Asserted so the opt-out reads as
    /// intentional rather than as a missed flag.
    #[test]
    fn seeded_ground_cover_is_emitted_and_grows_inside_settlements() {
        use crate::pds::generator::Placement;
        let record = (0u64..64)
            .map(|s| RoomRecord::default_for_did(&format!("did:test:gc{s}")))
            .find(|r| {
                r.placements.iter().any(|p| {
                    matches!(p, Placement::Scatter { generator_ref, .. }
                        if generator_ref.starts_with("ground_cover_"))
                })
            })
            .expect("a seed with ground cover");

        let mut covered = 0;
        for p in &record.placements {
            if let Placement::Scatter {
                generator_ref,
                avoid_urban,
                count,
                ..
            } = p
                && generator_ref.starts_with("ground_cover_")
            {
                covered += 1;
                assert!(
                    !*avoid_urban,
                    "{generator_ref} should grow inside the settlement"
                );
                assert!(*count > 0, "{generator_ref} placed zero instances");
                assert!(
                    record.generators.contains_key(generator_ref),
                    "{generator_ref} placement has no matching generator"
                );
            }
        }
        assert!(covered > 0);
    }

    /// Glacial rooms stay lifeless — the epic's binding decision.
    #[test]
    fn glacial_rooms_grow_no_ground_cover() {
        use crate::pds::generator::Placement;
        use crate::seeded_defaults::scene::{BiomeArchetype, SceneCharacter};
        let Some(seed) =
            (0u64..4096).find(|s| SceneCharacter::for_seed(*s).biome == BiomeArchetype::Glacial)
        else {
            panic!("no glacial seed found in the search range");
        };
        let record = RoomRecord::default_for_seed(seed, "did:test:glacial");
        assert!(
            !record.placements.iter().any(|p| matches!(
                p,
                Placement::Scatter { generator_ref, .. }
                    if generator_ref.starts_with("ground_cover_")
            )),
            "glacial seed {seed} grew ground cover"
        );
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
    fn seeded_rooms_grow_no_road_network() {
        // Roads (and the lot buildings they spawn) are too heavy for a good
        // default-room experience on wasm, so the RoadNetwork generator is
        // editor-opt-in only — no seeded room may carry one.
        for s in 0u64..64 {
            let record = RoomRecord::default_for_did(&format!("did:test:{s}"));
            assert!(
                crate::pds::room::find_road_config(&record).is_none(),
                "seeded room did:test:{s} must not carry a road network"
            );
        }
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
