//! Centralised **behaviour** constants for Symbios Overlands: the values the
//! client is compiled with, as opposed to the ones a record carries.
//!
//! Modules mirror the source file that consumes each constant group.
//!
//! What does **not** live here: anything the owner can tune. Terrain shape,
//! erosion, splat layers, water appearance and material finish are all
//! record-carried, and their tuning lives on the `Sovereign*` types'
//! `Default` impls in [`crate::pds`] with their hostile-input ceilings in
//! `pds::sanitize::limits`. Before #1157 this file also held a full
//! pre-record terrain pipeline - hydraulic/voronoi/thermal erosion and four
//! splat layers, 62 constants - that adjusted nothing at all: they had been
//! superseded by `SovereignTerrainConfig` and nothing referenced them. A
//! decoy knob in the file a contributor is told to look in first is worse
//! than no knob.
//!
//! Everything here is `pub(crate)` except [`state`], which the integration
//! tests read. That is deliberate and load-bearing: `pub` items in a `pub`
//! module are exempt from the `dead_code` lint, which is exactly why the
//! decoys survived. At crate visibility `-D warnings` reports the next one
//! the day it stops being used.

// ---------------------------------------------------------------------------
// Lighting (lib.rs)
// ---------------------------------------------------------------------------
pub(crate) mod lighting {
    /// Illuminance of the sun-like directional light (lux).
    pub const ILLUMINANCE: f32 = 15_000.0;
    /// Brightness of the scene-wide ambient light.
    pub const AMBIENT_BRIGHTNESS: f32 = 400.0;
    /// World-space position of the directional light source.
    pub const LIGHT_POS: [f32; 3] = [50.0, 40.0, 50.0];
    /// Sun colour (warm daylight, sRGB).
    pub const SUN_COLOR: [f32; 3] = [0.98, 0.95, 0.82];
    /// Cascade shadow: far bound of the first cascade (m), with the chase
    /// camera at rest ([`super::camera::ORBIT_RADIUS`]). Zoomed out, the
    /// bound moves with the camera's distance to the player - see
    /// [`crate::shadow_reach::reach`] (#1475).
    pub const CASCADE_FIRST_FAR: f32 = 15.0;
    /// Cascade shadow: maximum shadow-casting distance (m), with the chase
    /// camera at rest. Zoomed out it reaches as far past the player as it
    /// does here, up to the room's fog (#1475).
    pub const CASCADE_MAX_DIST: f32 = 200.0;
    /// Cascade shadow: the largest share of the shadow reach the first
    /// cascade may take once the zoom has pushed it out (#1475). Past half,
    /// the other three cascades are squeezed into a sliver at the far end;
    /// and uncapped, a camera zoomed out in thick fog would put the first
    /// bound past the last. Bevy's builder does not refuse that pair: it
    /// splits it backwards, falling from the first bound to the maximum, and
    /// the first cascade then shades every depth out to its own bound - past
    /// the fog - with the second sampled only in its blend band and the last
    /// two never. The cap keeps the split rising; it is not what stops a
    /// panic.
    pub const CASCADE_FIRST_SHARE_MAX: f32 = 0.5;

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
pub(crate) mod rover {
    // --- Suspension (Hooke's law + damping) ----------------------------------
    pub const SUSPENSION_REST_LENGTH: f32 = 0.8;
    pub const SUSPENSION_STIFFNESS: f32 = 4_200.0;
    pub const SUSPENSION_DAMPING: f32 = 175.0;

    // --- Drive ---------------------------------------------------------------
    pub const DRIVE_FORCE: f32 = 1_800.0;
    pub const TURN_TORQUE: f32 = 400.0;
    /// Longitudinal speed (m/s) past which - while reversing - the steer
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
    // damping moved onto `CarParams` as record tuning (#876) - their defaults
    // there are the values that used to live here.
    /// Below this squared magnitude the `up × world-up` righting axis has
    /// degenerated (the chassis is dead-inverted - a saddle it could perch on),
    /// so the assist falls back to the roll axis to tip it off its roof.
    pub const CAR_UPRIGHT_DEGENERATE_SQ: f32 = 1.0e-4;

    // --- Airplane uprighting (#1240 f162) ------------------------------------
    // The airplane was the only preset of four with no righting assist, so a
    // plane that flipped onto its back on flat ground sat there with cruise
    // thrust grinding it into the terrain and the fall respawn unable to fire.
    // Constants rather than record fields (unlike the car's, which #876
    // promoted): these are recovery behaviour, not feel to author.
    /// Tilt past which the airplane's righting assist engages, in degrees
    /// off world-up. Generous compared with the car's default, because a
    /// plane legitimately banks: below this a roll is flying, not a crash.
    pub const AIRPLANE_UPRIGHT_ENGAGE_TILT_DEGREES: f32 = 120.0;
    /// Restoring angular acceleration of the assist (rad/s², times mass).
    pub const AIRPLANE_UPRIGHT_ASSIST_ACCEL: f32 = 6.0;
    /// Spin damping, so the assist settles level instead of oscillating.
    pub const AIRPLANE_UPRIGHT_ASSIST_DAMPING: f32 = 3.0;
    /// Tilt past which the passive cruise thrust is cut, in degrees off
    /// world-up. Tighter than the engage tilt: an inverted plane under
    /// thrust is driving itself INTO the ground, and the assist should not
    /// have to fight the engine to lift it off.
    pub const AIRPLANE_THRUST_CUT_TILT_DEGREES: f32 = 100.0;
    /// Metres *below local ground* at which the rover is considered fallen
    /// through the terrain and respawned. Using a ground-relative delta
    /// rather than an absolute world-Y threshold keeps the respawn system
    /// from soft-locking on rooms whose `height_scale` sinks the entire
    /// heightmap far below the origin.
    pub const FALL_BELOW_GROUND: f32 = 20.0;
    /// Metres beyond the heightmap's half-extent at which the player is
    /// considered to have LEFT the world and is returned to spawn (#1240
    /// f169), and the width of the band over which the hover-boat's
    /// buoyancy fades out.
    ///
    /// The fall test cannot cover this: `respawn_if_fallen` samples the
    /// heightmap with the coordinates clamped INTO the extent, so past the
    /// edge the ground reference is the boundary height and an aircraft
    /// cruising above it never falls 20 m below anything. One margin for
    /// both because the boat must be recovered before its lift is gone,
    /// not after.
    pub const WORLD_EDGE_MARGIN: f32 = 100.0;

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
// Login-screen attract backdrop (attract.rs)
// ---------------------------------------------------------------------------
pub(crate) mod attract {
    /// Attract-camera orbit radius (m) - frames a whole settlement.
    /// Must stay within [`super::camera::ZOOM_UPPER_LIMIT`], which the
    /// orbit crate clamps the target radius against.
    pub const ORBIT_RADIUS: f32 = 150.0;
    /// Attract-camera pitch (rad) - a gentle aerial angle, well inside
    /// the [`super::camera`] pitch envelope.
    pub const ORBIT_PITCH: f32 = 0.5;
    /// Yaw drift rate (rad/s): one full lap every ~2.6 minutes - slow
    /// enough to read as a vista, fast enough to show it's alive.
    pub const YAW_RATE: f32 = 0.04;
    /// Focus lift above the terrain-centre height (m), so the framing
    /// centres on the built-up band rather than the ground plane.
    pub const FOCUS_LIFT: f32 = 8.0;
}

// ---------------------------------------------------------------------------
// Camera (camera.rs)
// ---------------------------------------------------------------------------
pub(crate) mod camera {
    /// Default orbit radius (metres from focus point).
    pub const ORBIT_RADIUS: f32 = 12.0;
    /// Default camera pitch angle (radians).
    pub const ORBIT_PITCH: f32 = 0.4;
    /// Initial camera world-space position [x, y, z].
    pub const INITIAL_POS: [f32; 3] = [0.0, 8.0, 12.0];
    /// Closest zoom (m) - keeps the camera from tunnelling inside the
    /// avatar's own visuals (#853).
    pub const ZOOM_LOWER_LIMIT: f32 = 2.0;
    /// Farthest zoom (m) - frames a whole settlement while staying well
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
    /// Sample count along the focus→camera ray for the terrain clamp -
    /// at the 200 m zoom ceiling this probes every ~12.5 m, finer than
    /// any terrain feature the 2 m-cell heightmap can express.
    pub const TERRAIN_CLAMP_SAMPLES: u32 = 16;
    /// Shortest focus→camera distance the terrain clamp may shrink to (m).
    pub const TERRAIN_CLAMP_MIN_DIST: f32 = 1.0;
    /// Freeze vehicle-yaw inheritance while `|forward.y|` exceeds this
    /// (#853): heading is undefined near-vertical, and Euler extraction
    /// there whipped the camera π mid-loop.
    pub const YAW_FREEZE_FORWARD_Y: f32 = 0.95;

    /// Camera distance (m, from a body's ROOT) at which a rigged body's hair
    /// swaps to the engine's far tier - `bevy_symbios_avatar::HairLod::switch`
    /// for this lens (#1358).
    ///
    /// **Not the adapter's 12 m default, and the difference is this lens.**
    /// The engine judged its far tier (symbios-avatar #350) at about 78 pixels
    /// a metre, which is what a 1080-line camera at 60° vertical shows at 12 m.
    /// Overlands' camera is Bevy's default 45° ([`super::camera`] leaves `fov`
    /// alone), so the same window resolves 1080 / (2·tan 22.5°·d) = 1304/d
    /// pixels a metre and does not fall to 78 until **16.7 m**. Measured on
    /// seed 42's head through the render tool's copy of this lens at 1920×1080:
    /// 24 px crown-to-collar at 12 m, 18 px at 16 m, 15 px at 20 m, against the
    /// 17 px the engine was judging.
    ///
    /// **The other half is where the chase camera rests.** [`ORBIT_RADIUS`] is
    /// 12 m from the chassis CENTRE and the body root hangs half a capsule
    /// (0.90 m) below it, so at rest the camera is 12.38 m from the root - past
    /// a 12 m switch. The adapter's default would put the owner's own body in
    /// its far tier at the zoom the game opens on, and flip it on any nudge.
    pub const HAIR_SWITCH: f32 = 16.7;

    /// Width of the band the hair tiers crossfade over (m). **Zero, and not a
    /// matter of taste** (#1358).
    ///
    /// Any non-zero margin switches on Bevy's dither shader, which in 0.19.1
    /// reads the visibility-range table as a 64-entry uniform on WebGL2 while
    /// the bind-group layout declares room for one entry:
    /// `pbr_opaque_mesh_pipeline` fails validation and the app quits on the
    /// first frame a band is drawn. Measured in Chromium against the published
    /// adapter with 40 bodies (bevy_symbios_avatar #48); margin 0 never
    /// compiles that shader. `a_hair_crossfade_would_quit_every_webgl2_client`
    /// in [`crate::player`] is the guard.
    pub const HAIR_MARGIN: f32 = 0.0;

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
// Ground-cover draw distance (world_builder/draw_distance.rs, #1480)
// ---------------------------------------------------------------------------
pub(crate) mod draw_distance {
    /// The player's ground-cover draw distance out of the box (m from the
    /// camera): past it, scattered and gridded copies up to
    /// [`SMALL_MAX_M`] are not drawn. A plant that size is a few pixels
    /// there, and the Understory's fog (300 m) still hides the next half of
    /// the way.
    pub const DEFAULT_M: f32 = 150.0;
    /// The Settings slider's near end (m). Below it the pop-in would be at
    /// the player's feet at the widest zoom.
    pub const MIN_M: f32 = 50.0;
    /// The slider's last distance before Unlimited (m).
    pub const MAX_M: f32 = 400.0;
    /// The slider's step and the grid every cut lands on (m). The grid is
    /// what bounds how many distinct ranges the feature can ever make - see
    /// [`WEBGL2_RANGE_SLOTS`].
    pub const STEP_M: f32 = 25.0;
    /// The slider's far end, shown as "Unlimited": a setting at or past it
    /// cuts nothing, and ground cover is drawn out to the fog as it always
    /// was. A finite number rather than infinity because the prefs file is
    /// JSON, which has no infinity.
    pub const UNLIMITED_M: f32 = MAX_M + STEP_M;
    /// A copy whose drawn box is at most this big (its largest side, m) is
    /// ground cover: cut at the setting.
    pub const SMALL_MAX_M: f32 = 2.0;
    /// A copy up to this big (m) is cut at the setting times
    /// `MEDIUM_MAX_M / SMALL_MAX_M`, where a copy of the class's largest size
    /// is as big on screen as the largest ground cover at its cut. Anything
    /// bigger - trees, rocks, buildings - is never cut.
    pub const MEDIUM_MAX_M: f32 = 4.0;
    /// Bevy 0.19.1's visibility-range table on WebGL2 (#1358): a uniform
    /// buffer of exactly this many entries (`bevy_render`'s
    /// `VISIBILITY_RANGE_UNIFORM_BUFFER_SIZE`, private), filled with every
    /// distinct `VisibilityRange` the app has EVER used - slots are never
    /// reused. On WebGL2, which has no GPU preprocessing, only the crossfade
    /// (dither) shader reads it, and a zero-margin range never compiles that
    /// shader, so overflowing it is not known to break anything; native GPU
    /// culling reads the table too (`mesh_preprocess.wgsl`), but as an
    /// unsized storage buffer. The feature still stays well inside the
    /// WebGL2 size by construction.
    pub const WEBGL2_RANGE_SLOTS: usize = 64;

    // The slider's stops are the grid: its ends and its default sit on it,
    // the default between the ends, and no cut can land at zero.
    const _: () = assert!(STEP_M > 0.0 && MIN_M >= STEP_M);
    const _: () = assert!(MIN_M < DEFAULT_M && DEFAULT_M < MAX_M);
    const _: () = assert!(MIN_M % STEP_M == 0.0 && MAX_M % STEP_M == 0.0);
    const _: () = assert!(DEFAULT_M % STEP_M == 0.0);
    const _: () = assert!(SMALL_MAX_M > 0.0 && SMALL_MAX_M < MEDIUM_MAX_M);
    // Every cut lands on the grid between one step and the medium class's
    // cut at the last distance, so that is how many distinct ranges there
    // can ever be: half the WebGL2 table at most
    // (`draw_distance::tests::ranges_stay_webgl2_safe`).
    const _: () =
        assert!((MAX_M * (MEDIUM_MAX_M / SMALL_MAX_M) / STEP_M) as usize <= WEBGL2_RANGE_SLOTS / 2);
}

// ---------------------------------------------------------------------------
// Terrain generation (terrain/)
// ---------------------------------------------------------------------------
pub(crate) mod terrain {
    pub const GRID_SIZE: usize = 512;
    pub const CELL_SCALE: f32 = 2.0;
    pub const HEIGHT_SCALE: f32 = 50.0;
    /// How many times the tiling textures repeat across the terrain.
    pub const TILE_SCALE: f32 = 90.0;

    // --- Water volume (visual) -----------------------------------------------
    pub mod water {
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
        // values, not authored per-volume - the per-volume amplitude /
        // wavelength / decay knobs live on `pds::WaterSurface`. Lifetimes
        // are seconds; rates are spawns-per-second.
        pub mod wake {
            /// Lifetime of a `SplashRing` spawned on water Enter/Exit. Short
            /// - an entry splash is a brief event, not a lingering swell.
            pub const SPLASH_LIFETIME: f32 = 0.9;
            /// Lifetime of a `RadialRipple` shed during slow Dwell.
            ///
            /// NOTE: with `DWELL_MIN_SPEED` (2.0) ≥
            /// `DIRECTIONAL_SPEED_THRESHOLD` (1.2), every Dwell stamp is
            /// now a `DirectionalWake` - the `RadialRipple` Dwell path
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
            /// `wake_decay_radius·(0.8 + 0.3·speed)` long - ≥ 5.6 m
            /// even at the 2 m/s gate with the default decay radius of
            /// 4.0 - so consecutive stamps at 2.5 m still overlap into
            /// a continuous wake with no dotting, while emitting ~4×
            /// fewer overlapping stamps than the old 0.6 m (which
            /// saturated the uniform cap into an unrealistic
            /// accumulation ridge, chainlink #254).
            pub const DWELL_SPACING: f32 = 2.5;
            /// Minimum speed (m/s) for Dwell to shed anything. Gated on
            /// the **raw** contact-sample velocity
            /// (avian `LinearVelocity` for the local player) - *not* a
            /// smoothed position signal. The earlier dual-EMA gate
            /// (fast vs slow position low-pass) was abandoned: a
            /// position EMA has a relaxation tail proportional to the
            /// prior speed, so after a fast straight run the smoothed
            /// position keeps drifting forward for seconds *after the
            /// boat has physically stopped*, holding the gate open and
            /// stamping a dense stack of concentric ripples right where
            /// it halts (chainlink #254, confirmed by in-engine
            /// instrumentation). Raw physics velocity has no tail and no
            /// seed-decay - it reads ~0 the instant the body stops - so
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
            /// frame hitch - e.g. a 30 m/s boat through a ~0.4 s stall
            /// ≈ 12 m - produces a *capped* burst rather than being
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
            /// Waterline tolerance (m) for *leaving* water contact -
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
        /// Grounding tolerance (m) for *leaving* terrain contact - the
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
        /// contact so footprints accrue when standing still - the
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
        /// texel - fine enough for a footprint, coarse enough that the
        /// window comfortably surrounds the local avatar.
        pub const WORLD_PERIOD: f32 = 64.0;
        /// Seconds between `decay_stains` passes. Decay is computed from
        /// the real elapsed time since the last pass, so the cadence
        /// only bounds cost, not the fade curve.
        pub const DECAY_INTERVAL: f32 = 0.25;
        /// Half-life (s) of the wetness channel (R). ~4 half-lives in
        /// 30 s ⇒ a wet patch is visually gone after ~30 s.
        pub const WET_HALFLIFE: f32 = 8.0;
        /// Half-life (s) of the dust channel (G) - a brief haze that
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

        /// Height above the water line (m) over which the damp-ground
        /// darkening fades out (#913, WS5 step 3).
        ///
        /// Deliberately close to the riparian scatter band that puts reeds
        /// at `[0, 3.5] m`: the darkened ground and the plants growing on
        /// it should be the same margin, or the shoreline reads as two
        /// unrelated effects that happen to be near each other.
        pub const MOISTURE_DEPTH: f32 = 4.0;
        /// How much darker fully-damp ground goes, as a fraction removed
        /// from the blended albedo at (and below) the water line, easing to
        /// zero by [`MOISTURE_DEPTH`] above it.
        ///
        /// Damp soil really is markedly darker than dry, but this is a
        /// multiply on top of the #900-tuned palette rather than a change
        /// to it - so it must stay modest enough that it reads as wet
        /// ground and never as a different biome.
        pub const MOISTURE_STRENGTH: f32 = 0.28;

        /// View distance (m) where the splat albedo starts cross-fading to
        /// each layer's mean colour (#1320).
        ///
        /// A texture tile covers 11.36 m ([`super::TILE_SCALE`] across the
        /// 1022 m world), so a few hundred metres out it spans only a few
        /// tens of pixels (20-30 px in `render --terrain`'s 640-px tiles at
        /// 300-600 m). By then the mips have correctly filtered away the
        /// per-texel crumb that masks the repeat, and what is left, the
        /// tile's own low-frequency residue, recurs exactly once per tile as
        /// a regular cross-hatch. #1320 bisected it to the albedo; the normal
        /// maps and the mesh do not carry it. Fading to the mean removes the
        /// repeat, together with detail nobody can resolve at that range.
        /// Ground nearer than 100 m is untouched (4 % faded at 125 m, 16 % at
        /// 150 m).
        pub const ALBEDO_FADE_NEAR: f32 = 100.0;
        /// View distance (m) where the albedo is fully each layer's mean
        /// colour. It must sit inside the fog's
        /// [`crate::config::camera::fog::VISIBILITY`] (asserted at the foot
        /// of this file): the fade has to finish while the fog still shows
        /// the ground, or the far ground left in view keeps the repeat.
        pub const ALBEDO_FADE_FAR: f32 = 300.0;
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
pub(crate) mod textures {
    /// Ground-splat layer resolution (pixels per side). Terrain layers are
    /// viewed up close and tile across the whole world, so they stay at the
    /// historical high resolution. This is the default for a fresh terrain
    /// record's `texture_size`; an authored record may override it.
    pub const SPLAT: u32 = 512;
    /// General surface- and card-material resolution (pixels per side) for
    /// every procedural material baked through
    /// `crate::world_builder::material::build_procedural_material` - catalogue
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
// Vegetation wind sway (wind.rs)
// ---------------------------------------------------------------------------
/// Tuning for the foliage wind-sway vertex shader (#916).
///
/// The motion is authored per *profile* - foliage hanging off an L-system
/// plant sways differently from a hand-sized ground-cover card - and the two
/// profiles differ only in these numbers; the shader itself has no branch.
/// Direction and speed are not here: those come from the room's
/// `Environment::cloud_wind_dir` / `cloud_speed`, so one wind drives the
/// clouds and the foliage together.
pub(crate) mod vegetation_wind {
    /// L-system foliage. `AMPLITUDE` is metres of lean at full weight, and
    /// weight reaches 1 at `REFERENCE_HEIGHT` metres above the plant's base -
    /// so a leaf in a 4 m canopy travels about a hand's width, and one on a
    /// waist-high shrub barely moves.
    pub mod branch {
        pub const AMPLITUDE: f32 = 0.16;
        pub const REFERENCE_HEIGHT: f32 = 4.0;
        /// Cross-wind flutter as a fraction of `AMPLITUDE`. Lower than the
        /// card profile's: a leaf cluster on a branch is held at one end,
        /// so it leans more than it shivers.
        pub const FLUTTER: f32 = 0.35;
    }

    /// Ground-cover cards (grass tufts, ferns, reeds, wildflowers).
    ///
    /// A card's entity origin sits at its *centre* rather than its base, so
    /// the height weight is biased by half: `HALF_HEIGHT` metres below the
    /// origin weighs 0 and the same distance above weighs 1. That is what
    /// makes the base near-static and the tip mobile without the shader
    /// needing to know how tall any particular card is.
    pub mod card {
        pub const AMPLITUDE: f32 = 0.035;
        pub const HALF_HEIGHT: f32 = 0.5;
        /// Higher than the branch profile: loose blades shimmer across the
        /// wind at least as much as they lean along it.
        pub const FLUTTER: f32 = 0.55;
    }
}

// ---------------------------------------------------------------------------
// Avatar-world interaction framework (interaction/)
// ---------------------------------------------------------------------------
/// Engine-tuning constants for the optional Phase-4 consumer channels
/// (#246 remainder). These are behaviour constants by design, not
/// authored per-room.
pub(crate) mod interaction {
    /// Minimum seconds between two stamps of the same contact recipe on the
    /// same avatar while the viewer has effects set to Reduced (#1221 f308).
    ///
    /// The sanitiser permits a `cooldown` of 0, and `stamp_decals` only
    /// consults the cooldown when it is `> 0.0` - so a Dwell recipe with
    /// zero stamps once per frame per avatar, which is what turns a
    /// permitted 64 m quad into a wall. A one-second floor keeps an
    /// authored effect legible and takes the per-frame case away.
    pub const REDUCED_EFFECT_COOLDOWN_SECS: f32 = 1.0;

    /// Projected-decal stamper (consumer channel C). Per-recipe decal
    /// appearance (ttl / size / alpha / colour / normal offset) is
    /// **PDS-authored** since #261 - see
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
        /// the camera. Roughly a head width - Bevy's 4 m default is far
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

        /// The rate this world bakes and plays a procedural bed at
        /// (#568, #1337 C7).
        ///
        /// 22.05 kHz halves the baked WAV and the decoded playback buffer
        /// against 44.1, and the pad and drone content a bed is made of
        /// sits well inside the 11 kHz Nyquist, so the memory win is
        /// inaudible. On wasm it is worth double: freed linear memory
        /// never returns to the OS, so every re-bake's high-water mark is
        /// permanent.
        ///
        /// Named here because three places have to agree about it and
        /// used to do so by coincidence: the seeded ambient bed
        /// (`seeded_defaults::room::audio`), the recipe a NEW Sequence
        /// slot is born as (`ui::room::audio::new_sequence_recipe`), and
        /// the rates that editor's picker offers. The second of the three
        /// was 44 100, so every hand-authored bed cost twice the generated
        /// one for no audible gain.
        ///
        /// It is deliberately NOT read by
        /// `pds::audio::SovereignSequenceRecipe::default`: that is a
        /// mirror of upstream's own default and is held to it field for
        /// field, so that a value drifting upstream is caught rather than
        /// quietly re-meaning. A world's taste is not a schema default.
        pub const WORLD_BED_SAMPLE_RATE: u32 = 22_050;
    }
}

// ---------------------------------------------------------------------------
// Network (network/)
// ---------------------------------------------------------------------------
pub(crate) mod network {
    /// Broadcast identity to peers every N fixed-update ticks.
    pub const IDENTITY_BROADCAST_INTERVAL_TICKS: u32 = 60;
    /// How long (seconds) after a peer connects we keep waiting for its
    /// [`crate::protocol::OverlandsMessage::Hello`] before reading the silence
    /// as "this build predates the protocol handshake" (#1121).
    ///
    /// `Hello` rides the same reliable broadcast as `Identity`, once every
    /// [`IDENTITY_BROADCAST_INTERVAL_TICKS`] ticks - one second at the 60 Hz
    /// fixed step - plus one immediately on connect. Three seconds is
    /// therefore three chances missed, not one, so a slow handshake or a
    /// single dropped announce does not accuse a compatible peer. The cost of
    /// being wrong is one wrong chip on one row, and it corrects itself the
    /// moment a `Hello` lands.
    pub const PROTOCOL_ANNOUNCE_GRACE_SECS: f64 = 3.0;
    /// Fallback spacing (seconds) between consecutive Transform broadcasts
    /// from a given peer, used by the jitter buffer to assign synthetic
    /// playout timestamps when WebRTC delivers packets in a burst.
    ///
    /// Broadcasts fire once per `FixedUpdate` tick, so the *true* spacing is
    /// exactly the fixed timestep. The live value is therefore read from
    /// `Time<Fixed>` at plugin build (see
    /// [`crate::network::SmootherConfigRes::from_fixed_timestep`]) rather than
    /// assumed here - that keeps the buffer's expected cadence provably equal
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
    /// becomes older than every buffered sample - the Hermite spline
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
    // 64 KiB RFC-8841 default). The send is fire-and-forget - the app never
    // sees the failure - so a full `RoomStateUpdate` for a heavily-authored
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
    /// this the broadcast is refused (logged + counted) rather than chunked -
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

    /// Bookkeeping cost charged against
    /// [`MAX_REASSEMBLY_BUFFER_BYTES`] for one in-flight reassembly, on top
    /// of the payload bytes it holds (#1114).
    ///
    /// The budget used to count payload only, which a flooding peer walked
    /// straight past: a one-byte fragment with a fresh `msg_id` allocates a
    /// slot vector for the whole declared message plus a `Partial` and a
    /// HashMap entry - some hundreds of bytes - and charged one byte for it.
    /// The cap could therefore never be reached, and partials grew until the
    /// ten-second age sweep happened to catch them.
    ///
    /// `PER_SLOT` is `size_of::<Option<Vec<u8>>>()` (24 on 64-bit); `BASE`
    /// covers the `Partial` struct, the map entry and allocator overhead.
    /// Both are deliberately approximate: the point is that opening a
    /// reassembly costs something, not that the figure is exact.
    pub const REASSEMBLY_SLOT_OVERHEAD_BYTES: usize = 24;
    /// See [`REASSEMBLY_SLOT_OVERHEAD_BYTES`].
    pub const REASSEMBLY_PARTIAL_OVERHEAD_BYTES: usize = 128;

    /// Most in-flight reassemblies one peer may hold open (#1114).
    ///
    /// A byte budget alone cannot bound the *count*, and the count is what
    /// the per-fragment sweep costs are proportional to. The sender chunks
    /// one message at a time, so a handful covers every legitimate case
    /// (an interleaved directed room push plus a broadcast, say); past it
    /// the peer's own oldest partial is dropped to make room. Per-sender,
    /// so a flooding peer can only ever evict its own work.
    pub const MAX_REASSEMBLIES_PER_PEER: usize = 8;

    /// Most in-flight reassemblies across all peers. With the per-peer cap
    /// above this is a second-order guard for a room full of peers all
    /// chunking at once.
    pub const MAX_REASSEMBLIES_TOTAL: usize = 64;

    /// How often the stale-partial sweep may run, in seconds (#1114).
    ///
    /// The sweep is O(partials) and used to run on *every* fragment, so a
    /// flooding peer made the per-frame drain quadratic and pinned the main
    /// thread. Partials are only ever dropped for being older than
    /// [`MAX_REASSEMBLY_AGE_SECS`], so running it on a timer instead costs
    /// at most one extra sweep interval of lifetime for a dead partial.
    pub const REASSEMBLY_SWEEP_INTERVAL_SECS: f64 = 1.0;

    /// How long a failed rigged-body resolution is left alone before the
    /// peer's references are tried again, and how far that wait doubles
    /// (#1113).
    ///
    /// A reference that cannot resolve - a wardrobe rkey minted by "wear a
    /// fresh body" but not yet published, a deleted record, a PDS that is
    /// down - used to be retried by every client in the room on every
    /// frame, because nothing recorded the failure: one peer with a
    /// dangling pointer turned every guest into a continuous load generator
    /// against `plc.directory` and a stranger's PDS. The backoff is per
    /// reference *set*, so the moment the peer edits what they wear (or
    /// finally publishes it) the wait is discarded and the new references
    /// resolve immediately.
    pub const RIG_RESOLVE_RETRY_BASE_SECS: f64 = 2.0;

    /// Minimum seconds between two rigged-body resolutions for the SAME
    /// peer, whatever they are wearing (#1126).
    ///
    /// The failure backoff above is keyed by reference set, which is right
    /// for a reference that cannot resolve and useless against a peer whose
    /// references keep *succeeding*: `resolved` is `#[serde(skip)]`, so
    /// every inbound `AvatarStateUpdate` arrives unresolved, and a peer
    /// alternating between two valid outfits presents a changed set every
    /// time. Each round trip then costs every guest in the room a DID
    /// document plus a wardrobe record plus up to sixteen attachments, to
    /// hosts of the peer's choosing, on the shared `IoTaskPool` - the same
    /// pool room loads, gifts and profile fetches queue on.
    ///
    /// So this floor is unconditional rather than "unless the set changed":
    /// a condition on the set is exactly what the alternating case defeats.
    /// Five seconds is invisible to the legitimate case - a wearer's own
    /// edits are already debounced and only need to arrive eventually -
    /// and turns an unbounded amplifier into a bounded trickle.
    pub const RIG_RESOLVE_MIN_INTERVAL_SECS: f64 = 5.0;
    /// Ceiling for the doubling in [`RIG_RESOLVE_RETRY_BASE_SECS`]. A minute
    /// is long enough that a room full of unresolvable peers costs nothing,
    /// and short enough that a PDS coming back up is picked up while the
    /// visitor is still there.
    pub const RIG_RESOLVE_RETRY_MAX_SECS: f64 = 60.0;

    /// First wait (seconds) before a failed per-peer fetch - the avatar
    /// record, the bsky profile, the relationship query - is tried again,
    /// and how far that wait doubles (#1217/#1218).
    ///
    /// Each of those fetches used to be one-shot: spawned from a single
    /// site on the peer's DID resolving, with no failure state and no
    /// retry. One HTTP hiccup at join time therefore lasted the whole
    /// session - a DID-seeded stranger standing in for the avatar someone
    /// actually published, or a permanent "identifying…" on a peer who was
    /// talking to you. Doubling from two seconds keeps a transient blip
    /// invisible while an outage settles into a slow poll instead of a load
    /// generator aimed at a stranger's PDS.
    ///
    /// The arithmetic is shared with [`RIG_RESOLVE_RETRY_BASE_SECS`] above
    /// through `network::presence::next_wait_secs`; these two carry their
    /// own numbers because the wardrobe fan-out is N+2 records per attempt
    /// and these are one.
    pub const PEER_FETCH_RETRY_BASE_SECS: f64 = 2.0;
    /// Ceiling for the doubling in [`PEER_FETCH_RETRY_BASE_SECS`]. Matches
    /// [`RIG_RESOLVE_RETRY_MAX_SECS`]: long enough that a room full of
    /// unreachable peers costs nothing, short enough that a PDS coming back
    /// up is picked up while the visitor is still in the room.
    pub const PEER_FETCH_RETRY_MAX_SECS: f64 = 60.0;

    /// The public web profile for a DID (#1223 f291).
    ///
    /// The app has no report path of its own and its mute is machine-local,
    /// so the escalation a user actually has is the one Bluesky provides -
    /// block and report at the ATProto layer, where they mean something
    /// beyond this client. `bsky.app` resolves a DID in the actor position
    /// exactly as it resolves a handle.
    pub fn bsky_profile_url(did: &str) -> String {
        format!("https://bsky.app/profile/{did}")
    }

    /// Seconds of silence from a peer before the roster says they are not
    /// responding (#1224 f335).
    ///
    /// A peer entity was despawned by exactly ONE path - the transport's
    /// `Disconnected` event - and the client had no liveness check of its
    /// own. So whenever the transport failed to report a drop (a wedged
    /// data channel, a suspended browser tab, a relay that loses a peer
    /// without closing the channel) a body stood frozen indefinitely,
    /// counted in the roster and offered as a gift and Visit target that
    /// would never answer. A ghost is worse than an absence: it makes the
    /// room look occupied.
    ///
    /// Generous against the true cadence: a parked peer still broadcasts
    /// every [`STATIONARY_BROADCAST_DIVISOR`]th 64 Hz tick, about twice a
    /// second, so fifteen seconds is roughly thirty missed packets.
    pub const PEER_QUIET_SECS: f64 = 15.0;

    /// Seconds of silence before the peer is despawned outright, with the
    /// same presence line the disconnect path writes (#1224 f335).
    ///
    /// Long, because the cost of being wrong is asymmetric: a peer wrongly
    /// swept reappears on their next sign of life - the sweep remembers
    /// them (#1429; before it did, a swept peer whose tab woke stayed
    /// bodiless until their connection cycled) - but a body removed from
    /// under a conversation is jarring. Two minutes is well past any
    /// transport hiccup and well short of "this room has been lying to me
    /// all session".
    pub const PEER_GHOST_SECS: f64 = 120.0;

    /// How long a swept peer is remembered, so their next transform,
    /// identity or hello brings them back (#1429). Their connection
    /// usually still stands - a browser tab asleep in the background - and
    /// the transport says nothing when it wakes. A connection the transport
    /// does close is forgotten at once; this bounds one it never does.
    pub const PEER_SWEPT_MEMORY_SECS: f64 = 30.0 * 60.0;

    /// Radius (metres) of the translucent stand-in body drawn for a
    /// chassis whose real one has not arrived yet (#1217 f328).
    ///
    /// Shared by the peer stand-in and the owner's own (#1255): two
    /// differently-shaped placeholders would read as two different kinds
    /// of absence, and there is only one.
    pub const BODY_PLACEHOLDER_RADIUS: f32 = 0.32;
    /// Length (metres) of the stand-in capsule's cylindrical section; the
    /// total height is this plus twice [`BODY_PLACEHOLDER_RADIUS`], so
    /// roughly a person. A peer chassis broadcasts its transform from the
    /// body's centre, and the capsule is centred on its own origin, so the
    /// stand-in stands where the peer stands.
    pub const BODY_PLACEHOLDER_LENGTH: f32 = 1.06;
    /// Colour of the stand-in. Deliberately a translucent neutral and not
    /// anything a body could be: the point is that it reads as a placeholder
    /// rather than as a person who chose to look like this.
    pub const BODY_PLACEHOLDER_COLOR: bevy::prelude::Color =
        bevy::prelude::Color::srgba(0.62, 0.68, 0.78, 0.35);

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
    /// so guests still converge on the released value - just ~7 Hz instead of
    /// per-frame.
    pub const ROOM_BROADCAST_MIN_INTERVAL_SECS: f64 = 0.15;

    /// Maximum age (seconds) an `IncomingOfferDialog` is allowed to sit on
    /// screen before it is auto-declined and evicted. Without this, an
    /// ignored garbage offer would hold the busy-gate forever and lock the
    /// recipient out of receiving legitimate gifts for the rest of the
    /// session - the dialog's anti-flood property "exactly one offer at a
    /// time" turns into a denial-of-service vector when no human is watching
    /// to dismiss it. 90 s is long enough for an attentive user to read and
    /// respond; past that, declining on the user's behalf is friendlier than
    /// silently breaking gifting.
    pub const OFFER_DIALOG_TIMEOUT_SECS: f64 = 90.0;

    /// Maximum age (seconds) an entry in `PendingOutgoingOffers` is kept
    /// before it is treated as abandoned and swept. A peer that goes
    /// offline, ignores the packet, or runs a modified client that drops
    /// the response would otherwise leave the sender's entry resident
    /// forever - across a long session, that's an unbounded leak any
    /// peer can provoke. Picked well above
    /// [`OFFER_DIALOG_TIMEOUT_SECS`] so a genuine reply (declined-on-
    /// timeout from the recipient) still races its own pending entry.
    pub const PENDING_OFFER_TIMEOUT_SECS: f64 = 180.0;

    /// Maximum number of (DID → `AvatarRecord`) entries kept in
    /// `PeerAvatarCache`. The cache is only cleared on logout, so a
    /// busy hub-room - or a malicious relay cycling thousands of
    /// authenticated DIDs in and out - would otherwise grow the
    /// resident set without bound across a long session. 256 covers
    /// the vast majority of real rooms (a portal-cluster hop brings
    /// in low-double-digit peers) while bounding worst-case memory.
    pub const MAX_PEER_AVATAR_CACHE_ENTRIES: usize = 256;

    /// Maximum number of (DID → cached bsky profile picture) entries kept
    /// in `BskyProfileCache` (#1125).
    ///
    /// The unbounded sibling of [`MAX_PEER_AVATAR_CACHE_ENTRIES`], and the
    /// more expensive one: each entry holds a decoded `Image` rather than a
    /// record. It is populated by every peer Identity and cleared only on
    /// logout, so a relay or peer set churning DIDs grew a guest's heap for
    /// the length of the session - and wasm never returns heap to the OS.
    /// Same bound as its sibling for the same reason: a portal-cluster hop
    /// brings in low-double-digit peers.
    pub const MAX_BSKY_PROFILE_CACHE_ENTRIES: usize = 256;

    /// Edge length the cached profile picture is downscaled to before it
    /// reaches `Assets<Image>` (#1125).
    ///
    /// Every consumer draws it through `draw_avatar_icon` at
    /// `AVATAR_ICON_PX` (18 logical px), so 64 covers 3.5× device pixel
    /// ratio with room to spare. What it replaces is what the source
    /// actually is: the bsky CDN resizes, but the wasm path fetches from
    /// the DID's own PDS, where the blob is whatever the owner uploaded -
    /// up to the 4096 px decode cap, which is 64 MiB of RGBA for ONE
    /// entry. Storing the icon at icon size makes each entry 16 KiB, so
    /// the bound above is a few megabytes rather than gigabytes.
    pub const BSKY_PROFILE_ICON_PX: u32 = 64;

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

    /// Consecutive relay service-auth refresh failures after which the client
    /// stops treating the outage as a hiccup and says so (#1215).
    ///
    /// At [`SERVICE_TOKEN_REFRESH_SECS`] apart, three in a row is a bit over
    /// two minutes of a credential that cannot be renewed - comfortably past
    /// any single transient PDS error, and still inside the window where the
    /// user's own remedy (sign in again) is worth offering before they have
    /// spent an hour building in a world they can no longer rejoin.
    pub const SERVICE_TOKEN_FAILURES_BEFORE_ALARM: u64 = 3;

    // --- WebRTC ICE (NAT traversal) -----------------------------------------
    /// Public STUN servers used for WebRTC ICE server-reflexive candidate
    /// discovery. Without at least one STUN server the client gathers only
    /// host (and mDNS `.local`) candidates, so two peers on different networks
    /// can complete relay signalling yet never form a peer-to-peer data
    /// channel - each then sees an empty region. STUN covers full-cone and
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
    /// cannot - chiefly those behind symmetric NAT or UDP-blocking firewalls.
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
    ///
    /// **500 since #1292; it was 50, chosen before #696 made the stash one
    /// record PER ITEM.** Under the pre-#696 monolith the whole stash was a
    /// single record against a 100 KiB soft budget, and 50 was a reasonable
    /// guess at what fit. That constraint no longer exists: each item is its
    /// own record with its own budget, and the publish path chunks by both
    /// write count and request bytes (`xrpc::chunk_writes`), so nothing on
    /// the wire cares how many items there are.
    ///
    /// What DOES bound it, measured across all 392 catalogue entries
    /// (median item 7.2 KiB, p90 23.8 KiB):
    ///
    /// * **The fetch walk.** `listRecords` returns 100 per page, so the
    ///   ceiling is [`MAX_INVENTORY_LIST_PAGES`] × 100 and a stash past it
    ///   would load SILENTLY TRUNCATED. The two `const` assertions at the
    ///   foot of this file keep that impossible.
    /// * **The Inventory panel's dirty check**, which was the real
    ///   ceiling and is not a wire limit at all. It built a whole-stash
    ///   `serde_json::Value` every frame the panel was open: 3.0 ms at 50
    ///   items, 17.5 ms at 200 - past the entire 60 fps budget - and 33.9 ms
    ///   at 500. #1292 caches it on the resource's change tick, so an idle
    ///   panel now pays nothing and only an edit re-serializes.
    ///
    /// What is left at 500 is ~3.3 ms/frame of unvirtualized row layout
    /// (6.6 µs per row, measured), which the search field added in #1275
    /// f134 usually cuts to a handful of rows. Virtualizing the list is the
    /// next lever if the cap ever rises again.
    pub const MAX_INVENTORY_ITEMS: usize = 500;

    /// Hard DoS bound `InventoryRecord::sanitize` truncates at - NOT the
    /// gameplay cap above (#841). Sanitize used to truncate straight to
    /// [`MAX_INVENTORY_ITEMS`] in lexicographic key order, silently
    /// deleting items the user had watched get saved (the alphabet chose
    /// which). An over-cap legacy stash now survives the load - the
    /// Inventory window shows it red and blocks publishing until it's
    /// pruned - while a hostile PDS still can't force an unbounded
    /// allocation. Matches the [`MAX_INVENTORY_LIST_PAGES`] fetch ceiling
    /// (6 pages × 100 records), so nothing the fetch can return is ever
    /// truncated.
    pub const MAX_INVENTORY_SANITIZE_ITEMS: usize = 600;

    /// Maximum `com.atproto.repo.listRecords` pages (100 records each) the
    /// inventory-item fetch walks before stopping (#696, raised for #1292).
    ///
    /// Six pages cover the [`MAX_INVENTORY_ITEMS`] cap with one page of
    /// headroom - enough that an over-cap stash LOADS INTACT and can be
    /// pruned (#841's rule: never silently delete what the owner watched
    /// get saved) - while a hostile PDS handing out endless cursors cannot
    /// keep the client paging forever.
    ///
    /// **Page count is no longer the memory bound.** It used to be: each
    /// page is capped at `xrpc::MAX_FETCH_BODY_BYTES` (16 MiB) on its own,
    /// so raising 2 → 6 would have tripled what a malicious PDS could make
    /// the client hold, from 32 MiB to 96 MiB - and on wasm the heap never
    /// shrinks, so that is permanent for the session. The walk carries one
    /// [`MAX_INVENTORY_FETCH_BYTES`] budget across all of its pages
    /// instead, which is *tighter* than the ceiling the two-page walk had.
    pub const MAX_INVENTORY_LIST_PAGES: usize = 6;

    /// Total decoded bytes the inventory fetch walk may consume across ALL
    /// its pages (#1292).
    ///
    /// One budget for the whole walk, so [`MAX_INVENTORY_LIST_PAGES`] can
    /// grow with the item cap without the hostile-input ceiling growing
    /// with it.
    ///
    /// 24 MiB against a measured 4.2 MiB for 600 median items and 14 MiB
    /// for 600 at the catalogue's p90 - headroom for a stash of unusually
    /// heavy items, and still below the 32 MiB the old two-page walk
    /// allowed.
    pub const MAX_INVENTORY_FETCH_BYTES: usize = 24 * 1024 * 1024;

    /// Maximum characters in an inventory item's display name. Items whose
    /// fetched name exceeds this are cut to it by `InventoryRecord::sanitize`
    /// (deterministically, before the count cap) so a hostile PDS cannot
    /// smuggle megabyte strings through the stash's item names. Cut, not dropped
    /// (#1205): the rename dialog enforces the same bound, and an item the
    /// owner watched save must not vanish at the next login.
    pub const MAX_INVENTORY_NAME_CHARS: usize = 256;

    /// Maximum `com.atproto.repo.listRecords` pages (100 records each) the
    /// room child-generator walk reads (#697). Four pages cover the
    /// `sanitize::limits::MAX_GENERATORS = 256` room cap with headroom,
    /// while a hostile PDS handing out endless cursors cannot keep the
    /// client paging forever.
    pub const MAX_ROOM_GENERATOR_PAGES: usize = 4;
}

// ---------------------------------------------------------------------------
// Diagnostic suite (diagnostics/) - epic #588
// ---------------------------------------------------------------------------
pub(crate) mod diagnostics {
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
    /// to. Repo-root `diagnostics/` - git-ignored and, unlike `target/`,
    /// survives `cargo clean`, so an agent's post-mortem file is not wiped by
    /// an unrelated rebuild. Overridable via [`DIR_ENV`].
    /// Native only: the wasm build has no filesystem, so its sink is the
    /// in-memory ring alone.
    #[cfg(not(target_arch = "wasm32"))]
    pub const DEFAULT_DIR: &str = "diagnostics";
    /// Stable filename an agent can always read for the newest run; refreshed
    /// (copied) on every flush alongside the timestamped per-session file.
    /// Native only: the wasm build has no filesystem, so its sink is the
    /// in-memory ring alone.
    #[cfg(not(target_arch = "wasm32"))]
    pub const LATEST_FILENAME: &str = "session-latest.jsonl";
    /// Env var overriding [`DEFAULT_DIR`] (e.g. a durable path outside the repo).
    /// Native only: the wasm build has no filesystem, so its sink is the
    /// in-memory ring alone.
    #[cfg(not(target_arch = "wasm32"))]
    pub const DIR_ENV: &str = "SYMBIOS_DIAG_DIR";
    /// Env var - set to `0` to disable native session-log persistence entirely
    /// (tests / CI). The in-memory ring still works.
    /// Native only: the wasm build has no filesystem, so its sink is the
    /// in-memory ring alone.
    #[cfg(not(target_arch = "wasm32"))]
    pub const DISABLE_ENV: &str = "SYMBIOS_DIAG";

    /// A frame longer than this is a hitch worth recording individually
    /// (#1144). Six 60 Hz frames: long enough that ordinary scheduling jitter
    /// and a heavy-but-normal frame stay out of the histogram, short enough to
    /// catch the sub-second stalls that actually matter now - a rigged-body
    /// install, a world-compile slice, a texture upload, an egui panel rebuild.
    pub const FRAME_HITCH_MS: f64 = 100.0;

    /// The worst frame in a scrape window past which `runtime.frame_hitch`
    /// fires. Well above [`FRAME_HITCH_MS`]: one long frame is worth counting,
    /// a quarter-second freeze is worth telling the user about.
    pub const FRAME_HITCH_ALERT_MS: f64 = 250.0;
}

// ---------------------------------------------------------------------------
// Avatar (avatar.rs)
// ---------------------------------------------------------------------------
pub(crate) mod avatar {
    /// User-Agent header sent to the ATProto API.
    pub const USER_AGENT: &str = "SymbiosOverlands/1.0";
}

// ---------------------------------------------------------------------------
// HTTP client defaults (lib.rs, avatar.rs, social.rs, ui/login/, ui/room/)
// ---------------------------------------------------------------------------
/// Retry policy for room ASSET fetches - sign images, referenced textures,
/// terrain splat layers, ambient beds and contact audio cues (#1247).
///
/// Separate constants from [`network::PEER_FETCH_RETRY_BASE_SECS`] even
/// though the numbers currently match, because what each protects is
/// different. A peer fetch is aimed at that peer's own PDS and is the price
/// of them being in the room; an asset fetch is aimed at a third-party host
/// named in somebody else's record, from every visitor's client at once, and
/// the person causing the traffic is not the person who authored it. If the
/// two ever have to diverge, this is the seam.
pub(crate) mod asset {
    /// First wait after a failed asset fetch. Short enough that a blip
    /// during a room load is invisible.
    pub const FETCH_RETRY_BASE_SECS: f64 = 2.0;
    /// Ceiling for the doubling. A dead source therefore settles into one
    /// request a minute rather than one per contact sample.
    pub const FETCH_RETRY_MAX_SECS: f64 = 60.0;
}

pub(crate) mod http {
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
    /// client on builder failure - reqwest's default has no timeouts, so
    /// this is a conservative hardening rather than a correctness gate.
    pub fn default_client() -> reqwest::Client {
        let builder = reqwest::Client::builder().user_agent(super::avatar::USER_AGENT);
        // Neither `timeout` nor `connect_timeout` are available on the WASM
        // reqwest client: it routes through the browser's fetch API, which
        // exposes no timeout controls on the builder. Per-request timeouts
        // are enforced by `run_or`, which every fetch site goes through.
        //
        // The redirect policy is native-only for the same reason: on wasm
        // the browser follows redirects itself and exposes no hook. That is
        // the target where a beacon is least useful anyway (CORS blocks the
        // read, and mixed content blocks plain http outright); the native
        // client is the one that will happily follow a stranger's record
        // onto a corporate intranet.
        #[cfg(not(target_arch = "wasm32"))]
        let builder = builder
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(redirect_policy())
            // Name the TLS backend rather than letting feature unification
            // pick it (#1154). The manifest asks for reqwest with
            // `default-features = false, features = ["rustls-tls"]`, but
            // proto-blue-common/oauth/xrpc depend on reqwest WITH defaults,
            // which unifies `default-tls` back into the graph - and
            // reqwest's `TlsBackend::default()` prefers native-tls whenever
            // `default-tls` is present. So every native PDS and OAuth
            // request went through OpenSSL while rustls sat linked and
            // unused, and the manifest's stated posture was false on the
            // one target where TLS features do anything (wasm ignores them
            // entirely, which is why this went unseen).
            //
            // This makes the runtime choice explicit. It does not remove
            // OpenSSL from the binary - that needs proto-blue to declare
            // `default-features = false` upstream. `cargo tree -i openssl`
            // is in the doc-verification checklist so its return is noticed.
            .use_rustls_tls();
        // Not `unwrap_or_default()`: that silently substituted
        // `Client::new()` - no timeout, no redirect policy, and default-tls
        // - for the client this function is entirely about configuring.
        // A builder failure here is a broken build, not a runtime
        // condition to paper over, and every caller of this is on a path
        // where a silently unconfigured client is worse than a loud stop.
        builder
            .build()
            .expect("HTTP client builder rejected its own configuration")
    }

    /// Follow at most three redirects, and never onto a scheme or host the
    /// reference itself would have been refused for (#1127).
    ///
    /// The default ten-hop policy made the URL checks in
    /// `pds::sanitize` advisory: a record could name a compliant
    /// `https://` address whose only job is to answer `302` with
    /// `http://192.168.0.1/`, and the client would follow it. Checking the
    /// origin and re-checking every hop is what makes the up-front check
    /// mean something.
    #[cfg(not(target_arch = "wasm32"))]
    fn redirect_policy() -> reqwest::redirect::Policy {
        /// Enough for the CDN and PDS shapes in use (a canonical-host hop
        /// plus a signed-URL hop); far short of a chain used to launder a
        /// destination past a one-shot check.
        const MAX_HOPS: usize = 3;

        reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= MAX_HOPS {
                return attempt.error("too many redirects");
            }
            if crate::pds::sanitize::is_fetchable_endpoint(attempt.url().as_str()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        })
    }

    /// Process-wide shared Tokio runtime, lazily constructed on first
    /// use and reused for every native HTTP `block_on` call. Replaces
    /// the per-request `Builder::new_current_thread().build()…block_on`
    /// boilerplate that used to be duplicated across ~18 fetch sites -
    /// each of which paid for a fresh `mio` reactor, an epoll fd,
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
    /// `IoTaskPool::spawn(async move { … })` task on native - the pool
    /// worker thread has no Tokio reactor of its own, and reqwest's
    /// async machinery needs one. On WASM the browser's fetch event
    /// loop drives futures directly, so this helper is native-only;
    /// call sites already cfg-gate the native/WASM split.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        SHARED_RUNTIME.block_on(fut)
    }

    /// Drive one HTTP future to completion inside an `IoTaskPool` task,
    /// bounded on both targets, yielding `on_timeout` if it never settles.
    ///
    /// **This helper exists to make the timeout unforgettable** (#1129).
    /// Before it, twenty call sites each hand-wrote the same
    /// `#[cfg(wasm32)] { fut.await } #[cfg(not)] { block_on(fut) }` fork,
    /// and the doc on [`default_client`] assigned wasm timeout enforcement
    /// to "the caller" - which two of twenty callers actually did. The
    /// other eighteen awaited a bare browser fetch, and a browser fetch has
    /// no idle-body timeout: a PDS that accepts a connection and then drips
    /// nothing leaves the corresponding task pending forever, while the
    /// native client self-heals at [`REQUEST_TIMEOUT`]. Since any DID you
    /// share a room with points the client at that user's PDS, a tarpit
    /// host could wedge avatar icons, terrain textures, inventory and the
    /// login completion flow - on the deployed target, silently.
    ///
    /// A `with_timeout` helper callers must *remember* to wrap around the
    /// wasm arm would have the same failure mode as the doc comment did.
    /// This one absorbs the whole fork, so a new fetch site cannot be
    /// written without a bound.
    ///
    /// `on_timeout` is eager rather than a closure because every caller's
    /// timeout value is a cheap constant, and it is unused on native, where
    /// reqwest's own builder timeout has already bounded the request.
    /// How long a publish task may sit unresolved before a poll system
    /// declares it dead and frees the editor (#1129).
    ///
    /// Belt and braces behind [`run_or`]'s per-request bound: that races a
    /// browser timer inside the task, this watches the wall clock from
    /// outside it. Twice [`REQUEST_TIMEOUT`] so the inner bound always wins
    /// a fair race - reaching this one means the task itself stopped
    /// making progress, which no amount of waiting will fix, and an editor
    /// stuck on `Publishing` cannot save, log out or travel.
    pub const PUBLISH_TASK_DEADLINE: Duration = Duration::from_secs(REQUEST_TIMEOUT.as_secs() * 2);

    /// The sentence a timed-out call reports, so the status line, the
    /// toast and the log all name the same limit rather than each
    /// inventing its own wording.
    pub fn timed_out(what: &str) -> String {
        timed_out_after(what, REQUEST_TIMEOUT)
    }

    /// The same sentence for a different bound. The outer
    /// [`PUBLISH_TASK_DEADLINE`] reports its own number through this
    /// (#1206): it used to borrow [`timed_out`] and tell the owner their
    /// save gave up "after 30s" a full minute after they pressed it.
    pub fn timed_out_after(what: &str, after: Duration) -> String {
        format!("{what} timed out after {}s", after.as_secs())
    }

    pub async fn run_or<F: std::future::Future>(fut: F, on_timeout: F::Output) -> F::Output {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Native is bounded by `default_client`'s builder timeout, so
            // this arm can never produce `on_timeout`.
            drop(on_timeout);
            block_on(fut)
        }
        #[cfg(target_arch = "wasm32")]
        {
            // Race the fetch against a browser timer; the loser is dropped,
            // which aborts the underlying fetch.
            let work = async { Some(fut.await) };
            let timer = async {
                gloo_timers::future::TimeoutFuture::new(REQUEST_TIMEOUT.as_millis() as u32).await;
                None
            };
            futures_lite::future::or(work, timer)
                .await
                .unwrap_or(on_timeout)
        }
    }
}

// ---------------------------------------------------------------------------
// Login UI (ui/login/)
// ---------------------------------------------------------------------------
pub(crate) mod login {
    /// Default ATProto PDS endpoint.
    pub const DEFAULT_PDS: &str = "https://bsky.social";
    /// Default relay signaller hostname.
    pub const DEFAULT_RELAY_HOST: &str = "37.143.131.78.nip.io";
    pub const DEFAULT_TARGET_DID: &str = "";
    /// Where the login form's "New here?" link sends visitors without an
    /// ATProto account - Bluesky's signup lives on its app root.
    pub const SIGNUP_URL: &str = "https://bsky.app";
}

// ---------------------------------------------------------------------------
// Agent client (agent/, #1413)
// ---------------------------------------------------------------------------
// Unix-only like the client itself: the wasm check denies warnings, and a
// constant nothing on that target reads is one.
#[cfg(unix)]
pub(crate) mod agent {
    use std::time::Duration;

    /// How long `agent login` waits for the browser consent before it gives
    /// up and releases the loopback port the app's own login also uses.
    pub const LOGIN_WAIT: Duration = Duration::from_secs(10 * 60);
    /// The agent client's directory under the app's config directory
    /// (`prefs::config_dir`): its saved sessions and its own settings, kept
    /// apart from the ones a person signed in on this machine uses.
    pub const HOME_DIR: &str = "agent";
    /// Saved sessions, one file per account, under [`HOME_DIR`]. Private:
    /// created 0700, and every file in it 0600.
    pub const SESSIONS_DIR: &str = "sessions";
    /// The daemon's own client settings (a `prefs::PrefsStore` directory),
    /// under [`HOME_DIR`].
    pub const PREFS_DIR: &str = "prefs";
    /// One frame of the daemon's loop. Physics runs on its own fixed step
    /// (64 Hz) either way, and so does the transform stream peers see; this
    /// sets how often everything else - the network, the UI, the agent's
    /// commands - gets a turn. Half a display's rate: nobody watches these
    /// frames, and at 60 an idle daemon spent 81% of a core (#1426).
    pub const FRAME: Duration = Duration::from_nanos(1_000_000_000 / 30);
    /// Threads in the daemon's compute pool, which runs the game's systems in
    /// parallel. Bevy's default is one per core, and dispatching a frame
    /// across sixteen of them cost more than the frame's work (#1426).
    /// Measured idle, offline, 30 Hz: default 51-56% of a core, 4 threads
    /// 32%, 2 threads 27%, with no change in the time to enter a world.
    pub const COMPUTE_THREADS: usize = 2;
    /// The size of the window the daemon pretends to have, in pixels at a
    /// scale factor of one. The UI lays itself out for at least 1280x720.
    pub const VIEWPORT: (u32, u32) = (1280, 720);

    // --- The control socket (#1416) ---

    /// The directory under `$XDG_RUNTIME_DIR` holding one control socket per
    /// running agent.
    pub const RUNTIME_DIR_NAME: &str = "symbios-overlands-agent";
    /// Where the sockets go instead, under [`HOME_DIR`], on a system with no
    /// runtime directory.
    pub const RUN_DIR: &str = "run";
    /// Each account's daemon log, written by `agent start`, under
    /// [`HOME_DIR`]. Private, like the sessions beside it: it names the
    /// accounts and worlds the agent met. (Chat text stays out of it, as it
    /// stays out of the game's own session log, #1144.)
    pub const LOG_DIR: &str = "logs";
    /// The game's own session log (the diagnostics sink and its crash file),
    /// under [`HOME_DIR`], one directory per account. The game's default is
    /// `diagnostics/` under the working directory, so a daemon started from
    /// anywhere but the repo root left megabytes an hour there, outside the
    /// root's ignore rule (#1476). Private, like [`LOG_DIR`].
    pub const DIAGNOSTICS_DIR: &str = "diagnostics";
    /// How many events the daemon keeps for `agent events`. An agent that
    /// falls further behind is told how many it missed.
    pub const EVENT_CAPACITY: usize = 1000;
    /// The longest request line the socket reads. Most commands are small,
    /// but `room set` may carry a whole world record, which a save refuses
    /// past `pds::record_size::HARD_RECORD_CEILING_BYTES` (900 KiB) - so
    /// the line holds that and the JSON around it. It only bounds what a
    /// misbehaving client can make the daemon buffer.
    pub const MAX_REQUEST_BYTES: u64 = 1024 * 1024;
    /// How long a connection has to send its request line.
    pub const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(5);
    /// How long a request for the world may wait for the daemon's frame to
    /// answer it. A healthy daemon answers within a frame; this is for one
    /// that has stopped running frames.
    pub const WORLD_ANSWER_TIMEOUT: Duration = Duration::from_secs(10);
    /// The longest `agent events --wait` the daemon honours in one call; an
    /// agent that wants to wait longer asks again.
    pub const MAX_EVENT_WAIT: Duration = Duration::from_secs(60);
    /// How long `agent start` waits for the daemon it launched to open its
    /// socket before calling the start failed.
    pub const START_WAIT: Duration = Duration::from_secs(60);
    /// How often `agent start` checks for that socket.
    pub const START_POLL: Duration = Duration::from_millis(100);

    // --- Seeing (#1416) ---

    /// How far `agent status` looks for placed things (m).
    pub const NEARBY_RADIUS_M: f32 = 80.0;
    /// The most placed things `agent status` lists, nearest first.
    pub const NEARBY_MAX: usize = 12;

    // --- Walking (#1418) ---

    /// How close a body on foot has to get for a walk to have arrived (m).
    pub const ARRIVE_ON_FOOT_M: f32 = 1.0;
    /// The same for a driven body, which cannot stop on a coin (m).
    pub const ARRIVE_WHEELED_M: f32 = 3.0;
    /// How much closer counts as progress (m). Less than this is jitter.
    pub const PROGRESS_STEP_M: f32 = 0.5;
    /// How long a walk may go without progress before it is `stuck` (s).
    pub const STUCK_AFTER_SECS: f64 = 6.0;
    /// How far off the nose a point may be before a driven body steers
    /// toward it: the sine of the angle, about 5 degrees.
    pub const STEER_DEAD_ZONE: f32 = 0.08;
    /// How far off the nose a point may be before a driven body stops and
    /// swings round to it on the spot, rather than steering on the move
    /// (degrees). Throttle and full lock together sent a car 90 degrees off
    /// its point 2.7 m straight on into the gateway in front of it (#1421).
    pub const SWING_FIRST_DEG: f32 = 30.0;
    /// The longest `agent walk-to --wait` waits for the walk to end before
    /// it gives up waiting (the walk itself carries on).
    pub const WALK_WAIT: Duration = Duration::from_secs(30 * 60);

    // --- Following and facing (#1421) ---

    /// How close `agent follow` keeps to the player it follows, unless told
    /// otherwise (m): near enough to talk, far enough not to crowd.
    pub const FOLLOW_DISTANCE_M: f32 = 3.0;
    /// How much farther than that the player may get before the follower
    /// sets off after them again (m). Without it a follower at the edge of
    /// its distance stops and starts every step the player takes.
    pub const FOLLOW_SLACK_M: f32 = 1.5;
    /// Past the follow distance by this much, a follower on foot runs to
    /// catch up (m): walking after someone who walks never closes the gap.
    pub const FOLLOW_RUN_BEYOND_M: f32 = 6.0;
    /// How far off the direction asked a `face` may leave the body (degrees).
    /// A body on foot faces the way it moves, and on a slope the ground
    /// pushes that sideways: one turn across a 10-degree rise ended 14
    /// degrees short however long it walked.
    pub const FACE_TOLERANCE_DEG: f32 = 10.0;
    /// How long a body on foot may try to turn, its steps and its pauses
    /// included, before the turn is called off (s). A turn is a few steps,
    /// not a walk: an unbounded one, deflected by a wall, walked 7 m.
    pub const FACE_STEP_SECS: f64 = 2.5;
    /// One step of a turn on foot (s): long enough for the default walker
    /// (turn rate 12/s) to turn nine-tenths of the way, about 0.2 m of
    /// ground. A slope pushes a step sideways, so the next one aims past the
    /// way asked by however far the last one came to rest from it.
    pub const FACE_PULSE_SECS: f64 = 0.25;
    /// How long a turn lets go and waits for the body to come to rest before
    /// it judges where the body faces (s): a walker keeps turning toward the
    /// way it still moves, and a swung car or boat carries its spin.
    pub const FACE_SETTLE_SECS: f64 = 0.3;
    /// How close the camera must look to where a walker was turned before
    /// it steps (degrees). The walk reads the camera as the frame began, so
    /// a step taken the frame the camera turns goes the old way.
    pub const CAMERA_CAUGHT_UP_DEG: f32 = 5.0;

    // --- Flying (#1430) ---

    /// How high a flying body cruises: its underside this far above the
    /// highest ground on the next stretch of its course, water counted as
    /// ground (m). Over the seeded gateways, settlements and trees; a
    /// landmark can stand taller, and a flight into one ends `stuck` as a
    /// walk into a wall does.
    pub const CRUISE_HEIGHT_M: f32 = 20.0;
    /// How far along its course a flying body looks at the ground it has to
    /// clear (m): six seconds at an airship's top speed, time to climb 30 m
    /// before it gets there. At 30 m of look-ahead, an airship came over a
    /// mesa's 30 m cliff 13.6 m clear of it rather than 20.
    pub const CRUISE_LOOKAHEAD_M: f32 = 45.0;
    /// How far apart it samples that ground (m).
    pub const CRUISE_SAMPLE_M: f32 = 2.0;
    /// The least a flying body keeps between its underside and whatever
    /// stands under it - a roof, a tree - which the ground's height does not
    /// know about (m).
    pub const CRUISE_CLEARANCE_M: f32 = 5.0;
    /// How near its cruising height a body taking off climbs before it sets
    /// off (m).
    pub const TAKE_OFF_WITHIN_M: f32 = 2.0;
    /// How close over its point a flying body has to be to come down on it
    /// (m).
    pub const ARRIVE_FLYING_M: f32 = 2.0;
    /// How slowly it has to be moving over its point to start down (m/s).
    pub const LAND_OVER_SPEED: f32 = 1.0;
    /// Within this of its point a flying body stops turning its nose (m): the
    /// bearing to a point it passes over swings right round.
    pub const NOSE_HOLD_WITHIN_M: f32 = 4.0;
    /// Coming down onto the ground, a flying body comes down at full speed
    /// and lets go once coasting would carry it this far under the surface
    /// (m) - so it always gets there, and meets it at about a fifth of a
    /// metre a second. Let down at full speed an airship meets the ground at
    /// 5 m/s; met at half a metre a second, a hillside tipped it over.
    pub const LAND_SINK_M: f32 = 0.2;
    /// How near what is below it counts as on it (m).
    pub const SURFACE_M: f32 = 0.05;
    /// How near what is below it a body at rest counts as down on it (m).
    pub const TOUCHDOWN_HEIGHT_M: f32 = 0.3;
    /// Coming down onto water, how far from its surface the height a body
    /// would coast to rest at, let go now, may be (m). Near its top speed
    /// down a frame's key moves that height about 0.17 m; from rest, about
    /// a metre.
    pub const WATER_REST_M: f32 = 0.15;
    /// How high over water a body has to be to get up to its top speed down
    /// before it lets go to coast onto the surface (m); lower, it climbs
    /// first. An airship coasts 6.25 m from 5 m/s, and takes about a second
    /// and three metres to get up to that speed.
    pub const WATER_COAST_FROM_M: f32 = 10.0;
    /// How far a coming-down body may move in [`TOUCHDOWN_SECS`] and still
    /// be at rest (m): a tenth of a metre a second. A descent near the
    /// ground is let alone down to about 0.3 m/s, and a body judged at rest
    /// while that slow is let go still moving.
    pub const TOUCHDOWN_STILL_M: f32 = 0.03;
    /// How long it has to have been at rest to have landed (s).
    pub const TOUCHDOWN_SECS: f64 = 0.3;
    /// How far from upright a body may lean and still be at rest (degrees).
    /// Set down on a hillside, an airship meets it with its uphill edge and
    /// tips, and its stabiliser then stands it up again - pivoting on that
    /// edge, which lifts it a hand's breadth clear.
    pub const TOUCHDOWN_LEAN_DEG: f32 = 2.0;
    /// The vertical speed a flying body asks for per metre it is off the
    /// height it wants (1/s).
    pub const ALTITUDE_GAIN: f32 = 1.0;
    /// How far its vertical speed may stray from the one it asks for before
    /// it presses Space or Shift (m/s). Either key moves it about 0.7 m/s in
    /// a daemon frame, so a narrower band would press one and then the other.
    pub const CLIMB_BAND: f32 = 0.5;
    /// The same for its speed over the ground, along its nose and beside it
    /// (m/s).
    pub const THRUST_BAND: f32 = 0.3;
    /// How far off where it wants its nose a flying body may leave it
    /// (degrees), once the spin it has left is counted in.
    pub const YAW_DEAD_ZONE_DEG: f32 = 2.0;
    /// How much of what W and S can do the approach to a point plans to
    /// brake with, so it stops over the point rather than past it.
    pub const APPROACH_BRAKE_SHARE: f32 = 0.5;
    /// Where the approach plans to have stopped, short of the point (m).
    pub const APPROACH_STOP_SHORT_M: f32 = 0.5;
    /// Coming down, the speed over the ground a flying body asks for per
    /// metre it is off its point (1/s), and the most it asks for (m/s).
    pub const LAND_SLIDE_GAIN: f32 = 0.5;
    pub const LAND_SLIDE_SPEED: f32 = 1.0;
    /// Turning this far counts as getting somewhere, for `stuck` (degrees):
    /// a long airship swings round more slowly than a walk is called stuck.
    pub const PROGRESS_TURN_DEG: f32 = 10.0;
    /// How far along its course a flying body sweeps itself for something in
    /// the way at its own height - a landmark, a tower, a cliff - which the
    /// ground's height does not show (m).
    pub const OBSTACLE_SWEEP_M: f32 = 20.0;
    /// How much further than it can stop in a thing ahead has to be before
    /// the body stops and climbs over it (m).
    pub const OBSTACLE_MARGIN_M: f32 = 4.0;

    // --- Following in a body that flies (#1430, owner 2026-09-24) ---

    /// How high a flying body escorts a player it follows: its underside
    /// this far above the highest ground on the way to them (m) - low enough
    /// to be with them, high enough to clear what they walk past.
    pub const ESCORT_HEIGHT_M: f32 = 8.0;
    /// How long a player has to stand still before a flying follower lands
    /// beside them (s).
    pub const LAND_BESIDE_AFTER_SECS: f64 = 5.0;
    /// Moving less than this a player stands still (m).
    pub const PLAYER_STILL_M: f32 = 0.5;
    /// The least room a flying follower leaves between its side and the
    /// player, whatever distance it was asked to keep (m) - it lands beside
    /// them, never on them.
    pub const ESCORT_ROOM_M: f32 = 1.5;

    // --- Flying an airplane (#1431, owner 2026-09-24) ---
    //
    // The game's airplane lifts straight up in proportion to its forward
    // speed and nothing else, so it holds its height by its speed; and at
    // the daemon's frame one press of A or D rolls it about 140 degrees and
    // one of W or S pitches it about 27, so it flies nose-level and turns by
    // the rudder alone. See `agent::daemon::movement::wing`.

    /// The slowest an airplane flies, as a share over its stall speed.
    pub const WING_STALL_MARGIN: f32 = 1.2;
    /// The vertical speed an airplane asks for per metre it is off its
    /// height (1/s), and the most it asks for up and down (m/s).
    pub const WING_CLIMB_GAIN: f32 = 0.6;
    pub const WING_MAX_CLIMB: f32 = 3.0;
    pub const WING_MAX_SINK: f32 = 2.0;
    /// The forward speed it asks for over the speed that holds it level,
    /// per m/s its vertical speed is short of the one it wants (s).
    pub const WING_SPEED_PER_CLIMB: f32 = 1.2;
    /// How far under the speed it asks for its forward speed may fall
    /// before it opens the throttle to cruise, and to full at three times
    /// as far (m/s). Inside that, the engine is cut: a frame at cruise adds
    /// about 0.6 m/s and one at full 1.2, so the speed is held by the odd
    /// frame of power against the drag.
    pub const WING_SPEED_BAND: f32 = 0.2;
    /// How far off where it wants its nose an airplane may leave it
    /// (degrees). A frame of rudder turns it about 12.
    pub const WING_YAW_DEAD_ZONE_DEG: f32 = 6.0;
    /// How far past its point the nose may lead its path, so the path,
    /// which follows the nose only over seconds, comes round sooner
    /// (degrees).
    pub const WING_LEAD_MAX_DEG: f32 = 40.0;
    /// The most its nose may point off its path (degrees): its lift is its
    /// speed along its nose, and a nose swung 140 degrees round to a point
    /// behind it left it no lift at all.
    pub const WING_NOSE_OFF_PATH_MAX_DEG: f32 = 45.0;
    /// How far its nose may pitch off level before it is pitched back
    /// (degrees). Nothing in flight pitches it, but a knock does - and
    /// pitched up, its thrust and its lift point skyward and it climbs
    /// without end.
    pub const WING_PITCH_TOLERANCE_DEG: f32 = 20.0;
    /// Its path lines up on its point when it points within this of it
    /// (degrees).
    pub const LINED_UP_DEG: f32 = 10.0;
    /// And when it passes within this of the point, to one side (m).
    pub const LINED_UP_ASIDE_M: f32 = 4.0;
    /// On a final, something standing under the glide that the ground's
    /// height does not show - a roof, a tree - is in the way when it stands
    /// this far over the ground (m), and the airplane is within this of it:
    /// a boulder is not, nor a cliff's edge its footprint straddles.
    pub const UNDER_THE_GLIDE_M: f32 = 3.0;
    /// How far out an airplane's final approach starts (m): lined up by
    /// then, it comes down on its point; not, it goes round.
    pub const FINAL_M: f32 = 80.0;
    /// How far out it flies to come round again (m): far enough that its
    /// path - which swings round over tens of metres - has lined up again
    /// by the time it is back at the final's start.
    pub const GO_ROUND_TO_M: f32 = 170.0;
    /// How many times it comes round before it lands where it can and says
    /// it is stuck.
    pub const MAX_GO_ROUNDS: u32 = 2;
    /// The final's glide: metres down per metre on.
    pub const GLIDE_SLOPE: f32 = 0.15;
    /// Where it aims to touch down, short of the point (m): about what it
    /// slides, engine off, after a gentle touchdown at its approach speed.
    /// Set down gently its lift still carries nearly all its weight, so the
    /// ground barely brakes it: 9 m from 8.4 m/s (a heavier touchdown, 1-2).
    pub const TOUCHDOWN_SHORT_M: f32 = 8.0;
    /// Under this height it rounds out, coming down more slowly the lower
    /// it is (m).
    pub const FLARE_FROM_M: f32 = 1.0;
    /// The sink it touches down at, rounded out (m/s).
    pub const FLARE_SINK: f32 = 0.6;
    /// Stopped this near its point, an airplane has arrived (m). What it
    /// slides after touching down varies from about 4 m to 9, so its stop
    /// cannot be promised much nearer; one run stopped 7.1 m off.
    pub const ARRIVE_WINGED_M: f32 = 10.0;
    /// The clear run an airplane needs on the ground to take off (m).
    pub const TAKE_OFF_RUN_M: f32 = 25.0;
    /// How high over the ground it sweeps its run (m): clear of the ground
    /// it rests on, not of a kerb.
    pub const TAKE_OFF_SWEEP_LIFT_M: f32 = 0.3;
    /// On the ground, a run this far off its nose is swung round to before
    /// it rolls (degrees).
    pub const WING_SWING_FIRST_DEG: f32 = 15.0;
    /// Landing where it can - halted, or out of tries - an airplane comes
    /// down at this (m/s), rounding out at the bottom. At 1 m/s a halt 12 m
    /// up glided on 185 m before it was down.
    pub const FORCED_SINK: f32 = 2.0;

    // --- Looking (#1420) ---

    /// A picture's size in pixels: about what a vision model takes in whole,
    /// 16:9 like the game's own window, and a width that is a multiple of 64
    /// so the GPU readback has no row padding to strip.
    pub const LOOK_SIZE: (u32, u32) = (1024, 576);
    /// Frames the snapshot camera renders before its picture is read back.
    /// A camera spawned where it shoots draws everything on its first frame:
    /// measured, frame 0 and frame 3 of the same pose differed only where
    /// something moves - fronds in the wind, clouds, water, the body's idle
    /// (0.05% and 0.72% of the pixels, two views, 2026-09-23). The one frame
    /// is for a material still being prepared when the camera arrives, which
    /// the renderer queues again on the next.
    pub const LOOK_WARMUP_FRAMES: u32 = 1;
    /// How long a `look` may take before it is given up and its camera
    /// removed. The first one in a daemon's life compiles the render
    /// pipelines it needs, on the render thread, before its first frame.
    pub const LOOK_ANSWER_TIMEOUT: Duration = Duration::from_secs(120);
    /// Where pictures go unless `--out` says otherwise, under [`HOME_DIR`]:
    /// one private directory per account.
    pub const LOOKS_DIR: &str = "looks";
    /// How many pictures that directory keeps; the oldest go first.
    pub const LOOKS_KEPT: usize = 32;
    /// Where the eyes are in a body, as a fraction of its height from its
    /// feet: an adult's eye line is 93-94% of their stature, and a vehicle
    /// sees from near its top.
    pub const EYE_HEIGHT_FRACTION: f32 = 0.93;

    // --- The interface (#1424) ---

    /// How long an interface command may take to answer. It gives itself up
    /// after 90 frames - three seconds at the daemon's rate - so this is for
    /// a daemon whose frames have slowed, as a world compiling slows them.
    pub const UI_ANSWER_TIMEOUT: Duration = Duration::from_secs(30);

    // --- Editing (#1422) ---

    /// How far past a catalogue entry's clearance radius `agent place` with
    /// no point puts it ahead of the agent (m): clear of the body, near
    /// enough to see it land.
    pub const PLACE_AHEAD_M: f32 = 2.0;
    /// The longest `agent save --wait` waits for the save to land. A save
    /// is given up on by the game after `PUBLISH_TASK_DEADLINE` (a minute);
    /// this is that and a margin.
    pub const SAVE_WAIT: Duration = Duration::from_secs(90);
    /// The longest `agent gift give --wait` waits for an answer (#1423). An
    /// offer nobody answers lapses after `PENDING_OFFER_TIMEOUT_SECS` (three
    /// minutes), which answers the wait too; this is that and a margin.
    pub const GIFT_WAIT: Duration = Duration::from_secs(200);

    // --- Travel (#1419) ---

    /// How long the unsaved-edits guard may stand asking before the daemon
    /// withdraws the trip and says why: nobody is there to answer the
    /// dialog it draws (s).
    pub const GUARD_WAIT_SECS: f64 = 3.0;
    /// The longest `agent travel --wait` waits for the trip to end.
    pub const TRAVEL_WAIT: Duration = Duration::from_secs(5 * 60);

    // --- Offline mode (#1415) ---

    /// The identity an offline agent stands in as: a well-formed `did:plc`
    /// no directory knows, so its world and its body are the ones seeded
    /// from it (the loading gate reads an unknown identity as "nothing
    /// published"). Its seeded body walks. `--stand-in` names another, and
    /// with it another body: `did:plc:agentofflinecar22222222e` is a
    /// roadster (a car), `did:plc:agentofflineboat2222222d` a steam tug (a
    /// hover-boat) and `did:plc:agentofflineair222222222` a twin-envelope
    /// airship (a helicopter, and the slowest of them to turn) - found with
    /// the render tool's `--outfit` (#1421, #1430).
    pub const OFFLINE_DID: &str = "did:plc:agentofflinestandin22222";
    /// The offline agent's handle, on a name that can never resolve.
    pub const OFFLINE_HANDLE: &str = "agent.offline.invalid";
    /// The relay an offline agent is pointed at: a name that can never
    /// resolve (RFC 6761), so its socket never leaves the machine and it
    /// meets nobody.
    pub const OFFLINE_RELAY_HOST: &str = "relay.offline.invalid";
}

// ---------------------------------------------------------------------------
// UI panels (ui/chat.rs, ui/diagnostics.rs, ui/avatar/, ui/room/, ui/login/)
// ---------------------------------------------------------------------------
pub(crate) mod ui {
    pub mod chat {
        /// How long a chat message may be, in CHARACTERS - the limit the
        /// user is held to, and the one the counter and the field's own
        /// `char_limit` are derived from.
        ///
        /// **Counted in characters because a byte cap is a script tax
        /// (#1264 f362).** The cap used to be 512 bytes, applied to a
        /// UTF-8 string: 512 Latin characters, about 256 of Greek,
        /// Cyrillic, Hebrew or Arabic, about 170 CJK and about 128 emoji.
        /// A Japanese writer got a third of the message length everyone
        /// else got, with no counter, no `char_limit` and no warning -
        /// they learned about it by watching their own sentence get
        /// amputated in their own HUD, on both sides silently.
        pub const MAX_MESSAGE_CHARS: usize = 512;

        /// Hard ceiling on a chat payload's SIZE IN BYTES, enforced on
        /// receipt against whatever a peer actually sent.
        ///
        /// This is the DoS backstop and it belongs on the wire, not on
        /// the typist: without it a hand-crafted packet could ship an 800
        /// KiB string of junk (well under the 1 MiB multiuser packet
        /// limit) that every guest's egui would re-wrap on every frame.
        ///
        /// Four bytes per permitted character, which is the most UTF-8
        /// can spend on one, so a message the sender was allowed to write
        /// can never be clipped by this on arrival -
        /// `the_wire_ceiling_cannot_truncate_a_permitted_message` holds
        /// that. The peer-side cost this bounds went up 4x with it and is
        /// still far below what the rolling history cap allows.
        pub const MAX_MESSAGE_BYTES: usize = 4 * MAX_MESSAGE_CHARS;
        /// Maximum chat entries retained in the rolling HUD log. A noisy (or
        /// malicious) peer could otherwise spam the channel until egui's
        /// scroll area holds megabytes of strings, re-wrapping every frame.
        pub const MAX_HISTORY_ENTRIES: usize = 500;

        /// How many messages one peer may send back-to-back before the
        /// per-sender token bucket starts dropping them (#1222 f296).
        ///
        /// Flood is the cheapest attack in any chat product, and the
        /// rolling cap above is what makes it destructive: 500 messages
        /// evict the room's entire prior conversation, permanently, before
        /// the victim can reach a mute control two windows away. A burst of
        /// eight covers every legitimate pattern - a pasted multi-line
        /// thought sent as several lines, two people answering at once -
        /// and costs a flooder their whole advantage.
        pub const BURST_MESSAGES: f64 = 8.0;
        /// Sustained rate (messages per second) the bucket refills at. One
        /// a second is faster than anybody types and two orders of
        /// magnitude below what a script can send.
        pub const MESSAGES_PER_SEC: f64 = 1.0;
        /// Minimum seconds between two "this peer is being throttled" log
        /// lines for the same sender. The point of the limiter is to stop
        /// N events becoming N records; a per-message log entry would move
        /// the flood into the session log instead of stopping it.
        pub const THROTTLE_REPORT_INTERVAL_SECS: f64 = 10.0;
        // Author + mutual colours moved to the semantic theme (#856):
        // author tag = `status.info`, mutual star = `accent` (the old
        // warm gold sat in the warn-amber family - a friend must not
        // read as a caution). The `★` glyph still carries the mutual cue
        // for colour-blind viewers / greyscale captures.
    }

    // Window geometry (positions AND default sizes) lives in
    // `crate::ui::layout` since #833 - defaults are computed from the
    // screen rect there, not pixel constants here.

    /// Where the Feedback affordance sends people (#1291).
    ///
    /// A `userinput.app` space, which is itself an ATProto app: the board
    /// is an `app.userinput.space` record and every post an
    /// `app.userinput.discussion` one, living in the same repo as this
    /// project's own `network.symbios.overlands.*` records. So a person
    /// who can sign in to Overlands can already post here with the
    /// identity they arrived with - which is why the affordance is worth
    /// having on the login screen as well as in game.
    ///
    /// The DID is the owner's and the rkey is the space record's, so the
    /// address is stable across a handle change. Do not "tidy" it into a
    /// handle-based URL.
    pub const FEEDBACK_URL: &str =
        "https://userinput.app/s/did:plc:z5yhcebtrvzblrojezn6pjgi/3mnx4ozkpex2s";

    /// Interface-scale bounds for the #1259 f239 control, and the floor
    /// and ceiling any persisted or keyboard-driven value is clamped to.
    ///
    /// 0.8 is the smallest step that still leaves the 9-to-11 pt `Small`
    /// tier readable; 2.0 is where the 1280x720 toolbar's ~1100 pt of
    /// non-wrapping controls stop fitting at all (see #1261). Both ends
    /// have to stay reachable BY THE SLIDER - a scale a user cannot undo
    /// from inside the app is a lockout.
    pub const UI_SCALE_MIN: f32 = 0.8;
    pub const UI_SCALE_MAX: f32 = 2.0;

    pub mod diagnostics {
        /// Severity → HUD colour `[R, G, B]` - the single map the diagnostics
        /// event-log tint, the anomaly badges/pills, the per-metric dots and the
        /// toolbar worst-active dot all read (C-6), so a warning is the same
        /// amber everywhere. Trace/Info are neutral greys; Warn amber, Error
        /// orange, Critical red.
        ///
        /// **Widened by #1259 f236.** The old ramp was three oranges: Warn
        /// `[210,170,90]` and Error `[210,120,90]` were identical in R and B,
        /// 50 apart in G, a 1.47:1 luminance ratio - and Error and Critical
        /// were 40 apart in channel-sum. Severity was legible only to
        /// somebody comparing two dots side by side. The steps now clear the
        /// palette's own distinctness bar AND fall monotonically in
        /// luminance (Warn 0.557 → Error 0.307 → Critical 0.182), so the
        /// ramp still ranks correctly in greyscale or under any of the
        /// dichromacies - hue is no longer carrying it alone.
        ///
        /// Trace was `[96,96,96]`: 2.71:1 against the window, under WCAG's
        /// 3:1 floor, on a tier that tints whole event-log lines.
        ///
        /// **Info quietened by #1271 f184.** It was `[220,220,220]` -
        /// 12.56:1 on the dark window against the Warn tier's 9.95:1, so
        /// the routine chatter was the brightest thing in the event log
        /// and an alarm line was quieter than the noise around it. An
        /// Info-severity line is secondary text, so the tier is now the
        /// dark palette's `text_weak` exactly (5.12:1, still clear of AA).
        /// `ui::theme::the_quiet_tiers_stay_inside_the_secondary_text_band`
        /// holds the rule for all three palettes; the ramp's ORDER is
        /// #1259's and is deliberately untouched.
        pub const SEVERITY_TRACE_RGB: [u8; 3] = [130, 130, 130];
        pub const SEVERITY_INFO_RGB: [u8; 3] = [140, 140, 140];
        pub const SEVERITY_WARN_RGB: [u8; 3] = [240, 190, 60];
        pub const SEVERITY_ERROR_RGB: [u8; 3] = [245, 110, 30];
        pub const SEVERITY_CRITICAL_RGB: [u8; 3] = [225, 45, 60];
    }

    pub mod login {
        // The "Enter the Overlands" button's fill colour moved to the
        // semantic theme (#855): `ui::theme::Theme::accent_fill`; the
        // backdrop gradient stops live there too (#896). Only geometry
        // stays here. The login screen (#896) is composed of a hero
        // wordmark plus two frameless cards centred as a pair from the
        // live screen rect - the same screen-relative philosophy as
        // `ui::layout` (#833), but computed locally since the login
        // screen has no toolbar carving the rect and no drag-to-move.
        /// Height (px) of the full-width "Enter the Overlands" button.
        /// Tall enough to read as the screen's primary call to action,
        /// not just another control.
        pub const ENTER_BUTTON_HEIGHT: f32 = 44.0;
        /// Button label text size (px) - larger than body text to
        /// match the enlarged hit area.
        pub const ENTER_BUTTON_TEXT_SIZE: f32 = 18.0;

        /// Content width (px) of the login card. On viewports too narrow
        /// to hold it plus [`EDGE_PAD`], the card shrinks to fit.
        pub const CARD_WIDTH: f32 = 400.0;
        /// Content width (px) of the `#Overlands` feed card.
        pub const FEED_CARD_WIDTH: f32 = 360.0;
        /// Horizontal gap between the two cards when they sit side by
        /// side; also the vertical gap when a narrow viewport stacks
        /// the feed card underneath the login card.
        pub const CARD_GUTTER: f32 = 24.0;
        /// Card chrome: content padding inside the rounded frame.
        pub const CARD_INNER_MARGIN: f32 = 16.0;
        /// Card chrome: corner rounding radius (px).
        pub const CARD_CORNER_RADIUS: f32 = 8.0;
        /// Top of the hero wordmark, as a fraction of screen height.
        pub const HERO_TOP_FRAC: f32 = 0.12;
        /// Top of the card pair, as a fraction of screen height -
        /// clamped below the hero's actual bottom edge at render time
        /// so short viewports never overlap the two.
        pub const CARDS_TOP_FRAC: f32 = 0.28;
        /// Cap on the feed card's scrollable body, as a fraction of
        /// screen height, so the pair reads as one balanced composition
        /// instead of a short card beside an arbitrarily tall one.
        pub const FEED_MAX_HEIGHT_FRAC: f32 = 0.48;
        /// Minimum breathing room (px) between any card edge and the
        /// screen edge on cramped viewports.
        pub const EDGE_PAD: f32 = 16.0;
        /// Hero wordmark text size (px).
        pub const WORDMARK_TEXT_SIZE: f32 = 32.0;
        /// Hero tagline text size (px).
        pub const TAGLINE_TEXT_SIZE: f32 = 15.0;
        /// Feed-card heading text size (px) - the card lost its window
        /// title bar (#896), so the heading renders in the body.
        pub const FEED_HEADING_TEXT_SIZE: f32 = 16.0;
        /// Padding inside the "New world" backdrop-re-roll chip (#978).
        /// Tighter than [`CARD_INNER_MARGIN`]: it wears the same card
        /// chrome, but a lone button in a full card's padding reads as a
        /// third panel competing with the pair rather than a control
        /// sitting on the world.
        pub const REROLL_INNER_MARGIN: f32 = 8.0;
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
        /// full live record to count its bytes - cheap enough at 2 Hz even
        /// for a large room record, wasteful at 60 Hz.
        pub const SIZE_READOUT_REFRESH_SECS: f64 = 0.5;

        /// Undoable steps each editor's history ring holds beyond the
        /// baseline (#862; decision 2026-07-18). Each entry is a whole
        /// record clone.
        ///
        /// This used to say "typically under the 100 KiB publish soft
        /// budget - so 32 bounds a ring at a few MiB". It does not:
        /// [`crate::pds::record_size::SOFT_RECORD_BUDGET_BYTES`] measures
        /// the largest single PUBLISHED record after the manifest/child
        /// split (#697), while the ring stores the ASSEMBLED in-memory
        /// room. GothicHorror's seeded default is 348.6 KiB assembled
        /// against 53.9 KiB largest published, before anything is
        /// authored. So depth alone was never a memory bound and
        /// [`UNDO_RING_BUDGET_BYTES`] is the one that is (#1270 f417).
        pub const UNDO_DEPTH: usize = 32;

        /// Serialized bytes an editor's history ring may hold before the
        /// oldest entries are evicted, independently of [`UNDO_DEPTH`]
        /// (#1270 f417).
        ///
        /// 8 MiB. The heaviest seeded default assembles to 348.6 KiB, so
        /// this holds a little over twenty of the worst case the
        /// catalogue can produce and all 33 of anything ordinary - the
        /// bound only bites on a room far bigger than anything shipped,
        /// which is exactly when it needs to.
        ///
        /// Measured on the SERIALIZED form, which is a lower bound on the
        /// in-memory cost (`String`s, `Vec` capacity slack, a `HashMap`'s
        /// table). Wrong in the safe direction, and it matters most on
        /// wasm, where a transient high-water mark is permanent - the
        /// linear heap never gives memory back.
        pub const UNDO_RING_BUDGET_BYTES: usize = 8 * 1024 * 1024;

        /// Undoable steps the byte budget may never trim below (#1270
        /// f417). A single record larger than [`UNDO_RING_BUDGET_BYTES`]
        /// on its own would otherwise evict the ring down to the
        /// baseline, which is a silent removal of undo - and undo matters
        /// most in exactly the enormous world that would trigger it.
        pub const MIN_UNDO_DEPTH: usize = 2;
    }

    /// Transient toast notifications (`crate::ui::toast`, #819).
    pub mod toast {
        /// Seconds a toast stays visible before pruning. Matches the 6 s
        /// the Diagnostics window's hand-rolled statuses used before they
        /// migrated onto this channel.
        pub const DURATION_SECS: f64 = 6.0;

        /// Queue cap: a burst past this drops the oldest entry. Toasts
        /// are glanceable feedback, not a log - the diagnostics event
        /// log is the durable record.
        pub const MAX_VISIBLE: usize = 6;

        // The Success dot colour moved to the semantic theme (#856):
        // `ui::theme::StatusPalette::ok`, shared with every other
        // success indicator in the app.

        /// Max text width before wrapping.
        pub const MAX_WIDTH: f32 = 320.0;

        /// Text length (chars) past which `Toasts::push` elides with an
        /// ellipsis (#1205). Toasts quote user-authored names and peer
        /// records; at `MAX_WIDTH` this is a handful of lines, so no
        /// string anyone wrote can cover the screen from the
        /// Foreground layer.
        pub const MAX_TEXT_CHARS: usize = 240;

        /// How far below the panel-free rect's TOP edge the toast stack
        /// begins, centred horizontally (#1286).
        ///
        /// **Why not a corner at all.** #1261 f43 moved the stack off the
        /// top-RIGHT because every right-anchored window in `ui::layout` -
        /// Chat, People, Inventory, Controls, Settings - opens in that
        /// corner, and the toast area is a real pointer area at
        /// `Order::Foreground`, so a stack of up to [`MAX_VISIBLE`] rows
        /// covered their title bars and ate clicks for the toast's full
        /// life. The bottom-right corner it moved to has no such
        /// neighbour, but it turned out to be **easy to miss on a large
        /// display**: the eye is on the middle of the screen and the
        /// feedback was in the far corner of a 5760-wide desktop.
        ///
        /// Centre-top is where a user is already looking and no window
        /// slot claims it - `SlotAnchor` is Left, Right or CenterLeft,
        /// and a CenterLeft editor's title bar starts well left of centre.
        ///
        /// 44 and not 8: `ui::modes`' movement-mode banner sits at the
        /// panel-free top + 8 with a popup frame, and it is a standing
        /// state cue that a transient message should not sit on top of.
        /// Measured from the PANEL-FREE rect, not `content_rect` - an
        /// anchored `Area` aligns within `content_rect`, which INCLUDES
        /// the toolbar panel, which is why anything anchored `CENTER_TOP`
        /// lands underneath it (the same trap `ui::layout`'s header
        /// records for windows).
        pub const TOP_OFFSET: f32 = 44.0;
    }

    /// Drag-to-place drop preview (`crate::ui::inventory::drop`, #831):
    /// the ground ring that shows where an armed drag will land.
    pub mod drop_preview {
        /// Ring + post colour when the release would place here
        /// [R, G, B, A] - the blob-edit "add" green family.
        pub const VALID_COLOR: [f32; 4] = [0.15, 0.85, 0.30, 0.9];
        /// Ring colour when the ground under the cursor can't take the
        /// drop (visiting someone else's overland) - the "carve" red.
        pub const INVALID_COLOR: [f32; 4] = [0.90, 0.15, 0.15, 0.9];
        /// Footprint radius when the dragged item has no catalogue
        /// clearance metadata (inventory blueprints).
        pub const DEFAULT_RADIUS_M: f32 = 0.75;
        /// Height of the vertical marker post above the hit point.
        pub const POST_HEIGHT_M: f32 = 1.5;
    }

    /// Gizmo snap increments (`crate::editor_gizmo::GizmoFramePref`,
    /// #827): the defaults the Snap toggle starts from. Chosen for
    /// building-scale alignment work - half-metre grid, 15° angles
    /// (24 stops per turn), quarter scale steps.
    pub mod gizmo_snap {
        pub const DISTANCE_M: f32 = 0.5;
        pub const ANGLE_DEG: f32 = 15.0;
        pub const SCALE: f32 = 0.25;
    }

    /// Click-to-pick face selection (`crate::editor_gizmo::face_pick`,
    /// #961): the brief in-scene confirmation of which face a click
    /// resolved.
    pub mod face_pick {
        /// Wireframe colour [R, G, B, A] for the picked face's triangles.
        /// Cyan - deliberately none of the neighbouring signals: not the
        /// selection box's amber, not the blob proxies' add-green /
        /// carve-red, not the wireframe's blue-grey.
        pub const HIGHLIGHT_COLOR: [f32; 4] = [0.25, 0.95, 1.0, 0.9];

        /// Seconds the highlight stays up, fading out over its life. Long
        /// enough to read on a face the cursor is still covering, short
        /// enough that it never becomes scenery.
        pub const HIGHLIGHT_SECS: f64 = 0.7;

        /// Metres each outlined triangle is lifted along its own normal
        /// before drawing. Gizmo lines are depth-tested, and an outline
        /// drawn exactly on the surface it outlines z-fights into dashes;
        /// a hair's clearance reads as a clean wireframe at any distance a
        /// face is actually clickable from.
        pub const HIGHLIGHT_LIFT_M: f32 = 0.004;

        /// Triangle ceiling for the highlight. A face can be the whole
        /// surface of a subdivided sphere; drawing every edge of it would
        /// cost more than the confirmation is worth, so past this many
        /// triangles the outline is a representative sample of the face
        /// rather than all of it.
        pub const MAX_HIGHLIGHT_TRIANGLES: usize = 1500;
    }

    /// Overhead peer nametags (`crate::ui::nametag`, #1226): the only
    /// in-world identity the product has.
    ///
    /// The numbers are a legibility budget, not a taste: a tag has to be
    /// readable at the distance you would actually decide to mute
    /// somebody from, and must not turn a busy room into a wall of text.
    pub mod nametag {
        /// Past this many metres a peer carries no tag at all. Chosen
        /// against the chat radius rather than the draw distance: a name
        /// you cannot act on is noise, and every social action in the
        /// product is addressed to somebody you can see.
        pub const MAX_DISTANCE_M: f32 = 60.0;

        /// Where the fade begins. The band between this and
        /// [`MAX_DISTANCE_M`] exists so a tag thins out as its owner
        /// walks away instead of blinking off mid-stride - a hard cutoff
        /// reads as a bug in exactly the frame the user is watching.
        pub const FADE_START_M: f32 = 35.0;

        /// Least alpha a drawn tag is given, so the fade never bottoms out
        /// into "present but invisible" - below this the tag is dropped.
        pub const MIN_ALPHA: f32 = 0.15;

        /// Clearance between the top of a peer's rendered bounds and the
        /// baseline of their tag, in metres. Enough that a tag does not
        /// sit on a hat, small enough that it still reads as attached to
        /// the body rather than floating over the scene.
        pub const HEAD_CLEARANCE_M: f32 = 0.35;

        /// Height above the chassis origin used when a peer has no
        /// rendered bounds yet - a body still resolving from the PDS, or
        /// a chassis whose meshes have not spawned. Roughly a person plus
        /// the clearance above; the tag is the only thing on screen for
        /// that peer, so it must not fall to the origin.
        pub const FALLBACK_HEIGHT_M: f32 = 2.2;

        /// Colour of the wire box drawn around the body of the peer whose
        /// roster row is under the pointer [R, G, B, A]. The identity
        /// accent (teal), deliberately NOT the editor's amber selection:
        /// hovering a person is not selecting an object, and the two
        /// boxes can be on screen at once.
        pub const FOCUS_BOX_COLOR: [f32; 4] = [0.18, 0.80, 0.78, 0.85];

        /// Floor on each axis of that box, so a flat or still-empty
        /// chassis draws a visible sliver rather than nothing.
        pub const MIN_FOCUS_BOX_EXTENT: f32 = 0.25;
    }

    /// In-scene selection highlight (`crate::editor_gizmo::highlight`,
    /// #822 / W5): wire boxes around what the gizmo will affect.
    pub mod selection_highlight {
        /// Box colour [R, G, B, A] for the selected node's subtree on the
        /// gizmo-hosting instance. Warm amber - the classic selection
        /// accent, distinct from the blob proxies' add-green/carve-red
        /// and the wireframe's cool blue-grey.
        pub const SELECTED_COLOR: [f32; 4] = [1.0, 0.82, 0.25, 0.95];

        /// Box colour for the OTHER live instances of the same blueprint
        /// node (a scattered generator edits every instance at once, so
        /// the blast radius is shown honestly - but dimly, one box per
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
        /// Green - "this element adds material". Alpha keeps the evaluated
        /// wireframe surface readable through the proxy.
        pub const PROXY_ADD_COLOR: [f32; 4] = [0.15, 0.85, 0.30, 0.28];
        /// Carve-element proxy tint. Red - "this element removes material".
        /// Slightly more opaque than [`PROXY_ADD_COLOR`]: carves sit inside
        /// the accumulated surface, so they need the extra presence to read
        /// through the wireframe shell.
        pub const PROXY_CARVE_COLOR: [f32; 4] = [0.90, 0.15, 0.15, 0.34];
        /// Alpha override applied to whichever element is selected for
        /// gizmo editing - same hue as its band, unmistakably brighter.
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

// ---------------------------------------------------------------------------
// Invariants
// ---------------------------------------------------------------------------
// The cross-constant relationships this file's doc comments promise, as
// compile-time assertions - a violation is a build failure, not a test
// failure, so it cannot be reached on any platform.
//
// This is the half of #1157 that deletion could not fix. The dead-code lint
// now catches a constant nothing *reads*; nothing caught a constant that is
// read and quietly contradicts the sentence beside it, and three of these
// span files, where no reviewer sees both numbers at once.

// `MAX_INVENTORY_SANITIZE_ITEMS` is documented as the DoS bound *above* the
// gameplay cap (#841). Inverting them restores exactly the bug that issue
// fixed: sanitise silently deleting items the user watched get saved.
const _: () = assert!(state::MAX_INVENTORY_SANITIZE_ITEMS >= state::MAX_INVENTORY_ITEMS);

// "Matches the MAX_INVENTORY_LIST_PAGES fetch ceiling (6 pages x 100
// records), so nothing the fetch can return is ever truncated."
const _: () = assert!(state::MAX_INVENTORY_LIST_PAGES * 100 <= state::MAX_INVENTORY_SANITIZE_ITEMS);

// The fetch must be able to READ BACK a full stash (#1292). Without this a
// raise to MAX_INVENTORY_ITEMS alone would let the owner save items that the
// next login silently drops on the floor - the walk stops after
// MAX_INVENTORY_LIST_PAGES with no signal that a cursor remained.
const _: () = assert!(state::MAX_INVENTORY_LIST_PAGES * 100 >= state::MAX_INVENTORY_ITEMS);

// "Page count is no longer the memory bound" (#1292). Per-page caps MULTIPLY:
// each page may spend MAX_FETCH_BODY_BYTES, so the two-page walk this replaced
// allowed 32 MiB and six pages would allow 96 MiB - and on wasm the heap never
// shrinks, so a login-time spike is resident for the session. The walk's own
// budget must stay tighter than what two pages already allowed…
const _: () =
    assert!(state::MAX_INVENTORY_FETCH_BYTES <= 2 * crate::pds::xrpc::MAX_FETCH_BODY_BYTES);
// …and it must be the binding limit, or the assertion above is satisfied by a
// number that never applies because the per-page caps bind first.
const _: () = assert!(
    state::MAX_INVENTORY_FETCH_BYTES
        < state::MAX_INVENTORY_LIST_PAGES * crate::pds::xrpc::MAX_FETCH_BODY_BYTES
);

// "Four pages cover the sanitize::limits::MAX_GENERATORS = 256 room cap with
// headroom" - a claim about a number in another file, which has been raised
// once already.
const _: () =
    assert!(state::MAX_ROOM_GENERATOR_PAGES * 100 >= crate::pds::sanitize::limits::MAX_GENERATORS);

// The far detail-normal tile is "much coarser than the near tile so the two
// scales blend": equal or inverted scales are the distance-repetition
// artifact the pair exists to break up.
const _: () =
    assert!(terrain::water::DEFAULT_NORMAL_SCALE_FAR < terrain::water::DEFAULT_NORMAL_SCALE_NEAR);

// A shadow cascade set whose first split sits at or beyond its own maximum
// distance still draws shadows - Bevy's builder does not check the pair -
// but its split comes out flat or backwards, so the first cascade shades
// every depth out to its own bound and the others go all but unused
// (`shadow_reach::tests::an_inverted_pair_is_split_backwards_not_refused`).
// `shadow_reach::reach` keeps the zoomed cuts rising too -
// `the_first_cascade_always_ends_inside_the_reach` - so this holds the rest
// pair it starts from, and the three below hold the premises the zoom rule
// is built on (#1475).
const _: () = assert!(lighting::CASCADE_FIRST_FAR < lighting::CASCADE_MAX_DIST);
// "At rest the cuts are what they always were": the first-cascade share cap
// must not bind at the rest zoom, or the default view's first cascade moves.
const _: () = assert!(
    lighting::CASCADE_FIRST_FAR < lighting::CASCADE_MAX_DIST * lighting::CASCADE_FIRST_SHARE_MAX
);
// A share of 1 or more lets the first cascade reach the last bound, which
// splits flat or backwards (above). 0 or less puts it at or behind the camera, and
// that Bevy's builder does refuse: with more than one cascade it asserts the
// first bound lies past its 0.1 m near bound, and the panic aborts the
// client.
const _: () =
    assert!(lighting::CASCADE_FIRST_SHARE_MAX > 0.0 && lighting::CASCADE_FIRST_SHARE_MAX < 1.0);
// The zoom rule reaches `CASCADE_MAX_DIST - ORBIT_RADIUS` past the player:
// the rest reach has to end beyond the rest camera's own focus.
const _: () = assert!(lighting::CASCADE_MAX_DIST > camera::ORBIT_RADIUS);

// The splat albedo fade (#1320) ramps between its two distances, and
// splat.wgsl switches it off when they are equal or inverted, which quietly
// brings the far-ground cross-hatch back.
const _: () = assert!(terrain::splat::ALBEDO_FADE_NEAR < terrain::splat::ALBEDO_FADE_FAR);
// "The fade has to finish while the fog still shows the ground." A far
// distance at or past VISIBILITY leaves ground the fog still shows with the
// fade unfinished, and the hatch stays there where the fog does not hide it.
const _: () = assert!(terrain::splat::ALBEDO_FADE_FAR < camera::fog::VISIBILITY);

#[cfg(test)]
mod http_client_tests {
    //! #1154: the shared client must actually carry the configuration this
    //! module spends fifty lines describing.

    /// The regression this closes is not a wrong value but a silent one.
    /// `default_client` ended in `builder.build().unwrap_or_default()`, so
    /// any builder failure produced `Client::new()` instead - no request
    /// timeout, no redirect policy, and reqwest's default TLS backend,
    /// which is the OpenSSL path this function exists to steer away from.
    /// Every hardening choice above would have been discarded without a
    /// word. Building must therefore succeed, and it must succeed on the
    /// configuration as written.
    #[test]
    fn the_shared_client_builds_with_its_configuration_intact() {
        // Panics rather than degrading if the builder ever rejects the
        // combination of timeouts, redirect policy and rustls backend.
        let client = super::http::default_client();
        // A second build proves it is not a one-shot: every fetch site
        // calls this per task.
        let again = super::http::default_client();
        drop((client, again));
    }

    /// `use_rustls_tls` is feature-gated in reqwest, so this is really a
    /// compile-time assertion with a runtime shell: if someone resolves
    /// #1154 the other way - dropping `rustls-tls` from the manifest and
    /// accepting OpenSSL - this stops compiling rather than quietly
    /// changing which TLS stack the client speaks.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn rustls_is_compiled_in_and_selectable() {
        let built = reqwest::Client::builder().use_rustls_tls().build();
        assert!(
            built.is_ok(),
            "the rustls backend must remain available to `default_client`"
        );
    }
}
