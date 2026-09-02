//! Ship's lantern — the fx hero (#1090): the first wearable that carries
//! the catalogue's whole richness toolkit on a body. An emissive hold (the
//! glass housing and the flame core), a small ember emitter, and a spatial
//! creak patch, all riding the left hand through every gait — the emitter
//! simulates in world space, so a carried lantern sheds a faint trail of
//! embers behind a walking body for free.
//!
//! Structured around the oversized-draft technique's one hard boundary:
//! **fx stay OUT of the draft.** Particle emission velocity and shape
//! sampling go through the emitter's global affine — a 0.1-scaled ancestor
//! would shrink them — while particle *sizes* are world units and would
//! not shrink, so an emitter inside a scaled draft tears its own
//! parameters apart. The root is therefore the carry ring at TRUE size
//! (its 10 mm rod is honest wrought iron, comfortably on the sanitiser
//! floor), the 10× cage draft hangs under it as one uniformly-scaled
//! child, and the emitter and the audio live beside the draft in the
//! root's true-scale frame.

use crate::catalogue::items::fx::{Emitter, node, patch};
use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, id_quat, nest, prim, prim_scaled, quat_x, solid, sphere,
    torus, uv_for_scale,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole, ThemeArchetype};
use crate::pds::{
    EmitterShape, Fp, Fp3, Generator, ParticleBlendMode, SovereignMaterialSettings,
    SovereignSparkConfig, SovereignTextureConfig,
};
use bevy_symbios_audio::{
    BiquadBandpass, Connection, Gain, GraphNode, Lfo, LfoShape, NodeId, NodeKind, WhiteNoise,
};

use super::{aged_iron, brass};

/// The size the cage is DRAWN at relative to the size it is carried at —
/// the [`super::circlet`] technique. A lantern's bars are 9 mm iron; drawn
/// at 90 mm they clear the sanitiser's 10 mm prim-local floor with room to
/// spare.
const DRAFT: f32 = 10.0;

/// One deterministic seed for the ember sprite bakes: `build` must return
/// the same tree every call (the sanitize round-trip test compares trees),
/// so nothing here may roll.
const EMBER_SEED: u64 = 0x1A57_F1A3;

/// Warm lamplight, shared by the glass, the flame and the embers so the
/// whole object reads as lit by one fire.
const LAMPLIGHT: [f32; 3] = [1.0, 0.62, 0.26];

/// The glass housing: oiled horn-glass around a flame — most of the glow.
fn glow_glass() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.38, 0.25, 0.13]),
        emission_color: Fp3(LAMPLIGHT),
        emission_strength: Fp(1.3),
        roughness: Fp(0.32),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        ..Default::default()
    }
}

/// The flame core, hot enough to read as the source inside the glass.
fn flame_core() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([1.0, 0.85, 0.55]),
        emission_color: Fp3([1.0, 0.75, 0.35]),
        emission_strength: Fp(3.0),
        roughness: Fp(0.5),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        ..Default::default()
    }
}

pub struct Lantern;

impl CatalogueEntry for Lantern {
    fn slug(&self) -> &'static str {
        // Not "lantern": the civic kit's street lantern owns that slug, and
        // `slugs_are_unique` holds the registry to one owner per name.
        "ships_lantern"
    }
    fn name(&self) -> &'static str {
        "Ship's Lantern"
    }
    fn description(&self) -> &'static str {
        "An iron-caged oil lamp that carries its own warm light, drifting embers and a slow creak."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Attachment
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Pirate, ThemeArchetype::Medieval]
    }
    fn wear_socket(&self) -> Option<symbios_avatar::Socket> {
        Some(symbios_avatar::Socket::LeftHand)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Root = a small axis-aligned brass collar at the attach origin (the
/// engine seats the origin just outside the palm; the lantern hangs below
/// it, exactly as the satchel hangs below its hip origin). The collar is
/// the root deliberately: children inherit the root transform whole, so a
/// rotated root — the carry ring's quarter-turn — would lay the entire
/// lantern on its side (the flag write-up's leaning-mast trap, and this
/// file hit it: the first build used the ring as root and rendered
/// horizontal). The ring is a rotated CHILD instead; below hangs one
/// 10×-draft cage under a single uniform downscale; beside the draft,
/// the true-scale fx.
fn build_tree() -> Generator {
    // The cage flies a shade under the plain 1/DRAFT: at full size the
    // housing grazed visibly into the thigh at rest (the hand hangs beside
    // the leg, and the engine seat's palm margin cannot know how far a
    // prop hangs). 15% off keeps it a ship's lantern and clears the worst
    // of the embed; the ring and collar stay true-sized — a hand is a
    // hand.
    let scale = 0.85 / DRAFT;

    // Root: brass collar the ring is shackled to, centred on the origin.
    let collar = prim(
        solid(cylinder_tapered(0.014, 0.030, 10, 0.0, brass())),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Carry ring above the collar, stood into the X–Y plane — the same
    // quarter-turn the satchel's belt loop takes. Its lower rod sinks
    // into the collar's top.
    let ring = prim(
        solid(torus(0.010, 0.032, aged_iron())),
        [0.0, 0.030, 0.0],
        quat_x(std::f32::consts::FRAC_PI_2),
    );

    // --- The cage, drawn at 10× ------------------------------------------
    // The sub-root prim is the stem, centred on the DRAFT origin; every
    // other part is authored in that same draft-local frame and pushed
    // directly as its child. NOT `nest`: nest rebases by the parent's own
    // translation, which is in the root's TRUE frame — mixing it with
    // draft-frame children is exactly the cross-frame arithmetic the
    // draft technique exists to avoid.
    let iron = || uv_for_scale(aged_iron(), scale);
    let mut cage = prim_scaled(
        // Stem from the collar down to the cap: spans ±0.15 about the
        // draft origin.
        solid(cylinder_tapered(0.12, 0.30, 10, 0.0, iron())),
        // True frame: stem top (+0.015 true) sinks into the collar.
        [0.0, -0.025, 0.0],
        id_quat(),
        [scale; 3],
    );
    let mut cage_parts = vec![
        // Cap: a shallow tapered drum, wider than the glass diagonal.
        prim(
            solid(cylinder_tapered(
                0.62,
                0.22,
                14,
                0.35,
                uv_for_scale(brass(), scale),
            )),
            [0.0, -0.27, 0.0],
            id_quat(),
        ),
        // Glass housing: one warm block just inside the corner posts.
        prim(
            solid(cuboid_tapered(
                [0.82, 1.10, 0.82],
                0.0,
                uv_for_scale(glow_glass(), scale),
            )),
            [0.0, -1.0, 0.0],
            id_quat(),
        ),
        // Flame core, low in the housing where a wick burns.
        prim(
            solid(sphere(0.15, 2, uv_for_scale(flame_core(), scale))),
            [0.0, -1.30, 0.0],
            id_quat(),
        ),
        // Base pan.
        prim(
            solid(cylinder_tapered(0.60, 0.16, 14, 0.0, iron())),
            [0.0, -1.68, 0.0],
            id_quat(),
        ),
        // Bottom finial knob.
        prim(
            solid(sphere(0.10, 2, uv_for_scale(brass(), scale))),
            [0.0, -1.79, 0.0],
            id_quat(),
        ),
    ];
    // Four corner posts, sunk into the cap above and the base below.
    for (sx, sz) in [(-1.0f32, -1.0f32), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
        cage_parts.push(prim(
            solid(cuboid_tapered([0.09, 1.35, 0.09], 0.0, iron())),
            [sx * 0.45, -1.0, sz * 0.45],
            id_quat(),
        ));
    }
    cage.children.extend(cage_parts);

    // --- The fx, at true scale, beside the draft --------------------------
    // Embers leak from under the cap and drift up and out; world-space
    // simulation means they trail a carried lantern.
    let embers = Emitter {
        shape: EmitterShape::Sphere { radius: Fp(0.03) },
        rate: 2.5,
        burst: 0,
        max: 12,
        life: (0.5, 1.2),
        speed: (0.08, 0.25),
        gravity: -0.05,
        accel: [0.0, 0.15, 0.0],
        drag: 0.4,
        size: (0.018, 0.0),
        start_color: [LAMPLIGHT[0], LAMPLIGHT[1], LAMPLIGHT[2], 1.0],
        end_color: [0.85, 0.2, 0.05, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::Spark(SovereignSparkConfig {
            seed: (EMBER_SEED ^ 0x00E3_BE85) as u32,
            points: 4,
            color_core: Fp3([1.0, 0.9, 0.6]),
            color_tip: Fp3([1.0, 0.45, 0.1]),
            ..Default::default()
        }),
    }
    .at([0.0, -0.08, 0.0], EMBER_SEED);

    let mut root = nest(collar, vec![ring, cage, embers]);
    root.audio = lantern_creak();
    root
}

/// A slow iron creak, spatial at the ring: filtered noise squeaking
/// through a high-Q bandpass, gated by a slow LFO so it is quiet most of
/// each swing and speaks briefly at the turn — the cadence of a carried
/// lamp rather than a machine hum. Patterned on the cyberpunk kit's
/// gated patches (`electric_crackle`).
fn lantern_creak() -> crate::pds::SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.5 }));
    let lfo = node(
        1,
        NodeKind::Lfo(Lfo {
            rate_hz: 1.1,
            shape: LfoShape::Sine,
            depth: 0.8,
            offset: 0.12,
        }),
    );
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(2),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 760.0,
            q: 5.0,
        }),
        inputs: bp_in,
    };
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(2))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(1))]);
    let vca = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    patch(vec![noise, lfo, bp, vca], NodeId(3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::pds::PrimCommon;
    use crate::pds::{GeneratorKind, SovereignAudioConfig};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Lantern.build(""), "lantern");
    }

    #[test]
    fn lantern_is_wearable_at_the_left_hand() {
        assert_eq!(
            Lantern.wear_socket(),
            Some(symbios_avatar::Socket::LeftHand)
        );
        assert_eq!(Lantern.role(), StructureRole::Attachment);
        assert_eq!(
            Lantern.wear_fit(),
            None,
            "a lantern is not fitted to a body"
        );
    }

    /// The fx ARE the item (#1090): a lantern that lost its emitter or its
    /// creak in a refactor is a cage on a ring, and nothing else in the
    /// suite would say so.
    #[test]
    fn the_lantern_carries_its_emitter_its_audio_and_its_glow() {
        let tree = Lantern.build("");
        assert!(
            !matches!(tree.audio, SovereignAudioConfig::None),
            "the creak patch is gone from the root"
        );

        fn walk(node: &Generator, emitters: &mut usize, glowing: &mut usize) {
            match &node.kind {
                GeneratorKind::ParticleSystem(_) => *emitters += 1,
                kind => {
                    if material_of(kind).is_some_and(|m| m.emission_strength.0 > 0.5) {
                        *glowing += 1;
                    }
                }
            }
            for child in &node.children {
                walk(child, emitters, glowing);
            }
        }
        fn material_of(kind: &GeneratorKind) -> Option<&crate::pds::SovereignMaterialSettings> {
            match kind {
                GeneratorKind::Cuboid {
                    common: PrimCommon { material, .. },
                    ..
                }
                | GeneratorKind::Sphere {
                    common: PrimCommon { material, .. },
                    ..
                }
                | GeneratorKind::Cylinder {
                    common: PrimCommon { material, .. },
                    ..
                }
                | GeneratorKind::Torus {
                    common: PrimCommon { material, .. },
                    ..
                } => Some(material),
                _ => None,
            }
        }
        let (mut emitters, mut glowing) = (0, 0);
        walk(&tree, &mut emitters, &mut glowing);
        assert_eq!(emitters, 1, "exactly one ember emitter");
        assert!(
            glowing >= 2,
            "the glass housing and the flame core both glow; found {glowing}"
        );
    }

    /// The draft-boundary rule this file's header states: the emitter must
    /// NOT sit under the scaled cage sub-root, where the affine would
    /// shrink its velocities while its particle sizes stayed world-sized.
    #[test]
    fn the_emitter_lives_outside_the_scaled_draft() {
        let tree = Lantern.build("");
        fn find_emitter_scales(node: &Generator, chain: &mut Vec<f32>, found: &mut Vec<Vec<f32>>) {
            chain.push(node.transform.scale.0[0]);
            if matches!(node.kind, GeneratorKind::ParticleSystem(_)) {
                found.push(chain.clone());
            }
            for child in &node.children {
                find_emitter_scales(child, chain, found);
            }
            chain.pop();
        }
        let mut found = Vec::new();
        find_emitter_scales(&tree, &mut Vec::new(), &mut found);
        assert_eq!(found.len(), 1);
        assert!(
            found[0].iter().all(|&s| (s - 1.0).abs() < 1e-6),
            "the ember emitter sits under a scaled ancestor ({:?}) — its \
             emission would shrink while its particle sizes did not",
            found[0]
        );
    }
}
