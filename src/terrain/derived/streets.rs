//! Berlin's street buildings on Berlin's footprints (#1598): where the
//! room's theme has its street house, long block and low building
//! ([`crate::catalogue::items::street`]), a footprint's secondary buildings
//! are drawn from those alone, each copy shaped to its footprint - a kind
//! by the building's storeys and use, a depth by its row, a frontage by
//! what is left of the row - rather than an entry scaled to fit.
//!
//! Copies of one street building at one fit, seed and scale share a
//! template: one tree grown, one set of merged meshes, one shell. A row of
//! one fit would repeat one house, so each fit is drawn with
//! [`STREET_VARIANTS`] seeds, neighbours rolled apart - their claddings,
//! their `Pick`s, their lit windows. The templates a plan grows are held
//! to a budget ([`template`]): past half of it a copy takes its fit's
//! first seed, and past all of it, the template of the same building that
//! comes nearest without outgrowing its slot - or, where none fits, it is
//! left out. A plan takes its buildings nearest the landing first, so the
//! budget goes to what a visitor sees first.

use crate::catalogue::CatalogueEntry;
use crate::catalogue::items::street::{StreetFit, StreetKind};
use crate::terrain::lots::fitted_scale;

use super::fit::SCALE_MIN;

/// The seeds each street building's fit is drawn with (#1598).
pub(crate) const STREET_VARIANTS: u8 = 2;

/// What tells a street building's seeds apart: variant `v` draws with the
/// plan's seed for its entry, crossed with `v` times this.
const VARIANT_SALT: u64 = 0x57EE_7FA5_ADE0_0001;

/// The seed variant `variant` of a street building is drawn with, from the
/// seed the plan grows its entry with: the entry's own for variant 0.
pub(crate) fn variant_seed(entry_seed: u64, variant: u8) -> u64 {
    entry_seed ^ u64::from(variant).wrapping_mul(VARIANT_SALT)
}

/// A theme's street buildings, one of each kind.
#[derive(Clone, Copy)]
pub(crate) struct Streets([&'static dyn CatalogueEntry; 3]);

impl Streets {
    /// The street buildings among `pool`, one of each kind - the first of a
    /// kind where there are more - or `None` where it lacks a kind: then
    /// its footprints take its other secondary buildings, as before #1598.
    pub(crate) fn of(pool: &[&'static dyn CatalogueEntry]) -> Option<Self> {
        let mut by_kind: [Option<&'static dyn CatalogueEntry>; 3] = [None; 3];
        for &entry in pool {
            if let Some(spec) = entry.street() {
                by_kind[index(spec.kind)].get_or_insert(entry);
            }
        }
        Some(Streets([by_kind[0]?, by_kind[1]?, by_kind[2]?]))
    }

    /// The street building of `kind`.
    pub(crate) fn entry(&self, kind: StreetKind) -> &'static dyn CatalogueEntry {
        self.0[index(kind)]
    }
}

/// `kind`'s place in [`StreetKind::ALL`].
fn index(kind: StreetKind) -> usize {
    match kind {
        StreetKind::House => 0,
        StreetKind::Block => 1,
        StreetKind::Low => 2,
    }
}

/// The next kind down, for a row's tail or a band too shallow for a kind:
/// a block's is a house, a house's a low building.
pub(crate) fn smaller(kind: StreetKind) -> Option<StreetKind> {
    match kind {
        StreetKind::Block => Some(StreetKind::House),
        StreetKind::House => Some(StreetKind::Low),
        StreetKind::Low => None,
    }
}

/// The kind a building of `storeys` is drawn as, and whether its ground
/// floor trades, by its storeys and Berlin's use for it: a building of one
/// or two storeys - or a workshop, a shop, a garage of three - is a low
/// building, one of three to seven a house, and a taller one a long block.
pub(crate) fn kind_of(storeys: u8, works: bool) -> StreetKind {
    match storeys {
        _ if storeys <= 2 || (works && storeys <= 3) => StreetKind::Low,
        3..=7 => StreetKind::House,
        _ => StreetKind::Block,
    }
}

/// The depth `kind` is drawn at in a row `band` deep (m), and its scale:
/// its deepest step that fits at its size, or else its shallowest drawn
/// smaller, in the lot layer's quarter-octaves down to [`SCALE_MIN`];
/// `None` where even that is too deep.
pub(crate) fn depth_for(kind: StreetKind, band: f32) -> Option<(u16, f32)> {
    let depths = kind.depths();
    if let Some(&depth) = depths.iter().rev().find(|&&d| f32::from(d) <= band + 1e-3) {
        return Some((depth, 1.0));
    }
    let shallowest = f32::from(depths[0]);
    let scale = fitted_scale(band / shallowest, SCALE_MIN, 1.0);
    (scale * shallowest <= band + 1e-3).then_some((depths[0], scale))
}

/// One street template of a plan: the building's slug, its fit, its seed
/// variant and its scale (`scale_e4`).
pub(crate) type StreetKey = (&'static str, StreetFit, u8, i64);

/// The template a street copy that wants `want` is drawn from, of the
/// templates `grown` so far against `budget` (see the module docs): `want`
/// itself where the plan has it or may grow it; its fit's first seed past
/// half the budget; and past all of it, the one grown of its building at
/// its scale that comes nearest without outgrowing its slot - its frontage
/// and depth first, so its row keeps its spacing, then its trade, then the
/// storeys nearest its own. A shallower one stands back to keep its front
/// on the street line (the planner's to do), a narrower one in the middle
/// of its slot. `None` where none fits: the copy is left out.
pub(crate) fn template(grown: &[StreetKey], budget: usize, want: StreetKey) -> Option<StreetKey> {
    let has = |key: &StreetKey| grown.contains(key);
    if has(&want) || grown.len() < budget / 2 {
        return Some(want);
    }
    let (slug, fit, _, scale_e4) = want;
    let first = (slug, fit, 0, scale_e4);
    if has(&first) || grown.len() < budget {
        return Some(first);
    }
    grown
        .iter()
        .filter(|&&(s, f, _, e)| {
            s == slug && e == scale_e4 && f.frontage <= fit.frontage && f.depth <= fit.depth
        })
        .min_by_key(|&&(_, f, variant, _)| {
            (
                fit.frontage - f.frontage,
                fit.depth - f.depth,
                f.trade != fit.trade,
                f.storeys.abs_diff(fit.storeys),
                f.storeys,
                variant,
            )
        })
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_building_is_a_kind_by_its_storeys_and_its_use() {
        assert_eq!(kind_of(1, false), StreetKind::Low);
        assert_eq!(kind_of(2, false), StreetKind::Low);
        assert_eq!(kind_of(3, false), StreetKind::House);
        assert_eq!(kind_of(3, true), StreetKind::Low, "a three-storey works");
        assert_eq!(kind_of(4, true), StreetKind::House);
        assert_eq!(kind_of(7, false), StreetKind::House);
        assert_eq!(kind_of(8, false), StreetKind::Block);
        assert_eq!(kind_of(15, false), StreetKind::Block);
    }

    #[test]
    fn a_row_takes_its_kinds_deepest_step_that_fits_or_its_shallowest_smaller() {
        assert_eq!(depth_for(StreetKind::House, 30.0), Some((14, 1.0)));
        assert_eq!(depth_for(StreetKind::House, 12.5), Some((11, 1.0)));
        assert_eq!(depth_for(StreetKind::House, 8.0), Some((8, 1.0)));
        let (depth, scale) = depth_for(StreetKind::House, 7.0).expect("drawn smaller");
        assert_eq!(depth, 8);
        assert!(scale < 1.0 && scale * 8.0 <= 7.0, "{scale}");
        assert_eq!(
            depth_for(StreetKind::House, 3.0),
            None,
            "under half its size"
        );
        assert_eq!(depth_for(StreetKind::Low, 6.0), Some((6, 1.0)));
    }

    #[test]
    fn a_street_buildings_seeds_are_its_own_and_one_is_its_entrys() {
        let seed = 0xABCD;
        assert_eq!(variant_seed(seed, 0), seed);
        assert_ne!(variant_seed(seed, 1), seed);
        assert_ne!(variant_seed(seed, 1), variant_seed(seed, 2));
    }

    /// Under half the budget a copy takes its own template; past it, its
    /// fit's first seed; past all of it, the nearest grown at its size on
    /// the ground, or none.
    #[test]
    fn the_templates_keep_to_their_budget() {
        let fit = |storeys: u8, trade: bool| StreetFit::new(16, 14, storeys, trade);
        let want = ("house", fit(5, true), 1, 10_000);
        assert_eq!(template(&[], 4, want), Some(want));
        let half: Vec<StreetKey> = vec![("a", fit(3, false), 0, 10_000); 2];
        assert_eq!(
            template(&half, 4, want),
            Some(("house", fit(5, true), 0, 10_000))
        );
        let full = vec![
            ("house", fit(4, false), 0, 10_000),
            ("house", fit(7, true), 1, 10_000),
            ("house", StreetFit::new(12, 14, 5, true), 0, 10_000),
            ("house", fit(6, true), 0, 5_000),
        ];
        assert_eq!(
            template(&full, 4, want),
            Some(("house", fit(7, true), 1, 10_000)),
            "its trade before its storeys, at its frontage, depth and scale"
        );
        assert_eq!(template(&full, 4, ("block", fit(5, true), 0, 10_000)), None);
        assert_eq!(template(&full, 4, full[2]), Some(full[2]), "its own");
        // Nothing at its frontage: the widest narrower one, never a wider.
        let wide = ("house", StreetFit::new(20, 14, 5, true), 0, 10_000);
        assert_eq!(
            template(&full, 4, wide),
            Some(("house", fit(7, true), 1, 10_000))
        );
        let narrow = ("house", StreetFit::new(10, 14, 5, true), 0, 10_000);
        assert_eq!(template(&full, 4, narrow), None, "none fits its slot");
    }
}
