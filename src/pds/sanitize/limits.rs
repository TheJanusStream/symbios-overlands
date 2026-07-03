//! Maximum values allowed in a `RoomRecord` / `AvatarRecord` /
//! `InventoryRecord`. Record fields outside these bounds are clamped
//! rather than rejected so slightly exotic records from forward-
//! compatible clients still round-trip, but a weaponised payload cannot
//! force a runaway allocation.

/// Heightmap edge length (cells per side). 2048² ≈ 4M f32 cells ≈ 16 MiB.
pub const MAX_GRID_SIZE: u32 = 2048;
/// FBM / noise octaves.
pub const MAX_OCTAVES: u32 = 32;
/// Voronoi seed-point count.
pub const MAX_VORONOI_SEEDS: u32 = 10_000;
/// Voronoi terrace-level count.
pub const MAX_VORONOI_TERRACES: u32 = 64;
/// Hydraulic erosion drop count.
pub const MAX_EROSION_DROPS: u32 = 500_000;
/// Thermal erosion iteration count.
pub const MAX_THERMAL_ITERATIONS: u32 = 500;
/// Splat texture resolution per side (pixels).
pub const MAX_TEXTURE_SIZE: u32 = 4096;
/// Ground / rock generator octaves.
pub const MAX_GROUND_OCTAVES: u32 = 12;
pub const MAX_ROCK_OCTAVES: u32 = 16;
/// Per-axis count cap for grid-based procedural textures (window panes,
/// iron grille bars, ashlar courses, wainscoting panels). The texture
/// pipeline iterates over `count` cells per pixel; with the
/// `MAX_TEXTURE_SIZE` 4096² envelope a 64×64 grid fits well under a
/// second of compute, while a million-cell grid would spin
/// `AsyncComputeTaskPool` for hours.
pub const MAX_TEXTURE_GRID_AXIS: u32 = 64;
/// Cell-count cap for Voronoi-style cell textures (stained glass).
/// The seed iteration is 1-D rather than per-axis, so the budget is
/// linear in `cell_count` — 256 keeps the worst case bounded while
/// still allowing dense decorative panels.
pub const MAX_TEXTURE_VORONOI_CELLS: u32 = 256;
/// Leaf-pair count cap for the foliage twig generator. The twig
/// generator emits a leaf pair per slot along the stem; the texture
/// pipeline iterates over each pair per pixel, so the budget mirrors
/// the grid-axis cap.
pub const MAX_TEXTURE_LEAF_PAIRS: u32 = 32;
/// Per-feature count caps for the particle sprite-card generators. Each
/// bounds a per-pixel inner loop (arms / sides / waves) or a per-frame
/// sub-feature loop (snowflake branches, flower petals); the values mirror
/// the upstream generator's own internal clamps so a hostile record is
/// bounded even if a future upstream version relaxes them. The shared atlas
/// dimension cap lives at [`MAX_PARTICLE_ATLAS_DIM`].
pub const MAX_SPRITE_SPARK_POINTS: u32 = 12;
pub const MAX_SPRITE_SNOWFLAKE_ARMS: u32 = 8;
pub const MAX_SPRITE_SNOWFLAKE_BRANCH_PAIRS: u32 = 5;
pub const MAX_SPRITE_PUFF_OCTAVES: u32 = 8;
pub const MAX_SPRITE_RING_WAVES: u32 = 16;
pub const MAX_SPRITE_SHARD_SIDES: u32 = 9;
pub const MAX_SPRITE_FLOWER_PETALS: u32 = 12;
/// Scatter placement count.
pub const MAX_SCATTER_COUNT: u32 = 100_000;
/// L-system derivation iterations. 12 is already enough to blow out most
/// lexical grammars — anything beyond this is almost certainly an attack.
pub const MAX_LSYSTEM_ITERATIONS: u32 = 12;
/// L-system source / finalization code length in bytes.
pub const MAX_LSYSTEM_CODE_BYTES: usize = 16_384;
/// L-system mesh resolution (stroke segments per twig).
pub const MAX_LSYSTEM_MESH_RESOLUTION: u32 = 32;
/// CGA shape grammar source length in bytes. The upstream parser caps a
/// single rule body at 1024 ops + 64 variants; the same DoS pressure
/// applies at the source level — a megabyte of `Name --> Name | Name |
/// …` lines would still spend its budget inside `parse_rule` before any
/// derivation-time guard fires. 16 KiB matches the L-system code cap.
pub const MAX_SHAPE_SOURCE_BYTES: usize = 16_384;
/// CGA shape grammar root-rule identifier length. The upstream parser
/// rejects identifiers above 64 bytes; we clamp earlier so a hostile
/// record cannot smuggle a megabyte of Unicode through `kind_tag` /
/// editor labels before the parser ever sees it.
pub const MAX_SHAPE_ROOT_RULE_BYTES: usize = 64;
/// Maximum number of named material slots on a `Shape` generator. Each
/// slot may pin a baked foliage texture in `Assets<Image>`, so a record
/// with thousands of unused slots inflates GPU memory even before any
/// terminal references them.
pub const MAX_SHAPE_MATERIAL_SLOTS: usize = 64;
/// Per-axis footprint clamp (metres). 1 km is well past any plausible
/// authored building / district footprint and keeps the initial scope
/// finite so `Interpreter::derive` cannot be smuggled an `f64` infinity.
pub const MAX_SHAPE_FOOTPRINT: f32 = 1_000.0;
/// Maximum number of `Placement` entries per `RoomRecord`. Clamping
/// `Scatter.count` alone is not enough — a record with ten-thousand
/// single-count scatter entries still weaponises `compile_room_record`.
pub const MAX_PLACEMENTS: usize = 1_024;
/// Maximum number of named generators per `RoomRecord`. Every generator
/// also materialises per-peer state (L-system material cache, lookup
/// work in hot loops) so a record with a million generator entries
/// would still inflate memory and slow every `compile_room_record` pass
/// even if no placement referenced them.
pub const MAX_GENERATORS: usize = 256;
/// Horizontal cell spacing for the heightmap mesh. The lower bound keeps
/// the mesh finite (cell_scale feeds straight into the collider builder
/// and a NaN/zero would panic `avian3d`), and the upper bound caps the
/// total world extent at a sane radius even with MAX_GRID_SIZE.
pub const MIN_CELL_SCALE: f32 = 0.01;
pub const MAX_CELL_SCALE: f32 = 64.0;
/// Vertical scale applied to normalised heightmap samples. Same rationale:
/// clamp to a finite positive range so a corrupted record can't smuggle
/// NaN/infinity into `HeightMapMeshBuilder`.
pub const MIN_HEIGHT_SCALE: f32 = 0.01;
pub const MAX_HEIGHT_SCALE: f32 = 10_000.0;
/// FBM lacunarity — the per-octave frequency multiplier. Floored at `1.0`
/// (frequency never decreases across octaves) and capped at the terrain
/// editor's authoring ceiling. No headroom above the editor here on purpose:
/// the value-noise lattice wraps mod 256, so `base_frequency · lacunarity^k`
/// past a few hundred is pure aliasing with no visual payoff — and pushes the
/// `coord as i32` lattice index toward overflow. The float output stays finite
/// regardless (the noise table is bounded), so this bound is about sanity, not
/// the finiteness that keeps `build_heightfield_collider` from panicking.
pub const MAX_LACUNARITY: f32 = 4.0;
/// FBM base (octave-0) frequency. Capped at the editor's slider ceiling for the
/// same reason as [`MAX_LACUNARITY`] — beyond it the noise only aliases.
pub const MAX_BASE_FREQUENCY: f32 = 32.0;
/// Hydraulic-erosion sediment-capacity multiplier. A finite cap keeps the
/// `slope · vel · water · capacity_factor` product finite; the value is only
/// ever a multiplier (never a divisor — see `HydraulicErosion::erode`), so the
/// lower bound can safely be `0`. Carries the usual forward-compat headroom
/// past the editor's 64.0 slider (unlike the frequency bounds above, a larger
/// multiplier only scales erosion, it can't overflow a coordinate).
pub const MAX_CAPACITY_FACTOR: f32 = 256.0;
/// Maximum recursion depth for any generator's child tree. Deep
/// hierarchies cost an entity + Transform chain per node; 16 is well
/// past any plausible hand-authored assembly.
pub const MAX_GENERATOR_DEPTH: u32 = 16;
/// Maximum total node count (root + descendants) for a single named
/// generator's tree. A malicious record with a million children would
/// otherwise spawn a million Bevy entities + colliders on every compile.
pub const MAX_GENERATOR_NODES: u32 = 1024;
/// Maximum absolute `twist` angle (radians) applied across a primitive's
/// Y extent. Two full turns in either direction is well past any
/// sculpting need — anything beyond that is just geometry noise.
pub const MAX_TORTURE_TWIST: f32 = 4.0 * std::f32::consts::PI;
/// Maximum magnitude of the per-axis `taper` factor. Clamped below 1.0
/// so a tapered primitive never collapses its top (or bottom) to a
/// single point — we'd lose vertices and the collider builder would
/// start returning zero-volume hulls.
pub const MAX_TORTURE_TAPER: f32 = 0.99;
/// Maximum magnitude of any component of the `bend` vector (world-units
/// of vertex displacement at the shape's top). 10 m is already a
/// dramatic curl on a 1 m primitive; beyond that the vertex torture pass
/// produces visually degenerate meshes the collider can't hug.
pub const MAX_TORTURE_BEND: f32 = 10.0;
/// Maximum magnitude of any `shear` component (world-units of linear top
/// displacement). Shares the bend bound — both are top-displacement offsets,
/// and 10 m is already a dramatic lean on a 1 m primitive.
pub const MAX_TORTURE_SHEAR: f32 = 10.0;
/// Maximum `hollow` bore fraction. Floored below 1.0 so a hollowed primitive
/// always keeps a wall of finite thickness (a 1.0 bore would erase the solid
/// and leave the collider builder a zero-volume shell).
pub const MAX_HOLLOW: f32 = 0.95;
/// Maximum Gerstner amplitude / time multiplier on a Water surface.
/// Both feed shader uniforms and a runaway value produces NaN normals.
pub const MAX_WAVE_SCALE: f32 = 100.0;
pub const MAX_WAVE_SPEED: f32 = 100.0;
/// Maximum `flow_strength` (force-per-metre-submerged) on a Water
/// surface. Bounded so a hostile record can't apply a near-infinite
/// tangent force to every floating object — earth gravity is ~9.81, so
/// 10× free-fall is the upper bound for any reasonable river / waterfall
/// effect.
pub const MAX_WATER_FLOW_STRENGTH: f32 = 100.0;
/// Maximum amplitude multiplier on the per-volume avatar-wake effect.
/// The shader scales each impulse by `wake_strength`; values above ~5
/// produce surface deformations that read as cartoonish "tidal-wave"
/// trails. Bounded so a hostile record can't push a vertical spike.
pub const MAX_WATER_WAKE_STRENGTH: f32 = 5.0;
/// Maximum metres between wake ripple peaks. Below ~0.1 m the ripple
/// frequency aliases with the per-pixel footprint and reads as noise;
/// above ~10 m the ripple wavelength dominates the entire plane and
/// stops looking like a wake.
pub const MAX_WATER_WAKE_RIPPLE_WAVELENGTH: f32 = 10.0;
/// Maximum 1/e decay radius for one wake-sample's contribution. A
/// 50-metre wake from a single avatar covers most of a typical pond
/// and is the practical upper bound before the effect loses spatial
/// locality.
pub const MAX_WATER_WAKE_DECAY_RADIUS: f32 = 50.0;
/// Maximum URL length (bytes) for a [`crate::pds::SignSource::Url`]
/// payload. 2048 matches the de-facto browser cap and keeps a hostile
/// record from smuggling megabytes of inert string through the room
/// recipe.
pub const MAX_SIGN_URL_BYTES: usize = 2048;
/// Maximum DID / CID length (bytes) for a Sign source. ATProto DIDs are
/// well under 256 bytes and CIDs (base32 v1) are ~60 bytes; 256 matches
/// the existing Portal DID cap and gives forward-compat headroom.
pub const MAX_SIGN_DID_BYTES: usize = 256;
pub const MAX_SIGN_CID_BYTES: usize = 256;
/// Per-axis panel size (metres) for a Sign generator. Mirrors the
/// primitive `c_dim` envelope so a megastructure billboard stays within
/// the 100 m world-cell budget.
pub const MAX_SIGN_SIZE: f32 = 100.0;
/// Per-axis UV repeat factor for a Sign generator. Bounded to keep the
/// fragment shader from sampling at sub-pixel rates that pin the GPU
/// on a hostile record. The lower bound is non-zero so the fragment's
/// `uv * repeat` term doesn't collapse the texture to a single texel.
pub const MIN_SIGN_UV_REPEAT: f32 = 0.001;
pub const MAX_SIGN_UV_REPEAT: f32 = 1_000.0;
/// Per-axis UV offset for a Sign generator. Wraps in the sampler
/// regardless, so any reasonable bound is fine; 1_000 matches the
/// repeat envelope so the editor's drag widgets feel symmetric.
pub const MAX_SIGN_UV_OFFSET: f32 = 1_000.0;
/// Hard cap on simultaneously-alive particles per emitter. Each
/// particle is a Bevy entity in v1; 512 keeps a handful of emitters
/// per room well within the engine's per-frame entity-iteration
/// budget without precluding "fire" / "dust storm" densities.
pub const MAX_PARTICLES: u32 = 512;
/// Continuous emit rate in particles per second. With
/// `MAX_PARTICLES` already capping the steady-state population,
/// 256 / s lets a short-lived burst (~0.5 s) replenish the cap
/// without overshooting it dramatically.
pub const MAX_PARTICLE_RATE: f32 = 256.0;
/// Per-cycle burst-count cap. Mirrors the per-emitter cap so a
/// burst can fill the steady-state population in one shot but not
/// queue up an arbitrary one-frame spike.
pub const MAX_PARTICLE_BURST: u32 = 512;
/// Per-particle lifetime envelope (seconds). 30 s keeps a slow
/// trailing trail visible across a placement traversal without
/// allowing a permanent fog effect that would never decay.
pub const MIN_PARTICLE_LIFETIME: f32 = 0.01;
pub const MAX_PARTICLE_LIFETIME: f32 = 30.0;
/// Per-particle initial-speed envelope (metres per second).
pub const MAX_PARTICLE_SPEED: f32 = 1_000.0;
/// Magnitude cap on per-axis constant acceleration (m/s²). 100 is
/// already 10× free-fall so any reasonable wind / float effect fits
/// comfortably inside.
pub const MAX_PARTICLE_ACCEL: f32 = 100.0;
/// Cap on the gravity multiplier. Allowed to be negative so a
/// "smoke rises" effect doesn't need a custom force vector.
pub const MAX_PARTICLE_GRAVITY_MULT: f32 = 10.0;
/// Linear drag coefficient cap (per-second exponential damping).
pub const MAX_PARTICLE_DRAG: f32 = 100.0;
/// Per-particle quad-size envelope (metres). Lower bound is `0.0`
/// so a particle can fade out completely by end-of-life — a zero-
/// area quad simply draws nothing, matching the natural
/// "shrink to vanish" effect.
pub const MIN_PARTICLE_SIZE: f32 = 0.0;
pub const MAX_PARTICLE_SIZE: f32 = 100.0;
/// Inherit-velocity factor cap. `1` matches the emitter, `2` lets
/// exhaust-style effects jet ahead. Above 2 the trail decouples
/// visually and looks bug-y rather than stylish.
pub const MAX_PARTICLE_INHERIT_VELOCITY: f32 = 2.0;
/// Active-emit duration cap (seconds). Looping emitters use this as
/// the burst-cadence period.
pub const MIN_PARTICLE_DURATION: f32 = 0.01;
pub const MAX_PARTICLE_DURATION: f32 = 600.0;
/// Emitter-shape geometry caps (metres / radians).
pub const MAX_PARTICLE_SHAPE_RADIUS: f32 = 100.0;
pub const MAX_PARTICLE_SHAPE_HALF_EXTENT: f32 = 100.0;
pub const MAX_PARTICLE_SHAPE_HEIGHT: f32 = 100.0;
pub const MAX_PARTICLE_CONE_HALF_ANGLE: f32 = std::f32::consts::PI;
/// Per-axis sprite-sheet atlas dimension cap. 16 × 16 = 256 frames
/// is well past any plausible animated particle effect and keeps
/// the per-frame mesh cache bounded.
pub const MAX_PARTICLE_ATLAS_DIM: u32 = 16;
/// Frame-cycle FPS cap for `AnimationFrameMode::OverLifetime`. 60
/// matches the typical render cadence; values above that just
/// stutter visually since the tick system samples at frame rate.
pub const MAX_PARTICLE_FRAME_FPS: f32 = 60.0;

/// Max authored contact-effect recipes per room (#246). Excess
/// recipes are dropped (keeping the lexicographically-first by name so
/// the survivor set is deterministic across peers). Generous — a busy
/// room rarely needs more than a handful of distinct effects.
pub const MAX_CONTACT_RECIPES: usize = 64;
/// Cap on a recipe's `name` length (chars) — bounds a pathological
/// authored string without truncating any sane identifier.
pub const MAX_CONTACT_RECIPE_NAME: usize = 64;
/// Multiplier a recipe applies to the sample footprint when sizing its
/// emitter shape. Bounds a hostile value from producing a kilometre-
/// wide spawn volume.
pub const MAX_CONTACT_RADIUS_SCALE: f32 = 16.0;
/// Per-avatar emission cooldown cap (s) for a continuous recipe.
pub const MAX_CONTACT_COOLDOWN: f32 = 60.0;
/// Cap on a recipe's `min_speed` trigger gate (m/s) — finite, sane.
pub const MAX_CONTACT_MIN_SPEED: f32 = 1_000.0;
/// Cap on the per-frame particle ceiling a room may request.
pub const MAX_CONTACT_PARTICLES_PER_FRAME: u32 = 4_096;

// --- Contact decal effect (#261) ----------------------------------------
/// Lower bound (s) on an authored decal's time-to-live — a decal must
/// live at least long enough to be seen (and to never divide-by-zero in
/// the runtime `age / ttl` interpolant).
pub const MIN_CONTACT_DECAL_TTL: f32 = 0.05;
/// Upper bound (s) on a decal's time-to-live. Generous (a long-lived
/// scorch mark) but finite so a hostile value can't pin quads forever;
/// the global live cap still bounds the population regardless.
pub const MAX_CONTACT_DECAL_TTL: f32 = 600.0;
/// Cap (m) on a decal quad's start/end side length — bounds a hostile
/// value from carpeting the world with one quad.
pub const MAX_CONTACT_DECAL_SIZE: f32 = 64.0;
/// Cap (m) on the surface-normal lift used to avoid z-fighting. Small —
/// a large value would float the decal visibly off the ground.
pub const MAX_CONTACT_DECAL_NORMAL_OFFSET: f32 = 1.0;

// --- Contact audio cue (#262) -------------------------------------------
/// Cap on an authored cue's linear volume floor / per-speed gain — a
/// hostile value can't blow out a listener's ears or clip the mixer.
pub const MAX_CONTACT_AUDIO_VOLUME: f32 = 4.0;
/// Min/max playback-speed (pitch) multiplier. Bounded so a cue can't
/// be slowed to a multi-hour drone or sped into an inaudible chirp.
pub const MIN_CONTACT_AUDIO_PITCH: f32 = 0.1;
pub const MAX_CONTACT_AUDIO_PITCH: f32 = 4.0;
/// Cap on the random ± pitch jitter (multiplier units).
pub const MAX_CONTACT_AUDIO_PITCH_JITTER: f32 = 1.0;
/// Cap (chars) on an authored audio URL / DID / CID — bounds a
/// pathological string the same way the Sign sanitiser does.
pub const MAX_CONTACT_AUDIO_URL: usize = 2_048;
pub const MAX_CONTACT_AUDIO_ID: usize = 256;
