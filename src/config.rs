//! Centralised configuration constants for Symbios Overlands.
//!
//! All tuneable values live here so they are easy to locate and adjust without
//! hunting through individual modules.  Modules mirror the source file that
//! consumes each constant group.

// ---------------------------------------------------------------------------
// Lighting (lib.rs)
// ---------------------------------------------------------------------------
pub mod lighting {
    /// Illuminance of the sun-like directional light (lux).
    pub const ILLUMINANCE: f32 = 15_000.0;
    /// Brightness of the scene-wide ambient light.
    pub const AMBIENT_BRIGHTNESS: f32 = 400.0;
    /// World-space position of the directional light source.
    pub const LIGHT_POS: [f32; 3] = [50.0, 40.0, 50.0];
    /// Sun colour (warm daylight, sRGB).
    pub const SUN_COLOR: [f32; 3] = [0.98, 0.95, 0.82];
    /// Cascade shadow: near-plane distance of the first cascade (m).
    pub const CASCADE_FIRST_FAR: f32 = 15.0;
    /// Cascade shadow: maximum shadow-casting distance (m).
    pub const CASCADE_MAX_DIST: f32 = 200.0;

    /// Sky-box colour (unlit grey, tinted by fog). sRGB hex ≈ #888888.
    pub const SKY_COLOR: [f32; 3] = [0.533, 0.533, 0.533];
    /// Uniform scale of the sky cuboid (must exceed max view distance).
    pub const SKY_SCALE: f32 = 2000.0;

    /// Defaults for the procedural cloud-deck layer authored on
    /// `pds::Environment`. The deck is a single horizontal plane at altitude
    /// `HEIGHT` rendered with a WGSL fragment shader that synthesises
    /// domain-warped FBM clouds, threshold-shaped by `COVER` and softened by
    /// `SOFTNESS`, drifting in `wind_dir` at `SPEED` m/s, and fading into the
    /// distance-fog colour at the horizon. Picked to look correct in WebGL2
    /// without compute shaders or volumetric passes.
    pub mod clouds {
        /// Fraction of sky covered by clouds. `0` is empty blue, `1` is
        /// totally overcast.
        pub const COVER: f32 = 0.45;
        /// Opacity multiplier for the clouds that survive the cover
        /// threshold. Lets the user dial down a fully overcast sky into a
        /// thin haze without remixing the noise field.
        pub const DENSITY: f32 = 0.85;
        /// Edge-softness band (in noise-value units) around the cover
        /// threshold. Larger values produce wispy clouds; smaller produces
        /// crisper towers.
        pub const SOFTNESS: f32 = 0.18;
        /// Cloud drift speed (m/s) along `WIND_DIR`.
        pub const SPEED: f32 = 4.0;
        /// World metres per UV unit for the FBM sampler. Larger ⇒ bigger
        /// individual cloud structures.
        pub const SCALE: f32 = 320.0;
        /// Altitude (m) of the cloud-deck plane in world space.
        pub const HEIGHT: f32 = 250.0;
        /// Sunlit-top tint (sRGB), warm-white by default.
        pub const COLOR: [f32; 3] = [1.0, 0.98, 0.94];
        /// Underside / shadowed tint (sRGB), cool-grey by default. Mixed
        /// with `COLOR` by the dot of the sun direction with world Y so a
        /// low sun produces moody undersides without a real lighting pass.
        pub const SHADOW_COLOR: [f32; 3] = [0.55, 0.62, 0.72];
        /// 2D drift direction in world XZ (will be normalised by the
        /// shader). Need not be unit length here.
        pub const WIND_DIR: [f32; 2] = [1.0, 0.3];
        /// Half-extent (m) of the cloud-deck plane mesh. Chosen well past
        /// any reasonable `fog_visibility` so the plane edge is never
        /// inside the visible fog band at any pitch.
        pub const PLANE_HALF_EXTENT: f32 = 4_000.0;
    }
}

// ---------------------------------------------------------------------------
// Rover (player/ + network/)
// ---------------------------------------------------------------------------
pub mod rover {
    // --- Suspension (Hooke's law + damping) ----------------------------------
    pub const SUSPENSION_REST_LENGTH: f32 = 0.8;
    pub const SUSPENSION_STIFFNESS: f32 = 4_200.0;
    pub const SUSPENSION_DAMPING: f32 = 175.0;
    /// Ray-cast length = rest length + this overshoot past the contact plane.
    pub const RAY_MAX_DIST: f32 = SUSPENSION_REST_LENGTH + 1.5;

    // --- Drive ---------------------------------------------------------------
    pub const DRIVE_FORCE: f32 = 1_800.0;
    pub const TURN_TORQUE: f32 = 400.0;
    /// Longitudinal speed (m/s) past which — while reversing — the steer
    /// response inverts. A real car's heading turns the opposite way for a
    /// fixed wheel angle in reverse vs. forward; this deadband keeps
    /// turn-in-place and the forward sign around a standstill so the sign
    /// doesn't flip on the sub-metre-per-second creep of a car nominally at
    /// rest (e.g. rolling back a touch on a slope).
    pub const REVERSE_STEER_SPEED: f32 = 0.5;
    pub const LATERAL_GRIP: f32 = 6_000.0;
    pub const JUMP_FORCE: f32 = 2_500.0;
    /// Torque strength nudging the chassis back to upright.
    pub const UPRIGHTING_TORQUE: f32 = 800.0;

    // --- Car rollover stability + recovery (#804) ----------------------------
    // The CoM fraction, engage tilt, righting acceleration, and righting
    // damping moved onto `CarParams` as record tuning (#876) — their defaults
    // there are the values that used to live here.
    /// Below this squared magnitude the `up × world-up` righting axis has
    /// degenerated (the chassis is dead-inverted — a saddle it could perch on),
    /// so the assist falls back to the roll axis to tip it off its roof.
    pub const CAR_UPRIGHT_DEGENERATE_SQ: f32 = 1.0e-4;
    /// Metres *below local ground* at which the rover is considered fallen
    /// through the terrain and respawned. Using a ground-relative delta
    /// rather than an absolute world-Y threshold keeps the respawn system
    /// from soft-locking on rooms whose `height_scale` sinks the entire
    /// heightmap far below the origin.
    pub const FALL_BELOW_GROUND: f32 = 20.0;

    // --- Chassis -------------------------------------------------------------
    pub const LINEAR_DAMPING: f32 = 1.5;
    pub const ANGULAR_DAMPING: f32 = 6.0;
    pub const MASS: f32 = 50.0;
    /// Chassis half-extents (local space).
    pub const CHASSIS_X: f32 = 0.8;
    pub const CHASSIS_Y: f32 = 0.2;
    pub const CHASSIS_Z: f32 = 1.2;

    // --- Spawn ---------------------------------------------------------------
    /// How many metres above the terrain surface the rover is placed at spawn.
    pub const SPAWN_HEIGHT_OFFSET: f32 = 1.0;
    /// Side length (m) of the square spawn-scatter region centred on the map.
    pub const SPAWN_SCATTER_SIZE: f32 = 10.0;

    // --- Buoyancy (swimming) -------------------------------------------------
    /// Target hover height (m) above the visual water plane.  Analogous to
    /// `SUSPENSION_REST_LENGTH` for land: the buoyancy system treats
    /// `water_level + WATER_REST_LENGTH` as the equilibrium altitude.
    pub const WATER_REST_LENGTH: f32 = 0.5;
    /// Upward force per metre of submersion (N/m).  Acts only when the chassis
    /// origin sits below the visual water plane.
    pub const BUOYANCY_STRENGTH: f32 = 2_500.0;
    /// Vertical drag coefficient applied while submerged (N·s/m).
    pub const BUOYANCY_DAMPING: f32 = 400.0;
    /// Maximum submersion depth (m) considered by the buoyancy force.  Prevents
    /// runaway forces if the rover clips far below the surface.
    pub const BUOYANCY_MAX_DEPTH: f32 = 1.2;
}

// ---------------------------------------------------------------------------
// Camera (camera.rs)
// ---------------------------------------------------------------------------
pub mod camera {
    /// Default orbit radius (metres from focus point).
    pub const ORBIT_RADIUS: f32 = 12.0;
    /// Default camera pitch angle (radians).
    pub const ORBIT_PITCH: f32 = 0.4;
    /// Initial camera world-space position [x, y, z].
    pub const INITIAL_POS: [f32; 3] = [0.0, 8.0, 12.0];
    /// Closest zoom (m) — keeps the camera from tunnelling inside the
    /// avatar's own visuals (#853).
    pub const ZOOM_LOWER_LIMIT: f32 = 2.0;
    /// Farthest zoom (m) — frames a whole settlement while staying well
    /// inside the fog-visibility envelope ([`fog::VISIBILITY`]), past
    /// which everything is haze anyway.
    pub const ZOOM_UPPER_LIMIT: f32 = 200.0;
    /// Lowest allowed orbit pitch (rad): slightly below horizontal so a
    /// player on a ridge can still look up at their avatar; the terrain
    /// clamp handles actual ground penetration.
    pub const PITCH_LOWER_LIMIT: f32 = -0.3;
    /// Highest allowed orbit pitch (rad): just shy of straight-down so
    /// the orbit basis never reaches the pole.
    pub const PITCH_UPPER_LIMIT: f32 = 1.54;
    /// Clearance (m) the terrain clamp keeps between the camera and the
    /// ground surface.
    pub const TERRAIN_CLEARANCE: f32 = 1.0;
    /// Sample count along the focus→camera ray for the terrain clamp —
    /// at the 200 m zoom ceiling this probes every ~12.5 m, finer than
    /// any terrain feature the 2 m-cell heightmap can express.
    pub const TERRAIN_CLAMP_SAMPLES: u32 = 16;
    /// Shortest focus→camera distance the terrain clamp may shrink to (m).
    pub const TERRAIN_CLAMP_MIN_DIST: f32 = 1.0;
    /// Freeze vehicle-yaw inheritance while `|forward.y|` exceeds this
    /// (#853): heading is undefined near-vertical, and Euler extraction
    /// there whipped the camera π mid-loop.
    pub const YAW_FREEZE_FORWARD_Y: f32 = 0.95;

    pub mod fog {
        /// sRGBA colour of the atmospheric haze (matches a mid-sky tone).
        pub const COLOR: [f32; 4] = [0.35, 0.48, 0.66, 1.0];
        /// Visibility distance (m) at which objects retain ≥ 5 % contrast.
        pub const VISIBILITY: f32 = 350.0;
        /// Atmospheric extinction colour (light lost to absorption), sRGB.
        pub const EXTINCTION_COLOR: [f32; 3] = [0.35, 0.5, 0.66];
        /// Atmospheric inscattering colour (light gained from the sun), sRGB.
        pub const INSCATTERING_COLOR: [f32; 3] = [0.8, 0.844, 1.0];
        /// sRGBA colour of sun-tinted fog (alpha controls influence strength).
        pub const DIRECTIONAL_LIGHT_COLOR: [f32; 4] = [1.0, 0.95, 0.85, 0.5];
        /// Exponent controlling how tightly the sun glow concentrates.
        pub const DIRECTIONAL_LIGHT_EXPONENT: f32 = 30.0;
    }
}

// ---------------------------------------------------------------------------
// Terrain generation (terrain/)
// ---------------------------------------------------------------------------
pub mod terrain {
    pub const SEED: u64 = 42;
    pub const GRID_SIZE: usize = 512;
    pub const CELL_SCALE: f32 = 2.0;
    pub const HEIGHT_SCALE: f32 = 50.0;
    // --- Hydraulic erosion -----------------------------------------------------
    pub mod hydraulic {
        pub const NUM_DROPS: u32 = 50_000;
        pub const MAX_STEPS: u32 = 64;
        pub const INERTIA: f32 = 0.05;
        pub const EROSION_RATE: f32 = 0.3;
        pub const DEPOSITION_RATE: f32 = 0.3;
        pub const EVAPORATION_RATE: f32 = 0.02;
        pub const CAPACITY_FACTOR: f32 = 8.0;
        pub const MIN_SLOPE: f32 = 0.01;
        pub const WATER_LEVEL: f32 = 0.0;
    }
    /// How many times the tiling textures repeat across the terrain.
    pub const TILE_SCALE: f32 = 90.0;

    // --- Voronoi terracing ---------------------------------------------------
    pub mod voronoi {
        /// Number of Voronoi seed points; more seeds → smaller plateaus.
        pub const NUM_SEEDS: usize = 1000;
        /// Number of discrete terrace height levels.
        pub const NUM_TERRACES: usize = 2;
    }

    // --- Thermal erosion -----------------------------------------------------
    pub mod thermal {
        pub const ITERATIONS: u32 = 30;
        pub const TALUS_ANGLE: f32 = 0.050;
    }

    // --- Water volume (visual) -----------------------------------------------
    pub mod water {
        /// Water plane altitude expressed as a fraction of HEIGHT_SCALE.
        pub const LEVEL_FACTOR: f32 = 0.10;

        // --- Environment-global water defaults (Room `Environment` fields) ---
        // These are the room-wide settings used as the fallback whenever a
        // record does not already carry them. Per-volume appearance lives on
        // `pds::WaterSurface` and is seeded from its own `Default` impl.

        /// Tiling frequency of the close-distance detail normal (per world-m).
        pub const DEFAULT_NORMAL_SCALE_NEAR: f32 = 0.85;
        /// Tiling frequency of the far-distance detail normal (per world-m).
        /// Much coarser than the near tile so the two scales blend without
        /// showing the per-pixel grain that produced the "repetitive at a
        /// distance" artifact on the old sum-of-sines implementation.
        pub const DEFAULT_NORMAL_SCALE_FAR: f32 = 0.08;
        /// Specular sun-glitter highlight strength.
        pub const DEFAULT_SUN_GLITTER: f32 = 1.8;
        /// sRGB tint applied to wave crests via cheap subsurface scattering.
        pub const DEFAULT_SCATTER_COLOR: [f32; 3] = [0.18, 0.45, 0.42];
        /// Default shore-foam band width (m). Defaults to 0.0 (no
        /// shoreline foam) so un-authored rooms render unchanged;
        /// raise it per water body to fade foam in where the water
        /// meets terrain (consumed via the camera's opaque depth
        /// prepass).
        pub const DEFAULT_SHORE_FOAM_WIDTH: f32 = 0.0;

        // --- Avatar-wake perturbation simulation -------------------------
        // Behaviour constants for the CPU perturbation pool
        // (`crate::interaction::perturbation`). These are engine-tuning
        // values, not authored per-volume — the per-volume amplitude /
        // wavelength / decay knobs live on `pds::WaterSurface`. Lifetimes
        // are seconds; rates are spawns-per-second.
        pub mod wake {
            /// Lifetime of a `SplashRing` spawned on water Enter/Exit. Short
            /// — an entry splash is a brief event, not a lingering swell.
            pub const SPLASH_LIFETIME: f32 = 0.9;
            /// Lifetime of a `RadialRipple` shed during slow Dwell.
            ///
            /// NOTE: with `DWELL_MIN_SPEED` (2.0) ≥
            /// `DIRECTIONAL_SPEED_THRESHOLD` (1.2), every Dwell stamp is
            /// now a `DirectionalWake` — the `RadialRipple` Dwell path
            /// (and therefore this constant) is currently unreachable.
            /// Kept because slow-wade ripples were a deliberate design
            /// casualty of the #254 raw-speed gate, not a removed
            /// feature; restoring them would re-enable this path.
            pub const RIPPLE_LIFETIME: f32 = 2.2;
            /// Lifetime of a `DirectionalWake` shed during Dwell. Long
            /// enough that the trail persists clearly behind a moving
            /// boat then fades. Bounded deliberately: at the widened
            /// `DWELL_SPACING` this keeps the live-stamp count well
            /// under the 32-slot per-plane uniform cap even at high
            /// speed (e.g. 20 m/s ÷ 2.5 m × 2.0 s ≈ 16), so the shader
            /// sums a sparse readable wake instead of a saturated pile.
            pub const WAKE_LIFETIME: f32 = 2.0;
            /// Distance (m) of avatar travel per shed Dwell
            /// perturbation. Distance-gated (not time-gated), so the
            /// spatial wake density is a fixed one-per-`DWELL_SPACING`
            /// regardless of speed or framerate.
            ///
            /// Tuned as a *vehicle-wake* knob: since Dwell only fires
            /// at raw speed ≥ `DWELL_MIN_SPEED` (2.0 m/s), this is no
            /// longer a footfall cadence. A single shader teardrop is
            /// `wake_decay_radius·(0.8 + 0.3·speed)` long — ≥ 5.6 m
            /// even at the 2 m/s gate with the default decay radius of
            /// 4.0 — so consecutive stamps at 2.5 m still overlap into
            /// a continuous wake with no dotting, while emitting ~4×
            /// fewer overlapping stamps than the old 0.6 m (which
            /// saturated the uniform cap into an unrealistic
            /// accumulation ridge, chainlink #254).
            pub const DWELL_SPACING: f32 = 2.5;
            /// Minimum speed (m/s) for Dwell to shed anything. Gated on
            /// the **raw** contact-sample velocity
            /// (avian `LinearVelocity` for the local player) — *not* a
            /// smoothed position signal. The earlier dual-EMA gate
            /// (fast vs slow position low-pass) was abandoned: a
            /// position EMA has a relaxation tail proportional to the
            /// prior speed, so after a fast straight run the smoothed
            /// position keeps drifting forward for seconds *after the
            /// boat has physically stopped*, holding the gate open and
            /// stamping a dense stack of concentric ripples right where
            /// it halts (chainlink #254, confirmed by in-engine
            /// instrumentation). Raw physics velocity has no tail and no
            /// seed-decay — it reads ~0 the instant the body stops — so
            /// `speed < DWELL_MIN_SPEED` cleanly suppresses the
            /// decelerate-to-halt burst (a settling/rocking hull also
            /// has near-zero net velocity, so it is covered too). Raise
            /// to require brisker motion before the water wakes; lower
            /// to wake on gentler movement.
            pub const DWELL_MIN_SPEED: f32 = 2.0;
            /// Max Dwell perturbations one avatar may shed in a single
            /// frame. Bounds a large-`dt` hitch; a genuine teleport is
            /// caught earlier by `DWELL_TELEPORT_DIST`.
            pub const DWELL_MAX_BURST: u32 = 4;
            /// Single-frame travel (m) above which the move is treated
            /// as a teleport (portal warp): the track resets with no
            /// emission instead of stamping a line of ripples between
            /// the old and new position.
            ///
            /// Must sit comfortably ABOVE `DWELL_MAX_BURST ·
            /// DWELL_SPACING` (4 · 2.5 = 10 m) so that a legitimate
            /// frame hitch — e.g. a 30 m/s boat through a ~0.4 s stall
            /// ≈ 12 m — produces a *capped* burst rather than being
            /// misread as a warp and silently dropping the wake. Only a
            /// genuine portal jump (well beyond a hitch) should reset.
            pub const DWELL_TELEPORT_DIST: f32 = 16.0;
            /// Speed (m/s) at or above which Dwell sheds `DirectionalWake`
            /// instead of `RadialRipple`. Below this the avatar is moving
            /// too slowly for a directional trail to read.
            pub const DIRECTIONAL_SPEED_THRESHOLD: f32 = 1.2;
            /// Global cap on live perturbations across every water plane.
            /// When exceeded the oldest are culled first. 512 covers a
            /// busy multiplayer lake; the per-plane uniform cap
            /// (`WAKE_SAMPLES_MAX`) bounds what actually reaches the GPU.
            pub const POOL_MAX: usize = 512;
            /// Waterline tolerance (m) for *entering* water contact.
            /// The classifier probes the avatar's *body bottom*
            /// (`origin.y − total_height/2`); contact begins when that
            /// point is within `CONTACT_SLACK` above the surface or
            /// anywhere below it. The slack catches a hull resting
            /// exactly at the waterline (e.g. a buoyant hover-boat that
            /// hovers a hair above the surface) without making an
            /// avatar flying well clear of the water emit ripples.
            pub const CONTACT_SLACK: f32 = 0.15;
            /// Waterline tolerance (m) for *leaving* water contact —
            /// the wide arm of a Schmitt trigger. Once an avatar is in
            /// contact it stays in contact until its body bottom rises
            /// more than `CONTACT_EXIT_SLACK` above the surface. Must
            /// exceed the settling-bob amplitude of a decelerating
            /// hull: without it, a boat coming to rest bobs across the
            /// tight enter threshold every frame, flipping
            /// Exit→Enter→Exit and spawning a burst of splash rings.
            /// The asymmetry (0.15 in, 0.6 out) is the hysteresis band
            /// that absorbs that chatter.
            pub const CONTACT_EXIT_SLACK: f32 = 0.6;
        }
    }

    // --- Avatar–terrain contact (interaction Phase 3, #245) ------------------
    /// Engine-tuning constants for the terrain side of the
    /// avatar-world interaction framework
    /// ([`crate::interaction::classifier`]). These are behaviour
    /// constants by design (mirroring `water::wake`), not authored
    /// per-room.
    pub mod ground {
        /// Grounding tolerance (m) for *entering* terrain contact. The
        /// classifier probes the avatar's body bottom
        /// (`origin.y − total_height/2`) against the heightmap surface
        /// height at that XZ; contact begins when the body bottom is
        /// within this distance above the surface (or below it). Sized
        /// to the humanoid grounded-ray pad (0.1 m at
        /// `src/player/humanoid.rs`) plus a margin for capsule rest
        /// height and heightmap bilerp error.
        pub const CONTACT_SLACK: f32 = 0.30;
        /// Grounding tolerance (m) for *leaving* terrain contact — the
        /// wide arm of a Schmitt trigger, identical in spirit to
        /// `water::wake::CONTACT_EXIT_SLACK`. Absorbs the few-cm
        /// physics jitter of a capsule resting on a heightfield so a
        /// standing avatar does not chatter Exit→Enter (which would
        /// reset footprint stamping every frame).
        pub const CONTACT_EXIT_SLACK: f32 = 0.55;
        /// Reference downward speed (m/s) that maps a terrain contact's
        /// `intensity` to 1.0. Vertical impact speed at or above this
        /// (a hard landing) is a full-strength contact; gentler motion
        /// scales linearly below it.
        pub const INTENSITY_VEL_REF: f32 = 5.0;
        /// Intensity floor while simply grounded (no vertical speed).
        /// Keeps a standing avatar registering a faint, continuous
        /// contact so footprints accrue when standing still — the
        /// "stand still → faint footprint" acceptance criterion.
        pub const INTENSITY_GROUNDED_FLOOR: f32 = 0.12;
    }

    // --- Splat stains overlay (interaction Phase 3, #245) -------------------
    /// CPU-stamped wetness / dust / footprint overlay sampled by
    /// `splat.wgsl`. The texture addresses the world toroidally: world
    /// XZ → `fract(xz / WORLD_PERIOD)`, sampled with a Repeat sampler.
    /// There is therefore no camera-recentred ring buffer and no
    /// origin-pop (the "follows camera without re-centering pop"
    /// criterion is met by construction); the trade-off is that stains
    /// repeat every `WORLD_PERIOD` metres, invisible in practice for
    /// ephemeral marks at this period.
    pub mod stains {
        /// Square stains-texture resolution (texels per side). RGBA8 on
        /// the GPU; an f32 shadow buffer of the same dimensions is kept
        /// CPU-side for slow-decay precision (a 5-minute footprint
        /// half-life would otherwise quantise to a fixed u8 and never
        /// fade).
        pub const TEXEL_DIM: usize = 256;
        /// World-space side length (m) the texture tiles over. At
        /// `TEXEL_DIM` 256 this is `WORLD_PERIOD / 256` ≈ 0.25 m per
        /// texel — fine enough for a footprint, coarse enough that the
        /// window comfortably surrounds the local avatar.
        pub const WORLD_PERIOD: f32 = 64.0;
        /// Seconds between `decay_stains` passes. Decay is computed from
        /// the real elapsed time since the last pass, so the cadence
        /// only bounds cost, not the fade curve.
        pub const DECAY_INTERVAL: f32 = 0.25;
        /// Half-life (s) of the wetness channel (R). ~4 half-lives in
        /// 30 s ⇒ a wet patch is visually gone after ~30 s.
        pub const WET_HALFLIFE: f32 = 8.0;
        /// Half-life (s) of the dust channel (G) — a brief haze that
        /// flashes the albedo lighter then clears within ~2 s.
        pub const DUST_HALFLIFE: f32 = 0.4;
        /// Half-life (s) of the footprint-indent channel (B). ~4
        /// half-lives in 300 s ⇒ a footprint decays over ~5 min.
        pub const FOOTPRINT_HALFLIFE: f32 = 70.0;
        /// Seconds after an avatar's last water contact during which
        /// terrain contacts still deposit wetness (tracked feet carry
        /// water onto land).
        pub const WET_CARRY_SECS: f32 = 6.0;
        /// Per-stamp additive footprint deposit (channel B), before the
        /// Gaussian falloff. Small so a footprint builds up over a few
        /// dwell frames rather than saturating instantly.
        pub const FOOTPRINT_DEPOSIT: f32 = 0.05;
        /// Per-stamp dust deposit (channel G) at full intensity, scaled
        /// by contact intensity. Larger than the footprint deposit so a
        /// fast pass reads as a visible (if short-lived) haze.
        pub const DUST_DEPOSIT: f32 = 0.35;
        /// Wetness deposit (channel R) per stamp while the avatar is
        /// still carrying water. Saturates the patch quickly.
        pub const WET_DEPOSIT: f32 = 0.6;
        /// Multiplier on the contact `footprint_radius` that sizes the
        /// Gaussian stamp disc in world metres.
        pub const STAMP_RADIUS_SCALE: f32 = 1.0;
    }

    // --- Splat material ------------------------------------------------------
    pub mod splat {
        /// Base colour of the terrain material before textures are uploaded.
        pub const PLACEHOLDER_COLOR: [f32; 3] = [0.35, 0.55, 0.25];
        pub const PLACEHOLDER_ROUGHNESS: f32 = 0.9;
        /// Perceptual roughness once real splat textures are applied.
        pub const MATERIAL_ROUGHNESS: f32 = 0.85;
        /// PBR metallic factor once real splat textures are applied.
        pub const MATERIAL_METALLIC: f32 = 0.0;
        /// Blend sharpness for triplanar axis transitions.
        pub const TRIPLANAR_SHARPNESS: f32 = 4.0;
    }

    // --- Splat layer: Grass (layer 0) ----------------------------------------
    pub mod grass {
        pub const SEED: u32 = 1;
        pub const MACRO_SCALE: f64 = 2.5;
        pub const MACRO_OCTAVES: usize = 4;
        pub const MICRO_SCALE: f64 = 10.0;
        pub const MICRO_OCTAVES: usize = 3;
        pub const MICRO_WEIGHT: f64 = 0.3;
        pub const COLOR_DRY: [f32; 3] = [0.07, 0.12, 0.03];
        pub const COLOR_MOIST: [f32; 3] = [0.03, 0.07, 0.01];
        pub const NORMAL_STRENGTH: f32 = 4.5;
        // Splat rule (altitude expressed as factor × HEIGHT_SCALE)
        pub const ALT_MAX_FACTOR: f32 = 0.45;
        pub const SLOPE_MAX: f32 = 0.30;
        pub const BLEND: f32 = 0.5;
    }

    // --- Splat layer: Dirt (layer 1) --------------------------------------------
    pub mod dirt {
        pub const SEED: u32 = 13;
        pub const MACRO_SCALE: f64 = 2.0;
        pub const MACRO_OCTAVES: usize = 5;
        pub const MICRO_SCALE: f64 = 8.0;
        pub const MICRO_OCTAVES: usize = 4;
        pub const MICRO_WEIGHT: f64 = 0.35;
        pub const COLOR_DRY: [f32; 3] = [0.52, 0.40, 0.26];
        pub const COLOR_MOIST: [f32; 3] = [0.28, 0.20, 0.12];
        pub const NORMAL_STRENGTH: f32 = 2.0;
        // Splat rule
        pub const ALT_MIN_FACTOR: f32 = 0.30;
        pub const ALT_MAX_FACTOR: f32 = 0.65;
        pub const SLOPE_MAX: f32 = 0.55;
        pub const BLEND: f32 = 0.5;
    }

    // --- Splat layer: Rock (layer 2) -----------------------------------------
    pub mod rock {
        pub const SEED: u32 = 7;
        pub const SCALE: f64 = 3.0;
        pub const OCTAVES: usize = 8;
        pub const ATTENUATION: f64 = 2.0;
        pub const COLOR_LIGHT: [f32; 3] = [0.37, 0.42, 0.36];
        pub const COLOR_DARK: [f32; 3] = [0.22, 0.20, 0.18];
        pub const NORMAL_STRENGTH: f32 = 4.0;
        // Splat rule
        pub const SLOPE_MIN: f32 = 0.25;
        pub const BLEND: f32 = 0.5;
    }

    // --- Splat layer: Snow (layer 3) -----------------------------------------
    pub mod snow {
        pub const SEED: u32 = 99;
        pub const MACRO_SCALE: f64 = 4.0;
        pub const MACRO_OCTAVES: usize = 3;
        pub const MICRO_SCALE: f64 = 12.0;
        pub const MICRO_OCTAVES: usize = 3;
        pub const MICRO_WEIGHT: f64 = 0.4;
        pub const COLOR_DRY: [f32; 3] = [0.95, 0.95, 0.98];
        pub const COLOR_MOIST: [f32; 3] = [0.80, 0.82, 0.88];
        pub const NORMAL_STRENGTH: f32 = 0.8;
        // Splat rule
        pub const ALT_MIN_FACTOR: f32 = 0.88;
        pub const SLOPE_MAX: f32 = 1.0;
        pub const BLEND: f32 = 4.0;
    }
}

// ---------------------------------------------------------------------------
// Procedural texture resolutions (per render class)
// ---------------------------------------------------------------------------
/// Bake resolutions for the procedural-texture pipeline, split by render
/// class so each can be tuned independently. These are the single source of
/// truth for the dimensions handed to `bevy_symbios_texture`; a future
/// revision may promote them to per-record or per-quality-tier settings, but
/// for now they are behaviour constants.
pub mod textures {
    /// Ground-splat layer resolution (pixels per side). Terrain layers are
    /// viewed up close and tile across the whole world, so they stay at the
    /// historical high resolution. This is the default for a fresh terrain
    /// record's `texture_size`; an authored record may override it.
    pub const SPLAT: u32 = 512;
    /// General surface- and card-material resolution (pixels per side) for
    /// every procedural material baked through
    /// `crate::world_builder::material::build_procedural_material` — catalogue
    /// constructs, primitives, foliage cards, avatars. Halved from the old
    /// 512 to cut bake time and memory; close-up architecture is the main
    /// place the drop is visible, and the per-class split lets that be
    /// raised again in isolation if needed.
    pub const SURFACE: u32 = 256;
    /// Per-atlas-cell resolution (pixels per side) for particle sprite
    /// sheets. A sprite emitter bakes one `variant_rows × variant_cols`
    /// atlas; multiplying this by the cell count gives the image size, so a
    /// lone glow is 128² while a 4×4 variant atlas is 512². Particles are
    /// small and short-lived on screen, so the per-cell budget is the
    /// smallest of the three classes.
    pub const PARTICLE_CELL: u32 = 128;
}

// ---------------------------------------------------------------------------
// Avatar-world interaction framework (interaction/)
// ---------------------------------------------------------------------------
/// Engine-tuning constants for the optional Phase-4 consumer channels
/// (#246 remainder). These are behaviour constants by design, not
/// authored per-room.
pub mod interaction {
    /// Projected-decal stamper (consumer channel C). Per-recipe decal
    /// appearance (ttl / size / alpha / colour / normal offset) is
    /// **PDS-authored** since #261 — see
    /// [`crate::pds::DecalParams`] (whose `Default` is the canonical
    /// seed). The only knob left here is the engine-side population
    /// cap, which is a behaviour bound, not artistic per-room data.
    pub mod decal {
        /// Hard cap on simultaneously-live decals. When exceeded the
        /// oldest are despawned first, so a long session can't grow an
        /// unbounded quad pile regardless of authored ttl.
        pub const MAX_LIVE: usize = 64;
    }

    /// Audio-cue consumer (#262). Per-cue appearance (clip / volume /
    /// pitch / spatial) is PDS-authored ([`crate::pds::AudioParams`]);
    /// the knobs here are engine-side safety/voice bounds.
    pub mod audio {
        /// Hard cap on simultaneously-playing contact cue voices. A
        /// many-avatar room or a spammy recipe can't drown the mixer /
        /// exhaust audio device voices; over the cap, new cues are
        /// dropped (never queued).
        pub const MAX_CONCURRENT_VOICES: usize = 24;
        /// Distance (m) between the spatial listener's ears, mounted on
        /// the camera. Roughly a head width — Bevy's 4 m default is far
        /// too wide and over-pans contact cues.
        pub const LISTENER_EAR_GAP: f32 = 0.3;
        /// Cap on a fetched audio clip body (bytes). Generous for a
        /// short Ogg SFX while bounding a hostile/oversized stream the
        /// same way the image cache does.
        pub const MAX_CLIP_BYTES: usize = 4 * 1024 * 1024;
        /// FIFO bound on distinct cached clips before the oldest is
        /// evicted (an attacker streaming randomised source URLs can't
        /// grow client memory without bound).
        pub const MAX_CACHE_ENTRIES: usize = 64;
    }
}

// ---------------------------------------------------------------------------
// Network (network/)
// ---------------------------------------------------------------------------
pub mod network {
    /// Broadcast identity to peers every N fixed-update ticks.
    pub const IDENTITY_BROADCAST_INTERVAL_TICKS: u32 = 60;
    /// Fallback spacing (seconds) between consecutive Transform broadcasts
    /// from a given peer, used by the jitter buffer to assign synthetic
    /// playout timestamps when WebRTC delivers packets in a burst.
    ///
    /// Broadcasts fire once per `FixedUpdate` tick, so the *true* spacing is
    /// exactly the fixed timestep. The live value is therefore read from
    /// `Time<Fixed>` at plugin build (see
    /// [`crate::network::SmootherConfigRes::from_fixed_timestep`]) rather than
    /// assumed here — that keeps the buffer's expected cadence provably equal
    /// to the real broadcast rate, so the synthetic playout clock cannot drift
    /// against wall clock and repeatedly slam the `MAX_JITTER_DRIFT_SECS`
    /// ceiling. This constant is used only as a fallback if `Time<Fixed>` is
    /// unavailable; it mirrors Bevy's default fixed timestep of 64 Hz.
    pub const EXPECTED_BROADCAST_INTERVAL_SECS: f64 = 1.0 / 64.0;
    /// Maximum amount (seconds) a jitter-buffered playout timestamp is
    /// allowed to sit ahead of wall-clock `now`.  If the sender's clock
    /// runs faster than ours, `(last + expected).max(now)` would
    /// accumulate drift forever, eventually pushing the newest sample so
    /// far into the future that `now - KINEMATIC_RENDER_DELAY_SECS`
    /// becomes older than every buffered sample — the Hermite spline
    /// then degenerates into a snap to the earliest sample and the
    /// remote mesh lags visibly.  The ceiling rebases drift to live
    /// wall-clock instead of letting it run away.
    pub const MAX_JITTER_DRIFT_SECS: f64 = 0.5;
    /// Delay (seconds) for the jitter buffer when smoothing remote peer
    /// transforms.  Rendering peers this far in the past guarantees a window
    /// of samples to interpolate between, hiding dropped packets.
    pub const KINEMATIC_RENDER_DELAY_SECS: f64 = 0.1;
    /// Maximum number of transform samples retained in each peer's buffer.
    pub const KINEMATIC_BUFFER_CAPACITY: usize = 32;
    /// Maximum absolute value of any coordinate component accepted from a
    /// remote Transform packet. `f32::MAX` is finite (so passes an
    /// `is_finite()` guard) but `f32::MAX - (-f32::MAX)` overflows to
    /// `+Inf` inside the Hermite tangent computation, which then poisons
    /// the avian3d broadphase via the local rigid body's neighbour list.
    /// 1e6 m is ~3 orders of magnitude beyond plausible play space and
    /// leaves ~32 orders of headroom before f32 arithmetic overflows.
    pub const MAX_REMOTE_COORD_ABS: f32 = 1.0e6;

    // --- Stationary bandwidth throttling ------------------------------------
    /// Linear speed (m/s) at or below which the rover is considered stationary
    /// and transform broadcasts are throttled to save bandwidth.
    pub const STATIONARY_SPEED_THRESHOLD: f32 = 0.1;
    /// Angular speed (rad/s) at or below which the rover is considered
    /// rotationally at rest.  Both linear and angular thresholds must be met
    /// before throttling kicks in, so a spinning-in-place chassis still
    /// streams smooth rotation updates at full rate.
    pub const STATIONARY_ANGULAR_THRESHOLD: f32 = 0.05;
    /// Only send a transform every N-th tick while stationary.  At the 64 Hz
    /// `FixedUpdate` tick this yields ~2 Hz (64 / 30 ≈ 2.1).
    pub const STATIONARY_BROADCAST_DIVISOR: u32 = 30;

    // --- Reliable-message chunking (#716) -----------------------------------
    // A WebRTC data-channel message has a hard whole-message ceiling of
    // 65536 bytes (64 KiB): `webrtc-sctp` rejects anything larger with
    // `ErrOutboundPacketTooLarge` *before* fragmentation, and neither
    // `matchbox_socket` 0.14 nor `bevy_symbios_multiuser` 0.6 raises,
    // negotiates, or chunks around it (native advertises no
    // `a=max-message-size`, so browser peers cap browser→native at the same
    // 64 KiB RFC-8841 default). The send is fire-and-forget — the app never
    // sees the failure — so a full `RoomStateUpdate` for a heavily-authored
    // room silently stops reaching guests. We therefore split large reliable
    // payloads into sub-ceiling chunks at the application layer and reassemble
    // them on the far side.

    /// Serialized-byte budget for one chunk's `data` field. Kept well under
    /// the 64 KiB SCTP whole-message ceiling so the bincode envelope
    /// `bevy_symbios_multiuser` wraps each `ChunkedPayload` in (enum tag +
    /// `msg_id`/`seq`/`total` + the `Vec` length prefix, ~20 bytes) never
    /// pushes the wire message over the wall. Also the direct-send threshold:
    /// a payload that serializes to `<=` this rides one message unchunked.
    pub const RELIABLE_CHUNK_DATA_BYTES: usize = 48 * 1024;

    /// Absolute ceiling on a single reliable payload's serialized size. Past
    /// this the broadcast is refused (logged + counted) rather than chunked —
    /// mirrors [`crate::pds::record_size::HARD_RECORD_CEILING_BYTES`] (a record
    /// this large cannot be published anyway) and stays under the multiuser
    /// crate's private 1 MiB bincode limit that would otherwise reject the
    /// reassembled message on decode.
    pub const MAX_RELIABLE_PAYLOAD_BYTES: usize = 900 * 1024;

    /// Total bytes the inbound chunk-reassembly buffer may hold across all
    /// in-flight partial messages before the oldest partials are evicted. A
    /// DoS bound: a peer streaming half-finished chunk sets cannot grow a
    /// guest's resident set without limit. Sized for a couple of concurrent
    /// max-size (900 KiB) reassemblies with headroom.
    pub const MAX_REASSEMBLY_BUFFER_BYTES: usize = 4 * 1024 * 1024;

    /// Maximum age (seconds) a partial reassembly is kept before it is
    /// discarded. Chunks of one message ride the ordered Reliable channel and
    /// complete in well under a second on any live link; a partial older than
    /// this means the sender vanished mid-message, so the fragments are dead
    /// weight.
    pub const MAX_REASSEMBLY_AGE_SECS: f64 = 10.0;

    /// Minimum spacing (seconds) between successive `RoomStateUpdate`
    /// broadcasts. The owner's editor rewrites `LiveRoomRecord` every frame a
    /// slider moves; without this debounce a drag would re-broadcast (and,
    /// for a large room, re-chunk) the whole record ~60×/s, flooding the
    /// ordered Reliable channel and stalling every other reliable message
    /// behind head-of-line blocking. The final drag state is always flushed,
    /// so guests still converge on the released value — just ~7 Hz instead of
    /// per-frame.
    pub const ROOM_BROADCAST_MIN_INTERVAL_SECS: f64 = 0.15;

    /// Maximum age (seconds) an `IncomingOfferDialog` is allowed to sit on
    /// screen before it is auto-declined and evicted. Without this, an
    /// ignored garbage offer would hold the busy-gate forever and lock the
    /// recipient out of receiving legitimate gifts for the rest of the
    /// session — the dialog's anti-flood property "exactly one offer at a
    /// time" turns into a denial-of-service vector when no human is watching
    /// to dismiss it. 90 s is long enough for an attentive user to read and
    /// respond; past that, declining on the user's behalf is friendlier than
    /// silently breaking gifting.
    pub const OFFER_DIALOG_TIMEOUT_SECS: f64 = 90.0;

    /// Maximum age (seconds) an entry in `PendingOutgoingOffers` is kept
    /// before it is treated as abandoned and swept. A peer that goes
    /// offline, ignores the packet, or runs a modified client that drops
    /// the response would otherwise leave the sender's entry resident
    /// forever — across a long session, that's an unbounded leak any
    /// peer can provoke. Picked well above
    /// [`OFFER_DIALOG_TIMEOUT_SECS`] so a genuine reply (declined-on-
    /// timeout from the recipient) still races its own pending entry.
    pub const PENDING_OFFER_TIMEOUT_SECS: f64 = 180.0;

    /// Maximum number of (DID → `AvatarRecord`) entries kept in
    /// `PeerAvatarCache`. The cache is only cleared on logout, so a
    /// busy hub-room — or a malicious relay cycling thousands of
    /// authenticated DIDs in and out — would otherwise grow the
    /// resident set without bound across a long session. 256 covers
    /// the vast majority of real rooms (a portal-cluster hop brings
    /// in low-double-digit peers) while bounding worst-case memory.
    pub const MAX_PEER_AVATAR_CACHE_ENTRIES: usize = 256;

    /// How often (seconds) to re-issue the relay **service-auth** token that
    /// the WebRTC signaller presents on every (re)connect. `getServiceAuth`
    /// mints a short-lived JWT (~60 s on bsky) and the app fetches it once at
    /// login; without periodic renewal, any reconnect (portal hop, dead-socket
    /// respawn, network flap) more than a token-lifetime later re-handshakes
    /// with an expired token and the relay rejects it HTTP 401 (its `exp`
    /// hardening), so peers silently fail to (re)join the room (#714). Chosen
    /// well inside the relay's own 60 s `exp` leeway so the token the signaller
    /// reads at reconnect time is always valid regardless of the exact PDS TTL.
    pub const SERVICE_TOKEN_REFRESH_SECS: f64 = 45.0;

    // --- WebRTC ICE (NAT traversal) -----------------------------------------
    /// Public STUN servers used for WebRTC ICE server-reflexive candidate
    /// discovery. Without at least one STUN server the client gathers only
    /// host (and mDNS `.local`) candidates, so two peers on different networks
    /// can complete relay signalling yet never form a peer-to-peer data
    /// channel — each then sees an empty region. STUN covers full-cone and
    /// (port-)restricted-cone NATs; symmetric NAT additionally needs a TURN
    /// relay (see [`TURN`]). Peers on the same LAN connect via host candidates
    /// regardless, which is why the missing ICE config went unnoticed in
    /// same-network testing.
    pub const STUN_URLS: &[&str] = &[
        "stun:stun.l.google.com:19302",
        "stun:stun1.l.google.com:19302",
        "stun:stun.cloudflare.com:3478",
    ];

    /// Static TURN configuration. TURN relays media through an operator-run
    /// server and therefore needs provisioned credentials, so it is unset by
    /// default: fill in a deployed TURN endpoint + long-term credentials (or
    /// later wire them from runtime config) to reach peers that STUN alone
    /// cannot — chiefly those behind symmetric NAT or UDP-blocking firewalls.
    /// An empty `url` runs STUN-only. `matchbox_socket` 0.14 exposes a single
    /// [`bevy_symbios_multiuser::prelude::RtcIceServerConfig`], so the TURN url
    /// shares the entry's credential with the STUN urls; browsers apply the
    /// credential only to `turn:`/`turns:` urls and ignore it for `stun:`, so
    /// listing both in one entry is safe.
    pub struct TurnConfig {
        pub url: &'static str,
        pub username: &'static str,
        pub credential: &'static str,
    }

    /// The active TURN relay. `url: ""` disables TURN (STUN-only).
    pub const TURN: TurnConfig = TurnConfig {
        url: "",
        username: "",
        credential: "",
    };

    /// Build the ICE server configuration for the multiuser socket
    /// ([`bevy_symbios_multiuser::prelude::SymbiosMultiuserConfig`]'s
    /// `ice_servers` field). Returns `None` only when neither STUN nor TURN is
    /// configured, which reverts to host-candidate-only (LAN-only) P2P.
    pub fn ice_servers() -> Option<bevy_symbios_multiuser::prelude::RtcIceServerConfig> {
        use bevy_symbios_multiuser::prelude::RtcIceServerConfig;
        let mut urls: Vec<String> = STUN_URLS.iter().map(|s| s.to_string()).collect();
        let mut username = None;
        let mut credential = None;
        if !TURN.url.is_empty() {
            urls.push(TURN.url.to_string());
            username = Some(TURN.username.to_string());
            credential = Some(TURN.credential.to_string());
        }
        if urls.is_empty() {
            return None;
        }
        Some(RtcIceServerConfig {
            urls,
            username,
            credential,
        })
    }
}

// ---------------------------------------------------------------------------
// App state (state.rs)
// ---------------------------------------------------------------------------
pub mod state {
    /// Maximum number of entries retained in the rolling diagnostics log.
    pub const MAX_DIAGNOSTICS_ENTRIES: usize = 200;
    /// Maximum number of generators the inventory stash retains. Mirrored
    /// in `pds::inventory::InventoryRecord::sanitize` so a hostile PDS
    /// blob cannot force the client into a multi-megabyte allocation at
    /// login, and consulted by the item-offer accept path so a peer
    /// cannot gift you over the cap.
    pub const MAX_INVENTORY_ITEMS: usize = 50;

    /// Hard DoS bound `InventoryRecord::sanitize` truncates at — NOT the
    /// gameplay cap above (#841). Sanitize used to truncate straight to
    /// [`MAX_INVENTORY_ITEMS`] in lexicographic key order, silently
    /// deleting items the user had watched get saved (the alphabet chose
    /// which). An over-cap legacy stash now survives the load — the
    /// Inventory window shows it red and blocks publishing until it's
    /// pruned — while a hostile PDS still can't force an unbounded
    /// allocation. Matches the [`MAX_INVENTORY_LIST_PAGES`] fetch ceiling
    /// (2 pages × 100 records), so nothing the fetch can return is ever
    /// truncated.
    pub const MAX_INVENTORY_SANITIZE_ITEMS: usize = 200;

    /// Maximum `com.atproto.repo.listRecords` pages (100 records each) the
    /// inventory-item fetch walks before stopping (#696). Two pages scan
    /// four times the [`MAX_INVENTORY_ITEMS`] cap — ample for any legitimate
    /// stash — while a hostile PDS handing out endless cursors cannot keep
    /// the client paging forever.
    pub const MAX_INVENTORY_LIST_PAGES: usize = 2;

    /// Maximum characters in an inventory item's display name. Items whose
    /// fetched name exceeds this are dropped by `InventoryRecord::sanitize`
    /// (deterministically, before the count cap) so a hostile PDS cannot
    /// smuggle megabyte strings through 50 item names.
    pub const MAX_INVENTORY_NAME_CHARS: usize = 256;

    /// Maximum `com.atproto.repo.listRecords` pages (100 records each) the
    /// room child-generator walk reads (#697). Four pages cover the
    /// `sanitize::limits::MAX_GENERATORS = 256` room cap with headroom,
    /// while a hostile PDS handing out endless cursors cannot keep the
    /// client paging forever.
    pub const MAX_ROOM_GENERATOR_PAGES: usize = 4;
}

// ---------------------------------------------------------------------------
// Diagnostic suite (diagnostics/) — epic #588
// ---------------------------------------------------------------------------
pub mod diagnostics {
    /// In-memory ring-buffer capacity for the session-event stream. Larger
    /// than [`super::state::MAX_DIAGNOSTICS_ENTRIES`] (the GUI tail window) so
    /// the native flush + wasm download-log button see more history than the
    /// scrolling HUD does. Bounded so the wasm heap stays flat.
    pub const RING_CAPACITY: usize = 4096;
    /// Flush the native NDJSON sink at least this often (seconds), so a hang
    /// or hard kill loses at most this much tail.
    pub const FLUSH_INTERVAL_SECS: f64 = 2.0;
    /// …or whenever this many un-flushed events have accrued, whichever first.
    pub const FLUSH_EVERY_N_EVENTS: usize = 64;
    /// Default directory (relative to the working dir) the native sink writes
    /// to. Repo-root `diagnostics/` — git-ignored and, unlike `target/`,
    /// survives `cargo clean`, so an agent's post-mortem file is not wiped by
    /// an unrelated rebuild. Overridable via [`DIR_ENV`].
    pub const DEFAULT_DIR: &str = "diagnostics";
    /// Stable filename an agent can always read for the newest run; refreshed
    /// (copied) on every flush alongside the timestamped per-session file.
    pub const LATEST_FILENAME: &str = "session-latest.jsonl";
    /// Env var overriding [`DEFAULT_DIR`] (e.g. a durable path outside the repo).
    pub const DIR_ENV: &str = "SYMBIOS_DIAG_DIR";
    /// Env var — set to `0` to disable native session-log persistence entirely
    /// (tests / CI). The in-memory ring still works.
    pub const DISABLE_ENV: &str = "SYMBIOS_DIAG";
}

// ---------------------------------------------------------------------------
// Avatar (avatar.rs)
// ---------------------------------------------------------------------------
pub mod avatar {
    /// User-Agent header sent to the ATProto API.
    pub const USER_AGENT: &str = "SymbiosOverlands/1.0";
}

// ---------------------------------------------------------------------------
// HTTP client defaults (lib.rs, avatar.rs, social.rs, ui/login/, ui/room/)
// ---------------------------------------------------------------------------
pub mod http {
    use std::time::Duration;
    /// Maximum time to wait for a TCP + TLS handshake. A tarpit peer that
    /// accepts the connection but never negotiates would otherwise hold
    /// the spawned task open forever, and on an `IoTaskPool` with a
    /// small thread budget a handful of such tasks can starve every
    /// subsequent HTTP request (avatar fetches, social resonance queries,
    /// room record reloads).
    #[cfg(not(target_arch = "wasm32"))]
    pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
    /// Whole-request wall-clock limit, including connection, TLS,
    /// request body, and response body. Any request that exceeds this
    /// returns an error the caller can log and recover from.
    pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
    /// Attempt to build a reqwest `Client` with connect + total-request
    /// timeouts and the project's User-Agent. Falls back to the default
    /// client on builder failure — reqwest's default has no timeouts, so
    /// this is a conservative hardening rather than a correctness gate.
    pub fn default_client() -> reqwest::Client {
        let builder = reqwest::Client::builder().user_agent(super::avatar::USER_AGENT);
        // Neither `timeout` nor `connect_timeout` are available on the WASM
        // reqwest client: it routes through the browser's fetch API, which
        // exposes no timeout controls on the builder. Per-request timeouts
        // must be enforced by the caller on wasm32.
        #[cfg(not(target_arch = "wasm32"))]
        let builder = builder
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT);
        builder.build().unwrap_or_default()
    }

    /// Process-wide shared Tokio runtime, lazily constructed on first
    /// use and reused for every native HTTP `block_on` call. Replaces
    /// the per-request `Builder::new_current_thread().build()…block_on`
    /// boilerplate that used to be duplicated across ~18 fetch sites
    /// — each of which paid for a fresh `mio` reactor, an epoll fd,
    /// and a timer wheel only to drop them at the end of the call.
    ///
    /// `multi_thread` (not `current_thread`) so concurrent `block_on`s
    /// from multiple `IoTaskPool` workers can drive their futures in
    /// parallel; `current_thread` would serialise them through the one
    /// driver thread.
    #[cfg(not(target_arch = "wasm32"))]
    static SHARED_RUNTIME: std::sync::LazyLock<tokio::runtime::Runtime> =
        std::sync::LazyLock::new(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("symbios-http")
                .build()
                .expect("failed to build shared Tokio runtime for HTTP block_on")
        });

    /// Run `fut` to completion on the shared HTTP Tokio runtime,
    /// blocking the calling thread until it resolves. Use from inside an
    /// `IoTaskPool::spawn(async move { … })` task on native — the pool
    /// worker thread has no Tokio reactor of its own, and reqwest's
    /// async machinery needs one. On WASM the browser's fetch event
    /// loop drives futures directly, so this helper is native-only;
    /// call sites already cfg-gate the native/WASM split.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        SHARED_RUNTIME.block_on(fut)
    }
}

// ---------------------------------------------------------------------------
// Login UI (ui/login/)
// ---------------------------------------------------------------------------
pub mod login {
    /// Default ATProto PDS endpoint.
    pub const DEFAULT_PDS: &str = "https://bsky.social";
    /// Default relay signaller hostname.
    pub const DEFAULT_RELAY_HOST: &str = "37.143.131.78.nip.io";
    pub const DEFAULT_TARGET_DID: &str = "";
    /// Where the login form's "New here?" link sends visitors without an
    /// ATProto account — Bluesky's signup lives on its app root.
    pub const SIGNUP_URL: &str = "https://bsky.app";
}

// ---------------------------------------------------------------------------
// UI panels (ui/chat.rs, ui/diagnostics.rs, ui/avatar/, ui/room/, ui/login/)
// ---------------------------------------------------------------------------
pub mod ui {
    pub mod chat {
        /// Maximum allowed length (in bytes) of a single chat message before
        /// it is truncated.  Caps peer-side rendering cost: without this an
        /// attacker could paste an 800 KiB string of junk that egui would try
        /// to word-wrap on every frame, creating an instant DoS for every
        /// guest in the room.  Well below the 1 MiB multiuser packet limit.
        pub const MAX_MESSAGE_LEN: usize = 512;
        /// Maximum chat entries retained in the rolling HUD log. A noisy (or
        /// malicious) peer could otherwise spam the channel until egui's
        /// scroll area holds megabytes of strings, re-wrapping every frame.
        pub const MAX_HISTORY_ENTRIES: usize = 500;
        // Author + mutual colours moved to the semantic theme (#856):
        // author tag = `status.info`, mutual star = `accent` (the old
        // warm gold sat in the warn-amber family — a friend must not
        // read as a caution). The `★` glyph still carries the mutual cue
        // for colour-blind viewers / greyscale captures.
    }

    // Window geometry (positions AND default sizes) lives in
    // `crate::ui::layout` since #833 — defaults are computed from the
    // screen rect there, not pixel constants here.

    pub mod diagnostics {
        /// Severity → HUD colour `[R, G, B]` — the single map the diagnostics
        /// event-log tint, the anomaly badges/pills, the per-metric dots and the
        /// toolbar worst-active dot all read (C-6), so a warning is the same
        /// amber everywhere. Trace/Info are neutral greys; Warn amber, Error
        /// orange-red, Critical red.
        pub const SEVERITY_TRACE_RGB: [u8; 3] = [96, 96, 96];
        pub const SEVERITY_INFO_RGB: [u8; 3] = [220, 220, 220];
        pub const SEVERITY_WARN_RGB: [u8; 3] = [210, 170, 90];
        pub const SEVERITY_ERROR_RGB: [u8; 3] = [210, 120, 90];
        pub const SEVERITY_CRITICAL_RGB: [u8; 3] = [220, 90, 90];
    }

    pub mod login {
        // The "Enter the Overlands" button's fill colour moved to the
        // semantic theme (#855): `ui::theme::Theme::accent_fill`. Only
        // geometry stays here.
        /// Minimum button size (px). Sized well above the default
        /// label-hugging button so it reads as the screen's primary
        /// call to action, not just another control.
        pub const ENTER_BUTTON_MIN_SIZE: [f32; 2] = [240.0, 40.0];
        /// Button label text size (px) — larger than body text to
        /// match the enlarged hit area.
        pub const ENTER_BUTTON_TEXT_SIZE: f32 = 18.0;

        /// Login form window: initial top-left position (px) and minimum
        /// content width. The `#Overlands` post feed renders in its *own*
        /// window pinned just to the right (see [`FEED_WINDOW_POS`]); the
        /// fixed width keeps the two from overlapping on the first paint,
        /// before the user has had a chance to drag either one.
        pub const WINDOW_POS: [f32; 2] = [40.0, 60.0];
        pub const WINDOW_MIN_WIDTH: f32 = 400.0;
        /// Feed window initial top-left. Sits at the login window's right
        /// edge (`WINDOW_POS.x + WINDOW_MIN_WIDTH`) plus a ~20 px gutter,
        /// with the same top so the two read as a side-by-side pair.
        pub const FEED_WINDOW_POS: [f32; 2] = [460.0, 60.0];
        /// Minimum content width of the feed window.
        pub const FEED_WINDOW_MIN_WIDTH: f32 = 360.0;
    }

    pub mod editor {
        /// Seconds of slider-idle time before the world / avatar editor
        /// flushes a pending edit into its `ResMut` change tick.
        ///
        /// Dragging an egui slider fires `changed()` every frame, which
        /// without debounce cascades into a per-frame terrain regen, room
        /// rebuild, and peer `RoomStateUpdate` / `AvatarStateUpdate`
        /// broadcast. Those rebuilds tear down in-flight foliage /
        /// splat-texture entities mid-generation, and the external
        /// `bevy_symbios_texture::poll_texture_tasks` panics when it tries
        /// to attach `TextureReady` to a despawned entity. Batching
        /// consecutive widget changes here drops the churn from ~60 Hz to
        /// ~4 Hz while staying imperceptible to the editor.
        pub const MENU_DEBOUNCE_SECS: f32 = 0.25;

        /// Seconds between refreshes of the record-size readout in the
        /// shared Save/Load/Reset row (#694). Each refresh serializes the
        /// full live record to count its bytes — cheap enough at 2 Hz even
        /// for a large room record, wasteful at 60 Hz.
        pub const SIZE_READOUT_REFRESH_SECS: f64 = 0.5;

        /// Undoable steps each editor's history ring holds beyond the
        /// baseline (#862; decision 2026-07-18). Each entry is a whole
        /// record clone — typically under the 100 KiB publish soft
        /// budget — so 32 bounds a ring at a few MiB even on wasm,
        /// where the heap never shrinks.
        pub const UNDO_DEPTH: usize = 32;
    }

    /// Transient toast notifications (`crate::ui::toast`, #819).
    pub mod toast {
        /// Seconds a toast stays visible before pruning. Matches the 6 s
        /// the Diagnostics window's hand-rolled statuses used before they
        /// migrated onto this channel.
        pub const DURATION_SECS: f64 = 6.0;

        /// Queue cap: a burst past this drops the oldest entry. Toasts
        /// are glanceable feedback, not a log — the diagnostics event
        /// log is the durable record.
        pub const MAX_VISIBLE: usize = 6;

        // The Success dot colour moved to the semantic theme (#856):
        // `ui::theme::StatusPalette::ok`, shared with every other
        // success indicator in the app.

        /// Max text width before wrapping.
        pub const MAX_WIDTH: f32 = 320.0;

        /// Anchor offset from the screen's top-right corner: clear of
        /// the window edge and below the toolbar strip.
        pub const ANCHOR_OFFSET: [f32; 2] = [-12.0, 40.0];
    }

    /// Drag-to-place drop preview (`crate::ui::inventory::drop`, #831):
    /// the ground ring that shows where an armed drag will land.
    pub mod drop_preview {
        /// Ring + post colour when the release would place here
        /// [R, G, B, A] — the blob-edit "add" green family.
        pub const VALID_COLOR: [f32; 4] = [0.15, 0.85, 0.30, 0.9];
        /// Ring colour when the ground under the cursor can't take the
        /// drop (visiting someone else's overland) — the "carve" red.
        pub const INVALID_COLOR: [f32; 4] = [0.90, 0.15, 0.15, 0.9];
        /// Footprint radius when the dragged item has no catalogue
        /// clearance metadata (inventory blueprints).
        pub const DEFAULT_RADIUS_M: f32 = 0.75;
        /// Height of the vertical marker post above the hit point.
        pub const POST_HEIGHT_M: f32 = 1.5;
    }

    /// Gizmo snap increments (`crate::editor_gizmo::GizmoFramePref`,
    /// #827): the defaults the Snap toggle starts from. Chosen for
    /// building-scale alignment work — half-metre grid, 15° angles
    /// (24 stops per turn), quarter scale steps.
    pub mod gizmo_snap {
        pub const DISTANCE_M: f32 = 0.5;
        pub const ANGLE_DEG: f32 = 15.0;
        pub const SCALE: f32 = 0.25;
    }

    /// In-scene selection highlight (`crate::editor_gizmo::highlight`,
    /// #822 / W5): wire boxes around what the gizmo will affect.
    pub mod selection_highlight {
        /// Box colour [R, G, B, A] for the selected node's subtree on the
        /// gizmo-hosting instance. Warm amber — the classic selection
        /// accent, distinct from the blob proxies' add-green/carve-red
        /// and the wireframe's cool blue-grey.
        pub const SELECTED_COLOR: [f32; 4] = [1.0, 0.82, 0.25, 0.95];

        /// Box colour for the OTHER live instances of the same blueprint
        /// node (a scattered generator edits every instance at once, so
        /// the blast radius is shown honestly — but dimly, one box per
        /// instance, so a 50-house scatter reads as context rather than
        /// noise).
        pub const SIBLING_COLOR: [f32; 4] = [1.0, 0.82, 0.25, 0.25];

        /// Floor on each box axis so flat/degenerate bounds (a card, an
        /// empty container) still draw a visible sliver.
        pub const MIN_BOX_EXTENT: f32 = 0.05;
    }

    /// In-scene BlobGroup element editing (#705): wireframe surface +
    /// gizmo-draggable per-element proxies (`crate::editor_gizmo::blob`).
    pub mod blob_edit {
        /// Additive-element proxy tint [R, G, B, A] (linear-ish sRGB floats).
        /// Green — "this element adds material". Alpha keeps the evaluated
        /// wireframe surface readable through the proxy.
        pub const PROXY_ADD_COLOR: [f32; 4] = [0.15, 0.85, 0.30, 0.28];
        /// Carve-element proxy tint. Red — "this element removes material".
        /// Slightly more opaque than [`PROXY_ADD_COLOR`]: carves sit inside
        /// the accumulated surface, so they need the extra presence to read
        /// through the wireframe shell.
        pub const PROXY_CARVE_COLOR: [f32; 4] = [0.90, 0.15, 0.15, 0.34];
        /// Alpha override applied to whichever element is selected for
        /// gizmo editing — same hue as its band, unmistakably brighter.
        pub const PROXY_SELECTED_ALPHA: f32 = 0.55;
        /// Wireframe line colour [R, G, B] of the swapped-in edge mesh. A
        /// cool pale blue-grey: visible against terrain, sky and the
        /// red/green proxies without reading as part of the model.
        pub const WIREFRAME_COLOR: [f32; 3] = [0.72, 0.82, 0.95];
        /// Minimum seconds between live re-mesh dispatches while an element
        /// drag is in progress. The SDF re-polygonization is CPU work (on
        /// WASM it shares the main thread), so the preview is throttled
        /// rather than per-frame.
        pub const PREVIEW_INTERVAL_SECS: f32 = 0.15;
        /// Grid-resolution cap for in-drag preview re-meshes. The committed
        /// mesh uses the authored resolution (≤48); the preview trades
        /// surface fidelity for a rebuild cheap enough to run mid-drag.
        pub const PREVIEW_MAX_RESOLUTION: u32 = 24;
    }
}
