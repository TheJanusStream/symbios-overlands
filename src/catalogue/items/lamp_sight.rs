//! Whether a lit lamp glows where it is seen (#1927): every part carrying an
//! authored light glows on every face it draws, and is seen glowing from
//! where a player stands - not hidden in an opaque shade, housing or beam.
//!
//! No material here is transparent but an alpha card (a window's panes, a
//! grille, a leaf), so a flame inside a glazed drum or a lens inside a hood
//! is never seen: what the eye meets on its way to it is the drum or the
//! hood, and if that does not glow the lamp reads unlit beside its own pool
//! of light. So a lamp is read the way it is drawn - each primitive meshed
//! by the real mesher, its faces given the materials they are drawn in (a
//! face override where one paints them) - and looked at from rings of eyes
//! around the tree, at a walker's eye height and at the orbit camera's:
//! from each, a ray to points on the lamp's surface meets first either the
//! lamp itself or a part glowing at least as brightly as a lamp must (its
//! glazing). Alpha cards are looked through.
//!
//! Read for the catalogue's lamps (`lit_items`). The seeded vehicles' lamps
//! were measured this way once, for #1927 (two seeds of each craft type, all
//! passing); nothing in the tree reads them so now.

use bevy::math::{Affine3A, Vec3};
use bevy::mesh::{Indices, Mesh};

use super::measure::{is_primitive, transform_of};
use crate::pds::{Generator, SovereignMaterialSettings};

/// The least a lamp's glow may be, on every face it draws, for it to read as
/// a lamp lit (the catalogue's dimmest lit glass glows at 1.2, its dimmest
/// lamp at 1.3): the dim amber of an unlit lantern's glass (0.35) does not.
pub(crate) const MIN_GLOW: f32 = 1.0;

/// How far past a tree's own reach across the rings of eyes stand (m):
/// beside it, near, across a square and across a field - a tall lamp, a
/// mast's aviation light, is seen from afar.
const RINGS_M: [f32; 4] = [2.0, 6.0, 20.0, 40.0];

/// The eye heights over the tree's ground (m): a walker's, and the orbit
/// camera's (12 m back at its 0.4 rad pitch, over a body's middle).
const EYE_HEIGHTS_M: [f32; 2] = [1.7, 5.7];

/// The eyes on each ring, evenly round it.
const YAWS: usize = 16;

/// How many eyes look at a lamp: every ring at every height.
pub(crate) const VIEWS: usize = RINGS_M.len() * EYE_HEIGHTS_M.len() * YAWS;

/// The fewest eyes a lamp must be seen glowing from: one in sixteen. A
/// lamp on one face of a building, in a doorway or a forge's mouth is seen
/// from a side; one inside a shade or a beam, from none.
pub(crate) const MIN_VIEWS: usize = 8;

/// The most points of a lamp's surface each eye looks for.
const SAMPLES: usize = 48;

/// One primitive as drawn: its triangles in the tree's frame, how brightly
/// each glows, which are alpha cards (looked through), its bounds, and
/// whether it carries a light.
struct Drawn {
    path: String,
    tris: Vec<[Vec3; 3]>,
    glow: Vec<f32>,
    card: Vec<bool>,
    lo: Vec3,
    hi: Vec3,
    lit: bool,
}

/// How brightly a material glows: its strength, or nothing when its
/// colour is black.
fn glow_of(material: &SovereignMaterialSettings) -> f32 {
    if material.emission_color.0.iter().any(|c| *c > 0.0) {
        material.emission_strength.0.max(0.0)
    } else {
        0.0
    }
}

/// Every primitive `tree` draws, each node's frame composed as the spawn
/// path composes it.
fn drawn(tree: &Generator) -> Vec<Drawn> {
    fn visit(node: &Generator, parent: Affine3A, path: String, out: &mut Vec<Drawn>) {
        let frame = parent * transform_of(&node.transform).compute_affine();
        if is_primitive(&node.kind) {
            let built = crate::world_builder::build_primitive_mesh(&node.kind);
            let positions = built
                .mesh
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .and_then(|a| a.as_float3())
                .unwrap_or(&[]);
            let indices: Vec<usize> = match built.mesh.indices() {
                Some(Indices::U16(v)) => v.iter().map(|&i| usize::from(i)).collect(),
                Some(Indices::U32(v)) => v.iter().map(|&i| i as usize).collect(),
                None => (0..positions.len()).collect(),
            };
            let faces = node.kind.faces().unwrap_or(&[]);
            let mut part = Drawn {
                path: path.clone(),
                tris: Vec::new(),
                glow: Vec::new(),
                card: Vec::new(),
                lo: Vec3::splat(f32::INFINITY),
                hi: Vec3::splat(f32::NEG_INFINITY),
                lit: node.light.is_some(),
            };
            for (t, corners) in indices.as_chunks::<3>().0.iter().enumerate() {
                let tri =
                    [0, 1, 2].map(|k| frame.transform_point3(Vec3::from(positions[corners[k]])));
                for p in tri {
                    part.lo = part.lo.min(p);
                    part.hi = part.hi.max(p);
                }
                part.tris.push(tri);
                // A face override paints its face whole; every other face
                // wears the base material.
                let face = built.faces.face_of(t as u32);
                let material = faces
                    .iter()
                    .find(|f| Some(f.face) == face)
                    .map(|f| &f.material)
                    .or(node.kind.material());
                part.glow.push(material.map_or(0.0, glow_of));
                part.card
                    .push(material.is_some_and(|m| m.texture.is_card()));
            }
            if !part.tris.is_empty() {
                out.push(part);
            }
        }
        for (i, child) in node.children.iter().enumerate() {
            visit(child, frame, format!("{path}/{i}"), out);
        }
    }
    let mut out = Vec::new();
    visit(tree, Affine3A::IDENTITY, String::new(), &mut out);
    out
}

/// Whether the ray from `o` along `d` meets the box before `reach`.
fn meets_box(o: Vec3, d: Vec3, lo: Vec3, hi: Vec3, reach: f32) -> bool {
    let step = d.recip();
    let (t0, t1) = ((lo - o) * step, (hi - o) * step);
    let enter = t0.min(t1).max_element();
    let leave = t0.max(t1).min_element();
    leave >= enter.max(0.0) && enter <= reach
}

/// How far along the ray from `o` along `d` it meets `tri`, if it does
/// (Moller and Trumbore's test).
fn meets_tri(o: Vec3, d: Vec3, [a, b, c]: [Vec3; 3]) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(-1e-6..=1.0 + 1e-6).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < -1e-6 || u + v > 1.0 + 1e-6 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-5).then_some(t)
}

/// The first part and triangle the ray from `o` along unit `d` meets
/// before `reach`, alpha cards looked through.
fn first_met(parts: &[Drawn], o: Vec3, d: Vec3, reach: f32) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize, f32)> = None;
    for (pi, part) in parts.iter().enumerate() {
        let near = best.map_or(reach, |b| b.2);
        if !meets_box(o, d, part.lo - 1e-4, part.hi + 1e-4, near) {
            continue;
        }
        for (ti, tri) in part.tris.iter().enumerate() {
            if part.card[ti] {
                continue;
            }
            if let Some(t) = meets_tri(o, d, *tri)
                && t <= best.map_or(reach, |b| b.2)
            {
                best = Some((pi, ti, t));
            }
        }
    }
    best.map(|(pi, ti, _)| (pi, ti))
}

/// One lit part of a tree as it is seen: where it hangs, the least its
/// drawn faces glow, and how many of the [`VIEWS`] eyes see it glowing at
/// `min_glow` or more (itself, or a part as bright in front of it).
#[derive(Clone, Debug)]
pub(crate) struct LampSight {
    pub path: String,
    pub least_glow: f32,
    pub seen: usize,
}

/// Every lit part of `tree` as it is seen - see the module docs - by eyes
/// standing on `ground`, a height in the tree's frame: a catalogue item's
/// origin is its ground (its footings reach under it).
pub(crate) fn lamp_sights(tree: &Generator, ground: f32, min_glow: f32) -> Vec<LampSight> {
    let parts = drawn(tree);
    if !parts.iter().any(|p| p.lit) {
        return Vec::new();
    }
    let lo = parts
        .iter()
        .fold(Vec3::splat(f32::INFINITY), |a, p| a.min(p.lo));
    let hi = parts
        .iter()
        .fold(Vec3::splat(f32::NEG_INFINITY), |a, p| a.max(p.hi));
    let middle = (lo + hi) * 0.5;
    let across = ((hi - lo) * 0.5).x.max(((hi - lo) * 0.5).z);
    let eyes: Vec<Vec3> = RINGS_M
        .iter()
        .flat_map(|ring| {
            EYE_HEIGHTS_M.iter().flat_map(move |height| {
                (0..YAWS).map(move |k| {
                    let yaw = k as f32 * std::f32::consts::TAU / YAWS as f32;
                    let r = across + ring;
                    Vec3::new(
                        middle.x + r * yaw.sin(),
                        ground + height,
                        middle.z + r * yaw.cos(),
                    )
                })
            })
        })
        .collect();
    parts
        .iter()
        .enumerate()
        .filter(|(_, p)| p.lit)
        .map(|(li, lamp)| {
            let step = (lamp.tris.len() / SAMPLES).max(1);
            let points: Vec<Vec3> = lamp
                .tris
                .iter()
                .step_by(step)
                .map(|t| (t[0] + t[1] + t[2]) / 3.0)
                .collect();
            let seen = eyes
                .iter()
                .filter(|&&eye| {
                    points.iter().any(|&point| {
                        let to = point - eye;
                        let far = to.length();
                        match first_met(&parts, eye, to / far, far + 0.01) {
                            Some((pi, _)) if pi == li => true,
                            Some((pi, ti)) => parts[pi].glow[ti] >= min_glow,
                            None => false,
                        }
                    })
                })
                .count();
            LampSight {
                path: lamp.path.clone(),
                least_glow: lamp.glow.iter().copied().fold(f32::INFINITY, f32::min),
                seen,
            }
        })
        .collect()
}

/// Every lit part of `tree` and the least it glows on any face it draws,
/// without looking for it.
pub(crate) fn lamp_glows(tree: &Generator) -> Vec<(String, f32)> {
    drawn(tree)
        .into_iter()
        .filter(|part| part.lit)
        .map(|part| {
            let least = part.glow.iter().copied().fold(f32::INFINITY, f32::min);
            (part.path, least)
        })
        .collect()
}

/// What is wrong with how `tree`'s lamps glow, one line each: a lamp with
/// a face glowing under `min_glow`, or one seen glowing at `min_glow` from
/// fewer than [`MIN_VIEWS`] eyes standing on `ground` ([`lamp_sights`]).
pub(crate) fn glow_faults(tree: &Generator, ground: f32, min_glow: f32) -> Vec<String> {
    lamp_sights(tree, ground, min_glow)
        .into_iter()
        .filter_map(|sight| {
            let dim = sight.least_glow < min_glow;
            let hidden = sight.seen < MIN_VIEWS;
            (dim || hidden).then(|| {
                format!(
                    "the lamp at {} glows at {} at least, seen glowing from {} of {VIEWS} eyes",
                    if sight.path.is_empty() {
                        "/"
                    } else {
                        &sight.path
                    },
                    sight.least_glow,
                    sight.seen
                )
            })
        })
        .collect()
}
