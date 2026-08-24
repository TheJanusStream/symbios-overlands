//! Gilded circlet — the measurement-fit hero (#1089): the first wearable
//! whose worn size is a *measurement*, not an authored constant. The entry
//! declares one fit dimension ([`WearFit::HeadBand`]) — the band's authored
//! inner diameter — and at dress time every client scales the worn subtree
//! so that diameter matches the wearer's brow circumference / π, read off
//! the built body through the engine's public measure surface
//! (`src/player/attachments.rs`, `brow_circumference`). Placed as world
//! decor, and on any body whose head cannot be measured, it stays exactly
//! this authored size.
//!
//! Drawn as an **oversized draft** — the Jolly Roger technique
//! ([`util::uv_for_scale`](crate::catalogue::items::util::uv_for_scale)
//! carries the write-up): everything is authored at [`DRAFT`]× true size in
//! the root's local frame and the root carries the one uniform downscale.
//! The sanitiser's floors are prim-*local*, so the 7 mm rod and the 8 mm
//! stones below exist only because they are drawn as 70 mm and 80 mm ones —
//! a direct draft floors every dimension at 10 mm, which is how the first
//! build's band came out as sheet-metal `Tube` instead of a rod.

use crate::catalogue::items::util::{
    cuboid_tapered, id_quat, nest, prim, prim_scaled, solid, sphere, torus, uv_for_scale,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole, ThemeArchetype, WearFit};
use crate::pds::Generator;

use super::{gemstone, gold};

/// The band's authored **inner** diameter, in TRUE worn metres — the
/// declared fit dimension. 0.178 m is the equivalent-circle diameter of a
/// 0.559 m brow circumference, the middle of the seeded-body spread
/// (measured over seeds 0..8: 0.499–0.829 m), so the fit scale stays near 1
/// on a typical head and the decor size reads as "a circlet somebody could
/// wear".
const BAND_INNER_DIAMETER: f32 = 0.178;

/// The size the circlet is DRAWN at, relative to the size it is worn at.
/// Ten buys legible numbers (a 70 mm rod rather than a 7 mm one) and
/// headroom over the sanitiser's 10 mm local-space floor; the root's
/// uniform `1 / DRAFT` scale flies it at true size.
const DRAFT: f32 = 10.0;

/// Radius of the band's rod at draft size — a slender 7 mm rod of gold
/// worn, which is the figure the 10 mm floor refused as a direct draft.
const ROD: f32 = 0.007 * DRAFT;

/// Deep emerald, the centre stone.
const EMERALD: [f32; 3] = [0.05, 0.42, 0.16];
/// Garnet studs at the temples.
const GARNET: [f32; 3] = [0.45, 0.08, 0.10];

pub struct Circlet;

impl CatalogueEntry for Circlet {
    fn slug(&self) -> &'static str {
        "circlet"
    }
    fn name(&self) -> &'static str {
        "Gilded Circlet"
    }
    fn description(&self) -> &'static str {
        "A slender gold band, emerald-set — worn, it fits itself to the brow it lands on."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Attachment
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Fantasy]
    }
    fn wear_socket(&self) -> Option<symbios_avatar::Socket> {
        Some(symbios_avatar::Socket::Crown)
    }
    fn wear_fit(&self) -> Option<WearFit> {
        Some(WearFit::HeadBand {
            inner_diameter: BAND_INNER_DIAMETER,
        })
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.4,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// **The band circles the origin** — the [`WearFit::HeadBand`] authoring
/// convention: a fitted seat puts the attach origin on the head's axis at
/// the measured hat line (`src/player/attachments.rs`, `fitted_seat`), so
/// the ring lives in the X–Z plane at `y = 0` and the ornament rises above
/// it. Face on `+Z` (the side meant to be seen — a crown seat applies no
/// yaw, so authored front IS worn front). Every stone and the peak sink
/// INTO the band's rod — intersecting solids, never coplanar, never
/// gapped.
///
/// The whole tree is drawn at [`DRAFT`]× in the root's local frame and the
/// root — the band itself, at the origin — carries the single uniform
/// downscale. Children sit at the origin-relative draft coordinates, which
/// the root's frame scales down with everything else (the `nest` trap:
/// rebasing never divides by scale, so the root being AT the origin is
/// what keeps this trivially correct). Materials pass through
/// [`uv_for_scale`] with the instanced scale — inert while every surface
/// is untextured, and already correct the day one gains a weave.
fn build_tree() -> Generator {
    let scale = 1.0 / DRAFT;
    // Draft-frame figures: the declared bore, then the rod ring around it.
    let bore = BAND_INNER_DIAMETER * DRAFT / 2.0;
    let major = bore + ROD;
    // The band is the root: a slender torus lying in the X–Z plane at the
    // origin, encircling the hat line it will be seated on.
    let band = prim_scaled(
        solid(torus(ROD, major, uv_for_scale(gold(), scale))),
        [0.0, 0.0, 0.0],
        id_quat(),
        [scale; 3],
    );
    let mut ornament = vec![
        // Front peak: a small gold point rising off the band's brow — the
        // one silhouette flourish. Its foot sinks into the rod.
        prim(
            solid(cuboid_tapered(
                [0.24, 0.42, 0.09],
                0.92,
                uv_for_scale(gold(), scale),
            )),
            [0.0, 0.24, major - 0.02],
            id_quat(),
        ),
        // Centre stone, proud of the rod's front face on the band line.
        prim(
            solid(sphere(0.13, 2, uv_for_scale(gemstone(EMERALD), scale))),
            [0.0, 0.0, major + 0.04],
            id_quat(),
        ),
    ];
    // Temple studs, one over each ear line — 8 mm garnets worn, drawn as
    // 80 mm ones.
    for side in [-1.0f32, 1.0] {
        ornament.push(prim(
            solid(sphere(0.08, 2, uv_for_scale(gemstone(GARNET), scale))),
            [side * (major + 0.02), 0.0, 0.0],
            id_quat(),
        ));
    }
    nest(band, ornament)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Circlet.build(""), "circlet");
    }

    #[test]
    fn circlet_is_wearable_at_the_crown_and_declares_its_fit() {
        assert_eq!(Circlet.wear_socket(), Some(symbios_avatar::Socket::Crown));
        assert_eq!(Circlet.role(), StructureRole::Attachment);
        // The declared fit IS the authored geometry: the number the wire
        // carries must be the bore the band's ring actually has, or the
        // worn band lands snug on nobody.
        let fit = Circlet.wear_fit().expect("the fit hero declares a fit");
        assert_eq!(
            fit,
            WearFit::HeadBand {
                inner_diameter: BAND_INNER_DIAMETER
            }
        );
        assert_eq!(fit.band_mm(), 178, "whole millimetres on the wire");
    }
}
