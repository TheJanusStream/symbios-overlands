//! Cross-theme "bring-it-to-life" emitters for the civic prop kit, built on
//! the shared [`Emitter`] recipe struct. Same contract as the per-theme
//! kits (`cyberpunk::fx`, `post_apoc::fx`): each function returns a
//! positioned [`Generator`] carrying a `GeneratorKind::ParticleSystem`, so
//! it drops straight into an [`assemble`](super::assemble) list alongside
//! the prop's primitives.
//!
//! Each effect is deliberately split into *layers* rather than one
//! do-everything emitter. A single emitter has one colour ramp, one size
//! ramp and one lifetime band, which is why prop fire built that way reads
//! as a fuzzy orange smear: real flame is a small fast white-hot core
//! inside a slower deep-orange body, shedding sparse embers, handing off to
//! near-black soot that only pales as it spreads. A fountain jet splits the
//! same way — a hard fast column plus the soft mist hanging around its
//! crown. Layering costs an extra emitter entity or two and buys all of
//! that for free.

use crate::catalogue::items::fx::Emitter;
use crate::pds::{
    EmitterShape, Fp, Fp3, Fp64, Generator, ParticleBlendMode, SovereignFlameConfig,
    SovereignPuffConfig, SovereignSoftDiscConfig, SovereignSparkConfig, SovereignTextureConfig,
};

/// The white-hot inner cone of an open fire: short-lived, fast, narrow, and
/// bright enough to read as the light source the outer body is lit by.
/// Place it just above the fuel bed — the particles carry themselves clear
/// of a container's rim on their own speed.
pub(super) fn flame_core(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.2),
            height: Fp(0.14),
        },
        rate: 55.0,
        burst: 0,
        max: 120,
        life: (0.4, 0.85),
        speed: (0.6, 1.15),
        // Buoyant: hot gas falls *up*. Drag then bleeds the lateral
        // component fast so the column stays inside a barrel's mouth.
        gravity: -0.3,
        accel: [0.0, 0.0, 0.0],
        drag: 0.9,
        size: (0.24, 0.03),
        start_color: [1.0, 0.93, 0.62, 1.0],
        // Cools out through the kit's shared flame hue, so the core's fade
        // lands exactly on the colour the body is burning at.
        end_color: [super::FIRE[0], super::FIRE[1], super::FIRE[2], 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::Flame(SovereignFlameConfig {
            seed: (seed ^ 0x00C0_47E0) as u32,
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// The deep-orange outer body the [`flame_core`] sits inside: wider, slower,
/// longer-lived, fading to the dull red of a flame tip going out. Sits a
/// little *below* the core so the two overlap instead of stacking.
pub(super) fn flame_body(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.32),
            height: Fp(0.18),
        },
        rate: 42.0,
        burst: 0,
        max: 110,
        life: (0.6, 1.15),
        speed: (0.4, 0.85),
        gravity: -0.2,
        accel: [0.0, 0.0, 0.0],
        drag: 1.1,
        size: (0.42, 0.06),
        start_color: [1.0, 0.55, 0.12, 0.95],
        end_color: [0.55, 0.09, 0.03, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::Flame(SovereignFlameConfig {
            seed: (seed ^ 0x00B0_D1E5) as u32,
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// Sparse embers thrown off the top of the flame — few, small, long-lived,
/// wandering. The rate is deliberately low: embers read as *events*, and a
/// steady stream of them looks like a sparkler, not a fire.
pub(super) fn embers(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.45),
            height: Fp(0.2),
        },
        rate: 5.0,
        burst: 0,
        max: 30,
        life: (1.1, 2.4),
        speed: (0.8, 1.7),
        gravity: -0.22,
        accel: [0.06, 0.0, 0.03],
        drag: 0.7,
        size: (0.045, 0.0),
        start_color: [1.0, 0.78, 0.34, 1.0],
        end_color: [0.9, 0.16, 0.03, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::Spark(SovereignSparkConfig {
            seed: (seed ^ 0x00E3_B205) as u32,
            points: 4,
            color_core: Fp3([1.0, 0.95, 0.75]),
            color_tip: Fp3([1.0, 0.45, 0.10]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// A fountain jet: a tight fast column of droplets thrown straight up under
/// *positive* gravity, so the arc and the fall-back are simulated rather
/// than sculpted. Place it at the nozzle; lifetimes are tuned so droplets
/// wink out around the height they'd hit the catch basin, since the shared
/// [`Emitter`] runs with terrain/water collision off and un-killed droplets
/// would otherwise rain on through the plinth and into the ground.
pub(super) fn water_jet(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.22),
            height: Fp(0.06),
        },
        rate: 165.0,
        burst: 0,
        max: 300,
        life: (0.8, 1.25),
        speed: (3.8, 4.6),
        // Real gravity — apex ≈ 0.9 m above the nozzle at these speeds,
        // and the lifetimes above land droplets back around the catch bowl.
        gravity: 1.0,
        accel: [0.0, 0.0, 0.0],
        // Droplets this size barely feel air; anything more and the column
        // stalls into a hover instead of arcing over.
        drag: 0.05,
        size: (0.13, 0.07),
        start_color: [0.88, 0.95, 1.0, 1.0],
        end_color: [0.48, 0.74, 0.9, 0.0],
        // Alpha, not additive: aerated water is opaque white, and additive
        // over pale marble blows straight out to a featureless glow.
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::SoftDisc(SovereignSoftDiscConfig {
            seed: (seed ^ 0x00A1_7E40) as u32,
            // A 2×2 sheet of per-cell-seeded droplets; `RandomFrame` deals
            // one per particle, so the column isn't 300 copies of one blob.
            variant_rows: 2,
            variant_cols: 2,
            color_core: Fp3([1.0, 1.0, 1.0]),
            color_halo: Fp3(super::WATER_BLUE),
            core_radius: Fp64(0.55),
            falloff: Fp64(1.4),
            ellipticity: Fp64(0.35),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// The fine mist hanging around a jet's crown: slow, soft, growing as it
/// dissipates, settling under a fraction of gravity. This is what sells a
/// jet as water rather than a blue rod — the hard column alone reads as
/// plastic.
pub(super) fn water_mist(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Sphere { radius: Fp(0.28) },
        rate: 26.0,
        burst: 0,
        max: 80,
        life: (0.7, 1.5),
        speed: (0.15, 0.55),
        gravity: 0.3,
        accel: [0.0, 0.0, 0.0],
        drag: 1.3,
        size: (0.14, 0.4),
        start_color: [0.92, 0.96, 1.0, 0.45],
        end_color: [0.72, 0.85, 0.94, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::SoftDisc(SovereignSoftDiscConfig {
            seed: (seed ^ 0x0031_5700) as u32,
            color_core: Fp3([1.0, 1.0, 1.0]),
            color_halo: Fp3([0.8, 0.9, 1.0]),
            core_radius: Fp64(0.2),
            falloff: Fp64(2.4),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// Sooty near-source smoke: dark, dense, still moving fast, only starting to
/// spread. Belongs just above the flame tips, where combustion has stopped
/// but the column hasn't cooled.
pub(super) fn smoke_soot(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.3),
            height: Fp(0.25),
        },
        rate: 14.0,
        burst: 0,
        max: 60,
        life: (1.6, 2.8),
        speed: (0.5, 1.0),
        gravity: -0.06,
        accel: [0.06, 0.3, 0.02],
        drag: 0.5,
        size: (0.34, 1.1),
        start_color: [0.18, 0.16, 0.15, 0.5],
        end_color: [0.34, 0.32, 0.31, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0050_0074) as u32,
            color_base: Fp3([0.30, 0.28, 0.27]),
            color_shadow: Fp3([0.12, 0.11, 0.10]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// The pale upper plume the [`smoke_soot`] column becomes: slow, wide,
/// long-lived, leaning off downwind and thinning to nothing. Place it well
/// above the soot so the two read as one continuous column that lightens
/// with height.
pub(super) fn smoke_plume(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        shape: EmitterShape::Cone {
            half_angle: Fp(0.4),
            height: Fp(0.4),
        },
        rate: 7.0,
        burst: 0,
        max: 48,
        life: (3.0, 5.5),
        speed: (0.25, 0.7),
        gravity: -0.03,
        accel: [0.14, 0.2, 0.04],
        drag: 0.6,
        size: (0.8, 2.4),
        start_color: [0.36, 0.35, 0.34, 0.3],
        end_color: [0.5, 0.49, 0.48, 0.0],
        blend: ParticleBlendMode::Alpha,
        sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
            seed: (seed ^ 0x0050_1075) as u32,
            color_base: Fp3([0.52, 0.51, 0.50]),
            color_shadow: Fp3([0.30, 0.29, 0.28]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}
