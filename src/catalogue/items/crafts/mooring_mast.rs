//! Mooring mast - a place for an airship (#1782): a four-legged steel
//! lattice tapering up from a concrete base, braced in a zigzag on every
//! face, to a railed platform, and over it a red-and-white head mast with
//! the cup an airship's nose is made fast in and a red aviation light lit
//! from dusk to dawn.
//!
//! [`HEIGHT`] tall over its origin, which a seeded world stands on the
//! highest ground under its base.

use super::{CONCRETE, PAINT_WHITE, SIGNAL_RED, STEEL, concrete, lamp, paint, steel};
use crate::catalogue::items::util::{
    cone, cuboid_tapered, cylinder_tapered, id_quat, nest, prim, quat_x, solid, strut,
};
use crate::catalogue::{CatalogueEntry, Craft, Footprint, StructureRole};
use crate::pds::Generator;

/// How tall the mast stands over its origin, its light's top (m).
pub const HEIGHT: f32 = 16.7;
/// Its base slab's side (m).
pub const BASE: f32 = 3.8;

/// The legs' half-spread at the foot and at the platform (m).
const FOOT: f32 = 1.5;
const HEAD: f32 = 0.45;
/// The platform's height (m) and the lattice's bays under it.
const PLATFORM: f32 = 13.5;
const BAYS: usize = 4;
const LEG_R: f32 = 0.09;
const BRACE_R: f32 = 0.045;
/// The head mast's height over the platform (m).
const HEAD_MAST: f32 = 2.2;
/// Its base's top over the origin, and how far the base reaches under it
/// (m): a plinth down to ground falling away that far, as a seeded mast
/// stands only where its ground falls no further.
const BASE_TOP: f32 = 0.1;
pub const BASE_DEPTH: f32 = 3.5;

pub struct MooringMast;

impl CatalogueEntry for MooringMast {
    fn slug(&self) -> &'static str {
        "mooring_mast"
    }
    fn name(&self) -> &'static str {
        "Mooring mast"
    }
    fn description(&self) -> &'static str {
        "Steel lattice mast on a concrete base, a railed platform and a mooring cup at \
         its head, its aviation light lit from dusk to dawn: a place to moor an airship."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Port
    }
    fn serves(&self) -> Option<Craft> {
        Some(Craft::Airship)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.0,
            min_spawn_dist: 20.0,
        }
    }
    fn build(&self, _local_did: &str) -> Generator {
        mooring_mast()
    }
}

/// The lattice's half-spread at height `y`.
fn spread(y: f32) -> f32 {
    FOOT + (HEAD - FOOT) * (y / PLATFORM)
}

/// The mast (see the module docs).
pub fn mooring_mast() -> Generator {
    let base_h = BASE_TOP + BASE_DEPTH;
    let base = prim(
        solid(cuboid_tapered(
            [BASE, base_h, BASE],
            0.0,
            concrete(CONCRETE),
        )),
        [0.0, BASE_TOP - base_h * 0.5, 0.0],
        id_quat(),
    );
    let foot_y = BASE_TOP;
    let corner = |sx: f32, sz: f32, y: f32| {
        let w = spread(y - foot_y);
        [sx * w, y, sz * w]
    };
    let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    let top = foot_y + PLATFORM;
    let mut parts = Vec::new();
    for &(sx, sz) in &corners {
        let mut leg = strut(
            corner(sx, sz, foot_y),
            corner(sx, sz, top),
            LEG_R,
            8,
            steel(STEEL),
        );
        leg.kind = solid(leg.kind);
        parts.push(leg);
    }
    // A band round each bay's top, and one diagonal on each face of every
    // bay, leaning the other way bay by bay: a zigzag up each face.
    let bay = PLATFORM / BAYS as f32;
    for k in 0..BAYS {
        let (y0, y1) = (foot_y + bay * k as f32, foot_y + bay * (k + 1) as f32);
        for (i, &(sx, sz)) in corners.iter().enumerate() {
            let (nx, nz) = corners[(i + 1) % corners.len()];
            if k + 1 < BAYS {
                parts.push(strut(
                    corner(sx, sz, y1),
                    corner(nx, nz, y1),
                    BRACE_R,
                    6,
                    steel(STEEL),
                ));
            }
            let (from, to) = if k % 2 == 0 {
                (corner(sx, sz, y0), corner(nx, nz, y1))
            } else {
                (corner(nx, nz, y0), corner(sx, sz, y1))
            };
            parts.push(strut(from, to, BRACE_R, 6, steel(STEEL)));
        }
    }

    // The platform, railed round, at the lattice's head.
    let deck = HEAD * 2.0 + 0.9;
    parts.push(prim(
        solid(cuboid_tapered([deck, 0.08, deck], 0.0, steel(STEEL))),
        [0.0, top + 0.04, 0.0],
        id_quat(),
    ));
    let rail_y = top + 1.0;
    let half = deck * 0.5 - 0.03;
    for (i, &(sx, sz)) in corners.iter().enumerate() {
        let (nx, nz) = corners[(i + 1) % corners.len()];
        parts.push(strut(
            [sx * half, top + 0.08, sz * half],
            [sx * half, rail_y, sz * half],
            0.03,
            6,
            steel(STEEL),
        ));
        parts.push(strut(
            [sx * half, rail_y, sz * half],
            [nx * half, rail_y, nz * half],
            0.03,
            6,
            steel(STEEL),
        ));
    }

    // The head mast in red and white, its mooring cup, and the light.
    let band = HEAD_MAST / 4.0;
    for k in 0..4 {
        let colour = if k % 2 == 0 { SIGNAL_RED } else { PAINT_WHITE };
        parts.push(prim(
            solid(cylinder_tapered(0.17, band, 12, 0.0, paint(colour))),
            [0.0, top + 0.08 + band * (k as f32 + 0.5), 0.0],
            id_quat(),
        ));
    }
    let head_top = top + 0.08 + HEAD_MAST;
    // A cone stood on its point: the cup the nose fits in.
    parts.push(prim(
        solid(cone(0.55, 0.6, 16, steel(STEEL))),
        [0.0, head_top + 0.3, 0.0],
        quat_x(std::f32::consts::PI),
    ));
    parts.push(prim(
        cylinder_tapered(0.12, 0.22, 10, 0.3, lamp([1.0, 0.12, 0.08], 6.0)),
        [0.0, head_top + 0.71, 0.0],
        id_quat(),
    ));
    nest(base, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure;

    /// It stands HEIGHT tall on its base, its lattice inside its base's
    /// square, and its light glows only from dusk to dawn.
    #[test]
    fn the_mast_stands_its_height_on_its_base() {
        let mast = mooring_mast();
        let bounds = measure::tree_bounds(&mast).expect("it draws");
        assert!((bounds.max.y - HEIGHT).abs() < 0.02, "{}", bounds.max.y);
        assert!(bounds.max.x <= BASE * 0.5 + 1e-3 && bounds.min.x >= -BASE * 0.5 - 1e-3);
        let lit: Vec<_> = mast
            .children
            .iter()
            .filter_map(|child| child.kind.material())
            .filter(|m| m.emission_strength.0 > 0.0)
            .collect();
        assert_eq!(lit.len(), 1, "one light");
        assert!(lit[0].dusk_to_dawn);
    }
}
