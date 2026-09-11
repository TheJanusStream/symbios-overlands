//! Trailing ivy — a fan of juvenile ivy shoots that runs out from its crown
//! and hangs over whatever edge it reaches (#972, the civic planter's edge
//! planting). A variegated leaf, cream at the margin, so it reads against
//! the dark clipped box it is planted under.
//!
//! **Why it trails.** Nothing in the grammar says "hang". A juvenile ivy
//! shoot has no stiffness to speak of, so the shoots are launched a little
//! above horizontal and gravity does the rest: tropism bends each drawn
//! segment by `elasticity·|H × T|`, and because the bend is per segment the
//! droop knob is the segment count (playbook §4) — eleven short internodes
//! arc over and fall, where two long ones would stay a stiff spike. The same
//! rule, read the other way, is what clears a container's rim: each shoot's
//! first internode is its oldest, stiffest wood and is drawn as ONE segment
//! (`F(rise)`), so it takes one bend where the young shoot beyond it takes
//! one per node — it climbs over the coping before gravity wins.
//!
//! **Growth.** Subapical and one internode per step (`V(n)` carries its own
//! node count, playbook §1 — `age` is per-rewrite), so iteration count is the
//! shoots' length. The seed is spent on topology rather than on angles (§7):
//! a stall alternative leaves shoots that rested a step shorter, and a
//! branch alternative sprouts a side shoot that starts three nodes older, so
//! the trailing edge comes out ragged instead of a fringe cut to one length.
//! Side shoots break only from the fourth node on — past a rim, where a
//! lateral has room — because one breaking at the rim runs along it and
//! droops into the stone. (The guard language's `and` is a single `&`.)
//!
//! **Leaves.** Ivy on a juvenile shoot is alternate and two-ranked, so each
//! node rolls `/(180)`. The leaf is YAWED off the stem, not pitched: a card's
//! plane is the turtle's heading and pitch axis, and a yaw keeps it in that
//! plane, so on a shoot hanging down a wall every leaf lies flat against the
//! wall with its face outward — which is how ivy actually hangs.
//!
//! The fan is authored toward the plant's own `-Z`; a container turns the
//! node to aim it. Slots: 0 bark (the stem), 2 leaf. There is no twig
//! cluster, so slot 1 is deliberately unused.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, PropMeshType, SovereignLeafConfig,
    SovereignMaterialSettings, SovereignTextureConfig,
};

pub struct TrailingIvy;

impl CatalogueEntry for TrailingIvy {
    fn slug(&self) -> &'static str {
        "lsys_trailing_ivy"
    }
    fn name(&self) -> &'static str {
        "Trailing Ivy"
    }
    fn description(&self) -> &'static str {
        "Variegated ivy whose shoots run out and hang over an edge."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Plant
    }
    fn build(&self, _local_did: &str) -> Generator {
        Generator::from_kind(build_kind())
    }
}

/// The shoots' launch yaws across the fan, in degrees either side of `-Z`,
/// with each one's launch pitch off the vertical.
const SHOOTS: [(f32, f32); 9] = [
    (-46.0, 38.0),
    (-35.0, 34.0),
    (-23.0, 40.0),
    (-12.0, 36.0),
    (0.0, 39.0),
    (11.0, 35.0),
    (22.0, 40.0),
    (34.0, 34.0),
    (45.0, 37.0),
];

pub(crate) fn build_kind() -> GeneratorKind {
    let mut materials = HashMap::new();
    // 0 — thin green-brown ivy stem.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.30, 0.28, 0.16]),
            roughness: Fp(0.85),
            ..Default::default()
        },
    );
    // 2 — variegated juvenile ivy leaf: five-lobed (two a side plus the
    // tip), no teeth, a pale gold margin over a mid green — a cream one
    // washes out to grey at a few metres.
    materials.insert(
        2,
        SovereignMaterialSettings {
            base_color: Fp3([0.34, 0.48, 0.24]),
            roughness: Fp(0.7),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig {
                color_base: Fp3([0.14, 0.33, 0.10]),
                color_edge: Fp3([0.62, 0.66, 0.36]),
                serration_strength: Fp64(0.0),
                lobe_count: Fp64(2.0),
                lobe_depth: Fp64(0.42),
                lobe_sharpness: Fp64(1.6),
                vein_count: Fp64(4.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Leaf);

    // `^` pitches the heading toward -Z; the yaw then swings it across X.
    let fan = SHOOTS
        .iter()
        .map(|(yaw, pitch)| format!("[^({pitch})+({yaw})F(rise)L/(180)V(0)]"))
        .collect::<String>();

    GeneratorKind::LSystem {
        source_code: format!(
            "#define il 0.05\n\
             #define rise 0.15\n\
             #define nmax 11\n\
             omega: !(0.006){fan}\n\
             v1: 0.66 : V(n) : n < nmax -> F(il)L/(180)V(n+1)\n\
             v2: 0.16 : V(n) : n < nmax -> V(n)\n\
             v3: 0.18 : V(n) : n > 2 & n < nmax - 3 -> F(il)L[-(38)V(n+3)]/(180)V(n+1)"
        ),
        finalization_code: "l1: L -> ,(2)[+(58)~(0,2.8)]\n\
             v4: V(n) : * -> ,(2)[+(40)~(0,1.6)][-(40)~(0,1.6)]"
            .to_string(),
        iterations: 11,
        seed: 1,
        angle: Fp(30.0),
        step: Fp(1.0),
        width: Fp(0.006),
        elasticity: Fp(0.3),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
        materials,
        prop_mappings,
        prop_scale: Fp(0.045),
        mesh_resolution: 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = TrailingIvy.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
