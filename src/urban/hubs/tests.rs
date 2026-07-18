use super::*;
use crate::urban::test_support::*;
use crate::urban::{Chain, Dims, RoadParts, build_road_graph, compute_truncations, extract_chains};
use bevy_symbios_ground::HeightMap;

/// WS4: a junction grows a real hub — a deck polygon meeting each incident
/// road at its mouth (one centre + 2 corners per arm) plus curb/skirt walls
/// closing the gaps — not the old circular fan.
#[test]
fn hub_meets_each_road_and_closes_gaps() {
    let dims = Dims::from_config(&cfg(7));
    // Three roads meeting at the origin at 0° / 120° / 240° — a clean Y.
    // Each arm's mouth is truncated 5 m out from the node along its heading.
    let t = 5.0_f32;
    let arm = |ang: f32| {
        let (dx, dz) = (ang.cos(), ang.sin());
        RoadEnd {
            node: 0,
            cx: dx * t,
            cz: dz * t,
            rx: -dz,
            rz: dx,
            half_w: 4.0,
            deck_y: 1.0,
            skirt_y: -4.0,
        }
    };
    let third = std::f32::consts::TAU / 3.0;
    let ends = [arm(0.0), arm(third), arm(2.0 * third)];
    let hm = HeightMap::new(64, 64, 2.0); // flat; hub welds skirt feet to each arm's skirt_y
    let mut parts = RoadParts::default();
    extrude_hubs(&ends, &hm, [0.0; 2], &dims, &mut parts);

    // Deck: 1 centre + 3 arms × 2 corners = 7 verts, 6 fan triangles.
    assert_eq!(parts.deck.vertices.len(), 1 + 3 * 2);
    assert_eq!(parts.deck.indices.len(), 6 * 3);
    // The gaps grow curb/skirt walls (not an open or flat fan).
    assert!(!parts.structure.is_empty(), "hub gaps left unclosed");
    // Every deck vertex sits at the incident deck height or above (the level
    // fit is upward-only; nothing dips below the roads it joins).
    for v in &parts.deck.vertices {
        assert!(
            v[1] + 1.0e-4 >= 1.0,
            "hub deck vertex {v:?} below the roads"
        );
    }
    // Every deck normal points up (the fan is wound front-up, not folded).
    for nrm in &parts.deck.normals {
        assert!(nrm[1] > 0.0, "hub deck normal {nrm:?} not upward");
    }
}

/// #584 (was #576 mean-fit): the hub apex fits the MAX incident mouth, kept
/// upward-only — never the mean (which would droop below the highest road). The
/// network levelling pins every incident mouth UP to that max, so once the
/// mouths are level the apex and every corner coincide → a genuinely FLAT
/// junction plane (no tent, no droop).
#[test]
fn hub_is_flat_at_the_max_incident_mouth() {
    let dims = Dims::from_config(&cfg(7));
    let t = 5.0_f32;
    let arm = |ang: f32, deck_y: f32| {
        let (dx, dz) = (ang.cos(), ang.sin());
        RoadEnd {
            node: 0,
            cx: dx * t,
            cz: dz * t,
            rx: -dz,
            rz: dx,
            half_w: 4.0,
            deck_y,
            skirt_y: deck_y - 5.0,
        }
    };
    let third = std::f32::consts::TAU / 3.0;
    let hm = HeightMap::new(64, 64, 2.0); // flat terrain at 0 → no upward clamp

    // Differing mouths (the pre-relaxation shape): the apex fits the MAX (3.0),
    // not the mean (2.0) — so the hub never droops below the highest road —
    // while each corner still meets its own mouth seamlessly.
    let mixed = [arm(0.0, 1.0), arm(third, 2.0), arm(2.0 * third, 3.0)];
    let mut parts = RoadParts::default();
    extrude_hubs(&mixed, &hm, [0.0; 2], &dims, &mut parts);
    let apex_y = parts.deck.vertices[0][1];
    assert!((apex_y - 3.0).abs() < 1.0e-3, "apex {apex_y} ≠ max 3.0");
    assert!(
        parts
            .deck
            .vertices
            .iter()
            .any(|v| (v[1] - 1.0).abs() < 1.0e-3),
        "lowest mouth not met seamlessly"
    );

    // Level mouths (the post-relaxation state): the whole deck — apex and every
    // corner — sits at the one height → a flat plane.
    let level = [arm(0.0, 3.0), arm(third, 3.0), arm(2.0 * third, 3.0)];
    let mut flat = RoadParts::default();
    extrude_hubs(&level, &hm, [0.0; 2], &dims, &mut flat);
    assert!(!flat.deck.vertices.is_empty(), "hub produced no deck");
    for v in &flat.deck.vertices {
        assert!(
            (v[1] - 3.0).abs() < 1.0e-3,
            "hub deck vertex {v:?} not flat at 3.0"
        );
    }
}

/// #576 regression (review wf_39a9f056-ef1): when arms truncate to different
/// distances and the deck half-width rivals the pull-back, adjacent mouths
/// splay past each other — a node-anchored fan over arm-grouped corners then
/// self-intersects (overlapping deck triangles that z-fight at their differing
/// heights). The centroid angular-sweep keeps the deck a SIMPLE polygon: its
/// corners come out monotonically ordered by angle around the apex.
#[test]
fn hub_deck_stays_simple_with_asymmetric_mouths() {
    let dims = Dims::from_config(&cfg(7));
    let hm = HeightMap::new(64, 64, 2.0);
    // Arms at 0 / 120 / 240°, deliberately asymmetric pull-backs (1.5 / 8 / 4 m)
    // with a wide deck (half_w 4) so arm 0's short mouth splays ±~69°.
    let arm = |ang: f32, t: f32, deck_y: f32| {
        let (dx, dz) = (ang.cos(), ang.sin());
        RoadEnd {
            node: 0,
            cx: dx * t,
            cz: dz * t,
            rx: -dz,
            rz: dx,
            half_w: 4.0,
            deck_y,
            skirt_y: deck_y - 5.0,
        }
    };
    let third = std::f32::consts::TAU / 3.0;
    let ends = [
        arm(0.0, 1.5, 1.0),
        arm(third, 8.0, 2.0),
        arm(2.0 * third, 4.0, 1.5),
    ];
    let mut parts = RoadParts::default();
    extrude_hubs(&ends, &hm, [0.0; 2], &dims, &mut parts);

    // Apex = vertex 0; the mouth corners follow in angular-sweep order, so
    // their angle around the apex is monotonic (⇒ a simple polygon).
    let apex = parts.deck.vertices[0];
    let angles: Vec<f32> = parts.deck.vertices[1..]
        .iter()
        .map(|v| (v[2] - apex[2]).atan2(v[0] - apex[0]))
        .collect();
    for w in angles.windows(2) {
        assert!(
            w[1] >= w[0] - 1.0e-4,
            "deck corners not angle-sorted (self-intersecting fan): {angles:?}"
        );
    }
    // Every triangle still faces up and is finite (not folded/degenerate).
    for nrm in &parts.deck.normals {
        assert!(
            nrm[1] > 0.0 && nrm.iter().all(|c| c.is_finite()),
            "bad hub deck normal {nrm:?}"
        );
    }
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
    let deck = parts.deck.vertices.clone();
    for sgn in [-1.0_f32, 1.0] {
        let corner = [
            e.cx + sgn * e.rx * e.half_w,
            e.deck_y,
            e.cz + sgn * e.rz * e.half_w,
        ];
        let hit = deck.iter().any(|v| {
            (v[0] - corner[0]).abs() < 1.0e-3
                && (v[1] - corner[1]).abs() < 1.0e-3
                && (v[2] - corner[2]).abs() < 1.0e-3
        });
        assert!(
            hit,
            "hub mouth corner {corner:?} not on the ribbon end (seam)"
        );
    }
}

/// #577: `fillet_arc` samples a circular bulge — the apex sits one sagitta
/// out from the chord midpoint along `bd`, and a zero sagitta is the straight
/// chord (so a near-collinear gap stays flat).
#[test]
fn fillet_arc_is_a_circular_bulge() {
    let a = [-2.0_f32, 0.0];
    let b = [2.0_f32, 0.0];
    let bd = [0.0_f32, 1.0]; // bulge toward +y
    let sag = 0.5_f32;
    let arc = fillet_arc(a, b, bd, sag, 6);
    assert_eq!(arc.len(), 7, "segs+1 samples");
    assert!((arc[0][0] - a[0]).abs() < 1.0e-4 && (arc[0][1] - a[1]).abs() < 1.0e-4);
    assert!((arc[6][0] - b[0]).abs() < 1.0e-4 && (arc[6][1] - b[1]).abs() < 1.0e-4);
    // The midpoint sample bulges out by ~sag along +y.
    assert!(
        (arc[3][1] - sag).abs() < 1.0e-3,
        "apex {} ≠ sagitta {sag}",
        arc[3][1]
    );
    // Every sample is equidistant from the reconstructed circle centre.
    let r = (sag * sag + 4.0) / (2.0 * sag);
    let c = [0.0_f32, sag - r];
    for p in &arc {
        let d = (p[0] - c[0]).hypot(p[1] - c[1]);
        assert!((d - r).abs() < 1.0e-2, "off-circle sample d={d} r={r}");
    }
    // Zero sagitta → the straight chord.
    let flat = fillet_arc(a, b, bd, 0.0, 6);
    assert!(flat.iter().all(|p| p[1].abs() < 1.0e-4), "flat arc bulged");

    // The straight-gap detector flags ONLY collinear arms (so real corners keep
    // their fillet — a too-low threshold that flattened them would fail here).
    let unit = |deg: f32| {
        let r = deg.to_radians();
        [r.cos(), r.sin()]
    };
    assert!(
        fillet_gap_is_straight(unit(0.0), unit(180.0)),
        "through road"
    ); // anti-parallel
    assert!(fillet_gap_is_straight(unit(0.0), unit(8.0)), "acute fork"); // near-parallel
    assert!(
        !fillet_gap_is_straight(unit(0.0), unit(90.0)),
        "right-angle corner"
    );
    assert!(
        !fillet_gap_is_straight(unit(0.0), unit(120.0)),
        "wide-Y corner"
    );
    assert!(!fillet_gap_is_straight(unit(0.0), unit(45.0)), "45° corner");

    // Endpoints stay EXACT even when `bd` is NOT perpendicular to the chord
    // (the asymmetric-hub case): the centre is placed on the chord's own
    // perpendicular bisector and the endpoints are pinned. A sloped chord with
    // an off-axis bulge direction would drift the endpoints under the old
    // `centre = mid − bd·(r−sag)` formula.
    // `bd2` is deliberately FAR from perpendicular to the chord (5,−2) — under
    // the old `centre = mid − bd·(r−sag)` this drifted the endpoints; the chord-
    // bisector centre + pinned endpoints must keep them exact.
    let (a2, b2, bd2) = ([-3.0_f32, 1.0], [2.0_f32, -1.0], norm2([1.0, 0.0]));
    let arc2 = fillet_arc(a2, b2, bd2, 0.7, 6);
    assert!(
        (arc2[0][0] - a2[0]).abs() < 1.0e-4 && (arc2[0][1] - a2[1]).abs() < 1.0e-4,
        "off-axis bd drifted the start endpoint: {:?}",
        arc2[0]
    );
    assert!(
        (arc2[6][0] - b2[0]).abs() < 1.0e-4 && (arc2[6][1] - b2[1]).abs() < 1.0e-4,
        "off-axis bd drifted the end endpoint: {:?}",
        arc2[6]
    );
    // Interior samples still share one circle (a real arc, not a kink).
    let cc = {
        // centre = mid − pn·(r−sag) with pn the chord-perp toward bd2.
        let chord = [b2[0] - a2[0], b2[1] - a2[1]];
        let cl = chord[0].hypot(chord[1]);
        let half2 = cl * 0.5;
        let r2 = (0.7 * 0.7 + half2 * half2) / (2.0 * 0.7);
        let mut perp = [-chord[1] / cl, chord[0] / cl];
        if perp[0] * bd2[0] + perp[1] * bd2[1] < 0.0 {
            perp = [-perp[0], -perp[1]];
        }
        let mid = [(a2[0] + b2[0]) * 0.5, (a2[1] + b2[1]) * 0.5];
        (
            [mid[0] - perp[0] * (r2 - 0.7), mid[1] - perp[1] * (r2 - 0.7)],
            r2,
        )
    };
    for p in &arc2 {
        let d = (p[0] - cc.0[0]).hypot(p[1] - cc.0[1]);
        assert!(
            (d - cc.1).abs() < 1.0e-2,
            "off-axis arc off-circle d={d} r={}",
            cc.1
        );
    }
}

/// #577: the hub curb is continuous with the incident ribbons — each fillet
/// arc starts/ends exactly on the road's outer-curb point, so there's no
/// notch where the hub curb meets the ribbon curb.
#[test]
fn hub_fillet_joins_the_ribbon_outer_curbs() {
    let dims = Dims::from_config(&cfg(7));
    let hm = HeightMap::new(96, 96, 2.0);
    let half = dims.minor_half_width;
    let wo = half + dims.curb_top_width + dims.chamfer_width;
    // A Y of three chains meeting at node 0 (degree 3).
    let third = std::f32::consts::TAU / 3.0;
    let chains: Vec<Chain> = (0..3)
        .map(|k| {
            let ang = k as f32 * third;
            let (dx, dz) = (ang.cos(), ang.sin());
            Chain {
                pts: vec![
                    (50.0, 50.0),
                    (50.0 + dx * 15.0, 50.0 + dz * 15.0),
                    (50.0 + dx * 40.0, 50.0 + dz * 40.0),
                ],
                half_w: half,
                end_nodes: [0, 1 + k],
                clip: [false, false],
            }
        })
        .collect();
    let mut degree = vec![1u32; 4];
    degree[0] = 3;
    let trims = compute_truncations(&chains, |nd| degree[nd] >= 3, &dims);
    let mut road_ends = Vec::new();
    let mut ribbon = RoadParts::default();
    for (ci, c) in chains.iter().enumerate() {
        let [s, e] = trims[ci];
        extrude_chain(
            c,
            s,
            e,
            &hm,
            [0.0; 2],
            &dims,
            &degree,
            &mut road_ends,
            &mut ribbon,
        );
    }
    assert_eq!(road_ends.len(), 3, "the Y must record three mouths");

    let mut hub = RoadParts::default();
    extrude_hubs(&road_ends, &hm, [0.0; 2], &dims, &mut hub);

    let near = |verts: &[[f32; 3]], p: [f32; 3]| {
        verts.iter().any(|v| {
            (v[0] - p[0]).abs() < 1.0e-3
                && (v[1] - p[1]).abs() < 1.0e-3
                && (v[2] - p[2]).abs() < 1.0e-3
        })
    };
    // The ribbon's skirt bottom is now a FIXED `skirt_depth` below the deck (no
    // terrain reach); the fillet must drop to the SAME depth so the two skirts
    // weld (no open band at the seam).
    let skirt_y = |deck_y: f32| deck_y - dims.skirt_depth;
    for e in &road_ends {
        for sgn in [-1.0_f32, 1.0] {
            // Outer-curb point (chamfer base, deck level) — the arc endpoint.
            let o = [e.cx + sgn * e.rx * wo, e.deck_y, e.cz + sgn * e.rz * wo];
            assert!(
                near(&ribbon.structure.vertices, o),
                "ribbon curb missing its outer point {o:?}"
            );
            assert!(
                near(&hub.structure.vertices, o),
                "fillet does not meet the ribbon outer curb at {o:?}"
            );
            // Skirt bottom — the fillet skirt foot must meet the ribbon's deep
            // skirt bottom (the seam-continuity HIGH the review caught).
            let foot = [o[0], skirt_y(e.deck_y), o[2]];
            assert!(
                near(&ribbon.structure.vertices, foot),
                "ribbon skirt missing its bottom point {foot:?}"
            );
            assert!(
                near(&hub.structure.vertices, foot),
                "fillet skirt foot leaves an open band — does not reach the ribbon skirt bottom {foot:?}"
            );
        }
    }
}

/// #577 (verify wf_7f36d6ce LOW): the skirt welds even with a SHALLOW skirt on
/// a CROSS-SLOPE. The fillet carries each ribbon's recorded `skirt_y` (a fixed
/// `skirt_depth` below the deck), so the foot lands exactly on the ribbon skirt
/// bottom regardless of depth or slope. Reads the ribbon's own skirt-bottom
/// vertex (lowest at each outer-curb XZ) and asserts the hub meets it.
#[test]
fn hub_fillet_skirt_welds_on_shallow_cross_slope() {
    let config = crate::pds::generator::RoadConfig {
        skirt_depth: crate::pds::types::Fp(0.5),
        ..cfg(7)
    };
    let dims = Dims::from_config(&config);
    let w = dims.minor_half_width;
    let wo = w + dims.curb_top_width + dims.chamfer_width;
    // A ramp in x → each mouth's two outer edges sit at different heights.
    let mut hm = HeightMap::new(96, 96, 2.0);
    let width = hm.width();
    for z in 0..width {
        for x in 0..width {
            hm.set(x, z, x as f32 * 0.3);
        }
    }
    let third = std::f32::consts::TAU / 3.0;
    let chains: Vec<Chain> = (0..3)
        .map(|k| {
            let ang = k as f32 * third;
            let (dx, dz) = (ang.cos(), ang.sin());
            Chain {
                pts: vec![
                    (90.0, 90.0),
                    (90.0 + dx * 15.0, 90.0 + dz * 15.0),
                    (90.0 + dx * 40.0, 90.0 + dz * 40.0),
                ],
                half_w: w,
                end_nodes: [0, 1 + k],
                clip: [false, false],
            }
        })
        .collect();
    let mut degree = vec![1u32; 4];
    degree[0] = 3;
    let trims = compute_truncations(&chains, |nd| degree[nd] >= 3, &dims);
    let mut road_ends = Vec::new();
    let mut ribbon = RoadParts::default();
    for (ci, c) in chains.iter().enumerate() {
        let [s, e] = trims[ci];
        extrude_chain(
            c,
            s,
            e,
            &hm,
            [0.0; 2],
            &dims,
            &degree,
            &mut road_ends,
            &mut ribbon,
        );
    }
    assert_eq!(road_ends.len(), 3, "the Y must record three mouths");
    let mut hub = RoadParts::default();
    extrude_hubs(&road_ends, &hm, [0.0; 2], &dims, &mut hub);

    // Lowest ribbon vertex at an XZ = that point's skirt bottom.
    let ribbon_floor = |xz: [f32; 2]| {
        ribbon
            .structure
            .vertices
            .iter()
            .filter(|v| (v[0] - xz[0]).abs() < 1.0e-3 && (v[2] - xz[1]).abs() < 1.0e-3)
            .map(|v| v[1])
            .fold(f32::INFINITY, f32::min)
    };
    let hub_has = |p: [f32; 3]| {
        hub.structure.vertices.iter().any(|v| {
            (v[0] - p[0]).abs() < 1.0e-3
                && (v[1] - p[1]).abs() < 1.0e-3
                && (v[2] - p[2]).abs() < 1.0e-3
        })
    };
    for e in &road_ends {
        for sgn in [-1.0_f32, 1.0] {
            let xz = [e.cx + sgn * e.rx * wo, e.cz + sgn * e.rz * wo];
            let floor = ribbon_floor(xz);
            assert!(floor.is_finite(), "no ribbon skirt at outer point {xz:?}");
            assert!(
                hub_has([xz[0], floor, xz[1]]),
                "fillet skirt foot {:?} did not weld to the ribbon skirt bottom {floor}",
                [xz[0], floor, xz[1]]
            );
        }
    }
}

/// #577: the curb-return fillets are wound front-out — every structure
/// triangle's geometric normal points away from the hub centre, so back-face
/// culling keeps the curb/skirt visible from outside (no inside-out corner).
/// A symmetric Y keeps the centroid at the origin so the radial faces-out
/// proxy is exact (a skewed hub makes near-tangent faces ambiguous).
#[test]
fn hub_fillet_faces_out() {
    let dims = Dims::from_config(&cfg(7));
    let hm = HeightMap::new(64, 64, 2.0); // flat at 0 → no upward tent
    let third = std::f32::consts::TAU / 3.0;
    let t = 5.0_f32;
    let arm = |ang: f32| {
        let (dx, dz) = (ang.cos(), ang.sin());
        RoadEnd {
            node: 0,
            cx: dx * t,
            cz: dz * t,
            rx: -dz,
            rz: dx,
            half_w: 4.0,
            deck_y: 1.0,
            skirt_y: -4.0,
        }
    };
    let ends = [arm(0.0), arm(third), arm(2.0 * third)];
    let mut parts = RoadParts::default();
    extrude_hubs(&ends, &hm, [0.0; 2], &dims, &mut parts);

    // The symmetric Y's mouth-corner centroid is the origin; the apex sits at
    // the mean deck height (clamped upward to flat terrain → stays 1.0).
    let center = [
        0.0_f32,
        1.0_f32.max(hm.get_height_at(0.0, 0.0) + ROAD_DEPTH_BIAS_M),
        0.0,
    ];
    let v = &parts.structure.vertices;
    let idx = &parts.structure.indices;
    assert!(!idx.is_empty(), "no fillet structure emitted");
    for tri in idx.chunks_exact(3) {
        let (a, b, c) = (v[tri[0] as usize], v[tri[1] as usize], v[tri[2] as usize]);
        let geo = cross(sub3(b, a), sub3(c, a));
        let mid = [
            (a[0] + b[0] + c[0]) / 3.0,
            (a[1] + b[1] + c[1]) / 3.0,
            (a[2] + b[2] + c[2]) / 3.0,
        ];
        assert!(
            dot(geo, sub3(mid, center)) > 1.0e-5,
            "fillet triangle faces inward: n·out = {}",
            dot(normalize(geo), normalize(sub3(mid, center)))
        );
    }
}

/// The hub fillet skirt welds to each arm's recorded `skirt_y` (the ribbon's
/// fixed depth below the deck) and IGNORES the terrain: even where the gap
/// terrain humps ABOVE the deck, the foot stays at the arm depth — it floats
/// clear as a bridge instead of rising to meet the ground — and no structure
/// pokes above the curb top (no inversion).
#[test]
fn hub_fillet_skirt_holds_the_arm_depth_over_humped_terrain() {
    let dims = Dims::from_config(&cfg(7));
    // Terrain humped to 2 m, above the deck (1 m): a terrain-reaching foot would
    // ride up onto it (~1.7). The fixed-depth skirt must ignore the hump.
    let mut hm = HeightMap::new(64, 64, 2.0);
    for c in hm.data_mut() {
        *c = 2.0;
    }
    let third = std::f32::consts::TAU / 3.0;
    let arm = |ang: f32| {
        let (dx, dz) = (ang.cos(), ang.sin());
        RoadEnd {
            node: 0,
            cx: dx * 5.0,
            cz: dz * 5.0,
            rx: -dz,
            rz: dx,
            half_w: 4.0,
            deck_y: 1.0,
            skirt_y: -4.0,
        }
    };
    let ends = [arm(0.0), arm(third), arm(2.0 * third)];
    let mut parts = RoadParts::default();
    extrude_hubs(&ends, &hm, [0.0; 2], &dims, &mut parts);

    let curb_top = 1.0 + dims.curb_height; // highest structure point
    let mut min_y = f32::INFINITY;
    for v in &parts.structure.vertices {
        assert!(
            v[1] <= curb_top + 1.0e-2,
            "structure vertex {v:?} pokes above the curb top (skirt inverted?)"
        );
        min_y = min_y.min(v[1]);
    }
    // The skirt foot sits at the arms' recorded skirt_y — the fixed depth below
    // the deck — NOT lifted to the 2 m terrain hump above it.
    let want = ends[0].skirt_y;
    assert!(
        (min_y - want).abs() < 0.1,
        "skirt foot {min_y} did not weld to the arm skirt depth {want}",
    );
}

/// #577: a through road's far edge (two anti-parallel arms with no branch
/// between) must stay a STRAIGHT curb, not bulge — the straight-gap detector
/// drops the fillet sagitta to 0 there. Builds a T (arms at 0°/90°/180°) and
/// checks the −z straight side runs flat at the outer-curb line (z = −wo).
#[test]
fn hub_through_road_far_edge_stays_straight() {
    let dims = Dims::from_config(&cfg(7));
    let w = 4.0_f32;
    let wo = w + dims.curb_top_width + dims.chamfer_width;
    let hm = HeightMap::new(64, 64, 2.0); // flat at 0
    let arm = |ang: f32| {
        let (dx, dz) = (ang.cos(), ang.sin());
        RoadEnd {
            node: 0,
            cx: dx * 6.0,
            cz: dz * 6.0,
            rx: -dz,
            rz: dx,
            half_w: w,
            deck_y: 1.0,
            skirt_y: -4.0,
        }
    };
    let (fp2, pi) = (std::f32::consts::FRAC_PI_2, std::f32::consts::PI);
    let ends = [arm(0.0), arm(fp2), arm(pi)];
    let mut parts = RoadParts::default();
    extrude_hubs(&ends, &hm, [0.0; 2], &dims, &mut parts);

    // The straight −z side's outer edge sits at z = −wo; a bulged fillet would
    // push structure past it (more negative z).
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
    // ...and its midpoint is present at the deck level on that straight line.
    let mid_present =
        parts.structure.vertices.iter().any(|v| {
            v[0].abs() < 1.0e-2 && (v[1] - 1.0).abs() < 1.0e-2 && (v[2] + wo).abs() < 1.0e-2
        });
    assert!(
        mid_present,
        "straight through-road far-edge midpoint missing"
    );
}

/// #577 (review wf_55dafda9 HIGH): on a SLOPED / asymmetric hub the fillet
/// strip is non-planar (its inner edge rides a deck chord that runs between two
/// different mouth heights), so a single per-strip winding decision back-winds
/// some triangles. Every emitted structure triangle's geometric winding must
/// agree with its (outward) stored shading normal — the per-segment winding
/// guarantees it. A symmetric flat Y never twists, so this needs varying deck_y.
#[test]
fn hub_fillet_winding_consistent_on_sloped_hub() {
    let dims = Dims::from_config(&cfg(7));
    let hm = HeightMap::new(64, 64, 2.0); // flat terrain; the SLOPE is in deck_y
    let third = std::f32::consts::TAU / 3.0;
    // Adjacent mouths at clearly different deck heights (1 / 2 / 4) + asymmetric
    // pull-backs → the fillet strips slope and curve (the twisting regime).
    let arm = |ang: f32, t: f32, deck_y: f32| {
        let (dx, dz) = (ang.cos(), ang.sin());
        RoadEnd {
            node: 0,
            cx: dx * t,
            cz: dz * t,
            rx: -dz,
            rz: dx,
            half_w: 4.0,
            deck_y,
            skirt_y: deck_y - 5.0,
        }
    };
    let ends = [
        arm(0.0, 5.0, 1.0),
        arm(third, 8.0, 2.0),
        arm(2.0 * third, 4.0, 4.0),
    ];
    let mut parts = RoadParts::default();
    extrude_hubs(&ends, &hm, [0.0; 2], &dims, &mut parts);

    let v = &parts.structure.vertices;
    let nrm = &parts.structure.normals;
    let idx = &parts.structure.indices;
    assert!(!idx.is_empty(), "no fillet structure emitted");
    let mut backwound = 0;
    for tri in idx.chunks_exact(3) {
        let (ia, ib, ic) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let geo = cross(sub3(v[ib], v[ia]), sub3(v[ic], v[ia]));
        // Average the three stored (outward) shading normals.
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
        "{backwound} fillet triangles are wound against their outward normal (mis-shaded)"
    );
}

/// #577 (review wf_55dafda9): the per-segment winding must hold on the REAL
/// pilot network — every hub there is skewed (asymmetric truncation, mouths at
/// different draped heights, ~23° acute branches), the regime a single
/// per-strip winding decision got wrong (~9% of structure tris). Isolates the
/// hub fillets (extrude_hubs into its own buffer) and asserts no fillet triangle
/// is wound against its outward shading normal.
#[test]
fn pilot_hub_fillets_are_wound_consistently() {
    let hm = pilot_heightmap();
    let config = cfg(PILOT_ROAD_SEED);
    let (graph, sub, lo) = build_road_graph(&hm, &config).expect("pilot must trace");
    let dims = Dims::from_config(&config);
    let chains = extract_chains(&graph, &sub, &dims);
    let mut degree = vec![0u32; graph.nodes.len()];
    for e in &graph.edges {
        if e.active {
            degree[e.start as usize] += 1;
            degree[e.end as usize] += 1;
        }
    }
    let trims = compute_truncations(
        &chains,
        |nd| degree.get(nd).copied().unwrap_or(0) >= 3,
        &dims,
    );
    let world_offset = [lo[0] as f32 * sub.scale(), lo[1] as f32 * sub.scale()];
    let mut road_ends = Vec::new();
    let mut ribbon = RoadParts::default();
    for (ci, c) in chains.iter().enumerate() {
        let [s, e] = trims[ci];
        extrude_chain(
            c,
            s,
            e,
            &sub,
            world_offset,
            &dims,
            &degree,
            &mut road_ends,
            &mut ribbon,
        );
    }
    let mut hub = RoadParts::default();
    extrude_hubs(&road_ends, &sub, world_offset, &dims, &mut hub);

    let v = &hub.structure.vertices;
    let nrm = &hub.structure.normals;
    assert!(
        !hub.structure.indices.is_empty(),
        "pilot grew no hub fillets"
    );
    let mut backwound = 0;
    for tri in hub.structure.indices.chunks_exact(3) {
        let (ia, ib, ic) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let geo = normalize(cross(sub3(v[ib], v[ia]), sub3(v[ic], v[ia])));
        let avg = normalize([
            nrm[ia][0] + nrm[ib][0] + nrm[ic][0],
            nrm[ia][1] + nrm[ib][1] + nrm[ic][1],
            nrm[ia][2] + nrm[ib][2] + nrm[ic][2],
        ]);
        // Tolerance skips genuinely degenerate (zero-area) slivers; a real
        // back-wound triangle reads clearly negative.
        if dot(geo, avg) < -0.01 {
            backwound += 1;
        }
    }
    assert_eq!(
        backwound, 0,
        "{backwound} pilot hub-fillet triangles wound against their normal"
    );
}

/// #577 (review wf_55dafda9 MEDIUM): on an ASYMMETRIC hub (differing per-arm
/// half-widths and pull-backs) the fillet arc must still start/end EXACTLY on
/// each ribbon's outer-curb point — the old `centre = mid − bd·(r−sag)` drifted
/// the endpoints (sub-metre notch) whenever `bd` was not perpendicular to the
/// outer-curb chord, which is the norm off a symmetric Y.
#[test]
fn hub_fillet_endpoints_exact_on_asymmetric_hub() {
    let dims = Dims::from_config(&cfg(7));
    let (ct, cf) = (dims.curb_top_width, dims.chamfer_width);
    let hm = HeightMap::new(64, 64, 2.0);
    // Deliberately skewed: different angles, half-widths, pull-backs, heights.
    let arm = |ang_deg: f32, t: f32, hw: f32, deck_y: f32| {
        let a = ang_deg.to_radians();
        let (dx, dz) = (a.cos(), a.sin());
        RoadEnd {
            node: 0,
            cx: dx * t,
            cz: dz * t,
            rx: -dz,
            rz: dx,
            half_w: hw,
            deck_y,
            skirt_y: deck_y - 5.0,
        }
    };
    let ends = [
        arm(10.0, 3.0, 6.0, 1.0),
        arm(130.0, 9.0, 3.0, 2.5),
        arm(250.0, 5.0, 5.0, 1.5),
    ];
    let mut parts = RoadParts::default();
    extrude_hubs(&ends, &hm, [0.0; 2], &dims, &mut parts);

    let near = |p: [f32; 3]| {
        parts.structure.vertices.iter().any(|v| {
            (v[0] - p[0]).abs() < 1.0e-3
                && (v[1] - p[1]).abs() < 1.0e-3
                && (v[2] - p[2]).abs() < 1.0e-3
        })
    };
    for e in &ends {
        let wo = e.half_w + ct + cf;
        for sgn in [-1.0_f32, 1.0] {
            let o = [e.cx + sgn * e.rx * wo, e.deck_y, e.cz + sgn * e.rz * wo];
            assert!(
                near(o),
                "asymmetric fillet missed the ribbon outer-curb point {o:?} (endpoint drift)"
            );
        }
    }
}
