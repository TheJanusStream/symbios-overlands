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
//! Collapse is **support-aware** (#776): parts carry a coarse conservative
//! bounding box and removal proceeds top-down — a part that still holds a
//! standing part above it cannot be destroyed, and a sweep afterwards fells
//! anything left without a contact chain to the ground (a lamp whose roof
//! is gone). A collapsed part either vanishes or topples to the ground as
//! intact debris, so ruin never leaves geometry hanging in the air.
//!
//! The lean is applied to the member's *root* transform, which the world
//! compiler composes with the placement pose (`cell_tf * generator.transform`,
//! see `world_builder::compile::dispatch`), so it tilts the whole structure
//! in place. The pass is deterministic in the member's `grammar_seed`, so
//! peers deriving the same room produce bit-identical ruins.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use super::generator::{Generator, GeneratorKind, TortureParams};
use super::material_finish::node_materials_mut;
use super::texture::{SovereignConcreteConfig, SovereignMaterialSettings, SovereignTextureConfig};
use super::types::{Fp, Fp2, Fp3, Fp4, Fp64, TransformData};
use crate::seeded_defaults::{EscalationTier, range_f32, signed_unit_f32, unit_f32};

/// Sub-stream salt so the ruin RNG is decorrelated from the member's
/// Shape-grammar seed (which reuses `grammar_seed` directly).
const RUIN_SALT: u64 = 0x5275_1A2D_0BAD_F00D;

/// Contact tolerance (m) of the coarse support model: two parts whose
/// conservative boxes come within this of touching count as in contact.
/// Generous enough to bridge authored seams and foundation sinks.
const SUPPORT_TOL: f32 = 0.35;
/// A part whose base is within this of the structure's lowest point sits on
/// the ground and needs no other support.
const GROUND_TOL: f32 = 0.35;
/// Fraction of collapsed parts that topple to the ground as intact debris
/// rather than vanish outright.
const FELL_FRACTION: f32 = 0.5;
/// Askew multiplier for survivors that still hold a standing part above
/// them — a wall under an intact roof barely leans, so their contact isn't
/// visually broken; free-standing survivors take the full stagger.
const SUPPORTER_ASKEW: f32 = 0.25;

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
    // survivors askew, drop rubble at the base, and short out some of the
    // emissive trim (dead / guttering neon).
    if collapse_p > 0.0 {
        collapse_children(node, collapse_p, max_lean, &mut rng);
        scatter_rubble(node, &mut rng);
        break_emissives(node, 0.5, &mut rng);
    }
}

/// Snuff or gutter the emissive on a fraction of materials — the dead and
/// flickering neon of a fought-over settlement. Cross-theme safe: materials
/// with no emission (most non-cyberpunk surfaces) are left untouched, so
/// this is a no-op anywhere there's nothing to break.
fn break_emissives(node: &mut Generator, prob: f32, rng: &mut ChaCha8Rng) {
    for mat in node_materials_mut(&mut node.kind) {
        if mat.emission_strength.0 > 0.0 && unit_f32(rng) < prob {
            // Half die outright; half gutter at a dim flicker.
            mat.emission_strength = Fp(if unit_f32(rng) < 0.5 {
                0.0
            } else {
                mat.emission_strength.0 * 0.15
            });
        }
    }
    for child in &mut node.children {
        break_emissives(child, prob, rng);
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

// ---------------------------------------------------------------------------
// Coarse support model
// ---------------------------------------------------------------------------

/// Conservative axis-aligned bounds of one part, in its parent's frame.
#[derive(Clone, Copy, Debug)]
struct Bounds {
    min: [f32; 3],
    max: [f32; 3],
}

impl Bounds {
    fn union(a: Bounds, b: Bounds) -> Bounds {
        let mut out = a;
        for k in 0..3 {
            out.min[k] = a.min[k].min(b.min[k]);
            out.max[k] = a.max[k].max(b.max[k]);
        }
        out
    }

    fn centre(&self) -> [f32; 3] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }

    fn half(&self) -> [f32; 3] {
        [
            (self.max[0] - self.min[0]) * 0.5,
            (self.max[1] - self.min[1]) * 0.5,
            (self.max[2] - self.min[2]) * 0.5,
        ]
    }
}

/// Coarse bounds of a kind's own geometry in its local frame, or `None` for
/// kinds with no support-relevant volume (particles, water, portals, …).
/// Prim meshes are origin-centred (Bevy primitive convention), so most arms
/// are symmetric boxes; Lathe / Spine / BlobGroup carry explicit local
/// coordinates and use them. Torture / cuts are ignored — staying a little
/// too big is the conservative direction for a support test.
fn kind_bounds(kind: &GeneratorKind) -> Option<Bounds> {
    use GeneratorKind as K;
    let sym = |h: [f32; 3]| {
        Some(Bounds {
            min: [-h[0], -h[1], -h[2]],
            max: h,
        })
    };
    match kind {
        K::Cuboid { size, .. } | K::Wedge { size, .. } | K::Bevel { size, .. } => {
            sym([size.0[0] * 0.5, size.0[1] * 0.5, size.0[2] * 0.5])
        }
        K::Sphere { radius, .. } => sym([radius.0; 3]),
        K::Cylinder { radius, height, .. }
        | K::Cone { radius, height, .. }
        | K::Tube { radius, height, .. } => sym([radius.0, height.0 * 0.5, radius.0]),
        K::Capsule { radius, length, .. } => sym([radius.0, length.0 * 0.5 + radius.0, radius.0]),
        K::Torus {
            minor_radius,
            major_radius,
            ..
        } => {
            let r = major_radius.0 + minor_radius.0;
            sym([r, minor_radius.0, r])
        }
        K::Plane { size, .. } | K::Sign { size, .. } => {
            sym([size.0[0] * 0.5, 0.02, size.0[1] * 0.5])
        }
        K::Tetrahedron { size, .. } => sym([size.0; 3]),
        K::Superellipsoid { half_extents, .. } => sym(half_extents.0),
        K::Helix {
            radius,
            tube_radius,
            pitch,
            turns,
            ..
        } => {
            let xz = radius.0 + tube_radius.0;
            sym([xz, pitch.0.abs() * turns.0 * 0.5 + tube_radius.0, xz])
        }
        K::Spine { points, .. } => points.iter().fold(None, |acc, p| {
            let r = p.radius.0.max(0.0);
            let e = Bounds {
                min: [
                    p.position.0[0] - r,
                    p.position.0[1] - r,
                    p.position.0[2] - r,
                ],
                max: [
                    p.position.0[0] + r,
                    p.position.0[1] + r,
                    p.position.0[2] + r,
                ],
            };
            Some(acc.map_or(e, |a| Bounds::union(a, e)))
        }),
        K::Lathe { points, .. } => {
            let r = points.iter().map(|p| p.radius.0.abs()).fold(0.0, f32::max);
            let (lo, hi) = points
                .iter()
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| {
                    (lo.min(p.height.0), hi.max(p.height.0))
                });
            (r > 0.0 && lo.is_finite()).then_some(Bounds {
                min: [-r, lo, -r],
                max: [r, hi, r],
            })
        }
        K::BlobGroup { elements, .. } => {
            elements
                .iter()
                .filter(|e| !e.subtract)
                .fold(None, |acc, e| {
                    let [rx, ry, rz] = e.radii.0;
                    // Rotation-safe bounding radius per element.
                    let r = (rx * rx + ry * ry + rz * rz).sqrt();
                    let b = Bounds {
                        min: [
                            e.position.0[0] - r,
                            e.position.0[1] - r,
                            e.position.0[2] - r,
                        ],
                        max: [
                            e.position.0[0] + r,
                            e.position.0[1] + r,
                            e.position.0[2] + r,
                        ],
                    };
                    Some(acc.map_or(b, |a| Bounds::union(a, b)))
                })
        }
        // Grammar geometry is internal; a coarse box keeps such parts
        // participating in the support model. Shape grammars extrude
        // upward from their base scope at the origin.
        K::Shape { footprint, .. } => Some(Bounds {
            min: [-footprint.0[0] * 0.5, 0.0, -footprint.0[2] * 0.5],
            max: [
                footprint.0[0] * 0.5,
                footprint.0[1].max(2.0),
                footprint.0[2] * 0.5,
            ],
        }),
        K::LSystem { .. } => Some(Bounds {
            min: [-1.0, 0.0, -1.0],
            max: [1.0, 2.0, 1.0],
        }),
        K::Gateway { size } => sym([size.0[0] * 0.5, size.0[1] * 0.5, size.0[2] * 0.5]),
        _ => None,
    }
}

/// `b` mapped through `t` (scale → rotate → translate), staying conservative:
/// the rotated box is re-boxed with the |R|·h absolute-matrix bound.
fn transformed(b: Bounds, t: &TransformData) -> Bounds {
    let s = t.scale.0;
    let c0 = b.centre();
    let h0 = b.half();
    let c = [c0[0] * s[0], c0[1] * s[1], c0[2] * s[2]];
    let h = [h0[0] * s[0].abs(), h0[1] * s[1].abs(), h0[2] * s[2].abs()];

    let [x, y, z, w] = t.rotation.0;
    let r = [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
        ],
        [
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ];
    let mut rc = [0.0f32; 3];
    let mut rh = [0.0f32; 3];
    for i in 0..3 {
        for k in 0..3 {
            rc[i] += r[i][k] * c[k];
            rh[i] += r[i][k].abs() * h[k];
        }
    }

    let tr = t.translation.0;
    Bounds {
        min: [
            rc[0] + tr[0] - rh[0],
            rc[1] + tr[1] - rh[1],
            rc[2] + tr[2] - rh[2],
        ],
        max: [
            rc[0] + tr[0] + rh[0],
            rc[1] + tr[1] + rh[1],
            rc[2] + tr[2] + rh[2],
        ],
    }
}

/// Subtree bounds in the node's own frame (its kind plus all descendants,
/// before the node's own transform), or `None` if nothing has volume.
fn subtree_bounds(node: &Generator) -> Option<Bounds> {
    let mut b = kind_bounds(&node.kind);
    for child in &node.children {
        if let Some(cb) = subtree_bounds(child) {
            let cb = transformed(cb, &child.transform);
            b = Some(b.map_or(cb, |a| Bounds::union(a, cb)));
        }
    }
    b
}

/// A top-level part's bounds in the structure root's frame. A part with no
/// derivable volume gets a small default box at its translation so it still
/// takes part in the support model.
fn part_bounds(child: &Generator) -> Bounds {
    let local = subtree_bounds(child).unwrap_or(Bounds {
        min: [-0.3; 3],
        max: [0.3; 3],
    });
    transformed(local, &child.transform)
}

/// Strict overlap of the two boxes' ground-plane footprints.
fn xz_overlap(a: &Bounds, b: &Bounds) -> bool {
    a.min[0] < b.max[0] && b.min[0] < a.max[0] && a.min[2] < b.max[2] && b.min[2] < a.max[2]
}

/// Whether the two boxes touch (within [`SUPPORT_TOL`]) on all three axes —
/// the contact edge of the support graph.
fn in_contact(a: &Bounds, b: &Bounds) -> bool {
    (0..3).all(|k| a.min[k] <= b.max[k] + SUPPORT_TOL && b.min[k] <= a.max[k] + SUPPORT_TOL)
}

/// Whether `upper` rests on `lower`: footprints overlap, `upper` is based
/// higher, and its base sits at (or interpenetrates down to) `lower`'s span.
fn rests_above(upper: &Bounds, lower: &Bounds) -> bool {
    xz_overlap(upper, lower)
        && upper.min[1] >= lower.min[1] + 0.05
        && upper.min[1] <= lower.max[1] + SUPPORT_TOL
}

/// What the collapse decided for one top-level part.
#[derive(Clone, Copy, PartialEq)]
enum Fate {
    /// Still standing (gets the askew knock).
    Stands,
    /// Toppled to the ground as intact debris.
    Felled,
    /// Destroyed outright.
    Gone,
}

/// Collapse a fraction of the top-level parts without leaving anything
/// hanging in the air. Two passes over a coarse support model (#776):
///
/// 1. **Top-down removal** — parts are visited highest-top first and roll
///    `collapse_p`; a part that still holds a standing part above it is
///    skipped, so roofs go before the walls beneath them. The part with the
///    lowest base is the anchor and never collapses, so nothing fully
///    vanishes.
/// 2. **Ground-connectivity sweep** — any standing part left without a
///    contact chain to the ground (its base near the structure's lowest
///    point, the root's own prim, or a supported neighbour) collapses too:
///    the lamp whose roof was destroyed falls with it.
///
/// A collapsed part vanishes or is felled ([`FELL_FRACTION`]) — toppled hard
/// and dropped to rest at the base as debris. Survivors are knocked askew,
/// scaled down by [`SUPPORTER_ASKEW`] when they still hold something up.
/// Nodes without children — grammars whose geometry is internal — are
/// untouched.
fn collapse_children(node: &mut Generator, collapse_p: f32, max_lean: f32, rng: &mut ChaCha8Rng) {
    let n = node.children.len();
    if n == 0 {
        return;
    }
    let bounds: Vec<Bounds> = node.children.iter().map(part_bounds).collect();
    // The root's own prim (typically the foundation slab) is an
    // always-standing support.
    let root_prim = kind_bounds(&node.kind);
    let base_y = bounds
        .iter()
        .chain(root_prim.iter())
        .map(|b| b.min[1])
        .fold(f32::INFINITY, f32::min);
    let anchor = (0..n)
        .min_by(|&a, &b| {
            bounds[a].min[1]
                .total_cmp(&bounds[b].min[1])
                .then(a.cmp(&b))
        })
        .expect("n > 0");

    let mut fate = vec![Fate::Stands; n];

    // Pass 1: top-down removal picks. Both rolls are drawn for every part
    // regardless of eligibility so the stream stays easy to reason about.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        bounds[b].max[1]
            .total_cmp(&bounds[a].max[1])
            .then(a.cmp(&b))
    });
    for &i in &order {
        let collapse_roll = unit_f32(rng);
        let fell_roll = unit_f32(rng);
        if i == anchor || collapse_roll >= collapse_p {
            continue;
        }
        let holds_something = (0..n)
            .any(|j| j != i && fate[j] == Fate::Stands && rests_above(&bounds[j], &bounds[i]));
        if holds_something {
            continue;
        }
        fate[i] = if fell_roll < FELL_FRACTION {
            Fate::Felled
        } else {
            Fate::Gone
        };
    }

    // Pass 2: fell every standing part with no contact chain to the ground.
    // Support only propagates through standing supported parts, so one
    // fixpoint suffices — a chain through a collapsed part never proves
    // anything.
    let standing = |fate: &[Fate], i: usize| fate[i] == Fate::Stands;
    let mut supported = vec![false; n];
    for i in 0..n {
        supported[i] = i == anchor
            || (standing(&fate, i)
                && (bounds[i].min[1] <= base_y + GROUND_TOL
                    || root_prim.is_some_and(|r| in_contact(&bounds[i], &r))));
    }
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..n {
            if standing(&fate, i)
                && !supported[i]
                && (0..n).any(|j| {
                    j != i
                        && standing(&fate, j)
                        && supported[j]
                        && in_contact(&bounds[i], &bounds[j])
                })
            {
                supported[i] = true;
                changed = true;
            }
        }
    }
    for i in 0..n {
        if standing(&fate, i) && !supported[i] {
            fate[i] = if unit_f32(rng) < FELL_FRACTION {
                Fate::Felled
            } else {
                Fate::Gone
            };
        }
    }

    // Apply the fates: drop, fell, or knock askew.
    let original = std::mem::take(&mut node.children);
    let mut survivors = Vec::with_capacity(n);
    for (i, mut child) in original.into_iter().enumerate() {
        match fate[i] {
            Fate::Gone => {}
            Fate::Felled => {
                fell_part(&mut child, &bounds[i], base_y, rng);
                survivors.push(child);
            }
            Fate::Stands => {
                let holds_something = (0..n).any(|j| {
                    j != i && fate[j] == Fate::Stands && rests_above(&bounds[j], &bounds[i])
                });
                let f = if holds_something {
                    SUPPORTER_ASKEW
                } else {
                    1.0
                };
                let a = signed_unit_f32(rng) * max_lean * f;
                let b = signed_unit_f32(rng) * max_lean * f;
                lean_node(&mut child, (a * a + b * b).sqrt(), a, b);
                child.transform.translation.0[1] -= range_f32(rng, 0.0, 0.2 * f);
                survivors.push(child);
            }
        }
    }
    node.children = survivors;
}

/// Topple a collapsed part to the ground: a hard lean about a random
/// horizontal axis, a small lateral slide, and a drop that leaves it lying
/// at the structure's base on roughly its thinnest side.
fn fell_part(child: &mut Generator, b: &Bounds, base_y: f32, rng: &mut ChaCha8Rng) {
    let angle = unit_f32(rng) * std::f32::consts::TAU;
    let topple = range_f32(rng, 0.8, 1.4);
    lean_node(child, topple, angle.sin(), angle.cos());

    let half = b.half();
    let lying_half = half[0].min(half[1]).min(half[2]).max(0.05);
    let centre = b.centre();
    child.transform.translation.0[0] += signed_unit_f32(rng) * 0.4;
    child.transform.translation.0[2] += signed_unit_f32(rng) * 0.4;
    child.transform.translation.0[1] += (base_y + lying_half * 0.9) - centre[1];
}

/// Broken masonry rubble — board-formed concrete with formwork pitting, in
/// a weathered grey that varies a little block-to-block.
fn rubble_material(grey: f32) -> SovereignMaterialSettings {
    let color = [grey, grey * 0.97, grey * 0.92];
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
            color_base: Fp3(color),
            color_pit: Fp3([grey * 0.6, grey * 0.58, grey * 0.55]),
            formwork_lines: Fp64(2.0),
            pit_density: Fp64(0.18),
            ..Default::default()
        }),
        ..SovereignMaterialSettings::default()
    }
}

/// Append a few small rubble blocks around the structure's base, each a
/// different size, grey and tumble.
fn scatter_rubble(node: &mut Generator, rng: &mut ChaCha8Rng) {
    let count = 2 + (unit_f32(rng) * 3.0) as usize; // 2..=4
    for _ in 0..count {
        let sx = range_f32(rng, 0.15, 0.45);
        let sy = range_f32(rng, 0.12, 0.3);
        let sz = range_f32(rng, 0.15, 0.45);
        let angle = unit_f32(rng) * std::f32::consts::TAU;
        let dist = range_f32(rng, 0.4, 1.4);
        let grey = range_f32(rng, 0.24, 0.36);
        // A tumbled block: yaw + a little tilt off vertical.
        let yaw = [0.0, (angle * 0.5).sin(), 0.0, (angle * 0.5).cos()];
        let tilt_a = signed_unit_f32(rng) * 0.5;
        let tilt_b = signed_unit_f32(rng) * 0.5;
        let rotation = tumble(yaw, tilt_a, tilt_b);
        let taper = range_f32(rng, 0.0, 0.3);
        node.children.push(Generator {
            kind: GeneratorKind::Cuboid {
                size: Fp3([sx, sy, sz]),
                solid: true,
                material: rubble_material(grey),
                torture: TortureParams {
                    taper: Fp2([taper, taper]),
                    ..Default::default()
                },
            },
            transform: TransformData {
                translation: Fp3([angle.sin() * dist, sy * 0.5, angle.cos() * dist]),
                rotation: Fp4(rotation),
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            children: Vec::new(),
            audio: super::audio::SovereignAudioConfig::None,
        });
    }
}

/// Compose a small horizontal tilt `(ax, _, az)` onto an existing yaw
/// quaternion — a rubble block knocked off-level.
fn tumble(yaw: [f32; 4], ax: f32, az: f32) -> [f32; 4] {
    let len = (ax * ax + az * az).sqrt();
    if len < 1e-5 {
        return yaw;
    }
    let angle = len.min(0.7);
    let half = angle * 0.5;
    let (s, c) = (half.sin(), half.cos());
    let tilt = [ax / len * s, 0.0, az / len * s, c];
    quat_mul(tilt, yaw)
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
                torture: TortureParams::default(),
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
    fn conflict_breaks_some_neon() {
        const NEON: [f32; 3] = [1.0, 0.1, 0.8];
        fn lit(y: f32) -> Generator {
            Generator {
                kind: GeneratorKind::Cuboid {
                    size: Fp3([0.5, 0.5, 0.5]),
                    solid: false,
                    material: SovereignMaterialSettings {
                        emission_color: Fp3(NEON),
                        emission_strength: Fp(8.0),
                        ..SovereignMaterialSettings::default()
                    },
                    torture: TortureParams::default(),
                },
                transform: TransformData {
                    translation: Fp3([0.0, y, 0.0]),
                    rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
                    scale: Fp3([1.0, 1.0, 1.0]),
                },
                children: Vec::new(),
                audio: super::super::audio::SovereignAudioConfig::None,
            }
        }
        fn neon_strengths(n: &Generator, out: &mut Vec<f32>) {
            if let GeneratorKind::Cuboid { material, .. } = &n.kind
                && material.emission_color.0 == NEON
            {
                out.push(material.emission_strength.0);
            }
            for c in &n.children {
                neon_strengths(c, out);
            }
        }

        // Across rooms, a conflict ruin should snuff or gutter some neon
        // (dim it below its authored 8.0) — broken signage.
        let any_broken = (0u64..20).any(|s| {
            let mut node = lit(1.0);
            node.children = vec![lit(2.0), lit(3.0), lit(4.0)];
            apply_ruin(&mut node, 0.95, s);
            let mut v = Vec::new();
            neon_strengths(&node, &mut v);
            v.iter().any(|&x| x < 8.0)
        });
        assert!(any_broken, "conflict should break some neon");

        // A calm room never touches the neon.
        let mut calm = lit(1.0);
        calm.children = vec![lit(2.0)];
        apply_ruin(&mut calm, 0.0, 3);
        let mut v = Vec::new();
        neon_strengths(&calm, &mut v);
        assert!(
            v.iter().all(|&x| (x - 8.0).abs() < 1e-6),
            "calm keeps neon intact"
        );
    }

    #[test]
    fn rotation_stays_unit_length() {
        let mut node = structure();
        apply_ruin(&mut node, 0.95, 99);
        let r = node.transform.rotation.0;
        let mag = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2] + r[3] * r[3]).sqrt();
        assert!((mag - 1.0).abs() < 1e-4, "lean must keep a unit quaternion");
    }

    // -- support-aware collapse (#776) ------------------------------------

    /// Roof slab size — X extent outside the rubble band (0.15..0.45) so the
    /// part stays identifiable after ruin adds rubble cuboids.
    const ROOF: [f32; 3] = [4.0, 0.3, 4.0];
    /// Pendant hanging just under the roof (in contact with it, nothing
    /// below it).
    const PENDANT: [f32; 3] = [0.5, 0.24, 0.5];
    /// Free-standing crate on the slab — the lowest-based part, so it is
    /// the collapse anchor and the columns stay genuinely removable.
    const CRATE: [f32; 3] = [0.6, 0.6, 0.6];

    fn boxed(size: [f32; 3], at: [f32; 3]) -> Generator {
        Generator {
            kind: GeneratorKind::Cuboid {
                size: Fp3(size),
                solid: true,
                material: SovereignMaterialSettings::default(),
                torture: TortureParams::default(),
            },
            transform: TransformData {
                translation: Fp3(at),
                rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            children: Vec::new(),
            audio: super::super::audio::SovereignAudioConfig::None,
        }
    }

    fn column(x: f32) -> Generator {
        Generator {
            kind: GeneratorKind::Cylinder {
                radius: Fp(0.15),
                height: Fp(2.0),
                resolution: 12,
                solid: true,
                material: SovereignMaterialSettings::default(),
                torture: TortureParams::default(),
            },
            transform: TransformData {
                translation: Fp3([x, 1.2, 0.0]),
                rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            children: Vec::new(),
            audio: super::super::audio::SovereignAudioConfig::None,
        }
    }

    /// A flush-authored pavilion: a foundation-slab root (spans y −0.2..0.2
    /// in its own frame), two columns (0.2..2.2) carrying a roof slab
    /// (2.2..2.5), a pendant hanging under the roof (1.86..2.1), and a
    /// crate on the slab (0.15..0.75).
    fn pavilion() -> Generator {
        let mut root = boxed([4.0, 0.4, 4.0], [0.0, 0.0, 0.0]);
        root.children = vec![
            column(-1.5),
            column(1.5),
            boxed(ROOF, [0.0, 2.35, 0.0]),
            boxed(PENDANT, [0.0, 1.98, 0.0]),
            boxed(CRATE, [1.2, 0.45, 1.2]),
        ];
        root
    }

    /// Y translation of the (unique) cuboid child with exactly this size.
    fn cuboid_y(node: &Generator, size: [f32; 3]) -> Option<f32> {
        node.children.iter().find_map(|c| match &c.kind {
            GeneratorKind::Cuboid { size: s, .. } if s.0 == size => {
                Some(c.transform.translation.0[1])
            }
            _ => None,
        })
    }

    fn standing_columns(node: &Generator) -> usize {
        node.children
            .iter()
            .filter(|c| {
                matches!(c.kind, GeneratorKind::Cylinder { .. })
                    && c.transform.translation.0[1] > 0.9
            })
            .count()
    }

    #[test]
    fn conflict_never_leaves_parts_floating() {
        for seed in 0..300u64 {
            let mut p = pavilion();
            apply_ruin(&mut p, 0.95, seed);

            // A roof still aloft (standing parts sink at most ~0.2 m; a
            // felled roof drops to the base) must have a standing column
            // under it.
            let roof_aloft = cuboid_y(&p, ROOF).is_some_and(|y| y > 1.5);
            if roof_aloft {
                assert!(
                    standing_columns(&p) > 0,
                    "seed {seed}: roof aloft with no standing column"
                );
            }
            // A pendant still hanging must have its roof above it.
            if cuboid_y(&p, PENDANT).is_some_and(|y| y > 1.4) {
                assert!(
                    roof_aloft,
                    "seed {seed}: pendant hangs in the air with its roof gone"
                );
            }
        }
    }

    #[test]
    fn anchor_part_never_collapses() {
        for seed in 0..100u64 {
            let mut p = pavilion();
            apply_ruin(&mut p, 0.95, seed);
            // The crate has the lowest base, so it is the anchor: always
            // present, standing (its authored XZ untouched — fell/askew
            // never move a standing anchor laterally).
            let crate_part = p
                .children
                .iter()
                .find(|c| matches!(&c.kind, GeneratorKind::Cuboid { size, .. } if size.0 == CRATE))
                .unwrap_or_else(|| panic!("seed {seed}: anchor crate vanished"));
            let t = crate_part.transform.translation.0;
            assert_eq!((t[0], t[2]), (1.2, 1.2), "seed {seed}: anchor was moved");
        }
    }

    #[test]
    fn felled_roofs_rest_near_the_ground() {
        let (mut saw_standing, mut saw_collapsed) = (false, false);
        for seed in 0..300u64 {
            let mut p = pavilion();
            apply_ruin(&mut p, 0.95, seed);
            match cuboid_y(&p, ROOF) {
                Some(y) if y > 1.5 => saw_standing = true,
                Some(y) => {
                    // Felled: lying at the base, not hovering mid-air.
                    assert!(y < 0.5, "seed {seed}: felled roof hovers at {y}");
                    saw_collapsed = true;
                }
                None => saw_collapsed = true,
            }
        }
        assert!(
            saw_standing && saw_collapsed,
            "sweep must exercise both outcomes (standing {saw_standing}, collapsed {saw_collapsed})"
        );
    }
}
