use super::*;
use crate::urban::test_support::*;
use crate::urban::{Chain, Dims, RoadParts, build_road_geometry};
use bevy_symbios_ground::HeightMap;

/// Every emitted vertex must be finite — a NaN from a degenerate miter or
/// normalize would poison the mesh.
#[test]
fn geometry_is_finite() {
    if let Some(parts) = build_road_geometry(&sloped_heightmap(), &cfg(7)) {
        assert!(!parts.deck.is_empty());
        for geo in surfaces(&parts) {
            for v in &geo.vertices {
                assert!(v.iter().all(|c| c.is_finite()), "non-finite vertex {v:?}");
            }
            for nrm in &geo.normals {
                assert!(nrm.iter().all(|c| c.is_finite()), "non-finite normal");
            }
        }
    }
}

/// WS2: the ribbon is shaded smoothly. The deck strip welds its vertices
/// along the chain (so adjacent quads share normals → no facet), which means
/// far fewer vertices than the 4-per-quad an unwelded flat-shaded build would
/// emit; and every normal is unit length (the smoothing's `normalize`).
#[test]
fn deck_is_welded_with_unit_normals() {
    let parts = build_road_geometry(&pilot_heightmap(), &cfg(PILOT_ROAD_SEED))
        .expect("pilot network must produce roads");
    let deck_quads = parts.deck.indices.len() / 6;
    assert!(deck_quads > 0, "no deck quads");
    assert!(
        parts.deck.vertices.len() < 4 * deck_quads,
        "deck is not welded ({} verts for {deck_quads} quads — flat per-face?)",
        parts.deck.vertices.len()
    );
    for geo in surfaces(&parts) {
        for nrm in &geo.normals {
            let len2 = nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2];
            assert!((len2 - 1.0).abs() < 1.0e-3, "non-unit normal {nrm:?}");
        }
    }
}

/// WS3: the drivable deck never sinks below the terrain. Every deck vertex
/// sits at or above the ground beneath it — the upward-only drape. (Road
/// geometry is authored in the heightmap frame, so `get_height_at` at the
/// vertex XZ is the terrain under it.)
#[test]
fn deck_never_buries() {
    let hm = pilot_heightmap();
    let parts =
        build_road_geometry(&hm, &cfg(PILOT_ROAD_SEED)).expect("pilot network must produce roads");
    for v in &parts.deck.vertices {
        let ground = hm.get_height_at(v[0], v[2]);
        assert!(
            v[1] + 1.0e-3 >= ground,
            "deck vertex {v:?} buried below terrain {ground}"
        );
    }
}

/// The skirt drops a FIXED `skirt_depth` below the deck instead of reaching down
/// to the terrain, so a deck that grade-levels HIGH over a deep dip leaves the
/// road underside floating clear — a bridge, not an earth-filled embankment.
/// Builds a road crossing a narrow deep dip and asserts (a) the structure over
/// the dip clears the dip floor by a wide margin and (b) the skirt bottom sits
/// exactly `skirt_depth` below the deck above it.
#[test]
fn high_deck_skirt_floats_clear_over_a_dip() {
    let dims = Dims::from_config(&cfg(7)); // skirt_depth = 5.0
    // A high plateau (30 m) with a narrow, deep dip (0 m) in the middle band. The
    // deck's longitudinal grade limit keeps it near the plateau across the 24 m
    // dip, so the deck rides ~28 m above a 0 m floor.
    let mut hm = HeightMap::new(96, 96, 2.0);
    let w = hm.width();
    for z in 0..w {
        for x in 0..w {
            let world_x = x as f32 * hm.scale();
            hm.set(
                x,
                z,
                if (world_x - 96.0).abs() < 12.0 {
                    0.0
                } else {
                    30.0
                },
            );
        }
    }
    let chain = Chain {
        pts: vec![(40.0, 96.0), (96.0, 96.0), (152.0, 96.0)],
        half_w: dims.minor_half_width,
        end_nodes: [0, 1],
        clip: [false, false],
    };
    let degree = vec![2u32, 2u32];
    let mut road_ends = Vec::new();
    let mut parts = RoadParts::default();
    extrude_chain(
        &chain,
        0.0,
        0.0,
        &hm,
        0.0,
        &dims,
        &degree,
        &mut road_ends,
        &mut parts,
    );

    // Over the dip centre nothing reaches down to the 0 m floor — the underside
    // floats clear. (A dynamic terrain-reaching skirt would sit at ~floor − 0.3.)
    let over_dip = |v: &[f32; 3]| (v[0] - 96.0).abs() < 8.0;
    let mut struct_min = f32::INFINITY;
    for v in &parts.structure.vertices {
        if over_dip(v) {
            let terrain = hm.get_height_at(v[0], v[2]); // ~0 over the dip
            assert!(
                v[1] > terrain + 2.0,
                "structure vertex {v:?} dives toward the dip floor {terrain} — not a bridge"
            );
            struct_min = struct_min.min(v[1]);
        }
    }
    let mut deck_min = f32::INFINITY;
    for v in &parts.deck.vertices {
        if over_dip(v) {
            deck_min = deck_min.min(v[1]);
        }
    }
    assert!(
        struct_min.is_finite() && deck_min.is_finite(),
        "road did not span the dip"
    );
    // The skirt bottom sits EXACTLY skirt_depth below the deck it hangs from —
    // the fixed-depth contract, independent of the terrain below.
    assert!(
        (deck_min - struct_min - dims.skirt_depth).abs() < 0.05,
        "skirt sat {} m below the deck, expected the fixed {} m",
        deck_min - struct_min,
        dims.skirt_depth
    );
}

/// #576 on the real pilot network (it carries acute junctions down to ~23°):
/// every deck normal — ribbon *and* hub — faces up, so back-face culling
/// keeps the drivable surface visible from above, and every vertex is finite.
/// Guards against a folded / downward-wound hub fan on real data.
#[test]
fn pilot_deck_is_finite_and_faces_up() {
    let hm = pilot_heightmap();
    let parts =
        build_road_geometry(&hm, &cfg(PILOT_ROAD_SEED)).expect("pilot network must produce roads");
    for v in &parts.deck.vertices {
        assert!(
            v.iter().all(|c| c.is_finite()),
            "non-finite deck vertex {v:?}"
        );
    }
    for nrm in &parts.deck.normals {
        assert!(
            nrm.iter().all(|c| c.is_finite()) && nrm[1] > 0.0,
            "deck normal {nrm:?} not finite-and-upward",
        );
    }
}

/// #579: a degree-1 dead-end gets a flat cross-section cap (closing the open
/// hollow tube), facing outward; an UNclipped degree-2 end does NOT (a mid-run
/// node, loop closure or used-edge break — perimeter clips are #582's job and
/// carry `clip=true`, set false throughout here). The cap faces along the road
/// tangent (±x for an x-running chain), HORIZONTAL — no ribbon face does (deck
/// +y, curb/skirt ±z lateral) — so counting its ±x normals uniquely detects it.
#[test]
fn dead_end_gets_a_cross_section_cap() {
    let dims = Dims::from_config(&cfg(7));
    let hm = HeightMap::new(64, 64, 2.0); // flat at 0
    let chain = Chain {
        pts: vec![(20.0, 20.0), (30.0, 20.0), (50.0, 20.0)],
        half_w: dims.minor_half_width,
        end_nodes: [0, 1],
        clip: [false, false],
    };
    // Cap normals for a given per-node degree: horizontal (|n.y|≈0), facing ±x.
    let cap_x = |degree: &[u32]| -> Vec<f32> {
        let mut road_ends = Vec::new();
        let mut parts = RoadParts::default();
        extrude_chain(
            &chain,
            0.0,
            0.0,
            &hm,
            0.0,
            &dims,
            degree,
            &mut road_ends,
            &mut parts,
        );
        for v in &parts.structure.vertices {
            assert!(
                v.iter().all(|c| c.is_finite()),
                "non-finite cap vertex {v:?}"
            );
        }
        parts
            .structure
            .normals
            .iter()
            .filter(|n| n[0].abs() > 0.9 && n[1].abs() < 0.05)
            .map(|n| n[0])
            .collect()
    };
    // degree-1 START → one cap (10 verts) facing −x; degree-2 end → none.
    let start = cap_x(&[1, 2]);
    assert_eq!(
        start.len(),
        10,
        "degree-1 start: expected one capped cross-section"
    );
    assert!(
        start.iter().all(|&x| x < 0.0),
        "start cap must face −x (outward)"
    );
    // degree-1 END → one cap facing +x (exercises the slot-last path + sign).
    let end = cap_x(&[2, 1]);
    assert_eq!(
        end.len(),
        10,
        "degree-1 end: expected one capped cross-section"
    );
    assert!(
        end.iter().all(|&x| x > 0.0),
        "end cap must face +x (outward)"
    );
    // Both ends degree-2 (district clips) → no caps.
    assert_eq!(
        cap_x(&[2, 2]).len(),
        0,
        "district-edge clips wrongly capped"
    );
    // A junction end (≥3) is closed by its hub, not a cap.
    assert_eq!(cap_x(&[1, 3]).len(), 10, "only the degree-1 end caps");
}

/// #579 (review wf_aabe1626 HIGH): the cap's explicit triangulation must TILE
/// the concave profile exactly — no gap, no overlap, no spill past the
/// silhouette (the bug the apex-fan had). The cross-section is rigid, so the
/// summed triangle areas must equal the profile polygon's shoelace area.
#[test]
fn dead_end_cap_triangulation_tiles_the_profile() {
    let dims = Dims::from_config(&cfg(7));
    let prof = profile(dims.minor_half_width, &dims);
    let tri_area = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
        ((b.0 - a.0) * (c.1 - a.1) - (c.0 - a.0) * (b.1 - a.1)).abs() * 0.5
    };
    // The exact triangulation push_end_cap emits (body + two curb wedges).
    let tris = [
        [7, 4, 5],
        [7, 5, 6],
        [1, 2, 3],
        [1, 3, 4],
        [7, 8, 9],
        [7, 9, 0],
    ];
    let tri_sum: f32 = tris
        .iter()
        .map(|t| tri_area(prof[t[0]], prof[t[1]], prof[t[2]]))
        .sum();
    let mut shoelace = 0.0_f32;
    for i in 0..prof.len() {
        let (a, b) = (prof[i], prof[(i + 1) % prof.len()]);
        shoelace += a.0 * b.1 - b.0 * a.1;
    }
    let poly = shoelace.abs() * 0.5;
    assert!(
        (tri_sum - poly).abs() < 1.0e-4,
        "cap triangulation does not tile the profile: triangles {tri_sum} vs polygon {poly}"
    );
    assert!(poly > 0.0, "degenerate profile");
}

/// #579 (review wf_aabe1626): the cap is a VERTICAL cross-section, so its normal
/// must stay HORIZONTAL on sloped terrain — using the road tangent would tilt it
/// by the longitudinal grade and mis-shade the cul-de-sac on a hill.
#[test]
fn dead_end_cap_normal_is_horizontal_on_a_slope() {
    let dims = Dims::from_config(&cfg(7));
    // A ramp in x → the deck/skirt grade is non-zero along the road.
    let mut hm = HeightMap::new(64, 64, 2.0);
    let w = hm.width();
    for z in 0..w {
        for x in 0..w {
            hm.set(x, z, x as f32 * 0.5);
        }
    }
    let chain = Chain {
        pts: vec![(20.0, 20.0), (35.0, 20.0), (60.0, 20.0)],
        half_w: dims.minor_half_width,
        end_nodes: [0, 1],
        clip: [false, false],
    };
    let degree = vec![1u32, 2u32];
    let mut road_ends = Vec::new();
    let mut parts = RoadParts::default();
    extrude_chain(
        &chain,
        0.0,
        0.0,
        &hm,
        0.0,
        &dims,
        &degree,
        &mut road_ends,
        &mut parts,
    );
    // Cap normals face ±x strongly; the grade-limited deck/curb never exceeds
    // |n.x| 0.5, so this isolates the cap.
    let caps: Vec<_> = parts
        .structure
        .normals
        .iter()
        .filter(|n| n[0].abs() > 0.5)
        .collect();
    assert!(!caps.is_empty(), "no cap emitted on sloped terrain");
    for n in caps {
        assert!(
            n[1].abs() < 1.0e-3,
            "cap normal not horizontal on a slope: {n:?}"
        );
        let len2 = n[0] * n[0] + n[1] * n[1] + n[2] * n[2];
        assert!((len2 - 1.0).abs() < 1.0e-3, "cap normal not unit: {n:?}");
    }
}

/// #582: a boundary-clip end (a road running off the network perimeter) is
/// capped like a dead-end even though its node is degree-2 — the cap is driven
/// by `chain.clip[slot]`, independent of degree. Same ±x-horizontal-normal
/// signature as the #579 dead-end cap, so counting those isolates it.
#[test]
fn clip_end_emits_a_cap_cross_section() {
    let dims = Dims::from_config(&cfg(7));
    let hm = HeightMap::new(64, 64, 2.0); // flat at 0
    // Degree-2 at BOTH ends: no dead-end, no junction → caps depend purely on
    // the clip flags, isolating the #582 path from the #579 degree path.
    let degree = vec![2u32, 2u32];
    let cap_x = |clip: [bool; 2]| -> Vec<f32> {
        let chain = Chain {
            pts: vec![(20.0, 20.0), (30.0, 20.0), (50.0, 20.0)],
            half_w: dims.minor_half_width,
            end_nodes: [0, 1],
            clip,
        };
        let mut road_ends = Vec::new();
        let mut parts = RoadParts::default();
        extrude_chain(
            &chain,
            0.0,
            0.0,
            &hm,
            0.0,
            &dims,
            &degree,
            &mut road_ends,
            &mut parts,
        );
        for v in &parts.structure.vertices {
            assert!(
                v.iter().all(|c| c.is_finite()),
                "non-finite cap vertex {v:?}"
            );
        }
        parts
            .structure
            .normals
            .iter()
            .filter(|n| n[0].abs() > 0.9 && n[1].abs() < 0.05)
            .map(|n| n[0])
            .collect()
    };
    // clip START only → one cap (10 verts) facing −x (outward).
    let start = cap_x([true, false]);
    assert_eq!(
        start.len(),
        10,
        "clip start: expected one capped cross-section"
    );
    assert!(
        start.iter().all(|&x| x < 0.0),
        "clip start cap must face −x"
    );
    // clip END only → one cap facing +x (exercises slot-last + sign).
    let end = cap_x([false, true]);
    assert_eq!(end.len(), 10, "clip end: expected one capped cross-section");
    assert!(end.iter().all(|&x| x > 0.0), "clip end cap must face +x");
    // No clip, degree-2 both ends → still open (regression lock: the #582 bug
    // was exactly this end left uncapped). A loop closure / used-edge break
    // arrives here as clip=[false,false] and must stay open.
    assert_eq!(
        cap_x([false, false]).len(),
        0,
        "non-clip degree-2 ends must stay open"
    );
    // Both ends clipped (a fully-perimeter sliver) → two caps.
    assert_eq!(cap_x([true, true]).len(), 20, "both rim ends must cap");
}

/// #582 (mirrors the #579 review wf_aabe1626 finding): a clip cap is a VERTICAL
/// cross-section, so its normal must stay HORIZONTAL on sloped terrain — the
/// road tangent would tilt it by the longitudinal grade and mis-shade the
/// perimeter end on a hill. Driven through the clip path (degree-2 end).
#[test]
fn clip_end_cap_normal_is_horizontal_on_a_slope() {
    let dims = Dims::from_config(&cfg(7));
    let mut hm = HeightMap::new(64, 64, 2.0);
    let w = hm.width();
    for z in 0..w {
        for x in 0..w {
            hm.set(x, z, x as f32 * 0.5); // ramp in x → non-zero deck grade
        }
    }
    let chain = Chain {
        pts: vec![(20.0, 20.0), (35.0, 20.0), (60.0, 20.0)],
        half_w: dims.minor_half_width,
        end_nodes: [0, 1],
        clip: [false, true], // far end runs off the rim
    };
    let degree = vec![2u32, 2u32];
    let mut road_ends = Vec::new();
    let mut parts = RoadParts::default();
    extrude_chain(
        &chain,
        0.0,
        0.0,
        &hm,
        0.0,
        &dims,
        &degree,
        &mut road_ends,
        &mut parts,
    );
    let caps: Vec<_> = parts
        .structure
        .normals
        .iter()
        .filter(|n| n[0].abs() > 0.5)
        .collect();
    assert!(!caps.is_empty(), "no clip cap emitted on sloped terrain");
    for n in caps {
        assert!(
            n[1].abs() < 1.0e-3,
            "clip cap normal not horizontal on a slope: {n:?}"
        );
        let len2 = n[0] * n[0] + n[1] * n[1] + n[2] * n[2];
        assert!(
            (len2 - 1.0).abs() < 1.0e-3,
            "clip cap normal not unit: {n:?}"
        );
    }
}
