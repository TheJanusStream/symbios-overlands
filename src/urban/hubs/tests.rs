use super::*;
use crate::urban::math::{cross, dot, normalize, sub3};
use crate::urban::test_support::*;
use crate::urban::truncation::Hub;
use crate::urban::{Chain, Dims, RoadParts, mesh_road_graph, plan_junctions};
use bevy_symbios_ground::HeightMap;
use symbios_tensor::RoadGraph;

/// A mouth `t` metres out from the origin along `ang` (radians), heading
/// away from a hub centred on the origin.
fn arm(ang: f32, t: f32, half_w: f32, deck_y: f32) -> RoadEnd {
    let (dx, dz) = (ang.cos(), ang.sin());
    RoadEnd {
        hub: 0,
        node: 0,
        cx: dx * t,
        cz: dz * t,
        dx,
        dz,
        half_w,
        deck_y,
        skirt_y: deck_y - 5.0,
        spine: vec![(0.0, 0.0), (dx * t, dz * t)],
    }
}

/// The mouth's outer-curb point on side `sgn` (the ribbon's chamfer base).
fn outer_point(e: &RoadEnd, sgn: f32, wo: f32) -> [f32; 3] {
    [e.cx - sgn * e.dz * wo, e.deck_y, e.cz + sgn * e.dx * wo]
}

fn near(verts: &[[f32; 3]], p: [f32; 3]) -> bool {
    verts.iter().any(|v| {
        (v[0] - p[0]).abs() < 1.0e-3 && (v[1] - p[1]).abs() < 1.0e-3 && (v[2] - p[2]).abs() < 1.0e-3
    })
}

/// The triangles of one surface, as XZ-plane corner triples with their
/// stored winding.
fn triangles(g: &crate::urban::RoadGeometry) -> Vec<[[f32; 3]; 3]> {
    g.indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| t.map(|i| g.vertices[i as usize]))
        .collect()
}

/// Twice the signed XZ area of a triangle, positive when its stored winding
/// faces up (+Y).
fn up_area2(t: &[[f32; 3]; 3]) -> f32 {
    cross(sub3(t[1], t[0]), sub3(t[2], t[0]))[1]
}

/// Whether two XZ triangles' interiors overlap - a separating-axis test on
/// copies shrunk 1 mm toward their centroids, so triangles that merely share
/// an edge or a corner never count.
fn interiors_overlap(a: &[[f32; 3]; 3], b: &[[f32; 3]; 3]) -> bool {
    let shrink = |t: &[[f32; 3]; 3]| {
        let c = [
            (t[0][0] + t[1][0] + t[2][0]) / 3.0,
            (t[0][2] + t[1][2] + t[2][2]) / 3.0,
        ];
        t.map(|p| {
            let (dx, dz) = (p[0] - c[0], p[2] - c[1]);
            let l = dx.hypot(dz).max(1.0e-6);
            let k = ((l - 1.0e-3) / l).max(0.0);
            [c[0] + dx * k, c[1] + dz * k]
        })
    };
    let (pa, pb) = (shrink(a), shrink(b));
    for poly in [&pa, &pb] {
        for i in 0..3 {
            let (p, q) = (poly[i], poly[(i + 1) % 3]);
            let axis = [q[1] - p[1], p[0] - q[0]];
            let proj = |t: &[[f32; 2]; 3]| {
                let v: Vec<f32> = t.iter().map(|c| c[0] * axis[0] + c[1] * axis[1]).collect();
                (
                    v.iter().copied().fold(f32::INFINITY, f32::min),
                    v.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                )
            };
            let ((a0, a1), (b0, b1)) = (proj(&pa), proj(&pb));
            if a1 <= b0 || b1 <= a0 {
                return false;
            }
        }
    }
    true
}

/// The deck of a hub (or a whole mesh) is sound: every triangle finite,
/// non-degenerate and wound to face up, and no two overlapping (a fold).
fn assert_deck_sound(deck: &crate::urban::RoadGeometry, what: &str) {
    let tris = triangles(deck);
    assert!(!tris.is_empty(), "{what}: no deck");
    for t in &tris {
        assert!(
            t.iter().flatten().all(|c| c.is_finite()),
            "{what}: non-finite deck triangle {t:?}"
        );
        let a2 = up_area2(t);
        assert!(
            a2 > 2.0e-4,
            "{what}: deck triangle degenerate or facing down (2·area {a2}): {t:?}"
        );
    }
    for (i, a) in tris.iter().enumerate() {
        for b in &tris[i + 1..] {
            assert!(
                !interiors_overlap(a, b),
                "{what}: deck triangles overlap (a fold): {a:?} / {b:?}"
            );
        }
    }
    for n in &deck.normals {
        assert!(n[1] > 0.0, "{what}: deck normal {n:?} not upward");
    }
}

/// No curb top or chamfer (structure at or above the deck height `deck_y`)
/// lies over the asphalt: no shard poking out of the deck.
fn assert_no_curb_over_deck(parts: &RoadParts, deck_y: f32, what: &str) {
    let deck = triangles(&parts.deck);
    for s in triangles(&parts.structure) {
        if s.iter().any(|p| p[1] < deck_y - 1.0e-2) || up_area2(&s).abs() < 1.0e-4 {
            continue; // a skirt, a bottom cap, or a vertical wall
        }
        for d in &deck {
            assert!(
                !interiors_overlap(&s, d),
                "{what}: curb over the asphalt: {s:?} over deck {d:?}"
            );
        }
    }
}

/// Every hub deck vertex lies inside the junction: within some arm's deck
/// strip behind its mouth, or within the widest half-width of the hub
/// centre - and every curb point likewise within the outer footprint. No
/// spike past the curb line.
fn assert_hub_within_its_arms(
    parts: &RoadParts,
    ends: &[RoadEnd],
    centre: (f32, f32),
    dims: &Dims,
    what: &str,
) {
    let outer = dims.curb_top_width + dims.chamfer_width;
    let reach = |v: &[f32; 3], extra: f32| {
        let w_max = ends.iter().map(|e| e.half_w + extra).fold(0.0, f32::max);
        if (v[0] - centre.0).hypot(v[2] - centre.1) <= w_max + 1.0e-2 {
            return true;
        }
        ends.iter().any(|e| {
            let (rx, rz) = (v[0] - e.cx, v[2] - e.cz);
            let along = rx * e.dx + rz * e.dz;
            let across = (rx * -e.dz + rz * e.dx).abs();
            along <= 1.0e-2 && across <= e.half_w + extra + 1.0e-2
        })
    };
    for v in &parts.deck.vertices {
        assert!(
            reach(v, 0.0),
            "{what}: deck vertex {v:?} outside the junction"
        );
    }
    for v in &parts.structure.vertices {
        assert!(
            reach(v, outer),
            "{what}: curb vertex {v:?} past the curb line"
        );
    }
}

/// The hub `ends` open into (all one hub of `hubs`) sound on every count:
/// deck, curbs and envelope.
fn assert_sound_hub(ends: &[RoadEnd], hubs: &[Hub], dims: &Dims, what: &str) -> RoadParts {
    let mut parts = RoadParts::default();
    extrude_hubs(ends, hubs, [0.0; 2], dims, &mut parts);
    assert_deck_sound(&parts.deck, what);
    let deck_y = ends.iter().map(|e| e.deck_y).fold(f32::MIN, f32::max);
    assert_no_curb_over_deck(&parts, deck_y, what);
    assert_hub_within_its_arms(&parts, ends, hubs[ends[0].hub].centre, dims, what);
    parts
}

/// The hub `ends` open into sound in its deck and envelope - for real
/// networks whose near-parallel streets overlap (the graph tidy's business),
/// where two such arms' curbs can lie over each other's asphalt.
fn assert_hub_deck_sound(ends: &[RoadEnd], hubs: &[Hub], dims: &Dims, what: &str) {
    let mut parts = RoadParts::default();
    extrude_hubs(ends, hubs, [0.0; 2], dims, &mut parts);
    assert_deck_sound(&parts.deck, what);
    assert_hub_within_its_arms(&parts, ends, hubs[ends[0].hub].centre, dims, what);
}

/// The one-node hub at the origin sound on every count.
fn assert_hub_sound(ends: &[RoadEnd], dims: &Dims, what: &str) -> RoadParts {
    assert_sound_hub(ends, &[hub_at((0.0, 0.0))], dims, what)
}

/// Extrude `ends` round a one-node hub at the origin.
fn origin_hub(ends: &[RoadEnd], dims: &Dims) -> RoadParts {
    let mut parts = RoadParts::default();
    extrude_hubs(ends, &[hub_at((0.0, 0.0))], [0.0; 2], dims, &mut parts);
    parts
}

/// WS4: a junction grows a real hub - a deck polygon meeting each incident
/// road at its mouth (both corners of every arm) plus curb/skirt walls closing
/// the gaps - not the old circular fan.
#[test]
fn hub_meets_each_road_and_closes_gaps() {
    let dims = Dims::from_config(&cfg(7));
    let third = std::f32::consts::TAU / 3.0;
    let ends = [
        arm(0.0, 5.0, 4.0, 1.0),
        arm(third, 5.0, 4.0, 1.0),
        arm(2.0 * third, 5.0, 4.0, 1.0),
    ];
    let parts = assert_hub_sound(&ends, &dims, "Y");
    for e in &ends {
        for sgn in [-1.0_f32, 1.0] {
            let c = [
                e.cx - sgn * e.dz * e.half_w,
                e.deck_y,
                e.cz + sgn * e.dx * e.half_w,
            ];
            assert!(
                near(&parts.deck.vertices, c),
                "mouth corner {c:?} not on the deck"
            );
        }
    }
    assert!(!parts.structure.is_empty(), "hub gaps left unclosed");
    for v in &parts.deck.vertices {
        assert!(
            v[1] + 1.0e-4 >= 1.0,
            "hub deck vertex {v:?} below the roads"
        );
    }
}

/// #584: the hub deck is held at the MAX incident mouth height - never the
/// mean (which would droop below the highest road) - and each mouth corner
/// keeps its own height so the seam holds even before the levelling pins
/// them. Once the mouths are level the whole deck is one flat plane.
#[test]
fn hub_is_flat_at_the_max_incident_mouth() {
    let dims = Dims::from_config(&cfg(7));
    let third = std::f32::consts::TAU / 3.0;
    let mixed = [
        arm(0.0, 5.0, 4.0, 1.0),
        arm(third, 5.0, 4.0, 2.0),
        arm(2.0 * third, 5.0, 4.0, 3.0),
    ];
    let parts = origin_hub(&mixed, &dims);
    let top = parts
        .deck
        .vertices
        .iter()
        .map(|v| v[1])
        .fold(f32::MIN, f32::max);
    assert!(
        (top - 3.0).abs() < 1.0e-3,
        "the deck tops out at {top}, not the max mouth 3.0"
    );
    assert!(
        parts
            .deck
            .vertices
            .iter()
            .any(|v| (v[1] - 1.0).abs() < 1.0e-3),
        "lowest mouth not met seamlessly"
    );
    for v in &parts.deck.vertices {
        assert!(
            v[1] >= 1.0 - 1.0e-3,
            "deck vertex {v:?} droops below every mouth"
        );
    }

    let level = [
        arm(0.0, 5.0, 4.0, 3.0),
        arm(third, 5.0, 4.0, 3.0),
        arm(2.0 * third, 5.0, 4.0, 3.0),
    ];
    let flat = origin_hub(&level, &dims);
    assert!(!flat.deck.vertices.is_empty(), "hub produced no deck");
    for v in &flat.deck.vertices {
        assert!(
            (v[1] - 3.0).abs() < 1.0e-3,
            "hub deck vertex {v:?} not flat at 3.0"
        );
    }
}

/// #576 regression (review wf_39a9f056-ef1): arms truncated to different
/// distances with a deck half-width that rivals the pull-back - the case a
/// node-anchored fan folded on - still give a sound deck.
#[test]
fn hub_deck_stays_simple_with_asymmetric_mouths() {
    let dims = Dims::from_config(&cfg(7));
    let third = std::f32::consts::TAU / 3.0;
    let ends = [
        arm(0.0, 4.0, 4.0, 1.0),
        arm(third, 8.0, 4.0, 1.0),
        arm(2.0 * third, 4.5, 4.0, 1.0),
    ];
    assert_hub_sound(&ends, &dims, "asymmetric Y");
}

/// #576 seamlessness: the hub's two mouth corners must land exactly on the
/// ribbon's end deck cross-section, so the deck flows in with no crack or
/// overlap. Drives a real ribbon through `extrude_chain`, then checks the
/// recorded `RoadEnd`'s mouth corners coincide with ribbon deck vertices.
#[test]
fn hub_mouth_corners_coincide_with_the_ribbon_end() {
    let dims = Dims::from_config(&cfg(7));
    let hm = HeightMap::new(64, 64, 2.0);
    let half = dims.minor_half_width;
    // A straight chain; node 1 (the +x end) is a junction, so it records a
    // truncated mouth.
    let chain = Chain {
        pts: vec![(10.0, 10.0), (20.0, 10.0), (40.0, 10.0)],
        half_w: half,
        end_nodes: [0, 1],
        clip: [false, false],
    };
    let degree = vec![1u32, 3u32];
    let mut road_ends = Vec::new();
    let mut parts = RoadParts::default();
    extrude_chain(
        &chain,
        0.0,
        3.0,
        &hm,
        [0.0; 2],
        &dims,
        &degree,
        &mut road_ends,
        &mut parts,
    );
    assert_eq!(road_ends.len(), 1, "the junction end must record a mouth");

    let e = &road_ends[0];
    assert!(
        (e.dx + 1.0).abs() < 1.0e-4 && e.dz.abs() < 1.0e-4,
        "the mouth heads away from its hub (-x), got ({}, {})",
        e.dx,
        e.dz
    );
    for sgn in [-1.0_f32, 1.0] {
        let corner = [
            e.cx - sgn * e.dz * e.half_w,
            e.deck_y,
            e.cz + sgn * e.dx * e.half_w,
        ];
        assert!(
            near(&parts.deck.vertices, corner),
            "hub mouth corner {corner:?} not on the ribbon end (seam)"
        );
    }
}

/// Plan, extrude (each chain levelled on its own) and close `chains` over
/// `hm`: the ribbons, their mouths and the hubs.
fn plan_and_extrude(
    chains: &[Chain],
    degree: &[u32],
    hm: &HeightMap,
    dims: &Dims,
) -> (RoadParts, Vec<RoadEnd>, RoadParts) {
    let plan = plan_junctions(chains, degree, dims);
    let mut road_ends = Vec::new();
    let mut ribbon = RoadParts::default();
    for (ci, c) in chains.iter().enumerate() {
        if plan.internal[ci] {
            continue;
        }
        let [s, e] = plan.trims[ci];
        if let Some(sample) = crate::urban::sample_chain(c, s, e, hm) {
            let floor: Vec<f32> = sample.frames.iter().map(|r| r.floor).collect();
            let base = crate::urban::level_chain(&floor, &sample.seg, [None, None]);
            crate::urban::extrude_ribbon(
                c,
                &sample,
                &base,
                [0.0; 2],
                dims,
                plan.chain_ends(ci),
                &mut road_ends,
                &mut ribbon,
            );
        }
    }
    let mut hub = RoadParts::default();
    extrude_hubs(&road_ends, &plan.hubs, [0.0; 2], dims, &mut hub);
    (ribbon, road_ends, hub)
}

/// A Y of three straight chains meeting at node 0 over `hm`, its arms
/// planned and extruded: the ribbons, their mouths and the hub.
fn y_junction(hm: &HeightMap, dims: &Dims, at: (f32, f32)) -> (RoadParts, Vec<RoadEnd>, RoadParts) {
    let half = dims.minor_half_width;
    let third = std::f32::consts::TAU / 3.0;
    let chains: Vec<Chain> = (0..3)
        .map(|k| {
            let ang = k as f32 * third;
            let (dx, dz) = (ang.cos(), ang.sin());
            Chain {
                pts: vec![
                    at,
                    (at.0 + dx * 15.0, at.1 + dz * 15.0),
                    (at.0 + dx * 40.0, at.1 + dz * 40.0),
                ],
                half_w: half,
                end_nodes: [0, 1 + k],
                clip: [false, false],
            }
        })
        .collect();
    let mut degree = vec![1u32; 4];
    degree[0] = 3;
    let out = plan_and_extrude(&chains, &degree, hm, dims);
    assert_eq!(out.1.len(), 3, "the Y must record three mouths");
    out
}

/// #577: the hub curb is continuous with the incident ribbons - each gap's
/// curb starts and ends exactly on the road's outer-curb point and its skirt
/// foot on the ribbon's skirt bottom, so there's no notch or open band where
/// the hub curb meets the ribbon curb.
#[test]
fn hub_curb_joins_the_ribbon_outer_curbs() {
    let dims = Dims::from_config(&cfg(7));
    let hm = HeightMap::new(96, 96, 2.0);
    let wo = dims.minor_half_width + dims.curb_top_width + dims.chamfer_width;
    let (ribbon, road_ends, hub) = y_junction(&hm, &dims, (50.0, 50.0));
    for e in &road_ends {
        for sgn in [-1.0_f32, 1.0] {
            let o = outer_point(e, sgn, wo);
            assert!(
                near(&ribbon.structure.vertices, o),
                "ribbon curb missing its outer point {o:?}"
            );
            assert!(
                near(&hub.structure.vertices, o),
                "hub curb does not meet the ribbon outer curb at {o:?}"
            );
            let foot = [o[0], e.deck_y - dims.skirt_depth, o[2]];
            assert!(
                near(&ribbon.structure.vertices, foot),
                "ribbon skirt missing its bottom point {foot:?}"
            );
            assert!(
                near(&hub.structure.vertices, foot),
                "hub skirt foot leaves an open band at {foot:?}"
            );
        }
    }
}

/// #577 (verify wf_7f36d6ce LOW): the skirt welds even with a SHALLOW skirt on
/// a CROSS-SLOPE. The hub curb carries each ribbon's recorded `skirt_y`, so the
/// foot lands exactly on the ribbon skirt bottom regardless of depth or slope.
#[test]
fn hub_curb_skirt_welds_on_shallow_cross_slope() {
    let config = crate::pds::generator::RoadConfig {
        skirt_depth: crate::pds::types::Fp(0.5),
        ..cfg(7)
    };
    let dims = Dims::from_config(&config);
    let wo = dims.minor_half_width + dims.curb_top_width + dims.chamfer_width;
    let mut hm = HeightMap::new(96, 96, 2.0);
    let width = hm.width();
    for z in 0..width {
        for x in 0..width {
            hm.set(x, z, x as f32 * 0.3);
        }
    }
    let (ribbon, road_ends, hub) = y_junction(&hm, &dims, (90.0, 90.0));
    let ribbon_floor = |xz: [f32; 2]| {
        ribbon
            .structure
            .vertices
            .iter()
            .filter(|v| (v[0] - xz[0]).abs() < 1.0e-3 && (v[2] - xz[1]).abs() < 1.0e-3)
            .map(|v| v[1])
            .fold(f32::INFINITY, f32::min)
    };
    for e in &road_ends {
        for sgn in [-1.0_f32, 1.0] {
            let o = outer_point(e, sgn, wo);
            let floor = ribbon_floor([o[0], o[2]]);
            assert!(floor.is_finite(), "no ribbon skirt at outer point {o:?}");
            assert!(
                near(&hub.structure.vertices, [o[0], floor, o[2]]),
                "hub skirt foot did not weld to the ribbon skirt bottom {floor} at {o:?}"
            );
        }
    }
}

/// #577: the hub curbs are wound front-out - every structure triangle's
/// geometric normal points away from the hub centre, so back-face culling
/// keeps the curb/skirt visible from outside (no inside-out corner). A
/// symmetric Y keeps the proxy exact.
#[test]
fn hub_curbs_face_out() {
    let dims = Dims::from_config(&cfg(7));
    let third = std::f32::consts::TAU / 3.0;
    let ends = [
        arm(0.0, 5.0, 4.0, 1.0),
        arm(third, 5.0, 4.0, 1.0),
        arm(2.0 * third, 5.0, 4.0, 1.0),
    ];
    let parts = origin_hub(&ends, &dims);
    let center = [0.0_f32, 1.0 - dims.skirt_depth * 0.5, 0.0];
    assert!(!parts.structure.indices.is_empty(), "no hub curb emitted");
    for t in triangles(&parts.structure) {
        let geo = cross(sub3(t[1], t[0]), sub3(t[2], t[0]));
        let mid = [
            (t[0][0] + t[1][0] + t[2][0]) / 3.0,
            (t[0][1] + t[1][1] + t[2][1]) / 3.0,
            (t[0][2] + t[1][2] + t[2][2]) / 3.0,
        ];
        assert!(
            dot(geo, sub3(mid, center)) > 1.0e-6,
            "hub curb triangle faces inward: n·out = {}",
            dot(normalize(geo), normalize(sub3(mid, center)))
        );
    }
}

/// The hub's skirt welds to each arm's recorded `skirt_y` (the ribbon's fixed
/// depth below the deck) and never reaches for the terrain, and no structure
/// pokes above the curb top (no inversion).
#[test]
fn hub_skirt_holds_the_arm_depth_and_nothing_pokes_above_the_curb() {
    let dims = Dims::from_config(&cfg(7));
    let third = std::f32::consts::TAU / 3.0;
    let ends = [
        arm(0.0, 5.0, 4.0, 1.0),
        arm(third, 5.0, 4.0, 1.0),
        arm(2.0 * third, 5.0, 4.0, 1.0),
    ];
    let parts = origin_hub(&ends, &dims);
    let curb_top = 1.0 + dims.curb_height;
    let mut min_y = f32::INFINITY;
    for v in &parts.structure.vertices {
        assert!(
            v[1] <= curb_top + 1.0e-2,
            "structure vertex {v:?} pokes above the curb top"
        );
        min_y = min_y.min(v[1]);
    }
    assert!(
        (min_y - ends[0].skirt_y).abs() < 1.0e-3,
        "skirt foot {min_y} did not weld to the arm skirt depth {}",
        ends[0].skirt_y
    );
}

/// #577: a through road's far edge (two anti-parallel arms with no branch
/// between) stays a STRAIGHT curb along the road's own outer line. A T (arms
/// at 0°/90°/180°): the −z side runs flat at z = −wo.
#[test]
fn hub_through_road_far_edge_stays_straight() {
    let dims = Dims::from_config(&cfg(7));
    let w = 4.0_f32;
    let wo = w + dims.curb_top_width + dims.chamfer_width;
    let (fp2, pi) = (std::f32::consts::FRAC_PI_2, std::f32::consts::PI);
    let ends = [
        arm(0.0, 6.0, w, 1.0),
        arm(fp2, 6.0, w, 1.0),
        arm(pi, 6.0, w, 1.0),
    ];
    let parts = assert_hub_sound(&ends, &dims, "T");
    let min_z = parts
        .structure
        .vertices
        .iter()
        .fold(f32::INFINITY, |m, v| m.min(v[2]));
    assert!(
        min_z >= -wo - 1.0e-2,
        "through-road far edge bulged to z={min_z}, past −wo={}",
        -wo
    );
    for v in &parts.structure.vertices {
        if v[2] < 0.0 && (v[1] - 1.0).abs() < 1.0e-3 && v[2] < -w - 1.0e-2 {
            assert!(
                (v[2] + wo).abs() < 1.0e-2,
                "far-edge curb point {v:?} off the outer line"
            );
        }
    }
}

/// #577 (review wf_55dafda9 HIGH): on a SLOPED / asymmetric hub every emitted
/// structure triangle's geometric winding must agree with its (outward)
/// stored shading normal.
#[test]
fn hub_curb_winding_consistent_on_sloped_hub() {
    let dims = Dims::from_config(&cfg(7));
    let third = std::f32::consts::TAU / 3.0;
    let ends = [
        arm(0.0, 5.0, 4.0, 1.0),
        arm(third, 8.0, 4.0, 2.0),
        arm(2.0 * third, 4.5, 4.0, 4.0),
    ];
    let parts = origin_hub(&ends, &dims);
    let (v, nrm) = (&parts.structure.vertices, &parts.structure.normals);
    assert!(!parts.structure.indices.is_empty(), "no hub curb emitted");
    let mut backwound = 0;
    for tri in parts.structure.indices.as_chunks::<3>().0 {
        let (ia, ib, ic) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let geo = cross(sub3(v[ib], v[ia]), sub3(v[ic], v[ia]));
        let avg = [
            nrm[ia][0] + nrm[ib][0] + nrm[ic][0],
            nrm[ia][1] + nrm[ib][1] + nrm[ic][1],
            nrm[ia][2] + nrm[ib][2] + nrm[ic][2],
        ];
        if dot(geo, avg) <= 0.0 {
            backwound += 1;
        }
    }
    assert_eq!(
        backwound, 0,
        "{backwound} hub curb triangles wound against their normal"
    );
}

/// #577 (review wf_55dafda9 MEDIUM): on an ASYMMETRIC hub (differing per-arm
/// half-widths and pull-backs) each gap's curb still starts/ends EXACTLY on
/// each ribbon's outer-curb point.
#[test]
fn hub_curb_endpoints_exact_on_asymmetric_hub() {
    let dims = Dims::from_config(&cfg(7));
    let (ct, cf) = (dims.curb_top_width, dims.chamfer_width);
    let deg = |d: f32| d.to_radians();
    let ends = [
        arm(deg(10.0), 9.0, 6.0, 1.0),
        arm(deg(130.0), 9.0, 3.0, 2.5),
        arm(deg(250.0), 8.0, 5.0, 1.5),
    ];
    let parts = assert_hub_sound(&ends, &dims, "asymmetric hub");
    for e in &ends {
        let wo = e.half_w + ct + cf;
        for sgn in [-1.0_f32, 1.0] {
            let o = outer_point(e, sgn, wo);
            assert!(
                near(&parts.structure.vertices, o),
                "asymmetric hub curb missed the ribbon outer-curb point {o:?}"
            );
        }
    }
}

/// #1558 - the curbs that jogged: where a wide street crosses a narrow one,
/// each corner's curb runs on along both streets' own outer lines to the
/// point where they meet, and the deck edge to the point where the two deck
/// edges meet - no diagonal chord stepping sideways because the two arms'
/// pull-backs differ. A major (half-width 3.5) crossing a minor (2.0) square,
/// the minor pulled back further than the major, as the boundary solve does.
#[test]
fn a_crossing_of_a_wide_and_a_narrow_street_keeps_its_curb_lines_straight() {
    let dims = Dims::from_config(&cfg(7));
    let outer = dims.curb_top_width + dims.chamfer_width;
    let (wm, wn) = (dims.major_half_width, dims.minor_half_width);
    let (fp2, pi) = (std::f32::consts::FRAC_PI_2, std::f32::consts::PI);
    let (tm, tn) = (wm, wm + outer + 0.1);
    let ends = [
        arm(0.0, tm, wm, 1.0),
        arm(fp2, tn, wn, 1.0),
        arm(pi, tm, wm, 1.0),
        arm(-fp2, tn, wn, 1.0),
    ];
    let parts = assert_hub_sound(&ends, &dims, "major x minor");
    // Every block corner: the deck corner (wn, wm) and the outer corner
    // (wn + outer, wm + outer), in each quadrant.
    for (sx, sz) in [(1.0_f32, 1.0_f32), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
        let q = [sx * wn, 1.0, sz * wm];
        assert!(
            near(&parts.deck.vertices, q),
            "deck corner {q:?} missing: the corner is cut by a chord"
        );
        let p = [sx * (wn + outer), 1.0, sz * (wm + outer)];
        assert!(
            near(&parts.structure.vertices, p),
            "outer curb corner {p:?} missing: the curb jogs instead of meeting"
        );
    }
    // And no deck reaches past the two streets' own edge lines.
    for v in &parts.deck.vertices {
        assert!(
            v[0].abs() <= wn + 1.0e-3 || v[2].abs() <= wm + 1.0e-3,
            "deck vertex {v:?} cuts into a block corner"
        );
    }
}

// --- #1558: the pathological junctions, meshed end to end -------------------

/// A graph from XZ node positions (window metres) and `(start, end, major)`
/// edges.
fn graph_of(nodes: &[(f32, f32)], edges: &[(u32, u32, bool)]) -> RoadGraph {
    typed_graph(nodes, edges)
}

/// Every vertex of the mesh lies within the outer footprint (deck, curb,
/// chamfer) of some street of `graph`: no curb or skirt past the curb line.
fn assert_within_footprints(parts: &RoadParts, graph: &RoadGraph, dims: &Dims, what: &str) {
    let outer = dims.curb_top_width + dims.chamfer_width;
    for v in parts.deck.vertices.iter().chain(&parts.structure.vertices) {
        let gap = graph
            .edges
            .iter()
            .filter(|e| e.active)
            .map(|e| {
                let (a, b) = (
                    graph.nodes[e.start as usize].position,
                    graph.nodes[e.end as usize].position,
                );
                let (ab, av) = ([b.x - a.x, b.y - a.y], [v[0] - a.x, v[2] - a.y]);
                let t = ((av[0] * ab[0] + av[1] * ab[1])
                    / (ab[0] * ab[0] + ab[1] * ab[1]).max(1.0e-9))
                .clamp(0.0, 1.0);
                let w = match e.road_type {
                    symbios_tensor::RoadType::Major => dims.major_half_width,
                    symbios_tensor::RoadType::Minor => dims.minor_half_width,
                } + outer;
                (av[0] - ab[0] * t).hypot(av[1] - ab[1] * t) - w
            })
            .fold(f32::INFINITY, f32::min);
        assert!(
            gap <= 0.05,
            "{what}: vertex {v:?} lies {gap} m past every curb line"
        );
    }
}

/// Mesh `graph` over a flat 256 m window (the junctions near its centre,
/// well inside the district interior) and check the whole deck is sound -
/// every triangle up, none folded or overlapping another, ribbons included
/// unless `ribbons_may_overlap` (a pair too sharp for any pull-back) - with
/// no curb over the asphalt and nothing past any street's curb line.
fn assert_network_sound(graph: &RoadGraph, what: &str, ribbons_may_overlap: bool) -> RoadParts {
    let dims = Dims::from_config(&cfg(7));
    let sub = HeightMap::new(128, 128, 2.0);
    let parts = mesh_road_graph(graph, &sub, [0, 0], &dims);
    let deck_y = parts
        .deck
        .vertices
        .iter()
        .map(|v| v[1])
        .fold(f32::MIN, f32::max);
    for v in &parts.deck.vertices {
        assert!(
            (v[1] - deck_y).abs() < 1.0e-3,
            "{what}: flat ground grew an unlevel deck {v:?}"
        );
    }
    if ribbons_may_overlap {
        let tris = triangles(&parts.deck);
        for t in &tris {
            assert!(
                up_area2(t) > 2.0e-4,
                "{what}: degenerate or downward deck {t:?}"
            );
        }
    } else {
        assert_deck_sound(&parts.deck, what);
        assert_no_curb_over_deck(&parts, deck_y, what);
    }
    assert_within_footprints(&parts, graph, &dims, what);
    parts
}

/// The deck covers `p` (XZ): some deck triangle contains it.
fn deck_covers(parts: &RoadParts, p: (f32, f32)) -> bool {
    triangles(&parts.deck).iter().any(|t| {
        let s =
            |a: [f32; 3], b: [f32; 3]| (b[0] - a[0]) * (p.1 - a[2]) - (b[2] - a[2]) * (p.0 - a[0]);
        let (d1, d2, d3) = (s(t[0], t[1]), s(t[1], t[2]), s(t[2], t[0]));
        (d1 >= -1.0e-4 && d2 >= -1.0e-4 && d3 >= -1.0e-4)
            || (d1 <= 1.0e-4 && d2 <= 1.0e-4 && d3 <= 1.0e-4)
    })
}

#[test]
fn an_acute_three_way_junction_meshes_soundly() {
    // A straight through road and a branch leaving it 25° off one half.
    let c = (128.0_f32, 128.0_f32);
    let a = 25f32.to_radians();
    let g = graph_of(
        &[
            c,
            (c.0 + 60.0, c.1),
            (c.0 - 60.0, c.1),
            (c.0 + 60.0 * a.cos(), c.1 + 60.0 * a.sin()),
        ],
        &[(0, 1, true), (0, 2, true), (0, 3, false)],
    );
    let parts = assert_network_sound(&g, "acute three-way", false);
    assert!(deck_covers(&parts, c), "the junction node is left bare");
}

#[test]
fn a_five_way_junction_meshes_soundly() {
    // Majors and minors, one pair 35 degrees apart.
    let c = (128.0_f32, 128.0_f32);
    let mut nodes = vec![c];
    let mut edges = Vec::new();
    for (k, (deg, major)) in [
        (0.0_f32, true),
        (35.0, false),
        (110.0, true),
        (200.0, false),
        (280.0, true),
    ]
    .into_iter()
    .enumerate()
    {
        let a = deg.to_radians();
        nodes.push((c.0 + 50.0 * a.cos(), c.1 + 50.0 * a.sin()));
        edges.push((0, k as u32 + 1, major));
    }
    let parts = assert_network_sound(&graph_of(&nodes, &edges), "five-way", false);
    assert!(deck_covers(&parts, c), "the junction node is left bare");
}

#[test]
fn two_junctions_a_metre_apart_mesh_as_one_hub() {
    // Two 3-way junctions 1 m apart along a major street, a minor street
    // leaving each to opposite sides - a tracer's offset crossing.
    let (a, b) = ((127.5_f32, 128.0_f32), (128.5_f32, 128.0_f32));
    let g = graph_of(
        &[
            a,
            b,
            (60.0, 128.0),
            (196.0, 128.0),
            (127.5, 60.0),
            (128.5, 196.0),
        ],
        &[
            (0, 1, true),
            (0, 2, true),
            (1, 3, true),
            (0, 4, false),
            (1, 5, false),
        ],
    );
    let parts = assert_network_sound(&g, "junctions 1 m apart", false);
    assert!(
        deck_covers(&parts, a) && deck_covers(&parts, b),
        "the cluster is left bare"
    );
    let sub = HeightMap::new(128, 128, 2.0);
    let dims = Dims::from_config(&cfg(7));
    let drawn = crate::urban::drawn_graph(&g, &sub);
    let chains = crate::urban::extract_chains(&drawn, &sub, &dims);
    let plan = plan_junctions(&chains, &crate::urban::active_degree(&drawn), &dims);
    assert_eq!(plan.hubs.len(), 1, "the two junctions are one hub");
    assert_eq!(
        plan.hubs[0].arms.len(),
        4,
        "with the four streets leaving it"
    );
}

#[test]
fn a_connector_shorter_than_its_pull_backs_is_drawn_inside_one_hub() {
    // Two crossings of a major street 4 m apart, their minor streets
    // splaying away from each other: the 4 m connector cannot carry both
    // ends' pull-backs (3.5 m each, plus a metre of ribbon).
    let (a, b) = ((126.0_f32, 128.0_f32), (130.0_f32, 128.0_f32));
    let g = graph_of(
        &[
            a,
            b,
            (60.0, 128.0),
            (196.0, 128.0),
            (86.0, 60.0),
            (86.0, 196.0),
            (170.0, 60.0),
            (170.0, 196.0),
        ],
        &[
            (0, 1, true),
            (0, 2, true),
            (1, 3, true),
            (0, 4, false),
            (0, 5, false),
            (1, 6, false),
            (1, 7, false),
        ],
    );
    let parts = assert_network_sound(&g, "short connector", false);
    assert!(
        deck_covers(&parts, ((a.0 + b.0) * 0.5, a.1)),
        "the connector is left bare"
    );
}

#[test]
fn a_near_parallel_pair_keeps_a_sound_hub() {
    // Two streets leaving a junction 8° apart - sharper than any pull-back
    // can part - plus the through street's other half. Past the cap the two
    // ribbons still overlap (the graph tidy's business); the hub between
    // their mouths must be sound regardless.
    let c = (128.0_f32, 128.0_f32);
    let a = 8f32.to_radians();
    let g = graph_of(
        &[
            c,
            (c.0 + 80.0, c.1),
            (c.0 + 80.0 * a.cos(), c.1 + 80.0 * a.sin()),
            (c.0 - 80.0, c.1),
        ],
        &[(0, 1, true), (0, 2, true), (0, 3, true)],
    );
    assert_network_sound(&g, "near-parallel pair", true);
    let dims = Dims::from_config(&cfg(7));
    let sub = HeightMap::new(128, 128, 2.0);
    let drawn = crate::urban::drawn_graph(&g, &sub);
    let chains = crate::urban::extract_chains(&drawn, &sub, &dims);
    let plan = plan_junctions(&chains, &crate::urban::active_degree(&drawn), &dims);
    let mut road_ends = Vec::new();
    let mut ribbons = RoadParts::default();
    let samples: Vec<_> = chains
        .iter()
        .enumerate()
        .map(|(ci, ch)| crate::urban::sample_chain(ch, plan.trims[ci][0], plan.trims[ci][1], &sub))
        .collect();
    let base = crate::urban::level_network(
        &chains,
        &samples,
        &plan,
        &crate::urban::hub_grounds(&chains, &samples, &plan, &dims),
        &sub,
    );
    for (ci, ch) in chains.iter().enumerate() {
        if let Some(s) = &samples[ci] {
            crate::urban::extrude_ribbon(
                ch,
                s,
                &base[ci],
                [0.0; 2],
                &dims,
                plan.chain_ends(ci),
                &mut road_ends,
                &mut ribbons,
            );
        }
    }
    assert_eq!(road_ends.len(), 3, "all three arms open into the hub");
    assert_sound_hub(&road_ends, &plan.hubs, &dims, "near-parallel hub");
}

/// #1558 - the street broken by a gap: a junction whose third street runs
/// out of the drawn district is no junction to the mesher. Its two drawn
/// streets run on through the node as one, so the street is not pulled back
/// short of it on both sides with nothing to close the gap.
#[test]
fn a_junction_that_loses_an_arm_to_the_district_clip_is_no_gap() {
    // Node 0 near the interior's rim; its third street runs out past it.
    let n = (128.0_f32, 30.0_f32);
    let g = graph_of(
        &[n, (90.0, 80.0), (166.0, 80.0), (128.0, 0.5)],
        &[(0, 1, false), (0, 2, false), (0, 3, true)],
    );
    let parts = assert_network_sound(&g, "clipped junction", false);
    assert!(
        deck_covers(&parts, n),
        "the street is broken by a gap at the node"
    );
}

/// The pilot network's hubs, isolated, are sound: no folded, inverted or
/// degenerate deck triangle, and nothing past the curb line. (The pilot holds
/// near-parallel streets a few metres apart, whose arms' curbs overlap each
/// other's decks along their whole length - a graph artefact the opt-in
/// tidy removes - so the curb-over-asphalt check is the synthetic tests'.)
#[test]
fn pilot_hubs_are_sound() {
    let hm = pilot_heightmap();
    let config = cfg(PILOT_ROAD_SEED);
    let (graph, sub, _lo) =
        crate::urban::build_road_graph(&hm, &config, None).expect("pilot must trace");
    let dims = Dims::from_config(&config);
    let drawn = crate::urban::drawn_graph(&graph, &sub);
    let chains = crate::urban::extract_chains(&drawn, &sub, &dims);
    let plan = plan_junctions(&chains, &crate::urban::active_degree(&drawn), &dims);
    let samples: Vec<_> = chains
        .iter()
        .enumerate()
        .map(|(ci, ch)| {
            (!plan.internal[ci])
                .then(|| crate::urban::sample_chain(ch, plan.trims[ci][0], plan.trims[ci][1], &sub))
                .flatten()
        })
        .collect();
    let base = crate::urban::level_network(
        &chains,
        &samples,
        &plan,
        &crate::urban::hub_grounds(&chains, &samples, &plan, &dims),
        &sub,
    );
    let mut road_ends = Vec::new();
    let mut ribbons = RoadParts::default();
    for (ci, ch) in chains.iter().enumerate() {
        if let Some(s) = &samples[ci] {
            crate::urban::extrude_ribbon(
                ch,
                s,
                &base[ci],
                [0.0; 2],
                &dims,
                plan.chain_ends(ci),
                &mut road_ends,
                &mut ribbons,
            );
        }
    }
    let mut by_hub: std::collections::BTreeMap<usize, Vec<RoadEnd>> = Default::default();
    for e in road_ends {
        by_hub.entry(e.hub).or_default().push(e);
    }
    assert!(by_hub.len() > 10, "the pilot exercises many hubs");
    for (h, ends) in &by_hub {
        if ends.len() >= 2 {
            assert_hub_deck_sound(ends, &plan.hubs, &dims, &format!("pilot hub {h}"));
        }
    }
}

/// #584/#1558: a hub's flat deck clears the ground under every corner of
/// its outline, not just under its node and its mouths - the levelling
/// reads the outline before any height exists. A single raised cell under
/// one block corner of a crossing (where the two curb lines meet, which
/// neither mouth nor node samples) must lift the whole hub over it.
#[test]
fn a_hub_clears_the_ground_under_its_corners() {
    let dims = Dims::from_config(&cfg(7));
    let mut sub = HeightMap::new(256, 256, 0.5);
    let c = (64.0_f32, 64.0_f32);
    let g = graph_of(
        &[
            c,
            (c.0 + 40.0, c.1),
            (c.0 - 40.0, c.1),
            (c.0, c.1 + 40.0),
            (c.0, c.1 - 40.0),
        ],
        &[(0, 1, true), (0, 2, true), (0, 3, false), (0, 4, false)],
    );
    // The corner of the +x/+z block: minor half-width across, major along.
    let corner = (c.0 + dims.minor_half_width, c.1 + dims.major_half_width);
    sub.set((corner.0 / 0.5) as usize, (corner.1 / 0.5) as usize, 2.0);
    let parts = mesh_road_graph(&g, &sub, [0, 0], &dims);
    assert!(
        parts
            .deck
            .vertices
            .iter()
            .any(|v| (v[0] - corner.0).abs() < 1.0e-3 && (v[2] - corner.1).abs() < 1.0e-3),
        "the corner is an outline vertex"
    );
    for v in &parts.deck.vertices {
        let ground = sub.get_height_at(v[0], v[2]);
        assert!(
            v[1] + 1.0e-3 >= ground,
            "deck vertex {v:?} buried below the ground {ground}"
        );
    }
}
