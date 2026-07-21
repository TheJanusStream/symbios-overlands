//! Palm — a tall bare trunk crowned by a whorl of drooping fronds. The trunk
//! grows one segment per iteration so iteration count reads as age (#910:
//! stub → frond skeleton → leafy juvenile → tall mature palm), and stochastic
//! rules vary crown frond count/spacing and whole-palm stance (vertical or
//! gently arcing lean) per seed. The signature coastal / tropical scatter
//! where a conifer reads wrong.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig,
    SovereignFrondConfig, SovereignMaterialSettings, SovereignTextureConfig,
};

pub struct Palm;

impl CatalogueEntry for Palm {
    fn slug(&self) -> &'static str {
        "lsys_palm"
    }
    fn name(&self) -> &'static str {
        "Palm"
    }
    fn description(&self) -> &'static str {
        "Tall bare trunk crowned by a whorl of drooping fronds — a coastal palm."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Plant
    }
    fn build(&self, _local_did: &str) -> Generator {
        Generator::from_kind(build_kind())
    }
}

fn build_kind() -> GeneratorKind {
    let mut materials = HashMap::new();
    // 0 — fibrous brown trunk.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.42, 0.30, 0.16]),
            roughness: Fp(0.9),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                color_light: Fp3([0.45, 0.32, 0.18]),
                color_dark: Fp3([0.20, 0.13, 0.07]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 1 — deep tropical green frond leaflet. A broad, entire-margined pinna
    // (no lobes) — the strap-shaped leaflet of a palm frond, not a broadleaf.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([0.24, 0.46, 0.18]),
            roughness: Fp(0.6),
            texture: SovereignTextureConfig::Frond(SovereignFrondConfig {
                color_base: Fp3([0.12, 0.34, 0.10]),
                color_edge: Fp3([0.20, 0.42, 0.12]),
                width: Fp64(0.2),
                tip_taper: Fp64(1.1),
                vein_count: Fp64(11.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Leaf);

    GeneratorKind::LSystem {
        // Age-progressive palm (#910): the trunk GROWS one segment per
        // iteration (T self-extends with a stochastic roll wander) under a
        // crown C picked once, stochastically, from three irregular 5/6/7-
        // frond whorls — so iteration count is the palm's age and the seed
        // varies frond count, spacing, and stance. G rolls the whole palm's
        // stance once: vertical or leaned ±3°. A vertical heading is a
        // tropism fixpoint (bend ∝ |heading × −Y|), so a vertical palm stays
        // straight while a leaned one arcs gracefully with height — never the
        // runaway shepherd's crook a per-segment pitch wander produced.
        // Rachis D pitches ever more steeply down (&8→60, tuned for the
        // softer 0.28 elasticity) so the feathered blade arcs over and
        // droops past horizontal — leaflet pairs P run out each side.
        source_code: "#define s 0.9\n\
                      omega: !(0.26)G\n\
                      g1: 0.4 : G -> T C\n\
                      g2: 0.3 : G -> &(3)T C\n\
                      g3: 0.3 : G -> ^(3)/(40)T C\n\
                      t1: 0.6 : T -> F(s)/(23)T\n\
                      t2: 0.4 : T -> F(s)/(48)T\n\
                      c1: 0.35 : C -> [^(18)&(30)D]/(58)[^(16)&(34)D]/(63)[^(20)&(30)D]/(55)[^(18)&(35)D]/(61)[^(17)&(31)D]\n\
                      c2: 0.35 : C -> [^(18)&(32)D]/(52)[^(15)&(33)D]/(57)[^(21)&(29)D]/(64)[^(18)&(34)D]/(49)[^(16)&(30)D]/(60)[^(19)&(33)D]\n\
                      c3: 0.3 : C -> [^(17)&(31)D]/(46)[^(20)&(35)D]/(55)[^(15)&(29)D]/(50)[^(18)&(33)D]/(52)[^(21)&(30)D]/(48)[^(17)&(34)D]/(54)[^(19)&(31)D]\n\
                      p2: D -> !(0.06)F(s*0.95)&(8)F(s*0.9)P&(12)F(s*0.9)P&(18)F(s*0.85)P&(30)F(s*0.75)P&(44)F(s*0.6)P&(60)F(s*0.45)P&(26)F(s*0.35)P"
            .to_string(),
        // Organ expression lives here, not in the growth rules (#917): the
        // grammar emits abstract leaflet-site markers `P` (and unexpanded
        // rachis tips `D` on the youngest frond) and this pass decides what
        // they BECOME — so the same palm skeleton can be re-skinned (frond
        // texture, dead/brown fronds, fruiting) without touching its
        // morphogenesis. It also fixes a silent gap: leaflet sites created
        // on the final growth step used to render nothing, because the
        // in-grammar `P` rule needed one more derivation step to fire.
        finalization_code: "P -> ,(1)[+(55)~(0,36)][-(55)~(0,36)]\n\
             D -> ,(1)[+(55)~(0,30)][-(55)~(0,30)]"
            .to_string(),
        iterations: 7,
        seed: 1,
        angle: Fp(60.0),
        step: Fp(1.0),
        width: Fp(0.26),
        elasticity: Fp(0.28),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
        materials,
        prop_mappings,
        prop_scale: Fp(0.055),
        mesh_resolution: 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = Palm.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
