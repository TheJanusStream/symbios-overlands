//! Fountain — a tiered marble basin with a central jet. A prosperity-Rich
//! scatter prop: ornamental waterworks signal civic wealth in any setting.

use crate::catalogue::items::util::{cylinder_tapered, id_quat, nest, prim, solid, torus, tube};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{MARBLE, WATER_BLUE, marble};

/// Wet water — glossy, faintly self-lit blue so the pools read clearly
/// against the pale marble instead of vanishing into it.
fn water() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(WATER_BLUE),
        emission_color: Fp3(WATER_BLUE),
        emission_strength: Fp(0.5),
        roughness: Fp(0.12),
        metallic: Fp(0.0),
        ..Default::default()
    }
}

/// Deterministic salt for the jet's emitter seeds.
const FX_SEED: u64 = 0x0F00_47A1;

pub struct Fountain;

impl CatalogueEntry for Fountain {
    fn slug(&self) -> &'static str {
        "fountain"
    }
    fn name(&self) -> &'static str {
        "Fountain"
    }
    fn description(&self) -> &'static str {
        "Tiered marble basin with a central water jet."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn prosperity_band(&self) -> ProsperityBand {
        ProsperityBand::only(ProsperityTier::Rich)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.0,
            min_spawn_dist: 22.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Built as a **tree that stands the way the fountain does** (#970): the
/// basin floor is the root, the rim wall and the pedestal rise from it, and
/// every course parents to the one it stands on. Each piece is still
/// authored in the prop's own world frame — [`nest`] does the rebasing —
/// but the owner now gets sub-assemblies instead of a heap of siblings.
///
/// Dragging the pedestal drum lifts the whole waterworks: shaft, bowl, rim,
/// pool, jet and spray. Dragging the bowl takes its rim, its pool and the
/// jet with it. Dragging the rim wall carries its coping. That is the point
/// of the depth — under a flat list each of those is a separate drag, and
/// the parts drift apart the moment one is missed.
fn build_tree() -> Generator {
    // Upper bowl and everything it carries: its coping ring, the pool that
    // sits in it, and the jet thrown from that pool's surface.
    let bowl = nest(
        prim(
            solid(cylinder_tapered(0.62, 0.14, 20, 0.0, marble(MARBLE))),
            [0.0, 1.49, 0.0],
            id_quat(),
        ),
        vec![
            prim(
                torus(0.05, 0.62, marble([0.8, 0.79, 0.76])),
                [0.0, 1.56, 0.0],
                id_quat(),
            ),
            prim(
                cylinder_tapered(0.55, 0.07, 20, 0.0, water()),
                [0.0, 1.57, 0.0],
                id_quat(),
            ),
            // The jet, thrown from the bowl's surface: a dense arcing column
            // under real gravity, wrapped in the mist its own break-up throws
            // off. Both are particle systems — a fountain's whole appeal is
            // that the water *moves*, and the static rod-and-orb this replaces
            // read as a blue plastic lollipop from every angle. The mist hangs
            // off the jet, since it is what the jet does at its apex.
            nest(
                super::fx::water_jet([0.0, 1.62, 0.0], FX_SEED),
                vec![super::fx::water_mist([0.0, 2.35, 0.0], FX_SEED ^ 0x55)],
            ),
        ],
    );

    // The pedestal: foot drum, fluted baluster shaft, then the bowl.
    let pedestal = nest(
        prim(
            solid(cylinder_tapered(
                0.34,
                0.18,
                16,
                0.0,
                marble([0.8, 0.79, 0.76]),
            )),
            [0.0, 0.49, 0.0],
            id_quat(),
        ),
        vec![nest(
            prim(
                solid(cylinder_tapered(0.20, 0.85, 12, 0.15, marble(MARBLE))),
                [0.0, 1.0, 0.0],
                id_quat(),
            ),
            vec![bowl],
        )],
    );

    // Basin floor disc — a solid bottom so the pool never reads hollow, and
    // the flat, unrotated root everything else stands on.
    nest(
        prim(
            solid(cylinder_tapered(1.45, 0.12, 24, 0.0, marble(MARBLE))),
            [0.0, 0.06, 0.0],
            id_quat(),
        ),
        vec![
            // Open marble rim wall holding the lower pool (a hollow ring),
            // capped by the rounded coping lip proud of its top.
            nest(
                prim(
                    solid(tube(1.5, 1.28, 0.5, 24, marble(MARBLE))),
                    [0.0, 0.25, 0.0],
                    id_quat(),
                ),
                vec![prim(
                    torus(0.08, 1.5, marble([0.8, 0.79, 0.76])),
                    [0.0, 0.5, 0.0],
                    id_quat(),
                )],
            ),
            // Lower pool — a broad blue disc sitting recessed below the rim.
            prim(
                cylinder_tapered(1.26, 0.26, 24, 0.0, water()),
                [0.0, 0.27, 0.0],
                id_quat(),
            ),
            pedestal,
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::GeneratorKind;

    /// Every part, as `(kind tag, world position)`, by summing translations
    /// down the tree — which is exactly what the spawner does.
    fn parts(g: &Generator, at: [f32; 3], out: &mut Vec<(&'static str, [f32; 3])>) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        out.push((g.kind.kind_tag(), here));
        for c in &g.children {
            parts(c, here, out);
        }
    }

    /// #970: nesting is a change of *structure*, not of geometry. Every
    /// piece must land exactly where the flat build put it — the whole
    /// premise of authoring in the prop's world frame and letting [`nest`]
    /// rebase is that the two are interchangeable on screen.
    #[test]
    fn nesting_leaves_every_part_where_it_was() {
        let mut got = Vec::new();
        parts(&Fountain.build(""), [0.0; 3], &mut got);
        got.sort_by(|a, b| a.1[1].partial_cmp(&b.1[1]).unwrap());
        let want = [
            ("Cylinder", 0.06),       // basin floor
            ("Tube", 0.25),           // rim wall
            ("Cylinder", 0.27),       // lower pool
            ("Cylinder", 0.49),       // pedestal drum
            ("Torus", 0.5),           // coping lip
            ("Cylinder", 1.0),        // baluster shaft
            ("Cylinder", 1.49),       // upper bowl
            ("Torus", 1.56),          // upper rim
            ("Cylinder", 1.57),       // upper pool
            ("ParticleSystem", 1.62), // jet
            ("ParticleSystem", 2.35), // mist
        ];
        assert_eq!(got.len(), want.len(), "part count changed: {got:?}");
        for ((tag, pos), (want_tag, want_y)) in got.iter().zip(want) {
            assert_eq!(*tag, want_tag, "at y={}", pos[1]);
            assert!(
                (pos[1] - want_y).abs() < 1e-4,
                "{tag} sits at y={}, not {want_y}",
                pos[1]
            );
            assert!(
                pos[0].abs() < 1e-4 && pos[2].abs() < 1e-4,
                "{tag} drifted off the axis: {pos:?}"
            );
        }
    }

    /// #970: the hierarchy has to *hold* — an upper part is only draggable
    /// as a unit if everything it carries is inside its subtree. Grabbing
    /// the pedestal drum must take the shaft, the bowl, the bowl's rim and
    /// pool, the jet and the mist with it; grabbing the rim wall must take
    /// its coping.
    #[test]
    fn each_sub_assembly_carries_what_it_holds_up() {
        let root = Fountain.build("");
        let subtree_size = |node: &Generator| {
            fn count(g: &Generator) -> usize {
                1 + g.children.iter().map(count).sum::<usize>()
            }
            count(node)
        };
        // Root is the basin floor, at the bottom, unrotated so nothing it
        // carries is spun by its own transform.
        assert!(
            matches!(root.kind, GeneratorKind::Cylinder { .. }),
            "the root should be the basin floor"
        );
        assert_eq!(root.transform.rotation.0, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(root.children.len(), 3, "rim wall, lower pool, pedestal");

        let rim = &root.children[0];
        assert_eq!(subtree_size(rim), 2, "the rim wall carries its coping");

        let pedestal = &root.children[2];
        assert_eq!(
            subtree_size(pedestal),
            7,
            "the pedestal carries shaft, bowl, rim, pool, jet and mist"
        );

        // …and the jet carries the spray it throws.
        let bowl = &pedestal.children[0].children[0];
        let jet = bowl
            .children
            .iter()
            .find(|c| matches!(c.kind, GeneratorKind::ParticleSystem(_)))
            .expect("the bowl holds the jet");
        assert_eq!(subtree_size(jet), 2, "the jet carries its mist");
    }
}
