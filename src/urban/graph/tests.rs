use super::*;
use crate::urban::test_support::*;

/// #583: a degree-1 dead-end ending transversely a few metres off a through-road
/// welds in - the touched edge splits at the foot-of-perpendicular, a real
/// (degree-3) junction appears there, and the dead-end becomes a degree-2 node.
#[test]
fn weld_creates_junction_for_real_near_miss() {
    // Through-road 0-1 along +x; shaft 2→3 drops toward it, tip 3 is 5 m above.
    let mut g = weld_graph(
        &[(0.0, 0.0), (100.0, 0.0), (50.0, 20.0), (50.0, 5.0)],
        &[(0, 1), (2, 3)],
    );
    assert_eq!(active_degrees(&g)[3], 1, "node 3 starts as a dead-end");
    assert_eq!(
        weld_endpoint_dangles(&mut g, 8.0),
        1,
        "the near-miss dead-end should weld"
    );
    let deg = active_degrees(&g);
    assert_eq!(deg[3], 2, "the welded dead-end becomes a through node");
    let junction = (0..g.nodes.len())
        .find(|&i| deg[i] == 3)
        .expect("a real degree-3 junction must appear");
    let p = g.nodes[junction].position;
    assert!(
        (p.x - 50.0).abs() < 1.0e-3 && p.y.abs() < 1.0e-3,
        "junction sits at the foot of perpendicular (50,0), got {p:?}"
    );
}

/// #583: a dead-end running NEAR-PARALLEL to a road (crossing angle below
/// WELD_MIN_CROSS_ANGLE) is a graze, not a junction - it must NOT weld (the
/// additive twin of the #571 graze cut must not re-introduce false junctions).
#[test]
fn graze_is_left_alone() {
    let mut g = weld_graph(
        &[(0.0, 0.0), (100.0, 0.0), (30.0, 3.0), (70.0, 3.0)],
        &[(0, 1), (2, 3)],
    );
    assert_eq!(
        weld_endpoint_dangles(&mut g, 8.0),
        0,
        "a parallel graze must not weld"
    );
}

/// #583: a genuine cul-de-sac ending in open space (no edge within tolerance) is
/// left for the #579 cap - the weld only fires when another road is near.
#[test]
fn true_cul_de_sac_is_left_alone() {
    let mut g = weld_graph(
        &[(0.0, 0.0), (100.0, 0.0), (50.0, 50.0), (50.0, 30.0)],
        &[(0, 1), (2, 3)],
    );
    assert_eq!(
        weld_endpoint_dangles(&mut g, 8.0),
        0,
        "a far cul-de-sac (30 m off) must not weld"
    );
}

/// #583: a dead-end whose foot lands in the outer margin of a segment (near an
/// endpoint) is a near-NODE case owned by merge_coincident_nodes - it must NOT
/// split the edge mid-span.
#[test]
fn endpoint_near_node_is_not_welded() {
    // Tip 3 is 5 m off the road but its foot is at t≈0.02 (< WELD_T_MARGIN).
    let mut g = weld_graph(
        &[(0.0, 0.0), (100.0, 0.0), (2.0, 25.0), (2.0, 5.0)],
        &[(0, 1), (2, 3)],
    );
    assert_eq!(
        weld_endpoint_dangles(&mut g, 8.0),
        0,
        "a near-endpoint foot must not split the edge"
    );
}

/// #583: a dead-end must never weld onto its OWN chain (a hairpin curling back
/// near an earlier segment of the same road) - the self-chain guard excludes it.
#[test]
fn self_chain_is_not_welded() {
    // Chain 0-1-2-3-4: edge 0-1 lies at y=0; the tip 4=(5,3) comes back 3 m
    // above it, but 0-1 is part of 4's own chain.
    let mut g = weld_graph(
        &[
            (0.0, 0.0),
            (50.0, 0.0),
            (50.0, 30.0),
            (5.0, 30.0),
            (5.0, 3.0),
        ],
        &[(0, 1), (1, 2), (2, 3), (3, 4)],
    );
    assert_eq!(active_degrees(&g)[4], 1, "node 4 is the dead-end");
    assert_eq!(
        weld_endpoint_dangles(&mut g, 8.0),
        0,
        "the only nearby edge is the dead-end's own chain → no weld"
    );
    assert_eq!(active_degrees(&g)[4], 1, "node 4 stays a dead-end");
}

/// #583: welding is idempotent - a second pass over an already-welded graph
/// welds nothing (the dead-end is now a degree-2 through node).
#[test]
fn weld_is_idempotent() {
    let mut g = weld_graph(
        &[(0.0, 0.0), (100.0, 0.0), (50.0, 20.0), (50.0, 5.0)],
        &[(0, 1), (2, 3)],
    );
    assert_eq!(weld_endpoint_dangles(&mut g, 8.0), 1);
    assert_eq!(
        weld_endpoint_dangles(&mut g, 8.0),
        0,
        "second pass welds nothing"
    );
}

/// #890: the style presets genuinely change the traced topology - a Grid
/// district on sloped terrain differs from the Hillside default - while the
/// forward-compat `Unknown` arm traces exactly as Hillside.
#[test]
fn road_style_changes_the_traced_graph() {
    use crate::pds::generator::RoadStyle;
    let hm = sloped_heightmap();
    let graph_for = |style: RoadStyle| {
        let mut c = cfg(7);
        c.style = style;
        build_road_graph_raw(&hm, &c, None).map(|(g, _, _)| {
            (
                g.nodes.len(),
                g.nodes
                    .iter()
                    .map(|n| (n.position.x, n.position.y))
                    .collect::<Vec<_>>(),
            )
        })
    };
    let hillside = graph_for(RoadStyle::Hillside).expect("hillside traces");
    let grid = graph_for(RoadStyle::Grid).expect("grid traces");
    let organic = graph_for(RoadStyle::Organic).expect("organic traces");
    let unknown = graph_for(RoadStyle::Unknown).expect("unknown traces");
    assert_eq!(unknown, hillside, "Unknown must trace as Hillside");
    assert_ne!(grid.1, hillside.1, "Grid must reshape the network");
    assert_ne!(organic.1, hillside.1, "Organic must reshape the network");
}

/// Every active edge of `graph` as its two endpoint positions.
fn active_segments(graph: &RoadGraph) -> Vec<(glam::Vec2, glam::Vec2)> {
    graph
        .edges
        .iter()
        .filter(|e| e.active)
        .map(|e| {
            (
                graph.nodes[e.start as usize].position,
                graph.nodes[e.end as usize].position,
            )
        })
        .collect()
}

/// How many points, sampled every metre along the active edges, stand at
/// or below `level` on `sub` - the district copy the graph was traced on.
fn drowned_samples(graph: &RoadGraph, sub: &HeightMap, level: f32) -> usize {
    active_segments(graph)
        .into_iter()
        .map(|(a, b)| {
            let steps = (a.distance(b).ceil() as usize).max(1);
            (0..=steps)
                .filter(|i| {
                    let p = a.lerp(b, *i as f32 / steps as f32);
                    sub.get_height_at(p.x, p.y) <= level
                })
                .count()
        })
        .sum()
}

/// A water line that floods the lower third of the pilot room's district,
/// so a network that ignores it runs well out across the lake bed.
fn pilot_water_level(hm: &HeightMap) -> f32 {
    let mut heights: Vec<f32> = hm.data().to_vec();
    heights.sort_by(f32::total_cmp);
    heights[heights.len() / 3]
}

/// #1552: a network that avoids water is handed the room's water line, so
/// no street is traced under it. The control is the same network with the
/// switch off over the same lake, which does run under the water - without
/// it the assertion could pass on a district that never reached the shore.
#[test]
fn a_network_that_avoids_water_traces_no_street_under_it() {
    let hm = pilot_heightmap();
    let level = pilot_water_level(&hm);
    let shore = RoadConfig {
        avoid_water: true,
        ..cfg(PILOT_ROAD_SEED)
    };
    let (graph, sub, _) =
        build_road_graph(&hm, &shore, Some(level)).expect("the shore network traces");
    assert!(
        active_segments(&graph).len() > 20,
        "the shore network still has a city's worth of streets"
    );
    assert_eq!(
        drowned_samples(&graph, &sub, level),
        0,
        "a street of a network that avoids water runs under the water line"
    );

    let (plain, plain_sub, _) = build_road_graph(&hm, &cfg(PILOT_ROAD_SEED), Some(level))
        .expect("the plain network traces");
    assert!(
        drowned_samples(&plain, &plain_sub, level) > 100,
        "the control: the same network with the switch off crosses the lake bed"
    );
}

/// #1552: the switch is the ONLY way the water reaches the tracer. With it
/// off, a room's water line changes nothing - a network saved before the
/// field traces exactly as it always did - and with it on in a dry room
/// there is no line to stop at.
#[test]
fn the_water_line_moves_only_a_network_that_avoids_water() {
    let hm = pilot_heightmap();
    let level = pilot_water_level(&hm);
    let positions = |config: &RoadConfig, water: Option<f32>| {
        build_road_graph_raw(&hm, config, water).map(|(g, _, _)| {
            g.nodes
                .iter()
                .map(|n| (n.position.x.to_bits(), n.position.y.to_bits()))
                .collect::<Vec<_>>()
        })
    };
    let plain = cfg(PILOT_ROAD_SEED);
    let dry = positions(&plain, None).expect("the plain network traces");
    assert_eq!(
        positions(&plain, Some(level)),
        Some(dry.clone()),
        "with the switch off the water line must not move a single node"
    );
    let shore = RoadConfig {
        avoid_water: true,
        ..cfg(PILOT_ROAD_SEED)
    };
    assert_eq!(
        positions(&shore, None),
        Some(dry.clone()),
        "with no water in the room the switch has nothing to stop at"
    );
    assert_ne!(
        positions(&shore, Some(level)),
        Some(dry),
        "the control: with the switch on, the water line does move the trace"
    );
}

// --- The street field (#1556) ------------------------------------------------

use crate::pds::generator::{RoadField, RoadKeepOut};
use crate::pds::types::Fp;

/// Each active edge of `road_type` as (room-frame midpoint, unit direction,
/// length), the window positions moved out by `shift` as the lots are.
fn room_edges(
    graph: &RoadGraph,
    shift: [f32; 2],
    road_type: RoadType,
) -> Vec<(glam::Vec2, glam::Vec2, f32)> {
    let shift = glam::Vec2::from(shift);
    graph
        .edges
        .iter()
        .filter(|e| e.active && e.road_type == road_type)
        .filter_map(|e| {
            let a = graph.nodes[e.start as usize].position + shift;
            let b = graph.nodes[e.end as usize].position + shift;
            let len = a.distance(b);
            (len > 1.0e-3).then(|| ((a + b) * 0.5, (b - a) / len, len))
        })
        .collect()
}

/// The share of `edges`' length that `keep` accepts.
fn length_share(
    edges: &[(glam::Vec2, glam::Vec2, f32)],
    keep: impl Fn(glam::Vec2, glam::Vec2) -> bool,
) -> f32 {
    let total: f32 = edges.iter().map(|e| e.2).sum();
    let kept: f32 = edges
        .iter()
        .filter(|(mid, dir, _)| keep(*mid, *dir))
        .map(|e| e.2)
        .sum();
    kept / total.max(1.0e-6)
}

/// The compass bearing a street direction runs along in the room frame,
/// degrees clockwise from north (-Z) toward east (+X), folded into
/// `[0, 180)`: a street runs both ways.
fn street_bearing(dir: glam::Vec2) -> f32 {
    dir.x.atan2(-dir.y).to_degrees().rem_euclid(180.0)
}

/// Whether a street direction lies within `tol` degrees of `bearing`,
/// mod 180.
fn runs_along(dir: glam::Vec2, bearing: f32, tol: f32) -> bool {
    let off = (street_bearing(dir) - bearing).rem_euclid(180.0);
    off.min(180.0 - off) < tol
}

/// A district moved off the room origin, so the window frame and the room
/// frame differ on both axes by different amounts.
fn offset_district(seed: u64) -> RoadConfig {
    RoadConfig {
        center: Fp2([60.0, -40.0]),
        ..cfg(seed)
    }
}

/// `config` with `field` as its street field.
fn with_field(config: RoadConfig, field: RoadField) -> RoadConfig {
    RoadConfig { field, ..config }
}

/// #1556: the bearing conversion is a quarter-turn shift. A compass bearing
/// `b` (clockwise from north, -Z, toward east, +X) points along
/// `(sin b, -cos b)`; the tracer lays a grid along `(cos a, sin a)`; and
/// `a = b - 90` degrees is the same vector - with north and east pinned
/// outright so a sign slip in the derivation cannot hide.
#[test]
fn a_grid_bearing_is_the_tracers_angle_a_quarter_turn_back() {
    let along = |b: f32| {
        let a = grid_angle(b);
        glam::Vec2::new(a.cos(), a.sin())
    };
    for b in [0.0_f32, 12.5, 30.0, 45.0, 90.0, 135.0, 179.5] {
        let compass = glam::Vec2::new(b.to_radians().sin(), -b.to_radians().cos());
        assert!(
            along(b).distance(compass) < 1.0e-6,
            "bearing {b}: the tracer's {:?} is not the compass {compass:?}",
            along(b)
        );
    }
    assert!(
        along(0.0).distance(glam::Vec2::new(0.0, -1.0)) < 1.0e-6,
        "0 is north, -Z"
    );
    assert!(
        along(90.0).distance(glam::Vec2::new(1.0, 0.0)) < 1.0e-6,
        "90 is east, +X"
    );
    // A grid repeats every half turn: one grid, one angle, to the bit.
    assert_eq!(grid_angle(180.0).to_bits(), grid_angle(0.0).to_bits());
    assert_eq!(grid_angle(-150.0).to_bits(), grid_angle(30.0).to_bits());
    assert_eq!(grid_angle(390.0).to_bits(), grid_angle(30.0).to_bits());
}

/// #1556: a grid at bearing 30 traces its major streets at bearing 30 in
/// the ROOM frame (mod 180), its minor streets square to them. Controls:
/// the same district without the field runs its majors every which way,
/// and a conversion that swapped the families would put the majors at 120.
#[test]
fn a_grid_traces_its_major_streets_at_its_bearing_in_the_room_frame() {
    let hm = pilot_heightmap();
    let base = offset_district(PILOT_ROAD_SEED);
    let grid = with_field(
        base.clone(),
        RoadField {
            terrain_weight: Fp(0.0),
            basis: vec![RoadBasis::Grid {
                center: base.center,
                bearing: Fp(30.0),
                radius: Fp(1024.0),
                strength: Fp(1.0),
            }],
            ..RoadField::default()
        },
    );
    let (graph, _, lo) = build_road_graph(&hm, &grid, None).expect("the grid district traces");
    let shift = window_to_room_shift(&hm, lo);
    let majors = room_edges(&graph, shift, RoadType::Major);
    let minors = room_edges(&graph, shift, RoadType::Minor);
    let at_30 = length_share(&majors, |_, d| runs_along(d, 30.0, 5.0));
    let at_120 = length_share(&majors, |_, d| runs_along(d, 120.0, 5.0));
    let minor_120 = length_share(&minors, |_, d| runs_along(d, 120.0, 5.0));
    let (plain, _, _) = build_road_graph(&hm, &base, None).expect("the plain district traces");
    let plain_30 = length_share(&room_edges(&plain, shift, RoadType::Major), |_, d| {
        runs_along(d, 30.0, 5.0)
    });
    assert!(
        majors.len() > 20 && minors.len() > 20,
        "a city's worth of streets"
    );
    assert!(at_30 > 0.8, "the majors must run at bearing 30: {at_30}");
    assert!(
        minor_120 > 0.8,
        "the minors must run square to them: {minor_120}"
    );
    assert!(
        at_120 < 0.1,
        "the control: no major runs across the grid: {at_120}"
    );
    assert!(
        plain_30 < 0.3,
        "the control: without the field the majors wander: {plain_30}"
    );
}

/// The share of the major streets' length 30-110 m from `centre` (room
/// frame) that rings it.
fn ringness(majors: &[(glam::Vec2, glam::Vec2, f32)], centre: glam::Vec2) -> f32 {
    let band: Vec<_> = majors
        .iter()
        .copied()
        .filter(|(mid, _, _)| (30.0..110.0).contains(&mid.distance(centre)))
        .collect();
    length_share(&band, |mid, dir| {
        dir.dot((mid - centre).normalize()).abs() < 0.3
    })
}

/// #1556: a ring centred on a ROOM point rings that point. The point is off
/// the window's centre, in a district moved off the room origin, so a ring
/// mapped into the window wrongly - unshifted, or shifted the wrong way -
/// would ring somewhere else entirely. The controls ring nothing round the
/// window's own centre or round where an unshifted mapping would land.
#[test]
fn a_ring_rings_the_room_point_it_was_given() {
    let hm = pilot_heightmap();
    let base = offset_district(PILOT_ROAD_SEED);
    let point = Fp2([90.0, -10.0]);
    let ring = with_field(
        base.clone(),
        RoadField {
            terrain_weight: Fp(0.0),
            basis: vec![RoadBasis::Ring {
                center: point,
                radius: Fp(200.0),
                strength: Fp(1.0),
            }],
            ..RoadField::default()
        },
    );
    let (graph, _, lo) = build_road_graph(&hm, &ring, None).expect("the ring district traces");
    let shift = window_to_room_shift(&hm, lo);
    let majors = room_edges(&graph, shift, RoadType::Major);
    let p = glam::Vec2::from(point.0);
    let window_centre = glam::Vec2::new(base.center.0[0], base.center.0[1]);
    let unshifted = p + glam::Vec2::from(shift);
    let (round_p, round_centre, round_unshifted) = (
        ringness(&majors, p),
        ringness(&majors, window_centre),
        ringness(&majors, unshifted),
    );
    assert!(
        round_p > 0.8,
        "the majors must ring the room point: {round_p}"
    );
    assert!(
        round_centre < 0.6,
        "the control: they do not ring the window centre: {round_centre}"
    );
    assert!(
        round_unshifted < 0.6,
        "the control: nor an unshifted point: {round_unshifted}"
    );
}

/// Every point, a metre apart along the active edges, in the room frame.
fn room_samples(graph: &RoadGraph, shift: [f32; 2]) -> Vec<glam::Vec2> {
    let shift = glam::Vec2::from(shift);
    active_segments(graph)
        .into_iter()
        .flat_map(|(a, b)| {
            let steps = (a.distance(b).ceil() as usize).max(1);
            (0..=steps).map(move |i| a.lerp(b, i as f32 / steps as f32) + shift)
        })
        .collect()
}

/// #1556: a keep-out disc given in ROOM metres keeps every street out of
/// that room disc. The tracer lets a street graze the rim by up to its snap
/// radius where a trace snaps onto a junction beside it, so the test reads
/// the disc that far in from its rim. The control is the same district
/// without the disc, which runs streets through its middle.
#[test]
fn a_keep_out_disc_keeps_the_streets_out_of_its_room_disc() {
    let hm = pilot_heightmap();
    let base = offset_district(PILOT_ROAD_SEED);
    let disc = RoadKeepOut {
        center: Fp2([90.0, -10.0]),
        radius: Fp(50.0),
    };
    let kept = with_field(
        base.clone(),
        RoadField {
            keep_out: vec![disc],
            ..RoadField::default()
        },
    );
    let (graph, _, lo) = build_road_graph(&hm, &kept, None).expect("the district traces");
    let (open, _, _) = build_road_graph(&hm, &base, None).expect("the open district traces");
    let shift = window_to_room_shift(&hm, lo);
    let centre = glam::Vec2::from(disc.center.0);
    let inside = |graph: &RoadGraph, depth: f32| {
        room_samples(graph, shift)
            .into_iter()
            .filter(|p| p.distance(centre) < disc.radius.0 - depth)
            .count()
    };
    let graze = TensorConfig::default().snap_radius;
    assert!(
        active_segments(&graph).len() > 20,
        "the district still has its streets"
    );
    assert_eq!(
        inside(&graph, graze),
        0,
        "a street runs into the keep-out disc"
    );
    assert!(
        inside(&open, graze) > 50,
        "the control: without the disc streets cross it"
    );
}

/// #1556: a basis field of a kind from a newer client is ignored at trace -
/// alone it traces the land's own streets, beside a ring it traces the
/// ring's - to the bit.
#[test]
fn a_basis_field_of_an_unknown_kind_is_ignored_at_trace() {
    let hm = pilot_heightmap();
    let positions = |config: &RoadConfig| {
        build_road_graph_raw(&hm, config, None).map(|(g, _, _)| {
            g.nodes
                .iter()
                .map(|n| (n.position.x.to_bits(), n.position.y.to_bits()))
                .collect::<Vec<_>>()
        })
    };
    let field = |basis: Vec<RoadBasis>| {
        with_field(
            cfg(PILOT_ROAD_SEED),
            RoadField {
                basis,
                ..RoadField::default()
            },
        )
    };
    let ring = RoadBasis::ring_at(Fp2([20.0, 10.0]));
    let plain = positions(&cfg(PILOT_ROAD_SEED)).expect("the plain network traces");
    assert_eq!(
        positions(&field(vec![RoadBasis::Unknown])),
        Some(plain.clone()),
        "an unknown kind alone must trace the land's own streets"
    );
    let ringed = positions(&field(vec![ring])).expect("the ring traces");
    assert_ne!(ringed, plain, "the control: a ring does move the streets");
    assert_eq!(
        positions(&field(vec![RoadBasis::Unknown, ring, RoadBasis::Unknown])),
        Some(ringed),
        "beside a ring an unknown kind must leave the ring's streets"
    );
}

/// #1556: an untouched street field hands the tracer its own defaults, so
/// a network that never set one traces as before the field existed - and a
/// set one passes through as authored, centres moved into the window.
#[test]
fn the_street_field_reaches_the_tracer_as_authored() {
    let hm = pilot_heightmap();
    let lo = [73, 23];
    let plain = tensor_config(&hm, &offset_district(7), None, lo);
    let defaults = symbios_tensor::TensorFieldConfig::default();
    assert_eq!(
        plain.field.smoothing.to_bits(),
        defaults.smoothing.to_bits()
    );
    assert_eq!(
        plain.field.terrain_weight.to_bits(),
        defaults.terrain_weight.to_bits()
    );
    assert!(plain.field.basis.is_empty() && plain.keep_out.is_empty());

    let shift = window_to_room_shift(&hm, lo);
    let set = tensor_config(
        &hm,
        &with_field(
            offset_district(7),
            RoadField {
                smoothing: Fp(12.5),
                terrain_weight: Fp(0.25),
                basis: vec![
                    RoadBasis::Ring {
                        center: Fp2([90.0, -10.0]),
                        radius: Fp(200.0),
                        strength: Fp(2.0),
                    },
                    RoadBasis::Unknown,
                    RoadBasis::Grid {
                        center: Fp2([-5.0, 7.5]),
                        bearing: Fp(30.0),
                        radius: Fp(80.0),
                        strength: Fp(0.5),
                    },
                ],
                keep_out: vec![RoadKeepOut {
                    center: Fp2([40.0, 60.0]),
                    radius: Fp(25.0),
                }],
            },
        ),
        None,
        lo,
    );
    assert_eq!(set.field.smoothing, 12.5);
    assert_eq!(set.field.terrain_weight, 0.25);
    let window = |x: f32, z: f32| glam::Vec2::new(x - shift[0], z - shift[1]);
    assert_eq!(
        set.field.basis,
        vec![
            BasisField::Radial {
                center: window(90.0, -10.0),
                radius: 200.0,
                strength: 2.0,
            },
            BasisField::Grid {
                center: window(-5.0, 7.5),
                angle: grid_angle(30.0),
                radius: 80.0,
                strength: 0.5,
            },
        ],
        "the unknown kind is left out, the rest moved into the window"
    );
    assert_eq!(
        set.keep_out,
        vec![KeepOut {
            center: window(40.0, 60.0),
            radius: 25.0,
        }]
    );
}

/// Values no street field should carry: non-finite, negative, zero, tiny,
/// and far past any room.
const HOSTILE: [f32; 11] = [
    f32::NAN,
    f32::INFINITY,
    f32::NEG_INFINITY,
    -1.0e9,
    -1.0,
    0.0,
    1.0e-30,
    3.0,
    1.0e9,
    f32::MAX,
    f32::MIN_POSITIVE,
];

/// A street field built from [`HOSTILE`] values, rotated by `turn` so each
/// slot meets every value across the turns, with lists far past their caps.
fn hostile_field(turn: usize) -> RoadField {
    let v = |k: usize| HOSTILE[(turn + k) % HOSTILE.len()];
    let mut basis = Vec::new();
    let mut keep_out = Vec::new();
    for i in 0..400 {
        basis.push(match i % 3 {
            0 => RoadBasis::Ring {
                center: Fp2([v(i), v(i + 1)]),
                radius: Fp(v(i + 2)),
                strength: Fp(v(i + 3)),
            },
            1 => RoadBasis::Grid {
                center: Fp2([v(i + 4), v(i + 5)]),
                bearing: Fp(v(i + 6)),
                radius: Fp(v(i + 7)),
                strength: Fp(v(i + 8)),
            },
            _ => RoadBasis::Unknown,
        });
        keep_out.push(RoadKeepOut {
            center: Fp2([v(i + 9), v(i + 10)]),
            radius: Fp(v(i)),
        });
    }
    RoadField {
        smoothing: Fp(v(1)),
        terrain_weight: Fp(v(2)),
        basis,
        keep_out,
    }
}

/// Whether the tracer accepts `cfg`'s field and keep-out discs: the checks
/// `generate_roads` makes before it traces a step.
fn tracer_accepts(cfg: &TensorConfig) -> bool {
    cfg.field.validate().is_ok()
        && cfg
            .keep_out
            .iter()
            .all(|d| d.center.is_finite() && d.radius.is_finite() && d.radius > 0.0)
}

/// #1556: whatever a record carries, its sanitised street field is one the
/// tracer accepts - `TensorFieldConfig::validate` passes and every keep-out
/// disc has a finite centre and a finite, positive radius, after the move
/// into the district window - so no record can make the tracer refuse its
/// network, and the lists are cut to their caps. Every hostile field is
/// first shown to be refused as it stands, so the sanitiser is what passes
/// it; and one is traced end to end.
#[test]
fn a_sanitised_street_field_always_passes_the_tracers_validation() {
    let hm = pilot_heightmap();
    for turn in 0..HOSTILE.len() {
        for center in [Fp2([0.0, 0.0]), Fp2([480.0, -480.0])] {
            let raw = RoadConfig {
                center,
                field: hostile_field(turn),
                ..cfg(PILOT_ROAD_SEED)
            };
            let lo = [0, 86];
            assert!(
                !tracer_accepts(&tensor_config(&hm, &raw, None, lo)),
                "the control: turn {turn} is refused as it stands"
            );
            let mut kind = crate::pds::GeneratorKind::RoadNetwork(raw);
            crate::pds::sanitize::sanitize_kind(&mut kind);
            let crate::pds::GeneratorKind::RoadNetwork(clean) = kind else {
                panic!("the sanitiser keeps the variant");
            };
            assert_eq!(clean.field.basis.len(), RoadField::MAX_BASIS);
            assert_eq!(clean.field.keep_out.len(), RoadField::MAX_KEEP_OUT);
            for lo in [[0, 0], [0, 86], [86, 0], [86, 86], [43, 21]] {
                let cfg = tensor_config(&hm, &clean, None, lo);
                assert!(
                    tracer_accepts(&cfg),
                    "turn {turn}, window {lo:?}: the tracer refuses {:?} / {:?}",
                    cfg.field,
                    cfg.keep_out
                );
            }
            if turn == 0 {
                assert!(
                    build_road_graph_raw(&hm, &clean, None).is_some(),
                    "the sanitised hostile network traces"
                );
            }
        }
    }
}

// --- #1558: the graph tidy of layout revision 1 -------------------------------

/// The district window the tidy tests trace in: flat, 256 m across, its
/// drawn interior a circle 112.6 m round (128, 128).
fn tidy_window() -> HeightMap {
    HeightMap::new(128, 128, 2.0)
}

/// A network at layout revision 1 with the default dimensions and spacings
/// (95/55 m): junctions closer than about 10.3 m merge, a stub is under
/// 27.5 m, a tiny face under 261 m2 and a doubled street within 13.75 m.
fn tidied() -> RoadConfig {
    RoadConfig {
        layout_revision: 1,
        ..RoadConfig::default()
    }
}

fn active(graph: &RoadGraph, e: usize) -> bool {
    graph.edges[e].active
}

#[test]
fn the_tidy_merges_junctions_a_few_metres_apart() {
    // A major street with a minor street leaving each of two junctions 4 m
    // apart to opposite sides - a tracer's offset crossing.
    let mut g = typed_graph(
        &[
            (124.0, 128.0),
            (128.0, 128.0),
            (40.0, 128.0),
            (216.0, 128.0),
            (124.0, 40.0),
            (128.0, 216.0),
        ],
        &[
            (0, 1, true),
            (2, 0, true),
            (1, 3, true),
            (0, 4, false),
            (1, 5, false),
        ],
    );
    tidy_graph(&mut g, &tidy_window(), &tidied());
    assert!(!active(&g, 0), "the 4 m street between them goes");
    let degree = active_degrees(&g);
    assert_eq!(degree[0], 4, "one junction of four streets");
    assert_eq!(degree[1], 0, "the other is no longer in the plan");
    let p = g.nodes[0].position;
    assert!(
        (p.x - 126.0).abs() < 1.0e-4 && (p.y - 128.0).abs() < 1.0e-4,
        "the junction stands between the two: {p:?}"
    );
}

#[test]
fn the_tidy_drops_one_of_two_streets_traced_side_by_side() {
    // Two major streets 12 m apart (more than a junction cluster, less than
    // a quarter of the minor spacing) running 136 m between two cross
    // streets; the second was traced later (higher edge numbers).
    let mut g = typed_graph(
        &[
            (60.0, 60.0),
            (60.0, 122.0),
            (60.0, 134.0),
            (60.0, 196.0),
            (196.0, 60.0),
            (196.0, 122.0),
            (196.0, 134.0),
            (196.0, 196.0),
        ],
        &[
            (0, 1, false),
            (1, 2, false),
            (2, 3, false),
            (4, 5, false),
            (5, 6, false),
            (6, 7, false),
            (1, 5, true),
            (2, 6, true),
        ],
    );
    tidy_graph(&mut g, &tidy_window(), &tidied());
    assert!(active(&g, 6), "the older street stays");
    assert!(!active(&g, 7), "its double goes");
    for e in 0..6 {
        assert!(active(&g, e), "cross street edge {e} stays");
    }
}

#[test]
fn the_tidy_opens_a_detour_loop() {
    // The owner's loop on the old plan: two junctions 33 m apart, joined by
    // a straight street and by a 113 m street looping out round a face of
    // 1,451 m2 - not a tiny face, but a detour over three times as long as
    // the way across.
    let mut g = typed_graph(
        &[
            (110.0, 128.0),
            (143.0, 128.0),
            (40.0, 128.0),
            (216.0, 128.0),
            (105.0, 100.0),
            (128.0, 82.0),
            (148.0, 100.0),
        ],
        &[
            (0, 1, true),
            (2, 0, true),
            (1, 3, true),
            (0, 4, false),
            (4, 5, false),
            (5, 6, false),
            (6, 1, false),
        ],
    );
    tidy_graph(&mut g, &tidy_window(), &tidied());
    assert!(
        (3..7).all(|e| !active(&g, e)),
        "the loop round the face goes"
    );
    assert!(
        (0..3).all(|e| active(&g, e)),
        "the straight street and the streets beyond stay"
    );
}

#[test]
fn the_tidy_keeps_a_crescent_between_two_junctions() {
    // Two junctions 116 m apart joined by streets of 118 m and 132 m round
    // a block of 2,453 m2 - the far side of a thin block a curved field
    // traces, not a loop. The tidy used to drop the longer street of ANY
    // two between the same junctions.
    let mut g = typed_graph(
        &[
            (70.0, 128.0),
            (186.0, 128.0),
            (24.0, 128.0),
            (232.0, 128.0),
            (128.0, 138.8),
            (128.0, 96.5),
        ],
        &[
            (2, 0, true),
            (1, 3, true),
            (0, 4, false),
            (4, 1, false),
            (0, 5, false),
            (5, 1, false),
        ],
    );
    tidy_graph(&mut g, &tidy_window(), &tidied());
    assert!(
        (0..6).all(|e| active(&g, e)),
        "both streets of the crescent stay"
    );
}

#[test]
fn the_tidy_drops_a_spun_ring_and_keeps_a_ring_road() {
    // Two loop streets, each leaving a junction and coming back to it: one
    // round 450 m2 (under a quarter of a 95 x 55 m block, more than a tiny
    // face) and one round 1,500 m2, room for lots.
    let mut g = typed_graph(
        &[
            (128.0, 150.0),
            (40.0, 150.0),
            (216.0, 150.0),
            (113.0, 120.0),
            (143.0, 120.0),
            (128.0, 110.0),
            (40.0, 110.0),
            (216.0, 110.0),
            (98.0, 60.0),
            (158.0, 60.0),
        ],
        &[
            (1, 0, true),
            (0, 2, true),
            (0, 3, false),
            (3, 4, false),
            (4, 0, false),
            (6, 5, true),
            (5, 7, true),
            (5, 8, false),
            (8, 9, false),
            (9, 5, false),
        ],
    );
    tidy_graph(&mut g, &tidy_window(), &tidied());
    assert!((2..5).all(|e| !active(&g, e)), "the small ring goes");
    assert!(
        (7..10).all(|e| active(&g, e)),
        "the ring road round room for lots stays"
    );
    assert!(
        [0, 1, 5, 6].iter().all(|&e| active(&g, e)),
        "the through streets stay"
    );
}

#[test]
fn the_tidy_drops_a_short_stub_and_keeps_a_long_dead_end() {
    let mut g = typed_graph(
        &[
            (40.0, 128.0),
            (100.0, 128.0),
            (170.0, 128.0),
            (216.0, 128.0),
            (100.0, 116.0),
            (170.0, 200.0),
        ],
        &[
            (0, 1, true),
            (1, 2, true),
            (2, 3, true),
            (1, 4, false),
            (2, 5, false),
        ],
    );
    tidy_graph(&mut g, &tidy_window(), &tidied());
    assert!(!active(&g, 3), "a 12 m stub goes");
    assert!(active(&g, 4), "a 72 m dead end stays");
}

#[test]
fn the_tidy_ends_streets_at_the_district_edge() {
    let mut g = typed_graph(
        &[
            (40.0, 128.0),
            (128.0, 128.0),
            (216.0, 128.0),
            (128.0, 60.0),
            (128.0, 250.0),
        ],
        &[(0, 1, true), (1, 2, true), (1, 3, false), (1, 4, false)],
    );
    tidy_graph(&mut g, &tidy_window(), &tidied());
    assert!(
        !active(&g, 3),
        "a street running out of the drawn district goes"
    );
    assert!(active(&g, 0) && active(&g, 1) && active(&g, 2));
}

#[test]
fn the_tidy_opens_a_tiny_face() {
    // A triangle of minor streets 15 m a side (a face of about 97 m2), a
    // street leaving each corner.
    let mut g = typed_graph(
        &[
            (120.0, 120.0),
            (135.0, 120.0),
            (127.5, 133.0),
            (60.0, 100.0),
            (196.0, 100.0),
            (127.5, 200.0),
        ],
        &[
            (0, 1, false),
            (1, 2, false),
            (2, 0, false),
            (3, 0, true),
            (1, 4, true),
            (2, 5, true),
        ],
    );
    tidy_graph(&mut g, &tidy_window(), &tidied());
    let open = (0..3).filter(|&e| !active(&g, e)).count();
    assert_eq!(open, 1, "one side of the tiny face goes, opening it");
    for e in 3..6 {
        assert!(active(&g, e), "street {e} leaving the corner stays");
    }
}

#[test]
fn the_tidy_keeps_a_clean_grid() {
    // A 3 x 3 grid at 60 m: nothing to tidy.
    let at = [68.0_f32, 128.0, 188.0];
    let nodes: Vec<(f32, f32)> = at
        .iter()
        .flat_map(|&z| at.iter().map(move |&x| (x, z)))
        .collect();
    let mut edges = Vec::new();
    for r in 0..3u32 {
        for c in 0..2u32 {
            edges.push((r * 3 + c, r * 3 + c + 1, true));
            edges.push((c * 3 + r, (c + 1) * 3 + r, false));
        }
    }
    let mut g = typed_graph(&nodes, &edges);
    let before: Vec<_> = g.nodes.iter().map(|n| n.position).collect();
    tidy_graph(&mut g, &tidy_window(), &tidied());
    assert!(
        g.edges.iter().all(|e| e.active),
        "a clean grid loses nothing"
    );
    let after: Vec<_> = g.nodes.iter().map(|n| n.position).collect();
    assert_eq!(before, after, "and nothing moves");
}

/// #1558: junctions every 4 m along a street merge in clusters no wider
/// than the threshold (about 10.3 m by default), never into one: the merge
/// used to union every short street transitively, chaining the whole run of
/// ten into one junction 36 m from some of its streets.
#[test]
fn a_junction_cluster_never_chains_along_a_run_of_short_streets() {
    // A major street through ten junctions 4 m apart, a minor street
    // leaving each, alternately one side and the other.
    let xs: Vec<f32> = (0..10).map(|k| 110.0 + 4.0 * k as f32).collect();
    let mut nodes = vec![(40.0, 128.0), (216.0, 128.0)];
    nodes.extend(xs.iter().map(|&x| (x, 128.0)));
    nodes.extend(
        xs.iter()
            .enumerate()
            .map(|(k, &x)| (x, if k % 2 == 0 { 60.0 } else { 196.0 })),
    );
    let mut edges = vec![(0, 2, true)];
    edges.extend((2..11).map(|j| (j, j + 1, true)));
    edges.push((11, 1, true));
    edges.extend((0..10).map(|k| (2 + k, 12 + k, false)));
    let mut g = typed_graph(&nodes, &edges);
    let reach = TidyBounds::new(&tidied()).cluster_len;
    assert!(merge_junction_clusters(&mut g, reach), "the run merges");
    // Which junction each minor street now leaves from.
    let mut clusters: std::collections::BTreeMap<u32, Vec<f32>> = Default::default();
    for (k, &x) in xs.iter().enumerate() {
        let e = &g.edges[11 + k];
        assert!(e.active, "minor street {k} stays");
        let far = 12 + k as u32;
        let from = if e.end == far { e.start } else { e.end };
        clusters.entry(from).or_default().push(x);
    }
    for (junction, run) in &clusters {
        let span = run.last().expect("one") - run.first().expect("one");
        assert!(
            span <= reach,
            "junction {junction} stands for traced junctions {span} m apart (threshold {reach})"
        );
    }
    assert_eq!(
        clusters.len(),
        4,
        "ten junctions 4 m apart make clusters of three: {clusters:?}"
    );
}

/// #1558: a junction stands for one cluster, merged once: three junctions
/// 6 m apart merge the nearer two (their middle is 9 m from the third, the
/// three 12 m across, past the 10.3 m threshold), and the merged junction
/// never merges again with its new neighbour - merging pass after pass
/// would chain it on, a cluster at a time.
#[test]
fn a_merged_junction_never_merges_again() {
    let mut g = typed_graph(
        &[
            (40.0, 128.0),
            (216.0, 128.0),
            (110.0, 128.0),
            (116.0, 128.0),
            (122.0, 128.0),
            (110.0, 60.0),
            (116.0, 196.0),
            (200.0, 60.0),
        ],
        &[
            (0, 2, true),
            (2, 3, true),
            (3, 4, true),
            (4, 1, true),
            (2, 5, false),
            (3, 6, false),
            (4, 7, false),
        ],
    );
    tidy_graph(&mut g, &tidy_window(), &tidied());
    let degree = active_degrees(&g);
    let junctions: Vec<usize> = (0..g.nodes.len()).filter(|&i| degree[i] >= 3).collect();
    assert_eq!(
        junctions,
        vec![2, 4],
        "the nearer two merged, the third did not"
    );
    let p = g.nodes[2].position;
    assert!(
        (p.x - 113.0).abs() < 1.0e-4 && (p.y - 128.0).abs() < 1.0e-4,
        "the merged junction stands between its two: {p:?}"
    );
    let q = g.nodes[4].position;
    assert!(
        (q.x - 122.0).abs() < 1.0e-4 && (q.y - 128.0).abs() < 1.0e-4,
        "the third junction stays where it was traced: {q:?}"
    );
    assert!(
        (4..7).all(|e| active(&g, e)),
        "every street leaving the junctions stays"
    );
}

/// #1558: on a dense plan of wide streets the cluster threshold is set by
/// the spacing, not only the width: a 30 x 60 m grid of 12 and 8 m
/// half-width streets (a 31.6 m threshold by the width alone, so every
/// 30 m block side would read as a cluster) loses nothing.
#[test]
fn the_tidy_keeps_a_dense_grid_of_wide_streets() {
    let xs = [68.0_f32, 98.0, 128.0, 158.0, 188.0];
    let zs = [68.0_f32, 128.0, 188.0];
    let nodes: Vec<(f32, f32)> = zs
        .iter()
        .flat_map(|&z| xs.iter().map(move |&x| (x, z)))
        .collect();
    let mut edges = Vec::new();
    for r in 0..3u32 {
        for c in 0..4u32 {
            edges.push((r * 5 + c, r * 5 + c + 1, true));
        }
    }
    for c in 0..5u32 {
        for r in 0..2u32 {
            edges.push((r * 5 + c, (r + 1) * 5 + c, false));
        }
    }
    let mut g = typed_graph(&nodes, &edges);
    let before: Vec<_> = g.nodes.iter().map(|n| n.position).collect();
    use crate::pds::types::Fp;
    let wide = RoadConfig {
        major_half_width: Fp(12.0),
        minor_half_width: Fp(8.0),
        major_spacing: Fp(60.0),
        minor_spacing: Fp(30.0),
        ..tidied()
    };
    tidy_graph(&mut g, &tidy_window(), &wide);
    assert!(
        g.edges.iter().all(|e| e.active),
        "the dense grid loses no street"
    );
    let after: Vec<_> = g.nodes.iter().map(|n| n.position).collect();
    assert_eq!(before, after, "and no junction moves");
}

/// The lots a graph grows once cleared of its streets (#1558).
fn cleared_lots(graph: &RoadGraph, sub: &HeightMap, config: &RoadConfig) -> usize {
    let mut blocks = graph.clone();
    let mut ground = HeightMap::new(sub.width(), sub.height(), sub.scale());
    ground.data_mut().copy_from_slice(sub.data());
    symbios_tensor::extract_blocks(&mut blocks);
    let lots = symbios_tensor::extract_lots(
        &blocks,
        &mut ground,
        &crate::urban::lot_config(config, None),
    );
    crate::urban::clear_lots(
        lots,
        &crate::urban::street_footprints(graph, config, crate::urban::LOT_STREET_MARGIN_M),
    )
    .len()
}

/// The length (m) of street the mesher draws of `graph`.
fn drawn_length(graph: &RoadGraph, sub: &HeightMap) -> f32 {
    let drawn = crate::urban::drawn_graph(graph, sub);
    drawn
        .edges
        .iter()
        .filter(|e| e.active)
        .map(|e| {
            (drawn.nodes[e.end as usize].position - drawn.nodes[e.start as usize].position).length()
        })
        .sum()
}

/// #1558: the tidy keeps a sound plan's streets and lots across the
/// editor's range, on the pilot terrain: the default and the editor's
/// widest streets (8/6 m) on the default 95/55 m spacing and on a dense
/// 60/30 m, a record's 12/8 m on 60/30 m, and the sparse end - 200/100 m,
/// 250/125 m and the editor's widest spacing, 500/400 m - in the default
/// district (170 m) and a 300 m one, Hillside and Grid, for the road seed
/// the end review traced (1) and the pilot's. The bounds:
/// - the drawn street length keeps at least two thirds of the original
///   plan's - doubles, loops, stubs and clusters are never a third of a
///   sound plan;
/// - the lots keep at least two thirds of what the original plan grows
///   once cut to the drawn district and cleared of its streets - the two
///   losses revision 1 takes on purpose (a block bounded by a street nobody
///   sees grows nothing, and no lot stands on a street), so the rest of the
///   tidy costs no third of them;
/// - no junction moves further than its cluster threshold, so no cluster
///   chained.
///
/// With the merge unbounded the dense 8/6 m plan kept 45% of its streets
/// and one lot; with the loop streets judged after the district cut, seed 1
/// at 200/100 m kept 146 m of 1,059 m and no lot of 41.
#[test]
fn the_tidy_never_collapses_a_wide_dense_or_sparse_plan() {
    use crate::pds::generator::RoadStyle;
    use crate::pds::types::Fp;
    let hm = pilot_heightmap();
    let mut plans: Vec<RoadConfig> = [
        (3.5, 2.0, 95.0, 55.0),
        (8.0, 6.0, 95.0, 55.0),
        (8.0, 6.0, 60.0, 30.0),
        (12.0, 8.0, 60.0, 30.0),
    ]
    .into_iter()
    .map(|(major_w, minor_w, major_s, minor_s)| RoadConfig {
        major_half_width: Fp(major_w),
        minor_half_width: Fp(minor_w),
        major_spacing: Fp(major_s),
        minor_spacing: Fp(minor_s),
        ..cfg(PILOT_ROAD_SEED)
    })
    .collect();
    for (major_s, minor_s) in [(200.0, 100.0), (250.0, 125.0), (500.0, 400.0)] {
        for extent in [170.0, 300.0] {
            for style in [RoadStyle::Hillside, RoadStyle::Grid] {
                for seed in [1, PILOT_ROAD_SEED] {
                    plans.push(RoadConfig {
                        seed,
                        style,
                        major_spacing: Fp(major_s),
                        minor_spacing: Fp(minor_s),
                        district_half_extent: Fp(extent),
                        ..RoadConfig::default()
                    });
                }
            }
        }
    }
    let (mut with_streets, mut with_lots) = (0, 0);
    for original in plans {
        let name = format!(
            "{}/{} m streets at {}/{} m, district {} m, {:?}, seed {}",
            original.major_half_width.0,
            original.minor_half_width.0,
            original.major_spacing.0,
            original.minor_spacing.0,
            original.district_half_extent.0,
            original.style,
            original.seed
        );
        let tidy = RoadConfig {
            layout_revision: 1,
            ..original.clone()
        };
        let (plan, sub, _) = build_road_graph(&hm, &original, None).expect("traces");
        let (tidied_plan, _, _) = build_road_graph(&hm, &tidy, None).expect("traces");

        let (was, now) = (drawn_length(&plan, &sub), drawn_length(&tidied_plan, &sub));
        with_streets += usize::from(was > 0.0);
        assert!(
            3.0 * now >= 2.0 * was,
            "{name}: the tidy kept {now:.0} m of {was:.0} m of drawn street"
        );

        let mut clipped = plan.clone();
        clip_to_district(&mut clipped, &sub);
        let base = cleared_lots(&clipped, &sub, &original);
        let lots = crate::urban::extract_building_lots(&hm, &tidy, None).len();
        assert!(
            3 * lots >= 2 * base,
            "{name}: {lots} lots, against {base} on the original plan cut to the district and cleared"
        );
        with_lots += usize::from(base > 0);

        let reach = TidyBounds::new(&tidy).cluster_len;
        let degree = active_degrees(&tidied_plan);
        for (i, (a, b)) in plan.nodes.iter().zip(&tidied_plan.nodes).enumerate() {
            let moved = (b.position - a.position).length();
            assert!(
                degree[i] == 0 || moved <= reach + 1.0e-3,
                "{name}: junction {i} moved {moved:.1} m (threshold {reach:.1} m)"
            );
        }
    }
    // 24 of the 28 draw streets (the widest spacing in the default district
    // draws none) and 15 grow lots: neither bound is vacuous.
    assert!(
        with_streets >= 20 && with_lots >= 12,
        "the plans exercise the bounds: {with_streets} of 28 draw streets, {with_lots} grow lots"
    );
}

/// #1558, the end review: a real block near the district's edge whose
/// corners lose their streets to the cut is a block, not a loop street.
/// With two of its corners left as junctions the long way round it reads
/// as one 120 m street beside its 20 m side - a detour by the length
/// ratio alone - and with one corner left it reads as a loop round 1,200
/// m2, under a quarter of a 95 x 55 m block. Judged after the cut, the
/// tidy dropped three sides of the first and all of the second.
#[test]
fn the_tidy_keeps_a_block_whose_corners_the_district_edge_cut() {
    // Two corners keep streets inward; the far two lose theirs (they end
    // outside the drawn circle, 112.6 m round (128, 128)).
    let mut lens = typed_graph(
        &[
            (180.0, 110.0),
            (180.0, 130.0),
            (230.0, 110.0),
            (230.0, 130.0),
            (100.0, 110.0),
            (100.0, 130.0),
            (250.0, 110.0),
            (250.0, 130.0),
        ],
        &[
            (0, 2, false),
            (2, 3, false),
            (3, 1, false),
            (1, 0, false),
            (4, 0, false),
            (5, 1, false),
            (2, 6, false),
            (3, 7, false),
        ],
    );
    tidy_graph(&mut lens, &tidy_window(), &tidied());
    assert!(
        (0..6).all(|e| active(&lens, e)),
        "the block with two junctions left keeps every side"
    );

    // One corner keeps its street inward; the other three lose theirs.
    let mut ring = typed_graph(
        &[
            (180.0, 110.0),
            (220.0, 110.0),
            (220.0, 140.0),
            (180.0, 140.0),
            (100.0, 110.0),
            (250.0, 110.0),
            (250.0, 140.0),
            (180.0, 245.0),
        ],
        &[
            (0, 1, false),
            (1, 2, false),
            (2, 3, false),
            (3, 0, false),
            (4, 0, false),
            (1, 5, false),
            (2, 6, false),
            (3, 7, false),
        ],
    );
    tidy_graph(&mut ring, &tidy_window(), &tidied());
    assert!(
        (0..5).all(|e| active(&ring, e)),
        "the block with one junction left keeps every side"
    );
    assert!(
        (5..8).all(|e| !active(&ring, e)),
        "the streets past the district's edge are cut"
    );
}

/// #1558: the tidy runs to a fixed point with no pass cap in the
/// derivation - one more pass after it removes nothing - and it never
/// erodes a long dead end edge by edge, as the sanitiser's stub cut (an
/// edge under 8 m at a dead end) would, one edge a pass.
#[test]
fn the_tidy_settles_and_erodes_no_dead_end() {
    // A 60 m dead end of 5 m edges off a through street.
    let mut nodes = vec![(40.0, 128.0), (128.0, 128.0), (216.0, 128.0)];
    nodes.extend((1..=12).map(|k| (128.0, 128.0 - 5.0 * k as f32)));
    let mut edges = vec![(0, 1, true), (1, 2, true), (1, 3, false)];
    edges.extend((3..14).map(|j| (j, j + 1, false)));
    let mut g = typed_graph(&nodes, &edges);
    tidy_graph(&mut g, &tidy_window(), &tidied());
    assert!(
        g.edges.iter().all(|e| e.active),
        "the 60 m dead end stays whole"
    );
    assert!(
        !tidy_pass(&mut g, &TidyBounds::new(&tidied())),
        "the tidy settled"
    );

    use crate::pds::types::Fp;
    let hm = pilot_heightmap();
    for (major_s, minor_s) in [(95.0, 55.0), (70.0, 35.0), (45.0, 25.0)] {
        let config = RoadConfig {
            major_spacing: Fp(major_s),
            minor_spacing: Fp(minor_s),
            layout_revision: 1,
            ..cfg(PILOT_ROAD_SEED)
        };
        let (mut plan, _, _) = build_road_graph(&hm, &config, None).expect("traces");
        assert!(
            !tidy_pass(&mut plan, &TidyBounds::new(&config)),
            "{major_s}/{minor_s} m: the tidy left something to remove"
        );
    }
}

/// FNV-1a over a graph's node positions and edges, every bit.
fn graph_hash(g: &RoadGraph) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x100_0000_01b3);
        }
    };
    for n in &g.nodes {
        feed(&n.position.x.to_bits().to_le_bytes());
        feed(&n.position.y.to_bits().to_le_bytes());
    }
    for e in &g.edges {
        feed(&e.start.to_le_bytes());
        feed(&e.end.to_le_bytes());
        feed(&[
            u8::from(e.active),
            u8::from(matches!(e.road_type, RoadType::Major)),
        ]);
    }
    h
}

/// FNV-1a over a graph's topology: every edge's ends, active flag and road
/// class - integers only, the same on every platform that traced the same
/// plan.
fn topology_hash(g: &RoadGraph) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x100_0000_01b3);
        }
    };
    for e in &g.edges {
        feed(&e.start.to_le_bytes());
        feed(&e.end.to_le_bytes());
        feed(&[
            u8::from(e.active),
            u8::from(matches!(e.road_type, RoadType::Major)),
        ]);
    }
    h
}

/// A fixed pseudo-random weight in `[0, 1)` for item `i` of a pinned list,
/// so a weighted sum moves when one item moves and the plain sums of two
/// moved items cancel.
fn pin_weight(i: usize) -> f64 {
    ((i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 11) as f64 / (1_u64 << 53) as f64
}

/// The plain and weighted sums of each column of `rows`, in f64: the sum
/// of every row's value and of every row's value times [`pin_weight`] of
/// its index, column by column.
fn pin_sums<const N: usize>(rows: impl Iterator<Item = [f64; N]>) -> Vec<f64> {
    let mut sums = vec![0.0_f64; 2 * N];
    for (i, row) in rows.enumerate() {
        let w = pin_weight(i);
        for (k, v) in row.into_iter().enumerate() {
            sums[k] += v;
            sums[N + k] += w * v;
        }
    }
    sums
}

/// How one network on the original plan is pinned (#1558): its graph's
/// counts and topology exactly, its street-furniture count exactly, and its
/// floats - node positions, spot positions and yaws, and where the lots are
/// pinned whole their positions, sizes and yaws - through [`pin_sums`],
/// which a last-bit difference in a platform's libm moves by far less than
/// [`PIN_TOLERANCE`] and any real change by far more.
#[derive(Debug)]
struct PlanPin {
    nodes: usize,
    edges: usize,
    topology: u64,
    node_sums: Vec<f64>,
    spots: usize,
    spot_sums: Vec<f64>,
    lots: usize,
    /// The lots' sums, where the fixture's lot subdivision holds under a
    /// one-ulp change in every maths function; `None` pins the lot count
    /// alone, within [`LOT_COUNT_BAND`].
    lot_sums: Option<Vec<f64>>,
}

/// How far (m, or for a yaw's cosine and sine, unitless) a pinned sum may
/// move. Under the `libm` crate, or any one maths function nudged one ulp
/// either way, these fixtures' sums moved by at most 4e-4; one street node
/// 10 cm off, or one spot or lot turned 5 degrees, moves one by more.
const PIN_TOLERANCE: f64 = 0.05;

/// How far (a fraction) a lot count pinned alone may move. symbios-tensor's
/// lot subdivision breaks near-ties by the last bits of the traced node
/// positions: with `acosf` or `tanf` (the tracer's `rationalize`) nudged one
/// ulp, the blocks stay the same and the plain pilot grows 208 or 209 lots
/// instead of 207, the small room 106 or 107 instead of 104 (#1563). The
/// lot path itself running differently - the street clearance at revision
/// 0 drops a third of them - moves it by far more.
const LOT_COUNT_BAND: f64 = 0.05;

impl PlanPin {
    /// The pin of the network `config` traces on `hm`, its lots pinned
    /// whole when `whole_lots`.
    fn of(hm: &HeightMap, config: &RoadConfig, water: Option<f32>, whole_lots: bool) -> Self {
        let (g, _, _) = build_road_graph(hm, config, water).expect("traces");
        let lots = crate::urban::extract_building_lots(hm, config, water);
        let spots = crate::urban::extract_furniture_spots(hm, config, water);
        let f = f64::from;
        Self {
            nodes: g.nodes.len(),
            edges: g.edges.len(),
            topology: topology_hash(&g),
            node_sums: pin_sums(g.nodes.iter().map(|n| [f(n.position.x), f(n.position.y)])),
            spots: spots.len(),
            spot_sums: pin_sums(spots.iter().map(|s| {
                let yaw = f(s.yaw);
                [f(s.position[0]), f(s.position[1]), yaw.cos(), yaw.sin()]
            })),
            lots: lots.len(),
            lot_sums: whole_lots.then(|| {
                pin_sums(lots.iter().map(|l| {
                    let yaw = f(l.yaw);
                    [
                        f(l.position[0]),
                        f(l.position[1]),
                        f(l.width),
                        f(l.depth),
                        yaw.cos(),
                        yaw.sin(),
                    ]
                }))
            }),
        }
    }

    /// Every way `self` (traced now) differs from `want` (the pin).
    fn differences(&self, want: &PlanPin) -> Vec<String> {
        let mut out = Vec::new();
        for (what, got, was) in [
            ("nodes", self.nodes, want.nodes),
            ("edges", self.edges, want.edges),
            ("spots", self.spots, want.spots),
        ] {
            if got != was {
                out.push(format!("{what}: {got}, pinned {was}"));
            }
        }
        if self.topology != want.topology {
            out.push(format!(
                "topology: {:#x}, pinned {:#x}",
                self.topology, want.topology
            ));
        }
        let lot_band = if want.lot_sums.is_some() {
            0.0
        } else {
            LOT_COUNT_BAND * want.lots as f64
        };
        if (self.lots as f64 - want.lots as f64).abs() > lot_band {
            out.push(format!("lots: {}, pinned {}", self.lots, want.lots));
        }
        let none = Vec::new();
        for (what, got, was) in [
            ("node sums", &self.node_sums, &want.node_sums),
            ("spot sums", &self.spot_sums, &want.spot_sums),
            (
                "lot sums",
                self.lot_sums.as_ref().unwrap_or(&none),
                want.lot_sums.as_ref().unwrap_or(&none),
            ),
        ] {
            for (k, (g, w)) in got.iter().zip(was).enumerate() {
                if (g - w).abs() > PIN_TOLERANCE {
                    out.push(format!("{what}[{k}]: {g}, pinned {w}"));
                }
            }
        }
        out
    }
}

/// The three networks the original plan is pinned on: the pilot network
/// plain and shaped by every field it can carry (a street field, a
/// district centre, the shore, a lot area), and a small sloped room - each
/// with its street furniture on.
fn pinned_networks() -> Vec<(&'static str, HeightMap, RoadConfig, Option<f32>)> {
    use crate::pds::generator::{RoadBasis, RoadKeepOut};
    use crate::pds::types::Fp;
    let pilot = pilot_heightmap();
    let mut heights: Vec<f32> = pilot.data().to_vec();
    heights.sort_by(f32::total_cmp);
    let level = heights[heights.len() / 4];
    let mut plain = cfg(PILOT_ROAD_SEED);
    plain.furniture.enabled = true;
    let mut shaped = cfg(PILOT_ROAD_SEED);
    shaped.furniture.enabled = true;
    shaped.avoid_water = true;
    shaped.center = Fp2([20.0, -30.0]);
    shaped.lots.lot_area = Fp(1500.0);
    shaped.major_spacing = Fp(80.0);
    shaped.minor_spacing = Fp(40.0);
    shaped.field = RoadField {
        smoothing: Fp(20.0),
        basis: vec![RoadBasis::Ring {
            center: Fp2([10.0, -60.0]),
            radius: Fp(200.0),
            strength: Fp(2.0),
        }],
        keep_out: vec![RoadKeepOut {
            center: Fp2([-40.0, 30.0]),
            radius: Fp(25.0),
        }],
        ..RoadField::default()
    };
    let mut small = cfg(7);
    small.furniture.enabled = true;
    let shaped_ground = pilot_heightmap();
    vec![
        ("plain", pilot, plain, None),
        ("shaped", shaped_ground, shaped, Some(level)),
        ("small", sloped_heightmap(), small, None),
    ]
}

/// The plain pilot network's pin, traced by the build before #1558 (whose
/// graph, lots and spots this tree's revision 0 matched to the bit on the
/// machine that captured it). Its lots are pinned by count alone.
fn pinned_plain() -> PlanPin {
    PlanPin {
        nodes: 2920,
        edges: 3304,
        topology: 0xf654_cc12_c939_41e8,
        node_sums: vec![
            522666.56205666065,
            520330.34686243534,
            261460.8449460137,
            259590.18030474897,
        ],
        spots: 87,
        spot_sums: vec![
            907.9129943847656,
            -821.1902618408203,
            3.839443727382623,
            -8.962750328360713,
            540.0675477850591,
            -592.0832878891562,
            -2.7115397854574095,
            -8.572693970034953,
        ],
        lots: 207,
        lot_sums: None,
    }
}

/// The shaped pilot network's pin, as [`pinned_plain`]; its lots hold whole.
fn pinned_shaped() -> PlanPin {
    PlanPin {
        nodes: 3465,
        edges: 4082,
        topology: 0xab3a_771c_da07_eaa1,
        node_sums: vec![
            710562.1651901007,
            666336.3959286846,
            355422.2376542296,
            333352.9370282754,
        ],
        spots: 105,
        spot_sums: vec![
            5334.795616149902,
            -564.8098983764648,
            -5.718796270333629,
            -3.7127522811153715,
            2480.2665631032423,
            -50.81371392122714,
            -2.6688757564669774,
            -0.8838416312139105,
        ],
        lots: 37,
        lot_sums: Some(vec![
            3054.5794525146484,
            -951.6087112426758,
            908.9880628585815,
            486.62370586395264,
            -1.2808264225229387,
            0.6682762698235367,
            1403.7710256477442,
            -444.0154063496208,
            429.66553626484284,
            247.70797544883587,
            -1.7768739117436432,
            -0.4891255250283597,
        ]),
    }
}

/// The small sloped room's pin, as [`pinned_plain`]; lots by count alone.
fn pinned_small() -> PlanPin {
    PlanPin {
        nodes: 1636,
        edges: 1837,
        topology: 0xb471_d8ae_4a7f_9d45,
        node_sums: vec![
            213194.6710704565,
            213453.59724402428,
            106510.24551447699,
            106566.06325603693,
        ],
        spots: 40,
        spot_sums: vec![
            201.2791290283203,
            206.02708435058594,
            3.5884067605582435,
            0.7988383301164014,
            90.7738993808347,
            169.12408208801574,
            0.26075438667022954,
            1.6733259414013502,
        ],
        lots: 104,
        lot_sums: None,
    }
}

/// #1558: a network on the original street plan (layout revision 0) is
/// traced, lotted and furnished as before the tidy existed, against the
/// plan the build before it traced: every edge's ends, flag and class and
/// every node and spot count exactly, every node and spot within
/// [`PIN_TOLERANCE`] through its sums, and the lots whole on the shaped
/// network and by count on the other two (see [`LOT_COUNT_BAND`]). The
/// floats are pinned through sums, not bits, because they flow through the
/// platform's libm (the tracer's `rationalize`, `extract_blocks`,
/// `extract_lots`, the spots' yaw), whose last bits differ between CI's
/// glibc, the wasm build and a newer glibc. Seen holding under an
/// LD_PRELOAD of the `libm` crate and with each of 31 f32 and f64 maths
/// functions nudged one ulp either way.
#[test]
fn the_original_street_plan_is_traced_and_lotted_as_before_the_tidy() {
    let pinned = [pinned_plain(), pinned_shaped(), pinned_small()];
    for ((name, hm, config, water), want) in pinned_networks().into_iter().zip(&pinned) {
        assert_eq!(config.layout_revision, 0);
        let got = PlanPin::of(&hm, &config, water, want.lot_sums.is_some());
        let differences = got.differences(want);
        assert!(
            differences.is_empty(),
            "{name}: the original plan moved: {differences:#?}\nnow {got:#?}"
        );
    }
}

/// Everything layout revision 2 derives for a network (#1563): the drawn
/// graph's every node and edge, every lot and every street-prop spot,
/// hashed to the bit.
fn district_hash(hm: &HeightMap, config: &RoadConfig, water: Option<f32>) -> u64 {
    let (g, _, _) = build_road_graph(hm, config, water).expect("traces");
    let lots = crate::urban::extract_building_lots(hm, config, water);
    let spots = crate::urban::extract_furniture_spots(hm, config, water);
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut eat = |v: u64| {
        for byte in v.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    eat(g.nodes.len() as u64);
    for n in &g.nodes {
        eat(n.position.x.to_bits().into());
        eat(n.position.y.to_bits().into());
    }
    eat(g.edges.len() as u64);
    for e in &g.edges {
        eat(e.start.into());
        eat(e.end.into());
        eat(e.active.into());
        eat(matches!(e.road_type, RoadType::Major).into());
    }
    eat(lots.len() as u64);
    for lot in &lots {
        for v in [
            lot.position[0],
            lot.position[1],
            lot.yaw,
            lot.width,
            lot.depth,
        ] {
            eat(v.to_bits().into());
        }
    }
    eat(spots.len() as u64);
    for spot in &spots {
        for v in [spot.position[0], spot.position[1], spot.yaw] {
            eat(v.to_bits().into());
        }
    }
    hash
}

/// The pinned networks at layout revision 2, and the plain one in the
/// organic style, whose jitter turns the field with `sin` and `cos`.
fn portable_networks() -> Vec<(&'static str, HeightMap, RoadConfig, Option<f32>)> {
    use crate::pds::generator::RoadStyle;
    let mut networks = pinned_networks();
    let organic = RoadConfig {
        style: RoadStyle::Organic,
        ..networks[0].2.clone()
    };
    networks.push(("organic", pilot_heightmap(), organic, None));
    for (_, _, config, _) in &mut networks {
        config.layout_revision = 2;
    }
    networks
}

/// [`portable_networks`]' hashes, in order.
const PORTABLE_PINS: [u64; 4] = [
    0x712c_9245_26fd_3819,
    0x822b_a4f9_97dc_b565,
    0x803c_27d9_3c3d_3093,
    0x246c_bf48_efa4_84db,
];

/// #1563: layout revision 2 derives the same district on every client.
/// The platform's maths answer `sin`, `acos`, `atan2` and `hypot`
/// differently in the last bit - CI's glibc and this machine's do, as do a
/// native client and the web one - and revisions 0 and 1 turn such bits
/// into different lots ([`LOT_COUNT_BAND`]); revision 2 takes every one of
/// them from the `libm` crate, so its pins are exact. They were recorded
/// with every platform maths function interposed by an `LD_PRELOAD` shim
/// and nudged three ulps, and without it, alike, with no platform maths
/// call made on the way.
#[test]
fn layout_revision_2_derives_the_same_district_on_every_platform() {
    let got: Vec<u64> = portable_networks()
        .iter()
        .map(|(name, hm, config, water)| {
            let hash = district_hash(hm, config, *water);
            println!("revision 2 {name}: {hash:#018x}");
            hash
        })
        .collect();
    assert_eq!(
        got, PORTABLE_PINS,
        "a district at layout revision 2 derived differently"
    );
}

/// The revision 2 pins hold whole districts, or they would prove nothing
/// about the lots and props: every network grows streets, lots and props.
#[test]
fn the_revision_2_pins_hold_whole_districts() {
    for (name, hm, config, water) in portable_networks() {
        let lots = crate::urban::extract_building_lots(&hm, &config, water);
        let spots = crate::urban::extract_furniture_spots(&hm, &config, water);
        assert!(
            lots.len() >= 10 && spots.len() >= 20,
            "{name}: {} lots, {} props",
            lots.len(),
            spots.len()
        );
    }
}

/// #1558: on the pilot network the tidy leaves no stub, no loop street and
/// no street off the drawn district, and no short street between two
/// junctions it could have merged - each of which the original plan has -
/// and traces the same plan twice. A short street the tidy leaves joins a
/// junction that already stands for a cluster as wide as the threshold (it
/// moved to the cluster's middle), which is why it was not merged further.
#[test]
fn the_tidy_cleans_the_pilot_plan() {
    let hm = pilot_heightmap();
    let count = |config: &RoadConfig| {
        let (g, sub, _) = build_road_graph(&hm, config, None).expect("traces");
        let degree = active_degrees(&g);
        let chains = graph_chains(&g);
        let ends = |c: &GraphChain| (c.nodes[0], *c.nodes.last().expect("nodes"));
        let clusters: Vec<(usize, usize)> = chains
            .iter()
            .filter(|c| {
                let (a, b) = ends(c);
                a != b && degree[a] >= 3 && degree[b] >= 3 && c.len < 2.5 * (3.5 + 0.62)
            })
            .map(ends)
            .collect();
        let stubs = chains
            .iter()
            .filter(|c| {
                let (a, b) = ends(c);
                a != b && (degree[a] == 1 || degree[b] == 1) && c.len < 27.5
            })
            .count();
        let loops = chains.iter().filter(|c| ends(c).0 == ends(c).1).count();
        let drawn = crate::urban::drawn_graph(&g, &sub);
        let off_district = g
            .edges
            .iter()
            .zip(&drawn.edges)
            .filter(|(e, d)| e.active && !d.active)
            .count();
        let positions: Vec<_> = g.nodes.iter().map(|n| n.position).collect();
        (
            clusters,
            stubs,
            loops,
            off_district,
            graph_hash(&g),
            positions,
        )
    };
    let original = count(&cfg(PILOT_ROAD_SEED));
    let mut config = cfg(PILOT_ROAD_SEED);
    config.layout_revision = 1;
    let tidy = count(&config);
    assert!(
        !original.0.is_empty() && original.1 > 0 && original.3 > 0,
        "the original plan has clusters, stubs and streets off the district: {:?}",
        (original.0.len(), original.1, original.2, original.3)
    );
    assert_eq!(
        (tidy.1, tidy.2, tidy.3),
        (0, 0, 0),
        "the tidy left stubs, loops or streets off the district"
    );
    assert!(
        tidy.0.len() < original.0.len(),
        "the tidy merged no cluster"
    );
    let moved = |i: usize| tidy.5[i] != original.5[i];
    for &(a, b) in &tidy.0 {
        assert!(
            moved(a) || moved(b),
            "a short street between two unmerged junctions {a} and {b} is left"
        );
    }
    assert_eq!(count(&config).4, tidy.4, "the tidy is deterministic");
}
