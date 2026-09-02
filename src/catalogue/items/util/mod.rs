//! Shared construction vocabulary for primitive-built catalogue
//! entries (lighthouse, stone circle, ziggurat, observatory).
//!
//! The shape-grammar entries (villa, castle, watchtower, temple)
//! don't need these — their geometry comes from the grammar
//! interpreter. The primitive entries assemble `Generator` trees by
//! hand, and these helpers keep that assembly at the "place a tapered
//! cylinder here" altitude instead of struct-literal plumbing.

mod build;
#[cfg(test)]
mod checks;
mod material;

#[cfg(test)]
pub(super) use build::blob_cell_size;
pub(super) use build::{
    BALUSTER_PITCH, assemble, attach, blob_box, blob_capsule, blob_ellipsoid, blob_group, carved,
    cone, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, footing, footing_disc,
    foundation_block, foundation_disc, helix, id_quat, nest, pfp_panel, plane, prim, prim_scaled,
    quat_mul, quat_x, quat_y, quat_z, railing, solid, sphere, strut, superellipsoid, torus, tube,
    wedge, with_cut, with_face,
};
#[cfg(test)]
pub(super) use checks::{
    assert_cards_do_not_overlap, assert_no_glazing_on_solids, assert_no_tilted_parents,
    assert_owner_panel, assert_sanitize_stable, blob_components, has_emissive, rotate_by,
    window_cards,
};
pub(super) use material::{
    ageing, bonded_boards, bonded_brick, bonded_siding, face_uv_offset, foundation_mat, glow,
    lit_interior, quarter_turn, tile, tiles_per_metre, upright_boards, uv_for_scale, window_card,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::generator::FaceKey;
    use crate::pds::{Fp3, GeneratorKind, SovereignMaterialSettings};

    /// [`attach`] lands a ground-frame child where it was authored, which
    /// a raw `push` onto the same root does not (#1010).
    #[test]
    fn attach_rebases_a_child_the_way_assemble_would() {
        let mat = SovereignMaterialSettings::default();
        let base = prim(
            solid(cuboid_tapered([4.0, 0.4, 4.0], 0.0, mat.clone())),
            [0.0, 0.2, 0.0],
            id_quat(),
        );
        // Authored in the prop's ground frame, 3 m up.
        let authored = [0.5, 3.0, -1.0];

        let mut root = assemble(vec![base.clone()]);
        attach(
            &mut root,
            prim(
                solid(cuboid_tapered([0.2; 3], 0.0, mat.clone())),
                authored,
                id_quat(),
            ),
        );
        let child = &root.children[0];
        // World = root + local, so the local must be authored − root.
        let world: Vec<f32> = (0..3)
            .map(|i| root.transform.translation.0[i] + child.transform.translation.0[i])
            .collect();
        for i in 0..3 {
            assert!(
                (world[i] - authored[i]).abs() < 1e-5,
                "axis {i}: landed at {} not {}",
                world[i],
                authored[i]
            );
        }

        // The raw push it replaces lands a whole root-height out.
        let mut naive = assemble(vec![base]);
        naive.children.push(prim(
            solid(cuboid_tapered([0.2; 3], 0.0, mat)),
            authored,
            id_quat(),
        ));
        let naive_world_y =
            naive.transform.translation.0[1] + naive.children[0].transform.translation.0[1];
        assert!(
            (naive_world_y - authored[1]).abs() > 0.19,
            "fixture should show the un-rebased error"
        );
    }

    /// It composes with [`nest`] roots too — the barn/farmhouse idiom.
    #[test]
    fn attach_works_on_a_nested_root() {
        let mat = SovereignMaterialSettings::default();
        let parent = prim(
            solid(cuboid_tapered([2.0, 1.0, 2.0], 0.0, mat.clone())),
            [1.0, 0.5, 2.0],
            id_quat(),
        );
        let mut root = nest(parent, vec![]);
        attach(
            &mut root,
            prim(
                solid(cuboid_tapered([0.2; 3], 0.0, mat)),
                [1.0, 4.0, 2.0],
                id_quat(),
            ),
        );
        let c = &root.children[0].transform.translation.0;
        assert!((c[0] - 0.0).abs() < 1e-5, "{c:?}");
        assert!((c[1] - 3.5).abs() < 1e-5, "{c:?}");
        assert!((c[2] - 0.0).abs() < 1e-5, "{c:?}");
    }

    fn tinted(c: [f32; 3]) -> SovereignMaterialSettings {
        SovereignMaterialSettings {
            base_color: Fp3(c),
            ..Default::default()
        }
    }

    /// The face override lands on the record where the spawner reads it,
    /// and inherits the prim's projection (a recolour must not re-mesh).
    #[test]
    fn with_face_records_an_override_that_inherits_the_projection() {
        let kind = with_face(
            cuboid_tapered([1.0, 1.0, 1.0], 0.0, tinted([0.1, 0.1, 0.1])),
            FaceKey::Top,
            tinted([0.9, 0.2, 0.2]),
        );
        let faces = kind.faces().expect("a cuboid carries face overrides");
        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0].face, FaceKey::Top);
        assert_eq!(faces[0].material.base_color, Fp3([0.9, 0.2, 0.2]));
        assert_eq!(faces[0].uv_mapping, None);
    }

    /// Naming the same face twice replaces it. The sanitizer keeps the FIRST
    /// entry of a duplicate pair, so an appending helper would hand the
    /// author the value they overwrote.
    #[test]
    fn with_face_replaces_rather_than_stacking_a_duplicate() {
        let kind = with_face(
            with_face(
                cuboid_tapered([1.0, 1.0, 1.0], 0.0, tinted([0.1, 0.1, 0.1])),
                FaceKey::Top,
                tinted([0.9, 0.2, 0.2]),
            ),
            FaceKey::Top,
            tinted([0.2, 0.9, 0.2]),
        );
        let faces = kind.faces().unwrap();
        assert_eq!(faces.len(), 1, "a repeated face must not stack");
        assert_eq!(faces[0].material.base_color, Fp3([0.2, 0.9, 0.2]));
    }

    /// A railing spans its run, stands on the level it is given, and is made
    /// of things you can see between. The count is bounded so a promenade run
    /// cannot quietly cost ninety prims.
    #[test]
    fn railing_spans_its_run_and_has_gaps_in_it() {
        let run = railing(
            [-3.0, 1.5, -2.0],
            [3.0, 1.5, -2.0],
            1.0,
            BALUSTER_PITCH,
            tinted([0.5, 0.5, 0.5]),
        );
        let ys: Vec<f32> = run.iter().map(|g| g.transform.translation.0[1]).collect();
        assert!(
            ys.iter().all(|y| (1.5..=2.55).contains(y)),
            "a railing must stand on the level it is given: {ys:?}"
        );
        let balusters = run
            .iter()
            .filter(|g| match &g.kind {
                GeneratorKind::Cuboid { size, .. } => size.0[0] < 0.09 && size.0[1] > 0.5,
                _ => false,
            })
            .count();
        assert!(
            (6..=24).contains(&balusters),
            "{balusters} balusters over a 6 m run reads as a plate or a ladder"
        );
        // Two rails plus balusters plus two posts.
        assert_eq!(run.len(), balusters + 4);
        let widest = run
            .iter()
            .filter_map(|g| match &g.kind {
                GeneratorKind::Cuboid { size, .. } => Some(size.0[0]),
                _ => None,
            })
            .fold(0.0_f32, f32::max);
        assert!(
            widest > 5.0 && widest <= 6.0,
            "the handrail spans {widest} of a 6 m run"
        );
    }

    /// A strut's built cylinder actually lands both endpoints it was given.
    ///
    /// Verified through [`rotate_by`] — the guards' one quaternion
    /// implementation — so the authoring helper and the checking helper agree
    /// on handedness by construction. Cases cover a genuinely 3D diagonal
    /// (all three components nonzero, the case every hand-rolled version got
    /// wrong), a horizontal run, and the two degenerate verticals.
    #[test]
    fn strut_spans_exactly_the_two_points_it_is_given() {
        let mat = SovereignMaterialSettings::default();
        let cases = [
            ([0.5, 1.0, -2.0], [3.0, 4.5, 1.5]),
            ([-1.0, 2.0, 0.0], [1.0, 2.0, 0.0]),
            ([0.0, 0.0, 0.0], [0.0, 3.0, 0.0]),
            ([1.0, 5.0, 1.0], [1.0, 1.0, 1.0]),
        ];
        for (from, to) in cases {
            let g = strut(from, to, 0.05, 8, mat.clone());
            let GeneratorKind::Cylinder { height, .. } = &g.kind else {
                panic!("a strut is a cylinder");
            };
            let half = height.0 * 0.5;
            let c = g.transform.translation.0;
            let q = g.transform.rotation.0;
            let tip = rotate_by(q, [0.0, half, 0.0]);
            let top = [c[0] + tip[0], c[1] + tip[1], c[2] + tip[2]];
            let bot = [c[0] - tip[0], c[1] - tip[1], c[2] - tip[2]];
            // One end lands on `to`, the other on `from` (order depends on
            // the run's direction; both together is the claim that matters).
            let close = |a: [f32; 3], b: [f32; 3]| {
                (a[0] - b[0]).abs() < 1e-4
                    && (a[1] - b[1]).abs() < 1e-4
                    && (a[2] - b[2]).abs() < 1e-4
            };
            assert!(
                (close(top, to) && close(bot, from)) || (close(top, from) && close(bot, to)),
                "strut {from:?} -> {to:?} built ends {bot:?} and {top:?}"
            );
        }
    }

    /// A kind with no faces at all (here a particle system) passes through
    /// untouched instead of panicking — helpers compose over whole trees.
    #[test]
    fn with_face_leaves_a_faceless_kind_alone() {
        let particles = GeneratorKind::default_particles();
        let out = with_face(particles.clone(), FaceKey::Top, tinted([1.0, 0.0, 0.0]));
        assert_eq!(out, particles);
    }
}
