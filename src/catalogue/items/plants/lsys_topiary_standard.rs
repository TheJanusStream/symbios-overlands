//! Topiary standard — a box "lollipop": one trained clean stem carrying a
//! head sheared to a ball (#972, the civic planter's centrepiece). The
//! formal counterpart to [`lsys_bush`](super::lsys_bush): where the bush is
//! left to its own basitonic dome, this plant's silhouette is imposed by a
//! gardener, and the grammar models the two things the gardener does.
//!
//! **Training.** `T(n)` is the leader, grown one internode a step with every
//! lateral rubbed off (the rule simply emits none) until it reaches the
//! trained height, where it is pinched: the apex stops and the head breaks
//! into twenty scaffold shoots `S`. Their launch pitches are the polar
//! angles of an even spread over the sphere (`cos θ` in equal steps from
//! 15° to 140°, the bottom cap left to the stem) rolled at the exact golden
//! angle, so the scaffolds fill the ball rather than stacking into columns.
//!
//! **Shearing.** Clipping is an environmental constraint, not a growth
//! habit, so it is modelled the way ABOP models any environment an
//! L-system cannot query: as a budget the module carries. A scaffold
//! launched at pitch `t` from the pinch point meets the shear sphere
//! (radius `R`, centred `c` above the pinch) after
//! `c·cos t + √(R² − c²·sin² t)` metres, and every shoot descended from it
//! carries `u`, the length already grown. While the remaining reach
//! exceeds one internode the shoot extends or forks; when it does not, the
//! shears have taken the tip and it becomes `C`, a clipped tuft. Clipping
//! is what makes box dense: every cut breaks the buds behind it, so the
//! foliage of a sheared ball is a shell of short twiglets. Once every tip is
//! `C` the head stops growing — a maintained standard, not an age sweep
//! that plateaus by accident.
//!
//! **Self-shading.** Box sheds the leaves it cannot light, so the inside of
//! a clipped ball is bare wood. `K(t,u)` leaf sites are expressed only in
//! the outer shell (less than `2.5·il` of reach left), which is also where
//! the triangle budget buys anything: interior cards would be invisible.
//!
//! Foliage is a monopodial Twig card (box leaves are opposite) laid
//! TANGENT to the ball: a card's plane is the turtle's heading and pitch
//! axis, so pitching `&(78)` off an outward-pointing tip lays it along the
//! surface with its face outward. A card left pointing along the tip would
//! be seen edge-on from exactly the direction that tip faces.
//!
//! Slots follow the playbook's convention for a new species: 0 bark (the
//! stem), 1 twig (the head's scaffold wood), 2 leaf (the clipped foliage).

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig,
    SovereignLeafConfig, SovereignMaterialSettings, SovereignTextureConfig, SovereignTwigConfig,
};

pub struct TopiaryStandard;

impl CatalogueEntry for TopiaryStandard {
    fn slug(&self) -> &'static str {
        "lsys_topiary_standard"
    }
    fn name(&self) -> &'static str {
        "Topiary Standard"
    }
    fn description(&self) -> &'static str {
        "Clipped box ball on a trained clean stem — formal planting."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Plant
    }
    fn build(&self, _local_did: &str) -> Generator {
        Generator::from_kind(build_kind())
    }
}

/// Height of the trained stem from the soil to the pinch point, in metres —
/// three internodes of [`STEM_INTERNODE`]. The planter derives the ball's
/// position from this.
pub(crate) const STEM_INTERNODE: f32 = 0.17;
/// Internodes in the trained stem before the pinch.
pub(crate) const STEM_INTERNODES: u32 = 3;
/// Shear sphere radius (the wood's reach; the foliage cards stand a few
/// centimetres proud of it).
pub(crate) const SHEAR_R: f32 = 0.28;
/// Height of the shear sphere's centre above the pinch point.
pub(crate) const SHEAR_C: f32 = 0.14;
/// Scaffold shoots the pinched head breaks into.
const SCAFFOLDS: u32 = 20;
/// Pitch of the highest scaffold from the vertical, in degrees.
const SCAFFOLD_TOP: f32 = 15.0;
/// Pitch of the lowest — below it is the stem's own cap.
const SCAFFOLD_BOTTOM: f32 = 140.0;

pub(crate) fn build_kind() -> GeneratorKind {
    let mut materials = HashMap::new();
    // 0 — smooth grey-brown box stem.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.36, 0.30, 0.22]),
            roughness: Fp(0.85),
            uv_scale: Fp(2.0),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                color_light: Fp3([0.44, 0.38, 0.29]),
                color_dark: Fp3([0.22, 0.18, 0.13]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 1 — the head's scaffold wood, still green-brown.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([0.30, 0.26, 0.15]),
            roughness: Fp(0.9),
            ..Default::default()
        },
    );
    // 2 — clipped box foliage: short twiglets of small, entire (unlobed,
    // unserrated) leaves in opposite pairs. Matte on purpose: box is glossy,
    // but at card scale a glossy card flashes near-white at the sun.
    materials.insert(
        2,
        SovereignMaterialSettings {
            base_color: Fp3([0.16, 0.30, 0.10]),
            roughness: Fp(0.8),
            texture: SovereignTextureConfig::Twig(SovereignTwigConfig {
                leaf: SovereignLeafConfig {
                    color_base: Fp3([0.08, 0.22, 0.06]),
                    color_edge: Fp3([0.14, 0.30, 0.08]),
                    serration_strength: Fp64(0.0),
                    lobe_count: Fp64(0.0),
                    lobe_depth: Fp64(0.0),
                    petiole_length: Fp64(0.06),
                    vein_count: Fp64(3.0),
                    ..Default::default()
                },
                stem_color: Fp3([0.22, 0.26, 0.10]),
                leaf_pairs: 5,
                leaf_angle: Fp64(1.2),
                leaf_scale: Fp64(0.36),
                sympodial: false,
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Twig);

    // The reach left to a shoot of scaffold pitch `t` that has grown `u`.
    let reach = "c*cos(t*rd)+sqrt(R*R-c*c*sin(t*rd)*sin(t*rd))-u";
    // The scaffold pitches: `cos θ` in equal steps over the head, so the
    // shoots are spread evenly over the sphere's area rather than its angle.
    let (cos_top, cos_bottom) = (
        SCAFFOLD_TOP.to_radians().cos(),
        SCAFFOLD_BOTTOM.to_radians().cos(),
    );
    let scaffolds = (0..SCAFFOLDS)
        .map(|i| {
            let c = cos_top + (cos_bottom - cos_top) * (i as f32 + 0.5) / SCAFFOLDS as f32;
            let t = (c.acos().to_degrees() * 10.0).round() / 10.0;
            format!("[&({t})S({t},0)]")
        })
        .collect::<Vec<_>>()
        .join("/(137.5)");
    // A clipped tuft: five twiglets laid tangent round the cut tip. A
    // terminal ring that never iterates is safe at exactly 72°.
    let rosette = (0..5)
        .map(|_| "[&(78)~(0,2.8)]")
        .collect::<Vec<_>>()
        .join("/(72)");

    GeneratorKind::LSystem {
        source_code: format!(
            "#define R {SHEAR_R}\n\
             #define c {SHEAR_C}\n\
             #define il 0.06\n\
             #define rd 0.0174533\n\
             #define vr 1.1\n\
             omega: !(0.012)T(0)\n\
             t1: T(n) : n < {STEM_INTERNODES} -> F({STEM_INTERNODE})T(n+1)\n\
             t2: T(n) : n >= {STEM_INTERNODES} -> {scaffolds}\n\
             s1: 0.6 : S(t,u) : {reach} > il -> ,(1)!(0.005)F(il)K(t,u)[+(26)S(t,u+il)]-(26)/(90)S(t,u+il)\n\
             s2: 0.4 : S(t,u) : {reach} > il -> ,(1)!(0.005)F(il)K(t,u)/(137.5)S(t,u+il)\n\
             s3: S(t,u) : {reach} <= il -> C\n\
             w1: !(w) : * -> !(w*vr)"
        ),
        finalization_code: format!(
            "k1: K(t,u) : {reach} < 2.5*il -> ,(2)[&(70)~(0,2.6)]/(180)[&(70)~(0,2.6)]\n\
             k2: K(t,u) : {reach} >= 2.5*il -> \n\
             c1: C -> ,(2){rosette}\n\
             s4: S(t,u) : * -> ,(2){rosette}\n\
             t3: T(n) : * -> "
        ),
        iterations: 11,
        seed: 1,
        angle: Fp(26.0),
        step: Fp(1.0),
        width: Fp(0.012),
        elasticity: Fp(0.02),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
        materials,
        prop_mappings,
        prop_scale: Fp(0.045),
        mesh_resolution: 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = TopiaryStandard.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
