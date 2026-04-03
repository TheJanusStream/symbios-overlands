//! Centralised configuration constants for Symbios Overlands.
//!
//! All tuneable values live here so they are easy to locate and adjust without
//! hunting through individual modules.  Modules mirror the source file that
//! consumes each constant group.

// ---------------------------------------------------------------------------
// Lighting (main.rs)
// ---------------------------------------------------------------------------
pub mod lighting {
    /// Illuminance of the sun-like directional light (lux).
    pub const ILLUMINANCE: f32 = 15_000.0;
    /// Brightness of the scene-wide ambient light.
    pub const AMBIENT_BRIGHTNESS: f32 = 400.0;
    /// World-space position of the directional light source.
    pub const LIGHT_POS: [f32; 3] = [50.0, 100.0, 50.0];
}

// ---------------------------------------------------------------------------
// Rover (rover.rs + network.rs)
// ---------------------------------------------------------------------------
pub mod rover {
    // --- Suspension (Hooke's law + damping) ----------------------------------
    pub const SUSPENSION_REST_LENGTH: f32 = 0.8;
    pub const SUSPENSION_STIFFNESS: f32 = 4_200.0;
    pub const SUSPENSION_DAMPING: f32 = 175.0;
    /// Ray-cast length = rest length + this overshoot past the contact plane.
    pub const RAY_MAX_DIST: f32 = SUSPENSION_REST_LENGTH + 1.5;

    // --- Drive ---------------------------------------------------------------
    pub const DRIVE_FORCE: f32 = 3_000.0;
    pub const TURN_TORQUE: f32 = 1_800.0;
    pub const LATERAL_GRIP: f32 = 6_000.0;
    pub const JUMP_FORCE: f32 = 2_500.0;
    /// Torque strength nudging the chassis back to upright.
    pub const UPRIGHTING_TORQUE: f32 = 800.0;
    /// World-space Y below which the rover is considered "fallen off" and respawned.
    pub const FALL_Y_THRESHOLD: f32 = -20.0;

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
        pub const COLOR: [f32; 4] = [0.52, 0.62, 0.74, 1.0];
        /// ExponentialSquared density.  At 1 024 m terrain width this gives
        /// ~5 % fog at 300 m, ~27 % at 700 m, and ~47 % at 1 000 m.
        pub const DENSITY: f32 = 0.008;
    }
}

// ---------------------------------------------------------------------------
// Terrain generation (terrain.rs)
// ---------------------------------------------------------------------------
pub mod terrain {
    pub const SEED: u64 = 42;
    pub const GRID_SIZE: usize = 513;
    pub const CELL_SCALE: f32 = 2.0;
    pub const HEIGHT_SCALE: f32 = 5.0;
    /// Number of hydraulic erosion droplets.
    pub const EROSION_DROPS: u32 = 30_000;
    /// How many times the tiling textures repeat across the terrain.
    pub const TILE_SCALE: f32 = 256.0;
    /// Resolution of each procedurally generated texture layer (pixels).
    pub const TEXTURE_SIZE: u32 = 512;

    // --- Voronoi terracing ---------------------------------------------------
    pub mod voronoi {
        /// Number of Voronoi seed points; more seeds → smaller plateaus.
        pub const NUM_SEEDS: usize = 1500;
        /// Number of discrete terrace height levels.
        pub const NUM_TERRACES: usize = 50;
    }

    // --- Thermal erosion -----------------------------------------------------
    pub mod thermal {
        pub const ITERATIONS: u32 = 50;
        pub const TALUS_ANGLE: f32 = 0.6;
    }

    // --- Splat material ------------------------------------------------------
    pub mod splat {
        /// Base colour of the terrain material before textures are uploaded.
        pub const PLACEHOLDER_COLOR: [f32; 3] = [0.35, 0.55, 0.25];
        pub const PLACEHOLDER_ROUGHNESS: f32 = 0.9;
        /// Perceptual roughness once real splat textures are applied.
        pub const MATERIAL_ROUGHNESS: f32 = 0.85;
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
        pub const BLEND: f32 = 4.0;
    }

    // --- Splat layer: Dirt (layer 1, uses GroundConfig::default()) -----------
    pub mod dirt {
        // Splat rule
        pub const ALT_MIN_FACTOR: f32 = 0.30;
        pub const ALT_MAX_FACTOR: f32 = 0.65;
        pub const SLOPE_MAX: f32 = 0.55;
        pub const BLEND: f32 = 2.0;
    }

    // --- Splat layer: Rock (layer 2, uses RockConfig::default()) -------------
    pub mod rock {
        // Splat rule
        pub const SLOPE_MIN: f32 = 0.25;
        pub const BLEND: f32 = 3.0;
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
    /// Broadcast identity to peers every N physics ticks.
    pub const IDENTITY_BROADCAST_INTERVAL_TICKS: u32 = 60;
}

// ---------------------------------------------------------------------------
// App state (state.rs)
// ---------------------------------------------------------------------------
pub mod state {
    /// Maximum number of entries retained in the rolling diagnostics log.
    pub const MAX_DIAGNOSTICS_ENTRIES: usize = 200;
}

// ---------------------------------------------------------------------------
// Avatar (avatar.rs)
// ---------------------------------------------------------------------------
pub mod avatar {
    /// User-Agent header sent to the ATProto API.
    pub const USER_AGENT: &str = "SymbiosOverlands/1.0";
}

// ---------------------------------------------------------------------------
// Login UI (ui/login.rs)
// ---------------------------------------------------------------------------
pub mod login {
    /// Default ATProto PDS endpoint.
    pub const DEFAULT_PDS: &str = "https://bsky.social";
    /// Default relay signaller hostname.
    pub const DEFAULT_RELAY_HOST: &str = "37.143.131.78.nip.io";
    pub const DEFAULT_HANDLE: &str = "";
    pub const DEFAULT_PASSWORD: &str = "";
}

// ---------------------------------------------------------------------------
// Airship vehicle (rover.rs + network.rs)
// ---------------------------------------------------------------------------
pub mod airship {
    use super::rover;

    /// Main hull dimensions match the physics chassis.
    pub const HULL_WIDTH: f32 = rover::CHASSIS_X * 2.0;   // 1.6 m
    pub const HULL_HEIGHT: f32 = rover::CHASSIS_Y * 2.0;  // 0.4 m
    pub const HULL_LENGTH: f32 = rover::CHASSIS_Z * 2.0;  // 2.4 m

    /// Lateral distance from centre to each outrigger pontoon.
    pub const PONTOON_SPREAD: f32 = 1.1;
    pub const PONTOON_LENGTH: f32 = 1.8;
    /// Square cross-section side length of each pontoon.
    pub const PONTOON_SIZE: f32 = 0.22;

    /// Thin horizontal struts connecting hull to pontoons.
    pub const STRUT_THICKNESS: f32 = 0.06;

    pub const MAST_RADIUS: f32 = 0.04;
    pub const MAST_HEIGHT: f32 = 0.9;

    /// Square solar sail side length.
    pub const SAIL_SIZE: f32 = 0.9;
    pub const SAIL_THICKNESS: f32 = 0.03;

    // --- Default material properties (steampunk palette) --------------------
    /// Brass hull [sRGB].
    pub const HULL_COLOR: [f32; 3] = [0.72, 0.50, 0.18];
    /// Dark-bronze pontoons [sRGB].
    pub const PONTOON_COLOR: [f32; 3] = [0.48, 0.30, 0.10];
    /// Copper mast [sRGB].
    pub const MAST_COLOR: [f32; 3] = [0.60, 0.38, 0.18];
    pub const METALLIC: f32 = 0.65;
    pub const ROUGHNESS: f32 = 0.55;
}

// ---------------------------------------------------------------------------
// UI panels (ui/chat.rs, ui/diagnostics.rs, ui/airship.rs)
// ---------------------------------------------------------------------------
pub mod ui {
    pub mod chat {
        /// Height reserved below the scroll area for the input row.
        pub const INPUT_RESERVE_HEIGHT: f32 = 40.0;
        /// Minimum height of the message scroll area.
        pub const SCROLL_MIN_HEIGHT: f32 = 60.0;
        /// Author label colour [R, G, B].
        pub const AUTHOR_COLOR: [u8; 3] = [100, 180, 255];
        /// Default egui window geometry.
        pub const WINDOW_DEFAULT_WIDTH: f32 = 380.0;
        pub const WINDOW_DEFAULT_HEIGHT: f32 = 400.0;
        /// Default top-left position [x, y] (right side of a typical 1080p window).
        pub const WINDOW_DEFAULT_POS: [f32; 2] = [820.0, 60.0];
    }

    pub mod diagnostics {
        pub const WINDOW_DEFAULT_WIDTH: f32 = 280.0;
        pub const WINDOW_DEFAULT_HEIGHT: f32 = 480.0;
        pub const WINDOW_DEFAULT_POS: [f32; 2] = [10.0, 60.0];
    }

    pub mod airship {
        pub const WINDOW_DEFAULT_WIDTH: f32 = 320.0;
        pub const WINDOW_DEFAULT_POS: [f32; 2] = [10.0, 420.0];
    }
}
