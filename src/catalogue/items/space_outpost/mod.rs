//! Space-Outpost-theme catalogue structures — a pressurised off-world colony
//! of habitat domes and support modules under a thin atmosphere.
//!
//! Two prosperity registers share one frontier-colony identity: the
//! established ([`OUTPOST_BAND`]) base (a habitat dome, a solar array, a comms
//! dish, a landing pad, a hydroponics module, a rover, cargo crates, a beacon
//! and an airlock) and the destitute ([`OUTPOST_POOR`]) wreck kit (a crash
//! shelter, a collapsed solar wreck, a scrap canister).
//!
//! Surfaces use the real procedural generators rather than flat colour: white
//! brushed [`hull`] plating, dark structural [`steel`], lit [`glass`]
//! viewports, glossy dark [`pv`] arrays, ceramic [`concrete`] pads and matte
//! [`painted`] hazard markings. The dome's viewports and interior glow, the
//! beacons and the grow-lights shine over a reactor-hum and comms-static bed
//! from [`fx`]. The theme's thin-atmosphere accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod airlock;
pub mod beacon;
pub mod cargo_crate;
pub mod comms_dish;
pub mod gateway;
pub mod habitat_dome;
pub mod hydroponics;
pub mod landing_pad;
pub mod rover;
pub mod solar_array;
// Poor (wreck) variants — the prosperity-Poor end of the theme.
pub mod crash_shelter;
pub mod scrap_canister;
pub mod solar_wreck;

pub mod fx;

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use super::util::{tile, tiles_per_metre};
use bevy_symbios_texture::metal::MetalStyle;

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, id_quat, prim, quat_mul, quat_x, quat_y, solid, torus,
    with_cut,
};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignConcreteConfig, SovereignMaterialSettings,
    SovereignMetalConfig, SovereignTextureConfig, SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established base — a crewed outpost reads as
/// a Modest-to-Rich colony. The poor end of the theme is the separate wreck
/// kit ([`crash_shelter`], …), tagged `Poor`, so a destitute space room grows
/// the derelict crash site instead.
pub(super) const OUTPOST_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the wreck kit — the destitute end of the theme, never
/// picked for a modest or affluent space room.
pub(super) const OUTPOST_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// White brushed hull plating — habitat shells, modules, the rover body.
pub(super) fn hull(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.35),
        metallic: Fp(0.7),
        uv_scale: tiles_per_metre(tile::METAL),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.4, 0.36, 0.32]),
            seam_count: Fp64(4.0),
            roughness: Fp64(0.35),
            metallic: Fp(0.7),
            rust_level: Fp64(0.02),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Dark structural steel — frames, masts, legs, dish mounts, wheels.
pub(super) fn steel(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.45),
        metallic: Fp(0.85),
        uv_scale: tiles_per_metre(tile::METAL),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.3, 0.22, 0.16]),
            roughness: Fp64(0.45),
            metallic: Fp(0.85),
            rust_level: Fp64(0.08),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Lit viewport glass — habitat windows, hydroponics glazing, hatches. A
/// faint inner glow (`glow`) so the ports read as lit rather than black.
pub(super) fn glass(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.12),
        metallic: Fp(0.4),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 2,
            panes_y: 1,
            glass_opacity: Fp64(0.35),
            grime_level: Fp64(0.05),
            color_frame: Fp3([0.6, 0.62, 0.66]),
            ..Default::default()
        }),
    }
}

/// Glossy dark photovoltaic — the solar arrays.
pub(super) fn pv(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.12),
        metallic: Fp(0.6),
        uv_scale: tiles_per_metre(tile::METAL),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.1, 0.12, 0.2]),
            seam_count: Fp64(8.0),
            roughness: Fp64(0.12),
            metallic: Fp(0.6),
            rust_level: Fp64(0.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Ceramic concrete — the landing pad and footings.
pub(super) fn concrete(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.8),
        uv_scale: tiles_per_metre(tile::CONCRETE),
        texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
            color_base: Fp3(color),
            formwork_lines: Fp64(3.0),
            formwork_depth: Fp64(0.08),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Flat matte paint — hazard markings, pad chevrons, crate stencils. A plain
/// coloured surface with no procedural texture.
pub(super) fn painted(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.6),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

// Hull + structure palette.
pub(super) const HULL_WHITE: [f32; 3] = [0.84, 0.85, 0.87];
pub(super) const HULL_PANEL: [f32; 3] = [0.70, 0.72, 0.76];
pub(super) const STEEL_DARK: [f32; 3] = [0.34, 0.36, 0.40];
pub(super) const PV_BLUE: [f32; 3] = [0.10, 0.14, 0.30];
pub(super) const PAD_GREY: [f32; 3] = [0.40, 0.40, 0.42];
pub(super) const HAZARD_YELLOW: [f32; 3] = [0.86, 0.72, 0.10];
pub(super) const SCORCH: [f32; 3] = [0.32, 0.28, 0.26];

// Glass + emissive palette.
pub(super) const GLASS_CYAN: [f32; 3] = [0.42, 0.66, 0.74];
pub(super) const VIEWPORT_LIT: [f32; 3] = [0.6, 0.95, 1.0];
pub(super) const INTERIOR_WARM: [f32; 3] = [1.0, 0.92, 0.78];
// Deep-saturated so the off-channels stay low and bloom can't lift the glow
// to a coral/pink wash — it holds a true red (the fantasy deep-saturate rule).
pub(super) const BEACON_RED: [f32; 3] = [1.0, 0.09, 0.07];
pub(super) const GROW_PINK: [f32; 3] = [1.0, 0.30, 0.74];
/// Deep-saturated status-LED green — combiner boxes, instrument panels.
pub(super) const STATUS_GREEN: [f32; 3] = [0.22, 1.0, 0.42];

// ---------------------------------------------------------------------------
// Theme-signature construction helpers (`pub(super)`, theme-local like
// steampunk's `cog()` / nordic's `gable_roof`). Built and validated on a hero
// before rollout, then reused across the kit.
// ---------------------------------------------------------------------------

/// Geodesic rib cage for a pressure dome — `meridians` upright semicircular
/// arcs fanned around the polar axis plus two latitude hoops, all sitting on a
/// hemisphere of `radius` centred at `center`. Author the glass shell a touch
/// smaller (≈ `radius - 0.08`) so the ribs stand proud. Turns a smooth glass
/// snowglobe into a paneled habitat dome — the Space-Outpost silhouette
/// signature.
pub(super) fn dome_ribs(
    center: [f32; 3],
    radius: f32,
    meridians: u32,
    mat: SovereignMaterialSettings,
) -> Vec<Generator> {
    let minor = 0.06_f32;
    let mut out = Vec::new();
    // Meridian arcs: upright semicircles fanned over [0, PI) — each arc runs
    // base-to-base over the apex, so n arcs read as 2n ribs.
    for k in 0..meridians {
        let theta = k as f32 / meridians as f32 * PI;
        out.push(prim(
            with_cut(
                torus(minor, radius, mat.clone()),
                [0.0, 0.5],
                [0.0, 1.0],
                0.0,
            ),
            center,
            quat_mul(quat_y(theta), quat_x(-FRAC_PI_2)),
        ));
    }
    // Latitude hoops part-way up the dome.
    for frac in [0.4_f32, 0.72] {
        let y = center[1] + radius * frac;
        let hoop_r = (radius * radius - (radius * frac).powi(2)).sqrt();
        out.push(prim(
            torus(minor, hoop_r, mat.clone()),
            [center[0], y, center[2]],
            id_quat(),
        ));
    }
    out
}

/// A framed photovoltaic panel — a dark PV cell field in a steel perimeter
/// frame with cross ribs dividing it into cells, so it reads as a real solar
/// panel rather than a flat slab. Lies in its local XZ plane (thin in Y, broad
/// faces ±Y, the lit cell face up); the caller tilts/positions it. Returned as
/// one subtree (the cell field is the local root, `id_quat`) so dropping it in
/// tilted as a child is rotation-safe.
pub(super) fn pv_panel(
    width: f32,
    length: f32,
    cell: SovereignMaterialSettings,
    frame: SovereignMaterialSettings,
) -> Generator {
    let t = 0.06_f32; // panel thickness
    let fr = 0.07_f32; // frame bar cross-section
    let yf = t * 0.5 + fr * 0.4; // proud of the +Y cell face
    let mut panel = prim(
        solid(cuboid_tapered([width, t, length], 0.0, cell)),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Perimeter frame (edges oversail the cell field).
    for sz in [-1.0_f32, 1.0] {
        panel.children.push(prim(
            cuboid_tapered([width + fr, fr, fr], 0.0, frame.clone()),
            [0.0, yf, sz * length * 0.5],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        panel.children.push(prim(
            cuboid_tapered([fr, fr, length + fr], 0.0, frame.clone()),
            [sx * width * 0.5, yf, 0.0],
            id_quat(),
        ));
    }
    // Cell-division ribs across the face — two along X (three columns), one
    // along Z (two rows).
    for fx in [-1.0_f32 / 3.0, 1.0 / 3.0] {
        panel.children.push(prim(
            cuboid_tapered([0.04, fr * 0.7, length], 0.0, frame.clone()),
            [fx * width, yf - 0.01, 0.0],
            id_quat(),
        ));
    }
    panel.children.push(prim(
        cuboid_tapered([width, fr * 0.7, 0.04], 0.0, frame),
        [0.0, yf - 0.01, 0.0],
        id_quat(),
    ));
    panel
}

/// A round pressure hatch on a wall facing ±Z (`zsign`, −1.0 = the −Z hero
/// front): a recessed door plate in a bolted rim ring with locking lugs, a
/// central lit port and a grab handle. Returns a Vec to splice into an
/// assemble list — every piece is a non-root prim, so the `quat_x` facing
/// rotation is safe. `port` should be a `glow` material so the window reads as
/// lit on the flat door face.
pub(super) fn pressure_hatch(
    center: [f32; 3],
    radius: f32,
    zsign: f32,
    door: SovereignMaterialSettings,
    rim: SovereignMaterialSettings,
    port: SovereignMaterialSettings,
) -> Vec<Generator> {
    let [cx, cy, cz] = center;
    let face = quat_x(FRAC_PI_2); // cylinder/disc axis Y -> Z
    let mut out = vec![
        // Bolted rim ring standing in the wall plane.
        prim(torus(0.09, radius, rim.clone()), [cx, cy, cz], face),
        // Recessed door plate.
        prim(
            solid(cylinder_tapered(radius - 0.06, 0.16, 18, 0.0, door)),
            [cx, cy, cz - zsign * 0.04],
            face,
        ),
    ];
    // Locking lugs around the rim.
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU;
        out.push(prim(
            cuboid_tapered([0.13, 0.13, 0.1], 0.0, rim.clone()),
            [
                cx + a.cos() * radius,
                cy + a.sin() * radius,
                cz + zsign * 0.05,
            ],
            id_quat(),
        ));
    }
    // Central lit port, proud of the door.
    out.push(prim(
        cylinder_tapered(radius * 0.34, 0.06, 14, 0.0, port),
        [cx, cy, cz + zsign * 0.13],
        face,
    ));
    // Grab handle across the port.
    out.push(prim(
        solid(cuboid_tapered([radius * 0.72, 0.08, 0.08], 0.0, rim)),
        [cx, cy, cz + zsign * 0.17],
        id_quat(),
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (wreck) variants must build clean trees the sanitiser
    /// leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &crash_shelter::CrashShelter,
            &solar_wreck::SolarWreck,
            &scrap_canister::ScrapCanister,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The habitat dome is the kit's lit hero — it must keep its emissive
    /// viewports and interior glow so escalation's broken-emissive ruin pass
    /// has light to snuff.
    #[test]
    fn habitat_dome_keeps_its_glow() {
        assert!(
            crate::catalogue::items::util::has_emissive(&habitat_dome::HabitatDome.build("")),
            "habitat dome lost its emissive viewports / interior glow"
        );
    }
}
