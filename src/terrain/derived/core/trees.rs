//! Berlin's trees on the walkable ground (#1588): every inventory tree as
//! the catalogue's species nearest its genus, as tall as the real one, on
//! its trunk.
//!
//! The owner chose Berlin's species over the region's own biome: a linden
//! is a broadleaf crown, an oak an oak, a pine a pine, whatever the world
//! grows elsewhere ([`species_for`]). A genus the table does not name is a
//! broadleaf crown, the city's commonest kind.
//!
//! Each species is grown once, at its catalogue size, its growth held to the
//! seeded stands' per-tree budget, and every copy scaled to its tree's
//! height ([`super::super::plan`]). A tree stands on a trunk as thick as its
//! measured girth. The trees nearest the landing are drawn first: past the
//! plan's entity budget a tree is drawn as its voxel shell; past
//! [`MAX_FAR_TREES`] of those, not at all.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::seeded_defaults::fnv1a_64;
use crate::terrain::geo::street_level::CoreTree;

use super::super::fit::RoomScene;
use super::super::plan::{Grow, Plan, PlannedBuilding, PlannedCopy, Policy, Solid};
use super::super::{SourceId, SourceLayer};
use super::Kept;

/// The most entities the near trees may be: a merged tree is a few, its
/// bark and its foliage.
pub(crate) const TREE_ENTITY_BUDGET: u32 = 6_000;

/// The most trees drawn as their shell.
pub(crate) const MAX_FAR_TREES: usize = 2_000;

/// The height a tree the inventory does not measure is drawn at (m).
const DEFAULT_HEIGHT_M: f32 = 10.0;

/// The trunk radius a tree the inventory does not measure stands on (m),
/// and the narrowest and widest any stands on.
const DEFAULT_TRUNK_M: f32 = 0.2;
const TRUNK_RANGE_M: (f32, f32) = (0.08, 0.8);

/// How far round a tree's trunk the record's own places keep clear (m).
const TREE_REACH_M: f32 = 1.0;

/// The broadleaf crown a genus the table does not name is drawn as.
const BROADLEAF: (&str, Option<&str>) = ("lsys_ternary_props", None);

/// The catalogue species and variant nearest each genus Berlin plants, by
/// its Latin name.
const SPECIES: &[(&str, &str, Option<&str>)] = &[
    // Dense oval crowns: lindens, maples, hornbeams, ashes.
    ("Tilia", "lsys_ternary_props", None),
    ("Acer", "lsys_ternary_props", None),
    ("Carpinus", "lsys_ternary_props", None),
    ("Fraxinus", "lsys_ternary_props", None),
    // Broad spreading crowns: planes, chestnuts, beeches, elms.
    ("Platanus", "lsys_sympodial_tree", None),
    ("Aesculus", "lsys_sympodial_tree", None),
    ("Fagus", "lsys_sympodial_tree", None),
    ("Ulmus", "lsys_sympodial_tree", None),
    ("Quercus", "lsys_oak", None),
    ("Betula", "lsys_birch", None),
    // Light feathered crowns: honey locusts, false acacias, pagoda trees.
    ("Gleditsia", "lsys_acacia", None),
    ("Robinia", "lsys_acacia", None),
    ("Sophora", "lsys_acacia", None),
    ("Styphnolobium", "lsys_acacia", None),
    // Small flowering trees: cherries, apples, pears, thorns, rowans.
    ("Prunus", "lsys_flowering_tree", None),
    ("Malus", "lsys_flowering_tree", None),
    ("Pyrus", "lsys_flowering_tree", None),
    ("Crataegus", "lsys_flowering_tree", None),
    ("Sorbus", "lsys_flowering_tree", None),
    ("Corylus", "lsys_hazel", None),
    ("Salix", "lsys_ternary_gravity", None),
    ("Taxus", "lsys_yew", None),
    // Conifers, and the columnar poplars.
    ("Pinus", "lsys_monopodial_tree", Some("pine")),
    ("Larix", "lsys_monopodial_tree", Some("larch_gold")),
    ("Picea", "lsys_monopodial_tree", None),
    ("Abies", "lsys_monopodial_tree", None),
    ("Pseudotsuga", "lsys_monopodial_tree", None),
    ("Populus", "lsys_monopodial_tree", None),
];

/// The catalogue species and variant nearest `genus` (see the module docs).
pub(crate) fn species_for(genus: Option<&str>) -> (&'static str, Option<&'static str>) {
    genus
        .and_then(|genus| {
            SPECIES
                .iter()
                .find(|(name, ..)| name.eq_ignore_ascii_case(genus))
                .map(|&(_, slug, variant)| (slug, variant))
        })
        .unwrap_or(BROADLEAF)
}

/// The trees' plan (see the module docs), for `room`, keeping clear of
/// `kept`, standing on `ground`.
pub(crate) fn plan(
    trees: &[CoreTree],
    room: &RoomScene,
    kept: &Kept,
    ground: &dyn Fn(f32, f32) -> f32,
) -> Plan {
    let (_, character) = room.theme();
    let mut plan = Plan {
        label: "core trees",
        policy: Policy {
            near_entities: TREE_ENTITY_BUDGET,
            far_copies: MAX_FAR_TREES,
            cut: false,
        },
        buildings: Vec::new(),
        copies: Vec::new(),
        did: room.did.clone(),
        character,
        seed: room.seed,
    };
    // Each species looked up once: `None` where the catalogue lacks it.
    let mut by_species: HashMap<(&'static str, Option<&'static str>), Option<usize>> =
        HashMap::new();
    for tree in trees {
        if !kept.clear(tree.x, tree.z, TREE_REACH_M) {
            continue;
        }
        let species = species_for(tree.genus.as_deref());
        let found = *by_species.entry(species).or_insert_with(|| {
            let entry = crate::catalogue::by_slug(species.0)?;
            let key = match species.1 {
                Some(variant) => format!("core/tree/{}#{variant}", species.0),
                None => format!("core/tree/{}", species.0),
            };
            plan.buildings.push(PlannedBuilding::new(
                entry,
                1.0,
                Grow::Plant { variant: species.1 },
                key,
            ));
            Some(plan.buildings.len() - 1)
        });
        let Some(building) = found else {
            continue;
        };
        let trunk = tree.girth.map_or(DEFAULT_TRUNK_M, |girth_cm| {
            (girth_cm / 100.0 / std::f32::consts::TAU).clamp(TRUNK_RANGE_M.0, TRUNK_RANGE_M.1)
        });
        // Each tree turned its own way, the same way on every visit.
        let yaw = (fnv1a_64(&tree.id) % 3600) as f32 / 3600.0 * std::f32::consts::TAU;
        plan.copies.push(PlannedCopy {
            building,
            pose: Transform::from_xyz(tree.x, ground(tree.x, tree.z), tree.z)
                .with_rotation(Quat::from_rotation_y(yaw)),
            near: true,
            source: SourceId::new(SourceLayer::Tree, tree.id.clone()),
            height: Some(tree.height.unwrap_or(DEFAULT_HEIGHT_M)),
            solid: Solid::Trunk(trunk),
        });
    }
    // Nearest the landing first: the near budget keeps those.
    let from_landing =
        |c: &PlannedCopy| kept.landing_distance2(c.pose.translation.x, c.pose.translation.z);
    plan.copies
        .sort_by(|a, b| from_landing(a).total_cmp(&from_landing(b)));
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::StructureRole;

    #[test]
    fn each_genus_grows_as_its_nearest_species() {
        assert_eq!(species_for(Some("Tilia")), ("lsys_ternary_props", None));
        assert_eq!(species_for(Some("quercus")), ("lsys_oak", None));
        assert_eq!(
            species_for(Some("Pinus")),
            ("lsys_monopodial_tree", Some("pine"))
        );
        assert_eq!(species_for(Some("Ginkgo")), BROADLEAF);
        assert_eq!(species_for(None), BROADLEAF);
        // Every species is a plant the catalogue has, in every variant
        // the table names.
        for &(genus, slug, variant) in SPECIES.iter().chain([&("", BROADLEAF.0, None)]) {
            let entry =
                crate::catalogue::by_slug(slug).unwrap_or_else(|| panic!("{genus}: no {slug}"));
            assert_eq!(entry.role(), StructureRole::Plant, "{slug}");
            if let Some(variant) = variant {
                assert!(
                    entry.variants().iter().any(|v| v.name == variant),
                    "{slug} has no {variant}"
                );
            }
        }
    }

    #[test]
    fn the_museumsinsel_trees_stand_as_tall_as_measured_on_their_trunks() {
        let level = crate::terrain::geo::tests::museum_level();
        // A landing off the core's middle, so its order is not the decode's.
        let kept = Kept {
            discs: vec![(-150.0, 150.0, super::super::LANDING_CLEAR_M)],
        };
        let plan = plan(
            &level.trees,
            &RoomScene::for_did("did:plc:trees"),
            &kept,
            &|_, _| 30.0,
        );
        let mut standing: Vec<&CoreTree> = level
            .trees
            .iter()
            .filter(|t| kept.clear(t.x, t.z, TREE_REACH_M))
            .collect();
        // Nearest the landing first, as the plan keeps them.
        standing.sort_by(|a, b| {
            kept.landing_distance2(a.x, a.z)
                .total_cmp(&kept.landing_distance2(b.x, b.z))
        });
        assert_eq!(plan.copies.len(), standing.len());
        assert!(plan.buildings.len() <= SPECIES.len() + 1, "one per species");
        for (copy, tree) in plan.copies.iter().zip(standing) {
            assert_eq!(
                copy.source,
                SourceId::new(SourceLayer::Tree, tree.id.clone())
            );
            assert_eq!(copy.height, Some(tree.height.unwrap_or(DEFAULT_HEIGHT_M)));
            assert_eq!(
                (copy.pose.translation.x, copy.pose.translation.z),
                (tree.x, tree.z)
            );
            let Solid::Trunk(r) = copy.solid else {
                panic!("a trunk");
            };
            assert!((TRUNK_RANGE_M.0..=TRUNK_RANGE_M.1).contains(&r));
            if let Some(girth) = tree.girth.filter(|g| (60.0..400.0).contains(g)) {
                let measured = girth / 100.0 / std::f32::consts::TAU;
                assert!((r - measured).abs() < 1e-4, "{r} for {girth} cm");
            }
            let species = species_for(tree.genus.as_deref());
            let building = &plan.buildings[copy.building];
            assert_eq!(building.entry.slug(), species.0);
            assert_eq!(building.grow, Grow::Plant { variant: species.1 });
        }
    }
}
