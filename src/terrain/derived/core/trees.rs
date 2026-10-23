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
//! height ([`super::super::plan`]). A walker meets a tree's trunk and passes
//! through its crown ([`crate::pds::InstanceSolid::Trunk`]). The trees
//! nearest the landing come first, so a cap on the copies a room holds
//! keeps those.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::seeded_defaults::fnv1a_64;
use crate::terrain::geo::street_level::CoreTree;

use super::super::fit::RoomScene;
use super::super::plan::{Grow, Plan, PlannedBuilding, PlannedCopy, Solid};
use super::Kept;

/// The height a tree the inventory does not measure is drawn at (m).
const DEFAULT_HEIGHT_M: f32 = 10.0;

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
    // Broad spreading crowns, the London plane since #1764: planes,
    // chestnuts, beeches, elms.
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
    // Small flowering trees, the cherry in blossom since #1763: cherries
    // pink, apples, pears and thorns white, rowans and whitebeams - whose
    // small white clusters do not cloud a crown - in summer leaf.
    ("Prunus", "lsys_flowering_tree", None),
    ("Malus", "lsys_flowering_tree", Some("white")),
    ("Pyrus", "lsys_flowering_tree", Some("white")),
    ("Crataegus", "lsys_flowering_tree", Some("white")),
    ("Sorbus", "lsys_flowering_tree", Some("summer")),
    ("Corylus", "lsys_hazel", None),
    ("Salix", "lsys_ternary_gravity", None),
    ("Taxus", "lsys_yew", None),
    // Columnar poplars: Lombardy and Berlin poplars, the street's poplars
    // (#1713). They drew as the spruce, a dark cone twice as wide.
    ("Populus", "lsys_poplar", None),
    // Conifers.
    ("Pinus", "lsys_monopodial_tree", Some("pine")),
    ("Larix", "lsys_monopodial_tree", Some("larch_gold")),
    ("Picea", "lsys_monopodial_tree", None),
    ("Abies", "lsys_monopodial_tree", None),
    ("Pseudotsuga", "lsys_monopodial_tree", None),
];

/// Each species' grown height (m), by its catalogue slug (#1647): its tree
/// grown as the seeded stands grow it ([`super::super::plan::grow_plant`];
/// a re-skin changes its materials alone) and measured in a copy's frame
/// as the spawner meshes it - what a seeded copy's scale is its tree's
/// height over. Pinned rather than measured at run time: a seeded record
/// must come out the same on every client, and an L-system's turtle turns
/// through sines and cosines that are not. The test
/// `the_grown_heights_are_what_the_trees_grow_to` holds it to the trees.
pub(crate) const GROWN_HEIGHT_M: &[(&str, f32)] = &[
    ("lsys_acacia", 6.12),
    ("lsys_birch", 9.21),
    ("lsys_flowering_tree", 6.03),
    ("lsys_hazel", 3.44),
    ("lsys_monopodial_tree", 9.20),
    ("lsys_oak", 17.05),
    ("lsys_poplar", 14.56),
    ("lsys_sympodial_tree", 14.99),
    ("lsys_ternary_gravity", 7.18),
    ("lsys_ternary_props", 13.71),
    ("lsys_yew", 9.50),
];

/// The height `slug`'s tree grows to ([`GROWN_HEIGHT_M`]), where pinned.
pub(crate) fn grown_height(slug: &str) -> Option<f32> {
    GROWN_HEIGHT_M
        .iter()
        .find(|(name, _)| *name == slug)
        .map(|&(_, height)| height)
}

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
            plan.buildings.push(PlannedBuilding::new(
                entry,
                1.0,
                Grow::Plant { variant: species.1 },
                Solid::Trunk,
            ));
            Some(plan.buildings.len() - 1)
        });
        let Some(building) = found else {
            continue;
        };
        // Each tree turned its own way, the same way on every visit.
        let yaw = (fnv1a_64(&tree.id) % 3600) as f32 / 3600.0 * std::f32::consts::TAU;
        plan.copies.push(PlannedCopy {
            building,
            at: Vec3::new(tree.x, ground(tree.x, tree.z), tree.z),
            yaw,
            height: Some(tree.height.unwrap_or(DEFAULT_HEIGHT_M)),
        });
    }
    // Nearest the landing first: a cap keeps those.
    let from_landing = |c: &PlannedCopy| kept.landing_distance2(c.at.x, c.at.z);
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
        assert_eq!(species_for(Some("Populus")), ("lsys_poplar", None));
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
    fn the_museumsinsel_trees_stand_as_tall_as_measured() {
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
            assert_eq!(copy.height, Some(tree.height.unwrap_or(DEFAULT_HEIGHT_M)));
            assert_eq!((copy.at.x, copy.at.z), (tree.x, tree.z));
            let species = species_for(tree.genus.as_deref());
            let building = &plan.buildings[copy.building];
            assert_eq!(building.entry.slug(), species.0);
            assert_eq!(building.grow, Grow::Plant { variant: species.1 });
            assert_eq!(building.solid, Solid::Trunk);
        }
    }

    /// The height `tree` grows to (m): its box's ([`grown_box_of`]).
    fn grown_height_of(tree: &crate::pds::Generator) -> f32 {
        let (lo, hi) = grown_box_of(tree);
        hi.y - lo.y
    }

    /// The box of the meshes `tree`'s L-system nodes grow, as the spawner
    /// meshes them, through their nodes' frames from the copy's.
    fn grown_box_of(tree: &crate::pds::Generator) -> (Vec3, Vec3) {
        use bevy::math::Affine3A;
        use bevy::mesh::VertexAttributeValues;
        fn walk(node: &crate::pds::Generator, frame: Affine3A, span: &mut (Vec3, Vec3)) {
            let frame = frame * Transform::from(&node.transform).compute_affine();
            if let crate::pds::GeneratorKind::LSystem {
                source_code,
                finalization_code,
                iterations,
                seed,
                angle,
                step,
                width,
                elasticity,
                tropism,
                mesh_resolution,
                prop_mappings,
                prop_scale,
                ..
            } = &node.kind
            {
                let meshes = crate::world_builder::lsystem::build_lsystem_geometry(
                    source_code,
                    finalization_code,
                    *iterations,
                    *seed,
                    *angle,
                    *step,
                    *width,
                    *elasticity,
                    *tropism,
                    *mesh_resolution,
                    prop_mappings,
                    *prop_scale,
                    "grown_height",
                )
                .expect("the tree grows");
                for (_, mesh) in meshes {
                    if let Some(VertexAttributeValues::Float32x3(points)) =
                        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                    {
                        for point in points {
                            let p = frame.transform_point3(Vec3::from(*point));
                            span.0 = span.0.min(p);
                            span.1 = span.1.max(p);
                        }
                    }
                }
            }
            for child in &node.children {
                walk(child, frame, span);
            }
        }
        let mut span = (Vec3::INFINITY, Vec3::NEG_INFINITY);
        walk(tree, Affine3A::IDENTITY, &mut span);
        span
    }

    /// #1713: a poplar in a Berlin street is a column, as the street's
    /// Lombardy and Berlin poplars are - not the spruce it drew as, a cone
    /// 9.2 m tall and 5.5 m across. Grown as a seeded copy grows it, it
    /// stands more than four times as tall as it is wide either way.
    #[test]
    fn berlins_poplars_stand_as_columns() {
        let (slug, variant) = species_for(Some("Populus"));
        let entry = crate::catalogue::by_slug(slug).expect("a catalogue species");
        let (lo, hi) = grown_box_of(&super::super::super::plan::grow_plant(
            entry,
            "did:plc:poplar",
            variant,
        ));
        let size = hi - lo;
        assert!(
            size.y > 4.0 * size.x.max(size.z),
            "{slug} grows {:.2} m tall and {:.2} x {:.2} m across",
            size.y,
            size.x,
            size.z
        );
    }

    /// #1647: every species Berlin's trees are drawn as has its grown height
    /// pinned, within 1 % of what its tree grows to - grown the same for any
    /// room, as a seeded record must be.
    #[test]
    fn the_grown_heights_are_what_the_trees_grow_to() {
        let mut slugs: Vec<&str> = SPECIES.iter().map(|&(_, slug, _)| slug).collect();
        slugs.push(BROADLEAF.0);
        slugs.sort_unstable();
        slugs.dedup();
        let mut faults = Vec::new();
        let mut measured = Vec::new();
        for slug in slugs {
            let entry = crate::catalogue::by_slug(slug).expect("a catalogue species");
            let tree = super::super::super::plan::grow_plant(entry, "did:plc:one", None);
            let other = super::super::super::plan::grow_plant(entry, "did:plc:two", None);
            assert_eq!(
                serde_json::to_value(&tree).ok(),
                serde_json::to_value(&other).ok(),
                "{slug} grows the same for any room"
            );
            let height = grown_height_of(&tree);
            measured.push(format!("(\"{slug}\", {height:.2}),"));
            match grown_height(slug) {
                Some(pinned) if ((pinned - height) / height).abs() <= 0.01 => {}
                pinned => faults.push(format!("{slug}: pinned {pinned:?}, grows to {height:.3}")),
            }
        }
        assert!(
            faults.is_empty(),
            "{}\nmeasured:\n{}",
            faults.join("\n"),
            measured.join("\n")
        );
    }
}
