//! Escalation-driven geometric damage — the "Ruins" modifier.
//!
//! After a settlement member is built and material-finished (see
//! [`material_finish`](crate::pds::material_finish)), [`apply_ruin`] leans,
//! settles, shrinks and partially collapses the structure by the room's
//! escalation tier, so a fought-over settlement reads as battered and
//! ruined while a peaceful one stands untouched:
//!
//! - [`EscalationTier::Calm`] — no-op.
//! - [`EscalationTier::Tense`] — light wear: a slight lean, a small settle
//!   into the ground, a touch smaller.
//! - [`EscalationTier::Conflict`] — heavy ruin: a pronounced topple, a
//!   deeper sink, a fraction of the top-level parts collapsed away (with
//!   the survivors knocked askew), and a little rubble scattered at the
//!   base.
//!
//! The lean is applied to the member's *root* transform, which the world
//! compiler composes with the placement pose (`cell_tf * generator.transform`,
//! see `world_builder::compile::dispatch`), so it tilts the whole structure
//! in place. The pass is deterministic in the member's `grammar_seed`, so
//! peers deriving the same room produce bit-identical ruins.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use super::generator::{Generator, GeneratorKind};
use super::texture::SovereignMaterialSettings;
use super::types::{Fp, Fp3, Fp4, TransformData};
use crate::seeded_defaults::{EscalationTier, range_f32, signed_unit_f32, unit_f32};

/// Sub-stream salt so the ruin RNG is decorrelated from the member's
/// Shape-grammar seed (which reuses `grammar_seed` directly).
const RUIN_SALT: u64 = 0x5275_1A2D_0BAD_F00D;

/// Per-tier damage envelope: `(max lean rad, max sink m, min scale, child
/// collapse probability)`.
fn envelope(tier: EscalationTier) -> Option<(f32, f32, f32, f32)> {
    match tier {
        EscalationTier::Calm => None,
        EscalationTier::Tense => Some((0.05, 0.10, 0.97, 0.0)),
        EscalationTier::Conflict => Some((0.22, 0.45, 0.85, 0.35)),
    }
}

/// Lean, settle and (at conflict) collapse a built settlement member by the
/// room's `escalation` (`[0, 1]`), deterministically in `seed`. A calm room
/// leaves the member untouched.
pub fn apply_ruin(node: &mut Generator, escalation: f32, seed: u64) {
    let Some((max_lean, max_sink, min_scale, collapse_p)) =
        envelope(EscalationTier::from_unit(escalation))
    else {
        return;
    };
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ RUIN_SALT);

    // Whole-structure lean about a random horizontal axis.
    let lean = range_f32(&mut rng, 0.4, 1.0) * max_lean;
    let (ax, az) = (signed_unit_f32(&mut rng), signed_unit_f32(&mut rng));
    lean_node(node, lean, ax, az);

    // Settle into the ground and shrink a touch.
    node.transform.translation.0[1] -= range_f32(&mut rng, max_sink * 0.5, max_sink);
    let scale = range_f32(&mut rng, min_scale, 1.0);
    for s in &mut node.transform.scale.0 {
        *s *= scale;
    }

    // Heavy ruin: collapse a fraction of the top-level parts, knock the
    // survivors askew, and drop a little rubble at the base.
    if collapse_p > 0.0 {
        collapse_children(node, collapse_p, max_lean, &mut rng);
        scatter_rubble(node, &mut rng);
    }
}

/// Compose a lean about the horizontal axis `(ax, _, az)` onto a node's
/// existing rotation. A degenerate (zero-length) axis is skipped.
fn lean_node(node: &mut Generator, angle: f32, ax: f32, az: f32) {
    let len = (ax * ax + az * az).sqrt();
    if len < 1e-5 {
        return;
    }
    let half = angle * 0.5;
    let (s, c) = (half.sin(), half.cos());
    let tilt = [ax / len * s, 0.0, az / len * s, c];
    node.transform.rotation = Fp4(quat_mul(tilt, node.transform.rotation.0));
}

/// Drop each top-level child with probability `collapse_p` (always keeping
/// the first, so nothing fully vanishes) and lean the survivors. Nodes
/// without children — Shape / L-system grammars whose geometry is internal
/// — are left as-is.
fn collapse_children(node: &mut Generator, collapse_p: f32, max_lean: f32, rng: &mut ChaCha8Rng) {
    if node.children.is_empty() {
        return;
    }
    let original = std::mem::take(&mut node.children);
    let mut survivors = Vec::with_capacity(original.len());
    for (i, mut child) in original.into_iter().enumerate() {
        if i > 0 && unit_f32(rng) < collapse_p {
            continue; // this part has been destroyed
        }
        let a = signed_unit_f32(rng) * max_lean;
        let b = signed_unit_f32(rng) * max_lean;
        lean_node(&mut child, (a * a + b * b).sqrt(), a, b);
        child.transform.translation.0[1] -= range_f32(rng, 0.0, 0.2);
        survivors.push(child);
    }
    node.children = survivors;
}

/// Dark, rough rubble — broken masonry colour.
fn rubble_material() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.30, 0.29, 0.27]),
        roughness: Fp(0.95),
        ..SovereignMaterialSettings::default()
    }
}

/// Append a few small rubble blocks around the structure's base.
fn scatter_rubble(node: &mut Generator, rng: &mut ChaCha8Rng) {
    let count = 2 + (unit_f32(rng) * 3.0) as usize; // 2..=4
    for _ in 0..count {
        let sx = range_f32(rng, 0.15, 0.45);
        let sy = range_f32(rng, 0.12, 0.3);
        let sz = range_f32(rng, 0.15, 0.45);
        let angle = unit_f32(rng) * std::f32::consts::TAU;
        let dist = range_f32(rng, 0.4, 1.4);
        node.children.push(Generator {
            kind: GeneratorKind::Cuboid {
                size: Fp3([sx, sy, sz]),
                solid: true,
                material: rubble_material(),
                twist: Fp(0.0),
                taper: Fp(range_f32(rng, 0.0, 0.3)),
                bend: Fp3([0.0, 0.0, 0.0]),
            },
            transform: TransformData {
                translation: Fp3([angle.sin() * dist, sy * 0.5, angle.cos() * dist]),
                rotation: Fp4([0.0, (angle * 0.5).sin(), 0.0, (angle * 0.5).cos()]),
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            children: Vec::new(),
            audio: super::audio::SovereignAudioConfig::None,
        });
    }
}

/// Hamilton product `a * b` of two `[x, y, z, w]` quaternions.
fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn structure() -> Generator {
        // A root with three child parts — stands in for a primitive-built
        // catalogue member.
        let part = |y: f32| Generator {
            kind: GeneratorKind::Cuboid {
                size: Fp3([1.0, 1.0, 1.0]),
                solid: true,
                material: SovereignMaterialSettings::default(),
                twist: Fp(0.0),
                taper: Fp(0.0),
                bend: Fp3([0.0, 0.0, 0.0]),
            },
            transform: TransformData {
                translation: Fp3([0.0, y, 0.0]),
                rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            children: Vec::new(),
            audio: super::super::audio::SovereignAudioConfig::None,
        };
        let mut root = part(0.5);
        root.children = vec![part(1.5), part(2.5), part(3.5)];
        root
    }

    fn is_identity_rot(node: &Generator) -> bool {
        let r = node.transform.rotation.0;
        (r[0]).abs() < 1e-6
            && (r[1]).abs() < 1e-6
            && (r[2]).abs() < 1e-6
            && (r[3] - 1.0).abs() < 1e-6
    }

    #[test]
    fn calm_room_is_untouched() {
        let before = structure();
        let mut after = before.clone();
        apply_ruin(&mut after, 0.0, 42);
        assert_eq!(before, after, "a calm room must not damage its members");
    }

    #[test]
    fn conflict_tilts_and_sinks_the_member() {
        let before = structure();
        let mut after = before.clone();
        apply_ruin(&mut after, 0.95, 42);
        assert!(
            !is_identity_rot(&after),
            "conflict should lean the structure"
        );
        assert!(
            after.transform.translation.0[1] < before.transform.translation.0[1],
            "conflict should settle the structure downward"
        );
        assert!(
            after.transform.scale.0[1] < before.transform.scale.0[1],
            "conflict should shrink the structure"
        );
    }

    #[test]
    fn tense_wear_is_lighter_than_conflict() {
        let mut tense = structure();
        apply_ruin(&mut tense, 0.5, 7); // Tense
        let mut conflict = structure();
        apply_ruin(&mut conflict, 0.95, 7); // Conflict
        // Conflict sinks the member further than light wear.
        let base = structure().transform.translation.0[1];
        let tense_sink = base - tense.transform.translation.0[1];
        let conflict_sink = base - conflict.transform.translation.0[1];
        assert!(
            conflict_sink > tense_sink,
            "conflict ({conflict_sink}) should sink more than tense ({tense_sink})"
        );
        // Tense keeps every part (no collapse); conflict can drop some and
        // adds rubble — in either case it never fully empties the member.
        assert_eq!(tense.children.len(), 3, "tense keeps all parts");
        assert!(!conflict.children.is_empty());
    }

    #[test]
    fn deterministic_in_seed() {
        let mut a = structure();
        let mut b = structure();
        apply_ruin(&mut a, 0.95, 123);
        apply_ruin(&mut b, 0.95, 123);
        assert_eq!(a, b);
    }

    #[test]
    fn rotation_stays_unit_length() {
        let mut node = structure();
        apply_ruin(&mut node, 0.95, 99);
        let r = node.transform.rotation.0;
        let mag = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2] + r[3] * r[3]).sqrt();
        assert!((mag - 1.0).abs() < 1e-4, "lean must keep a unit quaternion");
    }
}
