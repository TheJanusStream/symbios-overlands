//! Industrial-Park-theme catalogue structures — a steel-and-concrete works
//! under a grey haze.
//!
//! Two prosperity registers share one identity: the established
//! ([`INDUSTRIAL_BAND`]) working kit (factory, cooling tower, loading dock,
//! tank farm, shipping containers, pipe run, pallet stack, floodlight) and
//! the destitute ([`INDUSTRIAL_POOR`]) derelict kit (derelict shed, rusted
//! tank, scrap heap).
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! ribbed [`cladding`] and [`tank_steel`] metal, board-formed [`concrete`],
//! red [`brick`], [`glass`] windows, and heavily corroded [`rust`]. The
//! smokestack smokes, the cooling tower billows steam, floodlights glare,
//! and machinery hums under a steam hiss — all from [`fx`]. The theme's grey
//! haze accent lives in [`crate::seeded_defaults::room::accent`].

pub mod cooling_tower;
pub mod factory;
pub mod floodlight;
pub mod loading_dock;
pub mod pallet_stack;
pub mod pipe_run;
pub mod shipping_containers;
pub mod tank_farm;
// Poor (derelict) variants — the prosperity-Poor end of the theme.
pub mod derelict_shed;
pub mod rusted_tank;
pub mod scrap_heap;

pub mod fx;

use std::f32::consts::PI;

use bevy_symbios_texture::metal::MetalStyle;

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_mul, quat_x, quat_z, solid, torus,
};
use crate::pds::{
    Fp, Fp3, Fp4, Fp64, Generator, SovereignBrickConfig, SovereignConcreteConfig,
    SovereignCorrugatedConfig, SovereignMaterialSettings, SovereignMetalConfig,
    SovereignPlankConfig, SovereignTextureConfig, SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established works kit — clad sheds and
/// painted tanks read as a Modest-to-Rich industrial estate. The poor end is
/// the separate derelict kit ([`derelict_shed`], …), tagged `Poor`.
pub(super) const INDUSTRIAL_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the derelict kit — the destitute end of the theme,
/// never picked for a modest or affluent room.
pub(super) const INDUSTRIAL_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Ribbed corrugated cladding — the skin of factory sheds, dock walls, and
/// shipping containers.
pub(super) fn cladding(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.6),
        metallic: Fp(0.7),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Corrugated(SovereignCorrugatedConfig {
            color_metal: Fp3(color),
            color_rust: Fp3([0.42, 0.24, 0.12]),
            ridges: Fp64(16.0),
            ridge_depth: Fp64(0.9),
            roughness: Fp64(0.55),
            metallic: Fp(0.7),
            rust_level: Fp64(0.12),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Board-formed concrete — cooling towers, dock aprons, plinths, footings.
pub(super) fn concrete(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
            color_base: Fp3(color),
            formwork_lines: Fp64(5.0),
            formwork_depth: Fp64(0.12),
            pit_density: Fp64(0.12),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Smooth painted steel — storage tanks, pipes, gantries, the smokestack.
pub(super) fn tank_steel(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.4),
        metallic: Fp(0.85),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.4, 0.26, 0.14]),
            roughness: Fp64(0.4),
            metallic: Fp(0.85),
            rust_level: Fp64(0.1),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Sooty red brick — the older factory block and chimney.
pub(super) fn brick(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Brick(SovereignBrickConfig {
            color_brick: Fp3(color),
            color_mortar: Fp3([0.45, 0.43, 0.40]),
            scale: Fp64(5.0),
            cell_variance: Fp64(0.2),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Grimy industrial glazing (`glow` lights it from within).
pub(super) fn glass(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.3),
        metallic: Fp(0.3),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 4,
            panes_y: 3,
            glass_opacity: Fp64(0.5),
            grime_level: Fp64(0.35),
            color_frame: Fp3([0.3, 0.31, 0.32]),
            ..Default::default()
        }),
    }
}

/// Rough timber — wooden pallets and crates in the yard.
pub(super) fn timber(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.2, color[1] * 1.2, color[2] * 1.15]),
            color_wood_dark: Fp3([color[0] * 0.65, color[1] * 0.65, color[2] * 0.6]),
            plank_count: Fp64(4.0),
            knot_density: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Heavily corroded steel — the derelict kit and rusted fittings.
pub(super) fn rust(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        metallic: Fp(0.4),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Corrugated(SovereignCorrugatedConfig {
            color_metal: Fp3(color),
            color_rust: Fp3([0.46, 0.26, 0.12]),
            ridges: Fp64(14.0),
            ridge_depth: Fp64(1.0),
            roughness: Fp64(0.9),
            metallic: Fp(0.4),
            rust_level: Fp64(0.6),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Shared industrial construction helpers
// ---------------------------------------------------------------------------

/// Round hoop bands wrapping a cylindrical tank. The kit used square cuboid
/// rings whose corners jut ≈40 % past the tank wall (a box of half-extent `r`
/// reaches `r·√2` at the corner); a `torus` rides the shaft cleanly. `n`
/// bands sit evenly up `h`, each a hair proud of `r` so it never z-fights the
/// wall.
pub(super) fn tank_hoops(
    cx: f32,
    cz: f32,
    base_y: f32,
    r: f32,
    h: f32,
    n: u32,
    mat: SovereignMaterialSettings,
) -> Vec<Generator> {
    (1..=n)
        .map(|k| {
            let f = k as f32 / (n as f32 + 1.0);
            prim(
                torus(0.07, r + 0.05, mat.clone()),
                [cx, base_y + h * f, cz],
                id_quat(),
            )
        })
        .collect()
}

/// A spoked hand-wheel valve — an outer rim `torus`, a stubby hub axle, and
/// three diameter spoke bars crossing it. Authored flat in its local XZ plane
/// (axle along Y); `rot` stands it up (`quat_x(FRAC_PI_2)` faces it ±Z on a
/// riser). One positioned subtree → drop into an [`assemble`](crate::catalogue::items::util::assemble) list (the spokes
/// ride its rotation, so the rotated-root rule never applies). A bare `torus`
/// reads as a washer; the spokes make it a wheel.
pub(super) fn valve_wheel(
    center: [f32; 3],
    rot: Fp4,
    radius: f32,
    mat: SovereignMaterialSettings,
) -> Generator {
    use crate::catalogue::items::util::quat_y;
    let mut wheel = prim(torus(radius * 0.13, radius, mat.clone()), center, rot);
    // Stubby hub axle along the wheel axis.
    wheel.children.push(prim(
        solid(cylinder_tapered(
            radius * 0.24,
            radius * 0.5,
            12,
            0.0,
            mat.clone(),
        )),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    // Three diameter spokes in the wheel plane.
    for i in 0..3 {
        let a = i as f32 / 3.0 * PI;
        wheel.children.push(prim(
            cuboid_tapered(
                [radius * 1.94, radius * 0.09, radius * 0.09],
                0.0,
                mat.clone(),
            ),
            [0.0, 0.0, 0.0],
            quat_y(a),
        ));
    }
    wheel
}

/// A braced steel lattice mast — four corner legs leaning slightly inward as
/// they rise, ringed by horizontal bands and crossed by zig-zag diagonals on
/// every face. Returns the pieces for an [`assemble`](crate::catalogue::items::util::assemble) list; none is the root,
/// so the lean is safe. `base_y` is the foot, `h` the height, `half` the
/// half-width at the foot. A plain pole reads as a lamppost; the lattice reads
/// as plant steelwork.
pub(super) fn lattice_mast(
    base_y: f32,
    h: f32,
    half: f32,
    mat: SovereignMaterialSettings,
) -> Vec<Generator> {
    let lean = 0.05;
    let mut v = Vec::new();
    // Four corner legs, leaning inward.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        v.push(prim(
            solid(cylinder_tapered(0.1, h, 8, 0.16, mat.clone())),
            [sx * half, base_y + h * 0.5, sz * half],
            quat_mul(quat_z(-sx * lean), quat_x(sz * lean)),
        ));
    }
    // Legs taper inward, so width shrinks with height.
    let width_at = |f: f32| half * (1.0 - 0.16 * f);
    let ring = |yy: f32, ww: f32, mat: &SovereignMaterialSettings| -> Vec<Generator> {
        [
            (0.0, -ww, 2.0 * ww, 0.07_f32),
            (0.0, ww, 2.0 * ww, 0.07),
            (-ww, 0.0, 0.07, 2.0 * ww),
            (ww, 0.0, 0.07, 2.0 * ww),
        ]
        .into_iter()
        .map(|(ax, az, lx, lz)| {
            prim(
                cuboid_tapered([lx, 0.07, lz], 0.0, mat.clone()),
                [ax, yy, az],
                id_quat(),
            )
        })
        .collect()
    };
    let segs = 3;
    for s in 0..segs {
        let f0 = s as f32 / segs as f32;
        let f1 = (s + 1) as f32 / segs as f32;
        let (y0, y1) = (base_y + h * f0, base_y + h * f1);
        let ym = (y0 + y1) * 0.5;
        let wm = width_at((f0 + f1) * 0.5);
        if s == 0 {
            v.extend(ring(y0, width_at(f0), &mat));
        }
        v.extend(ring(y1, width_at(f1), &mat));
        // One diagonal per face, alternating direction per segment (zig-zag).
        let dir = if s % 2 == 0 { 1.0 } else { -1.0 };
        let diag_len = (4.0 * wm * wm + (y1 - y0) * (y1 - y0)).sqrt();
        let ang = (2.0 * wm).atan2(y1 - y0);
        for sgn in [-1.0_f32, 1.0] {
            // Front/back faces (Z-normal): lean in the XY plane.
            v.push(prim(
                cuboid_tapered([0.06, diag_len, 0.06], 0.0, mat.clone()),
                [0.0, ym, sgn * wm],
                quat_z(dir * ang),
            ));
            // Left/right faces (X-normal): lean in the YZ plane.
            v.push(prim(
                cuboid_tapered([0.06, diag_len, 0.06], 0.0, mat.clone()),
                [sgn * wm, ym, 0.0],
                quat_x(dir * ang),
            ));
        }
    }
    v
}

/// A flat lit dial/gauge on a dark backing plate. Emissive reads on a flat
/// face and goes dark on a curved one (a disc flush on a domed tank head
/// z-fights the curve), so mount gauges as this proud plate. Authored facing
/// `-Z` (the render hero front): the lit face stands proud toward the camera.
pub(super) fn gauge_plate(center: [f32; 3], size: f32, lit: [f32; 3]) -> Vec<Generator> {
    let [x, y, z] = center;
    vec![
        prim(
            cuboid_tapered([size, size, 0.06], 0.0, tank_steel([0.14, 0.14, 0.16])),
            [x, y, z],
            id_quat(),
        ),
        prim(
            cuboid_tapered([size * 0.66, size * 0.66, 0.04], 0.0, glow(lit, 2.6)),
            [x, y, z - 0.05],
            id_quat(),
        ),
    ]
}

// Steel + concrete palette.
pub(super) const STEEL_BLUE: [f32; 3] = [0.42, 0.46, 0.50];
pub(super) const CONCRETE_GREY: [f32; 3] = [0.55, 0.55, 0.56];
pub(super) const TANK_WHITE: [f32; 3] = [0.76, 0.76, 0.74];
pub(super) const PIPE_GREY: [f32; 3] = [0.50, 0.52, 0.54];
pub(super) const BRICK_DARK: [f32; 3] = [0.40, 0.25, 0.21];
pub(super) const RUST_BROWN: [f32; 3] = [0.44, 0.28, 0.16];

// Shipping-container colours.
pub(super) const CONTAINER_RED: [f32; 3] = [0.52, 0.20, 0.16];
pub(super) const CONTAINER_BLUE: [f32; 3] = [0.18, 0.32, 0.46];
pub(super) const CONTAINER_GREEN: [f32; 3] = [0.20, 0.36, 0.24];
pub(super) const CONTAINER_RUST: [f32; 3] = [0.50, 0.34, 0.20];

// Emissive trim.
pub(super) const FLOOD_WHITE: [f32; 3] = [1.0, 0.96, 0.85];
pub(super) const WINDOW_LIT: [f32; 3] = [0.85, 0.86, 0.70];
/// Warm sodium-vapour glow — lit factory windows at dusk, dock lamps, the
/// control gauge. Deep-saturated amber so a broad lit pane reads incandescent
/// rather than blooming to a washed near-white.
pub(super) const LAMP_AMBER: [f32; 3] = [1.0, 0.66, 0.26];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (derelict) variants must build clean trees the
    /// sanitiser leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &derelict_shed::DerelictShed,
            &rusted_tank::RustedTank,
            &scrap_heap::ScrapHeap,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The floodlight is the kit's lit hero — it must keep its emissive lamps
    /// so escalation's broken-emissive ruin pass has something to kill.
    #[test]
    fn floodlight_keeps_its_lamps() {
        assert!(
            crate::catalogue::items::util::has_emissive(&floodlight::Floodlight.build("")),
            "floodlight lost its emissive lamps"
        );
    }
}
