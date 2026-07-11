//! Post-apocalyptic-theme catalogue structures — a scavenged survivor
//! settlement of fortified ruins and welded scrap.
//!
//! Two prosperity registers share one wasteland identity: the established
//! ([`POSTAPOC_BAND`]) holdout (a fortified ruin, a salvage shack, a radio
//! mast, a fuel depot, a wrecked car, a scrap wall, fuel barrels, a tyre wall
//! and a signal fire) and the destitute ([`POSTAPOC_POOR`]) drifter kit (a
//! survivor lean-to, a rubble barricade, an ash pit).
//!
//! Surfaces use the real procedural generators rather than flat colour: heavy
//! [`rusted`] scrap, cracked [`concrete`], corrugated [`sheet`] metal, grey
//! [`plank`] and matte [`tarp`]. The ruin's barrel fire and worklight glow
//! over a desolate-wind and fire-crackle bed from [`fx`]. The theme's
//! dust-haze accent lives in [`crate::seeded_defaults::room::accent`].

pub mod fortified_ruin;
pub mod fuel_barrels;
pub mod fuel_depot;
pub mod gateway;
pub mod radio_mast;
pub mod salvage_shack;
pub mod scrap_wall;
pub mod signal_fire;
pub mod tire_wall;
pub mod wrecked_car;
// Poor (drifter) variants — the prosperity-Poor end of the theme.
pub mod ash_pit;
pub mod rubble_barricade;
pub mod survivor_lean_to;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, id_quat, prim, quat_mul, quat_x, quat_y, solid, torus,
};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignConcreteConfig, SovereignCorrugatedConfig,
    SovereignMaterialSettings, SovereignMetalConfig, SovereignPlankConfig, SovereignTextureConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the holdout — a fortified, lit, defended camp
/// reads as a Modest-to-Rich survivor settlement. The poor end of the theme is
/// the separate drifter kit ([`survivor_lean_to`], …), tagged `Poor`, so a
/// destitute wasteland room grows the lone hovel instead.
pub(super) const POSTAPOC_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the drifter kit — the destitute end of the theme, never
/// picked for a modest or affluent wasteland room.
pub(super) const POSTAPOC_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Heavily-rusted scrap metal — welded walls, drums, car bodies, the mast.
pub(super) fn rusted(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.7),
        metallic: Fp(0.6),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.42, 0.24, 0.12]),
            roughness: Fp64(0.7),
            metallic: Fp(0.6),
            rust_level: Fp64(0.6),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Cracked, stained concrete — the ruin's surviving walls and slabs.
pub(super) fn concrete(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.92),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
            color_base: Fp3(color),
            formwork_lines: Fp64(3.0),
            pit_density: Fp64(0.2),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Rusting corrugated sheet — shanty walls, fences, lean-to roofs.
pub(super) fn sheet(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.6),
        metallic: Fp(0.6),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Corrugated(SovereignCorrugatedConfig {
            color_metal: Fp3(color),
            ridges: Fp64(10.0),
            rust_level: Fp64(0.45),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Grey weathered plank — salvaged timber framing and boards.
pub(super) fn plank(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.2, color[1] * 1.2, color[2] * 1.2]),
            color_wood_dark: Fp3([color[0] * 0.6, color[1] * 0.6, color[2] * 0.6]),
            plank_count: Fp64(5.0),
            knot_density: Fp64(0.4),
            grain_warp: Fp64(0.5),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Matte cloth / rubber / dirt — tarps, tyres, ash, sandbags. A plain surface
/// with no procedural texture.
pub(super) fn tarp(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

// Scrap + structure palette.
pub(super) const RUST_BROWN: [f32; 3] = [0.46, 0.30, 0.18];
pub(super) const STEEL_GREY: [f32; 3] = [0.40, 0.40, 0.42];
pub(super) const CONCRETE_GREY: [f32; 3] = [0.50, 0.49, 0.46];
pub(super) const CORRUGATED_RUST: [f32; 3] = [0.50, 0.38, 0.26];
pub(super) const PLANK_GREY: [f32; 3] = [0.42, 0.40, 0.36];
pub(super) const TARP_FADED: [f32; 3] = [0.40, 0.46, 0.40];
pub(super) const TIRE_BLACK: [f32; 3] = [0.10, 0.10, 0.11];
pub(super) const CAR_RUST: [f32; 3] = [0.46, 0.33, 0.27];
pub(super) const ASH_GREY: [f32; 3] = [0.26, 0.25, 0.24];

// Emissive trim colours. Deep-saturated so bloom keeps them coloured instead
// of blowing out to a near-white blob: the fire stays incandescent orange and
// the warning beacon stays a true red rather than washing to coral. The
// worklight is deliberately near-white — a salvaged halogen work lamp.
pub(super) const FIRE_ORANGE: [f32; 3] = [1.0, 0.42, 0.10];
pub(super) const WORKLIGHT: [f32; 3] = [1.0, 0.95, 0.82];
pub(super) const SIGNAL_RED: [f32; 3] = [1.0, 0.09, 0.05];

// ---------------------------------------------------------------------------
// Ruin-signature helpers — the collapse-and-decay vocabulary shared across the
// kit (the fortified ruin's blasted base, the barricade heap, the lean-to's
// rubble), so "broken reinforced concrete" reads the same everywhere.
// ---------------------------------------------------------------------------

/// Cheap deterministic fractional hash of an index — gives each scattered
/// chunk a stable pseudo-random offset/size without an rng, so a pile never
/// looks gridded yet round-trips bit-identically through the sanitiser.
pub(super) fn frac(x: f32) -> f32 {
    x - x.floor()
}

/// A deterministic scatter of broken angular concrete chunks heaped around
/// `center` — collapse debris at a wall base, a barricade, a rubble heap. `n`
/// chunks within `spread` radius, the largest roughly `base` across, each
/// yawed and sized by a hash of its index. Returns loose decorative prims for
/// the caller to extend `prims` with *before* [`crate::catalogue::items::util::assemble`]
/// (never the root — these carry yaw).
pub(super) fn rubble_chunks(center: [f32; 3], spread: f32, base: f32, n: usize) -> Vec<Generator> {
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let h = k as f32 * 2.399_963; // golden-angle stride, decorrelates the lanes
        let r = spread * (0.2 + 0.8 * frac(h * 1.7));
        let a = h;
        let s = base * (0.4 + 0.6 * frac(h * 3.1));
        out.push(prim(
            cuboid_tapered([s, s * 0.7, s * 0.85], 0.3, concrete(CONCRETE_GREY)),
            [
                center[0] + a.cos() * r,
                center[1] + s * 0.35,
                center[2] + a.sin() * r,
            ],
            quat_y(h * 1.3),
        ));
    }
    out
}

/// A few thin rusted reinforcing bars jutting at angles from a snapped
/// reinforced-concrete edge — the unmistakable signature of blasted structure.
/// Bars sprout upward from `center`, each leaning out on its own axis. Loose
/// decorative prims (carry tilt — never the root).
pub(super) fn rebar_stubs(center: [f32; 3], len: f32, n: usize) -> Vec<Generator> {
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let h = k as f32 * 2.399_963;
        let lean = 0.15 + 0.5 * frac(h * 1.9);
        let l = len * (0.7 + 0.5 * frac(h * 2.3));
        out.push(prim(
            cylinder_tapered(0.03, l, 4, 0.0, rusted(RUST_BROWN)),
            [
                center[0] + (frac(h) - 0.5) * 0.3,
                center[1] + l * 0.3,
                center[2] + (frac(h * 1.4) - 0.5) * 0.3,
            ],
            quat_mul(quat_y(h * 1.7), quat_x(lean)),
        ));
    }
    out
}

/// A short stack of half-buried tyres (flat tori) packed with rubble at
/// `center` — a salvaged barrier unit. Two tyres with a dirt-filled bore disc,
/// used to pack out the tyre wall and shore up barricades. Loose prims.
pub(super) fn tyre_stack(center: [f32; 3], rise: f32) -> Vec<Generator> {
    let mut out = Vec::new();
    for (i, dy) in [0.0_f32, rise].into_iter().enumerate() {
        out.push(prim(
            solid(torus(0.18, 0.42, tarp(TIRE_BLACK))),
            [center[0], center[1] + dy, center[2]],
            id_quat(),
        ));
        // Dirt packed into the bore of the lower tyre only.
        if i == 0 {
            out.push(prim(
                cylinder_tapered(0.3, 0.14, 10, 0.0, tarp(ASH_GREY)),
                [center[0], center[1] + dy, center[2]],
                id_quat(),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (drifter) variants must build clean trees the sanitiser
    /// leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &survivor_lean_to::SurvivorLeanTo,
            &rubble_barricade::RubbleBarricade,
            &ash_pit::AshPit,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The fortified ruin is the kit's lit hero — it must keep its emissive
    /// barrel fire and worklight so escalation's broken-emissive ruin pass has
    /// fire to snuff.
    #[test]
    fn ruin_keeps_its_fire() {
        assert!(
            crate::catalogue::items::util::has_emissive(&fortified_ruin::FortifiedRuin.build("")),
            "fortified ruin lost its emissive fire / worklight"
        );
    }
}
