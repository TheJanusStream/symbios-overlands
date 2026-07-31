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

pub mod alien_monolithic;
pub mod alien_organic;
pub mod ancient;
pub mod civic;
pub mod civic_campus;
pub mod coastal_resort;
pub mod cyberpunk;
pub mod fantasy;
pub mod feudal_japan;
pub mod gothic_horror;
pub mod industrial_park;
pub mod medieval;
pub mod mesoamerican;
pub mod modern_city;
pub mod nordic;
pub mod patterns;
pub mod pirate;
pub mod plants;
pub mod post_apoc;
pub mod roadside;
pub mod rural_farmland;
pub mod solarpunk;
pub mod space_outpost;
pub mod sports_rec;
pub mod steampunk;
pub mod suburban;
pub mod tools;
pub mod wild_west;

pub mod foundation;
pub(crate) mod fx;
pub mod gateway_fit;
pub mod measure;
mod util;

#[cfg(test)]
mod shape_grammar_test;

/// The full set of catalogue entries the client ships with. Order is
/// preserved by the UI for display, so think of this as the
/// presentation order within each section.
pub const ENTRIES: &[&dyn CatalogueEntry] = &[
    // Buildings — Ancient/Classical theme (shape-grammar + primitive). Also
    // the settlement fallback theme, so it carries the deepest roster.
    &ancient::villa::Villa,
    &ancient::ruined_temple::RuinedTemple,
    &ancient::lighthouse::Lighthouse,
    &ancient::stone_circle::StoneCircle,
    &ancient::ziggurat::Ziggurat,
    &ancient::observatory::Observatory,
    &ancient::colonnade::Colonnade,
    &ancient::amphitheatre::Amphitheatre,
    &ancient::bathhouse::Bathhouse,
    &ancient::column_drum::ColumnDrum,
    &ancient::urn::Urn,
    &ancient::statue_plinth::StatuePlinth,
    &ancient::brazier::Brazier,
    // Buildings — Ancient/Classical poor (mudbrick) variants, prosperity Poor.
    &ancient::mudbrick_hut::MudbrickHut,
    &ancient::ruined_wall::RuinedWall,
    // Buildings — Medieval theme (landmark + secondaries + props).
    &medieval::medieval_castle::MedievalCastle,
    &medieval::watchtower::Watchtower,
    &medieval::chapel::Chapel,
    &medieval::blacksmith::Blacksmith,
    &medieval::market_hall::MarketHall,
    &medieval::well_house::WellHouse,
    &medieval::handcart::Handcart,
    &medieval::barrel_stack::BarrelStack,
    &medieval::trade_stall::TradeStall,
    &medieval::banner_pole::BannerPole,
    // Buildings — Medieval poor (cottar) variants, prosperity Poor.
    &medieval::wattle_hovel::WattleHovel,
    &medieval::lean_to::LeanTo,
    &medieval::kindling_pile::KindlingPile,
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
    &nordic::stave_church::StaveChurch,
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
    &modern_city::rowhouse_terrace::RowhouseTerrace,
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
    &industrial_park::sawtooth_mill::SawtoothMill,
    &industrial_park::pipe_run::PipeRun,
    &industrial_park::pallet_stack::PalletStack,
    &industrial_park::floodlight::Floodlight,
    // Buildings — Industrial Park poor (derelict) variants, prosperity Poor.
    &industrial_park::derelict_shed::DerelictShed,
    &industrial_park::rusted_tank::RustedTank,
    &industrial_park::scrap_heap::ScrapHeap,
    // Buildings — Coastal Resort theme (landmark + secondaries + props).
    &coastal_resort::grand_hotel::GrandHotel,
    &coastal_resort::resort_pier::ResortPier,
    &coastal_resort::beach_house::BeachHouse,
    &coastal_resort::boardwalk_shops::BoardwalkShops,
    &coastal_resort::lifeguard_tower::LifeguardTower,
    &coastal_resort::beach_umbrella::BeachUmbrella,
    &coastal_resort::deck_chair::DeckChair,
    &coastal_resort::dinghy::Dinghy,
    &coastal_resort::buoy::Buoy,
    // Buildings — Coastal Resort poor (fishing-hamlet) variants, prosperity Poor.
    &coastal_resort::fishing_shack::FishingShack,
    &coastal_resort::bait_stand::BaitStand,
    &coastal_resort::crab_traps::CrabTraps,
    // Buildings — Roadside / Highway theme (landmark + secondaries + props).
    &roadside::gas_station::GasStation,
    &roadside::roadside_diner::RoadsideDiner,
    &roadside::motel::Motel,
    &roadside::billboard::Billboard,
    &roadside::fuel_pump::FuelPump,
    &roadside::road_sign::RoadSign,
    &roadside::traffic_cone::TrafficCone,
    &roadside::vending_machine::VendingMachine,
    &roadside::guardrail::Guardrail,
    // Buildings — Roadside poor (busted-shoulder) variants, prosperity Poor.
    &roadside::produce_stand::ProduceStand,
    &roadside::boarded_shack::BoardedShack,
    &roadside::oil_drums::OilDrums,
    // Buildings — Civic / Campus theme (landmark + secondaries + props).
    &civic_campus::town_hall::TownHall,
    &civic_campus::library::Library,
    &civic_campus::lecture_hall::LectureHall,
    &civic_campus::dormitory::Dormitory,
    &civic_campus::clock_tower::ClockTower,
    &civic_campus::flagpole::Flagpole,
    &civic_campus::bike_rack::BikeRack,
    &civic_campus::notice_board::NoticeBoard,
    &civic_campus::campus_lamp::CampusLamp,
    // Buildings — Civic / Campus poor (underfunded) variants, prosperity Poor.
    &civic_campus::portable_classroom::PortableClassroom,
    &civic_campus::bus_shelter::BusShelter,
    &civic_campus::recycling_bins::RecyclingBins,
    // Buildings — Sports / Recreation theme (landmark + secondaries + props).
    &sports_rec::stadium::Stadium,
    &sports_rec::gym::Gym,
    &sports_rec::bleachers::Bleachers,
    &sports_rec::ticket_booth::TicketBooth,
    &sports_rec::clubhouse::Clubhouse,
    &sports_rec::goalpost::Goalpost,
    &sports_rec::floodlight_mast::FloodlightMast,
    &sports_rec::scoreboard::Scoreboard,
    &sports_rec::players_bench::PlayersBench,
    // Buildings — Sports / Recreation poor (rec-ground) variants, prosperity Poor.
    &sports_rec::rec_court::RecCourt,
    &sports_rec::backstop::Backstop,
    &sports_rec::tire_stack::TireStack,
    // Buildings — Steampunk theme (landmark + secondaries + props).
    &steampunk::cog_tower::CogTower,
    &steampunk::airship_dock::AirshipDock,
    &steampunk::foundry::Foundry,
    &steampunk::pump_house::PumpHouse,
    &steampunk::pipework::Pipework,
    &steampunk::pressure_tank::PressureTank,
    &steampunk::gear_pile::GearPile,
    &steampunk::gas_lamp::GasLamp,
    &steampunk::coal_hopper::CoalHopper,
    // Buildings — Steampunk poor (soot-yard) variants, prosperity Poor.
    &steampunk::tinkerers_shack::TinkerersShack,
    &steampunk::scrap_boiler::ScrapBoiler,
    &steampunk::cog_scrap::CogScrap,
    // Buildings — Solarpunk theme (landmark + secondaries + props).
    &solarpunk::biodome::Biodome,
    &solarpunk::green_pavilion::GreenPavilion,
    &solarpunk::wind_turbine::WindTurbine,
    &solarpunk::vertical_farm::VerticalFarm,
    &solarpunk::solar_panel::SolarPanel,
    &solarpunk::veggie_planter::VeggiePlanter,
    &solarpunk::water_channel::WaterChannel,
    &solarpunk::solar_lamp::SolarLamp,
    &solarpunk::beehive::Beehive,
    // Buildings — Solarpunk poor (grassroots) variants, prosperity Poor.
    &solarpunk::cob_roundhouse::CobRoundhouse,
    &solarpunk::poly_tunnel::PolyTunnel,
    &solarpunk::compost_heap::CompostHeap,
    // Buildings — Space Outpost theme (landmark + secondaries + props).
    &space_outpost::habitat_dome::HabitatDome,
    &space_outpost::solar_array::SolarArray,
    &space_outpost::comms_dish::CommsDish,
    &space_outpost::landing_pad::LandingPad,
    &space_outpost::hydroponics::Hydroponics,
    &space_outpost::rover::Rover,
    &space_outpost::cargo_crate::CargoCrate,
    &space_outpost::beacon::Beacon,
    &space_outpost::airlock::Airlock,
    // Buildings — Space Outpost poor (wreck) variants, prosperity Poor.
    &space_outpost::crash_shelter::CrashShelter,
    &space_outpost::solar_wreck::SolarWreck,
    &space_outpost::scrap_canister::ScrapCanister,
    // Buildings — High Fantasy theme (landmark + secondaries + props).
    &fantasy::wizard_tower::WizardTower,
    &fantasy::enchanted_library::EnchantedLibrary,
    &fantasy::fae_ring::FaeRing,
    &fantasy::crystal_shrine::CrystalShrine,
    &fantasy::runestone::Runestone,
    &fantasy::glow_mushroom::GlowMushroom,
    &fantasy::spell_circle::SpellCircle,
    &fantasy::mana_font::ManaFont,
    &fantasy::crystal_cluster::CrystalCluster,
    // Buildings — High Fantasy poor (hedge-magic) variants, prosperity Poor.
    &fantasy::hedge_hut::HedgeHut,
    &fantasy::standing_stone::StandingStone,
    &fantasy::toadstool_ring::ToadstoolRing,
    // Buildings — Gothic Horror theme (landmark + secondaries + props).
    &gothic_horror::cathedral::Cathedral,
    &gothic_horror::mausoleum::Mausoleum,
    &gothic_horror::cemetery::Cemetery,
    &gothic_horror::bell_tower::BellTower,
    &gothic_horror::gravestone::Gravestone,
    &gothic_horror::gargoyle::Gargoyle,
    &gothic_horror::dead_tree::DeadTree,
    &gothic_horror::iron_fence::IronFence,
    &gothic_horror::stone_cross::StoneCross,
    // Buildings — Gothic Horror poor (forsaken) variants, prosperity Poor.
    &gothic_horror::ruined_chapel::RuinedChapel,
    &gothic_horror::pauper_graves::PauperGraves,
    &gothic_horror::bone_pile::BonePile,
    // Buildings — Alien Organic theme (landmark + secondaries + props).
    &alien_organic::chitinous_hive::ChitinousHive,
    &alien_organic::pod_cluster::PodCluster,
    &alien_organic::fleshy_spire::FleshySpire,
    &alien_organic::membrane_wall::MembraneWall,
    &alien_organic::egg_sac::EggSac,
    &alien_organic::biolume_stalk::BiolumeStalk,
    &alien_organic::tendril::Tendril,
    &alien_organic::spore_vent::SporeVent,
    &alien_organic::creep_patch::CreepPatch,
    // Buildings — Alien Organic poor (necrotic) variants, prosperity Poor.
    &alien_organic::withered_hive::WitheredHive,
    &alien_organic::husk_pods::HuskPods,
    &alien_organic::rot_patch::RotPatch,
    // Buildings — Alien Monolithic theme (landmark + secondaries + props).
    &alien_monolithic::black_monolith::BlackMonolith,
    &alien_monolithic::levitating_platform::LevitatingPlatform,
    &alien_monolithic::light_pylon::LightPylon,
    &alien_monolithic::glyph_arch::GlyphArch,
    &alien_monolithic::floating_cube::FloatingCube,
    &alien_monolithic::glyph_stone::GlyphStone,
    &alien_monolithic::energy_node::EnergyNode,
    &alien_monolithic::monolith_shard::MonolithShard,
    &alien_monolithic::light_disc::LightDisc,
    // Buildings — Alien Monolithic poor (dormant) variants, prosperity Poor.
    &alien_monolithic::broken_monolith::BrokenMonolith,
    &alien_monolithic::dead_pylon::DeadPylon,
    &alien_monolithic::glyph_rubble::GlyphRubble,
    // Buildings — Post-apocalyptic theme (landmark + secondaries + props).
    &post_apoc::fortified_ruin::FortifiedRuin,
    &post_apoc::salvage_shack::SalvageShack,
    &post_apoc::radio_mast::RadioMast,
    &post_apoc::fuel_depot::FuelDepot,
    &post_apoc::wrecked_car::WreckedCar,
    &post_apoc::scrap_wall::ScrapWall,
    &post_apoc::fuel_barrels::FuelBarrels,
    &post_apoc::tire_wall::TireWall,
    &post_apoc::signal_fire::SignalFire,
    // Buildings — Post-apocalyptic poor (drifter) variants, prosperity Poor.
    &post_apoc::survivor_lean_to::SurvivorLeanTo,
    &post_apoc::rubble_barricade::RubbleBarricade,
    &post_apoc::ash_pit::AshPit,
    // Buildings — Wild West theme (landmark + secondaries + props).
    &wild_west::saloon::Saloon,
    &wild_west::water_tower::WaterTower,
    &wild_west::church::Church,
    &wild_west::jail::Jail,
    &wild_west::general_store::GeneralStore,
    &wild_west::hitching_post::HitchingPost,
    &wild_west::wagon::Wagon,
    &wild_west::frontier_fence::FrontierFence,
    &wild_west::wind_pump::WindPump,
    // Buildings — Wild West poor (bust) variants, prosperity Poor.
    &wild_west::prospector_shack::ProspectorShack,
    &wild_west::boot_hill::BootHill,
    &wild_west::tumbleweed::Tumbleweed,
    // Buildings — Pirate theme (landmark + secondaries + props).
    &pirate::harbour_battery::HarbourBattery,
    &pirate::harbour_tavern::HarbourTavern,
    &pirate::prize_warehouse::PrizeWarehouse,
    &pirate::careening_slip::CareeningSlip,
    &pirate::quay_capstan::QuayCapstan,
    &pirate::signal_mast::SignalMast,
    &pirate::powder_magazine::PowderMagazine,
    &pirate::rum_tuns::RumTuns,
    &pirate::longboat::Longboat,
    &pirate::rotting_hulk::RottingHulk,
    &pirate::gibbet_cage::GibbetCage,
    &pirate::tideline_bones::TidelineBones,
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
    // Plants — biome-specific species (epic #458 biome overhaul).
    &plants::lsys_cactus::Cactus,
    &plants::lsys_dead_shrub::DeadShrub,
    &plants::lsys_palm::Palm,
    &plants::lsys_mangrove::Mangrove,
    &plants::lsys_acacia::Acacia,
    &plants::lsys_bamboo::Bamboo,
    &plants::lsys_birch::Birch,
    &plants::lsys_bush::Bush,
    &plants::lsys_fern::Fern,
    &plants::lsys_flowering_tree::FloweringTree,
    // Ground-cover tier (#911) — crossed cards and flat decals, placed by
    // the hundred, so each is a handful of entities rather than a grammar.
    &plants::groundcover::GrassTuft,
    &plants::groundcover::DryGrassTuft,
    &plants::groundcover::Wildflower,
    &plants::groundcover::FernClump,
    &plants::groundcover::ReedClump,
    &plants::groundcover::ShoreGrass,
    &plants::groundcover::LilyPad,
    &plants::groundcover::DwarfShrub,
    &plants::groundcover::MossPatch,
    &plants::groundcover::LichenPatch,
    // Patterns — abstract L-system / ABOP demos.
    &patterns::lsys_branching::BranchingPattern,
    &patterns::lsys_koch_island::QuadraticKochIsland,
    &patterns::lsys_sierpinski::SierpinskiGasket,
    // Tools — utility items personalised at build time.
    &tools::my_teleporter::MyTeleporter,
    // Gateways — one bespoke per-theme social gateway (#749-772). Each is
    // tagged with its `ThemeArchetype`, so the seeded wiring's
    // `entries_for(theme, Gateway)` picks the matching gate; `civic_gateway`
    // carries no theme and is the cross-theme fallback (`by_slug`),
    // replacing the retired neutral placeholder.
    &alien_monolithic::gateway::AlienMonolithicGateway,
    &alien_monolithic::monument::AlienMonolithicMonument,
    &alien_organic::gateway::AlienOrganicGateway,
    &alien_organic::monument::AlienOrganicMonument,
    &ancient::gateway::AncientGateway,
    &ancient::monument::AncientMonument,
    &civic::gateway::CivicGateway,
    &civic::monument::CivicMonument,
    &civic_campus::gateway::CivicCampusGateway,
    &civic_campus::monument::CivicCampusMonument,
    &coastal_resort::gateway::CoastalResortGateway,
    &coastal_resort::monument::CoastalResortMonument,
    &cyberpunk::gateway::CyberpunkGateway,
    &cyberpunk::monument::CyberpunkMonument,
    &fantasy::gateway::FantasyGateway,
    &fantasy::monument::FantasyMonument,
    &feudal_japan::gateway::FeudalJapanGateway,
    &feudal_japan::monument::FeudalJapanMonument,
    &gothic_horror::gateway::GothicHorrorGateway,
    &gothic_horror::monument::GothicHorrorMonument,
    &industrial_park::gateway::IndustrialParkGateway,
    &industrial_park::monument::IndustrialParkMonument,
    &medieval::gateway::MedievalGateway,
    &medieval::monument::MedievalMonument,
    &mesoamerican::gateway::MesoamericanGateway,
    &mesoamerican::monument::MesoamericanMonument,
    &modern_city::gateway::ModernCityGateway,
    &modern_city::monument::ModernCityMonument,
    &nordic::gateway::NordicGateway,
    &nordic::monument::NordicMonument,
    &pirate::gateway::PirateGateway,
    &pirate::monument::PirateMonument,
    &post_apoc::gateway::PostApocGateway,
    &post_apoc::monument::PostApocMonument,
    &roadside::gateway::RoadsideGateway,
    &roadside::monument::RoadsideMonument,
    &rural_farmland::gateway::RuralFarmlandGateway,
    &rural_farmland::monument::RuralFarmlandMonument,
    &solarpunk::gateway::SolarpunkGateway,
    &solarpunk::monument::SolarpunkMonument,
    &space_outpost::gateway::SpaceOutpostGateway,
    &space_outpost::monument::SpaceOutpostMonument,
    &sports_rec::gateway::SportsRecGateway,
    &sports_rec::monument::SportsRecMonument,
    &steampunk::gateway::SteampunkGateway,
    &steampunk::monument::SteampunkMonument,
    &suburban::gateway::SuburbanGateway,
    &suburban::monument::SuburbanMonument,
    &wild_west::gateway::WildWestGateway,
    &wild_west::monument::WildWestMonument,
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

    /// #1039: every shape-grammar building must stand at grade. `footing`
    /// returns a root already sunk by half its buried plinth, so hanging
    /// the grammar on it with a bare child push inherits that offset and
    /// drops the building 1.6–2.9 m into its own foundation; `util::attach`
    /// rebases out of the root frame. All six shape entries shipped that
    /// way until this guard existed. (Phrased without the literal call so
    /// `no_entry_pushes_onto_an_assembled_root` does not flag this file.)
    #[test]
    fn shape_grammars_stand_at_grade() {
        let mut checked = 0;
        for e in ENTRIES {
            let built = e.build("");
            if !has_shape_node(&built) {
                continue;
            }
            checked += 1;
            super::shape_grammar_test::assert_shape_nodes_stand_at_grade(&built, e.slug());
        }
        assert!(
            checked >= 6,
            "expected the shape entries to be covered, saw {checked}"
        );
    }

    /// True when the tree contains a `GeneratorKind::Shape` anywhere.
    fn has_shape_node(node: &crate::pds::Generator) -> bool {
        matches!(node.kind, crate::pds::GeneratorKind::Shape { .. })
            || node.children.iter().any(has_shape_node)
    }
    use super::*;

    /// No entry pushes a child straight onto an assembled root (#1010).
    ///
    /// [`util::assemble`] and [`util::nest`] rebase the pieces handed to
    /// them out of the prop's ground frame; a child pushed onto the
    /// finished root afterwards is read in the root's *local* frame and
    /// never rebased, so it silently lands one root-height out. This
    /// shipped 65 times across 54 entries before it was caught — an
    /// emitter as much as 2.5 m off — because the offending line reads
    /// perfectly and nothing about the result looks broken in isolation.
    ///
    /// Since intent lives in the author's head rather than the built
    /// tree, this reads the sources: the call is simply banned in files
    /// that assemble their root, and [`util::attach`] is the way to add
    /// one more piece.
    #[test]
    fn no_entry_pushes_onto_an_assembled_root() {
        fn rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir)
                .expect("catalogue items dir")
                .flatten()
            {
                let p = e.path();
                if p.is_dir() {
                    rs_files(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p);
                }
            }
        }
        let root_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/catalogue/items");
        let mut files = Vec::new();
        rs_files(&root_dir, &mut files);
        assert!(files.len() > 100, "only found {} sources", files.len());

        let mut offenders = Vec::new();
        for path in files {
            // `util.rs` defines the idiom, and shows the wrong call in
            // `attach`'s docs precisely so authors recognise it.
            if path.file_name().is_some_and(|n| n == "util.rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path).expect("read source");
            if !src.contains("= assemble(") && !src.contains("= nest(") {
                continue;
            }
            // Whitespace-insensitive: rustfmt splits the call across lines.
            let flat: String = src.split_whitespace().collect::<Vec<_>>().join(" ");
            // Assembled from parts so this file does not match itself.
            let banned = ["root.children", ".push("].concat();
            let banned_split = ["root.children", " .push("].concat();
            if flat.contains(&banned) || flat.contains(&banned_split) {
                offenders.push(
                    path.strip_prefix(&root_dir)
                        .unwrap_or(&path)
                        .display()
                        .to_string(),
                );
            }
        }
        assert!(
            offenders.is_empty(),
            "{} entr(y/ies) push onto an assembled root instead of using \
             `util::attach`, so the child is never rebased out of the \
             ground frame:\n  {}",
            offenders.len(),
            offenders.join("\n  ")
        );
    }

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

    /// #940: `LogEnd` is an alpha card whose mask keeps one round slice and
    /// discards everything outside it, so it only works on a quad it fills
    /// edge to edge. Wrapped around a solid it deletes the solid — wood_pile
    /// shipped 19 cylinders rendering as floating slivers because of this.
    ///
    /// Deliberately narrower than "no cards on solids": the other cards mask
    /// their *interior* (window panes, chain-link gaps) and leave a frame, so
    /// they wrap curved and boxy geometry on purpose — the biodome's glazed
    /// sphere and ~70 window slabs across the catalogue depend on it. Only
    /// the border-masking card has a hard geometric requirement.
    #[test]
    fn log_end_cards_only_ever_land_on_planes() {
        use crate::pds::material_finish::node_materials_mut;
        use crate::pds::{Generator, GeneratorKind, SovereignTextureConfig};

        // Walks mutably purely to reuse `node_materials_mut`, the single
        // list of which kinds carry a material — a second immutable copy of
        // that match would be one more place to forget a new prim kind.
        fn walk(g: &mut Generator, slug: &str, bad: &mut Vec<String>) {
            let tag = g.kind.kind_tag();
            if !matches!(g.kind, GeneratorKind::Plane { .. })
                && node_materials_mut(&mut g.kind)
                    .into_iter()
                    .any(|m| matches!(m.texture, SovereignTextureConfig::LogEnd(_)))
            {
                bad.push(format!("{slug}: LogEnd on {tag}"));
            }
            for c in &mut g.children {
                walk(c, slug, bad);
            }
        }

        let mut bad = Vec::new();
        for e in ENTRIES {
            walk(&mut e.build(""), e.slug(), &mut bad);
        }
        assert!(
            bad.is_empty(),
            "LogEnd is a border-masking card and must sit on a Plane \
             (see `nordic::log_end`); found: {bad:?}"
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
    fn category_is_the_role_derived_section_for_every_entry() {
        use crate::catalogue::CatalogueCategory;
        // category() must stay a pure view of role(): every shipped entry sits
        // in the section its role maps to, so the catalogue UI grouping and the
        // settlement taxonomy can't drift. Checked against the registry itself
        // rather than a hand-summed per-theme building total that every
        // catalogue addition had to re-tally.
        for e in ENTRIES {
            assert_eq!(
                e.category(),
                e.role().category(),
                "entry {} reports a section that isn't its role's — category() \
                 has drifted from role()",
                e.slug()
            );
        }
        // The four sections partition the registry with no empties: a section
        // that lost all its content would be a silent regression a per-entry
        // check alone can't catch.
        for section in CatalogueCategory::ALL {
            assert!(
                ENTRIES.iter().any(|e| e.category() == section),
                "section {section:?} is empty"
            );
        }
    }
}
