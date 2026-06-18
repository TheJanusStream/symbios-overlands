//! Catalogue item registry. Entries live in per-theme subfolders
//! (`ancient`, `medieval`, …) for structures and per-role subfolders
//! (`plants`, `patterns`, `tools`) for everything else. Adding a new
//! entry is three steps: drop the file in the right subfolder, declare
//! it in that subfolder's `mod.rs`, and append `&path::Type` to
//! [`ENTRIES`].
//!
//! The flat [`ENTRIES`] list with categorisation via
//! [`super::CatalogueCategory`] (itself derived from
//! [`super::StructureRole`]) lets us re-bucket entries in the UI without
//! moving files — see the parent module's docstring for the rationale.

use super::CatalogueEntry;

pub mod ancient;
pub mod civic;
pub mod cyberpunk;
pub mod feudal_japan;
pub mod industrial_park;
pub mod medieval;
pub mod mesoamerican;
pub mod modern_city;
pub mod nordic;
pub mod patterns;
pub mod plants;
pub mod rural_farmland;
pub mod suburban;
pub mod tools;

mod util;

#[cfg(test)]
mod shape_grammar_test;

/// The full set of catalogue entries the client ships with. Order is
/// preserved by the UI for display, so think of this as the
/// presentation order within each section.
pub const ENTRIES: &[&dyn CatalogueEntry] = &[
    // Buildings — architectural entries (shape-grammar and
    // primitive-built), grouped into per-theme subfolders.
    &ancient::villa::Villa,
    &medieval::medieval_castle::MedievalCastle,
    &medieval::watchtower::Watchtower,
    &ancient::ruined_temple::RuinedTemple,
    &ancient::lighthouse::Lighthouse,
    &ancient::stone_circle::StoneCircle,
    &ancient::ziggurat::Ziggurat,
    &ancient::observatory::Observatory,
    // Buildings — Cyberpunk theme (landmark + secondaries + props).
    &cyberpunk::neon_megatower::NeonMegatower,
    &cyberpunk::data_spire::DataSpire,
    &cyberpunk::arcade_block::ArcadeBlock,
    &cyberpunk::holo_billboard::HoloBillboard,
    &cyberpunk::parking_stack::ParkingStack,
    &cyberpunk::neon_kiosk::NeonKiosk,
    &cyberpunk::drone_perch::DronePerch,
    &cyberpunk::cable_arch::CableArch,
    // Buildings — Cyberpunk poor (undercity) variants, prosperity Poor.
    &cyberpunk::scrap_shanty::ScrapShanty,
    &cyberpunk::container_stack::ContainerStack,
    &cyberpunk::tarp_shelter::TarpShelter,
    &cyberpunk::ewaste_pile::EwastePile,
    &cyberpunk::busted_terminal::BustedTerminal,
    // Buildings — Nordic theme (landmark + secondaries + props).
    &nordic::mead_hall::MeadHall,
    &nordic::boathouse::Boathouse,
    &nordic::signal_beacon::SignalBeacon,
    &nordic::rune_stones::RuneStones,
    &nordic::longship::Longship,
    &nordic::shield_rack::ShieldRack,
    &nordic::drying_rack::DryingRack,
    &nordic::totem_pole::TotemPole,
    // Buildings — Nordic poor (croft) variants, prosperity Poor.
    &nordic::turf_house::TurfHouse,
    &nordic::sod_shelter::SodShelter,
    &nordic::wood_pile::WoodPile,
    // Buildings — Feudal Japan theme (landmark + secondaries + props).
    &feudal_japan::pagoda::Pagoda,
    &feudal_japan::torii_gate::ToriiGate,
    &feudal_japan::tea_house::TeaHouse,
    &feudal_japan::dojo::Dojo,
    &feudal_japan::stone_lantern::StoneLantern,
    &feudal_japan::koi_pond::KoiPond,
    &feudal_japan::bamboo_fence::BambooFence,
    &feudal_japan::bonsai::Bonsai,
    // Buildings — Feudal Japan poor (farmstead) variants, prosperity Poor.
    &feudal_japan::minka::Minka,
    &feudal_japan::rice_shed::RiceShed,
    &feudal_japan::straw_bales::StrawBales,
    // Buildings — Mesoamerican theme (landmark + secondaries + props).
    &mesoamerican::step_pyramid::StepPyramid,
    &mesoamerican::ball_court::BallCourt,
    &mesoamerican::shrine::Shrine,
    &mesoamerican::stela::Stela,
    &mesoamerican::skull_rack::SkullRack,
    &mesoamerican::idol::Idol,
    &mesoamerican::fire_bowl::FireBowl,
    &mesoamerican::calendar_stone::CalendarStone,
    // Buildings — Mesoamerican poor (commoner) variants, prosperity Poor.
    &mesoamerican::adobe_hut::AdobeHut,
    &mesoamerican::maize_granary::MaizeGranary,
    &mesoamerican::clay_pots::ClayPots,
    // Buildings — Modern City theme (landmark + secondaries + props).
    &modern_city::glass_skyscraper::GlassSkyscraper,
    &modern_city::office_block::OfficeBlock,
    &modern_city::parking_garage::ParkingGarage,
    &modern_city::transit_stop::TransitStop,
    &modern_city::street_lamp::StreetLamp,
    &modern_city::traffic_light::TrafficLight,
    &modern_city::parked_car::ParkedCar,
    &modern_city::dumpster::Dumpster,
    // Buildings — Modern City poor (inner-city) variants, prosperity Poor.
    &modern_city::tenement::Tenement,
    &modern_city::corner_store::CornerStore,
    &modern_city::trash_bags::TrashBags,
    // Buildings — Suburban theme (landmark + secondaries + props).
    &suburban::community_center::CommunityCenter,
    &suburban::suburban_house::SuburbanHouse,
    &suburban::detached_garage::DetachedGarage,
    &suburban::mini_mart::MiniMart,
    &suburban::picket_fence::PicketFence,
    &suburban::mailbox::Mailbox,
    &suburban::minivan::Minivan,
    &suburban::swing_set::SwingSet,
    // Buildings — Suburban poor (trailer-lot) variants, prosperity Poor.
    &suburban::trailer_home::TrailerHome,
    &suburban::carport::Carport,
    &suburban::yard_junk::YardJunk,
    // Buildings — Rural/Farmland theme (landmark + secondaries + props).
    &rural_farmland::barn::Barn,
    &rural_farmland::farmhouse::Farmhouse,
    &rural_farmland::grain_silo::GrainSilo,
    &rural_farmland::windmill::Windmill,
    &rural_farmland::greenhouse::Greenhouse,
    &rural_farmland::tractor::Tractor,
    &rural_farmland::hay_bales::HayBales,
    &rural_farmland::scarecrow::Scarecrow,
    &rural_farmland::rail_fence::RailFence,
    // Buildings — Rural/Farmland poor (hardscrabble) variants, prosperity Poor.
    &rural_farmland::homestead_shack::HomesteadShack,
    &rural_farmland::pole_barn::PoleBarn,
    &rural_farmland::farm_junk::FarmJunk,
    // Buildings — Industrial Park theme (landmark + secondaries + props).
    &industrial_park::factory::Factory,
    &industrial_park::cooling_tower::CoolingTower,
    &industrial_park::loading_dock::LoadingDock,
    &industrial_park::tank_farm::TankFarm,
    &industrial_park::shipping_containers::ShippingContainers,
    &industrial_park::pipe_run::PipeRun,
    &industrial_park::pallet_stack::PalletStack,
    &industrial_park::floodlight::Floodlight,
    // Buildings — Industrial Park poor (derelict) variants, prosperity Poor.
    &industrial_park::derelict_shed::DerelictShed,
    &industrial_park::rusted_tank::RustedTank,
    &industrial_park::scrap_heap::ScrapHeap,
    // Buildings — cross-theme socio-political props (Prop role, tagged
    // with every theme but gated to a prosperity / escalation tier band;
    // see crate::catalogue::items::civic).
    &civic::shanty::Shanty,
    &civic::scrap_pile::ScrapPile,
    &civic::laundry_line::LaundryLine,
    &civic::barrel_fire::BarrelFire,
    &civic::fountain::Fountain,
    &civic::statue::Statue,
    &civic::banner::Banner,
    &civic::planter::Planter,
    &civic::barricade::Barricade,
    &civic::sandbag_wall::SandbagWall,
    &civic::watch_post::WatchPost,
    &civic::wreckage::Wreckage,
    &civic::bench::Bench,
    &civic::garden_bed::GardenBed,
    &civic::lantern::Lantern,
    &civic::market_stall::MarketStall,
    // Plants — L-system tree entries.
    &plants::lsys_monopodial_tree::MonopodialTree,
    &plants::lsys_sympodial_tree::SympodialTree,
    &plants::lsys_ternary_gravity::TernaryGravityTree,
    &plants::lsys_ternary_props::TernaryPropsTree,
    // Patterns — abstract L-system / ABOP demos.
    &patterns::lsys_branching::BranchingPattern,
    &patterns::lsys_koch_island::QuadraticKochIsland,
    &patterns::lsys_sierpinski::SierpinskiGasket,
    // Tools — utility items personalised at build time.
    &tools::my_teleporter::MyTeleporter,
];

/// Resolve a slug to its entry. Returns `None` if the slug doesn't
/// match any current entry — the drop handler treats that as a
/// silently-dropped stale drag (renaming a slug between sessions, or
/// a record referencing a removed entry, both land here).
pub fn by_slug(slug: &str) -> Option<&'static dyn CatalogueEntry> {
    ENTRIES.iter().copied().find(|e| e.slug() == slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_unique() {
        let mut slugs: Vec<&str> = ENTRIES.iter().map(|e| e.slug()).collect();
        slugs.sort();
        let len_before = slugs.len();
        slugs.dedup();
        assert_eq!(
            len_before,
            slugs.len(),
            "duplicate slug in catalogue ENTRIES — slugs must be unique"
        );
    }

    #[test]
    fn by_slug_resolves_every_entry() {
        for entry in ENTRIES {
            let resolved = by_slug(entry.slug());
            assert!(resolved.is_some(), "by_slug failed for {}", entry.slug());
        }
        assert!(by_slug("not-a-real-entry").is_none());
    }

    #[test]
    fn settlement_structures_are_themed() {
        use crate::catalogue::StructureRole::{Landmark, Prop, Secondary};
        for e in ENTRIES {
            if matches!(e.role(), Landmark | Secondary | Prop) {
                assert!(
                    !e.themes().is_empty(),
                    "entry {} has a settlement role but no themes() — the deriver \
                     would never place it",
                    e.slug()
                );
            }
        }
    }

    #[test]
    fn categories_unchanged_after_role_migration() {
        use crate::catalogue::CatalogueCategory::*;
        let count = |c| ENTRIES.iter().filter(|e| e.category() == c).count();
        // Deriving category() from role() must keep every entry in its
        // expected section. 8 ancient/medieval + 8 cyberpunk + 5 cyberpunk
        // poor + 8 nordic + 3 nordic poor + 8 feudal japan + 3 feudal japan
        // poor + 8 mesoamerican + 3 mesoamerican poor + 8 modern city + 3
        // modern city poor + 8 suburban + 3 suburban poor + 9 rural farmland
        // + 3 rural farmland poor + 8 industrial park + 3 industrial park poor
        // + 16 civic cross-theme props = 115 buildings.
        assert_eq!(count(Buildings), 115);
        assert_eq!(count(Plants), 4);
        assert_eq!(count(Patterns), 3);
        assert_eq!(count(Tools), 1);
    }
}
