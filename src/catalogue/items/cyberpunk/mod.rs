//! Cyberpunk-theme catalogue structures - the kit's quality benchmark for
//! every theme that follows.
//!
//! Three registers share the theme. The affluent ([`CYBER_BAND`]) downtown
//! buildings - megatower, data spire, arcade block, holo billboard, parking
//! stack - are realistic near-future architecture since #1559: curtain walls
//! and punched precast laid by texture ([`facade`]), setbacks, louvred plant,
//! lit lobbies and shopfronts, framed signs, aviation lights. Their glossy
//! neon street kit (kiosk, drone perch, cable arch) keeps the theme's neon,
//! and the destitute ([`CYBER_POOR`]) scrap-shanty undercity (container
//! shanty, container stack, tarp shelter, e-waste pile, busted terminal)
//! keeps its rust.
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! standing-seam [`metal`], [`corrugated`] container steel, [`concrete`]
//! decks, [`chain_link`] fencing, [`grille`] vents, brushed-rust [`rust`]
//! scrap and woven [`tarp`]; the downtown buildings' own vocabulary lives in
//! [`facade`]. Neon comes from strongly emissive [`super::util::glow`]
//! materials, and signature elements are brought to life with small particle
//! emitters and spatial-audio patches from [`fx`] (steam vents, failing-neon
//! sparks, transformer hum, drone whir, electrical crackle). The theme's
//! magenta fog accent lives in [`crate::seeded_defaults::room::accent`].
//!
//! **Emissive-strength discipline.** With HDR + bloom, a [`super::util::glow`]
//! surface clips to white once `colour × strength` pushes a channel past
//! `1.0`, and a *broad face* (a billboard panel, a screen) reaches that
//! point at a far lower strength than a *thin tube* (a band, an edge strip,
//! a ring). So the two can't share a value: thin neon trim runs hot
//! (`~5–9`) - the white-hot core plus a coloured bloom halo is exactly how
//! a neon tube reads - while broad faces stay moderate (`~1.5–3.5`) so they
//! read as lit *colour*, not a featureless white lightbox. A framed face
//! (panel ringed by a hot tube border) gets the best of both. Glass takes
//! none at all: emission adds its colour flat over panes and frames alike
//! (see [`facade`]).

pub mod arcade_block;
pub mod cable_arch;
pub mod data_spire;
pub mod drone_perch;
pub mod facade;
pub mod gateway;
pub mod holo_billboard;
pub mod monument;
pub mod neon_kiosk;
pub mod neon_megatower;
pub mod parking_stack;
pub mod street_block;
pub mod street_detached;
pub mod street_hall;
pub mod street_house;
pub mod street_low;
// Poor (undercity) variants - the prosperity-Poor end of the theme.
pub mod busted_terminal;
pub mod container_stack;
pub mod ewaste_pile;
pub mod scrap_shanty;
pub mod tarp_shelter;

pub mod fx;

use super::util::{ageing, tile, tiles_per_metre};
use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignChainLinkConfig, SovereignConcreteConfig, SovereignCorrugatedConfig,
    SovereignIronGrilleConfig, SovereignMaterialSettings, SovereignMetalConfig,
    SovereignTextureConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established neon kit - these glossy
/// megastructures read as a Modest-to-Rich settlement. The poor end of the
/// theme is the separate scrap-shanty kit ([`scrap_shanty`], …), tagged
/// `Poor`, so a destitute cyberpunk room grows the undercity instead.
pub(super) const CYBER_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the scrap-shanty undercity kit - the destitute end
/// of the theme, never picked for a modest or affluent cyberpunk room.
pub(super) const CYBER_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Dark, glossy structural metal - the body shared by every cyberpunk
/// build. Standing-seam panel lines + a touch of grime so the neon trim
/// reflects off a *surface*, not a flat slab.
pub(super) fn metal(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.32),
        metallic: Fp(0.85),
        uv_scale: tiles_per_metre(tile::METAL),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::StandingSeam,
            color_metal: Fp3(color),
            color_rust: Fp3([0.20, 0.12, 0.08]),
            seam_count: Fp64(8.0),
            seam_sharpness: Fp64(2.5),
            roughness: Fp64(0.32),
            metallic: Fp(0.85),
            rust_level: Fp64(0.06),
            weathering: ageing::stained(0x41, 0.8),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Ridged corrugated steel - shipping containers and lean-to roofing. The
/// correct surface for the scrap-shanty undercity, with built-in rust.
pub(super) fn corrugated(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.6),
        metallic: Fp(0.6),
        uv_scale: tiles_per_metre(tile::CORRUGATED_PITCH * 10.0),
        texture: SovereignTextureConfig::Corrugated(SovereignCorrugatedConfig {
            color_metal: Fp3(color),
            color_rust: Fp3([0.42, 0.22, 0.10]),
            ridges: Fp64(10.0),
            ridge_depth: Fp64(1.0),
            roughness: Fp64(0.5),
            metallic: Fp(0.6),
            rust_level: Fp64(0.3),
            weathering: ageing::corroded(0x42, 0.6),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Corroded brushed metal with heavy rust - battered scrap panels, drums,
/// dead chassis. The poor counterpoint to the glossy [`metal`].
pub(super) fn rust(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        metallic: Fp(0.4),
        uv_scale: tiles_per_metre(tile::METAL),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.30, 0.16, 0.08]),
            seam_count: Fp64(3.0),
            roughness: Fp64(0.85),
            metallic: Fp(0.4),
            rust_level: Fp64(0.55),
            weathering: ageing::corroded(0x43, 0.9),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Rusted chain-link / mesh - undercity fencing and cage panels.
pub(super) fn chain_link() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.5, 0.52, 0.54]),
        roughness: Fp(0.7),
        metallic: Fp(0.5),
        texture: SovereignTextureConfig::ChainLink(SovereignChainLinkConfig {
            rust_level: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Board-formed concrete - parking decks, stair cores, plinths.
pub(super) fn concrete(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        uv_scale: tiles_per_metre(tile::CONCRETE),
        texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
            color_base: Fp3(color),
            formwork_lines: Fp64(4.0),
            formwork_depth: Fp64(0.1),
            weathering: ageing::stained(0x44, 0.9),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Rusted iron louvre / grille - wall vents and exhaust louvres.
pub(super) fn grille() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.14, 0.13, 0.13]),
        roughness: Fp(0.6),
        metallic: Fp(0.6),
        texture: SovereignTextureConfig::IronGrille(SovereignIronGrilleConfig {
            rust_level: Fp64(0.25),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Sagging tarp / plastic sheeting over a makeshift shelter - woven-fabric
/// weave normal so it reads as cloth, not a painted plank.
pub(super) fn tarp(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        metallic: Fp(0.0),
        uv_scale: tiles_per_metre(tile::FABRIC),
        texture: SovereignTextureConfig::Fabric(crate::pds::SovereignFabricConfig::default()),
        ..Default::default()
    }
}

/// Near-black panelled body colour.
pub(super) const DARK_METAL: [f32; 3] = [0.06, 0.07, 0.10];
pub(super) const NEON_CYAN: [f32; 3] = [0.10, 0.95, 1.00];
pub(super) const NEON_MAGENTA: [f32; 3] = [1.00, 0.12, 0.78];
pub(super) const NEON_LIME: [f32; 3] = [0.55, 1.00, 0.20];

// Scrap-shanty palette - weathered container steel, rust, faded tarp.
pub(super) const CONTAINER_BLUE: [f32; 3] = [0.18, 0.30, 0.38];
pub(super) const CONTAINER_RUST: [f32; 3] = [0.45, 0.28, 0.18];
pub(super) const RUST_BROWN: [f32; 3] = [0.34, 0.22, 0.14];
pub(super) const TARP_BLUE: [f32; 3] = [0.18, 0.26, 0.42];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The five poor (undercity) variants must build clean trees the
    /// sanitiser leaves untouched, and each must still carry its dim neon.
    #[test]
    fn poor_variants_round_trip_and_keep_their_glow() {
        let entries: [&dyn CatalogueEntry; 5] = [
            &scrap_shanty::ScrapShanty,
            &container_stack::ContainerStack,
            &tarp_shelter::TarpShelter,
            &ewaste_pile::EwastePile,
            &busted_terminal::BustedTerminal,
        ];
        for e in entries {
            let built = e.build("");
            assert_sanitize_stable(&built, e.slug());
            assert!(
                crate::catalogue::items::util::has_emissive(&built),
                "{} lost its neon",
                e.slug()
            );
        }
    }

    /// How much wider than its corner reach a downtown building's spacing
    /// circle is (#1559). The settlement keeps a prop only half the two
    /// clearances from a building's centre, so a circle round the corners
    /// with a margin stood props inside the podiums; about twice the reach
    /// keeps every prop clear ([`no_prop_stands_in_a_downtown_building`]).
    pub(super) const SPACING: f32 = 1.75;

    /// How far from its axis a built entry reaches at the ground (m): the
    /// farthest corner of any solid that comes down to its plinth's foot -
    /// the footing, the plinth and anything set into them.
    fn ground_reach(entry: &dyn CatalogueEntry) -> f32 {
        crate::catalogue::items::measure::solids(&entry.build(""))
            .iter()
            .filter(|s| s.bounds.min.y <= 0.05)
            .flat_map(|s| {
                let (a, b) = (s.bounds.min, s.bounds.max);
                [
                    a.x.hypot(a.z),
                    a.x.hypot(b.z),
                    b.x.hypot(a.z),
                    b.x.hypot(b.z),
                ]
            })
            .fold(0.0_f32, f32::max)
    }

    /// A plinth root's size, the key a seeded room's generator is matched to
    /// its downtown entry by: the five plinths differ, and no ruin or scale
    /// changes a root's size.
    fn plinth_of(g: &crate::pds::Generator) -> Option<[f32; 3]> {
        match &g.kind {
            crate::pds::GeneratorKind::Cuboid { size, .. } => Some(size.0),
            _ => None,
        }
    }

    /// Each downtown entry's own measures at scale 1: its slug, its plinth,
    /// how far it reaches at the ground, and how high its tallest solid
    /// stands.
    fn downtown_measures() -> Vec<(&'static str, [f32; 3], f32, f32)> {
        downtown()
            .into_iter()
            .map(|entry| {
                let built = entry.build("");
                let top = crate::catalogue::items::measure::solids(&built)
                    .iter()
                    .map(|s| s.bounds.max.y)
                    .fold(0.0_f32, f32::max);
                let plinth = plinth_of(&built).expect("a plinth root");
                (entry.slug(), plinth, ground_reach(entry), top)
            })
            .collect()
    }

    /// The downtown entry `g` was built from, if any, with its measures.
    fn downtown_of(
        measures: &[(&'static str, [f32; 3], f32, f32)],
        g: &crate::pds::Generator,
    ) -> Option<(&'static str, f32, f32)> {
        let want = plinth_of(g)?;
        measures
            .iter()
            .find(|m| m.1 == want)
            .map(|&(slug, _, reach, top)| (slug, reach, top))
    }

    /// The first `n` Cyberpunk rooms the seeds give at escalation `tier`,
    /// built as the game builds a seeded room on its own terrain: a seed
    /// drawing Berlin has no settlement to measure (#1589).
    fn cyberpunk_rooms(
        tier: crate::seeded_defaults::EscalationTier,
        n: usize,
    ) -> Vec<(u64, crate::pds::RoomRecord)> {
        use crate::seeded_defaults::{EscalationTier, SceneCharacter, ThemeArchetype};
        (0..4000_u64)
            .filter(|&seed| {
                let scene = SceneCharacter::for_seed(seed);
                scene.theme == ThemeArchetype::Cyberpunk
                    && EscalationTier::from_unit(scene.escalation) == tier
            })
            .take(n)
            .map(|seed| {
                let did = format!("did:render:{seed}");
                let room =
                    crate::seeded_defaults::room::build::build_room_with_source(seed, &did, None);
                (seed, room)
            })
            .collect()
    }

    /// The five downtown buildings Isoline's lots grow (#1559).
    fn downtown() -> [&'static dyn CatalogueEntry; 5] {
        [
            &neon_megatower::NeonMegatower,
            &data_spire::DataSpire,
            &arcade_block::ArcadeBlock,
            &parking_stack::ParkingStack,
            &holo_billboard::HoloBillboard,
        ]
    }

    /// Each downtown building stands on the ground under its own footing
    /// (#1559): the ground radius it declares reaches every corner of what
    /// it puts on the ground, so the pad it is floored on is the highest
    /// ground under it - and no further, so the pad is not the highest
    /// ground within the much wider circle it is spaced by. Floored on that
    /// circle (23 m for the megatower), a building on a slope stood metres
    /// over its downhill side.
    #[test]
    fn each_downtown_building_stands_on_the_ground_under_it() {
        for entry in downtown() {
            let slug = entry.slug();
            let reach = ground_reach(entry);
            let ground = entry
                .ground_radius()
                .unwrap_or_else(|| panic!("{slug} declares no ground of its own"));
            assert!(
                ground >= reach - 1e-3,
                "{slug}: a {ground} m ground under {reach} m of footing"
            );
            assert!(
                ground <= reach + 0.2,
                "{slug}: a {ground} m ground round {reach} m of footing"
            );
            assert!(
                ground < entry.footprint().clearance,
                "{slug}: spaced by no more than its own ground"
            );
        }
    }

    /// A lot fits each downtown building by its own half side (#1559), not
    /// by the circle a seeded room spaces it by: the half side it declares
    /// covers its plinth and reaches no further than its widest part above
    /// the ground. The lot fit read the clearance, so a clearance wide
    /// enough to keep props out would have drawn the megatower at half its
    /// size on a lot it fills.
    #[test]
    fn a_lot_fits_each_downtown_building_by_its_own_half_side() {
        use crate::pds::GeneratorKind;
        for entry in downtown() {
            let slug = entry.slug();
            let built = entry.build("");
            let GeneratorKind::Cuboid { size, .. } = &built.kind else {
                panic!("{slug}: a plinth root");
            };
            let plinth = size.0[0].max(size.0[2]) * 0.5;
            let widest = crate::catalogue::items::measure::solids(&built)
                .iter()
                .filter(|s| s.bounds.max.y > 0.05)
                .map(|s| {
                    let (a, b) = (s.bounds.min, s.bounds.max);
                    a.x.abs().max(b.x.abs()).max(a.z.abs()).max(b.z.abs())
                })
                .fold(0.0_f32, f32::max);
            let half = entry.lot_half_width();
            assert!(
                half >= plinth,
                "{slug}: a {half} m half side on a {plinth} m plinth"
            );
            assert!(
                half <= widest + 0.2,
                "{slug}: a {half} m half side round {widest} m of building"
            );
        }
    }

    /// No prop stands in or against a downtown building in a seeded room
    /// (#1559). The settlement keeps a prop only half the two clearances
    /// from a building's centre, so a clearance cut to the building's own
    /// reach stood props inside the megatower's podium - 19 in the Calm and
    /// Tense rooms of seeds 0-6000, and 28 more against a wall. Over the
    /// Cyberpunk rooms of seeds 0-1000: no solid of any prop meets a solid
    /// of a downtown building at the ground, in plan, within 5 cm.
    #[test]
    fn no_prop_stands_in_a_downtown_building() {
        use crate::pds::{GeneratorKind, Placement};
        use crate::seeded_defaults::{SceneCharacter, ThemeArchetype};
        use bevy::math::Vec2;
        // A solid in plan, turned with its placement: centre, half
        // extents, yaw, and the heights it spans.
        struct Rect {
            c: Vec2,
            h: Vec2,
            yaw: f32,
            y: (f32, f32),
        }
        fn axes(yaw: f32) -> [Vec2; 2] {
            let (s, c) = yaw.sin_cos();
            [Vec2::new(c, -s), Vec2::new(s, c)]
        }
        fn corners(r: &Rect) -> [Vec2; 4] {
            let [ax, az] = axes(r.yaw);
            let (x, z) = (ax * r.h.x, az * r.h.y);
            [r.c - x - z, r.c + x - z, r.c + x + z, r.c - x + z]
        }
        // Separating axes, each box shrunk by 5 cm.
        fn meet(a: &Rect, b: &Rect) -> bool {
            if a.y.1 < b.y.0 || b.y.1 < a.y.0 {
                return false;
            }
            let (ca, cb) = (corners(a), corners(b));
            [a.yaw, b.yaw].into_iter().flat_map(axes).all(|axis| {
                let span = |cs: &[Vec2; 4]| {
                    cs.iter()
                        .map(|p| p.dot(axis))
                        .fold((f32::MAX, f32::MIN), |m, v| (m.0.min(v), m.1.max(v)))
                };
                let ((a0, a1), (b0, b1)) = (span(&ca), span(&cb));
                a1 - 0.05 >= b0 && b1 - 0.05 >= a0
            })
        }
        fn rects(g: &crate::pds::Generator, t: &crate::pds::TransformData) -> Vec<Rect> {
            let q = t.rotation.0;
            let yaw = 2.0 * q[1].atan2(q[3]);
            let at = Vec2::new(t.translation.0[0], t.translation.0[2]);
            let [ax, az] = axes(yaw);
            crate::catalogue::items::measure::solids(g)
                .into_iter()
                .map(|s| {
                    let c = s.bounds.center();
                    let h = (s.bounds.max - s.bounds.min) * 0.5;
                    Rect {
                        c: at + ax * c.x + az * c.z,
                        h: Vec2::new(h.x, h.z),
                        yaw,
                        y: (s.bounds.min.y, s.bounds.max.y),
                    }
                })
                .collect()
        }
        let size = |g: &crate::pds::Generator| match &g.kind {
            GeneratorKind::Cuboid { size, .. } => Some(size.0),
            _ => None,
        };
        let roots: Vec<(&str, [f32; 3])> = downtown()
            .iter()
            .map(|e| (e.slug(), size(&e.build("")).expect("a plinth root")))
            .collect();
        let (mut rooms, mut pairs) = (0, 0);
        for seed in 0..1000_u64 {
            if SceneCharacter::for_seed(seed).theme != ThemeArchetype::Cyberpunk {
                continue;
            }
            // On its own terrain: a seed drawing Berlin has no settlement
            // (#1589).
            let record = crate::seeded_defaults::room::build::build_room_with_source(
                seed,
                &format!("did:render:{seed}"),
                None,
            );
            let (mut buildings, mut props) = (Vec::new(), Vec::new());
            for placement in &record.placements {
                let Placement::Absolute {
                    generator_ref,
                    transform,
                    ..
                } = placement
                else {
                    continue;
                };
                let Some(g) = record.generators.get(generator_ref) else {
                    continue;
                };
                if generator_ref.starts_with("settlement_prop_") {
                    props.push((generator_ref, rects(g, transform)));
                } else if let Some((slug, _)) = roots.iter().find(|(_, s)| size(g) == Some(*s)) {
                    // What stands at the ground: footings meeting a prop's
                    // under the turf are nobody's business.
                    let at_ground = rects(g, transform)
                        .into_iter()
                        .filter(|r| r.y.0 < 2.0 && r.y.1 > 0.05)
                        .collect::<Vec<_>>();
                    buildings.push((*slug, at_ground));
                }
            }
            rooms += usize::from(!buildings.is_empty());
            for (slug, walls) in &buildings {
                for (prop, solids) in &props {
                    pairs += 1;
                    assert!(
                        !solids
                            .iter()
                            .filter(|r| r.y.1 > 0.05)
                            .any(|r| walls.iter().any(|w| meet(r, w))),
                        "seed {seed}: {prop} stands in the {slug}"
                    );
                }
            }
        }
        assert!(
            rooms >= 20 && pairs >= 300,
            "too few to say anything: {rooms} rooms, {pairs} pairs"
        );
    }

    /// A fought-over room sways a downtown building's top by about a metre
    /// at most (#1559): the tier's lean is an angle, and 0.22 rad of it
    /// swung the megatower's crown 30 m past its footprint. Over 64
    /// conflict ruins of each, the top of the trunk - the building the ruin
    /// pass may lean and knock but never take - moves at most 1.25 m in
    /// plan, the whole lean and the trunk's own knock together.
    #[test]
    fn each_downtown_ruin_sways_its_top_a_metre_at_most() {
        use crate::catalogue::items::measure::solids;
        for entry in downtown() {
            let slug = entry.slug();
            let before = solids(&entry.build(""));
            let top = before
                .iter()
                .filter(|s| s.path.first() == Some(&0))
                .max_by(|a, b| a.bounds.max.y.total_cmp(&b.bounds.max.y))
                .expect("a trunk");
            for seed in 0..64_u64 {
                let mut ruined = entry.build("");
                crate::pds::ruin::apply_ruin_bounded(
                    &mut ruined,
                    0.95,
                    seed,
                    entry.ruin_max_lean(),
                );
                let after = solids(&ruined);
                let moved = after
                    .iter()
                    .find(|s| s.path == top.path)
                    .unwrap_or_else(|| panic!("{slug} seed {seed}: the ruin took the trunk"));
                // The pass also shrinks the whole a touch about its origin,
                // which draws an off-centre top in: that is not a sway.
                let k = ruined.transform.scale.0[0];
                let (was, now) = (top.bounds.center() * k, moved.bounds.center());
                let sway = (now.x - was.x).hypot(now.z - was.z);
                assert!(sway <= 1.25, "{slug} seed {seed}: the top swayed {sway} m");
            }
        }
    }

    /// A fought-over seeded room leans each downtown building by its own
    /// bound (#1559), through the settlement: the room build hands the
    /// member's `ruin_max_lean` to the ruin pass. Handed nothing, the tier's
    /// 0.22 rad swung the megatower's crown up to 35 m. In the first six
    /// Conflict-tier Cyberpunk rooms, every downtown building's whole lean
    /// swings the top of its tree a metre at most (at scale 1).
    #[test]
    fn a_fought_over_room_leans_each_downtown_building_a_metre_at_most() {
        let measures = downtown_measures();
        let mut leaned = 0;
        let rooms = cyberpunk_rooms(crate::seeded_defaults::EscalationTier::Conflict, 6);
        for (seed, record) in &rooms {
            for (name, g) in &record.generators {
                let Some((slug, _, top)) = downtown_of(&measures, g) else {
                    continue;
                };
                let q = g.transform.rotation.0;
                let tilt = 2.0 * q[0].hypot(q[2]).min(1.0).asin();
                assert!(
                    tilt * top <= 1.0 + 1e-3,
                    "seed {seed}: {name} ({slug}) leans {tilt} rad, its top {} m over",
                    tilt * top
                );
                leaned += 1;
            }
        }
        assert!(
            rooms.len() == 6 && leaned >= 12,
            "too few to say anything: {} rooms, {leaned} buildings",
            rooms.len()
        );
    }

    /// A seeded room floors each downtown building on the ground under its
    /// own footing (#1559), through the settlement: the room build writes
    /// the entry's ground radius, at the member's scale, into the placement
    /// the compile snaps by. Written from the clearance, the pad was the
    /// highest ground within a 23 m circle round the megatower. In the
    /// first eight Calm-tier Cyberpunk rooms - where no ruin shrinks the
    /// root, so its scale is the member's - every downtown building's
    /// placement carries a ground that covers its footing's corners and
    /// stops within 20 cm of them.
    #[test]
    fn a_seeded_room_floors_each_downtown_building_on_the_ground_under_it() {
        use crate::pds::Placement;
        let measures = downtown_measures();
        let mut floored = 0;
        let rooms = cyberpunk_rooms(crate::seeded_defaults::EscalationTier::Calm, 8);
        for (seed, record) in &rooms {
            for placement in &record.placements {
                let Placement::Absolute {
                    generator_ref,
                    avoid_water_clearance,
                    ..
                } = placement
                else {
                    continue;
                };
                let Some(g) = record.generators.get(generator_ref) else {
                    continue;
                };
                let Some((slug, reach, _)) = downtown_of(&measures, g) else {
                    continue;
                };
                let scale = g.transform.scale.0[0];
                let ground = avoid_water_clearance.0;
                assert!(
                    ground >= reach * scale - 1e-3 && ground <= (reach + 0.2) * scale + 1e-3,
                    "seed {seed}: {generator_ref} ({slug}) at {scale}x stands on {ground} m \
                     of ground round {} m of footing",
                    reach * scale
                );
                floored += 1;
            }
        }
        assert!(
            rooms.len() == 8 && floored >= 12,
            "too few to say anything: {} rooms, {floored} buildings",
            rooms.len()
        );
    }
}
