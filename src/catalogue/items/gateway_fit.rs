//! Gateway veil fit measurement (#1006).
//!
//! Every themed gateway is a frame — jambs left and right, a lintel over
//! the top, a threshold underfoot — with one translucent
//! [`GeneratorKind::Gateway`] veil standing in the opening. The veil is
//! both the walk-in sensor and a rendered [`bevy::prelude::Cuboid`]
//! (see `world_builder::gateway`), so wherever its box does not
//! coincide with the opening its edges read as a floating cuboid instead
//! of a pane of light filling the gate.
//!
//! Fitting that box by hand across 24 bespoke frames means re-deriving
//! nested transforms, tapers and rotations per file. This module does it
//! from the built tree instead: it walks a gateway [`Generator`],
//! resolves every node's world transform, meshes each primitive through
//! the real mesher ([`build_primitive_mesh`]) so tapers and cuts are
//! accounted for, and reports the veil box against the solid geometry
//! around it.
//!
//! Two consumers:
//!
//! * [`probe`] — the per-face report the `--gateway-fit` dev command
//!   prints, used to derive each frame's true opening.
//! * [`fit_faults`] — the invariant the catalogue test asserts: each of
//!   the veil's four framed faces must sit *inside* solid geometry (no
//!   gap, no overhang) and the veil must not protrude past the frame's
//!   depth.
//!
//! Bounds are axis-aligned and therefore conservative for rotated or
//! tapered pieces: a tapered pylon measures as its untapered box. That
//! errs toward accepting a veil edge that a tapered jamb only partly
//! covers, which is the harmless direction — the frames that matter here
//! are axis-aligned masonry.

use bevy::prelude::*;

use crate::pds::{Generator, GeneratorKind, TransformData};
use crate::world_builder::build_primitive_mesh;

/// An axis-aligned box in the prop's ground-relative frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl Bounds {
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    /// Whether `p` lies within the box, with `slack` metres of tolerance
    /// on every side. Callers probing a face centre pass a small positive
    /// slack so an edge that lands exactly on a surface still counts as
    /// covered.
    pub fn contains(&self, p: Vec3, slack: f32) -> bool {
        (0..3).all(|i| p[i] >= self.min[i] - slack && p[i] <= self.max[i] + slack)
    }

    /// Whether the two boxes overlap on axis `i`.
    fn overlaps_axis(&self, other: &Bounds, i: usize) -> bool {
        self.min[i] <= other.max[i] && self.max[i] >= other.min[i]
    }
}

/// One solid piece of a gateway frame, in the prop's ground-relative
/// frame. "Solid" here means *occluding geometry* — an emissive trim tube
/// is as good a thing to bury a veil edge in as a masonry jamb, so the
/// only kinds excluded are the non-geometric ones (particles, lights, the
/// veil itself).
#[derive(Clone, Debug)]
pub struct SolidPiece {
    /// Child path from the tree root, for naming the piece in a report.
    pub path: Vec<usize>,
    pub kind_tag: &'static str,
    pub bounds: Bounds,
}

/// The veil plus every solid piece of one gateway, all resolved into the
/// prop's ground-relative frame.
#[derive(Clone, Debug)]
pub struct GatewayGeometry {
    pub veil: Bounds,
    pub solids: Vec<SolidPiece>,
}

/// Resolve a gateway tree into its veil box and solid pieces. Returns
/// `None` when the tree carries no [`GeneratorKind::Gateway`] node — a
/// structure without its zone is not a gateway.
pub fn measure(root: &Generator) -> Option<GatewayGeometry> {
    let mut solids = Vec::new();
    let mut veil = None;
    walk(
        root,
        &transform_of(&root.transform),
        &mut Vec::new(),
        &mut solids,
        &mut veil,
    );
    veil.map(|veil| GatewayGeometry { veil, solids })
}

fn transform_of(t: &TransformData) -> Transform {
    Transform {
        translation: Vec3::from_array(t.translation.0),
        rotation: Quat::from_array(t.rotation.0),
        scale: Vec3::from_array(t.scale.0),
    }
}

fn walk(
    node: &Generator,
    world: &Transform,
    path: &mut Vec<usize>,
    solids: &mut Vec<SolidPiece>,
    veil: &mut Option<Bounds>,
) {
    match &node.kind {
        GeneratorKind::Gateway { size } => {
            let half = Vec3::from_array(size.0) * 0.5;
            // The veil spawns as an axis-aligned Cuboid; gateways never
            // rotate it, so its own box needs no corner sweep.
            *veil = Some(Bounds {
                min: world.translation - half,
                max: world.translation + half,
            });
        }
        kind => {
            if let Some(bounds) = mesh_bounds(kind, world) {
                solids.push(SolidPiece {
                    path: path.clone(),
                    kind_tag: kind.kind_tag(),
                    bounds,
                });
            }
        }
    }

    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        let child_world = *world * transform_of(&child.transform);
        walk(child, &child_world, path, solids, veil);
        path.pop();
    }
}

/// World-space bounds of one primitive, meshed through the real mesher so
/// taper, cuts and hollows are reflected. `None` for kinds that carry no
/// occluding geometry (particles, lights, audio-only nodes, L-systems and
/// other non-primitive kinds the mesher does not own).
fn mesh_bounds(kind: &GeneratorKind, world: &Transform) -> Option<Bounds> {
    if !is_primitive(kind) {
        return None;
    }
    let built = build_primitive_mesh(kind);
    let positions = built
        .mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)?
        .as_float3()?;
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for p in positions {
        let w = world.transform_point(Vec3::from_array(*p));
        min = min.min(w);
        max = max.max(w);
    }
    (min.x <= max.x).then_some(Bounds { min, max })
}

/// Whether the mesher owns this kind — mirrors the primitive arm of the
/// spawn router in `world_builder::compile::dispatch`.
fn is_primitive(kind: &GeneratorKind) -> bool {
    matches!(
        kind,
        GeneratorKind::Cuboid { .. }
            | GeneratorKind::Sphere { .. }
            | GeneratorKind::Cylinder { .. }
            | GeneratorKind::Capsule { .. }
            | GeneratorKind::Cone { .. }
            | GeneratorKind::Torus { .. }
            | GeneratorKind::Plane { .. }
            | GeneratorKind::Tetrahedron { .. }
            | GeneratorKind::Tube { .. }
            | GeneratorKind::Bevel { .. }
            | GeneratorKind::Wedge { .. }
            | GeneratorKind::Helix { .. }
            | GeneratorKind::Superellipsoid { .. }
            | GeneratorKind::Spine { .. }
            | GeneratorKind::Lathe { .. }
            | GeneratorKind::BlobGroup { .. }
    )
}

/// The six directions a veil face can be probed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Left,
    Right,
    Top,
    Bottom,
    Front,
    Back,
}

impl Face {
    /// The four faces a frame is expected to bury: the jambs, the lintel
    /// and the threshold. Front/back open onto the approach and are
    /// bounded by the frame's *depth* instead — see [`fit_faults`].
    pub const FRAMED: [Self; 4] = [Self::Left, Self::Right, Self::Top, Self::Bottom];

    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "left jamb",
            Self::Right => "right jamb",
            Self::Top => "lintel",
            Self::Bottom => "threshold",
            Self::Front => "front",
            Self::Back => "back",
        }
    }

    /// The centre of this face of `b`.
    pub fn center_of(self, b: &Bounds) -> Vec3 {
        let c = b.center();
        match self {
            Self::Left => Vec3::new(b.min.x, c.y, c.z),
            Self::Right => Vec3::new(b.max.x, c.y, c.z),
            Self::Top => Vec3::new(c.x, b.max.y, c.z),
            Self::Bottom => Vec3::new(c.x, b.min.y, c.z),
            Self::Front => Vec3::new(c.x, c.y, b.min.z),
            Self::Back => Vec3::new(c.x, c.y, b.max.z),
        }
    }
}

/// What the geometry around one veil face looks like — the report the
/// `--gateway-fit` dev command prints per gateway.
#[derive(Clone, Debug)]
pub struct FaceProbe {
    pub face: Face,
    /// Where the veil's face currently sits, on that face's axis.
    pub veil_at: f32,
    /// The solid piece the face centre lands inside, if any. A face with
    /// no cover is either short of the frame (a gap) or past it (an
    /// overhang).
    pub covered_by: Option<SolidPiece>,
    /// The nearest solid *ahead* of the face along its outward axis that
    /// spans the opening — the frame surface the veil should reach into.
    pub nearest_ahead: Option<(f32, SolidPiece)>,
}

/// Probe all six veil faces against the frame.
pub fn probe(geo: &GatewayGeometry) -> Vec<FaceProbe> {
    [
        Face::Left,
        Face::Right,
        Face::Top,
        Face::Bottom,
        Face::Front,
        Face::Back,
    ]
    .into_iter()
    .map(|face| probe_face(geo, face))
    .collect()
}

fn probe_face(geo: &GatewayGeometry, face: Face) -> FaceProbe {
    let p = face.center_of(&geo.veil);
    let (axis, outward) = match face {
        Face::Left => (0, -1.0),
        Face::Right => (0, 1.0),
        Face::Top => (1, 1.0),
        Face::Bottom => (1, -1.0),
        Face::Front => (2, -1.0),
        Face::Back => (2, 1.0),
    };

    let covered_by = geo
        .solids
        .iter()
        .find(|s| s.bounds.contains(p, COVER_SLACK))
        .cloned();

    // The nearest solid ahead of the face that still spans the opening on
    // the other two axes — i.e. a piece the veil could grow into.
    let others: [usize; 2] = match axis {
        0 => [1, 2],
        1 => [0, 2],
        _ => [0, 1],
    };
    let nearest_ahead = geo
        .solids
        .iter()
        .filter(|s| others.iter().all(|&i| s.bounds.overlaps_axis(&geo.veil, i)))
        .filter_map(|s| {
            // Distance from the face to this piece's near surface, along
            // the outward direction. Negative means the piece is behind.
            let surface = if outward > 0.0 {
                s.bounds.min[axis]
            } else {
                s.bounds.max[axis]
            };
            let d = (surface - p[axis]) * outward;
            (d >= -COVER_SLACK).then(|| (d, s.clone()))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));

    FaceProbe {
        face,
        veil_at: p[axis],
        covered_by,
        nearest_ahead,
    }
}

/// Tolerance when asking whether a veil face is buried in a solid. Frames
/// are authored to centimetre precision and the veil is meant to overlap
/// its frame by a few centimetres, so a millimetre of slack only forgives
/// float drift.
const COVER_SLACK: f32 = 1.0e-3;

/// A way one gateway's veil fails to fit, phrased for a test failure.
#[derive(Clone, Debug)]
pub struct FitFault {
    pub face: Face,
    pub detail: String,
}

/// Sample points across one veil face: a grid inset from the face's own
/// edges, so the check covers the whole face instead of a single centre
/// point — the difference between "the veil meets the frame" and "the
/// veil meets a trim strip glued to the frame, with daylight either side
/// of it".
///
/// The inset keeps the samples off the extreme corners, where a jamb and
/// a lintel legitimately stop short of each other's mitre.
fn face_samples(b: &Bounds, face: Face) -> Vec<Vec3> {
    let (u, v) = match face {
        // (the two axes spanning this face)
        Face::Left | Face::Right => (1, 2),
        Face::Top | Face::Bottom => (0, 2),
        Face::Front | Face::Back => (0, 1),
    };
    let anchor = face.center_of(b);
    let mut out = Vec::with_capacity(9);
    for su in [0.15_f32, 0.5, 0.85] {
        for sv in [0.15_f32, 0.5, 0.85] {
            let mut p = anchor;
            p[u] = b.min[u] + (b.max[u] - b.min[u]) * su;
            p[v] = b.min[v] + (b.max[v] - b.min[v]) * sv;
            out.push(p);
        }
    }
    out
}

/// Check the fit invariant, returning one fault per offending face.
///
/// * Every sample across each of [`Face::FRAMED`] must be buried in a
///   solid piece. A sample floating in air means the veil is either short
///   of the frame (a gap) or past it (an overhang) at that spot — both
///   show the cuboid edge the overhaul removes.
/// * Front and back must not protrude past the depth of the pieces
///   burying the jambs, or the veil juts out of the gate mouth.
pub fn fit_faults(geo: &GatewayGeometry) -> Vec<FitFault> {
    let mut faults = Vec::new();

    for face in Face::FRAMED {
        let samples = face_samples(&geo.veil, face);
        let total = samples.len();
        let open: Vec<Vec3> = samples
            .into_iter()
            .filter(|p| {
                !geo.solids
                    .iter()
                    .any(|s| s.bounds.contains(*p, COVER_SLACK))
            })
            .collect();
        if let Some(first) = open.first() {
            faults.push(FitFault {
                face,
                detail: format!(
                    "{}/{total} samples across the veil's {} face are in open air (e.g. \
                     [{:.3}, {:.3}, {:.3}]) — the veil neither reaches nor is buried in the \
                     frame there",
                    open.len(),
                    face.label(),
                    first.x,
                    first.y,
                    first.z
                ),
            });
        }
    }

    // Burial depth: an edge hidden inside a pier passes the samples above
    // however far in it goes, so bound it. A veil swollen well past its
    // opening tints the frame it is supposed to sit in, and stops being a
    // pane in the gate.
    for face in Face::FRAMED {
        let Some(piece) = frame_piece(geo, face) else {
            continue;
        };
        let (buried, surface) = match face {
            Face::Left => (piece.bounds.max.x - geo.veil.min.x, piece.bounds.max.x),
            Face::Right => (geo.veil.max.x - piece.bounds.min.x, piece.bounds.min.x),
            Face::Top => (geo.veil.max.y - piece.bounds.min.y, piece.bounds.min.y),
            Face::Bottom => (piece.bounds.max.y - geo.veil.min.y, piece.bounds.max.y),
            Face::Front | Face::Back => continue,
        };
        if buried > MAX_EMBED {
            faults.push(FitFault {
                face,
                detail: format!(
                    "veil {} face is buried {buried:.3} m past the frame surface at \
                     {surface:.3} — more than the {MAX_EMBED:.2} m an edge needs to hide, so \
                     the veil is swollen past its opening",
                    face.label()
                ),
            });
        }
    }

    // Depth: the veil may not stand proud of the pieces burying its jambs.
    //
    // Measured against the piece each jamb face actually sits in, not the
    // theme's nominal frame piece. Some gates frame their opening with two
    // *rows* of jambs — a propylaea's column pairs, a lattice mast's legs —
    // and a veil standing correctly in one row is not "jutting past" the
    // other one it was never in.
    let jamb_depth = [Face::Left, Face::Right]
        .into_iter()
        .filter_map(|f| {
            let p = f.center_of(&geo.veil);
            geo.solids
                .iter()
                .find(|s| s.bounds.contains(p, COVER_SLACK))
                .or_else(|| frame_piece(geo, f))
        })
        .fold(None::<(f32, f32)>, |acc, s| {
            let (lo, hi) = (s.bounds.min.z, s.bounds.max.z);
            Some(match acc {
                // Widest jamb wins: a veil inside the deepest jamb is
                // inside the mouth even if a thin trim tube is shallower.
                Some((a, b)) => (a.min(lo), b.max(hi)),
                None => (lo, hi),
            })
        });
    if let Some((lo, hi)) = jamb_depth {
        if geo.veil.min.z < lo - COVER_SLACK {
            faults.push(FitFault {
                face: Face::Front,
                detail: format!(
                    "veil front at {:.3} juts {:.3} m past the frame depth ({lo:.3})",
                    geo.veil.min.z,
                    lo - geo.veil.min.z
                ),
            });
        }
        if geo.veil.max.z > hi + COVER_SLACK {
            faults.push(FitFault {
                face: Face::Back,
                detail: format!(
                    "veil back at {:.3} juts {:.3} m past the frame depth ({hi:.3})",
                    geo.veil.max.z,
                    geo.veil.max.z - hi
                ),
            });
        }
    }

    faults
}

/// Overlap of two closed intervals.
fn overlap(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.1.min(b.1) - a.0.max(b.0)).max(0.0)
}

/// How much of a face a piece must cover to count as framing it rather
/// than decorating it. A jamb runs the height and depth of the mouth; a
/// light strip glued to that jamb covers a tenth of its depth.
const FRAME_COVERAGE: f32 = 0.6;

/// The depth the gate mouth runs to — the `z` range of its jambs.
///
/// Established before any face is fitted, because it is the yardstick the
/// rest of the fit is measured against: a piece only counts as frame if
/// it spans most of the mouth, and "most of the mouth" needs the mouth's
/// depth first. Candidates are the pieces standing entirely to one side
/// of the gate's centreline (so lintels, slabs and foundations, which
/// cross it, are excluded) and running most of the opening's height; the
/// deepest of those is the jamb.
pub fn mouth_depth_range(geo: &GatewayGeometry) -> Option<(f32, f32)> {
    let v = &geo.veil;
    let cx = v.center().x;
    let vy = (v.min.y, v.max.y);
    let vlen = (vy.1 - vy.0).max(1e-6);
    geo.solids
        .iter()
        .filter(|s| s.bounds.min.x >= cx || s.bounds.max.x <= cx)
        .filter(|s| overlap((s.bounds.min.y, s.bounds.max.y), vy) / vlen >= 0.5)
        .map(|s| (s.bounds.min.z, s.bounds.max.z))
        .max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)))
}

/// The piece of frame that bounds the opening in one direction.
///
/// Two traps this navigates. Themes line their jambs with trim — a 16 cm
/// light strip on a 70 cm stanchion, a neon tube down a pylon — so the
/// *nearest* surface is often not the frame, and a veil fitted to it
/// leaves daylight either side at other depths. But themes also carry
/// signage and beams high above the opening, so the *largest* piece is
/// not the frame either, and fitting to it stretches the veil past the
/// gate's head. The frame is therefore the **innermost piece that
/// actually spans the mouth** on both of the face's own axes.
pub fn frame_piece(geo: &GatewayGeometry, face: Face) -> Option<&SolidPiece> {
    let v = &geo.veil;
    let mouth_z = mouth_depth_range(geo)?;
    let mouth_depth = (mouth_z.1 - mouth_z.0).max(1e-6);

    // (axis probed, outward direction, the axis spanning the face besides z)
    let (axis, outward, span_axis) = match face {
        Face::Left => (0, -1.0_f32, 1),
        Face::Right => (0, 1.0, 1),
        Face::Top => (1, 1.0, 0),
        Face::Bottom => (1, -1.0, 0),
        Face::Front => (2, -1.0, 0),
        Face::Back => (2, 1.0, 0),
    };
    let veil_span = (v.min[span_axis], v.max[span_axis]);
    let veil_len = (veil_span.1 - veil_span.0).max(1e-6);

    geo.solids
        .iter()
        .filter_map(|s| {
            let surface = if outward > 0.0 {
                s.bounds.min[axis]
            } else {
                s.bounds.max[axis]
            };
            // Must lie outward of the gate's centre on this axis, or it is
            // not framing this side at all.
            let reach = (surface - v.center()[axis]) * outward;
            if reach <= 0.0 {
                return None;
            }
            let spans_face = overlap(
                (s.bounds.min[span_axis], s.bounds.max[span_axis]),
                veil_span,
            ) / veil_len;
            let spans_depth = overlap((s.bounds.min.z, s.bounds.max.z), mouth_z) / mouth_depth;
            (spans_face >= FRAME_COVERAGE && spans_depth >= FRAME_COVERAGE).then_some((s, reach))
        })
        // The innermost qualifying piece is the one the opening ends at.
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(s, _)| s)
}

/// How far a fitted veil buries each edge inside the frame (metres).
///
/// Big enough that the edge is unambiguously inside the jamb / lintel /
/// threshold rather than kissing its surface — the coplanar-contact case
/// that z-fights — and small enough to sit inside even the thin emissive
/// trim tubes some themes use as their innermost frame piece.
pub const EMBED: f32 = 0.04;

/// How far a veil edge may sit past the frame surface before it counts as
/// swollen rather than fitted. Generous next to [`EMBED`] so a frame with
/// a chamfer or a stepped reveal still passes, tight enough to reject a
/// veil sized to swallow its own piers.
pub const MAX_EMBED: f32 = 0.30;

/// The box a veil should occupy to fill its frame: every framed face
/// grown until it is buried [`EMBED`] inside the surface ahead of it, and
/// the depth clamped into the frame's own.
///
/// Faces already buried are left alone — a veil edge hidden inside a
/// thick pier is invisible, which is all the fit requires. Faces with no
/// surface ahead are left alone too: there is nothing to reach, so the
/// gateway needs a frame change rather than a veil change, and the report
/// says so instead of inventing a number.
pub fn recommend(geo: &GatewayGeometry) -> Bounds {
    let mut out = geo.veil;

    // Reach each framed face to its frame piece and bury it by EMBED.
    // Faces whose frame piece already swallows them are left alone: an
    // edge hidden inside a thick pier is invisible, which is all the fit
    // asks for.
    for face in Face::FRAMED {
        let Some(piece) = frame_piece(geo, face) else {
            continue;
        };
        match face {
            Face::Left => out.min.x = out.min.x.min(piece.bounds.max.x - EMBED),
            Face::Right => out.max.x = out.max.x.max(piece.bounds.min.x + EMBED),
            Face::Top => out.max.y = out.max.y.max(piece.bounds.min.y + EMBED),
            Face::Bottom => out.min.y = out.min.y.min(piece.bounds.max.y - EMBED),
            Face::Front | Face::Back => {}
        }
    }

    // Depth: the mouth is only as deep as the shallower jamb, so the veil
    // never stands proud on either side.
    let mut lo = f32::NEG_INFINITY;
    let mut hi = f32::INFINITY;
    for face in [Face::Left, Face::Right] {
        if let Some(piece) = frame_piece(geo, face) {
            lo = lo.max(piece.bounds.min.z);
            hi = hi.min(piece.bounds.max.z);
        }
    }
    if lo.is_finite() && hi.is_finite() && hi > lo {
        out.min.z = out.min.z.max(lo);
        out.max.z = out.max.z.min(hi);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::util::{cuboid_tapered, id_quat, prim, solid};
    use super::*;
    use crate::pds::{Fp3, SovereignMaterialSettings};

    fn mat() -> SovereignMaterialSettings {
        SovereignMaterialSettings::default()
    }

    /// A minimal gate: two jambs, a lintel, a threshold slab, and a veil
    /// sized to the caller's box. Mirrors how the themed entries author
    /// their frames (flat list under a root at the origin).
    fn test_gate(veil_size: [f32; 3], veil_y: f32) -> Generator {
        let mut root = prim(
            solid(cuboid_tapered([4.0, 0.2, 1.0], 0.0, mat())),
            [0.0, 0.1, 0.0],
            id_quat(),
        );
        // Jambs centred at x = ±1.5, so their inner faces stand at ±1.2.
        for sx in [-1.0_f32, 1.0] {
            root.children.push(prim(
                solid(cuboid_tapered([0.6, 2.8, 1.0], 0.0, mat())),
                [sx * 1.5, 1.5, 0.0],
                id_quat(),
            ));
        }
        // Lintel spanning the mouth, underside at y = 3.0. Children are
        // authored in the root's local frame, so this rides 0.1 up with it.
        root.children.push(prim(
            solid(cuboid_tapered([3.6, 0.4, 1.0], 0.0, mat())),
            [0.0, 3.1, 0.0],
            id_quat(),
        ));
        root.children.push(prim(
            GeneratorKind::Gateway {
                size: Fp3(veil_size),
            },
            [0.0, veil_y - 0.1, 0.0],
            id_quat(),
        ));
        root
    }

    /// The opening is x ∈ [-1.2, 1.2], y ∈ [0.2, 3.0], z ∈ [-0.5, 0.5].
    /// A veil overlapping every bound by 4 cm is the fitted case.
    #[test]
    fn a_fitted_veil_reports_no_faults() {
        let geo = measure(&test_gate([2.48, 2.88, 1.0], 1.6)).expect("no veil found");
        let faults = fit_faults(&geo);
        assert!(faults.is_empty(), "fitted veil reported faults: {faults:?}");
    }

    /// The shipped-before-#1006 shape: a box too narrow and too short for
    /// this frame leaves all four framed faces hanging in the opening.
    #[test]
    fn a_gapped_veil_reports_every_framed_face() {
        let geo = measure(&test_gate([2.0, 2.0, 1.0], 1.6)).expect("no veil found");
        let faults = fit_faults(&geo);
        let faces: Vec<Face> = faults.iter().map(|f| f.face).collect();
        for face in Face::FRAMED {
            assert!(faces.contains(&face), "{face:?} not reported: {faults:?}");
        }
    }

    /// A veil wider than its frame overhangs into open air — the jamb
    /// faces land past the masonry, not inside it.
    #[test]
    fn an_overhanging_veil_reports_its_jambs() {
        let geo = measure(&test_gate([4.4, 2.88, 1.0], 1.6)).expect("no veil found");
        let faults = fit_faults(&geo);
        let faces: Vec<Face> = faults.iter().map(|f| f.face).collect();
        assert!(
            faces.contains(&Face::Left),
            "left overhang missed: {faults:?}"
        );
        assert!(
            faces.contains(&Face::Right),
            "right overhang missed: {faults:?}"
        );
    }

    /// A veil deeper than the jambs juts out of the mouth even though its
    /// four framed faces are buried — the depth check is what catches it.
    #[test]
    fn a_too_deep_veil_reports_front_and_back() {
        let geo = measure(&test_gate([2.48, 2.88, 1.8], 1.6)).expect("no veil found");
        let faults = fit_faults(&geo);
        let faces: Vec<Face> = faults.iter().map(|f| f.face).collect();
        assert!(faces.contains(&Face::Front), "front jut missed: {faults:?}");
        assert!(faces.contains(&Face::Back), "back jut missed: {faults:?}");
    }

    /// The recommendation must turn a gapped veil into a fitted one —
    /// this is the loop the overhaul runs, so it has to close.
    #[test]
    fn recommending_a_gapped_veil_makes_it_fit() {
        let gate = test_gate([2.0, 2.0, 1.0], 1.6);
        let geo = measure(&gate).expect("no veil found");
        assert!(
            !fit_faults(&geo).is_empty(),
            "fixture should start unfitted"
        );

        let want = recommend(&geo);
        // Re-measure with the recommended box in place.
        let fitted = test_gate(want.size().to_array(), want.center().y);
        let geo = measure(&fitted).expect("no veil found");
        let faults = fit_faults(&geo);
        assert!(
            faults.is_empty(),
            "recommended box still faults: {faults:?}"
        );
    }

    /// The recommendation reaches the frame and buries itself by EMBED —
    /// not more, so a veil never eats visibly into its own frame.
    #[test]
    fn recommendation_buries_edges_by_the_embed_constant() {
        let geo = measure(&test_gate([2.0, 2.0, 1.0], 1.6)).expect("no veil found");
        let want = recommend(&geo);
        // Opening is x ∈ [-1.2, 1.2], y ∈ [0.2, 3.0].
        assert!((want.min.x - (-1.2 - EMBED)).abs() < 1e-4, "{want:?}");
        assert!((want.max.x - (1.2 + EMBED)).abs() < 1e-4, "{want:?}");
        assert!((want.min.y - (0.2 - EMBED)).abs() < 1e-4, "{want:?}");
        assert!((want.max.y - (3.0 + EMBED)).abs() < 1e-4, "{want:?}");
    }

    /// A gate whose jambs are lined with a shallow trim strip, as several
    /// themes do (a status-light strip on a stanchion, a neon tube down a
    /// pylon). The strip stands proud of the jamb but is a fraction of its
    /// depth.
    fn test_gate_with_trim(veil_size: [f32; 3], veil_y: f32) -> Generator {
        let mut root = test_gate(veil_size, veil_y);
        // Strips at x = ±1.15, inboard of the jambs' ±1.2 faces, but only
        // 0.16 deep against the jambs' 1.0.
        for sx in [-1.0_f32, 1.0] {
            root.children.push(prim(
                cuboid_tapered([0.1, 2.4, 0.16], 0.0, mat()),
                [sx * 1.15, 1.4, 0.0],
                id_quat(),
            ));
        }
        root
    }

    /// The frame is the jamb, not the trim glued to it. Fitting to the
    /// strip would leave daylight either side of it at other depths.
    #[test]
    fn trim_strips_do_not_masquerade_as_the_frame() {
        let geo = measure(&test_gate_with_trim([2.0, 2.0, 1.0], 1.6)).expect("no veil");
        let piece = frame_piece(&geo, Face::Right).expect("no right frame piece");
        assert!(
            (piece.bounds.max.z - piece.bounds.min.z) > 0.5,
            "picked the shallow trim strip as the frame: {piece:?}"
        );
        // And the recommendation therefore keeps the full mouth depth.
        let want = recommend(&geo);
        assert!(
            (want.size().z - 1.0).abs() < 1e-4,
            "recommendation shrank to the trim depth: {want:?}"
        );
    }

    /// The grid sampling is what makes the trim case detectable: a veil
    /// fitted to the strip is buried at the centre but hanging in air at
    /// the front and back of the mouth.
    #[test]
    fn a_veil_fitted_to_trim_is_caught_by_the_grid() {
        // Reaches the strips (±1.19) but keeps the full mouth depth, so
        // its jamb faces are covered only across the strip's 0.16 m.
        let geo = measure(&test_gate_with_trim([2.38, 2.88, 1.0], 1.6)).expect("no veil");
        let faults = fit_faults(&geo);
        let faces: Vec<Face> = faults.iter().map(|f| f.face).collect();
        assert!(
            faces.contains(&Face::Left) && faces.contains(&Face::Right),
            "grid sampling missed the daylight beside the trim: {faults:?}"
        );
    }

    /// A veil sized to swallow its own piers hides every edge and would
    /// otherwise pass — the burial bound is what rejects it.
    #[test]
    fn a_swollen_veil_is_rejected_even_though_its_edges_are_hidden() {
        // Jambs span x 1.2..1.8; a veil out to ±1.75 buries its edges
        // half a metre deep in them.
        let geo = measure(&test_gate([3.5, 2.88, 1.0], 1.6)).expect("no veil");
        for face in [Face::Left, Face::Right] {
            let p = face.center_of(&geo.veil);
            assert!(
                geo.solids.iter().any(|s| s.bounds.contains(p, COVER_SLACK)),
                "fixture should bury its {face:?} edge"
            );
        }
        let faults = fit_faults(&geo);
        let faces: Vec<Face> = faults.iter().map(|f| f.face).collect();
        assert!(
            faces.contains(&Face::Left) && faces.contains(&Face::Right),
            "swollen veil accepted: {faults:?}"
        );
    }

    /// Every shipped gateway's veil fills its own frame (#1006).
    ///
    /// The one that matters: the themed gates are 24 bespoke frames and
    /// every one of them used to carry the same hard-coded 2.6 × 3.2 ×
    /// 1.4 box, which fit none of them. Retuning a frame without moving
    /// its veil re-opens the gap this closed, and this says so by name.
    #[test]
    fn every_shipped_gateway_veil_fits_its_frame() {
        use crate::catalogue::{ENTRIES, StructureRole};

        let mut checked = 0;
        let mut bad = Vec::new();
        for entry in ENTRIES {
            if entry.role() != StructureRole::Gateway {
                continue;
            }
            checked += 1;
            let built = entry.build("did:plc:gatewayfit");
            let geo = measure(&built)
                .unwrap_or_else(|| panic!("{} carries no Gateway zone", entry.slug()));
            for fault in fit_faults(&geo) {
                bad.push(format!("{}: {}", entry.slug(), fault.detail));
            }
        }
        assert!(
            checked >= 20,
            "only {checked} gateways found — registry slipped?"
        );
        assert!(
            bad.is_empty(),
            "{} gateway veil(s) do not fit their frame:\n  {}\n\
             Run `cargo run --bin render -- --gateway-fit all` for the per-face report.",
            bad.len(),
            bad.join("\n  ")
        );
    }

    /// Bounds come from the real mesher, so a taper narrows the measured
    /// box rather than being ignored.
    #[test]
    fn measured_bounds_track_the_mesher() {
        let straight = prim(
            solid(cuboid_tapered([2.0, 2.0, 2.0], 0.0, mat())),
            [0.0, 1.0, 0.0],
            id_quat(),
        );
        let bounds = mesh_bounds(&straight.kind, &transform_of(&straight.transform))
            .expect("cuboid has bounds");
        assert!((bounds.size().x - 2.0).abs() < 1e-4, "{bounds:?}");
        assert!((bounds.center().y - 1.0).abs() < 1e-4, "{bounds:?}");
    }
}
