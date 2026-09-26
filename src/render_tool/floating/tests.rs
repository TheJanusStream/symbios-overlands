//! The floating report's rules, each on a fixture that shows it.

use serde_json::{Value, json};

use super::body::tests::{cuboid, fp, generator, prim, sphere};
use super::*;

/// A 33 x 33 map, 2 m a cell, its centre at the world's origin, whose
/// ground rises half a metre per metre toward +X: `0.5 * (x + 32)`.
fn ramp() -> HeightMap {
    let mut map = HeightMap::new(33, 33, 2.0);
    for z in 0..33 {
        for x in 0..33 {
            map.data_mut()[z * 33 + x] = 0.5 * (x as f32 * 2.0);
        }
    }
    map
}

fn flat() -> HeightMap {
    HeightMap::new(33, 33, 2.0)
}

fn record(generators: Vec<(&str, Value)>, placements: Vec<Value>) -> RoomRecord {
    let generators: serde_json::Map<String, Value> = generators
        .into_iter()
        .map(|(name, wire)| {
            // Through the typed form, as a fetched record decodes it.
            let wire = serde_json::to_value(generator(wire)).expect("a generator re-encodes");
            (name.to_owned(), wire)
        })
        .collect();
    serde_json::from_value(json!({
        "$type": "network.symbios.overlands.room",
        "environment": {},
        "generators": generators,
        "placements": placements,
        "traits": {},
    }))
    .expect("a room record")
}

/// A snapped absolute placement of `name` at `(x, z)`.
fn absolute(name: &str, x: f32, z: f32) -> Value {
    json!({
        "$type": "network.symbios.place.absolute",
        "generator_ref": name,
        "transform": { "translation": [fp(x), 0, fp(z)] },
    })
}

/// A scatter of `name`: forty copies, each snapped at its own point.
fn scatter(name: &str) -> Value {
    json!({
        "$type": "network.symbios.place.scatter",
        "generator_ref": name,
        "bounds": { "type": "circle", "center": [0, 0], "radius": fp(20.0) },
        "count": 40,
        "local_seed": "1",
    })
}

/// The report's fields for `record` over `map`, by name.
fn fields(record: &RoomRecord, map: &HeightMap) -> HashMap<&'static str, Value> {
    report("did:test", record, map, &bodies_of(record))
        .into_iter()
        .collect()
}

/// The floating rows alone.
fn floating(record: &RoomRecord, map: &HeightMap) -> Vec<Value> {
    fields(record, map)["floating"]
        .as_array()
        .expect("a list")
        .clone()
}

fn gap(row: &Value) -> f32 {
    row["gap_m"].as_f64().expect("a gap") as f32
}

/// A cylinder standing `height` tall from `bottom` (m), `radius` wide.
fn post(radius: f32, height: f32, x: f32, bottom: f32, children: Vec<Value>) -> Value {
    prim(
        "cylinder",
        json!({ "radius": fp(radius), "height": fp(height), "resolution": 32, "solid": true }),
        [x, bottom + height / 2.0, 0.0],
        children,
    )
}

/// A limb: a cylinder laid along +X from `inner` to `inner + length`
/// from its parent's axis, at `y` up the parent's frame.
fn limb(inner: f32, length: f32, y: f32) -> Value {
    let mut wire = prim(
        "cylinder",
        json!({ "radius": fp(0.05), "height": fp(length), "resolution": 12, "solid": true }),
        [inner + length / 2.0, y, 0.0],
        vec![],
    );
    // A quarter turn about Z lays the cylinder's height along X.
    wire["transform"]["rotation"] = json!([0, 0, 7_071, 7_071]);
    wire
}

/// Class a: a limb 20 cm off its trunk's surface floats, named by its
/// pointer with the gap between them; the trunk stands on the ground.
#[test]
fn a_limb_placed_off_its_trunk_is_reported() {
    // A 0.3 m trunk 4 m tall; the limb starts 0.5 m out, 2 m up.
    let tree = post(0.3, 4.0, 0.0, 0.0, vec![limb(0.5, 1.0, 1.0)]);
    let rows = floating(
        &record(vec![("tree", tree)], vec![absolute("tree", 0.0, 0.0)]),
        &flat(),
    );

    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["part"], "/generators/tree/children/0");
    assert_eq!(rows[0]["class"], "a");
    assert_eq!(rows[0]["placements"], json!([0]));
    assert!((gap(&rows[0]) - 0.2).abs() < 0.02, "{}", rows[0]);
    // Where it is: 2.5 m up the placement, 1 m out.
    assert_eq!(rows[0]["at"], json!([1.0, 3.0, 0.0]), "{}", rows[0]);
}

/// The dead tree (#1477): a trunk tapered to half its width at the top,
/// its limbs set at the untapered radius. The low limb still meets the
/// trunk, the top one stands 18 cm off it - and forty scattered copies are
/// one generator, checked once.
#[test]
fn a_tapered_trunk_leaves_its_top_limb_floating() {
    let mut trunk = post(
        0.4,
        6.0,
        0.0,
        0.0,
        vec![limb(0.4, 1.0, -2.7), limb(0.4, 1.0, 2.4)],
    );
    trunk["torture"] = json!({ "taper": [5_000, 5_000] });
    let record = record(vec![("dead_tree", trunk)], vec![scatter("dead_tree")]);

    let fields = fields(&record, &flat());

    let rows = fields["floating"].as_array().expect("a list");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["part"], "/generators/dead_tree/children/1");
    // 90% of the way up, the trunk is 0.4 * (1 - 0.45) = 0.22 m wide.
    assert!((gap(&rows[0]) - 0.18).abs() < 0.02, "{}", rows[0]);
    assert!(rows[0].get("at").is_none(), "a scatter has no one place");
    let scatters = fields["scatters"].as_array().expect("a list");
    assert_eq!(scatters.len(), 1);
    assert!(
        (scatters[0]["ground_reach_m"].as_f64().expect("a reach") - 0.4).abs() < 0.01,
        "{}",
        scatters[0]
    );
}

/// A sound tree - limbs starting inside the trunk - floats nothing.
#[test]
fn a_sound_tree_floats_nothing() {
    let tree = post(
        0.3,
        4.0,
        0.0,
        0.0,
        vec![
            limb(0.1, 1.2, 1.0),
            limb(0.0, 1.0, 0.2),
            limb(0.2, 0.8, -1.5),
        ],
    );
    let rows = floating(
        &record(vec![("tree", tree)], vec![absolute("tree", 0.0, 0.0)]),
        &flat(),
    );
    assert!(rows.is_empty(), "{rows:?}");
}

/// A cap resting on its stem is held by the stem, not the ground; the same
/// cap 10 cm up floats by 10 cm.
#[test]
fn a_cap_resting_on_its_stem_is_not_floating() {
    let mushroom = |lift: f32| {
        let mut cap = sphere(0.15, [0.0, 0.15 + lift, 0.0]);
        cap["torture"] = json!({ "profile_cut": [5_000, 10_000] });
        post(0.04, 0.3, 0.0, 0.0, vec![cap])
    };
    let resting = floating(
        &record(
            vec![("cap", mushroom(0.0))],
            vec![absolute("cap", 0.0, 0.0)],
        ),
        &flat(),
    );
    assert!(resting.is_empty(), "{resting:?}");

    let lifted = floating(
        &record(
            vec![("cap", mushroom(0.1))],
            vec![absolute("cap", 0.0, 0.0)],
        ),
        &flat(),
    );
    assert_eq!(lifted.len(), 1, "{lifted:?}");
    assert!((gap(&lifted[0]) - 0.1).abs() < 0.01, "{}", lifted[0]);
}

/// A knot wholly inside its trunk has no surface near the trunk's, and is
/// held by it all the same.
#[test]
fn a_part_wholly_inside_another_is_held_by_it() {
    let tree = post(0.3, 4.0, 0.0, 0.0, vec![sphere(0.08, [0.0, 0.5, 0.0])]);
    let rows = floating(
        &record(vec![("tree", tree)], vec![absolute("tree", 0.0, 0.0)]),
        &flat(),
    );
    assert!(rows.is_empty(), "{rows:?}");
}

/// Parts standing apart on the ground - two stones, scattered - are each
/// held by it; a third hanging half a metre over it floats by that much.
/// Unsnapped, the ground holds nothing: the stones apart from the first
/// part float.
#[test]
fn the_ground_holds_what_stands_on_it_when_snapped() {
    let stones = || {
        cuboid(
            [0.5, 0.5, 0.5],
            [0.0, 0.25, 0.0],
            vec![
                cuboid([0.4, 0.4, 0.4], [2.0, -0.05, 0.0], vec![]),
                cuboid([0.4, 0.4, 0.4], [-2.0, 0.5, 0.0], vec![]),
            ],
        )
    };
    let rows = floating(
        &record(vec![("stones", stones())], vec![scatter("stones")]),
        &flat(),
    );
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["part"], "/generators/stones/children/1");
    assert!((gap(&rows[0]) - 0.55).abs() < 0.01, "{}", rows[0]);

    let mut unsnapped = scatter("stones");
    unsnapped["snap_to_terrain"] = json!(false);
    let rows = floating(
        &record(vec![("stones", stones())], vec![unsnapped]),
        &flat(),
    );
    assert_eq!(rows.len(), 2, "{rows:?}");
}

/// An unsnapped absolute placement stands where it says, and the real
/// ground there holds what reaches it - the Understory's mycelial threads,
/// laid on the terrain at absolute heights. A metre above the ground,
/// nothing reaches it and the stones apart from the first part float.
#[test]
fn an_unsnapped_placement_is_held_by_the_real_ground_under_it() {
    let stones = || {
        cuboid(
            [0.5, 0.5, 0.5],
            [0.0, 0.25, 0.0],
            vec![cuboid([0.4, 0.4, 0.4], [2.0, -0.05, 0.0], vec![])],
        )
    };
    let placed = |y: f32| {
        let mut placement = absolute("stones", 3.0, 0.0);
        placement["snap_to_terrain"] = json!(false);
        placement["transform"]["translation"][1] = json!(fp(y));
        record(vec![("stones", stones())], vec![placement])
    };

    let on_the_ground = floating(&placed(0.0), &flat());
    assert!(on_the_ground.is_empty(), "{on_the_ground:?}");

    let aloft = floating(&placed(1.0), &flat());
    assert_eq!(aloft.len(), 1, "{aloft:?}");
    assert_eq!(aloft[0]["part"], "/generators/stones/children/0");
    // Nothing is on the ground, so the gap is to the first part: 2 m apart
    // centre to centre, less their half widths.
    assert!((gap(&aloft[0]) - 1.55).abs() < 0.01, "{}", aloft[0]);
    assert_eq!(aloft[0]["at"], json!([5.0, 1.2, 0.0]), "{}", aloft[0]);
}

/// A generator's parts are held by the real ground where the game stands
/// it, not only by its own ground plane: a stone set up the hill to where
/// the ramp's ground is (the Puffball Meadow's fix) is not floating.
#[test]
fn a_part_fitted_to_the_real_ground_is_held_by_it() {
    // Snapped at x = 0 the anchor stands at 16 m; at x = 4 the ground is
    // 18 m, 2 m up the generator's frame.
    let meadow = cuboid(
        [0.2, 0.2, 0.2],
        [0.0, 0.1, 0.0],
        vec![cuboid([0.2, 0.2, 0.2], [4.0, 1.9, 0.0], vec![])],
    );
    let rows = floating(
        &record(vec![("meadow", meadow)], vec![absolute("meadow", 0.0, 0.0)]),
        &ramp(),
    );
    assert!(rows.is_empty(), "{rows:?}");
}

/// A wide generator authored at one height and snapped at its centre on a
/// hill (the Puffball Meadow): its downhill parts float by the ground they
/// lost, at their downhill edge; the uphill ones are buried, which is not a
/// float. On flat ground nothing floats.
#[test]
fn a_wide_generator_on_a_slope_floats_its_downhill_parts() {
    let meadow = || {
        cuboid(
            [0.2, 0.2, 0.2],
            [0.0, 0.1, 0.0],
            [-8.0, -4.0, 4.0, 8.0]
                .map(|x| cuboid([0.2, 0.2, 0.2], [x, 0.0, 0.0], vec![]))
                .to_vec(),
        )
    };
    let placements = || vec![absolute("meadow", 0.0, 0.0)];

    let rows = floating(&record(vec![("meadow", meadow())], placements()), &ramp());

    let named: Vec<(&str, &str, f32)> = rows
        .iter()
        .map(|r| {
            (
                r["part"].as_str().expect("a pointer"),
                r["class"].as_str().expect("a class"),
                gap(r),
            )
        })
        .collect();
    assert_eq!(named.len(), 2, "{rows:?}");
    // The anchor stands at 16 m; the box at x = -8 spans -8.1..-7.9 and its
    // downhill edge is over ground at 0.5 * (32 - 8.1) = 11.95 m.
    assert_eq!(named[0].0, "/generators/meadow/children/0");
    assert_eq!(named[0].1, "b");
    assert!((named[0].2 - 4.05).abs() < 0.01, "{rows:?}");
    assert_eq!(named[1].0, "/generators/meadow/children/1");
    assert!((named[1].2 - 2.05).abs() < 0.01, "{rows:?}");
    assert_eq!(rows[0]["placement"], 0);
    assert_eq!(rows[0]["at"][0], json!(-8.1), "{}", rows[0]);
    assert_eq!(rows[0]["at"][1], json!(16.0), "{}", rows[0]);
    assert_eq!(rows[0]["under_m"], json!(11.95), "{}", rows[0]);

    let level = floating(&record(vec![("meadow", meadow())], placements()), &flat());
    assert!(level.is_empty(), "{level:?}");
}

/// A part resting on another part is held by it where the ground falls
/// away (the Spore Spires, #1477): a post on a slab whose uphill end is in
/// the ground does not float, though the ground under the post is 2 m down.
/// The slab's own downhill end, 5 m out, hangs 2 m over the ramp: that is
/// the float, and it is named alone.
#[test]
fn a_part_standing_on_another_part_is_held_by_it_over_falling_ground() {
    let slab = cuboid(
        [6.0, 0.5, 1.0],
        [-2.0, -0.23, 0.0],
        vec![cuboid([0.1, 1.0, 0.1], [-2.0, 0.75, 0.0], vec![])],
    );
    let rows = floating(
        &record(vec![("slab", slab)], vec![absolute("slab", 0.0, 0.0)]),
        &ramp(),
    );
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["part"], "/generators/slab", "{rows:?}");
    // Its underside at 16 - 0.48 m over ground at 0.5 * (32 - 5) m.
    assert!((gap(&rows[0]) - 2.02).abs() < 0.01, "{}", rows[0]);
}

/// A post sunk 0.3 m into a thick slab that overhangs the ramp is held by
/// the slab it is inside, not by the slab's underside a ray from it meets
/// 0.7 m further down.
#[test]
fn a_part_sunk_into_another_is_held_by_it_over_falling_ground() {
    let slab = cuboid(
        [6.0, 1.0, 1.0],
        [-2.0, -0.48, 0.0],
        vec![cuboid([0.1, 1.0, 0.1], [-2.0, 0.2, 0.0], vec![])],
    );
    let rows = floating(
        &record(vec![("slab", slab)], vec![absolute("slab", 0.0, 0.0)]),
        &ramp(),
    );
    let named: Vec<&Value> = rows.iter().map(|r| &r["part"]).collect();
    assert_eq!(named, vec!["/generators/slab"], "{rows:?}");
}

/// A spine laid at one height 20 cm up, 12 m long and snapped at its centre
/// on the ramp (the Understory's windthrow, #1477): its uphill end is deep
/// in the rise, and its downhill end hangs 3 m over the ground all the same.
/// On flat ground the same spine floats nothing.
#[test]
fn a_long_part_one_end_in_rising_ground_still_floats() {
    let spine = || {
        let points =
            [-6.0, 6.0].map(|x| json!({ "position": [fp(x), fp(0.2), 0], "radius": fp(0.06) }));
        prim(
            "spine",
            json!({ "points": points, "resolution": 5, "samples_per_segment": 3, "solid": false }),
            [0.0; 3],
            vec![],
        )
    };
    let placements = || vec![absolute("windthrow", 0.0, 0.0)];

    let rows = floating(&record(vec![("windthrow", spine())], placements()), &ramp());
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["part"], "/generators/windthrow");
    assert_eq!(rows[0]["class"], "b");
    // Its underside, some 15 cm up, over ground 3 m under the anchor's.
    assert!((gap(&rows[0]) - 3.15).abs() < 0.03, "{}", rows[0]);

    let level = floating(&record(vec![("windthrow", spine())], placements()), &flat());
    assert!(level.is_empty(), "{level:?}");
}

/// A terrain generator whose water stands at `level` (m), as a record holds
/// its water.
fn pond(level: f32) -> (&'static str, Value) {
    (
        "terrain",
        json!({
            "$type": "network.symbios.gen.terrain",
            "children": [{
                "$type": "network.symbios.gen.water",
                "transform": { "translation": [0, fp(level), 0] },
            }],
        }),
    )
}

/// Water holds a part only where it holds it at every placement (the ghost
/// snag, #1477): a limb set off its trunk, meeting the pond's surface where
/// one placement stands the tree in the water and 1.2 m over dry ground at
/// the other, floats free of the trunk. A raft of two pads laid on the pond
/// at its only placement is held by the water.
#[test]
fn water_holds_a_part_only_where_it_holds_it_at_every_placement() {
    // At x = -20 the tree stands at 6 m and its limb, 2 m up, meets the
    // water at 8 m; at x = 0 it stands at 16 m on dry ground.
    let tree = post(0.3, 4.0, 0.0, 0.0, vec![limb(0.5, 1.0, 0.0)]);
    let rows = floating(
        &record(
            vec![("tree", tree), pond(8.0)],
            vec![absolute("tree", -20.0, 0.0), absolute("tree", 0.0, 0.0)],
        ),
        &ramp(),
    );
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["part"], "/generators/tree/children/0");
    assert_eq!(rows[0]["class"], "a");
    assert!((gap(&rows[0]) - 0.2).abs() < 0.02, "{}", rows[0]);

    // Unsnapped at the water's level over ground 4 m down.
    let raft = cuboid(
        [1.0, 0.2, 1.0],
        [0.0; 3],
        vec![cuboid([1.0, 0.2, 1.0], [3.0, 0.0, 0.0], vec![])],
    );
    let mut placement = absolute("raft", -24.0, 0.0);
    placement["snap_to_terrain"] = json!(false);
    placement["transform"]["translation"][1] = json!(fp(8.0));
    let rows = floating(
        &record(vec![("raft", raft), pond(8.0)], vec![placement]),
        &ramp(),
    );
    assert!(rows.is_empty(), "{rows:?}");
}

/// The report reads as the triangle report does: a field a line, and a row
/// of a list a line.
#[test]
fn the_report_prints_a_row_a_line() {
    let tree = post(
        0.3,
        4.0,
        0.0,
        0.0,
        vec![limb(0.5, 1.0, 1.0), limb(0.5, 1.0, -1.0)],
    );
    let record = record(vec![("a/b~c", tree)], vec![absolute("a/b~c", 0.0, 0.0)]);
    let printed = one_row_a_line(&report("did:test", &record, &flat(), &bodies_of(&record)));

    let parsed: Value = serde_json::from_str(&printed).expect("one JSON object");
    assert_eq!(parsed["floating"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        parsed["floating"][0]["part"],
        "/generators/a~1b~0c/children/0"
    );
    assert_eq!(
        printed
            .lines()
            .filter(|l| l.contains("\"class\":\"a\""))
            .count(),
        2,
        "{printed}"
    );
    assert_eq!(parsed["checked"]["parts"], 3);
}
