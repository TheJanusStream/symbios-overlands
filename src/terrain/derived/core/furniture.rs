//! Berlin's street furniture on the walkable ground (#1588): every surveyed
//! lamp, bench, bin, bollard, bus shelter, sign, fountain, advertising
//! column and bike rack as the theme's matching prop, where the theme has
//! one.
//!
//! A prop matches a kind by the words of its slug ([`matches()`]): a street
//! lamp, a gas lamp and a stone lantern are all lamps, a bus shelter and a
//! transit stop both shelters. The room's own props and secondary
//! buildings are searched, by its prosperity and escalation as the lot
//! layer's are - a calm room's benches, a rich room's fountains - and each
//! match is drawn no bigger than its kind's reach ([`reach_of`]). A kind
//! the theme has no prop for is left out: a medieval room has no bike
//! racks. Each item takes one of its kind's matches by its own survey id,
//! so it is the same prop on every visit.
//!
//! The [`MAX_FURNITURE`] nearest the landing are drawn, each turned to its
//! street as
//! the street level reads it, cut at the player's draw distance as ground
//! cover is. A prop whose working side is not its front is turned to suit
//! ([`ARM_ON_X`]): a street lamp reaches its arm over the carriageway. A
//! pole - a lamp, a sign, a bollard - stands on a post of its own; the rest
//! on their boxes.

use std::collections::HashMap;

use bevy::prelude::*;
use geodata::berlin::FurnitureKind;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::seeded_defaults::fnv1a_64;
use crate::terrain::geo::street_level::CoreFurniture;
use crate::terrain::lots::scale_e4;

use super::super::fit::{cache_key, fitted, footing, radius, room_theme, sized_pool};
use super::super::plan::{Grow, Plan, PlannedBuilding, PlannedCopy, Policy, Solid};
use super::super::{SourceId, SourceLayer};
use super::Kept;

/// The most items drawn, nearest first.
pub(crate) const MAX_FURNITURE: usize = 1_500;

/// The most entities the items may be: a merged prop is a few.
pub(crate) const FURNITURE_ENTITY_BUDGET: u32 = 4_000;

/// The salt of the furniture's own picks.
const STREAM_SALT: u64 = 0xF0A7_5EA7_B011_D1E5;

/// The props whose working side is local +X, not the catalogue's -Z front:
/// the street lamp's arm, which reaches over the road. Each is turned a
/// further quarter, so that side faces the street.
pub(crate) const ARM_ON_X: &[&str] = &["street_lamp"];

/// The words of a slug that make a prop one of `kind`: a word with an
/// underscore matches the whole slug, any other one word of it.
pub(crate) const fn words(kind: FurnitureKind) -> &'static [&'static str] {
    match kind {
        FurnitureKind::Lamp => &["lamp", "lantern", "lamppost", "streetlight", "floodlight"],
        FurnitureKind::Bench => &["bench", "seat", "seating"],
        FurnitureKind::Bin => &[
            "bin",
            "bins",
            "trash",
            "litter",
            "waste",
            "dumpster",
            "garbage",
            "recycling",
        ],
        FurnitureKind::Bollard => &["bollard", "bollards", "hitching_post"],
        FurnitureKind::Shelter => &["bus_shelter", "bus_stop", "tram_stop", "transit_stop"],
        FurnitureKind::Sign => &["sign", "signs", "signpost", "waymarker"],
        FurnitureKind::Fountain => &["fountain"],
        FurnitureKind::Column => &["column", "kiosk", "billboard", "advert", "poster"],
        FurnitureKind::BikeRack => &["bike", "bikes", "bicycle", "cycle"],
    }
}

/// Whether `slug` names a prop of `kind` (see [`words`]).
pub(crate) fn matches(kind: FurnitureKind, slug: &str) -> bool {
    words(kind).iter().any(|word| {
        if word.contains('_') {
            slug == *word
        } else {
            slug.split('_').any(|part| part == *word)
        }
    })
}

/// How far from its anchor an item of `kind` reaches at most (m): a match
/// bigger than this is drawn smaller, down to the fit's least.
pub(crate) const fn reach_of(kind: FurnitureKind) -> f32 {
    match kind {
        FurnitureKind::Bollard => 0.8,
        FurnitureKind::Sign | FurnitureKind::Bin => 1.5,
        FurnitureKind::Lamp | FurnitureKind::Bench | FurnitureKind::Column => 2.5,
        FurnitureKind::BikeRack => 3.0,
        FurnitureKind::Shelter | FurnitureKind::Fountain => 6.0,
    }
}

/// What an item of `kind` stands on: a pole on a post its own width, the
/// rest on their boxes.
const fn solid_of(kind: FurnitureKind) -> Solid {
    match kind {
        FurnitureKind::Lamp => Solid::Trunk(0.12),
        FurnitureKind::Sign => Solid::Trunk(0.06),
        FurnitureKind::Bollard => Solid::Trunk(0.1),
        _ => Solid::Box,
    }
}

/// Each kind's matches among the room's props and secondary buildings, each
/// at the scale it is drawn at.
pub(crate) fn matches_for(
    did: &str,
) -> HashMap<FurnitureKind, Vec<(&'static dyn CatalogueEntry, f32)>> {
    let (theme, character) = room_theme(did);
    let pool: Vec<&'static dyn CatalogueEntry> = [StructureRole::Prop, StructureRole::Secondary]
        .into_iter()
        .flat_map(|role| sized_pool(theme, role, character))
        .collect();
    FurnitureKind::ALL
        .into_iter()
        .map(|kind| {
            let found = pool
                .iter()
                .filter(|entry| matches(kind, entry.slug()))
                .filter_map(|&entry| fitted(entry, reach_of(kind)).map(|scale| (entry, scale)))
                .collect();
            (kind, found)
        })
        .collect()
}

/// The furniture's plan (see the module docs), for the room `did`, keeping
/// clear of `kept`, standing on `ground`.
pub(crate) fn plan(
    furniture: &[CoreFurniture],
    did: &str,
    kept: &Kept,
    ground: &dyn Fn(f32, f32) -> f32,
) -> Plan {
    let (_, character) = room_theme(did);
    let found = matches_for(did);
    let mut plan = Plan {
        label: "core furniture",
        policy: Policy {
            near_entities: FURNITURE_ENTITY_BUDGET,
            far_copies: 0,
            cut: true,
        },
        buildings: Vec::new(),
        copies: Vec::new(),
        did: did.to_owned(),
        character,
        seed: fnv1a_64(did) ^ STREAM_SALT,
    };
    let mut by_key: HashMap<(&'static str, i64), usize> = HashMap::new();
    // Nearest the landing first: the cap keeps those.
    let mut nearest: Vec<&CoreFurniture> = furniture.iter().collect();
    nearest.sort_by(|a, b| {
        kept.landing_distance2(a.x, a.z)
            .total_cmp(&kept.landing_distance2(b.x, b.z))
    });
    for item in nearest {
        if plan.copies.len() == MAX_FURNITURE {
            break;
        }
        let Some(choices) = found.get(&item.kind).filter(|choices| !choices.is_empty()) else {
            continue;
        };
        let (entry, scale) = choices[(fnv1a_64(&item.id) % choices.len() as u64) as usize];
        let reach = radius(entry) * scale;
        if !kept.clear(item.x, item.z, reach) {
            continue;
        }
        let key = (entry.slug(), scale_e4(scale));
        let building = *by_key.entry(key).or_insert_with(|| {
            plan.buildings.push(PlannedBuilding::new(
                entry,
                scale,
                Grow::Built,
                cache_key("core/furniture", key.0, key.1),
            ));
            plan.buildings.len() - 1
        });
        let y = footing(item.x, item.z, reach.min(1.0), ground);
        let turn = if ARM_ON_X.contains(&entry.slug()) {
            std::f32::consts::FRAC_PI_2
        } else {
            0.0
        };
        plan.copies.push(PlannedCopy {
            building,
            pose: Transform::from_xyz(item.x, y, item.z)
                .with_rotation(Quat::from_rotation_y(item.yaw + turn)),
            near: true,
            source: SourceId::new(SourceLayer::Furniture, item.id.clone()),
            height: None,
            solid: solid_of(item.kind),
        });
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_defaults::ThemeArchetype;

    #[test]
    fn a_prop_matches_a_kind_by_the_words_of_its_slug() {
        use FurnitureKind::*;
        for (kind, slug) in [
            (Lamp, "street_lamp"),
            (Lamp, "gas_lamp"),
            (Lamp, "stone_lantern"),
            (Lamp, "floodlight"),
            (Bench, "bench"),
            (Bench, "players_bench"),
            (Bin, "recycling_bins"),
            (Bin, "trash_bags"),
            (Bin, "dumpster"),
            (Bollard, "hitching_post"),
            (Shelter, "bus_shelter"),
            (Shelter, "transit_stop"),
            (Sign, "road_sign"),
            (Fountain, "fountain"),
            (Column, "column_drum"),
            (Column, "neon_kiosk"),
            (BikeRack, "bike_rack"),
            (BikeRack, "pelican_bicycle"),
        ] {
            assert!(matches(kind, slug), "{slug} is a {kind:?}");
        }
        for (kind, slug) in [
            (Lamp, "lighthouse"),
            (Lamp, "light_pylon"),
            (Bin, "cabin"),
            (Bollard, "watch_post"),
            (Bollard, "post_apoc_gateway"),
            (Shelter, "backstop"),
            (Shelter, "tarp_shelter"),
            (Sign, "signal_mast"),
            (Sign, "signal_fire"),
            (BikeRack, "skull_rack"),
            (BikeRack, "drying_rack"),
        ] {
            assert!(!matches(kind, slug), "{slug} is no {kind:?}");
        }
    }

    /// A DID of a modern city whose room grows street lamps.
    fn lit_city() -> String {
        (0..10_000)
            .map(|i| format!("did:plc:lit{i}"))
            .find(|did| {
                room_theme(did).0 == ThemeArchetype::ModernCity
                    && matches_for(did)[&FurnitureKind::Lamp]
                        .iter()
                        .any(|(entry, _)| entry.slug() == "street_lamp")
            })
            .expect("a lit city")
    }

    #[test]
    fn the_museumsinsel_furniture_takes_the_themes_matching_props() {
        let level = crate::terrain::geo::tests::museum_level();
        let did = lit_city();
        // A landing off the core's middle, so its order is not the decode's.
        let kept = Kept {
            discs: vec![(150.0, -150.0, super::super::LANDING_CLEAR_M)],
        };
        let plan = plan(&level.furniture, &did, &kept, &|_, _| 30.0);
        assert!(plan.policy.cut && plan.policy.far_copies == 0);
        // Nearest the landing first, which the cap keeps.
        let from_landing =
            |c: &PlannedCopy| kept.landing_distance2(c.pose.translation.x, c.pose.translation.z);
        assert!(
            plan.copies
                .windows(2)
                .all(|w| from_landing(&w[0]) <= from_landing(&w[1]))
        );
        assert!(!plan.copies.is_empty() && plan.copies.len() <= MAX_FURNITURE);
        let by_id: HashMap<&str, &CoreFurniture> =
            level.furniture.iter().map(|f| (&*f.id, f)).collect();
        let mut lamps = 0;
        for copy in &plan.copies {
            let item = by_id[&*copy.source.key];
            let planned = &plan.buildings[copy.building];
            assert!(
                matches(item.kind, planned.entry.slug()),
                "{}",
                planned.entry.slug()
            );
            assert!(radius(planned.entry) * planned.scale <= reach_of(item.kind) + 1e-4);
            assert_eq!(copy.solid, solid_of(item.kind));
            let front = Quat::from_rotation_y(item.yaw) * Vec3::NEG_Z;
            if planned.entry.slug() == "street_lamp" {
                lamps += 1;
                // Its arm (+X) over the carriageway, where its front would
                // have looked.
                let arm = copy.pose.rotation * Vec3::X;
                assert!(arm.distance(front) < 1e-4, "{arm} vs {front}");
            } else {
                let looks = copy.pose.rotation * Vec3::NEG_Z;
                assert!(
                    looks.distance(front) < 1e-4,
                    "{}: {looks} vs {front}",
                    planned.entry.slug()
                );
            }
        }
        assert!(lamps > 100, "{lamps} street lamps");
        // A kind the theme has no prop for is left out, not drawn as another.
        let found = matches_for(&did);
        for item in &level.furniture {
            if found[&item.kind].is_empty() {
                assert!(plan.copies.iter().all(|c| *c.source.key != *item.id));
            }
        }
    }
}
