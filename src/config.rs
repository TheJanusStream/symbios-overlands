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
// Rover (player/ + network.rs)
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
    pub const LATERAL_GRIP: f32 = 6_000.0;
    pub const JUMP_FORCE: f32 = 2_500.0;
    /// Torque strength nudging the chassis back to upright.
    pub const UPRIGHTING_TORQUE: f32 = 800.0;
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

    // --- Sail geometry -------------------------------------------------------
    pub const SAIL_THICKNESS: f32 = 0.05;
    pub const SAIL_SIZE: f32 = 0.8;
    /// Local-space Y offset of the sail panel above the chassis origin.
    pub const SAIL_OFFSET_Y: f32 = 0.7;

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
// Terrain generation (terrain.rs)
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
    /// Resolution of each procedurally generated texture layer (pixels).
    pub const TEXTURE_SIZE: u32 = 512;

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
        /// Screen-space refraction offset strength. Defaults to 0.0 — the
        /// shader reserves the channel for a future depth-prepass wiring.
        pub const DEFAULT_REFRACTION_STRENGTH: f32 = 0.0;
        /// Specular sun-glitter highlight strength.
        pub const DEFAULT_SUN_GLITTER: f32 = 1.8;
        /// sRGB tint applied to wave crests via cheap subsurface scattering.
        pub const DEFAULT_SCATTER_COLOR: [f32; 3] = [0.18, 0.45, 0.42];
        /// Default shore-foam band width (m). Defaults to 0.0 until the
        /// depth-prepass path needed to resolve shoreline depth is wired in.
        pub const DEFAULT_SHORE_FOAM_WIDTH: f32 = 0.0;
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
// Network (network.rs)
// ---------------------------------------------------------------------------
pub mod network {
    /// Broadcast identity to peers every N fixed-update ticks.
    pub const IDENTITY_BROADCAST_INTERVAL_TICKS: u32 = 60;
    /// Expected spacing (seconds) between consecutive Transform broadcasts
    /// from a given peer, used by the jitter buffer to assign synthetic
    /// playout timestamps when WebRTC delivers packets in a burst.  Chosen
    /// slightly smaller than the nominal FixedUpdate tick so the buffer stays
    /// in step with wall clock under normal delivery.
    pub const EXPECTED_BROADCAST_INTERVAL_SECS: f64 = 1.0 / 60.0;
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
    /// Emissive intensity applied to the mast tip of a mutual-follow peer.
    pub const MUTUAL_MAST_EMISSIVE: f32 = 5.0;

    // --- Stationary bandwidth throttling ------------------------------------
    /// Linear speed (m/s) at or below which the rover is considered stationary
    /// and transform broadcasts are throttled to save bandwidth.
    pub const STATIONARY_SPEED_THRESHOLD: f32 = 0.1;
    /// Angular speed (rad/s) at or below which the rover is considered
    /// rotationally at rest.  Both linear and angular thresholds must be met
    /// before throttling kicks in, so a spinning-in-place chassis still
    /// streams smooth rotation updates at full rate.
    pub const STATIONARY_ANGULAR_THRESHOLD: f32 = 0.05;
    /// Only send a transform every N-th tick while stationary.  At 60 fps this
    /// yields ~2 Hz (60 / 30 = 2).
    pub const STATIONARY_BROADCAST_DIVISOR: u32 = 30;
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
}

// ---------------------------------------------------------------------------
// Avatar (avatar.rs)
// ---------------------------------------------------------------------------
pub mod avatar {
    /// User-Agent header sent to the ATProto API.
    pub const USER_AGENT: &str = "SymbiosOverlands/1.0";
}

// ---------------------------------------------------------------------------
// HTTP client defaults (lib.rs, avatar.rs, social.rs, ui/login.rs, ui/room/)
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
}

// ---------------------------------------------------------------------------
// Login UI (ui/login.rs)
// ---------------------------------------------------------------------------
pub mod login {
    /// Default ATProto PDS endpoint.
    pub const DEFAULT_PDS: &str = "https://bsky.social";
    /// Default relay signaller hostname.
    pub const DEFAULT_RELAY_HOST: &str = "37.143.131.78.nip.io";
    pub const DEFAULT_TARGET_DID: &str = "";
}

// ---------------------------------------------------------------------------
// Airship vehicle (player/ + network.rs)
// ---------------------------------------------------------------------------
pub mod airship {
    use super::rover;

    /// Main hull dimensions match the physics chassis.
    pub const HULL_WIDTH: f32 = rover::CHASSIS_X * 2.0; // 1.6 m
    pub const HULL_HEIGHT: f32 = rover::CHASSIS_Y * 2.0; // 0.4 m
    pub const HULL_LENGTH: f32 = rover::CHASSIS_Z * 2.0; // 2.4 m

    /// Lateral distance from centre to each outrigger pontoon.
    pub const PONTOON_SPREAD: f32 = 1.1;
    pub const PONTOON_LENGTH: f32 = 1.8;
    /// Cross-section width of each pontoon (m).
    pub const PONTOON_WIDTH: f32 = 0.22;
    /// Cross-section height of each pontoon (m); keel depth for V-hull shape.
    pub const PONTOON_HEIGHT: f32 = 0.22;

    /// Thin horizontal struts connecting hull to pontoons (cylinder diameter).
    pub const STRUT_THICKNESS: f32 = 0.06;

    /// Depth of the V-hull keel below the deck rim (y = 0 in local mesh space).
    pub const HULL_DEPTH: f32 = 0.5;

    /// Downward offset for struts & pontoons as fraction (0–1) of hull keel depth.
    pub const STRUT_DROP: f32 = 0.0;

    pub const MAST_RADIUS: f32 = 0.04;
    pub const MAST_HEIGHT: f32 = 0.9;
    /// Default 2D offset [X, Z] of the mast position on the deck (m).
    pub const MAST_OFFSET: [f32; 2] = [0.0, 0.0];

    /// Square solar sail side length.
    pub const SAIL_SIZE: f32 = 0.6;
    pub const SAIL_THICKNESS: f32 = 0.03;

    // --- Default material properties (steampunk palette) --------------------
    /// Brass hull, sRGB.
    pub const HULL_COLOR: [f32; 3] = [0.72, 0.50, 0.18];
    /// Dark-bronze pontoons, sRGB.
    pub const PONTOON_COLOR: [f32; 3] = [0.48, 0.30, 0.10];
    /// Copper mast, sRGB.
    pub const MAST_COLOR: [f32; 3] = [0.60, 0.38, 0.18];
    /// Dark-iron struts, sRGB.
    pub const STRUT_COLOR: [f32; 3] = [0.35, 0.28, 0.22];
    pub const METALLIC: f32 = 0.65;
    pub const ROUGHNESS: f32 = 0.55;
}

// ---------------------------------------------------------------------------
// UI panels (ui/chat.rs, ui/diagnostics.rs, ui/avatar.rs, ui/room/, ui/login.rs)
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
        /// Author label colour [R, G, B].
        pub const AUTHOR_COLOR: [u8; 3] = [100, 180, 255];
        /// Default egui window geometry.
        pub const WINDOW_DEFAULT_WIDTH: f32 = 380.0;
        pub const WINDOW_DEFAULT_HEIGHT: f32 = 400.0;
        /// Rightmost slot in the top-row layout — the only panel that
        /// starts open, so it needs the space to render its scroll area
        /// and input field at first frame.
        pub const WINDOW_DEFAULT_POS: [f32; 2] = [960.0, 10.0];
    }

    pub mod diagnostics {
        pub const WINDOW_DEFAULT_WIDTH: f32 = 280.0;
        pub const WINDOW_DEFAULT_HEIGHT: f32 = 480.0;
        pub const WINDOW_DEFAULT_POS: [f32; 2] = [10.0, 10.0];
    }

    pub mod people {
        pub const WINDOW_DEFAULT_WIDTH: f32 = 280.0;
        pub const WINDOW_DEFAULT_HEIGHT: f32 = 300.0;
        pub const WINDOW_DEFAULT_POS: [f32; 2] = [770.0, 10.0];
    }

    pub mod airship {
        pub const WINDOW_DEFAULT_WIDTH: f32 = 320.0;
        pub const WINDOW_DEFAULT_POS: [f32; 2] = [200.0, 10.0];
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
    }
}
