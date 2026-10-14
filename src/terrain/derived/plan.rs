//! A plan of Berlin's walkable ground (#1587, #1588): the distinct
//! buildings, trees and props the planner ([`super::core`]) draws a square
//! with, each a catalogue entry at a drawn scale and grown one way, and the
//! copies of them its footprints and inventory points stand. The seeder
//! writes a plan into the record as instance sets (#1647, [`super::seed`]).

use bevy::prelude::*;

use crate::catalogue::CatalogueEntry;
use crate::pds::{Generator, GeneratorKind};

/// How one distinct building is grown from its catalogue entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Grow {
    /// As the lot layer grows its buildings and props: finished and ruined
    /// by the room's prosperity and escalation, drawn at its scale.
    Built,
    /// A street building (#1598) at `fit`, the fit its footprint gives it:
    /// its kind's one generator drawn at the fit's grammar values (#1646,
    /// [`crate::catalogue::items::street::StreetSpec::fit_values`]), with
    /// its `variant`th seed
    /// ([`super::streets::variant_seed`]); then as [`Self::Built`], but
    /// ruined in place: it stands flush in a row
    /// ([`crate::terrain::lots::grow_flush`]).
    Street {
        fit: crate::catalogue::items::street::StreetFit,
        variant: u8,
    },
    /// A plant, re-skinned by its named variant, its growth held to the
    /// seeded stands' per-tree budget, drawn at its catalogue size.
    Plant { variant: Option<&'static str> },
}

/// What a walker meets of a building's copies: its set's solid
/// ([`crate::pds::InstanceSolid`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Solid {
    /// Its building's voxel shell: a building's.
    Shell,
    /// The box its building fills: a prop's.
    Box,
    /// Its trunk, under a crown a walker passes through: a tree's.
    Trunk,
}

/// One distinct building of a plan: a catalogue entry at a drawn scale.
pub(crate) struct PlannedBuilding {
    pub entry: &'static dyn CatalogueEntry,
    pub scale: f32,
    pub grow: Grow,
    pub solid: Solid,
}

impl PlannedBuilding {
    /// How far its copies reach from where they stand, turned any way (m):
    /// a street building's lot at its fit (#1598), any other entry's
    /// clearance, at its scale.
    pub(crate) fn reach(&self) -> f32 {
        match self.grow {
            Grow::Street { fit, .. } => fit.reach_m() * self.scale,
            Grow::Built | Grow::Plant { .. } => super::fit::radius(self.entry) * self.scale,
        }
    }

    pub(crate) fn new(
        entry: &'static dyn CatalogueEntry,
        scale: f32,
        grow: Grow,
        solid: Solid,
    ) -> Self {
        PlannedBuilding {
            entry,
            scale,
            grow,
            solid,
        }
    }
}

/// One copy of a plan's building.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlannedCopy {
    /// Which of the plan's buildings.
    pub building: usize,
    /// Where it stands on the ground.
    pub at: Vec3,
    /// Its turn about +Y (radians, as [`Quat::from_rotation_y`] takes it):
    /// written into the record as the planner worked it out, never read
    /// back off a rotation's sines, so every client writes the same turn
    /// (#1132).
    pub yaw: f32,
    /// The height it is drawn at (m), its building scaled to it; `None`
    /// draws it as grown.
    pub height: Option<f32>,
}

impl PlannedCopy {
    /// Where it stands, turned.
    #[cfg(test)]
    pub(crate) fn pose(&self) -> Transform {
        Transform::from_translation(self.at).with_rotation(Quat::from_rotation_y(self.yaw))
    }
}

/// What a plan draws, nearest the landing first.
pub(crate) struct Plan {
    pub buildings: Vec<PlannedBuilding>,
    pub copies: Vec<PlannedCopy>,
    /// What the buildings are grown for: the room's DID, its prosperity and
    /// escalation, and the plan's seed.
    pub did: String,
    pub character: (f32, f32),
    pub seed: u64,
}

/// A plant as the seeded stands grow one: its catalogue tree, re-skinned by
/// `variant`, its L-system's growth stepped down until its expansion fits
/// the stands' per-tree budget.
pub(crate) fn grow_plant(
    entry: &dyn CatalogueEntry,
    did: &str,
    variant: Option<&'static str>,
) -> Generator {
    let mut tree = entry.build(did);
    if let GeneratorKind::LSystem {
        materials,
        source_code,
        finalization_code,
        iterations,
        seed,
        angle,
        step,
        width,
        elasticity,
        tropism,
        ..
    } = &mut tree.kind
    {
        if let Some(variant) = variant {
            crate::catalogue::items::plants::variant::apply_named(
                entry.variants(),
                variant,
                materials,
            );
        }
        let budget = crate::seeded_defaults::room::build::TREE_ENTITY_BUDGET;
        while *iterations > 2
            && crate::world_builder::lsystem::lsystem_entity_estimate(
                source_code,
                finalization_code,
                *iterations,
                *seed,
                *angle,
                *step,
                *width,
                *elasticity,
                *tropism,
                entry.slug(),
            )
            .is_some_and(|entities| entities > budget)
        {
            *iterations -= 1;
        }
    }
    tree
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A street building is ruined in place (#1598): a fought-over room
    /// sinks and weathers it, but neither leans nor shrinks it, so it
    /// stands flush against its neighbours as it was drawn to - nor drops
    /// rubble in its ground storey (#1646).
    #[test]
    fn a_street_copy_is_ruined_in_place() {
        let entry = crate::catalogue::by_slug("city_street_house").expect("registered");
        let spec = entry.street().expect("a street building");
        let built = spec.generator();
        for seed in 0..8u64 {
            let tree =
                crate::terrain::lots::grow_flush(entry, built.clone(), seed, (0.5, 0.95), 1.0);
            assert_eq!(tree.transform.scale, built.transform.scale, "seed {seed}");
            assert_eq!(
                tree.transform.rotation, built.transform.rotation,
                "seed {seed}"
            );
            assert!(
                tree.transform.translation.0[1] < built.transform.translation.0[1],
                "seed {seed}: sunk"
            );
            assert_eq!(
                tree.children.len(),
                built.children.len(),
                "seed {seed}: no rubble"
            );
        }
    }

    /// A plant is grown as the seeded stands grow one: re-skinned by its
    /// variant, and its growth stepped down until it fits their budget.
    #[test]
    fn a_plant_is_grown_within_the_stands_budget() {
        let entry = crate::catalogue::by_slug("lsys_monopodial_tree").expect("registered");
        let plain = grow_plant(entry, "did:plc:plant", None);
        let pine = grow_plant(entry, "did:plc:plant", Some("pine"));
        let (
            GeneratorKind::LSystem {
                materials: plain_materials,
                ..
            },
            GeneratorKind::LSystem {
                materials,
                source_code,
                finalization_code,
                iterations,
                seed,
                angle,
                step,
                width,
                elasticity,
                tropism,
                ..
            },
        ) = (&plain.kind, &pine.kind)
        else {
            panic!("an L-system");
        };
        assert_ne!(plain_materials, materials, "the pine re-skins it");
        let entities = crate::world_builder::lsystem::lsystem_entity_estimate(
            source_code,
            finalization_code,
            *iterations,
            *seed,
            *angle,
            *step,
            *width,
            *elasticity,
            *tropism,
            entry.slug(),
        );
        assert!(
            *iterations == 2
                || entities
                    .is_some_and(|n| n <= crate::seeded_defaults::room::build::TREE_ENTITY_BUDGET),
            "{iterations} iterations, {entities:?} entities"
        );
    }
}
