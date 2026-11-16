//! The catalogue's lamps and their lights, held whole (#1923): which items
//! carry a light, of what kind, how many, that each shines its own glow by
//! night only, and what they cost the WebGL2 cluster list
//! (`world_builder::light`).
//!
//! Which glowing parts are lamps is decided where each item is built (see
//! [`super::fx::light`]); this pins the outcome, item by item, in [`LIT`],
//! so a lamp that loses its light or a glow that gains one shows up here.

use bevy::math::{Affine3A, UVec2, Vec3};

use super::ENTRIES;
use super::fx::light::ITEM_CLUSTER_COST;
use crate::catalogue::CatalogueEntry;
use crate::pds::{Generator, LightFlicker, LightShape, LightSource};
use crate::world_builder::light::ClusterView;

/// Every lit item and its lights, by kind: `B` a steady point light (a
/// bulb, a lantern's lamp, a marker), `S` a spot (a street light, a
/// floodlight, a canopy's downlight), `F` a flicker (a flame, a fire), `P`
/// a pulse (a beacon). An item not named here carries no light.
const LIT: &[(&str, &str)] = &[
    ("lighthouse", "1F"),
    ("observatory", "1F"),
    ("brazier", "1F"),
    ("blacksmith", "1F"),
    ("cable_arch", "1B"),
    ("tarp_shelter", "1B"),
    ("signal_beacon", "1F"),
    ("lakeside_shrine", "2F"),
    ("stone_lantern", "1F"),
    ("step_pyramid", "1F"),
    ("causeway_temple", "1F"),
    ("fire_bowl", "1F"),
    ("waterfront_concert_hall", "4B"),
    ("street_lamp", "1S"),
    ("tenement", "1B"),
    ("suburban_house", "1B"),
    ("suburban_water_tower", "1B"),
    ("barn", "1S"),
    ("farmhouse", "1B"),
    ("greenhouse", "1B"),
    ("factory", "1S"),
    ("loading_dock", "3S"),
    ("container_crane_quay", "1B 2S"),
    ("floodlight", "1S"),
    ("resort_pier", "6B"),
    ("beach_house", "1B"),
    ("boardwalk_shops", "1B"),
    ("lifeguard_tower", "1B"),
    ("buoy", "1P"),
    ("clifftop_lighthouse", "1B"),
    ("fishing_shack", "1B"),
    ("bait_stand", "1B"),
    ("gas_station", "3S"),
    ("billboard", "3S"),
    ("town_hall", "2B"),
    ("dormitory", "2B 1S"),
    ("hilltop_observatory", "1B"),
    ("campus_lamp", "1B"),
    ("stadium", "4S"),
    ("rowing_boathouse", "3B"),
    ("floodlight_mast", "1S"),
    ("foundry", "1F"),
    ("gas_lamp", "1B"),
    ("brass_observatory", "1B"),
    ("scrap_boiler", "1F"),
    ("solar_lamp", "1B"),
    ("comms_dish", "1P"),
    ("landing_pad", "8B"),
    ("beacon", "1P"),
    ("drowned_abbey", "2F"),
    ("fortified_ruin", "1S 1F"),
    ("fuel_depot", "1S"),
    ("beached_hulk", "1S 1F"),
    ("signal_fire", "1F"),
    ("saloon", "2F"),
    ("river_landing", "2F"),
    ("headland_redoubt", "1F"),
    ("harbour_tavern", "1F"),
    ("rotting_hulk", "1F"),
    ("barrel_fire", "1F"),
    ("watch_post", "1F"),
    ("lantern", "1B"),
    ("ancient_gateway", "2F"),
    ("ancient_monument", "2F"),
    ("civic_gateway", "2B"),
    ("civic_monument", "2F"),
    ("market_cross_monument", "1F"),
    ("figurehead_monument", "1F"),
    ("stone_lantern_monument", "1F"),
    ("serpent_altar_monument", "1F"),
    ("holo_pillar_monument", "1P"),
    ("works_plaque_monument", "1B"),
    ("trophy_monument", "2S"),
    ("civic_campus_gateway", "2B"),
    ("civic_campus_monument", "2B"),
    ("coastal_resort_gateway", "2B"),
    ("coastal_resort_monument", "1B"),
    ("feudal_japan_gateway", "2F"),
    ("feudal_japan_monument", "1F"),
    ("gothic_horror_monument", "1F"),
    ("industrial_park_gateway", "1S"),
    ("industrial_park_monument", "1S"),
    ("medieval_gateway", "2F"),
    ("medieval_monument", "1F"),
    ("mesoamerican_gateway", "2F"),
    ("mesoamerican_monument", "2F"),
    ("nordic_monument", "1F"),
    ("pirate_gateway", "3F"),
    ("pirate_monument", "2F"),
    ("post_apoc_gateway", "1B 1P"),
    ("post_apoc_monument", "1S"),
    ("rural_farmland_gateway", "2B"),
    ("rural_farmland_monument", "1S"),
    ("solarpunk_gateway", "1B"),
    ("solarpunk_monument", "1S"),
    ("space_outpost_gateway", "2P"),
    ("space_outpost_monument", "1P"),
    ("sports_rec_gateway", "2S"),
    ("sports_rec_monument", "1S"),
    ("steampunk_gateway", "2B"),
    ("steampunk_monument", "1B"),
    ("suburban_gateway", "2B"),
    ("suburban_monument", "1B"),
    ("wild_west_gateway", "2F"),
    ("wild_west_monument", "1F"),
    ("harbour_gateway", "1F"),
    ("palisade_gateway", "2F"),
    ("yakuimon_gateway", "2F"),
    ("corbel_arch_gateway", "2F"),
    ("light_ring_gateway", "2P"),
    ("garden_arch_gateway", "1B"),
    ("bone_arch_gateway", "1F"),
    ("works_gateway", "2B"),
    ("jetty", "1B"),
    ("helipad", "8B"),
    ("mooring_mast", "1P"),
    ("ships_lantern", "1F"),
];

/// The items whose every light is a flame's or a fire's, and so flickers.
const FLAMES: &[&str] = &[
    "lighthouse",
    "observatory",
    "brazier",
    "blacksmith",
    "signal_beacon",
    "lakeside_shrine",
    "stone_lantern",
    "step_pyramid",
    "causeway_temple",
    "fire_bowl",
    "foundry",
    "scrap_boiler",
    "drowned_abbey",
    "signal_fire",
    "saloon",
    "river_landing",
    "headland_redoubt",
    "harbour_tavern",
    "rotting_hulk",
    "barrel_fire",
    "watch_post",
    "ancient_gateway",
    "ancient_monument",
    "civic_monument",
    "market_cross_monument",
    "figurehead_monument",
    "stone_lantern_monument",
    "serpent_altar_monument",
    "feudal_japan_gateway",
    "feudal_japan_monument",
    "gothic_horror_monument",
    "medieval_gateway",
    "medieval_monument",
    "mesoamerican_gateway",
    "mesoamerican_monument",
    "nordic_monument",
    "pirate_gateway",
    "pirate_monument",
    "wild_west_gateway",
    "wild_west_monument",
    "harbour_gateway",
    "palisade_gateway",
    "yakuimon_gateway",
    "corbel_arch_gateway",
    "bone_arch_gateway",
    "ships_lantern",
];

/// The items whose every light is a beacon's, and so pulses.
const BEACONS: &[&str] = &[
    "buoy",
    "comms_dish",
    "beacon",
    "holo_pillar_monument",
    "space_outpost_gateway",
    "space_outpost_monument",
    "light_ring_gateway",
    "mooring_mast",
];

/// Each light of `tree`, with the node it hangs on and that node's whole
/// frame in the tree's (turn, scale and all, as the engine composes it).
fn framed(tree: &Generator) -> Vec<(&Generator, &LightSource, Affine3A)> {
    fn visit<'a>(
        node: &'a Generator,
        parent: Affine3A,
        out: &mut Vec<(&'a Generator, &'a LightSource, Affine3A)>,
    ) {
        let world = parent * super::measure::transform_of(&node.transform).compute_affine();
        if let Some(light) = &node.light {
            out.push((node, light, world));
        }
        for child in &node.children {
            visit(child, world, out);
        }
    }
    let mut out = Vec::new();
    visit(tree, Affine3A::IDENTITY, &mut out);
    out
}

/// Each light of `tree`, with the node it hangs on and where it stands in
/// the tree's frame.
fn lights(tree: &Generator) -> Vec<(&Generator, &LightSource, Vec3)> {
    framed(tree)
        .into_iter()
        .map(|(node, light, world)| {
            (
                node,
                light,
                world.transform_point3(Vec3::from(light.offset.0)),
            )
        })
        .collect()
}

/// Each spot of `tree`: where its light stands, the way it shines (a unit
/// vector, taken from its node's whole frame as the engine takes a spot's
/// direction) and its reach, all in the tree's frame.
fn spots(tree: &Generator) -> Vec<(Vec3, Vec3, f32)> {
    framed(tree)
        .into_iter()
        .filter_map(|(_, light, world)| match light.shape {
            LightShape::Spot { aim, .. } => Some((
                world.transform_point3(Vec3::from(light.offset.0)),
                world.transform_vector3(Vec3::from(aim.0)).normalize(),
                light.range.0,
            )),
            _ => None,
        })
        .collect()
}

/// A light's kind, as [`LIT`] spells it.
fn kind(light: &LightSource) -> char {
    match (&light.shape, &light.flicker) {
        (_, Some(LightFlicker::Flicker { .. })) => 'F',
        (_, Some(LightFlicker::Pulse { .. })) => 'P',
        (LightShape::Spot { .. }, _) => 'S',
        _ => 'B',
    }
}

/// An item's lights as [`LIT`] spells them: a count before each kind, in
/// the order B, S, F, P.
fn signature(tree: &Generator) -> String {
    let kinds: Vec<char> = lights(tree).iter().map(|(_, l, _)| kind(l)).collect();
    "BSFP"
        .chars()
        .filter_map(|k| {
            let n = kinds.iter().filter(|&&c| c == k).count();
            (n > 0).then(|| format!("{n}{k}"))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// How far from an item's middle the eye stands, past the item's own
/// reach across (m): beside it, as a visitor walks up to it.
const STANDOFF_M: f32 = 2.0;

/// Eye height (m).
const EYE_Y: f32 = 1.7;

/// The most places of WebGL2's cluster index list the lights of `tree`
/// take, seen by a first-person eye standing beside it from any of eight
/// sides, looking level at its middle on a 1920 x 1080 screen - counted as
/// Bevy counts them (`world_builder::light::clusters`).
fn cluster_cost(tree: &Generator) -> u32 {
    let lit: Vec<(Vec3, f32)> = lights(tree)
        .iter()
        .map(|(_, l, at)| (*at, l.range.0))
        .collect();
    let bounds = super::measure::tree_bounds(tree).expect("a lit item draws");
    let middle = bounds.center();
    let half = bounds.size() * 0.5;
    let away = half.x.max(half.z) + STANDOFF_M;
    let view = ClusterView::of(
        &bevy::camera::Projection::Perspective(Default::default()),
        UVec2::new(1920, 1080),
    );
    (0..8)
        .map(|k| {
            let yaw = k as f32 * std::f32::consts::TAU / 8.0;
            let eye = Vec3::new(
                middle.x + away * yaw.sin(),
                EYE_Y,
                middle.z + away * yaw.cos(),
            );
            let look = Vec3::new(middle.x, EYE_Y, middle.z);
            let to_view = bevy::transform::components::GlobalTransform::from(
                bevy::transform::components::Transform::from_translation(eye)
                    .looking_at(look, Vec3::Y),
            )
            .affine()
            .inverse();
            let in_view: Vec<(Vec3, f32)> = lit
                .iter()
                .map(|&(at, range)| (to_view.transform_point3(at), range))
                .collect();
            let far = in_view
                .iter()
                .filter_map(|&(c, r)| view.reach(c, r))
                .fold(0.0, f32::max);
            let slices = view.slices(far);
            in_view
                .iter()
                .map(|&(c, r)| view.count(&slices, c, r))
                .sum::<u32>()
        })
        .max()
        .unwrap_or(0)
}

fn built(entry: &dyn CatalogueEntry) -> Generator {
    entry.build("did:plc:lit-items")
}

/// Every item named in [`LIT`] carries exactly the lights it names, and no
/// other item carries any.
#[test]
fn every_lamp_in_the_catalogue_carries_its_lights() {
    for &(slug, _) in LIT {
        assert!(
            ENTRIES.iter().any(|e| e.slug() == slug),
            "{slug}: no such item"
        );
    }
    for entry in ENTRIES {
        let want = LIT
            .iter()
            .find(|(slug, _)| *slug == entry.slug())
            .map_or("", |(_, sig)| *sig);
        assert_eq!(
            signature(&built(*entry)),
            want,
            "{}: its lights are not what the table says",
            entry.slug()
        );
    }
}

/// Each light hangs on a glowing part and takes that part's glow for its
/// colour, and shows only from dusk to dawn: none of the catalogue's lamps
/// is lit by day.
#[test]
fn every_light_shines_its_own_glow_and_only_by_night() {
    let mut seen = 0;
    for entry in ENTRIES {
        let tree = built(*entry);
        for (node, light, _) in lights(&tree) {
            seen += 1;
            let glow = node
                .kind
                .material()
                .filter(|m| m.emission_strength.0 > 0.0)
                .unwrap_or_else(|| {
                    panic!("{}: a light on a part that does not glow", entry.slug())
                });
            assert_eq!(
                light.color,
                glow.emission_color.on_wire(),
                "{}: a light not of its glow's colour",
                entry.slug()
            );
            assert!(light.dusk_to_dawn, "{}: a light lit by day", entry.slug());
        }
    }
    let want: usize = LIT
        .iter()
        .flat_map(|(_, sig)| sig.split(' '))
        .map(|n| n[..n.len() - 1].parse::<usize>().expect("a count"))
        .sum();
    assert_eq!(seen, want, "the catalogue's lights");
}

/// A flame's or a fire's light flickers, a beacon's pulses - and each stays
/// inside the flash cap the sanitiser holds it to.
#[test]
fn flames_flicker_and_beacons_pulse() {
    let find = |slug: &str| {
        ENTRIES
            .iter()
            .find(|e| e.slug() == slug)
            .unwrap_or_else(|| panic!("{slug}: no such item"))
    };
    for (list, want) in [(FLAMES, 'F'), (BEACONS, 'P')] {
        for slug in list {
            let tree = built(*find(slug));
            let lit = lights(&tree);
            assert!(!lit.is_empty(), "{slug}: unlit");
            for (_, light, _) in lit {
                assert_eq!(kind(light), want, "{slug}");
                let mut again = light.clone();
                crate::pds::sanitize::Sanitize::sanitize(&mut again);
                assert_eq!(&again.flicker, &light.flicker, "{slug}: past the flash cap");
            }
        }
    }
}

/// A street light, a downlight and a floodlight shine down, the item's
/// frame composed down to the lamp; only an uplight shines up.
#[test]
fn spots_shine_down_but_uplights() {
    let mut seen = 0;
    for entry in ENTRIES {
        let up = UPLIGHTS.contains(&entry.slug());
        for (_, aim, _) in spots(&built(*entry)) {
            seen += 1;
            assert!(
                if up { aim.y > 0.5 } else { aim.y < -0.3 },
                "{}: a spot aimed {aim}",
                entry.slug()
            );
        }
    }
    assert!(seen >= 20, "{seen} spots");
}

/// The items whose spots shine up, washing a face over them.
const UPLIGHTS: [&str; 2] = ["billboard", "trophy_monument"];

/// The items whose spots light a floor above the item's ground, and that
/// floor's height in the item (m): the beached hulk's worklight lights its
/// deck.
const FLOORS: &[(&str, f32)] = &[("beached_hulk", super::post_apoc::beached_hulk::DECK_TOP)];

/// The furthest along its aim a spot may meet what it lights, as a share of
/// its reach: Bevy's range window has taken a light to under half of
/// itself past about 0.7 of its reach, so a pool much further out than half
/// its reach reads faint, and one past three quarters not at all.
const POOL_SHARE: f32 = 0.6;

/// Every spot shining down meets the ground (or the floor it lights) along
/// its aim inside [`POOL_SHARE`] of its reach, its item's frame composed
/// down to the lamp: a raked floodlight reaches twice its beam's length,
/// not twice its height, and one too high for the longest reach lights
/// from partway down its beam (`fx::light::LONGEST_THROW_M`).
#[test]
fn every_spot_meets_what_it_lights_inside_its_reach() {
    let mut seen = 0;
    let mut short = Vec::new();
    for entry in ENTRIES {
        if UPLIGHTS.contains(&entry.slug()) {
            continue;
        }
        let floor = FLOORS
            .iter()
            .find(|(slug, _)| *slug == entry.slug())
            .map_or(0.0, |&(_, y)| y);
        for (at, aim, reach) in spots(&built(*entry)) {
            seen += 1;
            let throw = (at.y - floor) / -aim.y;
            if !(throw > 0.0 && throw <= POOL_SHARE * reach) {
                short.push(format!(
                    "{}: from {at} along {aim} it meets y {floor} {throw:.2} m off, {:.2} of its reach {reach}",
                    entry.slug(),
                    throw / reach
                ));
            }
        }
    }
    assert!(seen >= 20, "{seen} spots");
    assert!(short.is_empty(), "{}", short.join("\n"));
}

/// A ground marker's light stands [`MARKER_LIFT_M`] over its glowing part
/// in the item, straight up whatever the part's turn: on the surface it
/// lights it would graze it and light nothing.
#[test]
fn ground_markers_stand_their_lights_over_their_lamps() {
    use super::fx::light::{MARKER_LIFT_M, MARKER_REACH_M};
    for slug in ["helipad", "landing_pad"] {
        let entry = ENTRIES.iter().find(|e| e.slug() == slug).expect("an item");
        let tree = built(*entry);
        let markers: Vec<_> = framed(&tree)
            .into_iter()
            .filter(|(_, light, _)| light.range.0 == MARKER_REACH_M)
            .collect();
        assert_eq!(markers.len(), 8, "{slug}: its markers");
        for (_, light, world) in markers {
            let lift = world.transform_vector3(Vec3::from(light.offset.0));
            assert!(
                (lift - Vec3::Y * MARKER_LIFT_M).length() < 0.01,
                "{slug}: a marker's light stands {lift} off its lamp"
            );
        }
    }
}

/// A lamp turned and stretched on its part, in an item built the way the
/// catalogue's are, shines along the aim the item asks and stands where it
/// asks, read through the same frames the catalogue's checks read - and a
/// ground marker on a tilted part lifts its light straight up all the
/// same. No item has a lit spot stretched on its part, so this one stands
/// in for it.
#[test]
fn a_lamp_turned_and_stretched_in_an_item_shines_where_the_item_means() {
    use super::fx::light::{Lamp, MARKER_LIFT_M, lit, lit_at};
    use super::util::{
        assemble, cuboid_tapered, glow, id_quat, prim, quat_mul, quat_x, quat_y, solid,
    };
    let aim = Vec3::new(0.2, -1.0, -0.6);
    let at = Vec3::new(0.1, 4.2, -0.5);
    let mut head = prim(
        cuboid_tapered([0.4, 0.2, 0.3], 0.0, glow([1.0, 0.9, 0.7], 3.0)),
        [0.5, 4.0, -0.3],
        quat_mul(quat_y(0.9), quat_x(-0.7)),
    );
    head.transform.scale = crate::pds::Fp3([1.5, 0.5, 2.0]);
    let head = lit_at(
        head,
        Lamp::Flood {
            over: 4.0,
            aim: aim.to_array(),
        },
        at.to_array(),
    );
    let pad_at = Vec3::new(2.0, 0.05, 1.0);
    let pad = lit(
        prim(
            cuboid_tapered([0.3, 0.05, 0.3], 0.0, glow([0.2, 1.0, 0.3], 2.0)),
            pad_at.to_array(),
            quat_mul(quat_y(-0.4), quat_x(0.3)),
        ),
        Lamp::Marker,
    );
    let post = prim(
        solid(cuboid_tapered([0.3, 4.0, 0.3], 0.0, Default::default())),
        [0.0, 2.0, 0.0],
        id_quat(),
    );
    let tree = assemble(vec![post, head, pad]);
    let lit_spots = spots(&tree);
    assert_eq!(lit_spots.len(), 1);
    let (stands, shines, _) = lit_spots[0];
    assert!(
        (shines - aim.normalize()).length() < 1e-3,
        "it shines along {shines}, not {}",
        aim.normalize()
    );
    assert!(
        (stands - at).length() < 1e-3,
        "it stands at {stands}, not {at}"
    );
    let marker = lights(&tree)
        .into_iter()
        .find(|(_, light, _)| matches!(light.shape, LightShape::Point))
        .expect("the marker");
    let want = pad_at + Vec3::Y * MARKER_LIFT_M;
    assert!(
        (marker.2 - want).length() < 1e-3,
        "the marker's light stands at {}, not {want}",
        marker.2
    );
}

/// Seen from beside it, each lit item's lights together take no more of
/// WebGL2's cluster list than [`ITEM_CLUSTER_COST`] - under the line the
/// budget admits lights by, so every one of them can be drawn at once.
#[test]
fn every_lit_item_stays_in_its_cluster_cost() {
    let mut measured = 0;
    for entry in ENTRIES {
        let tree = built(*entry);
        if lights(&tree).is_empty() {
            continue;
        }
        measured += 1;
        let cost = cluster_cost(&tree);
        assert!(
            cost <= ITEM_CLUSTER_COST,
            "{}: its lights take {cost} places of the cluster list",
            entry.slug()
        );
    }
    assert_eq!(measured, LIT.len());
}

/// Every lit item still sanitises to itself, lights and all, and its
/// record stays inside the soft budget a publish holds it to.
#[test]
fn every_lit_item_sanitises_stably_and_fits_its_record() {
    use crate::pds::record_size::{SOFT_RECORD_BUDGET_BYTES, serialized_record_bytes};
    for &(slug, _) in LIT {
        let entry = ENTRIES.iter().find(|e| e.slug() == slug).expect("an item");
        let tree = built(*entry);
        assert!(!lights(&tree).is_empty(), "{slug}: unlit");
        super::util::assert_sanitize_stable(&tree, slug);
        let bytes = serialized_record_bytes(&tree).expect("serialises");
        assert!(
            bytes <= SOFT_RECORD_BUDGET_BYTES,
            "{slug}: {bytes} B over the record budget"
        );
    }
}
